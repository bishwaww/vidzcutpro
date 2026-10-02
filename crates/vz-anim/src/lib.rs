//! Keyframes, easing curves and interpolation. Pure maths, no window and no files.
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md.
//!
//! Every property that can be animated (position, opacity, volume, …) is an [`Animatable`]: either
//! one fixed value or a list of keyframes. Phase 2 only uses fixed values; Phase 3 adds the keyframe
//! editor, bezier easing and presets on top of this same type, so saved projects do not need converting.

use serde::{Deserialize, Serialize};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-anim";

/// How the value moves from one keyframe to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Easing {
    #[default]
    Linear,
    /// Jump: keep the old value until the next keyframe.
    Hold,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Map progress `p` (0 to 1) through the curve.
    pub fn apply(self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Self::Linear => p,
            Self::Hold => 0.0,
            Self::EaseIn => p * p,
            Self::EaseOut => 1.0 - (1.0 - p) * (1.0 - p),
            Self::EaseInOut => p * p * (3.0 - 2.0 * p),
        }
    }
}

/// A value that can be blended between two keyframes.
pub trait Lerp: Copy {
    fn lerp(a: Self, b: Self, f: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: Self, b: Self, f: f32) -> Self {
        a + (b - a) * f
    }
}

/// One keyframe: the value at a moment, measured from the start of the clip, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub time: i64,
    pub value: T,
    #[serde(default)]
    pub easing: Easing,
}

/// A value that is either fixed or moves between keyframes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Animatable<T> {
    Static(T),
    Keyframed(Vec<Keyframe<T>>),
}

impl<T: Default> Default for Animatable<T> {
    fn default() -> Self {
        Self::Static(T::default())
    }
}

impl<T: Lerp> Animatable<T> {
    /// The fixed value (or the first keyframe's value when animated). For editing screens that show one number.
    pub fn first(&self) -> Option<T> {
        match self {
            Self::Static(v) => Some(*v),
            Self::Keyframed(k) => k.first().map(|k| k.value),
        }
    }

    /// The value `t` microseconds after the clip's start.
    pub fn value_at(&self, t: i64) -> T
    where
        T: Default,
    {
        match self {
            Self::Static(v) => *v,
            Self::Keyframed(keys) => {
                let Some(first) = keys.first() else {
                    return T::default();
                };
                if t <= first.time || keys.len() == 1 {
                    return first.value;
                }
                let last = keys[keys.len() - 1];
                if t >= last.time {
                    return last.value;
                }
                let next = keys.partition_point(|k| k.time <= t);
                let (a, b) = (keys[next - 1], keys[next]);
                let span = (b.time - a.time).max(1) as f32;
                let progress = a.easing.apply((t - a.time) as f32 / span);
                T::lerp(a.value, b.value, progress)
            }
        }
    }

    pub fn is_animated(&self) -> bool {
        matches!(self, Self::Keyframed(k) if k.len() > 1)
    }
}

impl<T> From<T> for Animatable<T> {
    fn from(v: T) -> Self {
        Self::Static(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(time: i64, value: f32, easing: Easing) -> Keyframe<f32> {
        Keyframe {
            time,
            value,
            easing,
        }
    }

    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-anim");
    }

    #[test]
    fn a_fixed_value_is_the_same_everywhere() {
        let a = Animatable::Static(3.0f32);
        assert_eq!(a.value_at(-5), 3.0);
        assert_eq!(a.value_at(1_000_000), 3.0);
        assert!(!a.is_animated());
    }

    #[test]
    fn linear_keyframes_blend_and_clamp_at_the_ends() {
        let a = Animatable::Keyframed(vec![
            k(0, 0.0, Easing::Linear),
            k(1_000_000, 10.0, Easing::Linear),
        ]);
        assert_eq!(a.value_at(-1), 0.0);
        assert_eq!(a.value_at(500_000), 5.0);
        assert_eq!(a.value_at(2_000_000), 10.0);
        assert!(a.is_animated());
    }

    #[test]
    fn hold_keeps_the_old_value_until_the_next_keyframe() {
        let a = Animatable::Keyframed(vec![k(0, 1.0, Easing::Hold), k(1_000, 2.0, Easing::Linear)]);
        assert_eq!(a.value_at(999), 1.0);
        assert_eq!(a.value_at(1_000), 2.0);
    }

    #[test]
    fn easing_curves_start_and_end_in_the_right_place() {
        for e in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-6, "{e:?}");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6, "{e:?}");
        }
        assert!(Easing::EaseIn.apply(0.5) < 0.5);
        assert!(Easing::EaseOut.apply(0.5) > 0.5);
    }

    #[test]
    fn three_keyframes_pick_the_right_segment() {
        let a = Animatable::Keyframed(vec![
            k(0, 0.0, Easing::Linear),
            k(100, 10.0, Easing::Linear),
            k(200, 0.0, Easing::Linear),
        ]);
        assert_eq!(a.value_at(50), 5.0);
        assert_eq!(a.value_at(150), 5.0);
        assert_eq!(a.value_at(100), 10.0);
    }

    #[test]
    fn saves_as_a_plain_number_or_a_list() {
        let fixed: Animatable<f32> = Animatable::Static(0.5);
        assert_eq!(serde_json::to_string(&fixed).unwrap(), "0.5");
        let back: Animatable<f32> = serde_json::from_str("0.5").unwrap();
        assert_eq!(back, fixed);
        let keyed = Animatable::Keyframed(vec![k(0, 1.0, Easing::Linear)]);
        let back: Animatable<f32> =
            serde_json::from_str(&serde_json::to_string(&keyed).unwrap()).unwrap();
        assert_eq!(back, keyed);
    }
}
