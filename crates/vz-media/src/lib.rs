//! Probing, decoding, frame cache, thumbnails, waveforms and proxies (FFmpeg).
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md. Phase 0 only proves that FFmpeg links and starts.

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-media";

/// Start FFmpeg (safe to call many times).
pub fn init() -> Result<(), ffmpeg_next::Error> {
    ffmpeg_next::init()
}

/// The version of the FFmpeg library vidzcut is linked with, e.g. `"avformat 63.x"`.
pub fn ffmpeg_version() -> String {
    let v = ffmpeg_next::format::version();
    format!("libavformat {}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff)
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-media");
    }

    #[test]
    fn ffmpeg_starts_and_has_the_encoders_we_need() {
        super::init().expect("FFmpeg should start");
        println!("{}", super::ffmpeg_version());
        // Windows build: hardware encoders (NVIDIA, Intel) and the OpenH264 fallback. macOS: VideoToolbox.
        let needed: &[&str] = if cfg!(windows) {
            &["h264_nvenc", "h264_qsv", "libopenh264", "aac"]
        } else {
            &["aac"]
        };
        for name in needed {
            assert!(
                ffmpeg_next::encoder::find_by_name(name).is_some(),
                "encoder {name} is missing"
            );
        }
        // The Windows build must be the LGPL build: x264 and x265 are GPL and must not be present.
        // (The macOS CI uses Homebrew's FFmpeg only to compile; the release build ships an LGPL one.)
        if cfg!(windows) {
            for name in ["libx264", "libx265"] {
                assert!(
                    ffmpeg_next::encoder::find_by_name(name).is_none(),
                    "{name} (GPL) must not be in the build"
                );
            }
        }
    }
}
