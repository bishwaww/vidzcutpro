//! Colour and look settings of a clip: adjustments, masks, background removal and effects.

use crate::layer::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One of the eight colour bands of the HSL controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Band {
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Magenta,
}

impl Band {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
        Self::Magenta,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Cyan => "Cyan",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Magenta => "Magenta",
        }
    }

    /// The hue (degrees, 0 to 360) at the middle of the band.
    pub fn centre(self) -> f32 {
        match self {
            Self::Red => 0.0,
            Self::Orange => 30.0,
            Self::Yellow => 60.0,
            Self::Green => 120.0,
            Self::Cyan => 180.0,
            Self::Blue => 240.0,
            Self::Purple => 280.0,
            Self::Magenta => 320.0,
        }
    }
}

/// Changes to one colour band, each from -100 to 100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HslBand {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

/// A tone curve: points `(input, output)` between 0 and 1, joined smoothly. Empty means "unchanged".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub points: Vec<(f32, f32)>,
}

impl Curve {
    pub fn is_identity(&self) -> bool {
        self.points.is_empty() || self.points.iter().all(|(x, y)| (x - y).abs() < 1e-4)
    }

    /// The output for an input between 0 and 1 (a smooth curve through the points, with straight ends).
    pub fn value(&self, x: f32) -> f32 {
        if self.points.is_empty() {
            return x;
        }
        let mut pts: Vec<(f32, f32)> = self.points.clone();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        // the curve reaches the ends of the range through the first and last point
        if pts[0].0 > 0.0 {
            pts.insert(0, (0.0, pts[0].1));
        }
        if pts[pts.len() - 1].0 < 1.0 {
            let last = pts[pts.len() - 1].1;
            pts.push((1.0, last));
        }
        let x = x.clamp(0.0, 1.0);
        let i = pts.partition_point(|p| p.0 <= x).clamp(1, pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        let span = (b.0 - a.0).max(1e-6);
        let t = ((x - a.0) / span).clamp(0.0, 1.0);
        // slopes at the points (monotone cubic, so the curve never loops back)
        let slope = |k: usize| -> f32 {
            let (p0, p1) = (pts[k.saturating_sub(1)], pts[(k + 1).min(pts.len() - 1)]);
            if (p1.0 - p0.0).abs() < 1e-6 {
                0.0
            } else {
                (p1.1 - p0.1) / (p1.0 - p0.0)
            }
        };
        let (m0, m1) = (slope(i - 1) * span, slope(i) * span);
        let (t2, t3) = (t * t, t * t * t);
        let v = (2.0 * t3 - 3.0 * t2 + 1.0) * a.1
            + (t3 - 2.0 * t2 + t) * m0
            + (-2.0 * t3 + 3.0 * t2) * b.1
            + (t3 - t2) * m1;
        v.clamp(0.0, 1.0)
    }
}

/// The four curves: all channels together, and red, green, blue on their own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curves {
    pub master: Curve,
    pub red: Curve,
    pub green: Curve,
    pub blue: Curve,
}

impl Curves {
    pub fn is_identity(&self) -> bool {
        self.master.is_identity()
            && self.red.is_identity()
            && self.green.is_identity()
            && self.blue.is_identity()
    }
}

/// A colour wheel: a tint (red, green, blue, each -1 to 1) and a brightness change (-1 to 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wheel {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub luma: f32,
}

impl Wheel {
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// Lift (shadows), gamma (midtones), gain (highlights) and offset (everything).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wheels {
    pub lift: Wheel,
    pub gamma: Wheel,
    pub gain: Wheel,
    pub offset: Wheel,
}

impl Wheels {
    pub fn is_zero(&self) -> bool {
        self.lift.is_zero() && self.gamma.is_zero() && self.gain.is_zero() && self.offset.is_zero()
    }
}

/// Where a look-up table (a colour "look") comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LutSource {
    /// One that comes with vidzcut (see `vz-presets`).
    Builtin(String),
    /// A `.cube` file the user loaded.
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LutRef {
    pub source: LutSource,
    /// 0 to 1: how strongly the look is applied.
    pub strength: f32,
}

/// All the colour settings of a clip. Sliders run from -100 to 100 unless said otherwise; 0 means "unchanged".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Adjust {
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub brilliance: f32,
    /// 0 to 100.
    pub sharpen: f32,
    pub clarity: f32,
    /// 0 to 100: lifts the blacks for a faded look.
    pub fade: f32,
    /// 0 to 100 (darkening the corners); negative lightens them.
    pub vignette: f32,
    /// 0 to 100.
    pub grain: f32,
    pub hsl: [HslBand; 8],
    pub curves: Curves,
    pub wheels: Wheels,
    pub lut: Option<LutRef>,
}

impl Default for Adjust {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            tint: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            brilliance: 0.0,
            sharpen: 0.0,
            clarity: 0.0,
            fade: 0.0,
            vignette: 0.0,
            grain: 0.0,
            hsl: [HslBand::default(); 8],
            curves: Curves::default(),
            wheels: Wheels::default(),
            lut: None,
        }
    }
}

