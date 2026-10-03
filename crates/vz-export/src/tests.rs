use super::*;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use vz_core::{Document, Project, US_PER_SEC};
use vz_media::testutil::{self, Pattern, Spec};
use vz_media::{MaxSize, VideoDecoder};
use vz_render::Renderer;

fn temp_out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("vidzcut-export-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    let _ = std::fs::remove_file(&p);
    p
}

fn doc_with(files: &[PathBuf]) -> Document {
    let mut doc = Document::new(Project::new("export test"));
    for path in files {
        let id = doc.import_media(vz_media::probe(path).unwrap()).unwrap();
        doc.add_clip(id, usize::MAX).unwrap();
    }
    doc
}

fn ffprobe() -> PathBuf {
    let ffmpeg = testutil::ffmpeg_program();
    let name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    match ffmpeg.parent() {
        Some(dir) if dir.join(name).exists() => dir.join(name),
        _ => PathBuf::from("ffprobe"),
    }
}

/// Ask ffprobe about a file.
fn probe_json(path: &Path) -> serde_json::Value {
    let out = Command::new(ffprobe())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .output()
        .expect("could not run ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn stream<'a>(info: &'a serde_json::Value, kind: &str) -> &'a serde_json::Value {
    info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
        .unwrap_or_else(|| panic!("no {kind} stream"))
}

fn number(v: &serde_json::Value) -> f64 {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_f64())
        .unwrap()
}

fn export_to(doc: &Document, name: &str, short_side: u32) -> (ExportReport, PathBuf) {
    let out = temp_out(name);
    let settings = ExportSettings::for_project(&doc.project, out.clone(), short_side);
    let cancel = AtomicBool::new(false);
    let mut last = None;
    let report = export(&doc.project, &settings, &mut |p| last = Some(p), &cancel)
        .expect("export should work");
    let last = last.expect("progress was reported");
    assert_eq!(
        last.frames_done, last.frames_total,
        "progress reaches 100 %"
    );
    (report, out)
}

#[test]
fn sizes_for_a_short_side() {
    assert_eq!(size_for_short_side(1920, 1080, 1080), (1920, 1080));
    assert_eq!(size_for_short_side(1920, 1080, 720), (1280, 720));
    assert_eq!(size_for_short_side(1080, 1920, 1080), (1080, 1920));
    assert_eq!(size_for_short_side(1920, 1080, 2160), (3840, 2160));
}

#[test]
fn bit_rate_grows_with_size_and_quality() {
    let p = Project::new("x");
    let mut s = ExportSettings::for_project(&p, "x.mp4".into(), 1080);
    let normal = s.video_bit_rate();
    assert!(
        (3_000_000..8_000_000).contains(&normal),
        "1080p30 should be a few Mbps, was {normal}"
    );
    s.quality = Quality::Higher;
    assert!(s.video_bit_rate() > normal);
    s.quality = Quality::Lower;
    assert!(s.video_bit_rate() < normal);
}

#[test]
fn exports_a_playable_mp4_with_the_right_shape_and_sound() {
    let chart = testutil::standard(); // 4 s, 640x360, 25 fps, 440 Hz
    let mut doc = doc_with(&[chart]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let (report, out) = export_to(&doc, "basic.mp4", 360);
    println!("encoded with {} in {:.2}s", report.encoder, report.seconds);
    assert!(report.bytes > 10_000);

    let info = probe_json(&out);
    let v = stream(&info, "video");
    let a = stream(&info, "audio");
    assert_eq!(v["codec_name"], "h264");
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(640), Some(360))
    );
    assert_eq!(v["r_frame_rate"], "25/1");
    assert_eq!(v["pix_fmt"], "yuv420p");
    assert_eq!(a["codec_name"], "aac");
    assert_eq!(number(&a["sample_rate"]), 48_000.0);
    assert_eq!(a["channels"], 2);
    let duration = number(&info["format"]["duration"]);
    assert!((duration - 4.0).abs() < 0.15, "duration was {duration}");
    // picture and sound start together (AAC adds a small priming delay)
    let (vs, as_) = (number(&v["start_time"]), number(&a["start_time"]));
    assert!(
        (vs - as_).abs() < 0.08,
        "video starts at {vs}, audio at {as_}"
    );
}

