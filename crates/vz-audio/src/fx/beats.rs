//! Finding the beat of music: a curve of "how much new sound starts now" is made from the
//! spectrum, the tempo comes from how well that curve repeats, and the beats are the best-fitting grid.

use super::fft::Fft;
use std::f32::consts::PI;
use vz_core::Micros;

const FRAME: usize = 1024;
const HOP: usize = 512;
const MIN_BPM: f32 = 70.0;
const MAX_BPM: f32 = 180.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Beats {
    pub bpm: f32,
    /// Times of the beats from the start of the audio that was analysed.
    pub times: Vec<Micros>,
}

pub struct BeatDetector {
    rate: u32,
    fft: Fft,
    window: Vec<f32>,
    ring: Vec<f32>,
    fill: usize,
    previous: Vec<f32>,
    onsets: Vec<f32>,
}

impl BeatDetector {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            fft: Fft::new(FRAME),
            window: (0..FRAME)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FRAME as f32).cos())
                .collect(),
            ring: vec![0.0; FRAME],
            fill: 0,
            previous: Vec::new(),
            onsets: Vec::new(),
        }
    }

    /// Feed more mono samples.
    pub fn feed(&mut self, mono: &[f32]) {
        for &x in mono {
            self.ring[self.fill] = x;
            self.fill += 1;
            if self.fill == FRAME {
                self.look();
                self.ring.copy_within(HOP.., 0);
                self.fill = FRAME - HOP;
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
        self.fft.run(&mut re, &mut im, false);
        // bins up to 8 kHz
        let top = ((8000.0 / self.rate as f32) * FRAME as f32) as usize;
        let top = top.clamp(8, FRAME / 2);
        let now: Vec<f32> = (1..top)
            .map(|k| (1.0 + 100.0 * re[k].hypot(im[k]) / FRAME as f32 * 8.0).ln())
            .collect();
        let flux = if self.previous.len() == now.len() {
            now.iter()
                .zip(&self.previous)
                .map(|(a, b)| (a - b).max(0.0))
                .sum()
        } else {
            0.0
        };
        self.previous = now;
        self.onsets.push(flux);
    }

    pub fn finish(self) -> Beats {
        let fps = self.rate as f32 / HOP as f32;
        let env = prepare(&self.onsets, fps);
        if env.len() < (fps * 4.0) as usize {
            return Beats::default();
        }
        let Some(period) = best_period(&env, fps) else {
            return Beats::default();
        };
        let phase = best_phase(&env, period);
        let mut times = Vec::new();
        let mut at = phase;
        while at < env.len() as f32 {
            // the picture of a frame sits a little after the start of its window
            let samples = at * HOP as f32 + FRAME as f32 / 4.0;
            times.push((samples as f64 / f64::from(self.rate) * 1e6) as Micros);
            at += period;
        }
        Beats {
            bpm: 60.0 * fps / period,
            times,
        }
    }
}

/// Take away the slow average so only the sudden rises are left.
fn prepare(onsets: &[f32], fps: f32) -> Vec<f32> {
    let half = (fps * 0.4) as usize;
    let mut out = Vec::with_capacity(onsets.len());
    let mut prefix = vec![0.0f64; onsets.len() + 1];
    for (i, v) in onsets.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(*v);
    }
    for (i, v) in onsets.iter().enumerate() {
        let a = i.saturating_sub(half);
        let b = (i + half + 1).min(onsets.len());
        let mean = ((prefix[b] - prefix[a]) / (b - a) as f64) as f32;
        out.push((v - mean * 1.1).max(0.0));
    }
    out
}

fn autocorrelation(env: &[f32], lag: usize) -> f32 {
    if lag >= env.len() {
        return 0.0;
    }
    let n = env.len() - lag;
    env[..n]
        .iter()
        .zip(&env[lag..])
        .map(|(a, b)| a * b)
        .sum::<f32>()
        / n as f32
}

