//! The five-band equaliser.

use super::biquad::{Biquad, Coeffs};
use vz_core::{EQ_BANDS, EQ_FREQS, Eq};

pub struct Equaliser {
    /// Only the bands that change something.
    bands: Vec<Biquad>,
}

impl Equaliser {
    pub fn new(rate: u32, eq: &Eq) -> Self {
        let mut bands = Vec::new();
        for (i, (&gain, &freq)) in eq.gains.iter().zip(EQ_FREQS.iter()).enumerate() {
            if gain.abs() < 0.05 {
                continue;
            }
            let gain = gain.clamp(-12.0, 12.0);
            let c = match i {
                0 => Coeffs::low_shelf(rate, freq, gain),
                n if n == EQ_BANDS - 1 => Coeffs::high_shelf(rate, freq, gain),
                _ => Coeffs::peaking(rate, freq, gain, 0.9),
            };
            bands.push(Biquad::new(c));
        }
        Self { bands }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for band in &mut self.bands {
            band.process(samples);
        }
    }
}

/// The total gain (a factor) of the equaliser at `freq`, for drawing its curve.
pub fn response(rate: u32, eq: &Eq, freq: f32) -> f64 {
    let mut total = 1.0;
    for (i, (&gain, &f)) in eq.gains.iter().zip(EQ_FREQS.iter()).enumerate() {
        if gain.abs() < 0.05 {
            continue;
        }
        let gain = gain.clamp(-12.0, 12.0);
        let c = match i {
            0 => Coeffs::low_shelf(rate, f, gain),
            n if n == EQ_BANDS - 1 => Coeffs::high_shelf(rate, f, gain),
            _ => Coeffs::peaking(rate, f, gain, 0.9),
        };
        total *= c.response(rate, freq);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_equaliser_does_nothing_and_boost_lifts_bass() {
        let flat = Equaliser::new(48_000, &Eq::default());
        assert!(flat.bands.is_empty());
        let eq = Eq {
            gains: [6.0, 0.0, 0.0, 0.0, 0.0],
        };
        let low = response(48_000, &eq, 30.0);
        let high = response(48_000, &eq, 4000.0);
        assert!(low > 1.9 && low < 2.1, "{low}");
        assert!((high - 1.0).abs() < 0.03, "{high}");
    }
}
