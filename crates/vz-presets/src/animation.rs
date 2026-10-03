//! Ready-made animations for clips: In (when it appears), Out (when it leaves) and Combo (a loop while it is shown).
//!
//! Each is plain data: a list of channels, each moving one property (offset, size, turn, fade, blur, wipe) along a curve.
//! They are added on top of whatever the clip already does with its own keyframes.

use std::sync::OnceLock;
use vz_anim::{Dir, Easing, Family};
use vz_core::{Clip, Micros, US_PER_SEC};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimKind {
    In,
    Out,
    Combo,
}

/// What a channel moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimProp {
    /// Sideways and up-down offset as a share of the canvas width / height (0 = where the clip is).
    OffsetX,
    OffsetY,
    /// Size multiplier (1 = unchanged), both ways, or across only.
    Scale,
    ScaleX,
    /// Extra turn in degrees.
    Rotation,
    /// Opacity multiplier (1 = unchanged).
    Opacity,
    /// Blur in reference pixels (0 = sharp).
    Blur,
    /// How much of that side is hidden, 0 to 1 (a wipe).
    HideLeft,
    HideRight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// From one value to another along a curve.
    Tween { from: f32, to: f32, easing: Easing },
    /// A swing around the neutral value: `amplitude * sin(2π · cycles · p)`.
    Wave { amplitude: f32, cycles: f32 },
    /// A swing that dies away: `amplitude * (1 - p) * sin(2π · cycles · p)`.
    Decay { amplitude: f32, cycles: f32 },
    /// The same, growing instead of dying away.
    Grow { amplitude: f32, cycles: f32 },
}

impl Motion {
    fn value(self, p: f32) -> f32 {
        let turn = std::f32::consts::TAU;
        match self {
            Self::Tween { from, to, easing } => from + (to - from) * easing.apply(p),
            Self::Wave { amplitude, cycles } => amplitude * (turn * cycles * p).sin(),
            Self::Decay { amplitude, cycles } => amplitude * (1.0 - p) * (turn * cycles * p).sin(),
            Self::Grow { amplitude, cycles } => amplitude * p * (turn * cycles * p).sin(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Channel {
    pub prop: AnimProp,
    pub motion: Motion,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: AnimKind,
    pub channels: Vec<Channel>,
    /// How long it takes (one cycle for a combo) unless the user changes it.
    pub default_duration: Micros,
}

/// What animations do to a clip at one moment, on top of its own transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimState {
    /// Offsets as a share of the canvas width and height.
    pub dx: f32,
    pub dy: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub blur: f32,
    pub hide_left: f32,
    pub hide_right: f32,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            blur: 0.0,
            hide_left: 0.0,
            hide_right: 0.0,
        }
    }
}

impl AnimState {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&mut self, prop: AnimProp, v: f32) {
        match prop {
            AnimProp::OffsetX => self.dx += v,
            AnimProp::OffsetY => self.dy += v,
            AnimProp::Scale => {
                self.scale_x *= v;
                self.scale_y *= v;
            }
            AnimProp::ScaleX => self.scale_x *= v,
            AnimProp::Rotation => self.rotation += v,
            AnimProp::Opacity => self.opacity *= v.clamp(0.0, 1.0),
            AnimProp::Blur => self.blur = self.blur.max(v),
            AnimProp::HideLeft => self.hide_left = self.hide_left.max(v.clamp(0.0, 1.0)),
            AnimProp::HideRight => self.hide_right = self.hide_right.max(v.clamp(0.0, 1.0)),
        }
    }
}

fn ch(prop: AnimProp, motion: Motion) -> Channel {
    Channel { prop, motion }
}

fn tween(prop: AnimProp, from: f32, to: f32, easing: Easing) -> Channel {
    ch(prop, Motion::Tween { from, to, easing })
}

const OUT: Easing = Easing::Named(Family::Cubic, Dir::Out);
const IN: Easing = Easing::Named(Family::Cubic, Dir::In);
const BACK_OUT: Easing = Easing::Named(Family::Back, Dir::Out);
const BACK_IN: Easing = Easing::Named(Family::Back, Dir::In);

