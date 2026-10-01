//! The project model: time, tracks, clips, commands with undo/redo, save/load.
//!
//! This crate has no window, no GPU and no FFmpeg, so it can be tested very fast.
//! See docs/ARCHITECTURE.md.

pub mod project;
pub mod time;

pub use project::{AspectPreset, Canvas, FORMAT_VERSION, LoadError, Project};
pub use time::{FrameRate, Micros, US_PER_SEC};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-core";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-core");
    }
}
