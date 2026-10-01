//! Mixes everything on the timeline into one stereo stream. Playback and export both use this,
//! so what you hear is what you export.

use crate::reader::AudioReader;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vz_core::{MediaKind, Micros, Project};

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
                let Some(asset) = project.asset(clip.media) else {
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
