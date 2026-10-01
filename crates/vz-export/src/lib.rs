//! Writing the finished video file.
//!
//! Every frame is drawn by the same renderer the player uses and the sound comes from the same mixer,
//! so the file matches the preview. Encoders are tried in order (graphics-card encoders first, then the
//! software OpenH264) and the first one that works is used.

mod encode;

pub use encode::{Cancelled, ExportError, ExportReport, Progress, export};

use std::path::PathBuf;
use vz_core::{FrameRate, Micros, Project};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-export";

/// How good the picture should be (and so how big the file gets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Lower,
    Recommended,
    Higher,
}

impl Quality {
    pub const ALL: [Self; 3] = [Self::Lower, Self::Recommended, Self::Higher];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lower => "Lower",
            Self::Recommended => "Recommended",
            Self::Higher => "Higher",
        }
    }

    /// Bits per pixel per frame.
    fn bits_per_pixel(self) -> f64 {
        match self {
            Self::Lower => 0.04,
            Self::Recommended => 0.07,
            Self::Higher => 0.12,
        }
    }
}

/// Everything the exporter needs to know.
#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub quality: Quality,
    /// Export only this part of the timeline (start, end). `None` exports everything.
    pub range: Option<(Micros, Micros)>,
}

impl ExportSettings {
    /// Settings for a project at a chosen short side (e.g. 1080 gives 1920x1080 for a 16:9 canvas).
    pub fn for_project(project: &Project, path: PathBuf, short_side: u32) -> Self {
        let (width, height) =
            size_for_short_side(project.canvas.width, project.canvas.height, short_side);
        Self {
            path,
            width,
            height,
            frame_rate: project.frame_rate,
            quality: Quality::Recommended,
            range: None,
        }
    }

    /// Target video bit rate in bits per second.
    pub fn video_bit_rate(&self) -> i64 {
        let pixels = f64::from(self.width) * f64::from(self.height);
        (pixels * self.frame_rate.as_f64() * self.quality.bits_per_pixel()) as i64
    }
}

/// The canvas size scaled so its short side is `short_side` (both sides even).
pub fn size_for_short_side(canvas_w: u32, canvas_h: u32, short_side: u32) -> (u32, u32) {
    let scale = f64::from(short_side) / f64::from(canvas_w.min(canvas_h));
    let even = |v: f64| ((v.round() as u32).max(2) + 1) & !1;
    (
        even(f64::from(canvas_w) * scale),
        even(f64::from(canvas_h) * scale),
    )
}

#[cfg(test)]
mod tests;
