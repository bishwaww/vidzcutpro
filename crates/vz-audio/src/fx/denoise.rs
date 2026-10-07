//! Noise reduction: a spectral gate. First the noise is measured (the quietest parts of the clip),
//! then every short slice of sound has that much noise taken away from each frequency.

use super::fft::Fft;
use std::f32::consts::PI;

/// Size of one slice. 512 samples is about 11 ms: good for voices and a small delay.
pub const FRAME: usize = 512;
/// A new slice starts every `HOP` samples.
const HOP: usize = FRAME / 4;
/// Frequency bins in a profile.
pub const BINS: usize = FRAME / 2 + 1;

fn window() -> Vec<f32> {
    // square root of a Hann window: used twice (in and out) it adds up to a Hann window
    (0..FRAME)
        .map(|i| (PI * i as f32 / FRAME as f32).sin())
        .collect()
}

/// Measure noise from mono samples: the average spectrum of the quietest slices.
pub struct NoiseMeter {
    fft: Fft,
    window: Vec<f32>,
    ring: Vec<f32>,
    fill: usize,
    skip: usize,
    /// (loudness, spectrum) of every slice looked at.
    slices: Vec<(f32, Vec<f32>)>,
}

impl Default for NoiseMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl NoiseMeter {
    pub fn new() -> Self {
        Self {
            fft: Fft::new(FRAME),
            window: window(),
            ring: vec![0.0; FRAME],
            fill: 0,
            skip: 0,
            slices: Vec::new(),
        }
    }

    /// Feed more mono samples.
    pub fn feed(&mut self, mono: &[f32]) {
        for &x in mono {
            self.ring[self.fill] = x;
            self.fill += 1;
            if self.fill == FRAME {
                // look at every second slice (hop of half a frame) to keep memory small
                if self.skip == 0 {
                    self.look();
                }
                self.skip = (self.skip + 1) % 2;
                self.ring.copy_within(FRAME / 2.., 0);
                self.fill = FRAME / 2;
            }
        }
    }

    fn look(&mut self) {
        let mut re: Vec<f32> = self
            .ring
            .iter()
            .zip(&self.window)
            .map(|(a, b)| a * b)
            .collect();
        let mut im = vec![0.0; FRAME];
        let energy = re.iter().map(|v| v * v).sum::<f32>() / FRAME as f32;
        self.fft.run(&mut re, &mut im, false);
        let mags = (0..BINS).map(|k| re[k].hypot(im[k])).collect();
        self.slices.push((energy, mags));
    }

    /// The noise level of every bin, or `None` if there was too little sound to tell.
    pub fn finish(mut self) -> Option<Vec<f32>> {
        if self.slices.len() < 8 {
            return None;
        }
        self.slices.sort_by(|a, b| a.0.total_cmp(&b.0));
        let take = (self.slices.len() / 10).max(4).min(self.slices.len());
        let mut profile = vec![0.0f32; BINS];
        for (_, mags) in &self.slices[..take] {
            for (p, m) in profile.iter_mut().zip(mags) {
                *p += m / take as f32;
            }
        }
        Some(profile)
    }
}

/// Profiles are measured at this sample rate.
pub const PROFILE_RATE: u32 = 48_000;

/// A profile measured at [`PROFILE_RATE`] moved to the bins of another sample rate.
pub fn profile_for_rate(profile: &[f32], rate: u32) -> Vec<f32> {
    if rate == PROFILE_RATE || profile.is_empty() {
        return profile.to_vec();
    }
    (0..profile.len())
        .map(|k| {
            let x = k as f32 * rate as f32 / PROFILE_RATE as f32;
            let i = x.floor() as usize;
            let frac = x - i as f32;
            let a = profile[i.min(profile.len() - 1)];
            let b = profile[(i + 1).min(profile.len() - 1)];
            a * (1.0 - frac) + b * frac
        })
        .collect()
}

/// Removes the measured noise from a stereo stream. The output is `FRAME` samples (about 11 ms) late.
pub struct Denoiser {
    fft: Fft,
    window: Vec<f32>,
    /// How much noise to take away from the level of each bin.
    floor: Vec<f32>,
    over: f32,
    keep: f32,
    channels: [Channel; 2],
}