impl Adjust {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Does anything need the picture around a pixel (not just the pixel itself)?
    pub fn needs_neighbours(&self) -> bool {
        self.sharpen != 0.0 || self.clarity != 0.0
    }
}

/// The shapes a mask can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskShape {
    /// Everything on one side of a straight line.
    Linear,
    /// A band across the middle (the rest is hidden).
    Mirror,
    Circle,
    Rectangle,
    Heart,
    Star,
}

impl MaskShape {
    pub const ALL: [Self; 6] = [
        Self::Linear,
        Self::Mirror,
        Self::Circle,
        Self::Rectangle,
        Self::Heart,
        Self::Star,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Mirror => "Mirror",
            Self::Circle => "Circle",
            Self::Rectangle => "Rectangle",
            Self::Heart => "Heart",
            Self::Star => "Star",
        }
    }
}

/// A mask on a clip: only the part inside the shape is shown (or only the part outside, if inverted).
/// Positions and sizes are shares of the clip's picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskDef {
    pub shape: MaskShape,
    /// Centre of the shape, from the middle of the picture: -0.5 (left / top edge) to 0.5.
    pub x: f32,
    pub y: f32,
    /// Size as a share of the picture's width and height (1.0 = as wide as the picture).
    pub width: f32,
    pub height: f32,
    /// Degrees, clockwise.
    pub rotation: f32,
    /// Softness of the edge, 0 to 1 (a share of the shape's size).
    pub feather: f32,
    /// Rounded corners of a rectangle, 0 to 1.
    pub corner: f32,
    pub invert: bool,
}

impl MaskDef {
    pub fn new(shape: MaskShape) -> Self {
        Self {
            shape,
            x: 0.0,
            y: 0.0,
            width: 0.6,
            height: 0.6,
            rotation: 0.0,
            feather: 0.0,
            corner: 0.0,
            invert: false,
        }
    }
}

/// Removes a colour (a green screen) from a picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChromaKey {
    pub color: Color,
    /// 0 to 100: how far from the key colour a colour may be and still disappear.
    pub intensity: f32,
    /// 0 to 100: darkens semi-transparent edges (shadows cast on the screen) instead of removing them.
    pub shadow: f32,
    /// 0 to 100: softness of the edge.
    pub feather: f32,
    /// 0 to 100: takes the key colour out of what is left (green light on hair, for example).
    pub spill: f32,
}

impl ChromaKey {
    pub fn green() -> Self {
        Self {
            color: [0, 255, 0, 255],
            intensity: 40.0,
            shadow: 20.0,
            feather: 20.0,
            spill: 50.0,
        }
    }
}

/// An effect (see `vz-presets`) put on a clip, or on everything below an effect clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectRef {
    pub preset: String,
    /// The values of the effect's controls, in the order the preset lists them.
    pub params: Vec<f32>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_without_points_or_with_a_straight_line_changes_nothing() {
        assert!(Curve::default().is_identity());
        let line = Curve {
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        };
        assert!(line.is_identity());
        for x in [0.0, 0.3, 0.9] {
            assert!((line.value(x) - x).abs() < 1e-4);
        }
    }

    #[test]
    fn a_curve_passes_through_its_points_and_never_overshoots() {
        let c = Curve {
            points: vec![(0.0, 0.0), (0.25, 0.4), (0.75, 0.6), (1.0, 1.0)],
        };
        assert!((c.value(0.25) - 0.4).abs() < 1e-3);
        assert!((c.value(0.75) - 0.6).abs() < 1e-3);
        let mut last = 0.0;
        for i in 0..=100 {
            let v = c.value(i as f32 / 100.0);
            assert!((0.0..=1.0).contains(&v));
            assert!(v >= last - 1e-4, "steady rise at {i}");
            last = v;
        }
    }

    #[test]
    fn the_ends_of_a_curve_hold_when_the_outer_points_are_inside_the_range() {
        let c = Curve {
            points: vec![(0.2, 0.3), (0.8, 0.7)],
        };
        assert!((c.value(0.0) - 0.3).abs() < 1e-3);
        assert!((c.value(1.0) - 0.7).abs() < 1e-3);
    }

    #[test]
    fn new_settings_change_nothing_and_save_and_load() {
        let a = Adjust::default();
        assert!(a.is_identity());
        let mut b = a.clone();
        b.exposure = 20.0;
        b.curves.master.points = vec![(0.5, 0.6)];
        b.lut = Some(LutRef {
            source: LutSource::Builtin("warm".into()),
            strength: 0.5,
        });
        assert!(!b.is_identity());
        let back: Adjust = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn the_bands_run_around_the_colour_wheel() {
        let centres: Vec<f32> = Band::ALL.iter().map(|b| b.centre()).collect();
        assert!(centres.windows(2).all(|w| w[0] < w[1]));
    }
}
