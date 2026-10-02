//! What a clip shows or plays: a media file, or text.

use crate::ids::MediaId;
use crate::layer::{Color, WHITE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Content {
    /// A video, photo, sticker image or sound file from the media pool.
    Media(MediaId),
    Text(TextContent),
}

impl Content {
    pub fn media(&self) -> Option<MediaId> {
        match self {
            Self::Media(id) => Some(*id),
            Self::Text(_) => None,
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
