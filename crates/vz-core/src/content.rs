//! What a clip shows or plays: a media file, or text.

use crate::ids::MediaId;
use crate::layer::{Color, WHITE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Content {
    /// A video, photo, sticker image or sound file from the media pool.
    Media(MediaId),
    Text(TextContent),
    /// A drawn shape (rectangle, star, heart, arrow, ...).
    Shape(ShapeContent),
}

impl Content {
    pub fn media(&self) -> Option<MediaId> {
        match self {
            Self::Media(id) => Some(*id),
            Self::Text(_) | Self::Shape(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    pub style: TextStyle,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TextStyle::default(),
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
