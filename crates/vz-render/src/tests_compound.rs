//! Compound clips: a timeline played as one clip.

use super::*;
use vz_core::{ShapeContent, ShapeKind, TextContent, US_PER_SEC};
use vz_media::testutil::{self, Pattern, Spec};

const SIZE: (u32, u32) = (320, 180);

fn pixel(f: &VideoFrame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
}

fn near(p: [u8; 4], rgb: [u8; 3], tolerance: i32) -> bool {
    (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tolerance)
}

fn solid(name: &str, rgb: u32) -> PathBuf {
    testutil::video(
        name,
        Spec {
            pattern: Pattern::Solid(rgb),
            width: 320,
            height: 180,
            audio_hz: None,
            seconds: 3,
            ..Spec::default()
        },
    )
}

/// A project with a blue picture on the main track and a red square (a compound clip of one shape) above it.
fn red_square_compound() -> (vz_core::Document, vz_core::ClipId) {
    let blue = solid("cmp_blue_320x180.mp4", 0x0000C8);
    let mut doc = vz_core::Document::new(Project::new("compound"));
    let m = doc.import_media(vz_media::probe(&blue).unwrap()).unwrap();
    doc.add_clip(m, 0).unwrap();
    let shape = ShapeContent {
        width: 300.0,
        height: 300.0,
        fill: Some([255, 0, 0, 255]),
        ..ShapeContent::new(ShapeKind::Rectangle)
    };
    let (_, square) = doc.add_shape_auto(0, 2 * US_PER_SEC, shape).unwrap();
    let compound = doc.make_compound(&[square]).unwrap();
    (doc, compound)
}

#[test]
fn a_compound_clip_shows_what_is_inside_it_over_what_is_below() {
    let (doc, _) = red_square_compound();
    let f = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    // the square is in the middle, the picture shows round it
    assert!(
        near(pixel(&f, 160, 90), [255, 0, 0], 3),
        "{:?}",
        pixel(&f, 160, 90)
    );
    assert!(
        near(pixel(&f, 3, 3), [0, 0, 200], 3),
        "{:?}",
        pixel(&f, 3, 3)
    );
}

#[test]
fn nothing_shows_before_or_after_the_compound() {
    let (doc, _) = red_square_compound();
    let f = Renderer::new().render(&doc.project, 5 * US_PER_SEC / 2, SIZE);
    assert!(
        near(pixel(&f, 160, 90), [0, 0, 200], 3),
        "{:?}",
        pixel(&f, 160, 90)
    );
}

#[test]
fn a_compound_clip_can_be_moved_scaled_and_faded_like_any_clip() {
    let (mut doc, compound) = red_square_compound();
    doc.update_clip(compound, "x", None, |c| {
        c.transform.x = 400.0.into();
        c.opacity = 0.5.into();
    })
    .unwrap();
    let f = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    // moved to the right (400 canvas pixels is 67 here): the middle is blue again, the new place is the square mixed half and half with blue
    assert!(
        near(pixel(&f, 60, 90), [0, 0, 200], 4),
        "{:?}",
        pixel(&f, 60, 90)
    );
    let p = pixel(&f, 227, 90);
    assert!(
        (110..145).contains(&i32::from(p[0])) && (80..120).contains(&i32::from(p[2])),
        "half red over blue: {p:?}"
    );
}

#[test]
fn soft_edges_inside_a_compound_keep_their_see_through_look() {
    let blue = solid("cmp_blue_320x180.mp4", 0x0000C8);
    let mut doc = vz_core::Document::new(Project::new("soft"));
    let m = doc.import_media(vz_media::probe(&blue).unwrap()).unwrap();
    doc.add_clip(m, 0).unwrap();
    // a half see-through white shape
    let shape = ShapeContent {
        width: 300.0,
        height: 300.0,
        fill: Some([255, 255, 255, 128]),
        ..ShapeContent::new(ShapeKind::Rectangle)
    };
    let (_, id) = doc.add_shape_auto(0, 2 * US_PER_SEC, shape).unwrap();
    doc.make_compound(&[id]).unwrap();
    let f = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    // white at half strength over blue (0, 0, 200): about (128, 128, 228)
    let p = pixel(&f, 160, 90);
    assert!(near(p, [128, 128, 228], 6), "{p:?}");
}

