//! Built-in effects, transitions, filters, animations and text styles.
//!
//! Phase 2 has the text styles. They are plain data (a [`TextStyle`] and some sample words), so
//! later the same shape can be loaded from files and from the user's own saved styles.

use vz_core::{
    BLACK, ShapeContent, ShapeKind, TextBackground, TextShadow, TextStroke, TextStyle, WHITE,
};

pub mod animation;
pub mod speed;
pub mod transition;

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-presets";

/// A ready-made look for text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPreset {
    pub name: &'static str,
    pub style: TextStyle,
    /// What a new text with this look says until the user types their own.
    pub text: &'static str,
    /// Short words shown on the preset's tile.
    pub sample: &'static str,
}

const YELLOW: [u8; 4] = [252, 198, 16, 255];
const DARK: [u8; 4] = [10, 10, 10, 255];

fn base(f: impl FnOnce(&mut TextStyle)) -> TextStyle {
    let mut s = TextStyle::default();
    f(&mut s);
    s
}

/// All built-in text styles, in the order the panel shows them.
pub fn text_presets() -> Vec<TextPreset> {
    vec![
        TextPreset {
            name: "Default",
            style: base(|_| {}),
            text: "Your text",
            sample: "Aa",
        },
        TextPreset {
            name: "Title",
            style: base(|s| {
                s.size = 120.0;
                s.bold = true;
                s.stroke = Some(TextStroke {
                    color: BLACK,
                    width: 5.0,
                });
                s.shadow = Some(TextShadow {
                    color: [0, 0, 0, 160],
                    blur: 10.0,
                    dx: 4.0,
                    dy: 6.0,
                });
            }),
            text: "Big title",
            sample: "Title",
        },
        TextPreset {
            name: "Subtitle",
            style: base(|s| {
                s.size = 54.0;
                s.shadow = Some(TextShadow {
                    color: [0, 0, 0, 200],
                    blur: 6.0,
                    dx: 2.0,
                    dy: 3.0,
                });
            }),
            text: "Subtitle goes here",
            sample: "Subtitle",
        },
        TextPreset {
            name: "Yellow bar",
            style: base(|s| {
                s.size = 64.0;
                s.bold = true;
                s.color = DARK;
                s.background = Some(TextBackground {
                    color: YELLOW,
                    padding: 24.0,
                    radius: 12.0,
                });
            }),
            text: "Headline",
            sample: "Headline",
        },
        TextPreset {
            name: "Caption box",
            style: base(|s| {
                s.size = 48.0;
                s.background = Some(TextBackground {
                    color: [0, 0, 0, 170],
                    padding: 18.0,
                    radius: 10.0,
                });
            }),
            text: "A caption",
            sample: "Caption",
        },
        TextPreset {
            name: "Neon",
            style: base(|s| {
                s.size = 96.0;
                s.bold = true;
                s.color = YELLOW;
                s.shadow = Some(TextShadow {
                    color: [252, 198, 16, 230],
                    blur: 28.0,
                    dx: 0.0,
                    dy: 0.0,
                });
            }),
            text: "Neon",
            sample: "Neon",
        },
        TextPreset {
            name: "Outline",
            style: base(|s| {
                s.size = 100.0;
                s.bold = true;
                s.stroke = Some(TextStroke {
                    color: BLACK,
                    width: 9.0,
                });
            }),
            text: "Outline",
            sample: "Outline",
        },
        TextPreset {
            name: "Retro",
            style: base(|s| {
                s.size = 100.0;
                s.bold = true;
                s.italic = true;
                s.color = [255, 140, 40, 255];
                s.letter_spacing = 0.04;
                s.shadow = Some(TextShadow {
                    color: [60, 20, 0, 255],
                    blur: 0.0,
                    dx: 6.0,
                    dy: 6.0,
                });
            }),
            text: "Retro",
            sample: "Retro",
        },
        TextPreset {
            name: "Sinhala headline",
            style: base(|s| {
                s.font = "Noto Sans Sinhala".into();
                s.size = 96.0;
                s.bold = true;
                s.shadow = Some(TextShadow {
                    color: [0, 0, 0, 190],
                    blur: 8.0,
                    dx: 3.0,
                    dy: 4.0,
                });
            }),
            text: "ශ්‍රී ලංකාව",
            sample: "ශ්‍රී ලංකාව",
        },
        TextPreset {
            name: "Sinhala serif",
            style: base(|s| {
                s.font = "Noto Serif Sinhala".into();
                s.size = 80.0;
                s.stroke = Some(TextStroke {
                    color: BLACK,
                    width: 3.0,
                });
            }),
            text: "ක්‍රීඩාව",
            sample: "ක්‍රීඩාව",
        },
        TextPreset {
            name: "Tamil",
            style: base(|s| {
                s.font = "Noto Sans Tamil".into();
                s.size = 84.0;
                s.shadow = Some(TextShadow {
                    color: [0, 0, 0, 190],
                    blur: 8.0,
                    dx: 3.0,
                    dy: 4.0,
                });
            }),
            text: "தமிழ் எழுத்து",
            sample: "தமிழ்",
        },
        TextPreset {
            name: "Sinhala caption",
            style: base(|s| {
                s.font = "Noto Sans Sinhala".into();
                s.size = 52.0;
                s.color = WHITE;
                s.background = Some(TextBackground {
                    color: [0, 0, 0, 170],
                    padding: 18.0,
                    radius: 10.0,
                });
            }),
            text: "ඇස් සම්බන්ධ රෝග",
            sample: "ඇස් සම්බන්ධ",
        },
    ]
}

