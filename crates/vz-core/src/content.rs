//! What a clip shows or plays: a media file, or text.

use crate::ids::MediaId;
use crate::layer::{Color, Gradient, WHITE};
use crate::time::Micros;
use serde::{Deserialize, Serialize};
use vz_anim::Easing;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Content {
    /// A video, photo, sticker image or sound file from the media pool.
    Media(MediaId),
    Text(TextContent),
    /// A drawn shape (rectangle, star, heart, arrow, ...).
    Shape(ShapeContent),
    /// Draws nothing itself: its colour settings and effects change everything below it for as long as it lasts.
    /// (An adjustment layer; a single effect put on the timeline is the same thing with one effect in it.)
    Adjustment,
}

impl Content {
    pub fn media(&self) -> Option<MediaId> {
        match self {
            Self::Media(id) => Some(*id),
            Self::Text(_) | Self::Shape(_) | Self::Adjustment => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
    /// Lines stretched to the full width (needs a wrap width).
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStroke {
    pub color: Color,
    /// Width in canvas pixels at a 1080-pixel canvas height.
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextShadow {
    pub color: Color,
    /// Blur radius and offset, in canvas pixels at a 1080-pixel canvas height.
    pub blur: f32,
    pub dx: f32,
    pub dy: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBackground {
    pub color: Color,
    /// Space between the text and the edge of the box, and the corner radius (same units as above).
    pub padding: f32,
    pub radius: f32,
}

/// A glow around the letters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextGlow {
    pub color: Color,
    /// Reference pixels.
    pub radius: f32,
    /// 0 to 1.
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextCase {
    #[default]
    Normal,
    Upper,
    Lower,
    /// First letter of every word big.
    Title,
}

/// A different look for part of the text (counted in characters, from `start` up to but not including `end`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    #[serde(default)]
    pub color: Option<Color>,
    /// Size as a multiple of the normal size.
    #[serde(default)]
    pub size: Option<f32>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub font: Option<String>,
    /// Keeps the room but draws nothing.
    #[serde(default)]
    pub hidden: bool,
}

/// How words of a caption are treated while it is on screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum WordMode {
    #[default]
    Off,
    /// Words change to the highlight colour as they are spoken.
    Karaoke { highlight: Color, scale: f32 },
    /// Words appear one after the other.
    Reveal { highlight: Option<Color> },
    /// Only the word being spoken is shown.
    Single { scale: f32 },
    /// Chosen words (counted from 0) stand out all the time.
    Keywords {
        highlight: Color,
        scale: f32,
        words: Vec<usize>,
    },
}

/// What a text animator moves: single letters, words or lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextUnit {
    #[default]
    Character,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextPhase {
    /// Plays at the start of the clip.
    #[default]
    In,
    /// Plays at the end of the clip.
    Out,
    /// Repeats all the time.
    Loop,
}

/// Animates the parts of a text one after another: each unit starts `delay` seconds after the one before and takes
/// `duration` seconds to go between its "away" look and its normal look.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextAnimator {
    pub unit: TextUnit,
    pub phase: TextPhase,
    /// How far away from its place a unit starts (reference pixels).
    pub dx: f32,
    pub dy: f32,
    /// Size at the away look (1 = normal).
    pub scale: f32,
    /// Degrees at the away look.
    pub rotation: f32,
    /// Opacity at the away look, 0 to 1.
    pub opacity: f32,
    pub delay: f32,
    pub duration: f32,
    pub easing: Easing,
}

impl Default for TextAnimator {
    fn default() -> Self {
        Self {
            unit: TextUnit::Character,
            phase: TextPhase::In,
            dx: 0.0,
            dy: 40.0,
            scale: 1.0,
            rotation: 0.0,
            opacity: 0.0,
            delay: 0.06,
            duration: 0.4,
            easing: Easing::EaseOut,
        }
    }
}

/// How a piece of text looks. Sizes are in canvas pixels at a 1080-pixel canvas height; they grow and shrink with the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Font family name. Empty means "pick a font that has the letters" (Sinhala, Tamil or Latin).
    #[serde(default)]
    pub font: String,
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    pub color: Color,
    #[serde(default)]
    pub align: TextAlign,
    /// 1.0 is the font's normal line height.
    pub line_spacing: f32,
    /// Extra space between letters, as a share of the font size.
    pub letter_spacing: f32,
    #[serde(default)]
    pub stroke: Option<TextStroke>,
    #[serde(default)]
    pub shadow: Option<TextShadow>,
    #[serde(default)]
    pub background: Option<TextBackground>,
    /// Wrap lines at this width (canvas pixels at 1080 high). `None` wraps only at line breaks.
    #[serde(default)]
    pub box_width: Option<f32>,
    /// Fills the letters with a gradient instead of the colour.
    #[serde(default)]
    pub gradient: Option<Gradient>,
    /// Up to two more outlines outside the first, each as wide again as its `width`.
    #[serde(default)]
    pub more_strokes: Vec<TextStroke>,
    #[serde(default)]
    pub glow: Option<TextGlow>,
    /// Bends the line along an arc: -100 (a smile) to 100 (a frown). 0 is straight.
    #[serde(default)]
    pub curve: f32,
    #[serde(default)]
    pub case: TextCase,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: String::new(),
            size: 72.0,
            bold: false,
            italic: false,
            underline: false,
            color: WHITE,
            align: TextAlign::Center,
            line_spacing: 1.0,
            letter_spacing: 0.0,
            stroke: None,
            shadow: None,
            background: None,
            box_width: None,
            gradient: None,
            more_strokes: Vec::new(),
            glow: None,
            curve: 0.0,
            case: TextCase::Normal,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    pub style: TextStyle,
    /// Different looks for parts of the text.
    #[serde(default)]
    pub spans: Vec<StyleSpan>,
    #[serde(default)]
    pub animators: Vec<TextAnimator>,
    #[serde(default)]
    pub words: WordMode,
    /// When each word starts, in microseconds from the start of the clip. Empty: spread over the clip by word length.
    #[serde(default)]
    pub word_times: Vec<Micros>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// The kinds of shape vidzcut can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Star,
    Heart,
    Arrow,
    Plus,
    Bubble,
    Line,
}

impl ShapeKind {
    pub const ALL: [Self; 12] = [
        Self::Rectangle,
        Self::Ellipse,
        Self::Triangle,
        Self::Diamond,
        Self::Pentagon,
        Self::Hexagon,
        Self::Star,
        Self::Heart,
        Self::Arrow,
        Self::Plus,
        Self::Bubble,
        Self::Line,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Triangle => "Triangle",
            Self::Diamond => "Diamond",
            Self::Pentagon => "Pentagon",
            Self::Hexagon => "Hexagon",
            Self::Star => "Star",
            Self::Heart => "Heart",
            Self::Arrow => "Arrow",
            Self::Plus => "Plus",
            Self::Bubble => "Speech bubble",
            Self::Line => "Line",
        }
    }
}

/// A shape: its kind, size and colours. Sizes are in reference pixels (a canvas whose short side is 1080 pixels),
/// like text sizes, and scale with the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeContent {
    pub kind: ShapeKind,
    pub width: f32,
    pub height: f32,
    /// `None` leaves the inside empty.
    pub fill: Option<Color>,
    pub stroke: Option<TextStroke>,
    /// Rounded corners for rectangles, in reference pixels.
    #[serde(default)]
    pub corner_radius: f32,
}

impl ShapeContent {
    pub fn new(kind: ShapeKind) -> Self {
        let (width, height) = match kind {
            ShapeKind::Line => (400.0, 12.0),
            ShapeKind::Arrow => (360.0, 200.0),
            ShapeKind::Bubble => (360.0, 260.0),
            _ => (300.0, 300.0),
        };
        Self {
            kind,
            width,
            height,
            fill: Some([252, 198, 16, 255]),
            stroke: None,
            corner_radius: 0.0,
        }
    }
}