struct Channel {
    input: Vec<f32>,
    output: Vec<f32>,
    /// Gain of the previous slice per bin (smoothing stops the "bubbling" of cheap noise removal).
    gain: Vec<f32>,
    since: usize,
}

impl Channel {
    fn new() -> Self {
        Self {
            input: vec![0.0; FRAME],
            output: vec![0.0; FRAME],
            gain: vec![1.0; BINS],
            since: 0,
        }
    }
}

impl Denoiser {
    pub fn new(amount: f32, profile: &[f32]) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let mut floor = vec![0.0; BINS];
        for (f, p) in floor.iter_mut().zip(profile) {
            *f = *p;
        }
        Self {
            fft: Fft::new(FRAME),
            window: window(),
            floor,
            // how hard to subtract, and how much of the original level may stay at the very least
            over: 1.0 + 2.0 * amount,
            keep: 0.03 + 0.4 * (1.0 - amount),
            channels: [Channel::new(), Channel::new()],
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for frame in samples.as_chunks_mut::<2>().0 {
            for (ch, v) in frame.iter_mut().enumerate() {
                *v = self.tick(ch, *v);
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let c = &mut self.channels[ch];
        // oldest sample out, newest in (the buffers are tiny, shifting is fine)
        let out = c.output[0];
        c.output.copy_within(1.., 0);
        c.output[FRAME - 1] = 0.0;
        c.input.copy_within(1.., 0);
        c.input[FRAME - 1] = x;
        c.since += 1;
        if c.since >= HOP {
            c.since = 0;
            let mut re: Vec<f32> = c
                .input
                .iter()
                .zip(&self.window)
                .map(|(a, b)| a * b)
                .collect();
            let mut im = vec![0.0; FRAME];
            self.fft.run(&mut re, &mut im, false);
            for k in 0..BINS {
                let mag = re[k].hypot(im[k]);
                let wanted = if mag > 1e-9 {
                    ((mag - self.over * self.floor[k]) / mag).max(self.keep)
                } else {
                    self.keep
                };
                // fall fast, recover slowly
                let g = if wanted < c.gain[k] {
                    0.5 * c.gain[k] + 0.5 * wanted
                } else {
                    0.8 * c.gain[k] + 0.2 * wanted
                };
                c.gain[k] = g;
                re[k] *= g;
                im[k] *= g;
                if k > 0 && k < FRAME / 2 {
                    re[FRAME - k] = re[k];
                    im[FRAME - k] = -im[k];
                }
            }
            self.fft.run(&mut re, &mut im, true);
            // sqrt-Hann in and out at a quarter hop adds up to 2
            for i in 0..FRAME {
                c.output[i] += re[i] * self.window[i] * 0.5;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn hiss(n: usize, seed: u32, level: f32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((s >> 8) as f32 / 8_388_608.0 - 1.0) * level
            })
            .collect()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    #[test]
    fn meter_needs_enough_sound() {
        let mut m = NoiseMeter::new();
        m.feed(&hiss(300, 1, 0.1));
        assert!(m.finish().is_none());
    }

    #[test]
    fn hiss_goes_away_and_a_tone_stays() {
        // one second of hiss, then one second of tone over the same hiss
        let n = RATE as usize;
        let noise = hiss(2 * n, 7, 0.05);
        let tone: Vec<f32> = (0..2 * n)
            .map(|i| {
                if i < n {
                    0.0
                } else {
                    0.4 * (2.0 * PI * 1000.0 * i as f32 / RATE).sin()
                }
            })
            .collect();
        let mono: Vec<f32> = noise.iter().zip(&tone).map(|(a, b)| a + b).collect();

        let mut meter = NoiseMeter::new();
        meter.feed(&mono);
        let profile = meter.finish().expect("profile");
        assert_eq!(profile.len(), BINS);

        let mut stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        Denoiser::new(0.8, &profile).process(&mut stereo);
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();

        let quiet_before = rms(&mono[n / 4..n / 2]);
        let quiet_after = rms(&left[n / 4..n / 2]);
        assert!(
            quiet_after < quiet_before * 0.35,
            "hiss {quiet_before} -> {quiet_after}"
        );
        let tone_before = rms(&mono[n + n / 4..n + n / 2]);
        let tone_after = rms(&left[n + n / 4..n + n / 2]);
        assert!(
            tone_after > tone_before * 0.8,
            "tone {tone_before} -> {tone_after}"
        );
    }
}
