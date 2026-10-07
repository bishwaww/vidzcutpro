//! Compressor, limiter and de-esser.

use super::biquad::{Biquad, Coeffs};
use vz_core::Compressor as CompressorSettings;

fn db_to_factor(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn factor_to_db(factor: f32) -> f32 {
    20.0 * factor.max(1e-6).log10()
}

/// Smoothing coefficient for a time constant in milliseconds.
fn smoothing(rate: u32, ms: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * rate as f32)).exp()
}

/// Makes loud parts quieter. Both channels get the same gain so the picture of the sound does not move.
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    makeup: f32,
    attack: f32,
    release: f32,
    /// Current gain reduction in decibels (zero or negative).
    reduction_db: f32,
    limiter: Option<Limiter>,
}

impl Compressor {
    pub fn new(rate: u32, s: &CompressorSettings) -> Self {
        Self {
            threshold_db: s.threshold_db.clamp(-60.0, 0.0),
            ratio: s.ratio.clamp(1.0, 20.0),
            makeup: db_to_factor(s.makeup_db.clamp(0.0, 24.0)),
            attack: smoothing(rate, s.attack_ms.clamp(0.1, 200.0)),
            release: smoothing(rate, s.release_ms.clamp(5.0, 2000.0)),
            reduction_db: 0.0,
            limiter: s.limit.then(|| Limiter::new(rate, -1.0)),
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.as_chunks_mut::<2>().0 {
            let level = factor_to_db(frame[0].abs().max(frame[1].abs()));
            let over = (level - self.threshold_db).max(0.0);
            let target = -over * (1.0 - 1.0 / self.ratio);
            // gain falls quickly (attack) and recovers slowly (release)
            let c = if target < self.reduction_db {
                self.attack
            } else {
                self.release
            };
            self.reduction_db = c * self.reduction_db + (1.0 - c) * target;
            let gain = db_to_factor(self.reduction_db) * self.makeup;
            frame[0] *= gain;
            frame[1] *= gain;
        }
        if let Some(l) = &mut self.limiter {
            l.process(samples);
        }
    }
}

/// Keeps the sound under a ceiling without hard clipping.
pub struct Limiter {
    ceiling: f32,
    release: f32,
    gain: f32,
}

impl Limiter {
    pub fn new(rate: u32, ceiling_db: f32) -> Self {
        Self {
            ceiling: db_to_factor(ceiling_db),
            release: smoothing(rate, 80.0),
            gain: 1.0,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.as_chunks_mut::<2>().0 {
            let peak = frame[0].abs().max(frame[1].abs());
            let needed = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            // the gain drops at once and comes back slowly
            if needed < self.gain {
                self.gain = needed;
            } else {
                self.gain = self.release * self.gain + (1.0 - self.release) * needed;
            }
            frame[0] *= self.gain;
            frame[1] *= self.gain;
            // whatever is left over (the gain is only exact on the peak sample) is rounded off
            frame[0] = frame[0].clamp(-self.ceiling, self.ceiling);
            frame[1] = frame[1].clamp(-self.ceiling, self.ceiling);
        }
    }
}

/// Softens harsh "s" sounds: while the band above 5.5 kHz is too strong the whole sound is turned down a little
/// (a gain on the whole sound avoids the phase mismatch a band subtraction would leave).
pub struct DeEsser {
    filter: Biquad,
    strength: f32,
    envelope: f32,
    attack: f32,
    release: f32,
}

impl DeEsser {
    pub fn new(rate: u32, strength: f32) -> Self {
        Self {
            filter: Biquad::new(Coeffs::high_pass(rate, 5500.0, 0.8)),
            strength: strength.clamp(0.0, 1.0),
            envelope: 0.0,
            attack: smoothing(rate, 1.0),
            release: smoothing(rate, 40.0),
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        // stronger setting = lower threshold, deeper cut
        let threshold = 0.12 * (1.0 - 0.7 * self.strength);
        let max_cut = 0.3 + 0.7 * (1.0 - self.strength); // lowest factor the sound may reach
        for frame in samples.as_chunks_mut::<2>().0 {
            let sl = self.filter.tick(0, frame[0]);
            let sr = self.filter.tick(1, frame[1]);
            let level = sl.abs().max(sr.abs());
            let c = if level > self.envelope {
                self.attack
            } else {
                self.release
            };
            self.envelope = c * self.envelope + (1.0 - c) * level;
            let g = if self.envelope > threshold {
                (threshold / self.envelope).max(max_cut)
            } else {
                1.0
            };
            frame[0] *= g;
            frame[1] *= g;
        }
    }
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

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn compressor_reduces_loud_sound_and_leaves_quiet_alone() {
        let settings = CompressorSettings {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            makeup_db: 0.0,
            limit: false,
        };
        let mut loud = tone(440.0, 0.8, 1.0);
        Compressor::new(RATE, &settings).process(&mut loud);
        let after = peak(&loud[loud.len() / 2..]);
        // 0.8 is -1.9 dB, 18 dB over the threshold; 4:1 leaves about 4.5 dB over: about -15.5 dB
        assert!(after < 0.25 && after > 0.1, "{after}");
        let mut quiet = tone(440.0, 0.05, 1.0);
        Compressor::new(RATE, &settings).process(&mut quiet);
        assert!((peak(&quiet) - 0.05).abs() < 0.002);
    }

    #[test]
    fn limiter_never_goes_over_the_ceiling() {
        let mut buf = tone(300.0, 3.0, 0.5);
        Limiter::new(RATE, -1.0).process(&mut buf);
        assert!(peak(&buf) <= 0.8913 + 1e-4, "{}", peak(&buf));
    }

    #[test]
    fn de_esser_lowers_high_hiss_but_not_the_voice_band() {
        let mut hiss = tone(8000.0, 0.5, 0.5);
        DeEsser::new(RATE, 1.0).process(&mut hiss);
        let after = peak(&hiss[hiss.len() / 2..]);
        assert!(after < 0.35, "{after}");
        let mut voice = tone(300.0, 0.5, 0.5);
        DeEsser::new(RATE, 1.0).process(&mut voice);
        assert!(peak(&voice[voice.len() / 2..]) > 0.45);
    }
}
