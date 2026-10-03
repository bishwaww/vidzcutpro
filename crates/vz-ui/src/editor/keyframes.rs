//! Keyframes on a clip's properties: which properties can be animated, how a changed value is written
//! (a fixed value, or a keyframe at the playhead when the property is animated) and how to jump between keyframes.

use vz_anim::Animatable;
use vz_core::{Clip, Micros};

/// The properties of a clip that can have keyframes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    X,
    Y,
    ScaleX,
    ScaleY,
    Rotation,
    Opacity,
    Volume,
}

impl Prop {
    pub const TRANSFORM: [Self; 5] = [Self::X, Self::Y, Self::ScaleX, Self::ScaleY, Self::Rotation];

    pub fn get(self, c: &Clip) -> &Animatable<f32> {
        match self {
            Self::X => &c.transform.x,
            Self::Y => &c.transform.y,
            Self::ScaleX => &c.transform.scale_x,
            Self::ScaleY => &c.transform.scale_y,
            Self::Rotation => &c.transform.rotation,
            Self::Opacity => &c.opacity,
            Self::Volume => &c.volume,
        }
    }

    pub fn get_mut(self, c: &mut Clip) -> &mut Animatable<f32> {
        match self {
            Self::X => &mut c.transform.x,
            Self::Y => &mut c.transform.y,
            Self::ScaleX => &mut c.transform.scale_x,
            Self::ScaleY => &mut c.transform.scale_y,
            Self::Rotation => &mut c.transform.rotation,
            Self::Opacity => &mut c.opacity,
            Self::Volume => &mut c.volume,
        }
    }

    /// Used to join the changes of one drag into one undo step.
    pub fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::ScaleX => "scale-x",
            Self::ScaleY => "scale-y",
            Self::Rotation => "rotation",
            Self::Opacity => "opacity",
            Self::Volume => "volume",
        }
    }
}

/// How close (in microseconds) a keyframe must be to the playhead to count as "at the playhead": half a frame.
pub fn tolerance(frame: Micros) -> Micros {
    (frame / 2).max(1)
}

/// The time inside the clip (0 at its start) that the playhead stands at, kept within the clip.
pub fn local_time(clip: &Clip, playhead: Micros) -> Micros {
    (playhead - clip.start).clamp(0, clip.duration)
}

/// Is the playhead on the clip (so a keyframe can be put there)?
pub fn playhead_on(clip: &Clip, playhead: Micros) -> bool {
    playhead >= clip.start && playhead <= clip.end()
}

/// Write `value` into a property at clip-local time `local`: a keyframe if the property is animated, otherwise the fixed value.
pub fn write(clip: &mut Clip, prop: Prop, local: Micros, value: f32, tolerance: Micros) {
    let a = prop.get_mut(clip);
    if a.keys().is_empty() {
        *a = Animatable::Static(value);
    } else {
        a.set_key(local, value, tolerance);
    }
}

/// Is the property animated (has keyframes)?
pub fn animated(clip: &Clip, prop: Prop) -> bool {
    !prop.get(clip).keys().is_empty()
}

/// Is there a keyframe on the property at `local`?
pub fn has_key(clip: &Clip, prop: Prop, local: Micros, tolerance: Micros) -> bool {
    prop.get(clip).key_at(local, tolerance).is_some()
}

/// Put a keyframe (holding the current value) on each property at `local`, or take them away if all have one there.
pub fn toggle_key(clip: &mut Clip, props: &[Prop], local: Micros, tolerance: Micros) {
    let all_keyed = props.iter().all(|p| has_key(clip, *p, local, tolerance));
    for p in props {
        let a = p.get_mut(clip);
        if all_keyed {
            a.remove_key(local, tolerance);
        } else {
            let v = a.value_at(local);
            a.set_key(local, v, tolerance);
        }
    }
}

/// Stop animating the properties: each keeps its value at `local`.
pub fn clear_animation(clip: &mut Clip, props: &[Prop], local: Micros) {
    for p in props {
        p.get_mut(clip).freeze_at(local);
    }
}

