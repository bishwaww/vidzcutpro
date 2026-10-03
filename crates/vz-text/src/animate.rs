//! Text animators: what each letter, word or line is doing at a moment.

use vz_core::{Micros, TextAnimator, TextPhase, TextUnit};

/// How a unit of text is moved, sized and faded right now (relative to where it normally is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitState {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
}

impl Default for UnitState {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
        }
    }
}

impl UnitState {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    fn away(an: &TextAnimator, amount: f32) -> Self {
        // `amount` is how far from normal: 0 normal, 1 fully away
        Self {
            dx: an.dx * amount,
            dy: an.dy * amount,
            scale: 1.0 + (an.scale - 1.0) * amount,
            rotation: an.rotation * amount,
            opacity: 1.0 + (an.opacity - 1.0) * amount,
        }
    }

    fn combine(self, o: Self) -> Self {
        Self {
            dx: self.dx + o.dx,
            dy: self.dy + o.dy,
            scale: self.scale * o.scale,
            rotation: self.rotation + o.rotation,
            opacity: self.opacity * o.opacity,
        }
    }
}

/// How many seconds a whole animator takes for `count` units: the last starts after all the delays.
pub fn total_seconds(an: &TextAnimator, count: usize) -> f32 {
    an.delay * count.saturating_sub(1) as f32 + an.duration
}

/// The state of unit number `index` (of `count` units of the animator's kind) at `local` microseconds into a clip that
/// lasts `duration`. Sizes (`dx`, `dy`) are in reference pixels.
pub fn state(
    an: &TextAnimator,
    index: usize,
    count: usize,
    local: Micros,
    duration: Micros,
) -> UnitState {
    let t = local as f32 / 1_000_000.0;
    let dur = an.duration.max(0.001);
    let amount = match an.phase {
        TextPhase::In => {
            let p = ((t - index as f32 * an.delay) / dur).clamp(0.0, 1.0);
            1.0 - an.easing.apply(p)
        }
        TextPhase::Out => {
            let begins = duration as f32 / 1_000_000.0 - total_seconds(an, count);
            let p = ((t - begins - index as f32 * an.delay) / dur).clamp(0.0, 1.0);
            an.easing.apply(p)
        }
        TextPhase::Loop => {
            let phase = t / dur - index as f32 * an.delay / dur;
            0.5 - 0.5 * (std::f32::consts::TAU * phase).cos()
        }
    };
    UnitState::away(an, amount)
}

/// All animators together for one unit. `indices` and `counts` give the unit's place for each kind of unit
/// (character, word, line).
pub fn combined(
    animators: &[TextAnimator],
    indices: [usize; 3],
    counts: [usize; 3],
    local: Micros,
    duration: Micros,
) -> UnitState {
    animators.iter().fold(UnitState::default(), |acc, an| {
        let k = match an.unit {
            TextUnit::Character => 0,
            TextUnit::Word => 1,
            TextUnit::Line => 2,
        };
        acc.combine(state(an, indices[k], counts[k], local, duration))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_anim::Easing;
    use vz_core::US_PER_SEC;

    fn fly_in() -> TextAnimator {
        TextAnimator {
            dy: 100.0,
            opacity: 0.0,
            delay: 0.1,
            duration: 0.5,
            easing: Easing::Linear,
            ..TextAnimator::default()
        }
    }

    #[test]
    fn an_in_animator_starts_away_and_ends_normal_one_unit_after_another() {
        let a = fly_in();
        let s = |i: usize, ms: i64| state(&a, i, 5, ms * 1000, 5 * US_PER_SEC);
        assert_eq!(s(0, 0).dy, 100.0);
        assert_eq!(s(0, 0).opacity, 0.0);
        assert!((s(0, 250).dy - 50.0).abs() < 1e-3);
        assert!(s(0, 500).is_neutral());
        // the third unit has not started at 150 ms (it starts at 200 ms)
        assert_eq!(s(2, 150).dy, 100.0);
        assert!(s(2, 700).is_neutral());
        assert!(
            s(4, 900).dy > 0.0,
            "the last one is still on its way at 900 ms"
        );
        assert!(s(4, 900 + 100).is_neutral());
    }

    #[test]
    fn an_out_animator_plays_at_the_end_of_the_clip() {
        let a = TextAnimator {
            phase: TextPhase::Out,
            ..fly_in()
        };
        let s = |i: usize, ms: i64| state(&a, i, 3, ms * 1000, 3 * US_PER_SEC);
        assert!(s(0, 1000).is_neutral(), "nothing happens in the middle");
        // 3 units: the whole thing takes 0.1*2 + 0.5 = 0.7 s, so it starts at 2.3 s
        assert!(s(0, 2300).is_neutral());
        assert!(s(0, 2800).dy > 90.0 || s(0, 2800).dy > 0.0);
        assert!(
            (s(0, 3000).dy - 100.0).abs() < 1e-3,
            "the first unit is fully away at the end"
        );
        assert_eq!(s(2, 3000).dy, 100.0);
    }

    #[test]
    fn a_loop_animator_repeats() {
        let a = TextAnimator {
            phase: TextPhase::Loop,
            delay: 0.0,
            duration: 1.0,
            ..fly_in()
        };
        let s = |ms: i64| state(&a, 0, 1, ms * 1000, 10 * US_PER_SEC);
        assert!(s(0).is_neutral());
        assert!((s(500).dy - 100.0).abs() < 1e-3);
        assert!((s(1000).dy).abs() < 1e-3);
        assert!((s(2500).dy - 100.0).abs() < 1e-3);
        // a delay shifts the units against each other
        let delayed = TextAnimator { delay: 0.25, ..a };
        assert!(state(&delayed, 1, 4, 0, 10 * US_PER_SEC).dy > 40.0);
    }

    #[test]
    fn animators_of_different_kinds_add_up_for_one_unit() {
        let by_letter = fly_in();
        let by_line = TextAnimator {
            unit: TextUnit::Line,
            dy: 0.0,
            dx: 60.0,
            opacity: 1.0,
            ..fly_in()
        };
        let s = combined(
            &[by_letter, by_line],
            [0, 0, 0],
            [5, 2, 1],
            0,
            5 * US_PER_SEC,
        );
        assert_eq!((s.dx, s.dy, s.opacity), (60.0, 100.0, 0.0));
        let none = combined(&[], [0; 3], [1; 3], 0, US_PER_SEC);
        assert!(none.is_neutral());
    }

    #[test]
    fn scale_and_turn_come_back_to_normal() {
        let a = TextAnimator {
            scale: 0.0,
            rotation: -90.0,
            dy: 0.0,
            opacity: 1.0,
            easing: Easing::Linear,
            ..fly_in()
        };
        let start = state(&a, 0, 1, 0, 5 * US_PER_SEC);
        assert_eq!((start.scale, start.rotation), (0.0, -90.0));
        assert!(state(&a, 0, 1, 1_000_000, 5 * US_PER_SEC).is_neutral());
    }
}
