//! The project model: time, tracks, clips, commands with undo/redo, save/load.
//!
//! This crate has no window, no GPU and no FFmpeg, so it can be tested very fast.
//! See docs/ARCHITECTURE.md.

pub mod command;
pub mod content;
pub mod document;
pub mod edit;
pub mod ids;
pub mod layer;
pub mod media;
pub mod project;
pub mod time;
pub mod timeline;

pub use command::{Command, History, MergeKey, TrackFlags};
pub use content::{
    Content, TextAlign, TextBackground, TextContent, TextShadow, TextStroke, TextStyle,
};
pub use document::{
    DocError, Document, EXTENSION, RecoveryItem, kind_accepts_media, track_accepts,
};
pub use edit::EditError;
pub use ids::{ClipId, GroupId, LinkId, MarkerId, MediaId, TimelineId, TrackId};
pub use layer::{BLACK, BlendMode, Color, Crop, PlacedTransform, Transform, WHITE};
pub use media::{DEFAULT_IMAGE_DURATION, MediaAsset, MediaKind};
pub use project::{AspectPreset, Background, Canvas, FORMAT_VERSION, LoadError, Project};
pub use time::{FrameRate, Micros, US_PER_SEC};
pub use timeline::{Clip, Layer, Marker, Timeline, Track, TrackKind};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-core";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-core");
    }
}
