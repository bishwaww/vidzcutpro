//! Voice effects made with ordinary sound processing: pitch change, robot, echo, reverb,
//! telephone, megaphone, radio, underwater and vibrato.

use super::biquad::{Biquad, Coeffs};
use std::f32::consts::PI;
use vz_core::{Voice, VoiceEffect};

/// A ring of samples that can be read at fractional positions.
struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(2)],
            at: 0,
        }
    }

    fn push(&mut self, x: f32) {
        self.buf[self.at] = x;
        self.at = (self.at + 1) % self.buf.len();
    }

    /// The sample pushed `back` samples ago (1.0 = the last one), interpolated.
    fn read(&self, back: f32) -> f32 {
        let len = self.buf.len();
        let back = back.clamp(1.0, (len - 2) as f32);
        let whole = back.floor();
        let frac = back - whole;
        let i0 = (self.at + len - whole as usize) % len;
        let i1 = (i0 + len - 1) % len;
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }
}

/// Changes pitch without changing length: two overlapping read points slide through a short delay.
struct PitchShifter {
    delay: [Delay; 2],
    window: f32,
    phase: f32,
    ratio: f32,
}

impl PitchShifter {
    fn new(rate: u32, ratio: f32) -> Self {
        let window = rate as f32 * 0.045;
        Self {
            delay: [
                Delay::new(window as usize * 2 + 8),
                Delay::new(window as usize * 2 + 8),
            ],
            window,
            phase: 0.0,
            ratio,
        }
    }

    fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        for (d, v) in self.delay.iter_mut().zip(x) {
            d.push(v);
        }
        let step = (1.0 - self.ratio) / self.window;
        self.phase = (self.phase + step).rem_euclid(1.0);
        let mut out = [0.0f32; 2];
        for tap in 0..2 {
            let p = (self.phase + tap as f32 * 0.5).rem_euclid(1.0);
            let weight = (PI * p).sin().powi(2);
            let back = 1.0 + p * self.window;
            for (o, d) in out.iter_mut().zip(&self.delay) {
                *o += d.read(back) * weight;
            }
        }
        out
    }
}

/// Comb filter with damping (one of the eight of a Freeverb).
struct Comb {
    buf: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len],
            at: 0,
            store: 0.0,
        }
    }

    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.at];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.at] = x + self.store * feedback;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

struct AllPass {
    buf: Vec<f32>,
    at: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len],
            at: 0,
        }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let held = self.buf[self.at];
        let out = held - x;
        self.buf[self.at] = x + held * 0.5;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

/// A Freeverb-style room.
struct Reverb {
    combs: [Vec<Comb>; 2],
    all: [Vec<AllPass>; 2],
    feedback: f32,
    damp: f32,
}

