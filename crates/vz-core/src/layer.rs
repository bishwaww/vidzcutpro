//! How a clip is placed and mixed onto the picture: position, size, turn, opacity, blend mode, crop.

use serde::{Deserialize, Serialize};
use vz_anim::Animatable;

/// A colour: red, green, blue, alpha (0–255 each, not premultiplied).
pub type Color = [u8; 4];

pub const WHITE: Color = [255, 255, 255, 255];
pub const BLACK: Color = [0, 0, 0, 255];

/// How a layer's colours combine with what is already below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    Lighten,
    Screen,
    ColorDodge,
    /// "Linear dodge" in other editors.
    Add,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [Self; 18] = [
        Self::Normal,
        Self::Darken,
        Self::Multiply,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::Lighten,
        Self::Screen,
        Self::ColorDodge,
        Self::Add,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Darken => "Darken",
            Self::Multiply => "Multiply",
            Self::ColorBurn => "Color burn",
            Self::LinearBurn => "Linear burn",
            Self::Lighten => "Lighten",
            Self::Screen => "Screen",
            Self::ColorDodge => "Color dodge",
            Self::Add => "Add",
            Self::Overlay => "Overlay",
            Self::SoftLight => "Soft light",
            Self::HardLight => "Hard light",
            Self::Difference => "Difference",
            Self::Exclusion => "Exclusion",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
        }
    }
}

fn one() -> Animatable<f32> {
    Animatable::Static(1.0)
}

fn zero() -> Animatable<f32> {
    Animatable::Static(0.0)
}

fn yes() -> bool {
    true
}

/// Where a layer sits on the canvas. Distances are canvas pixels; the middle of the canvas is (0, 0),
/// positive x is to the right and positive y is down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    #[serde(default = "zero")]
    pub x: Animatable<f32>,
    #[serde(default = "zero")]
    pub y: Animatable<f32>,
    /// 1.0 = the picture fitted inside the canvas (or the text/shape at its own size).
    #[serde(default = "one")]
    pub scale_x: Animatable<f32>,
    #[serde(default = "one")]
    pub scale_y: Animatable<f32>,
    /// Degrees, clockwise.
    #[serde(default = "zero")]
    pub rotation: Animatable<f32>,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    /// Whether the size controls keep the picture's shape (a setting of the editing screen, saved with the clip).
    #[serde(default = "yes")]
    pub uniform_scale: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: zero(),
            y: zero(),
            scale_x: one(),
            scale_y: one(),
            rotation: zero(),
            flip_h: false,
            flip_v: false,
            uniform_scale: true,
        }
    }
}

/// The values of a [`Transform`] at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedTransform {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Transform {
    /// The values `t` microseconds after the clip's start.
    pub fn at(&self, t: i64) -> PlacedTransform {
        PlacedTransform {
            x: self.x.value_at(t),
            y: self.y.value_at(t),
            scale_x: self.scale_x.value_at(t),
            scale_y: self.scale_y.value_at(t),
            rotation: self.rotation.value_at(t),
            flip_h: self.flip_h,
            flip_v: self.flip_v,
        }
    }

    /// True when nothing moves, turns or resizes the picture.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}

/// How much to cut off each side of the picture, as a share (0.0 to 0.9) of its width or height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    #[serde(default)]
    pub left: f32,
    #[serde(default)]
    pub top: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
}

impl Crop {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// The same crop with every side inside its limits, and at least 5 % of the picture left in each direction.
    pub fn clamped(self) -> Self {
        let side = |v: f32| v.clamp(0.0, 0.9);
        let mut c = Self {
            left: side(self.left),
            top: side(self.top),
            right: side(self.right),
            bottom: side(self.bottom),
        };
        if c.left + c.right > 0.95 {
            let k = 0.95 / (c.left + c.right);
            c.left *= k;
            c.right *= k;
        }
        if c.top + c.bottom > 0.95 {
            let k = 0.95 / (c.top + c.bottom);
            c.top *= k;
            c.bottom *= k;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_transform_changes_nothing() {
        let t = Transform::default();
        assert!(t.is_identity());
        let p = t.at(123);
        assert_eq!(
            (p.x, p.y, p.scale_x, p.scale_y, p.rotation),
            (0.0, 0.0, 1.0, 1.0, 0.0)
        );
    }

    #[test]
    fn all_eighteen_blend_modes_have_a_name() {
        assert_eq!(BlendMode::ALL.len(), 18);
        assert!(BlendMode::ALL.iter().all(|m| !m.label().is_empty()));
    }

    #[test]
    fn crop_stays_inside_its_limits() {
        let c = Crop {
            left: 0.8,
            right: 0.8,
            top: -1.0,
            bottom: 2.0,
        }
        .clamped();
        assert!(c.left + c.right <= 0.95 + 1e-6);
        assert_eq!(c.top, 0.0);
        assert!(c.bottom <= 0.9);
        assert!(Crop::default().is_none());
    }

    #[test]
    fn an_old_clip_without_these_fields_still_loads() {
        let t: Transform = serde_json::from_str("{}").unwrap();
        assert!(t.is_identity());
    }
}

/// A ready-made animation chosen for a clip (see `vz-presets`): which preset and how long it takes.
/// For "combo" (looping) animations `duration` is the length of one cycle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimRef {
    pub preset: String,
    pub duration: crate::time::Micros,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    #[default]
    Linear,
    /// From the middle outwards.
    Radial,
}

/// A colour that changes across a shape or a piece of text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Direction of a linear gradient in degrees (0 = left to right, 90 = top to bottom).
    pub angle: f32,
    /// Colours at positions 0 to 1 along the gradient, in order.
    pub stops: Vec<(f32, Color)>,
}