#[test]
fn the_file_matches_the_preview() {
    let red = testutil::video(
        "solid_red_rt.mp4",
        Spec {
            pattern: Pattern::Solid(0xFF0000),
            seconds: 2,
            ..Spec::default()
        },
    );
    let blue = testutil::video(
        "solid_blue_rt.mp4",
        Spec {
            pattern: Pattern::Solid(0x0000FF),
            seconds: 2,
            ..Spec::default()
        },
    );
    let mut doc = doc_with(&[red, blue]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let (_, out) = export_to(&doc, "colours.mp4", 360);

    let mut file = VideoDecoder::open(&out, MaxSize::FULL).unwrap();
    let mut preview = Renderer::new();
    for t in [500_000, 1_500_000, 2_500_000, 3_500_000] {
        let from_file = file.frame_at(t).unwrap().unwrap();
        let expected = preview.render(&doc.project, t, (640, 360));
        let diff: f64 = from_file
            .rgba
            .iter()
            .zip(&expected.rgba)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .sum::<f64>()
            / from_file.rgba.len() as f64;
        assert!(
            diff < 8.0,
            "frame at {t} differs from the preview by {diff} levels on average"
        );
    }
}

#[test]
fn trimmed_and_split_edits_are_what_gets_exported() {
    let chart = testutil::standard();
    let mut doc = doc_with(&[chart.clone(), chart]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let first = doc.project.timeline().main_track().clips[0].id;
    doc.trim_right(first, US_PER_SEC, None).unwrap(); // 1 s + 4 s
    assert_eq!(doc.project.duration(), 5 * US_PER_SEC);
    let (_, out) = export_to(&doc, "edited.mp4", 360);
    let duration = number(&probe_json(&out)["format"]["duration"]);
    assert!((duration - 5.0).abs() < 0.15, "duration was {duration}");
}

#[test]
fn a_range_exports_only_that_part() {
    let doc = doc_with(&[testutil::standard()]);
    let out = temp_out("range.mp4");
    let mut settings = ExportSettings::for_project(&doc.project, out.clone(), 360);
    settings.range = Some((US_PER_SEC, 3 * US_PER_SEC));
    export(
        &doc.project,
        &settings,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let duration = number(&probe_json(&out)["format"]["duration"]);
    assert!((duration - 2.0).abs() < 0.15, "duration was {duration}");
}

#[test]
fn an_empty_timeline_is_refused_politely() {
    let doc = Document::new(Project::new("empty"));
    let settings = ExportSettings::for_project(&doc.project, temp_out("empty.mp4"), 360);
    let r = export(
        &doc.project,
        &settings,
        &mut |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(r, Err(ExportError::Empty)));
}

#[test]
fn cancelling_stops_and_leaves_no_file() {
    let doc = doc_with(&[testutil::standard()]);
    let out = temp_out("cancelled.mp4");
    let settings = ExportSettings::for_project(&doc.project, out.clone(), 360);
    let cancel = AtomicBool::new(false);
    let r = export(
        &doc.project,
        &settings,
        &mut |p| {
            if p.frames_done > 8 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(r, Err(ExportError::Cancelled)));
    assert!(!out.exists(), "no finished file");
    assert!(
        !partial_path(&out).exists(),
        "no half-written file left behind"
    );
}

use std::path::Path;
use std::sync::atomic::Ordering;

fn partial_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".partial");
    PathBuf::from(p)
}

/// Manual check with real footage: `VIDZCUT_REAL_VIDEO=path cargo test -p vz-export real_footage -- --ignored --nocapture`.
/// Exports 10 seconds from 60 s into the file at 1080p and reports the speed. Reads the file, never changes it.
#[test]
#[ignore = "needs a real video: set VIDZCUT_REAL_VIDEO"]
fn real_footage_exports_quickly_and_correctly() {
    let Some(path) = std::env::var_os("VIDZCUT_REAL_VIDEO").map(PathBuf::from) else {
        eprintln!("set VIDZCUT_REAL_VIDEO to try this");
        return;
    };
    let mut doc = doc_with(&[path]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_30).unwrap();
    let id = doc.project.timeline().main_track().clips[0].id;
    doc.trim_left(id, 60 * US_PER_SEC, None).unwrap();
    doc.trim_right(id, 10 * US_PER_SEC, None).unwrap();
    let (report, out) = export_to(&doc, "real.mp4", 1080);
    let fps = report.frames as f64 / report.seconds;
    println!(
        "REAL FOOTAGE: {} frames with {} in {:.1}s = {:.0} fps, {:.1} MB",
        report.frames,
        report.encoder,
        report.seconds,
        fps,
        report.bytes as f64 / 1e6
    );
    let info = probe_json(&out);
    let v = stream(&info, "video");
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(1920), Some(1080))
    );
    assert!((number(&info["format"]["duration"]) - 10.0).abs() < 0.2);
}

