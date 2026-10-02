//! Whole-project rendering tests: overlays, transforms, opacity, blend modes and backgrounds.

use super::*;
use vz_core::{Background, BlendMode, Document, Project, TrackKind, US_PER_SEC};
use vz_media::testutil::{self, Pattern, Spec};

fn solid(name: &str, rgb: u32, w: u32, h: u32) -> PathBuf {
    testutil::video(
        name,
        Spec {
            pattern: Pattern::Solid(rgb),
            width: w,
            height: h,
            audio_hz: None,
            seconds: 3,
            ..Spec::default()
        },
    )
}

fn pixel(f: &VideoFrame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

fn near(p: [u8; 4], rgb: [u8; 3], tolerance: i32) -> bool {
    (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tolerance)
}

struct Scene {
    doc: Document,
    overlay_track: vz_core::TrackId,
}

/// A 320x180 canvas with a solid green main clip and an empty overlay track.
fn scene() -> Scene {
    let green = solid("lay_green_320x180.mp4", 0x00C800, 320, 180);
    let mut doc = Document::new(Project::new("layers"));
    let id = doc.import_media(vz_media::probe(&green).unwrap()).unwrap();
    doc.add_clip(id, 0).unwrap();
    doc.set_frame_rate(vz_core::FrameRate::FPS_25).unwrap();
    let overlay_track = doc.add_track(TrackKind::Overlay).unwrap();
    Scene { doc, overlay_track }
}

/// Put a solid-colour square-ish video on the overlay track and return its clip id.
fn add_overlay(s: &mut Scene, name: &str, rgb: u32, w: u32, h: u32) -> vz_core::ClipId {
    let path = solid(name, rgb, w, h);
    let m = s.doc.import_media(vz_media::probe(&path).unwrap()).unwrap();
    s.doc.place_media(s.overlay_track, m, 0).unwrap()
}

const SIZE: (u32, u32) = (320, 180);

#[test]
fn an_overlay_is_drawn_above_the_main_track() {
    let mut s = scene();
    add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(
        near(pixel(&f, 160, 90), [255, 0, 0], 12),
        "{:?}",
        pixel(&f, 160, 90)
    );
}

#[test]
fn scale_and_position_place_the_overlay() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| {
            c.transform.scale_x = 0.25.into();
            c.transform.scale_y = 0.25.into();
            // positions are in canvas pixels (1920 x 1080); this render is 6 times smaller
            c.transform.x = 600.0.into(); // 100 pixels to the right in the render: centre at x = 260
            c.transform.y = (-300.0).into(); // 50 pixels up in the render
        })
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(
        near(pixel(&f, 260, 40), [255, 0, 0], 12),
        "the small red square is at its new place"
    );
    assert!(
        near(pixel(&f, 160, 90), [0, 200, 0], 12),
        "the green background shows elsewhere"
    );
    assert!(near(pixel(&f, 20, 150), [0, 200, 0], 12));
    // the square is 80 x 45: 260 +- 40 and 40 +- 22
    assert!(
        near(pixel(&f, 215, 40), [0, 200, 0], 12),
        "just left of the square"
    );
    assert!(
        near(pixel(&f, 305, 40), [0, 200, 0], 12),
        "just right of the square"
    );
    assert!(
        near(pixel(&f, 297, 40), [255, 0, 0], 12),
        "just inside the right edge"
    );
}

#[test]
fn opacity_mixes_the_overlay_with_what_is_below() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| c.opacity = 0.5.into())
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    let p = pixel(&f, 160, 90);
    assert!(near(p, [128, 100, 0], 14), "half red over green, was {p:?}");
}

#[test]
fn a_blend_mode_changes_the_result() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| c.blend = BlendMode::Multiply)
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(
        near(pixel(&f, 160, 90), [0, 0, 0], 14),
        "red x green = black, was {:?}",
        pixel(&f, 160, 90)
    );
}

#[test]
fn crop_removes_part_of_the_overlay() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| c.crop.left = 0.5)
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(
        near(pixel(&f, 40, 90), [0, 200, 0], 12),
        "left half cropped away"
    );
    assert!(near(pixel(&f, 280, 90), [255, 0, 0], 12));
}