impl Gradient {
    pub fn two(from: Color, to: Color, angle: f32) -> Self {
        Self {
            kind: GradientKind::Linear,
            angle,
            stops: vec![(0.0, from), (1.0, to)],
        }
    }

    /// The colour at position `t` (0 to 1) along the gradient.
    pub fn color_at(&self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let (Some(first), Some(last)) = (self.stops.first(), self.stops.last()) else {
            return WHITE;
        };
        if t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }
        let i = self
            .stops
            .partition_point(|s| s.0 <= t)
            .clamp(1, self.stops.len() - 1);
        let (a, b) = (self.stops[i - 1], self.stops[i]);
        let f = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
        let mix = |k: usize| {
            (f32::from(a.1[k]) + (f32::from(b.1[k]) - f32::from(a.1[k])) * f + 0.5) as u8
        };
        [mix(0), mix(1), mix(2), mix(3)]
    }

    /// The colour at a place in a box: (`u`, `v`) are 0 to 1 from the left and from the top.
    pub fn at(&self, u: f32, v: f32) -> Color {
        let t = match self.kind {
            GradientKind::Linear => {
                let (sin, cos) = self.angle.to_radians().sin_cos();
                0.5 + ((u - 0.5) * cos + (v - 0.5) * sin) / (cos.abs() + sin.abs()).max(1e-6)
            }
            GradientKind::Radial => ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0,
        };
        self.color_at(t)
    }
}

#[cfg(test)]
mod gradient_tests {
    use super::*;

    #[test]
    fn a_two_colour_gradient_blends_between_its_ends() {
        let g = Gradient::two([0, 0, 0, 255], [200, 100, 50, 255], 0.0);
        assert_eq!(g.color_at(0.0), [0, 0, 0, 255]);
        assert_eq!(g.color_at(1.0), [200, 100, 50, 255]);
        assert_eq!(g.color_at(0.5), [100, 50, 25, 255]);
        assert_eq!(g.color_at(-3.0), [0, 0, 0, 255]);
    }

    #[test]
    fn a_linear_gradient_follows_its_angle_and_a_radial_one_the_distance() {
        let across = Gradient::two([0, 0, 0, 255], [255, 255, 255, 255], 0.0);
        assert!(across.at(0.0, 0.5)[0] < 10 && across.at(1.0, 0.5)[0] > 245);
        assert!((i32::from(across.at(0.5, 0.0)[0]) - i32::from(across.at(0.5, 1.0)[0])).abs() < 3);
        let down = Gradient::two([0, 0, 0, 255], [255, 255, 255, 255], 90.0);
        assert!(down.at(0.5, 0.0)[0] < 10 && down.at(0.5, 1.0)[0] > 245);
        let radial = Gradient {
            kind: GradientKind::Radial,
            ..across
        };
        assert!(radial.at(0.5, 0.5)[0] < 10 && radial.at(1.0, 0.5)[0] > 245);
    }

    #[test]
    fn many_stops_are_followed_in_order() {
        let g = Gradient {
            kind: GradientKind::Linear,
            angle: 0.0,
            stops: vec![
                (0.0, [255, 0, 0, 255]),
                (0.5, [0, 255, 0, 255]),
                (1.0, [0, 0, 255, 255]),
            ],
        };
        assert_eq!(g.color_at(0.5), [0, 255, 0, 255]);
        assert_eq!(g.color_at(0.25), [128, 128, 0, 255]);
        assert_eq!(Gradient { stops: vec![], ..g }.color_at(0.3), WHITE);
    }
}
