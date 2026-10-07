//! Loudness in LUFS as defined by ITU-R BS.1770 / EBU R128 (K-weighting, 400 ms blocks, two gates).

use super::biquad::Coeffs;
use std::f64::consts::PI;

/// Silence is reported as this.
pub const SILENCE_LUFS: f32 = -120.0;

fn k_weighting(rate: u32) -> [Coeffs; 2] {
    let fs = f64::from(rate);
    // stage 1: a shelf that lifts the highs by about 4 dB
    let (f0, g, q) = (
        1_681.974_450_955_533,
        3.999_843_853_973_347,
        0.707_175_236_955_419_6,
    );
    let k = (PI * f0 / fs).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Coeffs {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };
    // stage 2: a high-pass at 38 Hz
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let high = Coeffs {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };
    [shelf, high]
}

/// One channel of the weighting filters (direct form I, f64).
#[derive(Clone, Copy, Default)]
struct Stage {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Stage {
    fn tick(&mut self, c: &Coeffs, x: f64) -> f64 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Measures the loudness of a stereo stream that arrives in pieces.
pub struct LoudnessMeter {
    coeffs: [Coeffs; 2],
    stages: [[Stage; 2]; 2],
    /// Sum of squares of the weighted samples (left + right) in the current 100 ms step.
    step_sum: f64,
    step_len: usize,
    step_target: usize,
    /// Energy of the last four 100 ms steps (a block is four steps).
    recent: [f64; 4],
    steps_done: usize,
    block_powers: Vec<f64>,
    peak: f32,
}

impl LoudnessMeter {
    pub fn new(rate: u32) -> Self {
        Self {
            coeffs: k_weighting(rate),
            stages: [[Stage::default(); 2]; 2],
            step_sum: 0.0,
            step_len: 0,
            step_target: (rate as usize / 10).max(1),
            recent: [0.0; 4],
            steps_done: 0,
            block_powers: Vec::new(),
            peak: 0.0,
        }
    }

    /// Feed interleaved stereo.
    #[allow(clippy::needless_range_loop)]
    pub fn feed(&mut self, samples: &[f32]) {
        for frame in samples.as_chunks::<2>().0 {
            let mut sum = 0.0;
            for ch in 0..2 {
                self.peak = self.peak.max(frame[ch].abs());
                let mut v = f64::from(frame[ch]);
                for s in 0..2 {
                    v = self.stages[ch][s].tick(&self.coeffs[s], v);
                }
                sum += v * v;
            }
            self.step_sum += sum;
            self.step_len += 1;
            if self.step_len == self.step_target {
                self.recent.rotate_left(1);
                self.recent[3] = self.step_sum / self.step_target as f64;
                self.step_sum = 0.0;
                self.step_len = 0;
                self.steps_done += 1;
                if self.steps_done >= 4 {
                    self.block_powers
                        .push(self.recent.iter().sum::<f64>() / 4.0);
                }
            }
        }
    }

    /// Highest sample seen (a factor, 1.0 = full scale).
    pub fn peak(&self) -> f32 {
        self.peak
    }

    /// Integrated loudness of everything fed so far.
    pub fn lufs(&self) -> f32 {
        let loud = |power: f64| -0.691 + 10.0 * power.log10();
        let above_absolute: Vec<f64> = self
            .block_powers
            .iter()
            .copied()
            .filter(|&p| p > 0.0 && loud(p) > -70.0)
            .collect();
        if above_absolute.is_empty() {
            return SILENCE_LUFS;
        }
        let mean = above_absolute.iter().sum::<f64>() / above_absolute.len() as f64;
        let relative_gate = loud(mean) - 10.0;
        let kept: Vec<f64> = above_absolute
            .into_iter()
            .filter(|&p| loud(p) > relative_gate)
            .collect();
        if kept.is_empty() {
            return SILENCE_LUFS;
        }
        let mean = kept.iter().sum::<f64>() / kept.len() as f64;
        loud(mean) as f32
    }
}

/// Loudness of a whole buffer of interleaved stereo.
pub fn integrated_lufs(stereo: &[f32], rate: u32) -> f32 {
    let mut m = LoudnessMeter::new(rate);
    m.feed(stereo);
    m.lufs()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tone(freq: f32, amp: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .flat_map(|i| {
                let v = amp * (i as f32 * 2.0 * std::f32::consts::PI * freq / RATE as f32).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn the_ebu_reference_tone_reads_minus_23() {
        // EBU Tech 3341: a 1 kHz sine at -23 dBFS in both channels reads -23 LUFS
        let lufs = integrated_lufs(&tone(1000.0, 10f32.powf(-23.0 / 20.0), 5.0), RATE);
        assert!((lufs + 23.0).abs() < 0.3, "{lufs}");
    }

    #[test]
    fn six_decibels_louder_reads_six_louder() {
        let a = integrated_lufs(&tone(500.0, 0.1, 3.0), RATE);
        let b = integrated_lufs(&tone(500.0, 0.2, 3.0), RATE);
        assert!((b - a - 6.02).abs() < 0.1, "{a} {b}");
    }

    #[test]
    fn silence_and_very_quiet_sound_read_as_silence() {
        assert_eq!(integrated_lufs(&vec![0.0; 96_000], RATE), SILENCE_LUFS);
        assert_eq!(
            integrated_lufs(&tone(1000.0, 0.00001, 2.0), RATE),
            SILENCE_LUFS
        );
        assert_eq!(integrated_lufs(&[], RATE), SILENCE_LUFS);
    }

    #[test]
    fn a_quiet_gap_does_not_drag_the_result_down() {
        let mut buf = tone(1000.0, 0.1, 3.0);
        let loud_only = integrated_lufs(&buf, RATE);
        buf.extend(tone(1000.0, 0.0005, 3.0));
        let with_gap = integrated_lufs(&buf, RATE);
        assert!((loud_only - with_gap).abs() < 0.5, "{loud_only} {with_gap}");
    }
}
