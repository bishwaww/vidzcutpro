//! Mixes everything on the timeline into one stereo stream. Playback and export both use this,
//! so what you hear is what you export.

use crate::reader::AudioReader;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vz_core::{Clip, MediaKind, Micros, Project};

/// Microseconds to a sample number at `rate` (rounded to the nearest sample).
pub fn micros_to_samples(t: Micros, rate: u32) -> i64 {
    ((i128::from(t) * i128::from(rate) + 500_000).div_euclid(1_000_000)) as i64
}

/// A sample number back to microseconds.
pub fn samples_to_micros(samples: i64, rate: u32) -> Micros {
    (i128::from(samples) * 1_000_000 / i128::from(rate.max(1))) as Micros
}

pub struct Mixer {
    rate: u32,
    readers: HashMap<PathBuf, AudioReader>,
    /// Files whose sound could not be read; logged once, then treated as silent.
    broken: HashSet<PathBuf>,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            readers: HashMap::new(),
            broken: HashSet::new(),
            scratch: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Sound for timeline samples `start_sample .. start_sample + frames`, as interleaved stereo in `out`
    /// (`out` is resized to `frames * 2`).
    pub fn render(
        &mut self,
        project: &Project,
        start_sample: i64,
        frames: usize,
        out: &mut Vec<f32>,
    ) {
        out.clear();
        out.resize(frames * 2, 0.0);
        let end_sample = start_sample + frames as i64;

        for track in &project.timeline().tracks {
            if track.muted {
                continue;
            }
            for clip in &track.clips {
                let clip_start = micros_to_samples(clip.start, self.rate);
                let clip_end = micros_to_samples(clip.end(), self.rate);
                let from = start_sample.max(clip_start);
                let to = end_sample.min(clip_end);
                if from >= to {
                    continue;
                }
                let Some(asset) = clip.media().and_then(|m| project.asset(m)) else {
                    continue;
                };
                if !asset.has_audio
                    || asset.kind == MediaKind::Image
                    || self.broken.contains(&asset.path)
                {
                    continue;
                }
                if !self.readers.contains_key(&asset.path) {
                    match AudioReader::open(&asset.path, self.rate) {
                        Ok(r) => {
                            self.readers.insert(asset.path.clone(), r);
                        }
                        Err(e) => {
                            tracing::warn!("no sound from {}: {e}", asset.path.display());
                            self.broken.insert(asset.path.clone());
                            continue;
                        }
                    }
                }
                let file_start = micros_to_samples(clip.source_in, self.rate) + (from - clip_start);
                let n = (to - from) as usize;
                self.scratch.resize(n * 2, 0.0);
                let reader = self.readers.get_mut(&asset.path).expect("opened above");
                if let Err(e) = reader.read(file_start, &mut self.scratch) {
                    tracing::warn!("sound read error in {}: {e}", asset.path.display());
                    self.readers.remove(&asset.path);
                    self.broken.insert(asset.path.clone());
                    continue;
                }
                apply_gain(clip, self.rate, from - clip_start, &mut self.scratch);
                let dst = &mut out[((from - start_sample) * 2) as usize..][..n * 2];
                for (d, s) in dst.iter_mut().zip(&self.scratch) {
                    *d += s;
                }
            }
        }
        // keep the sum from going past full scale (a proper limiter arrives with the Phase 6 mixer)
        for v in out.iter_mut() {
            *v = v.clamp(-1.0, 1.0);
        }
    }
}

/// Apply the clip's volume and fades to `samples` (interleaved stereo) that start `first` samples into the clip.
fn apply_gain(clip: &Clip, rate: u32, first: i64, samples: &mut [f32]) {
    let length = micros_to_samples(clip.duration, rate);
    let fade_in = micros_to_samples(clip.fade_in.clamp(0, clip.duration), rate);
    let fade_out = micros_to_samples(clip.fade_out.clamp(0, clip.duration), rate);
    let animated = clip.volume.is_animated();
    let mut volume = clip
        .volume
        .value_at(samples_to_micros(first, rate))
        .max(0.0);
    for (i, frame) in samples.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let at = first + i as i64;
        if animated && i % 256 == 0 {
            volume = clip.volume.value_at(samples_to_micros(at, rate)).max(0.0);
        }
        let mut gain = volume;
        if fade_in > 0 && at < fade_in {
            gain *= at as f32 / fade_in as f32;
        }
        let left = length - at;
        if fade_out > 0 && left < fade_out {
            gain *= left.max(0) as f32 / fade_out as f32;
        }
        if gain != 1.0 {
            frame[0] *= gain;
            frame[1] *= gain;
        }
    }
}
