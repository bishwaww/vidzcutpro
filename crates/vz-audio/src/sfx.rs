//! Our own sound effects, made from maths (no recordings, so no licences): whoosh, pop, click, swoosh,
//! ding, camera shutter, typing, riser and impact.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Whoosh,
    Pop,
    Click,
    Swoosh,
    Ding,
    Shutter,
    Typing,
    Riser,
    Impact,
}

impl Sfx {
    pub const ALL: [Self; 9] = [
        Self::Whoosh,
        Self::Pop,
        Self::Click,
        Self::Swoosh,
        Self::Ding,
        Self::Shutter,
        Self::Typing,
        Self::Riser,
        Self::Impact,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Whoosh => "Whoosh",
            Self::Pop => "Pop",
            Self::Click => "Click",
            Self::Swoosh => "Swoosh",
            Self::Ding => "Ding",
            Self::Shutter => "Camera shutter",
            Self::Typing => "Typing",
            Self::Riser => "Riser",
            Self::Impact => "Impact",
        }
    }

    /// A file-name friendly name.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Whoosh => "whoosh",
            Self::Pop => "pop",
            Self::Click => "click",
            Self::Swoosh => "swoosh",
            Self::Ding => "ding",
            Self::Shutter => "camera-shutter",
            Self::Typing => "typing",
            Self::Riser => "riser",
            Self::Impact => "impact",
        }
    }

    pub fn seconds(self) -> f32 {
        match self {
            Self::Whoosh => 1.2,
            Self::Pop => 0.25,
            Self::Click => 0.12,
            Self::Swoosh => 0.8,
            Self::Ding => 1.6,
            Self::Shutter => 0.35,
            Self::Typing => 2.0,
            Self::Riser => 3.0,
            Self::Impact => 1.8,
        }
    }
}

/// The same noise every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// A one-pole low-pass for shaping noise.
struct Smooth(f32);

impl Smooth {
    fn tick(&mut self, x: f32, k: f32) -> f32 {
        self.0 += (x - self.0) * k.clamp(0.0, 1.0);
        self.0
    }
}

/// The effect as mono samples at `rate`.
pub fn render(sfx: Sfx, rate: u32) -> Vec<f32> {
    let n = (sfx.seconds() * rate as f32) as usize;
    let r = rate as f32;
    let mut noise = Noise(0x9E37_79B9);
    let mut out = vec![0.0f32; n];
    match sfx {
        Sfx::Whoosh | Sfx::Swoosh => {
            // noise through a filter that opens then closes
            let (len, peak) = if sfx == Sfx::Whoosh {
                (1.2, 0.45)
            } else {
                (0.8, 0.3)
            };
            let mut lp = Smooth(0.0);
            let mut lp2 = Smooth(0.0);
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r / len;
                let env = (PI * t).sin().powf(1.5);
                let cutoff = 0.01 + 0.35 * (PI * t).sin().powi(2);
                let v = lp2.tick(lp.tick(noise.next(), cutoff), cutoff);
                *o = v * env * 4.0 * peak / 0.45;
            }
        }
        Sfx::Pop => {
            let mut phase = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let f = 90.0 + 700.0 * (-t * 40.0).exp();
                phase += 2.0 * PI * f / r;
                *o = phase.sin() * (-t * 22.0).exp() * 0.9;
            }
        }
        Sfx::Click => {
            let mut hp = Smooth(0.0);
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let x = noise.next();
                let v = x - hp.tick(x, 0.2);
                *o = v * (-t * 160.0).exp() * 0.9;
            }
        }
        Sfx::Ding => {
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let v = (2.0 * PI * 1318.5 * t).sin()
                    + 0.5 * (2.0 * PI * 2637.0 * t).sin() * (-t * 6.0).exp()
                    + 0.25 * (2.0 * PI * 3951.0 * t).sin() * (-t * 9.0).exp();
                *o = v * (-t * 2.8).exp() * 0.4 * (1.0 - (-t * 800.0).exp());
            }
        }
        Sfx::Shutter => {
            // two clicks, the second a little lower and later
            let mut hp = Smooth(0.0);
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let x = noise.next();
                let v = x - hp.tick(x, 0.15);
                let a = (-t * 120.0).exp();
                let t2 = (t - 0.11).max(0.0);
                let b = if t >= 0.11 {
                    (-t2 * 90.0).exp() * 0.7
                } else {
                    0.0
                };
                *o = v * (a + b) * 0.8;
            }
        }
        Sfx::Typing => {
            // a burst of keys at uneven times
            let mut hp = Smooth(0.0);
            let mut next = 0.05f32;
            let mut last = -1.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                if t >= next {
                    last = t;
                    next += 0.07 + 0.09 * (noise.next() * 0.5 + 0.5);
                }
                let x = noise.next();
                let v = x - hp.tick(x, 0.25);
                let since = if last >= 0.0 { t - last } else { 10.0 };
                *o = v * (-since * 180.0).exp() * 0.7;
            }
        }
        Sfx::Riser => {
            let mut lp = Smooth(0.0);
            let mut phase = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                let k = t / 3.0;
                phase += 2.0 * PI * (120.0 + 1400.0 * k * k) / r;
                let tone = (phase.sin() + 0.5 * (phase * 2.0).sin()) * 0.4;
                let hiss = lp.tick(noise.next(), 0.05 + 0.8 * k) * 0.8;
                *o = (tone + hiss) * k.powf(1.6) * 0.7;
            }
        }
        Sfx::Impact => {
            let mut lp = Smooth(0.0);
            let mut phase = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                let t = i as f32 / r;
                phase += 2.0 * PI * (38.0 + 90.0 * (-t * 14.0).exp()) / r;
                let boom = phase.sin() * (-t * 2.6).exp();
                let crack = lp.tick(noise.next(), 0.5) * (-t * 28.0).exp();
                *o = (boom * 0.9 + crack * 0.6) * (1.0 - (-t * 1500.0).exp());
            }
        }
    }
    // a short fade at the end so nothing clicks
    let fade = (rate as usize / 100).min(n);
    for i in 0..fade {
        out[n - 1 - i] *= i as f32 / fade as f32;
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 0.0 {
        let gain = 0.8 / peak;
        out.iter_mut().for_each(|v| *v *= gain);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_has_sound_of_the_right_length_without_clipping() {
        for sfx in Sfx::ALL {
            let s = render(sfx, 48_000);
            assert_eq!(s.len(), (sfx.seconds() * 48_000.0) as usize, "{sfx:?}");
            let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!((peak - 0.8).abs() < 0.01, "{sfx:?} peak {peak}");
            assert!(s.iter().all(|v| v.is_finite()), "{sfx:?}");
            assert_eq!(*s.last().unwrap(), 0.0);
        }
    }

    #[test]
    fn the_riser_gets_louder() {
        let s = render(Sfx::Riser, 48_000);
        let rms = |a: &[f32]| (a.iter().map(|v| v * v).sum::<f32>() / a.len() as f32).sqrt();
        assert!(rms(&s[s.len() * 3 / 4..]) > rms(&s[..s.len() / 4]) * 3.0);
    }
}
