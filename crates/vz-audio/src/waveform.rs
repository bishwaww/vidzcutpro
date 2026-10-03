//! A rough picture of how loud a file is over time, for drawing the waveform on audio clips.

use crate::reader::{AudioError, AudioReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use vz_core::Micros;

/// The file is read at this sample rate; only the loudness shape matters, not the quality.
const RATE: u32 = 8_000;
/// How many loudness values are kept for each second of sound.
pub const PEAKS_PER_SECOND: i64 = 50;
const SAMPLES_PER_PEAK: usize = (RATE as i64 / PEAKS_PER_SECOND) as usize;

#[derive(Clone, Debug, PartialEq)]
pub struct Waveform {
    /// Loudest sample (0.0 to 1.0) of each 20 ms piece of the file.
    pub peaks: Vec<f32>,
    /// The largest value in `peaks`.
    pub loudest: f32,
}

impl Waveform {
    pub fn new(peaks: Vec<f32>) -> Self {
        let loudest = peaks.iter().copied().fold(0.0, f32::max);
        Self { peaks, loudest }
    }

    /// The loudest peak between two times in the file.
    pub fn peak_between(&self, from: Micros, to: Micros) -> f32 {
        let at =
            |t: Micros| (i128::from(t.max(0)) * i128::from(PEAKS_PER_SECOND) / 1_000_000) as usize;
        let first = at(from).min(self.peaks.len().saturating_sub(1));
        let last = at(to).clamp(first, self.peaks.len().saturating_sub(1));
        self.peaks
            .get(first..=last)
            .map_or(0.0, |s| s.iter().copied().fold(0.0, f32::max))
    }
}

/// Read a file's sound and measure it. `duration` is the file's length; `cancel` stops it early (returns `Ok(None)`).
pub fn compute(
    path: &Path,
    duration: Micros,
    cancel: Option<&AtomicBool>,
) -> Result<Option<Waveform>, AudioError> {
    let mut reader = AudioReader::open(path, RATE)?;
    let total = (i128::from(duration) * i128::from(RATE) / 1_000_000) as i64;
    let mut peaks = Vec::with_capacity((total as usize) / SAMPLES_PER_PEAK + 1);
    let chunk = RATE as usize; // one second at a time
    let mut buffer = vec![0.0f32; chunk * 2];
    let mut at = 0i64;
    while at < total {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Ok(None);
        }
        let frames = ((total - at) as usize).min(chunk);
        let out = &mut buffer[..frames * 2];
        reader.read(at, out)?;
        for piece in out.chunks(SAMPLES_PER_PEAK * 2) {
            peaks.push(piece.iter().fold(0.0f32, |m, v| m.max(v.abs())).min(1.0));
        }
        at += frames as i64;
    }
    Ok(Some(Waveform::new(peaks)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_media::testutil;

    #[test]
    fn a_tone_has_steady_peaks_and_the_right_length() {
        let path = testutil::standard(); // a 4 s tone
        let w = compute(&path, 4_000_000, None).unwrap().unwrap();
        assert!(
            (w.peaks.len() as i64 - 4 * PEAKS_PER_SECOND).abs() <= 2,
            "{}",
            w.peaks.len()
        );
        let middle = &w.peaks[40..160];
        let (lo, hi) = middle
            .iter()
            .fold((1.0f32, 0.0f32), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        assert!(lo > 0.05 && hi < 1.0 && hi - lo < 0.2, "{lo} {hi}");
    }

    #[test]
    fn peak_between_finds_the_loudest_piece() {
        let w = Waveform::new(vec![0.1, 0.2, 0.9, 0.3, 0.1]);
        assert_eq!(w.peak_between(0, 20_000), 0.2);
        assert_eq!(w.peak_between(0, 1_000_000), 0.9);
        assert_eq!(w.peak_between(60_000, 70_000), 0.3);
        // outside the file is quiet, not a crash
        assert_eq!(Waveform::new(vec![]).peak_between(0, 5), 0.0);
    }

    #[test]
    fn cancelling_stops_the_work() {
        let path = testutil::standard();
        let cancel = AtomicBool::new(true);
        assert!(compute(&path, 4_000_000, Some(&cancel)).unwrap().is_none());
    }
}