/// A ready-made shape or sticker.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapePreset {
    pub name: &'static str,
    pub shape: ShapeContent,
}

fn shape(
    name: &'static str,
    kind: ShapeKind,
    size: (f32, f32),
    fill: Option<[u8; 4]>,
    stroke: Option<(vz_core::Color, f32)>,
) -> ShapePreset {
    ShapePreset {
        name,
        shape: ShapeContent {
            kind,
            width: size.0,
            height: size.1,
            fill,
            stroke: stroke.map(|(color, width)| TextStroke { color, width }),
            corner_radius: 0.0,
        },
    }
}

/// Plain shapes: one of each kind, white with a thin dark outline so they show on any picture.
pub fn basic_shapes() -> Vec<ShapePreset> {
    ShapeKind::ALL
        .iter()
        .map(|kind| {
            let size = ShapeContent::new(*kind);
            shape(
                kind.label(),
                *kind,
                (size.width, size.height),
                Some(WHITE),
                Some((DARK, 4.0)),
            )
        })
        .collect()
}

/// Ready-made colourful stickers drawn by vidzcut (no outside artwork).
pub fn sticker_presets() -> Vec<ShapePreset> {
    let red = [235, 64, 52, 255];
    let green = [60, 190, 110, 255];
    let blue = [60, 130, 240, 255];
    let pink = [255, 105, 180, 255];
    let mut sale = shape(
        "Sale star",
        ShapeKind::Star,
        (320.0, 320.0),
        Some(YELLOW),
        Some((red, 10.0)),
    );
    sale.shape.corner_radius = 0.0;
    vec![
        sale,
        shape(
            "Heart",
            ShapeKind::Heart,
            (300.0, 280.0),
            Some(red),
            Some((WHITE, 8.0)),
        ),
        shape(
            "Pink heart",
            ShapeKind::Heart,
            (300.0, 280.0),
            Some(pink),
            None,
        ),
        shape(
            "Arrow",
            ShapeKind::Arrow,
            (380.0, 220.0),
            Some(YELLOW),
            Some((DARK, 8.0)),
        ),
        shape(
            "Green arrow",
            ShapeKind::Arrow,
            (380.0, 220.0),
            Some(green),
            Some((WHITE, 8.0)),
        ),
        shape(
            "Speech bubble",
            ShapeKind::Bubble,
            (380.0, 280.0),
            Some(WHITE),
            Some((DARK, 8.0)),
        ),
        shape(
            "Blue bubble",
            ShapeKind::Bubble,
            (380.0, 280.0),
            Some(blue),
            Some((WHITE, 8.0)),
        ),
        shape(
            "Badge",
            ShapeKind::Hexagon,
            (300.0, 300.0),
            Some(red),
            Some((WHITE, 12.0)),
        ),
        shape(
            "Gem",
            ShapeKind::Diamond,
            (260.0, 320.0),
            Some(blue),
            Some((WHITE, 8.0)),
        ),
        shape(
            "Plus",
            ShapeKind::Plus,
            (260.0, 260.0),
            Some(green),
            Some((WHITE, 8.0)),
        ),
        shape(
            "Ring",
            ShapeKind::Ellipse,
            (300.0, 300.0),
            None,
            Some((YELLOW, 18.0)),
        ),
        shape(
            "Underline",
            ShapeKind::Line,
            (500.0, 18.0),
            Some(YELLOW),
            None,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_are_unique_and_texts_not_empty() {
        let presets = text_presets();
        let mut names: Vec<_> = presets.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), presets.len());
        assert!(
            presets
                .iter()
                .all(|p| !p.text.is_empty() && !p.sample.is_empty())
        );
    }

    #[test]
    fn there_are_sinhala_and_tamil_presets() {
        let presets = text_presets();
        assert!(
            presets
                .iter()
                .filter(|p| p.style.font.contains("Sinhala"))
                .count()
                >= 2
        );
        assert!(presets.iter().any(|p| p.style.font.contains("Tamil")));
    }

    #[test]
    fn shape_presets_are_named_uniquely_and_cover_every_kind() {
        let basic = basic_shapes();
        assert_eq!(basic.len(), ShapeKind::ALL.len());
        let stickers = sticker_presets();
        let mut names: Vec<_> = stickers.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), stickers.len());
        assert!(
            stickers
                .iter()
                .all(|p| p.shape.fill.is_some() || p.shape.stroke.is_some())
        );
    }
}
