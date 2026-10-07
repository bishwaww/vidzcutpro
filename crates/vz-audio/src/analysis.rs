//! Measuring a piece of a media file: loudness, background noise and the beat. These take a while
//! on long files, so the window runs them in a background thread.

use crate::fx::beats::{BeatDetector, Beats};
use crate::fx::denoise::NoiseMeter;
use crate::fx::loudness::LoudnessMeter;
use crate::reader::{AudioError, AudioReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use vz_core::Micros;

/// Files are read at this rate for measuring (noise profiles are made for it, see `fx::denoise`).
pub const ANALYSIS_RATE: u32 = 48_000;

/// Which measurements to make.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wanted {
    pub loudness: bool,
    pub noise: bool,
    pub beats: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// Integrated loudness, or `None` when it was not asked for.
    pub lufs: Option<f32>,
    /// Highest sample (a factor).
    pub peak: f32,
    pub noise: Option<Vec<f32>>,
    /// Beat times are given as times inside the media file.
    pub beats: Option<Beats>,
}

/// Measure `length` of the file starting at `from`. `Ok(None)` if `cancel` was set.
pub fn analyse(
    path: &Path,
    from: Micros,
    length: Micros,
    wanted: Wanted,
    cancel: &AtomicBool,
) -> Result<Option<Analysis>, AudioError> {
    let mut reader = AudioReader::open(path, ANALYSIS_RATE)?;
    let first = crate::mixer::micros_to_samples(from.max(0), ANALYSIS_RATE);
    let total = crate::mixer::micros_to_samples(length.max(0), ANALYSIS_RATE);
    let mut loudness = wanted.loudness.then(|| LoudnessMeter::new(ANALYSIS_RATE));
    let mut noise = wanted.noise.then(NoiseMeter::new);
    let mut beat = wanted.beats.then(|| BeatDetector::new(ANALYSIS_RATE));
    let mut peak = 0.0f32;

    let chunk = ANALYSIS_RATE as usize;
    let mut buffer = vec![0.0f32; chunk * 2];
    let mut mono = vec![0.0f32; chunk];
    let mut at = 0i64;
    while at < total {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let frames = ((total - at) as usize).min(chunk);
        let out = &mut buffer[..frames * 2];
        reader.read(first + at, out)?;
        peak = out.iter().fold(peak, |m, v| m.max(v.abs()));
        if let Some(l) = &mut loudness {
            l.feed(out);
        }
        if noise.is_some() || beat.is_some() {
            for (m, f) in mono.iter_mut().zip(out.as_chunks::<2>().0) {
                *m = (f[0] + f[1]) * 0.5;
            }
            if let Some(n) = &mut noise {
                n.feed(&mono[..frames]);
            }
            if let Some(b) = &mut beat {
                b.feed(&mono[..frames]);
            }
        }
        at += frames as i64;
    }
    Ok(Some(Analysis {
        lufs: loudness.map(|l| l.lufs()),
        peak,
        noise: noise.and_then(NoiseMeter::finish),
        beats: beat.map(|b| {
            let mut beats = b.finish();
            for t in &mut beats.times {
                *t += from;
            }
            beats
        }),
    }))
}