fn preset(
    id: &'static str,
    name: &'static str,
    kind: AnimKind,
    seconds: f32,
    channels: Vec<Channel>,
) -> AnimPreset {
    AnimPreset {
        id,
        name,
        kind,
        channels,
        default_duration: (f64::from(seconds) * US_PER_SEC as f64) as Micros,
    }
}

fn build() -> Vec<AnimPreset> {
    use AnimKind::{Combo, In, Out as OutKind};
    use AnimProp::{Blur, HideLeft, HideRight, OffsetX, OffsetY, Opacity, Rotation, Scale, ScaleX};
    let lin = Easing::Linear;
    vec![
        // ---- In
        preset(
            "fade_in",
            "Fade in",
            In,
            0.6,
            vec![tween(Opacity, 0.0, 1.0, lin)],
        ),
        preset(
            "zoom_in",
            "Zoom in",
            In,
            0.6,
            vec![tween(Scale, 0.3, 1.0, OUT), tween(Opacity, 0.0, 1.0, OUT)],
        ),
        preset(
            "zoom_out_in",
            "Zoom out",
            In,
            0.6,
            vec![tween(Scale, 1.7, 1.0, OUT), tween(Opacity, 0.0, 1.0, OUT)],
        ),
        preset(
            "slide_left_in",
            "Slide left",
            In,
            0.5,
            vec![tween(OffsetX, 1.0, 0.0, OUT)],
        ),
        preset(
            "slide_right_in",
            "Slide right",
            In,
            0.5,
            vec![tween(OffsetX, -1.0, 0.0, OUT)],
        ),
        preset(
            "slide_up_in",
            "Slide up",
            In,
            0.5,
            vec![tween(OffsetY, 1.0, 0.0, OUT)],
        ),
        preset(
            "slide_down_in",
            "Slide down",
            In,
            0.5,
            vec![tween(OffsetY, -1.0, 0.0, OUT)],
        ),
        preset(
            "spin_in",
            "Spin",
            In,
            0.8,
            vec![
                tween(Rotation, -360.0, 0.0, OUT),
                tween(Scale, 0.0, 1.0, OUT),
            ],
        ),
        preset(
            "swing_in",
            "Swing",
            In,
            0.9,
            vec![
                tween(
                    Rotation,
                    35.0,
                    0.0,
                    Easing::Named(Family::Elastic, Dir::Out),
                ),
                tween(Opacity, 0.0, 1.0, lin),
            ],
        ),
        preset(
            "pop_in",
            "Pop",
            In,
            0.5,
            vec![tween(Scale, 0.0, 1.0, BACK_OUT)],
        ),
        preset(
            "bounce_in",
            "Bounce",
            In,
            0.9,
            vec![tween(
                OffsetY,
                -1.0,
                0.0,
                Easing::Named(Family::Bounce, Dir::Out),
            )],
        ),
        preset(
            "blur_in",
            "Blur in",
            In,
            0.7,
            vec![tween(Blur, 40.0, 0.0, OUT), tween(Opacity, 0.0, 1.0, OUT)],
        ),
        preset(
            "wipe_in",
            "Wipe",
            In,
            0.7,
            vec![tween(HideRight, 1.0, 0.0, Easing::EaseInOut)],
        ),
        preset(
            "shake_in",
            "Shake",
            In,
            0.7,
            vec![
                ch(
                    OffsetX,
                    Motion::Decay {
                        amplitude: 0.05,
                        cycles: 5.0,
                    },
                ),
                tween(Opacity, 0.0, 1.0, lin),
            ],
        ),
        preset(
            "turn_in",
            "Turn",
            In,
            0.6,
            vec![tween(ScaleX, 0.0, 1.0, OUT)],
        ),
        // ---- Out
        preset(
            "fade_out",
            "Fade out",
            OutKind,
            0.6,
            vec![tween(Opacity, 1.0, 0.0, lin)],
        ),
        preset(
            "zoom_in_out",
            "Zoom in",
            OutKind,
            0.6,
            vec![tween(Scale, 1.0, 1.7, IN), tween(Opacity, 1.0, 0.0, IN)],
        ),
        preset(
            "zoom_out_out",
            "Zoom out",
            OutKind,
            0.6,
            vec![tween(Scale, 1.0, 0.3, IN), tween(Opacity, 1.0, 0.0, IN)],
        ),
        preset(
            "slide_left_out",
            "Slide left",
            OutKind,
            0.5,
            vec![tween(OffsetX, 0.0, -1.0, IN)],
        ),
        preset(
            "slide_right_out",
            "Slide right",
            OutKind,
            0.5,
            vec![tween(OffsetX, 0.0, 1.0, IN)],
        ),
        preset(
            "slide_up_out",
            "Slide up",
            OutKind,
            0.5,
            vec![tween(OffsetY, 0.0, -1.0, IN)],
        ),
        preset(
            "slide_down_out",
            "Slide down",
            OutKind,
            0.5,
            vec![tween(OffsetY, 0.0, 1.0, IN)],
        ),
        preset(
            "spin_out",
            "Spin",
            OutKind,
            0.8,
            vec![tween(Rotation, 0.0, 360.0, IN), tween(Scale, 1.0, 0.0, IN)],
        ),
        preset(
            "swing_out",
            "Swing",
            OutKind,
            0.7,
            vec![
                tween(Rotation, 0.0, -35.0, IN),
                tween(Opacity, 1.0, 0.0, lin),
            ],
        ),
        preset(
            "pop_out",
            "Pop",
            OutKind,
            0.5,
            vec![tween(Scale, 1.0, 0.0, BACK_IN)],
        ),
        preset(
            "drop_out",
            "Drop",
            OutKind,
            0.7,
            vec![tween(OffsetY, 0.0, 1.0, BACK_IN)],
        ),
        preset(
            "blur_out",
            "Blur out",
            OutKind,
            0.7,
            vec![tween(Blur, 0.0, 40.0, IN), tween(Opacity, 1.0, 0.0, IN)],
        ),
        preset(
            "wipe_out",
            "Wipe",
            OutKind,
            0.7,
            vec![tween(HideLeft, 0.0, 1.0, Easing::EaseInOut)],
        ),
        preset(
            "shake_out",
            "Shake",
            OutKind,
            0.7,
            vec![
                ch(
                    OffsetX,
                    Motion::Grow {
                        amplitude: 0.05,
                        cycles: 5.0,
                    },
                ),
                tween(Opacity, 1.0, 0.0, lin),
            ],
        ),
        preset(
            "turn_out",
            "Turn",
            OutKind,
            0.6,
            vec![tween(ScaleX, 1.0, 0.0, IN)],
        ),
        // ---- Combo (loops)
        preset(
            "wobble",
            "Wobble",
            Combo,
            1.0,
            vec![ch(
                Rotation,
                Motion::Wave {
                    amplitude: 8.0,
                    cycles: 1.0,
                },
            )],
        ),
        preset(
            "pulse",
            "Pulse",
            Combo,
            0.8,
            vec![ch(
                Scale,
                Motion::Wave {
                    amplitude: 0.08,
                    cycles: 1.0,
                },
            )],
        ),
        preset(
            "rotate_loop",
            "Rotate",
            Combo,
            3.0,
            vec![tween(Rotation, 0.0, 360.0, lin)],
        ),
        preset(
            "float",
            "Float",
            Combo,
            2.0,
            vec![ch(
                OffsetY,
                Motion::Wave {
                    amplitude: 0.025,
                    cycles: 1.0,
                },
            )],
        ),
        preset(
            "shake_loop",
            "Shake",
            Combo,
            0.5,
            vec![
                ch(
                    OffsetX,
                    Motion::Wave {
                        amplitude: 0.012,
                        cycles: 6.0,
                    },
                ),
                ch(
                    OffsetY,
                    Motion::Wave {
                        amplitude: 0.008,
                        cycles: 5.0,
                    },
                ),
            ],
        ),
        preset(
            "zoom_loop",
            "Zoom in-out",
            Combo,
            2.0,
            vec![tween(
                Scale,
                1.0,
                1.15,
                Easing::Named(Family::Sine, Dir::InOut),
            )],
        ),
    ]
}