#[test]
fn a_hidden_overlay_track_is_not_drawn() {
    let mut s = scene();
    add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .set_track_flags(s.overlay_track, |f| f.hidden = true)
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(near(pixel(&f, 160, 90), [0, 200, 0], 12));
}

#[test]
fn overlays_only_show_while_their_clip_plays() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc.trim_right(id, US_PER_SEC, None).unwrap();
    let mut r = Renderer::new();
    assert!(near(
        pixel(&r.render(&s.doc.project, US_PER_SEC / 2, SIZE), 160, 90),
        [255, 0, 0],
        12
    ));
    assert!(near(
        pixel(&r.render(&s.doc.project, 2 * US_PER_SEC, SIZE), 160, 90),
        [0, 200, 0],
        12
    ));
}

#[test]
fn the_background_colour_shows_where_a_clip_does_not_reach() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_160x90.mp4", 0xFF0000, 160, 90);
    s.doc
        .update_clip(id, "x", None, |c| c.transform.scale_x = 0.5.into())
        .unwrap();
    // a portrait canvas: the 16:9 green clip leaves bars above and below
    s.doc.set_aspect(vz_core::AspectPreset::Tall9x16).unwrap();
    s.doc
        .set_background(Background::Color([0, 0, 255, 255]))
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, (180, 320));
    assert!(
        near(pixel(&f, 90, 5), [0, 0, 255], 4),
        "blue bar at the top, was {:?}",
        pixel(&f, 90, 5)
    );
    assert!(near(pixel(&f, 5, 315), [0, 0, 255], 4));
}

#[test]
fn a_blurred_background_fills_the_bars_with_the_clip_colours() {
    let mut s = scene();
    s.doc.set_aspect(vz_core::AspectPreset::Tall9x16).unwrap();
    s.doc.set_background(Background::Blur(0.5)).unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, (180, 320));
    let bar = pixel(&f, 90, 4);
    assert!(
        near(bar, [0, 200, 0], 24),
        "the bar is a blurred green, was {bar:?}"
    );
    assert!(
        near(pixel(&f, 90, 160), [0, 200, 0], 12),
        "the real picture is still sharp in the middle"
    );
}

#[test]
fn scaling_up_a_small_video_stays_clean() {
    // a 160x90 red video scaled to 3x covers the canvas
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_160x90.mp4", 0xFF0000, 160, 90);
    s.doc
        .update_clip(id, "x", None, |c| {
            c.transform.scale_x = 3.0.into();
            c.transform.scale_y = 3.0.into();
        })
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(near(pixel(&f, 5, 5), [255, 0, 0], 12));
    assert!(near(pixel(&f, 314, 174), [255, 0, 0], 12));
}

#[test]
fn rotating_an_overlay_leaves_the_corners_of_the_canvas_uncovered() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| c.transform.rotation = 45.0.into())
        .unwrap();
    let f = Renderer::new().render(&s.doc.project, US_PER_SEC, SIZE);
    assert!(near(pixel(&f, 160, 90), [255, 0, 0], 12));
    assert!(
        near(pixel(&f, 3, 3), [0, 200, 0], 12),
        "the turned rectangle no longer reaches the corner"
    );
}

#[test]
fn the_frame_looks_the_same_at_a_smaller_preview_size() {
    let mut s = scene();
    let id = add_overlay(&mut s, "lay_red_320x180.mp4", 0xFF0000, 320, 180);
    s.doc
        .update_clip(id, "x", None, |c| {
            c.transform.scale_x = 0.5.into();
            c.transform.scale_y = 0.5.into();
            c.transform.x = 60.0.into();
        })
        .unwrap();
    // the project canvas is 1920x1080; the same layout rendered small must put the square in the same relative place
    let mut r = Renderer::new();
    let big = r.render(&s.doc.project, US_PER_SEC, (960, 540));
    let small = r.render(&s.doc.project, US_PER_SEC, (480, 270));
    for (fx, fy) in [(0.5, 0.5), (0.8, 0.5), (0.2, 0.5), (0.5, 0.1)] {
        let a = pixel(&big, (960.0 * fx) as u32, (540.0 * fy) as u32);
        let b = pixel(&small, (480.0 * fx) as u32, (270.0 * fy) as u32);
        assert!(
            near(a, [b[0], b[1], b[2]], 12),
            "at ({fx},{fy}) the big preview has {a:?} but the small one {b:?}"
        );
    }
}
