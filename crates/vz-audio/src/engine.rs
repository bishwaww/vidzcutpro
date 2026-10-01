//! Playing the timeline's sound through the speakers. The sound device's own clock is the master
//! clock of playback: the picture follows it.
//!
//! Three parts: a thread that mixes ahead of time and sends small chunks, the sound card's callback
//! that plays the chunks (it never waits and never blocks), and counters that tell the window how
//! much has been played.

use crate::mixer::{Mixer, micros_to_samples, samples_to_micros};
use crate::reader::AudioError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use vz_core::{Micros, Project};

/// Frames per chunk (about 21 ms at 48 kHz).
const CHUNK_FRAMES: usize = 1024;
/// How many chunks the mixing thread may be ahead (about 170 ms).
const CHUNKS_AHEAD: usize = 8;

struct Chunk {
    generation: u64,
    data: Vec<f32>,
}

enum Command {
    Project(Arc<Project>),
    Play { t: Micros, generation: u64 },
    Pause,
}

/// Numbers shared between the window, the mixing thread and the sound callback.
#[derive(Default)]
struct Shared {
    /// Changes at every play/seek, so old chunks are recognised and thrown away.
    generation: AtomicU64,
    playing: AtomicBool,
    /// Frames played since the current generation started.
    played_frames: AtomicU64,
    /// Timeline time (µs) at which the current generation started.
    base_micros: AtomicI64,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    commands: Sender<Command>,
    rate: u32,
}

impl AudioEngine {
    /// Open the default sound device and start the (silent) stream.
    pub fn start() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::Device("no output device".into()))?;
        let supported = device
            .default_output_config()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let rate = supported.sample_rate();
        let channels = usize::from(supported.channels());
        let config = supported.config();
        tracing::info!(
            "sound device: {rate} Hz, {channels} channels, {:?}",
            supported.sample_format()
        );

        let shared = Arc::new(Shared::default());
        let (chunk_tx, chunk_rx) = bounded::<Chunk>(CHUNKS_AHEAD);
        let (recycle_tx, recycle_rx) = bounded::<Vec<f32>>(CHUNKS_AHEAD + 2);
        let (commands, command_rx) = unbounded::<Command>();

        {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("vidzcut-audio-mix".into())
                .spawn(move || mixing_thread(rate, command_rx, chunk_tx, recycle_rx, shared))
                .map_err(|e| AudioError::Device(e.to_string()))?;
        }

        let stream = match supported.sample_format() {
            cpal::SampleFormat::I16 => build::<i16>(
                &device,
                &config,
                channels,
                shared.clone(),
                chunk_rx,
                recycle_tx,
            ),
            cpal::SampleFormat::U16 => build::<u16>(
                &device,
                &config,
                channels,
                shared.clone(),
                chunk_rx,
                recycle_tx,
            ),
            _ => build::<f32>(
                &device,
                &config,
                channels,
                shared.clone(),
                chunk_rx,
                recycle_tx,
            ),
        }?;
        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            shared,
            commands,
            rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Tell the engine about the project (call after every change; then call [`Self::play`] again to re-sync).
    pub fn set_project(&self, project: Arc<Project>) {
        let _ = self.commands.send(Command::Project(project));
    }

    /// Start (or restart) playing from timeline time `t`.
    pub fn play(&self, t: Micros) {
        let generation = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.shared.played_frames.store(0, Ordering::SeqCst);
        self.shared.base_micros.store(t, Ordering::SeqCst);
        let _ = self.commands.send(Command::Play { t, generation });
        self.shared.playing.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        let _ = self.commands.send(Command::Pause);
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Where playback is, by the sound card's clock.
    pub fn position(&self) -> Micros {
        let played = self.shared.played_frames.load(Ordering::Relaxed);
        self.shared.base_micros.load(Ordering::Relaxed)
            + samples_to_micros(played as i64, self.rate)
    }
}

fn mixing_thread(
    rate: u32,
    commands: Receiver<Command>,
    chunks: Sender<Chunk>,
    recycle: Receiver<Vec<f32>>,
    shared: Arc<Shared>,
) {
    let mut mixer = Mixer::new(rate);
    let mut project: Option<Arc<Project>> = None;
    let mut playing = false;
    let mut generation = 0;
    let mut position: i64 = 0;
    let mut pending: Option<Chunk> = None;
    loop {
        // commands first (the newest play/seek wins)
        loop {
            match commands.try_recv() {
                Ok(Command::Project(p)) => project = Some(p),
                Ok(Command::Play { t, generation: g }) => {
                    playing = true;
                    generation = g;
                    position = micros_to_samples(t, rate);
                    pending = None;
                }
                Ok(Command::Pause) => {
                    playing = false;
                    pending = None;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if !playing || project.is_none() || shared.generation.load(Ordering::Relaxed) != generation
        {
            if let Ok(cmd) = commands.recv_timeout(std::time::Duration::from_millis(5)) {
                // put it back in order by handling it on the next loop turn
                match cmd {
                    Command::Project(p) => project = Some(p),
                    Command::Play { t, generation: g } => {
                        playing = true;
                        generation = g;
                        position = micros_to_samples(t, rate);
                        pending = None;
                    }
                    Command::Pause => {
                        playing = false;
                        pending = None;
                    }
                }
            }
            continue;
        }
        let chunk = match pending.take() {
            Some(c) => c,
            None => {
                let mut data = recycle.try_recv().unwrap_or_default();
                if let Some(p) = &project {
                    mixer.render(p, position, CHUNK_FRAMES, &mut data);
                }
                position += CHUNK_FRAMES as i64;
                Chunk { generation, data }
            }
        };
        match chunks.try_send(chunk) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(c)) => {
                pending = Some(c);
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => return,
        }
    }
}

/// The part that runs inside the sound card's callback: copy chunks to the output, never wait.
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    shared: Arc<Shared>,
    chunks: Receiver<Chunk>,
    recycle: Sender<Vec<f32>>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let mut current: Vec<f32> = Vec::new();
    let mut at = 0usize; // in floats
    let mut started_generation = u64::MAX;
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                let silence = T::from_sample(0.0f32);
                if !shared.playing.load(Ordering::Relaxed) {
                    out.fill(silence);
                    return;
                }
                let generation = shared.generation.load(Ordering::SeqCst);
                let mut played = 0u64;
                for frame in out.chunks_mut(channels.max(1)) {
                    // get the next stereo frame (or silence if the mixing thread is behind)
                    let (l, r) = loop {
                        if at + 1 < current.len() {
                            let pair = (current[at], current[at + 1]);
                            at += 2;
                            break pair;
                        }
                        // this chunk is used up: recycle it and fetch the next one of this generation
                        let spent = std::mem::take(&mut current);
                        if spent.capacity() > 0 {
                            let _ = recycle.try_send(spent);
                        }
                        at = 0;
                        match chunks.try_recv() {
                            Ok(c) if c.generation == generation => {
                                started_generation = generation;
                                current = c.data;
                            }
                            Ok(c) => {
                                let _ = recycle.try_send(c.data); // an old chunk from before a seek
                            }
                            Err(_) => break (0.0, 0.0),
                        }
                    };
                    // the clock starts counting when the first sound of this play/seek arrives
                    if started_generation == generation {
                        played += 1;
                    }
                    match frame.len() {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            frame[2..].iter_mut().for_each(|s| *s = silence);
                        }
                    }
                }
                shared.played_frames.fetch_add(played, Ordering::Relaxed);
            },
            |e| tracing::error!("sound device error: {e}"),
            None,
        )
        .map_err(|e| AudioError::Device(e.to_string()))
}
