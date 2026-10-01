//! Makes small test videos with the FFmpeg program, so tests do not need real footage.
//! (Used by tests only; the files are made on first use in the system temp folder.)

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

/// What a test video shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    /// Moving test chart with a running clock.
    Chart,
    /// One flat colour, `0xRRGGBB`.
    Solid(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub seconds: u32,
    pub audio_hz: Option<u32>,
    pub pattern: Pattern,
    /// Write a "rotate by this many degrees" note, like phones do.
    pub rotate: u32,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            fps: 25,
            seconds: 4,
            audio_hz: Some(440),
            pattern: Pattern::Chart,
            rotate: 0,
        }
    }
}

/// The FFmpeg program: `VIDZCUT_FFMPEG`, else the one in `third_party`, else whatever is on PATH.
pub fn ffmpeg_program() -> PathBuf {
    if let Some(p) = std::env::var_os("VIDZCUT_FFMPEG") {
        return PathBuf::from(p);
    }
    if let Some(dir) = std::env::var_os("FFMPEG_DIR") {
        let exe = Path::new(&dir).join("bin").join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if exe.exists() {
            return exe;
        }
    }
    PathBuf::from("ffmpeg")
}

/// Make (once) and return the path of a test video.
pub fn video(name: &str, spec: Spec) -> PathBuf {
    let dir = std::env::temp_dir().join("vidzcut-fixtures");
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(&dir).expect("temp folder");
    let out = dir.join(name);
    if out.exists() {
        return out;
    }
    let tmp = dir.join(format!("{name}.partial.mp4"));
    for codec in ["libopenh264", "mpeg4"] {
        let mut cmd = Command::new(ffmpeg_program());
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        let size = format!("{}x{}", spec.width, spec.height);
        match spec.pattern {
            Pattern::Chart => cmd.args([
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=size={size}:rate={}", spec.fps),
            ]),
            Pattern::Solid(rgb) => cmd.args([
                "-f",
                "lavfi",
                "-i",
                &format!("color=c=0x{rgb:06X}:size={size}:rate={}", spec.fps),
            ]),
        };
        if let Some(hz) = spec.audio_hz {
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency={hz}:sample_rate=48000"),
            ]);
        }
        cmd.args([
            "-t",
            &spec.seconds.to_string(),
            "-c:v",
            codec,
            "-pix_fmt",
            "yuv420p",
        ]);
        if spec.audio_hz.is_some() {
            cmd.args(["-c:a", "aac", "-b:a", "128k", "-shortest"]);
        }
        cmd.args(["-f", "mp4"]).arg(&tmp);
        let status = cmd
            .status()
            .expect("could not run FFmpeg to make a test video");
        if status.success() {
            if spec.rotate == 0 {
                std::fs::rename(&tmp, &out).expect("rename test video");
            } else {
                // The rotation note is an input option, so it is added by re-wrapping the finished file.
                let status = Command::new(ffmpeg_program())
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-y",
                        "-display_rotation",
                    ])
                    .arg(spec.rotate.to_string())
                    .arg("-i")
                    .arg(&tmp)
                    .args(["-c", "copy", "-f", "mp4"])
                    .arg(&out)
                    .status()
                    .expect("could not run FFmpeg");
                assert!(status.success(), "could not add the rotation note");
                let _ = std::fs::remove_file(&tmp);
            }
            return out;
        }
    }
    panic!("FFmpeg could not make the test video {name}");
}

/// A standard 4 s, 640x360, 25 fps video with a 440 Hz tone.
pub fn standard() -> PathBuf {
    video("standard_640x360_25fps_4s.mp4", Spec::default())
}
