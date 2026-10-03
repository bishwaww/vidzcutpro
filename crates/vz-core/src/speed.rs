//! How fast a clip plays: constant speed, speed curves, reverse, and freeze frames.
//!
//! A clip shows the part of its media file from `source_in` for `source_span()`. The speed (a number or a curve over
//! the clip's own time) says how much media passes per second of timeline: at 2 the clip eats the media twice as
//! fast and is half as long. Reversed clips play that same part of the file backwards.

use crate::time::Micros;
use crate::timeline::Clip;

/// The slowest and fastest speeds a clip can have (0 is a freeze frame).
pub const MIN_SPEED: f32 = 0.0;
pub const MAX_SPEED: f32 = 100.0;

impl Clip {
    /// The speed (media seconds per timeline second) `local` microseconds after the clip's start.
    pub fn speed_at(&self, local: Micros) -> f64 {
        f64::from(self.speed.value_at(local).clamp(MIN_SPEED, MAX_SPEED))
    }

    /// Is the speed something other than a plain 1x?
    pub fn is_retimed(&self) -> bool {
        self.reverse || self.speed != vz_anim::Animatable::Static(1.0)
    }

    /// How much media (in microseconds) goes by during the first `local` microseconds of the clip.
    /// Before the start the first speed carries on, so the answer is negative there.
    pub fn advance(&self, local: Micros) -> Micros {
        if local == 0 {
            return 0;
        }
        if local < 0 || !self.speed.is_animated() {
            return (self.speed_at(local.max(0)) * local as f64).round() as Micros;
        }
        // the speed changes along the way: add up pieces between keyframes (Simpson's rule on each)
        let mut edges: Vec<Micros> = vec![0];
        edges.extend(
            self.speed
                .keys()
                .iter()
                .map(|k| k.time)
                .filter(|t| *t > 0 && *t < local),
        );
        edges.push(local);
        let mut total = 0.0;
        for pair in edges.windows(2) {
            let (a, b) = (pair[0] as f64, pair[1] as f64);
            const STEPS: usize = 16;
            let h = (b - a) / STEPS as f64;
            let f = |t: f64| self.speed_at(t.round() as Micros);
            let mut sum = f(a) + f(b);
            for i in 1..STEPS {
                sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
            }
            total += sum * h / 3.0;
        }
        total.round() as Micros
    }

    /// How much media the whole clip uses.
    pub fn source_span(&self) -> Micros {
        self.advance(self.duration)
    }

    /// The moment of the media file shown at timeline time `t`. Outside the clip the first or last speed carries on, so
    /// a transition can ask for pictures just before or after the clip.
    pub fn source_time(&self, t: Micros) -> Micros {
        let local = t - self.start;
        if self.reverse {
            self.source_in + self.source_span() - self.advance_extended(local)
        } else {
            self.source_in + self.advance_extended(local)
        }
    }

    /// Like `advance`, but past the end of the clip the last speed carries on.
    fn advance_extended(&self, local: Micros) -> Micros {
        if local <= self.duration {
            self.advance(local)
        } else {
            self.source_span()
                + (self.speed_at(self.duration) * (local - self.duration) as f64).round() as Micros
        }
    }

