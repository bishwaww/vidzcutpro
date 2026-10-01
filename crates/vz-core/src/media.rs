//! Imported media files, as the project remembers them.

use crate::ids::MediaId;
use crate::time::{FrameRate, Micros, US_PER_SEC};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

/// How long a still image lasts when it is first put on the timeline.
pub const DEFAULT_IMAGE_DURATION: Micros = 5 * US_PER_SEC;

/// A file the user imported. The project stores where it is and what is in it, never the file itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: MediaId,
    /// Name shown in the media grid (the file name).
    pub name: String,
    /// Where the file was when it was imported.
    pub path: PathBuf,
    /// Path relative to the project file, if the file lives under the project's folder.
    /// Used to find the file again when a project folder is moved.
    #[serde(default)]
    pub relative_path: Option<PathBuf>,
    pub kind: MediaKind,
    /// Length of the media. Zero for still images (they can be any length).
    pub duration: Micros,
    pub width: u32,
    pub height: u32,
    /// Frame rate of the video stream, if any.
    #[serde(default)]
    pub frame_rate: Option<FrameRate>,
    pub has_audio: bool,
    #[serde(default)]
    pub file_size: u64,
}

impl MediaAsset {
    /// The longest a clip of this media can be (images have no limit).
    pub fn max_clip_duration(&self) -> Option<Micros> {
        match self.kind {
            MediaKind::Image => None,
            _ => Some(self.duration),
        }
    }

    /// The length a clip gets when first added.
    pub fn default_clip_duration(&self) -> Micros {
        match self.kind {
            MediaKind::Image => DEFAULT_IMAGE_DURATION,
            _ => self.duration,
        }
    }

    pub fn has_picture(&self) -> bool {
        matches!(self.kind, MediaKind::Video | MediaKind::Image)
    }
}
