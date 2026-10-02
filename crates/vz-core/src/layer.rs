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
