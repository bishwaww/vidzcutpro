//! Recording a voice-over from a microphone into a WAV file.

use crate::reader::AudioError;
use crate::wav::WavWriter;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use crossbeam_channel::{Sender, unbounded};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use vz_core::Micros;

fn name_of(device: &cpal::Device) -> String {
    device.description().map_or_else(
        |_| "Unknown microphone".to_string(),
        |d| d.name().to_string(),
    )
}

/// Names of the microphones, the default one first.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let default = host.default_input_device().map(|d| name_of(&d));
    let mut names: Vec<String> = host
        .input_devices()
        .map(|devices| devices.map(|d| name_of(&d)).collect())
        .unwrap_or_default();
    if let Some(d) = default {
        names.retain(|n| *n != d);
        names.insert(0, d);
    }
    names
}

#[derive(Default)]
struct Shared {
    /// Loudest sample since the level was last read, as the bits of an `f32`.
    peak: AtomicU32,
    frames: AtomicU64,
}

pub struct Recorder {
    stream: Option<cpal::Stream>,
    shared: Arc<Shared>,
    sender: Option<Sender<Vec<f32>>>,
    writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    path: PathBuf,
    rate: u32,
}

impl Recorder {
    /// Start recording from the named microphone (or the default one) into `path`.
    pub fn start(device_name: Option<&str>, path: &Path) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(wanted) => host
                .input_devices()
                .ok()
                .and_then(|mut all| all.find(|d| name_of(d) == wanted))
                .or_else(|| host.default_input_device()),
            None => host.default_input_device(),
        }
        .ok_or_else(|| AudioError::Device("no microphone found".into()))?;
        let supported = device
            .default_input_config()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let rate = supported.sample_rate();
        let channels = usize::from(supported.channels()).max(1);
        let config = supported.config();

        let mut wav = WavWriter::create(path, rate, 1)
            .map_err(|e| AudioError::Device(format!("could not create {}: {e}", path.display())))?;
        let (sender, receiver) = unbounded::<Vec<f32>>();
        let writer = std::thread::Builder::new()
            .name("vidzcut-recorder".into())
            .spawn(move || {
                while let Ok(chunk) = receiver.recv() {
                    wav.write(&chunk)?;
                }
                wav.finish()
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;

        let shared = Arc::new(Shared::default());
        let stream = match supported.sample_format() {
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &config, channels, shared.clone(), sender.clone())
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &config, channels, shared.clone(), sender.clone())
            }
            cpal::SampleFormat::I32 => {
                build::<i32>(&device, &config, channels, shared.clone(), sender.clone())
            }
            _ => build::<f32>(&device, &config, channels, shared.clone(), sender.clone()),
        }?;
        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(Self {
            stream: Some(stream),
            shared,
            sender: Some(sender),
            writer: Some(writer),
            path: path.to_path_buf(),
            rate,
        })
    }

    /// How loud the microphone was since the last call (0.0 to 1.0), for a level meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.peak.swap(0, Ordering::Relaxed)).min(1.0)
    }

    /// How long the recording is so far.
    pub fn length(&self) -> Micros {
        (self.shared.frames.load(Ordering::Relaxed) as i128 * 1_000_000 / i128::from(self.rate))
            as Micros
    }

    /// Stop and close the file. Returns where it is and how long it is.
    pub fn stop(mut self) -> Result<(PathBuf, Micros), AudioError> {
        let length = self.length();
        drop(self.stream.take());
        drop(self.sender.take());
        if let Some(w) = self.writer.take() {
            w.join()
                .map_err(|_| AudioError::Device("the recording thread stopped".into()))?
                .map_err(|e| AudioError::Device(format!("could not write the recording: {e}")))?;
        }
        Ok((self.path.clone(), length))
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    shared: Arc<Shared>,
    sender: Sender<Vec<f32>>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let mut mono = Vec::with_capacity(data.len() / channels);
                let mut peak = 0.0f32;
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
                    let v = sum / frame.len() as f32;
                    peak = peak.max(v.abs());
                    mono.push(v);
                }
                shared
                    .frames
                    .fetch_add(mono.len() as u64, Ordering::Relaxed);
                let old = f32::from_bits(shared.peak.load(Ordering::Relaxed));
                if peak > old {
                    shared.peak.store(peak.to_bits(), Ordering::Relaxed);
                }
                let _ = sender.send(mono);
            },
            |e| tracing::error!("microphone error: {e}"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
