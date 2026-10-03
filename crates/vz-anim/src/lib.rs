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

/// The families of ready-made curves (each comes as ease in, ease out or both).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    Sine,
    Quad,
    Cubic,
    Back,
    Elastic,
    Bounce,
}

impl Family {
    pub const ALL: [Self; 6] = [
        Self::Sine,
        Self::Quad,
        Self::Cubic,
        Self::Back,
        Self::Elastic,
        Self::Bounce,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Quad => "Quad",
            Self::Cubic => "Cubic",
            Self::Back => "Back",
            Self::Elastic => "Elastic",
            Self::Bounce => "Bounce",
        }
    }

    /// The "ease in" shape of the family (slow start).
    fn ease_in(self, p: f32) -> f32 {
        use std::f32::consts::PI;
        match self {
            Self::Sine => 1.0 - (p * PI / 2.0).cos(),
            Self::Quad => p * p,
            Self::Cubic => p * p * p,
            Self::Back => {
                let c = 1.70158;
                (c + 1.0) * p * p * p - c * p * p
            }
            Self::Elastic => {
                if p <= 0.0 || p >= 1.0 {
                    p
                } else {
                    -(2.0f32.powf(10.0 * p - 10.0)) * ((p * 10.0 - 10.75) * (2.0 * PI / 3.0)).sin()
                }
            }
            Self::Bounce => 1.0 - Self::bounce_out(1.0 - p),
        }
    }

    fn bounce_out(p: f32) -> f32 {
        let (n, d) = (7.5625, 2.75);
        if p < 1.0 / d {
            n * p * p
        } else if p < 2.0 / d {
            let p = p - 1.5 / d;
            n * p * p + 0.75
        } else if p < 2.5 / d {
            let p = p - 2.25 / d;
            n * p * p + 0.9375
        } else {
            let p = p - 2.625 / d;
            n * p * p + 0.984_375
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    In,
    Out,
    InOut,
}

impl Dir {
    pub fn label(self) -> &'static str {
        match self {
            Self::In => "ease in",
            Self::Out => "ease out",
            Self::InOut => "ease in-out",
        }
    }
}

/// How the value moves from one keyframe to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    #[default]
    Linear,
    /// Jump: keep the old value until the next keyframe.
    Hold,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// A ready-made curve such as "Bounce, ease out".
    Named(Family, Dir),
    /// A custom curve drawn with two handles, like CSS `cubic-bezier(x1, y1, x2, y2)`.
    Bezier(f32, f32, f32, f32),
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
            Self::Named(family, Dir::In) => family.ease_in(p),
            Self::Named(family, Dir::Out) => 1.0 - family.ease_in(1.0 - p),
            Self::Named(family, Dir::InOut) => {
                if p < 0.5 {
                    family.ease_in(p * 2.0) / 2.0
                } else {
                    1.0 - family.ease_in((1.0 - p) * 2.0) / 2.0
                }
            }
            Self::Bezier(x1, y1, x2, y2) => bezier(x1, y1, x2, y2, p),
        }
    }

    /// A short name for menus.
    pub fn label(self) -> String {
        match self {
            Self::Linear => "Linear".into(),
            Self::Hold => "Hold (jump)".into(),
            Self::EaseIn => "Ease in".into(),
            Self::EaseOut => "Ease out".into(),
            Self::EaseInOut => "Ease in-out".into(),
            Self::Named(f, d) => format!("{} {}", f.label(), d.label()),
            Self::Bezier(..) => "Custom curve".into(),
        }
    }

    /// The curves to offer in a menu.
    pub fn presets() -> Vec<Self> {
        let mut v = vec![
            Self::Linear,
            Self::Hold,
            Self::EaseIn,
            Self::EaseOut,
            Self::EaseInOut,
        ];
        for f in Family::ALL {
            for d in [Dir::In, Dir::Out, Dir::InOut] {
                v.push(Self::Named(f, d));
            }
        }
        v
    }
}

