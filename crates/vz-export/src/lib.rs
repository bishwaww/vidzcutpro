//! Writing the finished video file.
//!
//! Every frame is drawn by the same renderer the player uses and the sound comes from the same mixer,
//! so the file matches the preview. Encoders are tried in order (graphics-card encoders first, then the
//! software OpenH264) and the first one that works is used.

mod audio;
mod encode;
mod gif;
mod picture;
mod presets;

pub use audio::{AudioExportSettings, AudioFormat, export_audio};
pub use encode::{Cancelled, ExportError, ExportReport, Progress, export};
pub use gif::{Dither, GifSettings, export_gif};
pub use picture::{PictureFormat, save_picture};
pub use presets::{PRESETS, Preset};

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

/// The video format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoCodec {
    #[default]
    H264,
    /// Smaller files for the same quality; needs a graphics-card encoder or the software one in our FFmpeg.
    Hevc,
}

impl VideoCodec {
    pub const ALL: [Self; 2] = [Self::H264, Self::Hevc];

    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264 (works everywhere)",
            Self::Hevc => "HEVC / H.265 (smaller files)",
        }
    }

    /// Is any encoder for this codec on this computer?
    pub fn is_available(self) -> bool {
        let _ = vz_media::init();
        !encode::encoder_candidates(self).is_empty()
    }

    /// How much of the H.264 bit rate gives the same look.
    fn bit_rate_factor(self) -> f64 {
        match self {
            Self::H264 => 1.0,
            Self::Hevc => 0.7,
        }
    }
}

/// The file type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Container {
    #[default]
    Mp4,
    Mov,
}

impl Container {
    pub const ALL: [Self; 2] = [Self::Mp4, Self::Mov];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
        }
    }

    fn format_name(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
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
    /// A video bit rate chosen by the user, in kilobits per second. Overrides `quality`.
    pub custom_kbps: Option<u32>,
    pub codec: VideoCodec,
    pub container: Container,
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
            custom_kbps: None,
            codec: VideoCodec::H264,
            container: Container::Mp4,
            range: None,
        }
    }

    /// Target video bit rate in bits per second.
    pub fn video_bit_rate(&self) -> i64 {
        if let Some(k) = self.custom_kbps {
            return i64::from(k) * 1000;
        }
        let pixels = f64::from(self.width) * f64::from(self.height);
        (pixels
            * self.frame_rate.as_f64()
            * self.quality.bits_per_pixel()
            * self.codec.bit_rate_factor()) as i64
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
