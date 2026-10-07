//! Two-pole filters (the "audio EQ cookbook" shapes), used by the equaliser, the voice effects and the loudness meter.

use std::f64::consts::PI;

/// Filter settings, already divided by `a0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    /// Lets everything through unchanged.
    pub const PASS: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn normalised(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    fn parts(rate: u32, freq: f32) -> (f64, f64, f64) {
        let freq = f64::from(freq).clamp(10.0, f64::from(rate) * 0.49);
        let w = 2.0 * PI * freq / f64::from(rate);
        (w, w.cos(), w.sin())
    }

    pub fn low_pass(rate: u32, freq: f32, q: f32) -> Self {
        let (_, cos, sin) = Self::parts(rate, freq);
        let alpha = sin / (2.0 * f64::from(q));
        Self::normalised(
            (1.0 - cos) / 2.0,
            1.0 - cos,
            (1.0 - cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    pub fn high_pass(rate: u32, freq: f32, q: f32) -> Self {
        let (_, cos, sin) = Self::parts(rate, freq);
        let alpha = sin / (2.0 * f64::from(q));
        Self::normalised(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    /// Lifts or cuts around `freq` (gain in decibels).
    pub fn peaking(rate: u32, freq: f32, gain_db: f32, q: f32) -> Self {
        let (_, cos, sin) = Self::parts(rate, freq);
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let alpha = sin / (2.0 * f64::from(q));
        Self::normalised(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    /// Lifts or cuts everything below `freq`.
    pub fn low_shelf(rate: u32, freq: f32, gain_db: f32) -> Self {
        let (_, cos, sin) = Self::parts(rate, freq);
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let alpha = sin / 2.0 * 2f64.sqrt();
        let beta = 2.0 * a.sqrt() * alpha;
        Self::normalised(
            a * ((a + 1.0) - (a - 1.0) * cos + beta),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - beta),
            (a + 1.0) + (a - 1.0) * cos + beta,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - beta,
        )
    }

    /// Lifts or cuts everything above `freq`.
    pub fn high_shelf(rate: u32, freq: f32, gain_db: f32) -> Self {
        let (_, cos, sin) = Self::parts(rate, freq);
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let alpha = sin / 2.0 * 2f64.sqrt();
        let beta = 2.0 * a.sqrt() * alpha;
        Self::normalised(
            a * ((a + 1.0) + (a - 1.0) * cos + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - beta),
            (a + 1.0) - (a - 1.0) * cos + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - beta,
        )
    }

    /// Gain of the filter at `freq` (a factor), for tests and drawing curves.
    pub fn response(&self, rate: u32, freq: f32) -> f64 {
        let w = 2.0 * PI * f64::from(freq) / f64::from(rate);
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).sqrt()
    }
}

/// One filter running on a stereo stream.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub c: Coeffs,
    z1: [f64; 2],
    z2: [f64; 2],
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        Self {
            c,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.c.b0 * x + self.z1[ch];
        self.z1[ch] = self.c.b1 * x - self.c.a1 * y + self.z2[ch];
        self.z2[ch] = self.c.b2 * x - self.c.a2 * y;
        y as f32
    }

    /// Filter interleaved stereo in place.
    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.as_chunks_mut::<2>().0 {
            frame[0] = self.tick(0, frame[0]);
            frame[1] = self.tick(1, frame[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    #[test]
    fn shelves_and_peaks_have_the_requested_gain() {
        let db = |g: f64| 20.0 * g.log10();
        let low = Coeffs::low_shelf(RATE, 100.0, 6.0);
        assert!((db(low.response(RATE, 20.0)) - 6.0).abs() < 0.3);
        assert!(db(low.response(RATE, 5000.0)).abs() < 0.3);
        let high = Coeffs::high_shelf(RATE, 10_000.0, -6.0);
        assert!((db(high.response(RATE, 20_000.0)) + 6.0).abs() < 0.5);
        assert!(db(high.response(RATE, 200.0)).abs() < 0.3);
        let peak = Coeffs::peaking(RATE, 1000.0, 5.0, 1.0);
        assert!((db(peak.response(RATE, 1000.0)) - 5.0).abs() < 0.05);
        assert!(db(peak.response(RATE, 50.0)).abs() < 0.3);
    }

    #[test]
    fn pass_filters_cut_the_far_side() {
        let lp = Coeffs::low_pass(RATE, 1000.0, 0.707);
        assert!(lp.response(RATE, 100.0) > 0.95);
        assert!(lp.response(RATE, 8000.0) < 0.05);
        let hp = Coeffs::high_pass(RATE, 1000.0, 0.707);
        assert!(hp.response(RATE, 8000.0) > 0.95);
        assert!(hp.response(RATE, 100.0) < 0.05);
    }

    #[test]
    fn running_filter_matches_its_response() {
        let mut f = Biquad::new(Coeffs::low_pass(RATE, 1000.0, 0.707));
        let n = 9600;
        let mut buf: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = (i as f32 * 2.0 * std::f32::consts::PI * 8000.0 / RATE as f32).sin();
                [v, v]
            })
            .collect();
        f.process(&mut buf);
        let tail = &buf[buf.len() / 2..];
        let peak = tail.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 0.06, "8 kHz should be almost gone, peak {peak}");
    }
}
