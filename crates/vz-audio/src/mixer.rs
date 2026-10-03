//! Mixes everything on the timeline into one stereo stream. Playback and export both use this,
//! so what you hear is what you export.

use crate::reader::AudioReader;
use crate::retime::Retimer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vz_core::{Clip, Content, MediaKind, Micros, Project, Timeline};

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
    /// Sound makers for clips whose speed is not plain (one per clip, they remember where they were).
    retimers: HashMap<vz_core::ClipId, Retimer>,
    /// Files whose sound could not be read; logged once, then treated as silent.
    broken: HashSet<PathBuf>,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            readers: HashMap::new(),
            retimers: HashMap::new(),
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
        if self.retimers.len() > 16 {
            let alive: std::collections::HashSet<_> = project
                .timelines
                .iter()
                .flat_map(|t| t.tracks.iter())
                .flat_map(|t| t.clips.iter().map(|c| c.id))
                .collect();
            self.retimers.retain(|id, _| alive.contains(id));
        }
        self.mix(project, project.timeline(), start_sample, frames, out, 0);
        // keep the sum from going past full scale (a proper limiter arrives with the Phase 6 mixer)
        for v in out.iter_mut() {
            *v = v.clamp(-1.0, 1.0);
        }
    }

    /// Add the sound of `timeline` for samples `start_sample .. start_sample + frames` into `out` (interleaved stereo).
    /// Compound clips play their own timeline (at normal speed; a retimed compound is silent for now).
    fn mix(
        &mut self,
        project: &Project,
        timeline: &Timeline,
        start_sample: i64,
        frames: usize,
        out: &mut [f32],
        depth: u8,
    ) {
        let end_sample = start_sample + frames as i64;

        for track in &timeline.tracks {
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
                if let Content::Compound(inner) = &clip.content {
                    let Some(inner) = project.timeline_by_id(*inner) else {
                        continue;
                    };
                    if clip.is_retimed() || depth >= 6 {
                        continue;
                    }
                    let n = (to - from) as usize;
                    let mut inside = vec![0.0f32; n * 2];
                    let inner_start =
                        micros_to_samples(clip.source_in, self.rate) + (from - clip_start);
                    self.mix(project, inner, inner_start, n, &mut inside, depth + 1);
                    apply_gain(clip, self.rate, from - clip_start, &mut inside);
                    let dst = &mut out[((from - start_sample) * 2) as usize..][..n * 2];
                    for (d, s) in dst.iter_mut().zip(&inside) {
                        *d += s;
                    }
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
                let n = (to - from) as usize;
                self.scratch.resize(n * 2, 0.0);
                let reader = self.readers.get_mut(&asset.path).expect("opened above");
                let result = if clip.is_retimed() {
                    self.retimers.entry(clip.id).or_default().render(
                        reader,
                        clip,
                        self.rate,
                        from - clip_start,
                        &mut self.scratch,
                    )
                } else {
                    let file_start =
                        micros_to_samples(clip.source_in, self.rate) + (from - clip_start);
                    reader.read(file_start, &mut self.scratch)
                };
                if let Err(e) = result {
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