    /// The longest clip (in timeline microseconds) whose media use does not exceed `span` microseconds of media.
    pub fn duration_for_span(&self, span: Micros) -> Micros {
        if span <= 0 {
            return 0;
        }
        if !self.speed.is_animated() {
            let s = self.speed_at(0);
            return if s <= 0.0 {
                Micros::MAX / 4
            } else {
                (span as f64 / s).floor() as Micros
            };
        }
        let (mut lo, mut hi) = (0, self.duration.max(1));
        // grow the search range until it covers the span (or give up on very slow clips)
        while self.advance(hi) < span && hi < Micros::MAX / 8 {
            hi *= 2;
        }
        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            if self.advance(mid) <= span {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }

    /// How far (in timeline microseconds) the start of the clip can move earlier before the media runs out.
    /// `media_len` is the length of the file (`None` for pictures and text, which never run out).
    pub fn room_before(&self, media_len: Option<Micros>) -> Micros {
        let s = self.speed_at(0);
        if s <= 0.0 {
            return Micros::MAX / 4;
        }
        let media = if self.reverse {
            match media_len {
                Some(len) => len - (self.source_in + self.source_span()),
                None => return Micros::MAX / 4,
            }
        } else {
            self.source_in
        };
        ((media.max(0)) as f64 / s).floor() as Micros
    }

    /// The longest this clip can be (timeline microseconds) without using media that is not there.
    pub fn max_duration(&self, media_len: Option<Micros>) -> Micros {
        if self.reverse {
            // the part of the file below the clip's top end
            self.duration_for_span(self.source_in + self.source_span())
        } else {
            match media_len {
                Some(len) => self.duration_for_span(len - self.source_in),
                None => Micros::MAX / 4,
            }
        }
    }

    /// This clip with `delta` microseconds taken off its start (positive) or put back (negative): its length, the media it
    /// uses and its keyframes follow. The clip keeps its `start`; the caller moves it if needed.
    pub fn trimmed_left(
        &self,
        delta: Micros,
        media_len: Option<Micros>,
    ) -> Result<Self, crate::edit::EditError> {
        use crate::edit::EditError;
        let mut c = self.clone();
        if self.media().is_some() {
            let passed = self.advance(delta);
            if self.reverse {
                // the top end of the used part of the file moves; the bottom stays
                if media_len.is_some_and(|len| self.source_in + self.source_span() - passed > len) {
                    return Err(EditError::BeyondMedia);
                }
            } else {
                if self.source_in + passed < 0 {
                    return Err(EditError::BeyondMedia);
                }
                c.source_in += passed;
            }
        }
        c.duration -= delta;
        c.shift_keys(-delta);
        Ok(c)
    }

    /// This clip with a new length (the end moves): the media it uses follows.
    pub fn with_duration(
        &self,
        new_duration: Micros,
        media_len: Option<Micros>,
    ) -> Result<Self, crate::edit::EditError> {
        use crate::edit::EditError;
        let mut c = self.clone();
        if self.media().is_some() {
            let span = self.advance(new_duration);
            if self.reverse {
                let new_in = self.source_in + self.source_span() - span;
                if new_in < 0 {
                    return Err(EditError::BeyondMedia);
                }
                c.source_in = new_in;
            } else if media_len.is_some_and(|len| self.source_in + span > len) {
                return Err(EditError::BeyondMedia);
            }
        }
        c.duration = new_duration;
        Ok(c)
    }

    /// Where the two halves of this clip, cut `local` microseconds in, take their media from: `(left_in, right_in)`.
    pub fn split_sources(&self, local: Micros) -> (Micros, Micros) {
        let passed = self.advance(local);
        if self.reverse {
            // the left half plays the top of the range, the right half the rest below it
            (self.source_in + self.source_span() - passed, self.source_in)
        } else {
            (self.source_in, self.source_in + passed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::time::US_PER_SEC;
    use vz_anim::{Animatable, Easing, Keyframe};

    fn clip(seconds: i64) -> Clip {
        Clip::media_clip(MediaId::new(), 0, 2 * US_PER_SEC, seconds * US_PER_SEC)
    }

    fn keyed(points: &[(i64, f32)]) -> Animatable<f32> {
        Animatable::Keyframed(
            points
                .iter()
                .map(|(t, v)| Keyframe {
                    time: *t,
                    value: *v,
                    easing: Easing::Linear,
                })
                .collect(),
        )
    }

    #[test]
    fn at_normal_speed_media_time_follows_timeline_time() {
        let mut c = clip(4);
        c.start = 10 * US_PER_SEC;
        assert_eq!(c.source_time(10 * US_PER_SEC), 2 * US_PER_SEC);
        assert_eq!(c.source_time(11 * US_PER_SEC), 3 * US_PER_SEC);
        assert_eq!(c.source_span(), 4 * US_PER_SEC);
        assert!(!c.is_retimed());
    }

    #[test]
    fn double_speed_eats_media_twice_as_fast() {
        let mut c = clip(4);
        c.speed = 2.0.into();
        assert_eq!(c.source_span(), 8 * US_PER_SEC);
        assert_eq!(c.source_time(US_PER_SEC), 4 * US_PER_SEC);
        c.speed = 0.5.into();
        assert_eq!(c.source_span(), 2 * US_PER_SEC);
        assert!(c.is_retimed());
    }

    #[test]
    fn a_reversed_clip_plays_the_same_part_backwards() {
        let mut c = clip(4);
        c.reverse = true;
        // the part of the file is 2 s .. 6 s; the clip starts at its end
        assert_eq!(c.source_time(0), 6 * US_PER_SEC);
        assert_eq!(c.source_time(US_PER_SEC), 5 * US_PER_SEC);
        assert_eq!(c.source_time(4 * US_PER_SEC), 2 * US_PER_SEC);
        c.speed = 2.0.into();
        assert_eq!(c.source_time(0), 10 * US_PER_SEC);
        assert_eq!(c.source_time(US_PER_SEC), 8 * US_PER_SEC);
    }

    #[test]
    fn a_speed_curve_is_added_up_piece_by_piece() {
        // speed rises from 1 to 3 over two seconds: the media that passes is the average (2) times 2 s
        let mut c = clip(2);
        c.speed = keyed(&[(0, 1.0), (2 * US_PER_SEC, 3.0)]);
        let span = c.source_span();
        assert!((span - 4 * US_PER_SEC).abs() < 1_000, "{span}");
        // halfway through time, less than half of the media has gone by
        let half = c.advance(US_PER_SEC);
        assert!((half - 1_500_000).abs() < 1_000, "{half}");
        // and the clock never runs backwards
        let mut last = -1;
        for i in 0..40 {
            let now = c.advance(i * 50_000);
            assert!(now >= last);
            last = now;
        }
    }

    #[test]
    fn a_freeze_shows_one_moment_forever() {
        let mut c = clip(3);
        c.speed = 0.0.into();
        assert_eq!(c.source_span(), 0);
        assert_eq!(c.source_time(0), 2 * US_PER_SEC);
        assert_eq!(c.source_time(2 * US_PER_SEC), 2 * US_PER_SEC);
        assert!(
            c.max_duration(Some(10 * US_PER_SEC)) > 1_000 * US_PER_SEC,
            "a freeze can be any length"
        );
    }

    #[test]
    fn past_the_ends_the_speed_carries_on_for_transitions() {
        let mut c = clip(4);
        c.speed = 2.0.into();
        assert_eq!(
            c.source_time(-US_PER_SEC),
            0,
            "one second before the start at double speed is 2 s earlier"
        );
        assert_eq!(
            c.source_time(5 * US_PER_SEC),
            2 * US_PER_SEC + 10 * US_PER_SEC
        );
    }

    #[test]
    fn the_longest_clip_for_a_given_amount_of_media() {
        let mut c = clip(1);
        assert_eq!(c.duration_for_span(6 * US_PER_SEC), 6 * US_PER_SEC);
        c.speed = 2.0.into();
        assert_eq!(c.duration_for_span(6 * US_PER_SEC), 3 * US_PER_SEC);
        c.speed = keyed(&[(0, 1.0), (US_PER_SEC, 3.0)]);
        let d = c.duration_for_span(4 * US_PER_SEC);
        assert!((c.advance(d) - 4 * US_PER_SEC).abs() < 2_000, "{d}");
        assert!(c.advance(d + 10_000) > 4 * US_PER_SEC);
    }

    #[test]
    fn how_far_a_clip_can_grow_at_each_end() {
        let file = 10 * US_PER_SEC;
        let mut c = clip(3); // uses 2 s .. 5 s of a 10 s file
        assert_eq!(c.room_before(Some(file)), 2 * US_PER_SEC);
        assert_eq!(c.max_duration(Some(file)), 8 * US_PER_SEC);
        c.speed = 2.0.into(); // uses 2 s .. 8 s
        assert_eq!(c.room_before(Some(file)), US_PER_SEC);
        assert_eq!(c.max_duration(Some(file)), 4 * US_PER_SEC);
        c.reverse = true; // the same part, backwards: room "before" is above 8 s, "after" is below 2 s
        assert_eq!(c.room_before(Some(file)), US_PER_SEC);
        assert_eq!(
            c.max_duration(Some(file)),
            4 * US_PER_SEC,
            "8 s of media below the top end, at double speed"
        );
        assert_eq!(
            c.max_duration(None),
            4 * US_PER_SEC,
            "reversed: only what is below the top end"
        );
    }

    #[test]
    fn cutting_gives_each_half_the_right_part_of_the_file() {
        let mut c = clip(4); // 2 s .. 6 s
        let (l, r) = c.split_sources(US_PER_SEC);
        assert_eq!((l, r), (2 * US_PER_SEC, 3 * US_PER_SEC));
        c.reverse = true;
        // left half plays 6 s down to 5 s: its part starts at 5 s; the right half plays 5 s down to 2 s
        let (l, r) = c.split_sources(US_PER_SEC);
        assert_eq!((l, r), (5 * US_PER_SEC, 2 * US_PER_SEC));
        c.reverse = false;
        c.speed = 2.0.into();
        let (l, r) = c.split_sources(US_PER_SEC);
        assert_eq!((l, r), (2 * US_PER_SEC, 4 * US_PER_SEC));
    }
}
