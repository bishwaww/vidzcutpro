//! The project: the one thing that is saved to a `.vzp` file.
//!
//! Phase 0 only has the settings (name, canvas, frame rate). Tracks and clips arrive in Phase 1.

use crate::time::{FrameRate, Micros};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version of the `.vzp` file layout. Raise it when the layout changes and add a migration.
pub const FORMAT_VERSION: u32 = 1;

/// Why a project file could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("this is not a valid vidzcut project file: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error(
        "this project was made by a newer vidzcut (file version {found}, this vidzcut understands up to {supported}). Please update vidzcut."
    )]
    TooNew { found: u32, supported: u32 },
}

/// Common canvas shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectPreset {
    Wide16x9,
    Tall9x16,
    Square1x1,
    Photo4x3,
    Portrait3x4,
    Portrait4x5,
    Ultrawide21x9,
    Cinema235,
}

impl AspectPreset {
    pub const ALL: [Self; 8] = [
        Self::Wide16x9,
        Self::Tall9x16,
        Self::Square1x1,
        Self::Photo4x3,
        Self::Portrait3x4,
        Self::Portrait4x5,
        Self::Ultrawide21x9,
        Self::Cinema235,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wide16x9 => "16:9",
            Self::Tall9x16 => "9:16",
            Self::Square1x1 => "1:1",
            Self::Photo4x3 => "4:3",
            Self::Portrait3x4 => "3:4",
            Self::Portrait4x5 => "4:5",
            Self::Ultrawide21x9 => "21:9",
            Self::Cinema235 => "2.35:1",
        }
    }

    /// Canvas size in pixels with a short side of 1080 (as in most editors).
    pub fn size(self) -> (u32, u32) {
        match self {
            Self::Wide16x9 => (1920, 1080),
            Self::Tall9x16 => (1080, 1920),
            Self::Square1x1 => (1080, 1080),
            Self::Photo4x3 => (1440, 1080),
            Self::Portrait3x4 => (1080, 1440),
            Self::Portrait4x5 => (1080, 1350),
            Self::Ultrawide21x9 => (2520, 1080),
            Self::Cinema235 => (2538, 1080),
        }
    }
}

/// Size of the picture in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

impl Canvas {
    pub fn aspect(self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

/// Project settings and (later) everything in the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub id: Uuid,
    pub name: String,
    pub canvas: Canvas,
    pub frame_rate: FrameRate,
    /// Total length of the timeline. Zero while the project is empty.
    pub duration: Micros,
}

impl Project {
    /// A new, empty project: 16:9, 1920x1080, 30 fps.
    pub fn new(name: impl Into<String>) -> Self {
        let (width, height) = AspectPreset::Wide16x9.size();
        Self {
            format_version: FORMAT_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            canvas: Canvas { width, height },
            frame_rate: FrameRate::FPS_30,
            duration: 0,
        }
    }

    pub fn set_aspect(&mut self, preset: AspectPreset) {
        let (width, height) = preset.size();
        self.canvas = Canvas { width, height };
    }

    /// The preset that matches the current canvas, if any.
    pub fn aspect_preset(&self) -> Option<AspectPreset> {
        AspectPreset::ALL
            .into_iter()
            .find(|p| p.size() == (self.canvas.width, self.canvas.height))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a project can always be turned into JSON")
    }

    /// Read a project from JSON, upgrading old file versions. Refuses files from a newer vidzcut.
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        let mut value: serde_json::Value = serde_json::from_str(text)?;
        let found = value
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if found > FORMAT_VERSION {
            return Err(LoadError::TooNew {
                found,
                supported: FORMAT_VERSION,
            });
        }
        migrate(&mut value, found);
        Ok(serde_json::from_value(value)?)
    }
}

/// Upgrade an older file to the current layout, one version at a time.
/// There is nothing to upgrade yet (version 1 is the first); future versions add steps here.
fn migrate(value: &mut serde_json::Value, from: u32) {
    let _ = (value, from);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_defaults() {
        let p = Project::new("My first video");
        assert_eq!((p.canvas.width, p.canvas.height), (1920, 1080));
        assert_eq!(p.frame_rate, FrameRate::FPS_30);
        assert_eq!(p.duration, 0);
        assert_eq!(p.aspect_preset(), Some(AspectPreset::Wide16x9));
    }

    #[test]
    fn save_then_load_gives_the_same_project() {
        let mut p = Project::new("ශ්‍රී ලංකාව — ක්‍රීඩාව");
        p.set_aspect(AspectPreset::Tall9x16);
        p.frame_rate = FrameRate::FPS_29_97;
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
        assert_eq!(back.name, "ශ්‍රී ලංකාව — ක්‍රීඩාව");
    }

    #[test]
    fn newer_files_are_refused_with_a_clear_message() {
        let mut p = Project::new("x");
        p.format_version = FORMAT_VERSION + 1;
        match Project::from_json(&p.to_json()) {
            Err(LoadError::TooNew { found, supported }) => {
                assert_eq!((found, supported), (FORMAT_VERSION + 1, FORMAT_VERSION));
            }
            other => panic!("expected TooNew, got {other:?}"),
        }
    }

    #[test]
    fn garbage_is_refused() {
        assert!(matches!(
            Project::from_json("not json"),
            Err(LoadError::Invalid(_))
        ));
    }

    #[test]
    fn aspect_presets_have_a_1080_short_side() {
        for p in AspectPreset::ALL {
            let (w, h) = p.size();
            assert_eq!(w.min(h), 1080, "{p:?}");
        }
    }
}
