//! Templates the user makes from their own clips: text, shapes and effect layers saved as a group
//! that can be put on any project later.

use crate::content::Content;
use crate::time::Micros;
use crate::timeline::{Clip, TrackKind};
use serde::{Deserialize, Serialize};

pub const TEMPLATE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserTemplate {
    pub version: u32,
    pub name: String,
    /// The canvas size the template was made on (positions are scaled when it is used on another canvas).
    pub canvas: (u32, u32),
    /// How long the whole template lasts.
    pub duration: Micros,
    /// The layers, bottom layer first; each clip's `start` is relative to where the template is placed.
    pub parts: Vec<(TrackKind, Clip)>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error(
        "select some text, shapes or effects first (pictures and videos cannot be saved in a template)"
    )]
    NothingToSave,
}

/// Can this clip be part of a template? (Anything that needs a file of its own cannot.)
pub fn can_save(clip: &Clip) -> bool {
    matches!(
        clip.content,
        Content::Text(_) | Content::Shape(_) | Content::Adjustment
    )
}

impl UserTemplate {
    /// Make a template from clips (bottom layer first). Returns it and how many clips had to be left out.
    pub fn from_clips(
        name: &str,
        canvas: (u32, u32),
        clips: &[(TrackKind, &Clip)],
    ) -> Result<(Self, usize), TemplateError> {
        let kept: Vec<(TrackKind, &Clip)> =
            clips.iter().copied().filter(|(_, c)| can_save(c)).collect();
        if kept.is_empty() {
            return Err(TemplateError::NothingToSave);
        }
        let left_out = clips.len() - kept.len();
        let first = kept.iter().map(|(_, c)| c.start).min().unwrap_or(0);
        let end = kept.iter().map(|(_, c)| c.end()).max().unwrap_or(first);
        let parts = kept
            .into_iter()
            .map(|(kind, c)| {
                let mut clip = c.clone();
                clip.start -= first;
                clip.group = None;
                clip.link = None;
                (kind, clip)
            })
            .collect();
        Ok((
            Self {
                version: TEMPLATE_VERSION,
                name: name.trim().to_string(),
                canvas,
                duration: end - first,
                parts,
            },
            left_out,
        ))
    }

    /// The layers for a canvas of `width` by `height`: positions follow the canvas size.
    pub fn build(&self, width: u32, height: u32) -> Vec<(TrackKind, Clip)> {
        let kx = width as f32 / self.canvas.0.max(1) as f32;
        let ky = height as f32 / self.canvas.1.max(1) as f32;
        self.parts
            .iter()
            .map(|(kind, clip)| {
                let mut clip = clip.clone();
                if (kx - 1.0).abs() > 1e-4 || (ky - 1.0).abs() > 1e-4 {
                    clip.transform.x.scale_values(kx);
                    clip.transform.y.scale_values(ky);
                }
                (*kind, clip)
            })
            .collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Option<Self> {
        let t: Self = serde_json::from_str(text).ok()?;
        (t.version <= TEMPLATE_VERSION && !t.parts.is_empty()).then_some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::TextContent;
    use vz_anim::Animatable;

    fn text_clip(start: Micros, x: f32) -> Clip {
        let mut c = Clip::text_clip(TextContent::new("ශ්‍රී ලංකාව"), start, 2_000_000);
        c.transform.x = Animatable::Static(x);
        c
    }

    #[test]
    fn a_template_starts_at_zero_and_keeps_only_what_can_be_saved() {
        let a = text_clip(5_000_000, 100.0);
        let b = text_clip(6_000_000, 0.0);
        let media = Clip::media_clip(crate::MediaId::new(), 5_000_000, 0, 1_000_000);
        let (t, left_out) = UserTemplate::from_clips(
            "  My title ",
            (1920, 1080),
            &[
                (TrackKind::Text, &a),
                (TrackKind::Text, &b),
                (TrackKind::Overlay, &media),
            ],
        )
        .unwrap();
        assert_eq!(t.name, "My title");
        assert_eq!(left_out, 1);
        assert_eq!(t.parts.len(), 2);
        assert_eq!(t.parts[0].1.start, 0);
        assert_eq!(t.parts[1].1.start, 1_000_000);
        assert_eq!(t.duration, 3_000_000);
    }

    #[test]
    fn nothing_to_save_is_an_error() {
        let media = Clip::media_clip(crate::MediaId::new(), 0, 0, 1_000_000);
        assert_eq!(
            UserTemplate::from_clips("x", (1920, 1080), &[(TrackKind::Overlay, &media)]),
            Err(TemplateError::NothingToSave)
        );
        assert_eq!(
            UserTemplate::from_clips("x", (1920, 1080), &[]),
            Err(TemplateError::NothingToSave)
        );
    }

    #[test]
    fn positions_follow_the_canvas_and_the_file_round_trips() {
        let a = text_clip(0, 100.0);
        let (t, _) = UserTemplate::from_clips("t", (1920, 1080), &[(TrackKind::Text, &a)]).unwrap();
        let tall = t.build(1080, 1920);
        assert_eq!(tall[0].1.transform.x, Animatable::Static(56.25));
        assert_eq!(
            t.build(1920, 1080)[0].1.transform.x,
            Animatable::Static(100.0)
        );
        let back = UserTemplate::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
        // Sinhala text survives the file
        match &back.parts[0].1.content {
            Content::Text(text) => assert_eq!(text.text, "ශ්‍රී ලංකාව"),
            _ => panic!("text expected"),
        }
        assert!(UserTemplate::from_json("not json").is_none());
    }
}
