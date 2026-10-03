//! Sound for clips that are sped up, slowed down, reversed or follow a speed curve.
//!
//! Two ways to change the speed of sound:
//! * "varispeed": play the samples faster or slower, like a tape. The pitch follows the speed.
//! * "keep pitch": cut the sound into small overlapping pieces and re-lay them closer or further apart, picking each
//!   piece where it fits best with the one before (WSOLA). The voice keeps its pitch.
//!
//! Both work through a "virtual stream" of the clip's sound: a reversed clip reads the file backwards, so in the
//! virtual stream time always runs forwards and the speed is just how fast the reading position moves.

use crate::mixer::{micros_to_samples, samples_to_micros};
use crate::reader::{AudioError, AudioReader};
use vz_core::{Clip, Micros};

/// Anything that can give stereo samples at a position (a file reader, or a made-up signal in tests).
pub trait SampleSource {
    /// Fill `out` (interleaved stereo) with the frames starting at frame `start` (past the ends: silence).
    fn read(&mut self, start: i64, out: &mut [f32]) -> Result<(), AudioError>;
}

impl SampleSource for AudioReader {
    fn read(&mut self, start: i64, out: &mut [f32]) -> Result<(), AudioError> {
        AudioReader::read(self, start, out)
    }
}

/// The size of one piece (in frames), and how far apart pieces start in the output.
const WINDOW: usize = 1024;
const HOP: usize = WINDOW / 2;
/// How far from the ideal place a piece may be taken to fit the previous piece better.
const SEARCH: i64 = 256;

/// A copy of one stretch of the file, so that reading near the same place again (or backwards) is cheap.
#[derive(Default)]
struct Cache {
    start: i64,
    data: Vec<f32>, // interleaved stereo
}

impl Cache {
    fn frames(&self) -> i64 {
        (self.data.len() / 2) as i64
    }

    /// Frames `from .. from + out.len()/2` of the file, in order.
    fn fetch(
        &mut self,
        src: &mut dyn SampleSource,
        from: i64,
        out: &mut [f32],
    ) -> Result<(), AudioError> {
        let n = (out.len() / 2) as i64;
        if from < self.start || from + n > self.start + self.frames() {
            // load a generous stretch around the wanted place: a second behind (reverse playback walks down) and ahead
            let pad = 48_000;
            let begin = (from - pad).max(0);
            let len = (n + 2 * pad).max(96_000);
            let mut data = vec![0.0; (len * 2) as usize];
            src.read(begin, &mut data)?;
            self.start = begin;
            self.data = data;
        }
        for i in 0..n {
            let at = from + i;
            let j = ((at - self.start) * 2) as usize;
            let (l, r) = if at < 0 {
                (0.0, 0.0)
            } else {
                (
                    self.data.get(j).copied().unwrap_or(0.0),
                    self.data.get(j + 1).copied().unwrap_or(0.0),
                )
            };
            out[(i * 2) as usize] = l;
            out[(i * 2 + 1) as usize] = r;
        }
        Ok(())
    }
}

/// What makes the retimed sound of a clip: if any of this changes, the old state is thrown away.
#[derive(Clone, PartialEq)]
struct Signature {
    source_in: Micros,
    duration: Micros,
    speed: vz_anim::Animatable<f32>,
    reverse: bool,
    keep_pitch: bool,
}

impl Signature {
    fn of(clip: &Clip) -> Self {
        Self {
            source_in: clip.source_in,
            duration: clip.duration,
            speed: clip.speed.clone(),
            reverse: clip.reverse,
            keep_pitch: clip.keep_pitch,
        }
    }
}

/// Produces the sound of one retimed clip, block after block.
#[derive(Default)]
pub struct Retimer {
    signature: Option<Signature>,
    cache: Cache,
    /// The clip-local output sample the next block is expected to start at (a different start means a jump).
    expected: i64,
    // keep-pitch state
    next_frame: i64,
    prev_start: Option<i64>,
    /// Overlap-add buffer: output samples from `acc_base` on (interleaved stereo).
    acc: Vec<f32>,
    acc_base: i64,
}