/// Beat distance in frames (with a fraction), or `None` if the sound has no clear beat.
#[allow(clippy::needless_range_loop)]
fn best_period(env: &[f32], fps: f32) -> Option<f32> {
    let min_lag = (fps * 60.0 / MAX_BPM).floor() as usize;
    let max_lag = (fps * 60.0 / MIN_BPM).ceil() as usize;
    let energy = autocorrelation(env, 0).max(1e-9);
    let mut scores = vec![0.0f32; max_lag + 2];
    let mut best = (0usize, 0.0f32);
    for lag in min_lag.saturating_sub(1).max(1)..=max_lag + 1 {
        // a gentle preference for tempos near 120 BPM
        let bpm = 60.0 * fps / lag as f32;
        let octaves = (bpm / 120.0).log2();
        let weight = (-0.5 * (octaves / 0.8).powi(2)).exp();
        // a beat is not always a whole number of frames long, so look one frame either side too
        let near = autocorrelation(env, lag.saturating_sub(1))
            + 2.0 * autocorrelation(env, lag)
            + autocorrelation(env, lag + 1);
        let s = near * weight;
        scores[lag] = s;
        if (min_lag..=max_lag).contains(&lag) && s > best.1 {
            best = (lag, s);
        }
    }
    let (lag, score) = best;
    if lag == 0 || score / energy < 0.03 {
        return None;
    }
    // refine with a parabola through the neighbours
    let (a, b, c) = (scores[lag - 1], scores[lag], scores[lag + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some(lag as f32 + shift)
}

/// Where in the first beat period the grid should start.
fn best_phase(env: &[f32], period: f32) -> f32 {
    let steps = period.ceil() as usize * 2;
    let mut best = (0.0f32, f32::MIN);
    for s in 0..steps {
        let phase = s as f32 * period / steps as f32;
        let mut sum = 0.0;
        let mut at = phase;
        while (at as usize) + 1 < env.len() {
            // look a frame either side so a small drift still counts
            let i = at.round() as usize;
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(env.len() - 1);
            sum += env[i] + 0.4 * (env[lo] + env[hi]);
            at += period;
        }
        if sum > best.1 {
            best = (phase, sum);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A drum-like click every `beat` seconds.
    fn clicks(seconds: f32, beat: f32, first: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        let mut out = vec![0.0f32; n];
        let mut seed = 99u32;
        let mut t = first;
        while t < seconds {
            let start = (t * RATE as f32) as usize;
            for i in 0..(RATE as usize / 25).min(n.saturating_sub(start)) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
                out[start + i] += noise * (-(i as f32) / 300.0).exp() * 0.8;
            }
            t += beat;
        }
        out
    }

    fn detect(mono: &[f32]) -> Beats {
        let mut d = BeatDetector::new(RATE);
        d.feed(mono);
        d.finish()
    }

    #[test]
    fn finds_the_tempo_and_the_places_of_the_beats() {
        let beats = detect(&clicks(16.0, 0.5, 0.2));
        assert!((beats.bpm - 120.0).abs() < 3.0, "bpm {}", beats.bpm);
        assert!(beats.times.len() >= 28, "{} beats", beats.times.len());
        // every beat is close to a real click (clicks are at 0.2 + k * 0.5 s)
        for t in &beats.times {
            let s = *t as f64 / 1e6;
            let off = ((s - 0.2) / 0.5).rem_euclid(1.0);
            let off = off.min(1.0 - off) * 0.5;
            assert!(off < 0.04, "beat at {s} s is {off} s from a click");
        }
    }

    #[test]
    fn follows_a_different_tempo() {
        let beats = detect(&clicks(16.0, 0.4, 0.1)); // 150 BPM
        assert!((beats.bpm - 150.0).abs() < 4.0, "bpm {}", beats.bpm);
    }

    #[test]
    fn silence_and_very_short_sound_have_no_beats() {
        assert!(detect(&vec![0.0; RATE as usize * 8]).times.is_empty());
        assert!(detect(&clicks(2.0, 0.5, 0.1)).times.is_empty());
    }
}