/// The y of a CSS-style cubic bezier curve at horizontal position `x` (0 to 1).
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let at = |t: f32, a: f32, b: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    // find t with at(t, x1, x2) == x by bisection (the x curve rises steadily for x1, x2 in 0..=1)
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if at(mid, x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0)) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at((lo + hi) / 2.0, y1, y2)
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

    /// All keyframes (empty for a fixed value).
    pub fn keys(&self) -> &[Keyframe<T>] {
        match self {
            Self::Static(_) => &[],
            Self::Keyframed(k) => k,
        }
    }

    /// Is there a keyframe at `t` (give or take `tolerance` microseconds)?
    pub fn key_at(&self, t: i64, tolerance: i64) -> Option<usize> {
        self.keys()
            .iter()
            .position(|k| (k.time - t).abs() <= tolerance)
    }

    /// Put a keyframe with `value` at `t` (replacing one that is already there). A fixed value becomes animated.
    pub fn set_key(&mut self, t: i64, value: T, tolerance: i64) {
        let t = t.max(0);
        match self {
            Self::Static(_) => {
                *self = Self::Keyframed(vec![Keyframe {
                    time: t,
                    value,
                    easing: Easing::default(),
                }]);
            }
            Self::Keyframed(keys) => {
                match keys.iter().position(|k| (k.time - t).abs() <= tolerance) {
                    Some(i) => keys[i].value = value,
                    None => {
                        let at = keys.partition_point(|k| k.time < t);
                        keys.insert(
                            at,
                            Keyframe {
                                time: t,
                                value,
                                easing: Easing::default(),
                            },
                        );
                    }
                }
            }
        }
    }

    /// Take away the keyframe at `t`. When the last one goes, the property keeps the value it had there as a fixed value.
    pub fn remove_key(&mut self, t: i64, tolerance: i64)
    where
        T: Default,
    {
        let held = self.value_at(t);
        if let Self::Keyframed(keys) = self {
            if let Some(i) = keys.iter().position(|k| (k.time - t).abs() <= tolerance) {
                keys.remove(i);
            }
            if keys.is_empty() {
                *self = Self::Static(held);
            }
        }
    }

    /// Stop animating: keep the value at `t` as a fixed value.
    pub fn freeze_at(&mut self, t: i64)
    where
        T: Default,
    {
        *self = Self::Static(self.value_at(t));
    }

    /// The time of the closest keyframe before `t`.
    pub fn prev_key(&self, t: i64, tolerance: i64) -> Option<i64> {
        self.keys()
            .iter()
            .rev()
            .map(|k| k.time)
            .find(|k| *k < t - tolerance)
    }

    /// The time of the closest keyframe after `t`.
    pub fn next_key(&self, t: i64, tolerance: i64) -> Option<i64> {
        self.keys()
            .iter()
            .map(|k| k.time)
            .find(|k| *k > t + tolerance)
    }

    /// Change how the value moves from the keyframe at index `i` to the next one.
    pub fn set_easing(&mut self, i: usize, easing: Easing) {
        if let Self::Keyframed(keys) = self
            && let Some(k) = keys.get_mut(i)
        {
            k.easing = easing;
        }
    }

    /// Move keyframe `i` to time `t`, keeping the list in order.
    pub fn move_key(&mut self, i: usize, t: i64) {
        if let Self::Keyframed(keys) = self
            && i < keys.len()
        {
            let mut k = keys.remove(i);
            k.time = t.max(0);
            let at = keys.partition_point(|o| o.time < k.time);
            keys.insert(at, k);
        }
    }

    /// Add `delta` microseconds to every keyframe time (when the start of a clip changes). Keyframes pushed before
    /// time 0 are dropped, and a keyframe at 0 keeps the value the animation had there.
    pub fn shift(&mut self, delta: i64)
    where
        T: Default,
    {
        if delta == 0 {
            return;
        }
        let start_value = self.value_at(-delta);
        if let Self::Keyframed(keys) = self {
            let easing_before = keys
                .iter()
                .rev()
                .find(|k| k.time + delta < 0)
                .map(|k| k.easing);
            let had_dropped = easing_before.is_some();
            for k in keys.iter_mut() {
                k.time += delta;
            }
            keys.retain(|k| k.time >= 0);
            if had_dropped && keys.first().is_none_or(|k| k.time > 0) {
                keys.insert(
                    0,
                    Keyframe {
                        time: 0,
                        value: start_value,
                        easing: easing_before.unwrap_or_default(),
                    },
                );
            }
            if keys.len() <= 1 && had_dropped {
                *self = Self::Static(start_value);
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

    #[test]
    fn every_named_curve_starts_at_zero_and_ends_at_one() {
        for e in Easing::presets() {
            if e == Easing::Hold {
                continue;
            }
            assert!(e.apply(0.0).abs() < 1e-4, "{e:?} start {}", e.apply(0.0));
            assert!(
                (e.apply(1.0) - 1.0).abs() < 1e-4,
                "{e:?} end {}",
                e.apply(1.0)
            );
        }
    }

    #[test]
    fn the_curves_have_their_characteristic_shapes() {
        assert!(
            Easing::Named(Family::Cubic, Dir::In).apply(0.5)
                < Easing::Named(Family::Quad, Dir::In).apply(0.5)
        );
        // "back" pulls back below zero before going forward, and overshoots past one when easing out
        assert!(Easing::Named(Family::Back, Dir::In).apply(0.2) < 0.0);
        assert!(Easing::Named(Family::Back, Dir::Out).apply(0.8) > 1.0);
        // bounce out touches one several times
        let b = Easing::Named(Family::Bounce, Dir::Out);
        assert!(b.apply(0.5) < 1.0 && b.apply(0.95) > 0.95);
        // in-out curves are symmetric around the middle
        let s = Easing::Named(Family::Sine, Dir::InOut);
        assert!((s.apply(0.5) - 0.5).abs() < 1e-4);
        assert!((s.apply(0.25) + s.apply(0.75) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_bezier_with_straight_handles_is_linear_and_css_ease_is_not() {
        let line = Easing::Bezier(0.33, 0.33, 0.67, 0.67);
        for p in [0.1, 0.4, 0.8] {
            assert!((line.apply(p) - p).abs() < 0.02, "{p}");
        }
        let ease = Easing::Bezier(0.25, 0.1, 0.25, 1.0);
        assert!(ease.apply(0.25) > 0.35);
        assert!((ease.apply(0.0)).abs() < 1e-3 && (ease.apply(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_first_keyframe_turns_a_fixed_value_into_an_animation() {
        let mut a = Animatable::Static(1.0f32);
        a.set_key(500, 4.0, 0);
        assert!(a.key_at(500, 0).is_some());
        assert_eq!(a.value_at(0), 4.0, "one keyframe holds everywhere");
        a.set_key(1500, 8.0, 0);
        assert!(a.is_animated());
        assert_eq!(a.value_at(1000), 6.0);
        // setting at a time that already has a key changes that key
        a.set_key(1500, 10.0, 0);
        assert_eq!(a.keys().len(), 2);
        assert_eq!(a.value_at(2000), 10.0);
    }

    #[test]
    fn keys_stay_in_time_order_and_can_be_moved() {
        let mut a = Animatable::Static(0.0f32);
        for t in [300, 100, 200] {
            a.set_key(t, t as f32, 0);
        }
        let times: Vec<_> = a.keys().iter().map(|k| k.time).collect();
        assert_eq!(times, [100, 200, 300]);
        a.move_key(0, 250);
        let times: Vec<_> = a.keys().iter().map(|k| k.time).collect();
        assert_eq!(times, [200, 250, 300]);
        a.shift(-100);
        assert_eq!(a.keys()[0].time, 100);
        // shifting past the first keys keeps the value at the new start
        let mut b = Animatable::Static(0.0f32);
        b.set_key(0, 0.0, 0);
        b.set_key(1000, 100.0, 0);
        b.set_key(2000, 0.0, 0);
        b.shift(-500);
        assert_eq!(b.keys()[0].time, 0);
        assert_eq!(b.value_at(0), 50.0);
        assert_eq!(b.keys()[1].time, 500);
        b.shift(-5000);
        assert_eq!(b, Animatable::Static(0.0));
    }

    #[test]
    fn removing_the_last_key_keeps_the_value_it_gave() {
        let mut a = Animatable::Static(0.0f32);
        a.set_key(100, 5.0, 0);
        a.remove_key(100, 0);
        assert_eq!(a, Animatable::Static(5.0));
        let mut b = Animatable::Static(0.0f32);
        b.set_key(0, 0.0, 0);
        b.set_key(100, 10.0, 0);
        b.remove_key(0, 0);
        assert_eq!(b.value_at(0), 10.0);
    }

    #[test]
    fn jumping_between_keys_finds_the_neighbours() {
        let mut a = Animatable::Static(0.0f32);
        for t in [100, 200, 300] {
            a.set_key(t, 1.0, 0);
        }
        assert_eq!(a.prev_key(200, 0), Some(100));
        assert_eq!(a.next_key(200, 0), Some(300));
        assert_eq!(a.prev_key(100, 0), None);
        assert_eq!(a.next_key(300, 0), None);
        assert_eq!(a.next_key(150, 0), Some(200));
    }

    #[test]
    fn easing_is_stored_per_keyframe_and_saved() {
        let mut a = Animatable::Static(0.0f32);
        a.set_key(0, 0.0, 0);
        a.set_key(1000, 10.0, 0);
        a.set_easing(0, Easing::Named(Family::Bounce, Dir::Out));
        let back: Animatable<f32> =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
        a.set_easing(0, Easing::Bezier(0.1, 0.2, 0.3, 0.4));
        let back: Animatable<f32> =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
}