impl Reverb {
    fn new(rate: u32, room: f32, damp: f32) -> Self {
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const ALL: [usize; 4] = [556, 441, 341, 225];
        let scale = rate as f32 / 44_100.0;
        let side = |spread: usize| {
            (
                COMBS
                    .iter()
                    .map(|&n| Comb::new(((n + spread) as f32 * scale) as usize))
                    .collect::<Vec<_>>(),
                ALL.iter()
                    .map(|&n| AllPass::new(((n + spread) as f32 * scale) as usize))
                    .collect::<Vec<_>>(),
            )
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(23);
        Self {
            combs: [cl, cr],
            all: [al, ar],
            feedback: room,
            damp,
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn tick(&mut self, x: [f32; 2]) -> [f32; 2] {
        let input = (x[0] + x[1]) * 0.015;
        let mut out = [0.0f32; 2];
        for ch in 0..2 {
            let mut sum = 0.0;
            for c in &mut self.combs[ch] {
                sum += c.tick(input, self.feedback, self.damp);
            }
            for a in &mut self.all[ch] {
                sum = a.tick(sum);
            }
            out[ch] = sum;
        }
        out
    }
}

/// A tiny noise maker (the same every time, so playback and export match).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

enum Kind {
    Pitch(PitchShifter),
    Robot {
        phase: f32,
        step: f32,
        comb: Delay,
        delay: f32,
    },
    Echo {
        delay: Delay,
        samples: f32,
    },
    Reverb {
        reverb: Reverb,
        wet: f32,
    },
    /// Filters then a bit of distortion.
    Filtered {
        filters: Vec<Biquad>,
        drive: f32,
        hiss: f32,
        noise: Noise,
    },
    Underwater {
        low: Biquad,
        delay: Delay,
        phase: f32,
        step: f32,
        centre: f32,
        depth: f32,
    },
    Vibrato {
        delay: Delay,
        phase: f32,
        step: f32,
        centre: f32,
        depth: f32,
    },
}

pub struct VoiceFx {
    kind: Kind,
    /// How much of the changed sound is mixed in.
    amount: f32,
    /// True when the effect should replace the sound completely at the top setting.
    full_replace: bool,
}

impl VoiceFx {
    pub fn new(rate: u32, voice: &Voice) -> Self {
        let amount = voice.amount.clamp(0.0, 1.0);
        let r = rate as f32;
        let filtered = |filters: Vec<Coeffs>, drive: f32, hiss: f32| Kind::Filtered {
            filters: filters.into_iter().map(Biquad::new).collect(),
            drive,
            hiss,
            noise: Noise(12345),
        };
        let (kind, full_replace) = match voice.effect {
            // pitch changes use the amount as how far to go: up to one octave
            VoiceEffect::Chipmunk => (
                Kind::Pitch(PitchShifter::new(rate, 1.0 + 0.4 + 0.6 * amount)),
                true,
            ),
            VoiceEffect::Deep => (
                Kind::Pitch(PitchShifter::new(rate, 1.0 - 0.15 - 0.35 * amount)),
                true,
            ),
            VoiceEffect::Robot => (
                Kind::Robot {
                    phase: 0.0,
                    step: 2.0 * PI * 55.0 / r,
                    comb: Delay::new((r * 0.02) as usize + 4),
                    delay: r * 0.006,
                },
                false,
            ),
            VoiceEffect::Echo => (
                Kind::Echo {
                    delay: Delay::new((r * 0.9) as usize),
                    samples: r * 0.32,
                },
                false,
            ),
            VoiceEffect::Room => (
                Kind::Reverb {
                    reverb: Reverb::new(rate, 0.70, 0.45),
                    wet: 3.0,
                },
                false,
            ),
            VoiceEffect::Hall => (
                Kind::Reverb {
                    reverb: Reverb::new(rate, 0.90, 0.25),
                    wet: 3.0,
                },
                false,
            ),
            VoiceEffect::Telephone => (
                filtered(
                    vec![
                        Coeffs::high_pass(rate, 330.0, 0.8),
                        Coeffs::high_pass(rate, 330.0, 0.8),
                        Coeffs::low_pass(rate, 3300.0, 0.8),
                        Coeffs::low_pass(rate, 3300.0, 0.8),
                    ],
                    2.0,
                    0.0,
                ),
                false,
            ),
            VoiceEffect::Megaphone => (
                filtered(
                    vec![
                        Coeffs::high_pass(rate, 600.0, 0.9),
                        Coeffs::low_pass(rate, 4200.0, 0.9),
                        Coeffs::peaking(rate, 1800.0, 6.0, 1.2),
                    ],
                    5.0,
                    0.0,
                ),
                false,
            ),
            VoiceEffect::Radio => (
                filtered(
                    vec![
                        Coeffs::high_pass(rate, 450.0, 0.7),
                        Coeffs::low_pass(rate, 5200.0, 0.7),
                    ],
                    1.6,
                    0.012,
                ),
                false,
            ),
            VoiceEffect::Underwater => (
                Kind::Underwater {
                    low: Biquad::new(Coeffs::low_pass(rate, 650.0, 0.8)),
                    delay: Delay::new((r * 0.03) as usize),
                    phase: 0.0,
                    step: 2.0 * PI * 0.8 / r,
                    centre: r * 0.006,
                    depth: r * 0.004,
                },
                false,
            ),
            VoiceEffect::Vibrato => (
                Kind::Vibrato {
                    delay: Delay::new((r * 0.03) as usize),
                    phase: 0.0,
                    step: 2.0 * PI * 5.5 / r,
                    centre: r * 0.008,
                    depth: r * 0.0035,
                },
                true,
            ),
        };
        Self {
            kind,
            amount,
            full_replace,
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        let amount = self.amount;
        let replace = self.full_replace;
        for frame in samples.as_chunks_mut::<2>().0 {
            let dry = *frame;
            let wet: [f32; 2] = match &mut self.kind {
                Kind::Pitch(p) => p.tick(dry),
                Kind::Robot {
                    phase,
                    step,
                    comb,
                    delay,
                } => {
                    *phase = (*phase + *step) % (2.0 * PI);
                    let m = phase.sin();
                    let mono = (dry[0] + dry[1]) * 0.5 * m;
                    let c = comb.read(*delay);
                    comb.push(mono + c * 0.45);
                    let v = (mono + c * 0.6) * 0.8;
                    [v, v]
                }
                Kind::Echo { delay, samples } => {
                    let mono = (dry[0] + dry[1]) * 0.5;
                    let tap = delay.read(*samples);
                    delay.push(mono + tap * 0.42);
                    [dry[0] + tap * 0.8, dry[1] + tap * 0.8]
                }
                Kind::Reverb { reverb, wet } => {
                    let r = reverb.tick(dry);
                    [dry[0] + r[0] * *wet, dry[1] + r[1] * *wet]
                }
                Kind::Filtered {
                    filters,
                    drive,
                    hiss,
                    noise,
                } => {
                    let mut out = dry;
                    for f in filters.iter_mut() {
                        out = [f.tick(0, out[0]), f.tick(1, out[1])];
                    }
                    let n = noise.next() * *hiss;
                    let d = *drive;
                    [
                        ((out[0] * d).tanh() + n) / d.max(1.0).sqrt(),
                        ((out[1] * d).tanh() + n) / d.max(1.0).sqrt(),
                    ]
                }
                Kind::Underwater {
                    low,
                    delay,
                    phase,
                    step,
                    centre,
                    depth,
                } => {
                    *phase = (*phase + *step) % (2.0 * PI);
                    let back = *centre + phase.sin() * *depth;
                    let l = low.tick(0, dry[0]);
                    let r = low.tick(1, dry[1]);
                    delay.push((l + r) * 0.5);
                    let v = delay.read(back) * 1.6;
                    [v, v]
                }
                Kind::Vibrato {
                    delay,
                    phase,
                    step,
                    centre,
                    depth,
                } => {
                    *phase = (*phase + *step) % (2.0 * PI);
                    let back = *centre + phase.sin() * *depth * (0.3 + 0.7 * amount);
                    delay.push((dry[0] + dry[1]) * 0.5);
                    let v = delay.read(back);
                    [v, v]
                }
            };
            let mix = if replace { 1.0 } else { amount };
            frame[0] = dry[0] * (1.0 - mix) + wet[0] * mix;
            frame[1] = dry[1] * (1.0 - mix) + wet[1] * mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tone(freq: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .flat_map(|i| {
                let v = 0.5 * (i as f32 * 2.0 * PI * freq / RATE as f32).sin();
                [v, v]
            })
            .collect()
    }

    fn pitch_hz(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        crossings as f32 / (left.len() as f32 / RATE as f32)
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    fn run(effect: VoiceEffect, amount: f32, input: &mut [f32]) {
        VoiceFx::new(RATE, &Voice { effect, amount }).process(input);
    }

    #[test]
    fn chipmunk_raises_and_deep_lowers_the_pitch() {
        let mut up = tone(400.0, 1.0);
        run(VoiceEffect::Chipmunk, 0.5, &mut up);
        let up_hz = pitch_hz(&up[RATE as usize / 2..]);
        assert!(up_hz > 520.0 && up_hz < 680.0, "chipmunk {up_hz}");
        let mut down = tone(400.0, 1.0);
        run(VoiceEffect::Deep, 0.5, &mut down);
        let down_hz = pitch_hz(&down[RATE as usize / 2..]);
        assert!(down_hz > 220.0 && down_hz < 320.0, "deep {down_hz}");
    }

    #[test]
    fn echo_keeps_going_after_the_sound_stops() {
        let mut buf = tone(300.0, 0.3);
        buf.extend(std::iter::repeat_n(0.0, RATE as usize));
        run(VoiceEffect::Echo, 1.0, &mut buf);
        // 0.32 s after the tone ended an echo is still there
        let from = (0.3 * RATE as f32) as usize * 2;
        let tail = &buf[from + 2 * 2000..from + 2 * 9000];
        assert!(rms(tail) > 0.05, "{}", rms(tail));
    }

    #[test]
    fn reverb_adds_a_tail() {
        let mut buf = tone(300.0, 0.2);
        buf.extend(std::iter::repeat_n(0.0, RATE as usize));
        run(VoiceEffect::Hall, 1.0, &mut buf);
        let tail = &buf[(0.35 * RATE as f32) as usize * 2..(0.6 * RATE as f32) as usize * 2];
        assert!(rms(tail) > 0.01, "{}", rms(tail));
    }

    #[test]
    fn telephone_removes_bass_and_treble() {
        let mut bass = tone(80.0, 0.5);
        run(VoiceEffect::Telephone, 1.0, &mut bass);
        assert!(rms(&bass[RATE as usize / 2..]) < 0.08);
        let mut mid = tone(1000.0, 0.5);
        run(VoiceEffect::Telephone, 1.0, &mut mid);
        assert!(rms(&mid[RATE as usize / 2..]) > 0.2);
        let mut treble = tone(9000.0, 0.5);
        run(VoiceEffect::Telephone, 1.0, &mut treble);
        assert!(rms(&treble[RATE as usize / 2..]) < 0.05);
    }

    #[test]
    fn every_effect_gives_finite_sound_of_the_same_length() {
        for effect in VoiceEffect::ALL {
            let mut buf = tone(250.0, 0.5);
            let n = buf.len();
            run(effect, 0.7, &mut buf);
            assert_eq!(buf.len(), n);
            assert!(
                buf.iter().all(|v| v.is_finite() && v.abs() < 4.0),
                "{effect:?}"
            );
        }
    }

    #[test]
    fn amount_zero_leaves_a_mixed_effect_dry() {
        let mut buf = tone(500.0, 0.2);
        let original = buf.clone();
        run(VoiceEffect::Telephone, 0.0, &mut buf);
        assert_eq!(buf, original);
    }
}