impl Retimer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sound for the clip-local samples `local_start ..` (as many as `out` holds, interleaved stereo).
    pub fn render(
        &mut self,
        src: &mut dyn SampleSource,
        clip: &Clip,
        rate: u32,
        local_start: i64,
        out: &mut [f32],
    ) -> Result<(), AudioError> {
        out.fill(0.0);
        let frames = (out.len() / 2) as i64;
        let signature = Signature::of(clip);
        if self.signature.as_ref() != Some(&signature) {
            *self = Self::default();
            self.signature = Some(signature);
        }
        if clip.source_span() == 0 {
            return Ok(()); // a freeze frame has no sound
        }
        let steady = clip.speed.is_animated() || (clip.speed_at(0) - 1.0).abs() > 1e-6;
        if clip.keep_pitch && steady {
            self.keep_pitch(src, clip, rate, local_start, out)?;
        } else {
            self.varispeed(src, clip, rate, local_start, out)?;
        }
        self.expected = local_start + frames;
        Ok(())
    }

    /// Where in the clip's virtual stream (in frames) clip-local output sample `local` is.
    fn virtual_pos(clip: &Clip, rate: u32, local: i64) -> i64 {
        micros_to_samples(clip.advance(samples_to_micros(local, rate)), rate)
    }

    /// Frames `v .. v + len` of the virtual stream, in order (a reversed clip reads the file backwards).
    fn virtual_read(
        &mut self,
        src: &mut dyn SampleSource,
        clip: &Clip,
        rate: u32,
        v: i64,
        out: &mut [f32],
    ) -> Result<(), AudioError> {
        let n = (out.len() / 2) as i64;
        if clip.reverse {
            let top = micros_to_samples(clip.source_in + clip.source_span(), rate);
            // virtual frame v is file frame top - 1 - v
            self.cache.fetch(src, top - v - n, out)?;
            // turn the frame order around
            let (mut a, mut b) = (0usize, (n as usize).saturating_sub(1));
            while a < b {
                for c in 0..2 {
                    out.swap(a * 2 + c, b * 2 + c);
                }
                a += 1;
                b -= 1;
            }
        } else {
            let base = micros_to_samples(clip.source_in, rate);
            self.cache.fetch(src, base + v, out)?;
        }
        Ok(())
    }

    /// Plain resampling: the reading position moves by the speed for every output sample.
    fn varispeed(
        &mut self,
        src: &mut dyn SampleSource,
        clip: &Clip,
        rate: u32,
        local_start: i64,
        out: &mut [f32],
    ) -> Result<(), AudioError> {
        let n = out.len() / 2;
        let first = Self::virtual_pos(clip, rate, local_start);
        // positions of every output frame (the exact start of the block, then the speed added up)
        let mut positions = Vec::with_capacity(n + 1);
        let mut pos = first as f64;
        for i in 0..n {
            if i % 256 == 0 {
                // follow the speed curve: the speed in the middle of the next 256 samples
                let mid = samples_to_micros(local_start + i as i64 + 128, rate);
                let step = clip.speed_at(mid);
                positions.push((pos, step));
            }
            let step = positions.last().map_or(1.0, |p| p.1);
            pos += step;
        }
        let last = pos.ceil() as i64 + 2;
        let lo = first.min(last);
        let len = (last - first).unsigned_abs() as usize + 4;
        let mut chunk = vec![0.0; len * 2];
        self.virtual_read(src, clip, rate, lo, &mut chunk)?;
        let mut pos = first as f64;
        let mut step = 1.0;
        for i in 0..n {
            if i % 256 == 0 {
                let mid = samples_to_micros(local_start + i as i64 + 128, rate);
                step = clip.speed_at(mid);
            }
            let rel = pos - lo as f64;
            let (i0, f) = (rel.floor().max(0.0) as usize, (rel - rel.floor()) as f32);
            if i0 + 1 < len {
                for c in 0..2 {
                    let a = chunk[i0 * 2 + c];
                    let b = chunk[(i0 + 1) * 2 + c];
                    out[i * 2 + c] = a + (b - a) * f;
                }
            }
            pos += step;
        }
        Ok(())
    }

    /// Time stretching that keeps the pitch (WSOLA).
    fn keep_pitch(
        &mut self,
        src: &mut dyn SampleSource,
        clip: &Clip,
        rate: u32,
        local_start: i64,
        out: &mut [f32],
    ) -> Result<(), AudioError> {
        let n = (out.len() / 2) as i64;
        if self.expected != local_start {
            // a jump: start again one piece early, so the overlap at the start of the block is complete
            let k = (local_start / HOP as i64 - 1).max(0);
            self.next_frame = k;
            self.prev_start = None;
            self.acc.clear();
            self.acc_base = k * HOP as i64;
        }
        let want = local_start + n;
        // output is complete up to the start of the next piece
        while self.next_frame * (HOP as i64) < want + HOP as i64 {
            self.add_piece(src, clip, rate)?;
        }
        // copy out, then forget what has been played
        for i in 0..n {
            let j = ((local_start + i - self.acc_base) * 2) as usize;
            if let (Some(l), Some(r)) = (self.acc.get(j), self.acc.get(j + 1))
                && local_start + i >= self.acc_base
            {
                out[(i * 2) as usize] = *l;
                out[(i * 2 + 1) as usize] = *r;
            }
        }
        let keep_from = (want - HOP as i64).max(self.acc_base);
        let drop = ((keep_from - self.acc_base) * 2) as usize;
        if drop > 0 && drop <= self.acc.len() {
            self.acc.drain(..drop);
            self.acc_base = keep_from;
        }
        Ok(())
    }

    /// Cut one piece out of the sound and lay it into the output.
    fn add_piece(
        &mut self,
        src: &mut dyn SampleSource,
        clip: &Clip,
        rate: u32,
    ) -> Result<(), AudioError> {
        let k = self.next_frame;
        let out_pos = k * HOP as i64;
        let ideal = Self::virtual_pos(clip, rate, out_pos);
        // read the area where the piece may come from, and the natural continuation of the previous piece
        let lo = ideal - SEARCH;
        let span = WINDOW + 2 * SEARCH as usize;
        let mut area = vec![0.0; span * 2];
        self.virtual_read(src, clip, rate, lo, &mut area)?;
        let mut best = SEARCH as usize;
        if let Some(prev) = self.prev_start {
            let want_start = prev + HOP as i64; // where the previous piece would carry on
            let mut template = vec![0.0; WINDOW * 2];
            self.virtual_read(src, clip, rate, want_start, &mut template)?;
            let mut best_score = f32::MIN;
            // compare the first half-window, every 4th frame, on a mono mix
            for cand in (0..=2 * SEARCH as usize).step_by(2) {
                let mut score = 0.0;
                for i in (0..WINDOW / 2).step_by(4) {
                    let a = area[(cand + i) * 2] + area[(cand + i) * 2 + 1];
                    let b = template[i * 2] + template[i * 2 + 1];
                    score += a * b;
                }
                if score > best_score {
                    best_score = score;
                    best = cand;
                }
            }
        }
        self.prev_start = Some(lo + best as i64);
        // overlap-add the piece with a Hann window (two pieces half a window apart add up to exactly one)
        let end = ((out_pos + WINDOW as i64 - self.acc_base) * 2) as usize;
        if self.acc.len() < end {
            self.acc.resize(end, 0.0);
        }
        for i in 0..WINDOW {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * (i as f32 + 0.5) / WINDOW as f32).cos();
            let at = (out_pos - self.acc_base + i as i64) * 2;
            if at < 0 {
                continue;
            }
            let at = at as usize;
            self.acc[at] += area[(best + i) * 2] * w;
            self.acc[at + 1] += area[(best + i) * 2 + 1] * w;
        }
        self.next_frame += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::US_PER_SEC;

    const RATE: u32 = 48_000;

    /// A steady 440 Hz tone, the same in both ears, loud enough to measure.
    struct Tone;

    impl SampleSource for Tone {
        fn read(&mut self, start: i64, out: &mut [f32]) -> Result<(), AudioError> {
            for (i, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let at = start + i as i64;
                let v = if at < 0 {
                    0.0
                } else {
                    (at as f32 / RATE as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5
                };
                frame[0] = v;
                frame[1] = v;
            }
            Ok(())
        }
    }

    /// A rising ramp: the value tells the position in the file (one unit per second, over a 4 second file).
    struct Ramp;

    impl SampleSource for Ramp {
        fn read(&mut self, start: i64, out: &mut [f32]) -> Result<(), AudioError> {
            for (i, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let at = start + i as i64;
                let v = if at < 0 {
                    0.0
                } else {
                    at as f32 / RATE as f32 / 4.0
                };
                frame[0] = v;
                frame[1] = v;
            }
            Ok(())
        }
    }

    fn clip(seconds: i64) -> Clip {
        Clip::media_clip(vz_core::MediaId::new(), 0, 0, seconds * US_PER_SEC)
    }

    fn pitch_hz(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        crossings as f32 / (left.len() as f32 / RATE as f32)
    }

    fn rms(stereo: &[f32]) -> f32 {
        (stereo.iter().map(|v| v * v).sum::<f32>() / stereo.len().max(1) as f32).sqrt()
    }

    fn render(c: &Clip, from: i64, frames: usize) -> Vec<f32> {
        let mut r = Retimer::new();
        let mut out = vec![0.0; frames * 2];
        r.render(&mut Tone, c, RATE, from, &mut out).unwrap();
        out
    }

    #[test]
    fn varispeed_changes_the_pitch_with_the_speed() {
        let mut c = clip(4);
        c.keep_pitch = false;
        c.speed = 2.0.into();
        c.duration = 2 * US_PER_SEC;
        let out = render(&c, 4800, 48_000);
        let hz = pitch_hz(&out);
        assert!((hz - 880.0).abs() < 25.0, "{hz}");
        c.speed = 0.5.into();
        c.duration = 8 * US_PER_SEC;
        let hz = pitch_hz(&render(&c, 4800, 48_000));
        assert!((hz - 220.0).abs() < 15.0, "{hz}");
    }

    #[test]
    fn keeping_the_pitch_keeps_the_note_and_the_loudness() {
        for speed in [0.6f32, 1.5, 2.0] {
            let mut c = clip(4);
            c.keep_pitch = true;
            c.speed = speed.into();
            c.duration = (4.0 * US_PER_SEC as f32 / speed) as i64;
            let out = render(&c, 24_000, 48_000);
            let hz = pitch_hz(&out);
            assert!((hz - 440.0).abs() < 30.0, "speed {speed}: {hz} Hz");
            let level = rms(&out);
            assert!(
                level > 0.25 && level < 0.45,
                "speed {speed}: loudness {level}"
            );
        }
    }

    #[test]
    fn a_reversed_clip_reads_the_file_backwards() {
        let mut c = clip(4);
        c.reverse = true;
        c.keep_pitch = false;
        let mut r = Retimer::new();
        let mut out = vec![0.0; 2 * 4800];
        r.render(&mut Ramp, &c, RATE, 0, &mut out).unwrap();
        // the clip starts at the end of the file (value near 1) and falls
        assert!(out[0] > 0.99, "{}", out[0]);
        assert!(out[out.len() - 2] < out[0]);
        for pair in out.as_chunks::<2>().0.windows(2) {
            assert!(pair[1][0] <= pair[0][0] + 1e-6, "must fall steadily");
        }
    }

    #[test]
    fn a_reversed_clip_at_double_speed_falls_twice_as_fast() {
        let mut c = clip(2);
        c.speed = 2.0.into(); // 4 s of file in 2 s
        c.reverse = true;
        c.keep_pitch = false;
        let mut r = Retimer::new();
        let mut out = vec![0.0; 4 * RATE as usize];
        r.render(&mut Ramp, &c, RATE, 0, &mut out).unwrap();
        // after one second the position in the file has fallen by two seconds (0.5 of the ramp)
        let at = RATE as usize;
        assert!((out[at * 2] - 0.5).abs() < 0.01, "{}", out[at * 2]);
    }

    #[test]
    fn playing_in_blocks_gives_the_same_sound_as_one_long_block() {
        let mut c = clip(4);
        c.keep_pitch = true;
        c.speed = 1.5.into();
        c.duration = 3 * US_PER_SEC;
        let all = render(&c, 0, 30_000);
        let mut r = Retimer::new();
        let mut blocks = Vec::new();
        for start in (0..30_000).step_by(1000) {
            let mut out = vec![0.0; 2000];
            r.render(&mut Tone, &c, RATE, start, &mut out).unwrap();
            blocks.extend(out);
        }
        let diff = all
            .iter()
            .zip(&blocks)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(diff < 1e-4, "{diff}");
    }

    #[test]
    fn jumping_somewhere_else_gives_clean_sound_straight_away() {
        let mut c = clip(8);
        c.keep_pitch = true;
        c.speed = 2.0.into();
        c.duration = 4 * US_PER_SEC;
        let mut r = Retimer::new();
        let mut out = vec![0.0; 2 * 4800];
        r.render(&mut Tone, &c, RATE, 0, &mut out).unwrap();
        // a seek far ahead: the block after the jump is as loud as steady sound
        let mut after = vec![0.0; 2 * 9600];
        r.render(&mut Tone, &c, RATE, 100_000, &mut after).unwrap();
        let level = rms(&after);
        assert!(level > 0.3 && level < 0.4, "{level}");
    }

    #[test]
    fn a_freeze_frame_is_silent() {
        let mut c = clip(3);
        c.speed = 0.0.into();
        let out = render(&c, 0, 4800);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn changing_the_clip_starts_fresh() {
        let mut c = clip(4);
        c.keep_pitch = true;
        c.speed = 2.0.into();
        c.duration = 2 * US_PER_SEC;
        let mut r = Retimer::new();
        let mut out = vec![0.0; 2 * 4800];
        r.render(&mut Tone, &c, RATE, 0, &mut out).unwrap();
        c.speed = 3.0.into();
        c.duration = US_PER_SEC;
        r.render(&mut Tone, &c, RATE, 4800, &mut out).unwrap();
        assert!(rms(&out) > 0.2);
    }
}
