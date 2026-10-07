//! Ready-made export settings for the places people post to.

use crate::{Quality, VideoCodec};
use vz_core::FrameRate;

/// One ready-made choice in the export window.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub name: &'static str,
    /// Shorter side of the picture in pixels (1080 gives 1920x1080 or 1080x1920).
    pub short_side: u32,
    /// `None` keeps the project's frame rate.
    pub frame_rate: Option<FrameRate>,
    pub quality: Quality,
    pub codec: VideoCodec,
    /// The shape of picture the place wants, as width : height. The picture shape comes from the project's
    /// Aspect ratio, so the window warns when it differs.
    pub aspect: Option<(u32, u32)>,
    pub note: &'static str,
}

pub const PRESETS: [Preset; 7] = [
    Preset {
        name: "YouTube 1080p",
        short_side: 1080,
        frame_rate: None,
        quality: Quality::Recommended,
        codec: VideoCodec::H264,
        aspect: Some((16, 9)),
        note: "Full HD for YouTube.",
    },
    Preset {
        name: "YouTube 4K",
        short_side: 2160,
        frame_rate: None,
        quality: Quality::Higher,
        codec: VideoCodec::H264,
        aspect: Some((16, 9)),
        note: "Ultra HD. Big file and slow to make.",
    },
    Preset {
        name: "TikTok / Reels / Shorts",
        short_side: 1080,
        frame_rate: Some(FrameRate::FPS_30),
        quality: Quality::Recommended,
        codec: VideoCodec::H264,
        aspect: Some((9, 16)),
        note: "Tall 9:16 video, 1080 x 1920.",
    },
    Preset {
        name: "Facebook feed",
        short_side: 720,
        frame_rate: Some(FrameRate::FPS_30),
        quality: Quality::Recommended,
        codec: VideoCodec::H264,
        aspect: None,
        note: "720p, light enough to upload fast.",
    },
    Preset {
        name: "Instagram square 1:1",
        short_side: 1080,
        frame_rate: Some(FrameRate::FPS_30),
        quality: Quality::Recommended,
        codec: VideoCodec::H264,
        aspect: Some((1, 1)),
        note: "Square, 1080 x 1080.",
    },
    Preset {
        name: "Instagram portrait 4:5",
        short_side: 1080,
        frame_rate: Some(FrameRate::FPS_30),
        quality: Quality::Recommended,
        codec: VideoCodec::H264,
        aspect: Some((4, 5)),
        note: "Portrait, 1080 x 1350.",
    },
    Preset {
        name: "WhatsApp small file",
        short_side: 480,
        frame_rate: Some(FrameRate::FPS_30),
        quality: Quality::Lower,
        codec: VideoCodec::H264,
        aspect: None,
        note: "Small file that is quick to send.",
    },
];

impl Preset {
    /// Does a canvas of this size have the shape the preset wants? (Always true when it does not care.)
    pub fn fits_canvas(&self, width: u32, height: u32) -> bool {
        match self.aspect {
            None => true,
            Some((w, h)) => {
                (f64::from(width) / f64::from(height) - f64::from(w) / f64::from(h)).abs() < 0.02
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_know_which_canvas_shape_they_want() {
        let tiktok = PRESETS[2];
        assert!(tiktok.fits_canvas(1080, 1920));
        assert!(!tiktok.fits_canvas(1920, 1080));
        assert!(PRESETS[3].fits_canvas(1920, 1080));
        assert!(PRESETS[5].fits_canvas(1080, 1350));
    }
}
