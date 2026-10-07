//! A small radix-2 FFT (power-of-two sizes only).

use std::f32::consts::PI;

pub struct Fft {
    n: usize,
    cos: Vec<f32>,
    sin: Vec<f32>,
    reversed: Vec<usize>,
}

impl Fft {
    pub fn new(n: usize) -> Self {
        assert!(
            n.is_power_of_two() && n >= 2,
            "FFT size must be a power of two"
        );
        let bits = n.trailing_zeros();
        let reversed = (0..n)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        let cos = (0..n / 2)
            .map(|k| (2.0 * PI * k as f32 / n as f32).cos())
            .collect();
        let sin = (0..n / 2)
            .map(|k| (2.0 * PI * k as f32 / n as f32).sin())
            .collect();
        Self {
            n,
            cos,
            sin,
            reversed,
        }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// In-place transform. `inverse` also divides by `n`.
    pub fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        debug_assert!(re.len() == self.n && im.len() == self.n);
        for i in 0..self.n {
            let j = self.reversed[i];
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let sign = if inverse { 1.0 } else { -1.0 };
        let mut size = 2;
        while size <= self.n {
            let half = size / 2;
            let step = self.n / size;
            for start in (0..self.n).step_by(size) {
                for k in 0..half {
                    let wr = self.cos[k * step];
                    let wi = sign * self.sin[k * step];
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            size *= 2;
        }
        if inverse {
            let s = 1.0 / self.n as f32;
            for v in re.iter_mut().chain(im.iter_mut()) {
                *v *= s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_tone_and_inverts_back() {
        let n = 256;
        let fft = Fft::new(n);
        let original: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 8.0 * i as f32 / n as f32).sin())
            .collect();
        let mut re = original.clone();
        let mut im = vec![0.0; n];
        fft.run(&mut re, &mut im, false);
        let mag: Vec<f32> = re.iter().zip(&im).map(|(r, i)| r.hypot(*i)).collect();
        let (best, _) = mag[..n / 2]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        assert_eq!(best, 8);
        assert!((mag[8] - n as f32 / 2.0).abs() < 0.01);
        fft.run(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&original) {
            assert!((a - b).abs() < 1e-4);
        }
    }
}