fn all() -> &'static [AnimPreset] {
    static ALL: OnceLock<Vec<AnimPreset>> = OnceLock::new();
    ALL.get_or_init(build)
}

/// The animations of one kind, in the order the panel shows them.
pub fn presets(kind: AnimKind) -> Vec<&'static AnimPreset> {
    all().iter().filter(|p| p.kind == kind).collect()
}

pub fn find(id: &str) -> Option<&'static AnimPreset> {
    all().iter().find(|p| p.id == id)
}

/// How long the in and out animations of a clip really last: each as asked, but together never longer than the clip.
fn windows(clip: &Clip) -> (Micros, Micros) {
    let want_in = clip.anim_in.as_ref().map_or(0, |a| a.duration.max(0));
    let want_out = clip.anim_out.as_ref().map_or(0, |a| a.duration.max(0));
    let total = want_in + want_out;
    if total > clip.duration && total > 0 {
        let k = clip.duration as f64 / total as f64;
        (
            (want_in as f64 * k) as Micros,
            (want_out as f64 * k) as Micros,
        )
    } else {
        (want_in, want_out)
    }
}

fn run(preset: &AnimPreset, p: f32, state: &mut AnimState) {
    for c in &preset.channels {
        // swings go around the "nothing happens" value: 1 for sizes and opacity, 0 for the rest
        let around = match (c.motion, c.prop) {
            (Motion::Tween { .. }, _) => 0.0,
            (_, AnimProp::Scale | AnimProp::ScaleX | AnimProp::Opacity) => 1.0,
            _ => 0.0,
        };
        state.apply(c.prop, around + c.motion.value(p));
    }
}