#[test]
fn text_inside_a_compound_is_drawn() {
    let blue = solid("cmp_blue_320x180.mp4", 0x0000C8);
    let mut doc = vz_core::Document::new(Project::new("text"));
    let m = doc.import_media(vz_media::probe(&blue).unwrap()).unwrap();
    doc.add_clip(m, 0).unwrap();
    let mut text = TextContent::new("HELLO");
    text.style.size = 200.0;
    text.style.color = [255, 255, 255, 255];
    let (_, id) = doc.add_text_auto(0, 2 * US_PER_SEC, text).unwrap();
    let plain = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    doc.make_compound(&[id]).unwrap();
    let nested = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    let white = |f: &VideoFrame| {
        f.rgba
            .chunks(4)
            .filter(|p| p[0] > 200 && p[2] > 200)
            .count()
    };
    assert!(white(&plain) > 200);
    // the same letters, give or take the edge pixels
    let (a, b) = (white(&plain) as f64, white(&nested) as f64);
    assert!((a - b).abs() / a < 0.08, "{a} vs {b}");
}

#[test]
fn a_compound_inside_a_compound_works_and_a_loop_stops() {
    let (mut doc, compound) = red_square_compound();
    // fold the compound into another one
    let outer = doc.make_compound(&[compound]).unwrap();
    let f = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    assert!(near(pixel(&f, 160, 90), [255, 0, 0], 3));
    // a clip that plays its own timeline must not hang the renderer
    let me = doc.project.timeline().id;
    let inner = doc.compound_timeline(outer).unwrap();
    doc.project
        .timelines
        .iter_mut()
        .find(|t| t.id == inner)
        .unwrap()
        .tracks
        .push({
            let mut t = vz_core::Track::new(vz_core::TrackKind::Overlay, "loop");
            t.clips
                .push(vz_core::Clip::compound_clip(me, 0, 2 * US_PER_SEC));
            t
        });
    let f = Renderer::new().render(&doc.project, US_PER_SEC, SIZE);
    assert_eq!((f.width, f.height), SIZE);
}

#[test]
fn the_picture_of_the_timeline_that_is_open_is_what_is_rendered() {
    let (mut doc, compound) = red_square_compound();
    let inner = doc.compound_timeline(compound).unwrap();
    doc.set_active_timeline(inner);
    // inside the compound there is only the red square, no blue picture
    let f = Renderer::new().render(&doc.project, US_PER_SEC / 2, SIZE);
    assert!(near(pixel(&f, 160, 90), [255, 0, 0], 3));
    assert!(near(pixel(&f, 3, 3), [0, 0, 0], 2), "{:?}", pixel(&f, 3, 3));
}

#[test]
fn unmatting_recovers_colour_and_see_through_amount() {
    let mut black = VideoFrame::black(2, 1);
    let mut white = VideoFrame::black(2, 1);
    // pixel 0: nothing drawn; pixel 1: pure red at half strength
    white.rgba[..4].copy_from_slice(&[255, 255, 255, 255]);
    black.rgba[4..8].copy_from_slice(&[128, 0, 0, 255]);
    white.rgba[4..8].copy_from_slice(&[255, 128, 128, 255]);
    let out = unmatte(&black, &white);
    assert_eq!(pixel(&out, 0, 0), [0, 0, 0, 0]);
    let p = pixel(&out, 1, 0);
    assert!(
        (i32::from(p[3]) - 128).abs() <= 3 && p[0] > 240 && p[1] < 5,
        "{p:?}"
    );
}