/// All keyframe times of the listed properties, sorted, without repeats (within `tolerance`).
pub fn key_times(clip: &Clip, props: &[Prop], tolerance: Micros) -> Vec<Micros> {
    let mut times: Vec<Micros> = props
        .iter()
        .flat_map(|p| p.get(clip).keys().iter().map(|k| k.time))
        .collect();
    times.sort_unstable();
    times.dedup_by(|a, b| (*a - *b).abs() <= tolerance);
    times
}

/// The nearest keyframe time before (`forward == false`) or after the playhead's clip-local time.
pub fn neighbour(
    clip: &Clip,
    props: &[Prop],
    local: Micros,
    tolerance: Micros,
    forward: bool,
) -> Option<Micros> {
    let times = key_times(clip, props, tolerance);
    if forward {
        times.into_iter().find(|t| *t > local + tolerance)
    } else {
        times.into_iter().rev().find(|t| *t < local - tolerance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> Clip {
        Clip::text_clip(vz_core::TextContent::new("t"), 1_000_000, 4_000_000)
    }

    #[test]
    fn a_fixed_property_is_just_changed() {
        let mut c = clip();
        write(&mut c, Prop::X, 500_000, 40.0, 10);
        assert_eq!(c.transform.x, Animatable::Static(40.0));
    }

    #[test]
    fn an_animated_property_gets_a_keyframe_where_the_playhead_is() {
        let mut c = clip();
        toggle_key(&mut c, &[Prop::X], 0, 10);
        write(&mut c, Prop::X, 2_000_000, 90.0, 10);
        assert_eq!(c.transform.x.keys().len(), 2);
        assert_eq!(
            c.transform.x.value_at(1_000_000),
            45.0,
            "halfway between the two keys"
        );
        // changing again at the same moment changes that keyframe instead of adding one
        write(&mut c, Prop::X, 2_000_005, 100.0, 10);
        assert_eq!(c.transform.x.keys().len(), 2);
    }

    #[test]
    fn the_diamond_adds_then_removes_and_keeps_the_value() {
        let mut c = clip();
        c.opacity = 0.5.into();
        toggle_key(&mut c, &[Prop::Opacity], 1_000_000, 10);
        assert!(has_key(&c, Prop::Opacity, 1_000_000, 10));
        assert_eq!(c.opacity.value_at(0), 0.5);
        toggle_key(&mut c, &[Prop::Opacity], 1_000_000, 10);
        assert_eq!(c.opacity, Animatable::Static(0.5));
    }

    #[test]
    fn one_diamond_can_key_width_and_height_together() {
        let mut c = clip();
        toggle_key(&mut c, &[Prop::ScaleX, Prop::ScaleY], 0, 10);
        assert!(animated(&c, Prop::ScaleX) && animated(&c, Prop::ScaleY));
        clear_animation(&mut c, &[Prop::ScaleX, Prop::ScaleY], 0);
        assert!(!animated(&c, Prop::ScaleX));
    }

    #[test]
    fn jumping_goes_to_the_neighbouring_keyframe_of_any_listed_property() {
        let mut c = clip();
        toggle_key(&mut c, &[Prop::X], 1_000_000, 10);
        toggle_key(&mut c, &[Prop::Rotation], 3_000_000, 10);
        let props = Prop::TRANSFORM;
        assert_eq!(neighbour(&c, &props, 0, 10, true), Some(1_000_000));
        assert_eq!(neighbour(&c, &props, 1_000_000, 10, true), Some(3_000_000));
        assert_eq!(neighbour(&c, &props, 3_000_000, 10, false), Some(1_000_000));
        assert_eq!(neighbour(&c, &props, 3_000_000, 10, true), None);
    }

    #[test]
    fn local_time_stays_inside_the_clip() {
        let c = clip();
        assert_eq!(local_time(&c, 0), 0);
        assert_eq!(local_time(&c, 3_000_000), 2_000_000);
        assert_eq!(local_time(&c, 99_000_000), 4_000_000);
        assert!(playhead_on(&c, 5_000_000));
        assert!(!playhead_on(&c, 6_000_001));
    }
}