/// What the clip's animations do `local` microseconds after its start.
pub fn state_at(clip: &Clip, local: Micros) -> AnimState {
    let mut state = AnimState::default();
    if let Some(combo) = &clip.anim_combo {
        if let Some(p) = find(&combo.preset) {
            let period = combo.duration.max(1);
            let phase = local.rem_euclid(period) as f32 / period as f32;
            run(p, phase, &mut state);
        }
        return state;
    }
    let (in_len, out_len) = windows(clip);
    if let Some(a) = &clip.anim_in
        && in_len > 0
        && local < in_len
        && let Some(p) = find(&a.preset)
    {
        run(
            p,
            (local.max(0) as f32 / in_len as f32).clamp(0.0, 1.0),
            &mut state,
        );
    }
    if let Some(a) = &clip.anim_out
        && out_len > 0
        && local > clip.duration - out_len
        && let Some(p) = find(&a.preset)
    {
        let p_out = (local - (clip.duration - out_len)) as f32 / out_len as f32;
        run(p, p_out.clamp(0.0, 1.0), &mut state);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{AnimRef, TextContent};

    fn clip() -> Clip {
        Clip::text_clip(TextContent::new("t"), 0, 4 * US_PER_SEC)
    }

    fn anim(id: &str, seconds: f64) -> Option<AnimRef> {
        Some(AnimRef {
            preset: id.into(),
            duration: (seconds * US_PER_SEC as f64) as Micros,
        })
    }

    #[test]
    fn there_are_plenty_of_animations_of_each_kind_with_unique_ids() {
        assert!(presets(AnimKind::In).len() >= 12);
        assert!(presets(AnimKind::Out).len() >= 12);
        assert!(presets(AnimKind::Combo).len() >= 5);
        let mut ids: Vec<_> = all().iter().map(|p| p.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        for p in all() {
            assert!(!p.channels.is_empty() && p.default_duration > 0, "{}", p.id);
        }
    }

    #[test]
    fn without_animations_nothing_changes() {
        assert!(state_at(&clip(), 1_000_000).is_neutral());
    }

    #[test]
    fn an_in_animation_starts_changed_and_ends_neutral() {
        let mut c = clip();
        c.anim_in = anim("fade_in", 1.0);
        assert_eq!(state_at(&c, 0).opacity, 0.0);
        assert!((state_at(&c, 500_000).opacity - 0.5).abs() < 1e-4);
        assert!(state_at(&c, US_PER_SEC).is_neutral());
        assert!(
            state_at(&c, 3 * US_PER_SEC).is_neutral(),
            "nothing happens in the middle"
        );
    }

    #[test]
    fn an_out_animation_ends_changed() {
        let mut c = clip();
        c.anim_out = anim("fade_out", 1.0);
        assert!(state_at(&c, 3 * US_PER_SEC - 1).is_neutral());
        assert!((state_at(&c, 3_500_000).opacity - 0.5).abs() < 1e-4);
        assert!(state_at(&c, 4 * US_PER_SEC).opacity < 0.001);
    }

    #[test]
    fn slide_and_zoom_move_the_right_things() {
        let mut c = clip();
        c.anim_in = anim("slide_left_in", 1.0);
        assert_eq!(
            state_at(&c, 0).dx,
            1.0,
            "starts a whole canvas to the right"
        );
        assert_eq!(state_at(&c, 0).dy, 0.0);
        c.anim_in = anim("pop_in", 1.0);
        assert_eq!(state_at(&c, 0).scale_x, 0.0);
        assert!(state_at(&c, 700_000).scale_x > 1.0, "a pop overshoots");
        c.anim_in = anim("wipe_in", 1.0);
        assert_eq!(state_at(&c, 0).hide_right, 1.0);
        assert_eq!(state_at(&c, US_PER_SEC).hide_right, 0.0);
    }

    #[test]
    fn a_combo_repeats_and_replaces_in_and_out() {
        let mut c = clip();
        c.anim_in = anim("fade_in", 1.0);
        c.anim_combo = anim("pulse", 1.0);
        // the combo wins: no fade at the start
        assert_eq!(state_at(&c, 0).opacity, 1.0);
        let a = state_at(&c, 250_000).scale_x;
        let b = state_at(&c, 1_250_000).scale_x;
        assert!((a - b).abs() < 1e-4, "one cycle later it is the same");
        assert!(a > 1.0);
        assert!(state_at(&c, 750_000).scale_x < 1.0);
    }

    #[test]
    fn in_and_out_share_a_short_clip_instead_of_overlapping() {
        let mut c = Clip::text_clip(TextContent::new("t"), 0, US_PER_SEC);
        c.anim_in = anim("fade_in", 2.0);
        c.anim_out = anim("fade_out", 2.0);
        let (i, o) = windows(&c);
        assert_eq!(i + o, US_PER_SEC);
        assert!(state_at(&c, 0).opacity < 0.01);
        assert!(state_at(&c, US_PER_SEC - 1).opacity < 0.01);
        assert!(state_at(&c, US_PER_SEC / 2 - 1).opacity > 0.9);
    }

    #[test]
    fn unknown_ids_do_nothing() {
        let mut c = clip();
        c.anim_in = anim("no_such_thing", 1.0);
        assert!(state_at(&c, 0).is_neutral());
    }

    #[test]
    fn a_shake_dies_away() {
        let mut c = clip();
        c.anim_in = anim("shake_in", 1.0);
        let early = (0..50)
            .map(|i| state_at(&c, i * 8_000).dx.abs())
            .fold(0.0, f32::max);
        let late = (800..990)
            .map(|i| state_at(&c, i * 1_000).dx.abs())
            .fold(0.0, f32::max);
        assert!(early > late * 3.0, "{early} vs {late}");
    }
}
