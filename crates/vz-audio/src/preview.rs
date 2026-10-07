//! Playing one sound file through the speakers for a quick listen (music and sound-effect lists).

use crate::reader::{AudioError, AudioReader};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{Receiver, bounded};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Plays a file until it ends or [`Previewer::stop`] is called. A new `play` stops the old one.
#[derive(Default)]
pub struct Previewer {
    current: Option<(PathBuf, Arc<AtomicBool>)>,
}

impl Previewer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The file being played now, if any.
    pub fn playing(&self) -> Option<&Path> {
        self.current
            .as_ref()
            .filter(|(_, done)| !done.load(Ordering::Relaxed))
            .map(|(p, _)| p.as_path())
    }

    pub fn stop(&mut self) {
        if let Some((_, done)) = self.current.take() {
            done.store(true, Ordering::Relaxed);
        }
    }

    /// Play up to `length` microseconds of the file (at most two minutes).
    pub fn play(&mut self, path: &Path, length: vz_core::Micros) {
        self.stop();
        let done = Arc::new(AtomicBool::new(false));
        self.current = Some((path.to_path_buf(), done.clone()));
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            if let Err(e) = run(&path, length, &done) {
                tracing::warn!("could not play {}: {e}", path.display());
            }
            done.store(true, Ordering::Relaxed);
        });
    }
}

fn run(path: &Path, length: vz_core::Micros, stop: &Arc<AtomicBool>) -> Result<(), AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioError::Device("no output device".into()))?;
    let supported = device
        .default_output_config()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    let rate = supported.sample_rate();
    let channels = usize::from(supported.channels()).max(1);
    let config = supported.config();
    let mut reader = AudioReader::open(path, rate)?;

    let (tx, rx) = bounded::<Vec<f32>>(6);
    let finished = Arc::new(AtomicBool::new(false));
    let stream = match supported.sample_format() {
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, rx, finished.clone()),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, rx, finished.clone()),
        _ => build::<f32>(&device, &config, channels, rx, finished.clone()),
    }?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;

    // two minutes are enough for a preview
    let chunk = 4096usize;
    let total = crate::mixer::micros_to_samples(length.clamp(0, 120_000_000), rate);
    let mut at = 0i64;
    let mut buf = vec![0.0f32; chunk * 2];
    while at < total && !stop.load(Ordering::Relaxed) {
        reader.read(at, &mut buf)?;
        let mut data = buf.clone();
        loop {
            match tx.try_send(data) {
                Ok(()) => break,
                Err(crossbeam_channel::TrySendError::Full(d)) => {
                    data = d;
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => return Ok(()),
            }
        }
        at += chunk as i64;
    }
    drop(tx);
    // let the last chunks play out
    while !finished.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    chunks: Receiver<Vec<f32>>,
    finished: Arc<AtomicBool>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let mut current: Vec<f32> = Vec::new();
    let mut at = 0usize;
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                for frame in out.chunks_mut(channels) {
                    if at + 1 >= current.len() {
                        match chunks.try_recv() {
                            Ok(c) => {
                                current = c;
                                at = 0;
                            }
                            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                finished.store(true, Ordering::Relaxed);
                                current.clear();
                            }
                            Err(_) => current.clear(),
                        }
                    }
                    let (l, r) = if at + 1 < current.len() {
                        let p = (current[at], current[at + 1]);
                        at += 2;
                        p
                    } else {
                        (0.0, 0.0)
                    };
                    match frame.len() {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            for s in &mut frame[2..] {
                                *s = T::from_sample(0.0f32);
                            }
                        }
                    }
                }
            },
            |e| tracing::error!("sound device error: {e}"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