#[test]
fn text_shapes_and_overlays_are_in_the_exported_file() {
    use vz_core::{ShapeContent, ShapeKind, TextContent};
    let green = testutil::video(
        "solid_green_layers.mp4",
        Spec {
            pattern: Pattern::Solid(0x00A000),
            seconds: 3,
            ..Spec::default()
        },
    );
    let mut doc = doc_with(&[green]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let mut headline = TextContent::new("ශ්‍රී ලංකාව");
    headline.style.size = 160.0;
    headline.style.bold = true;
    doc.add_text_auto(0, 3 * US_PER_SEC, headline).unwrap();
    let mut star = ShapeContent::new(ShapeKind::Star);
    star.fill = Some([255, 0, 0, 255]);
    doc.add_shape_auto(US_PER_SEC, 2 * US_PER_SEC, star)
        .unwrap();

    let (_, out) = export_to(&doc, "layers.mp4", 360);
    let mut file = VideoDecoder::open(&out, MaxSize::FULL).unwrap();
    let mut preview = Renderer::new();
    for t in [500_000, 2_000_000] {
        let from_file = file.frame_at(t).unwrap().unwrap();
        let expected = preview.render(&doc.project, t, (640, 360));
        let diff: f64 = from_file
            .rgba
            .iter()
            .zip(&expected.rgba)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .sum::<f64>()
            / from_file.rgba.len() as f64;
        assert!(
            diff < 8.0,
            "frame at {t} differs from the preview by {diff}"
        );
        // the text (white) is in the file
        let white = from_file
            .rgba
            .chunks(4)
            .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
            .count();
        assert!(white > 300, "text missing at {t}: {white} white pixels");
    }
    // the red star only appears after one second
    let early = file.frame_at(500_000).unwrap().unwrap();
    let later = file.frame_at(2_000_000).unwrap().unwrap();
    let red = |f: &vz_media::VideoFrame| {
        f.rgba
            .chunks(4)
            .filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60)
            .count()
    };
    assert_eq!(red(&early), 0);
    assert!(red(&later) > 500, "{}", red(&later));
}

#[test]
fn speed_transitions_and_animations_are_in_the_exported_file() {
    let chart = testutil::standard(); // 4 s with sound
    let mut doc = doc_with(&[chart.clone(), chart]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let ids: Vec<_> = doc
        .project
        .timeline()
        .main_track()
        .clips
        .iter()
        .map(|c| c.id)
        .collect();
    // the first clip at double speed is 2 s long, so the project is 6 s
    doc.set_speed(ids[0], 2.0, None).unwrap();
    doc.set_transition(
        ids[0],
        Some(vz_core::Transition {
            after: ids[0],
            preset: "dissolve".into(),
            duration: US_PER_SEC,
            easing: vz_anim::Easing::Linear,
        }),
        None,
    )
    .unwrap();
    doc.update_clip(ids[1], "a", None, |c| {
        c.anim_out = Some(vz_core::AnimRef {
            preset: "fade_out".into(),
            duration: US_PER_SEC,
        });
    })
    .unwrap();
    let (_, out) = export_to(&doc, "retimed.mp4", 360);
    let info = probe_json(&out);
    let duration = number(&info["format"]["duration"]);
    assert!((duration - 6.0).abs() < 0.2, "duration was {duration}");
    assert_eq!(stream(&info, "audio")["codec_name"], "aac");

    // the file shows what the preview shows: during the transition, and in the fade-out at the end
    let mut file = VideoDecoder::open(&out, MaxSize::FULL).unwrap();
    let mut preview = Renderer::new();
    for t in [1_800_000, 2_000_000, 5_700_000] {
        let from_file = file.frame_at(t).unwrap().unwrap();
        let expected = preview.render(&doc.project, t, (640, 360));
        let diff: f64 = from_file
            .rgba
            .iter()
            .zip(&expected.rgba)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .sum::<f64>()
            / from_file.rgba.len() as f64;
        assert!(
            diff < 10.0,
            "frame at {t} differs from the preview by {diff}"
        );
    }
}

#[test]
fn colour_effects_masks_and_adjustment_layers_are_in_the_exported_file() {
    use vz_core::{Adjust, EffectRef, MaskDef, MaskShape};
    let chart = testutil::standard();
    let mut doc = doc_with(&[chart]);
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let id = doc.project.timeline().main_track().clips[0].id;
    doc.update_clip(id, "look", None, |c| {
        c.adjust.contrast = 40.0;
        c.adjust.vignette = 60.0;
        c.masks = vec![MaskDef::new(MaskShape::Circle)];
    })
    .unwrap();
    let bw = EffectRef {
        preset: "black_white".into(),
        params: vec![100.0],
        enabled: true,
    };
    doc.add_adjustment_auto(US_PER_SEC, 2 * US_PER_SEC, Adjust::default(), vec![bw])
        .unwrap();
    let (_, out) = export_to(&doc, "looks.mp4", 360);
    let mut file = VideoDecoder::open(&out, MaxSize::FULL).unwrap();
    let mut preview = Renderer::new();
    for t in [500_000, 1_500_000, 3_000_000] {
        let from_file = file.frame_at(t).unwrap().unwrap();
        let expected = preview.render(&doc.project, t, (640, 360));
        let diff: f64 = from_file
            .rgba
            .iter()
            .zip(&expected.rgba)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .sum::<f64>()
            / from_file.rgba.len() as f64;
        assert!(
            diff < 10.0,
            "frame at {t} differs from the preview by {diff}"
        );
    }
    // inside the black-and-white layer the picture really has no colour
    let frame = file.frame_at(1_500_000).unwrap().unwrap();
    let colourful = frame
        .rgba
        .chunks(4)
        .filter(|p| {
            (i32::from(p[0]) - i32::from(p[1])).abs() > 40
                || (i32::from(p[1]) - i32::from(p[2])).abs() > 40
        })
        .count();
    assert!(
        colourful * 50 < frame.rgba.len() / 4,
        "{colourful} coloured pixels"
    );
}
