//! Probing, decoding, frame cache, thumbnails, waveforms and proxies (FFmpeg).
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md.

pub mod decoder;
pub mod error;
pub mod probe;
pub mod testutil;
pub mod worker;

pub use decoder::{FrameSource, MaxSize, VideoDecoder, VideoFrame};
pub use error::{MediaError, Result};
pub use probe::{is_image_path, probe};
pub use worker::{Event, MediaWorker};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-media";

/// Start FFmpeg (safe to call many times).
pub fn init() -> std::result::Result<(), ffmpeg_next::Error> {
    ffmpeg_next::init()
}

/// The version of the FFmpeg library vidzcut is linked with, e.g. `"libavformat 63.1.102"`.
pub fn ffmpeg_version() -> String {
    let v = ffmpeg_next::format::version();
    format!("libavformat {}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff)
}

#[cfg(test)]
mod tests {
    use super::testutil::{self, Pattern, Spec};
    use super::*;
    use vz_core::{FrameRate, MediaKind, US_PER_SEC};

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

    #[test]
    fn probe_reads_a_video() {
        let a = probe(&testutil::standard()).unwrap();
        assert_eq!(a.kind, MediaKind::Video);
        assert_eq!((a.width, a.height), (640, 360));
        assert_eq!(a.frame_rate, Some(FrameRate::FPS_25));
        assert!(a.has_audio);
        assert!(
            (a.duration - 4 * US_PER_SEC).abs() < 100_000,
            "duration {}",
            a.duration
        );
        assert_eq!(a.name, "standard_640x360_25fps_4s.mp4");
    }

    #[test]
    fn probe_reports_a_video_without_sound() {
        let p = testutil::video(
            "silent_320x240.mp4",
            Spec {
                width: 320,
                height: 240,
                audio_hz: None,
                seconds: 2,
                ..Spec::default()
            },
        );
        let a = probe(&p).unwrap();
        assert!(!a.has_audio);
        assert_eq!((a.width, a.height), (320, 240));
    }

    #[test]
    fn probe_turns_phone_videos_upright() {
        let p = testutil::video(
            "phone_rotated.mp4",
            Spec {
                width: 640,
                height: 360,
                rotate: 90,
                seconds: 1,
                ..Spec::default()
            },
        );
        let a = probe(&p).unwrap();
        assert_eq!(
            (a.width, a.height),
            (360, 640),
            "a sideways phone video is reported upright"
        );
    }

    #[test]
    fn probe_refuses_a_file_that_is_not_media() {
        let path = std::env::temp_dir().join("vidzcut-not-media.txt");
        std::fs::write(&path, "hello").unwrap();
        assert!(probe(&path).is_err());
        assert!(probe(std::path::Path::new("/no/such/file.mp4")).is_err());
    }

    #[test]
    fn frames_are_found_by_time() {
        let mut d = VideoDecoder::open(&testutil::standard(), MaxSize::FULL).unwrap();
        let frame = 40_000; // 25 fps
        for t in [
            0, 40_000, 1_000_000, 1_019_999, 1_020_000, 3_960_000, 500_000, 2_000_000,
        ] {
            let f = d.frame_at(t).unwrap().expect("a frame");
            assert!(
                f.pts <= t && t < f.pts + frame,
                "asked {t}, got a frame at {}",
                f.pts
            );
            assert_eq!((f.width, f.height), (640, 360));
        }
    }

    #[test]
    fn playing_forward_visits_every_frame_once() {
        let mut d = VideoDecoder::open(&testutil::standard(), MaxSize::FULL).unwrap();
        let mut last = -1;
        let mut count = 0;
        for n in 0..100 {
            let f = d.frame_at(n * 40_000 + 5_000).unwrap().unwrap();
            assert!(f.pts > last, "frame {n} repeated or went backwards");
            last = f.pts;
            count += 1;
        }
        assert_eq!(count, 100);
    }

    #[test]
    fn a_time_past_the_end_gives_the_last_frame() {
        let mut d = VideoDecoder::open(&testutil::standard(), MaxSize::FULL).unwrap();
        let f = d.frame_at(60 * US_PER_SEC).unwrap().unwrap();
        assert!(f.pts >= 3_900_000, "got {}", f.pts);
    }

    #[test]
    fn colours_come_out_in_the_right_order() {
        let p = testutil::video(
            "solid_red.mp4",
            Spec {
                pattern: Pattern::Solid(0xFF0000),
                audio_hz: None,
                seconds: 1,
                width: 128,
                height: 72,
                ..Spec::default()
            },
        );
        let mut d = VideoDecoder::open(&p, MaxSize::FULL).unwrap();
        let f = d.frame_at(100_000).unwrap().unwrap();
        let mid = ((36 * 128 + 64) * 4) as usize;
        let (r, g, b, a) = (
            f.rgba[mid],
            f.rgba[mid + 1],
            f.rgba[mid + 2],
            f.rgba[mid + 3],
        );
        assert!(
            r > 200 && g < 60 && b < 60 && a == 255,
            "pixel was rgba({r},{g},{b},{a}), expected red"
        );
    }

    #[test]
    fn phone_videos_are_decoded_upright() {
        let p = testutil::video(
            "phone_rotated.mp4",
            Spec {
                width: 640,
                height: 360,
                rotate: 90,
                seconds: 1,
                ..Spec::default()
            },
        );
        let mut d = VideoDecoder::open(&p, MaxSize::FULL).unwrap();
        let f = d.frame_at(0).unwrap().unwrap();
        assert_eq!((f.width, f.height), (360, 640));
        assert_eq!(f.rgba.len(), 360 * 640 * 4);
    }

    #[test]
    fn decoded_size_is_limited_but_keeps_the_shape() {
        let mut d = VideoDecoder::open(&testutil::standard(), MaxSize::new(320, 320)).unwrap();
        let f = d.frame_at(0).unwrap().unwrap();
        assert_eq!((f.width, f.height), (320, 180));
    }

    #[test]
    fn still_images_are_one_frame_for_any_time() {
        let path = std::env::temp_dir().join("vidzcut-test-image.png");
        image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 200, 30, 255]))
            .save(&path)
            .unwrap();
        let a = probe(&path).unwrap();
        assert_eq!(
            (a.kind, a.width, a.height, a.duration),
            (MediaKind::Image, 40, 20, 0)
        );
        let mut s = FrameSource::open(&path, MaxSize::FULL).unwrap();
        for t in [0, 5 * US_PER_SEC] {
            let f = s.frame_at(t).unwrap().unwrap();
            assert_eq!(&f.rgba[..4], &[10, 200, 30, 255]);
        }
    }

    #[test]
    fn the_worker_imports_and_makes_a_thumbnail() {
        let worker = MediaWorker::start(|| {});
        worker.import(testutil::standard());
        worker.import("/no/such/file.mp4".into());
        let mut imported = 0;
        let mut failed = 0;
        for _ in 0..200 {
            for e in worker.poll() {
                match e {
                    Event::Imported { asset, poster } => {
                        imported += 1;
                        let p = poster.expect("a thumbnail");
                        assert_eq!((p.width, p.height), (320, 180));
                        assert_eq!(asset.kind, MediaKind::Video);
                    }
                    Event::ImportFailed { .. } => failed += 1,
                    Event::Poster { .. } => {}
                }
            }
            if imported + failed == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!((imported, failed), (1, 1));
    }

    #[test]
    fn hd_footage_keeps_its_colours() {
        // an orange picture encoded with the HD (BT.709) matrix must come back orange, not shifted
        let p = testutil::video(
            "hd_bt709_orange.mp4",
            Spec {
                pattern: Pattern::Solid(0xE08030),
                width: 1280,
                height: 720,
                audio_hz: None,
                seconds: 1,
                bt709: true,
                ..Spec::default()
            },
        );
        let mut d = VideoDecoder::open(&p, MaxSize::FULL).unwrap();
        let f = d.frame_at(200_000).unwrap().unwrap();
        let mid = ((360 * 1280 + 640) * 4) as usize;
        let (r, g, b) = (
            i32::from(f.rgba[mid]),
            i32::from(f.rgba[mid + 1]),
            i32::from(f.rgba[mid + 2]),
        );
        assert!(
            (r - 0xE0).abs() <= 6 && (g - 0x80).abs() <= 6 && (b - 0x30).abs() <= 6,
            "expected about (224, 128, 48), got ({r}, {g}, {b})"
        );
    }

    #[test]
    fn going_backwards_gives_the_same_pictures_as_going_forwards() {
        let path = testutil::standard();
        let size = MaxSize::new(160, 90);
        let times: Vec<i64> = (0..75).map(|i| i * 40_000).collect();
        let mut forward = VideoDecoder::open(&path, size).unwrap();
        let expected: Vec<Vec<u8>> = times
            .iter()
            .map(|t| forward.frame_at(*t).unwrap().unwrap().rgba.clone())
            .collect();
        let mut backward = VideoDecoder::open(&path, size).unwrap();
        let started = std::time::Instant::now();
        for (i, t) in times.iter().enumerate().rev() {
            let frame = backward.frame_at(*t).unwrap().unwrap();
            assert_eq!(frame.rgba, expected[i], "picture at {t} differs");
        }
        // a run of pictures is decoded once, not seeked for every step
        assert!(
            started.elapsed().as_secs_f32() < 5.0,
            "{:?}",
            started.elapsed()
        );
    }
}
