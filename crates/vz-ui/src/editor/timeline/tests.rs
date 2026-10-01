use super::*;
use crate::editor::{EditorState, Images};
use crate::settings::UiSettings;
use std::collections::{HashMap, HashSet};
use vz_core::{Document, FrameRate, MediaAsset, Project};

#[test]
fn zoom_range_is_sensible() {
    assert!((pixels_per_second(0.0) - 20.0).abs() < 0.01);
    assert!((pixels_per_second(1.0) - 400.0).abs() < 0.01);
    assert!(pixels_per_second(0.5) > pixels_per_second(0.2));
    // out-of-range values are clamped
    assert_eq!(pixels_per_second(-3.0), pixels_per_second(0.0));
    assert_eq!(pixels_per_second(9.0), pixels_per_second(1.0));
}

#[test]
fn zoom_for_is_the_inverse() {
    for z in [0.0, 0.25, 0.5, 0.9] {
        assert!((zoom_for(pixels_per_second(z)) - z).abs() < 0.001);
    }
}

#[test]
fn geometry_converts_both_ways() {
    let geo = Geo {
        lane_x0: 100.0,
        pps: 50.0,
        scroll_x: 25.0,
    };
    assert_eq!(geo.x(0), 75.0);
    assert_eq!(geo.time(geo.x(3 * US_PER_SEC)), 3 * US_PER_SEC);
    assert_eq!(geo.span(50.0), US_PER_SEC);
}

// ---- editing logic, driven the way the mouse and keyboard drive it ----

/// 100 points per second, no scrolling, timeline starts at x = 0: easy sums.
const GEO: Geo = Geo {
    lane_x0: 0.0,
    pps: 100.0,
    scroll_x: 0.0,
};

fn media(secs: i64) -> MediaAsset {
    MediaAsset {
        id: vz_core::MediaId::new(),
        name: "clip".into(),
        path: format!("/fake/{secs}.mp4").into(),
        relative_path: None,
        kind: MediaKind::Video,
        duration: secs * US_PER_SEC,
        width: 1920,
        height: 1080,
        frame_rate: Some(FrameRate::FPS_30),
        has_audio: true,
        file_size: 1,
    }
}

/// A document with clips of the given lengths (each cut from its own 60 s file).
fn doc(lengths: &[i64]) -> (Document, Vec<ClipId>) {
    let mut doc = Document::new(Project::new("t"));
    let mut ids = Vec::new();
    for (i, secs) in lengths.iter().enumerate() {
        let mut asset = media(60);
        asset.path = format!("/fake/{i}.mp4").into();
        let m = doc.import_media(asset).unwrap();
        let id = doc.add_clip(m, usize::MAX).unwrap();
        doc.trim_right(id, secs * US_PER_SEC, None).unwrap();
        ids.push(id);
    }
    doc.seal_history();
    (doc, ids)
}

fn with_ctx<R>(doc: &mut Document, f: impl FnOnce(&mut Ctx<'_>) -> R) -> (R, EditorState) {
    let egui_ctx = egui::Context::default();
    let texture = egui_ctx.load_texture("t", egui::ColorImage::example(), Default::default());
    let (thumbs, missing) = (HashMap::new(), HashSet::new());
    let (mut state, mut settings) = (EditorState::default(), UiSettings::default());
    let result = {
        let mut ctx = Ctx {
            doc,
            state: &mut state,
            settings: &mut settings,
            layout_gen: 0,
            images: Images { mark: &texture },
            thumbs: &thumbs,
            missing: &missing,
            player_texture: None,
            actions: Vec::new(),
            saved_at: None,
        };
        f(&mut ctx)
    };
    (result, state)
}

fn lengths(doc: &Document) -> Vec<i64> {
    doc.project
        .timeline()
        .main_track()
        .clips
        .iter()
        .map(|c| c.duration / US_PER_SEC)
        .collect()
}

#[test]
fn split_cuts_the_clip_under_the_playhead_and_selects_the_new_piece() {
    let (mut d, _) = doc(&[4]);
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.playhead = US_PER_SEC;
        split_at_playhead(ctx);
    });
    assert_eq!(lengths(&d), vec![1, 3]);
    assert_eq!(state.selection.len(), 1);
    assert_eq!(
        state.selection[0],
        d.project.timeline().main_track().clips[1].id
    );
}

#[test]
fn splitting_at_a_clip_edge_says_why_it_did_nothing() {
    let (mut d, _) = doc(&[4]);
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.playhead = 0;
        split_at_playhead(ctx);
    });
    assert_eq!(lengths(&d), vec![4]);
    assert!(state.toast.is_some());
}

#[test]
fn q_removes_everything_left_of_the_playhead_in_one_undo_step() {
    let (mut d, _) = doc(&[4, 2]);
    with_ctx(&mut d, |ctx| {
        ctx.state.playhead = US_PER_SEC;
        trim_to_playhead(ctx, Side::Left);
    });
    assert_eq!(lengths(&d), vec![3, 2]);
    assert!(d.undo());
    assert_eq!(lengths(&d), vec![4, 2], "one undo brings it all back");
}

#[test]
fn w_removes_everything_right_of_the_playhead() {
    let (mut d, _) = doc(&[4, 2]);
    with_ctx(&mut d, |ctx| {
        ctx.state.playhead = 3 * US_PER_SEC;
        trim_to_playhead(ctx, Side::Right);
    });
    assert_eq!(lengths(&d), vec![3, 2]);
}

#[test]
fn delete_removes_the_selection_and_closes_the_gap() {
    let (mut d, ids) = doc(&[2, 3, 4]);
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.selection = vec![ids[1]];
        delete_selection(ctx);
    });
    assert_eq!(lengths(&d), vec![2, 4]);
    assert_eq!(
        d.project.timeline().main_track().clips[1].start,
        2 * US_PER_SEC
    );
    assert!(state.selection.is_empty());
}

#[test]
fn dragging_a_clip_past_the_middle_of_its_neighbour_changes_the_drop_slot() {
    let (mut d, ids) = doc(&[2, 2, 2]); // A 0-2, B 2-4, C 4-6
    let orig = d.project.timeline().main_track().clips[0].clone();
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.drag = Some(TimelineDrag::Move {
            clip: ids[0],
            orig: orig.clone(),
            grab_dx: 50.0,
            to_index: 0,
        });
        // pointer at x = 400: the dragged clip's centre is at 400 - 50 + 100 = 450 -> 4.5 s: past B (centre 3 s), before C (centre 5 s)
        let drag = ctx.state.drag.clone().unwrap();
        drag_to(ctx, drag, GEO, Pos2::new(400.0, 10.0));
    });
    match state.drag {
        Some(TimelineDrag::Move { to_index, .. }) => assert_eq!(to_index, 1),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        lengths(&d),
        vec![2, 2, 2],
        "nothing changes until the mouse is released"
    );
}

#[test]
fn dragging_an_edge_trims_step_by_step_and_is_one_undo() {
    let (mut d, ids) = doc(&[4, 2]);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 77,
        };
        for x in [20.0, 60.0, 100.0] {
            drag_to(ctx, drag.clone(), GEO, Pos2::new(x, 0.0));
        }
    });
    // the pointer moved 1 s to the right: 1 s trimmed off the left
    let c = d.project.timeline().main_track().clips[0].clone();
    assert_eq!((c.source_in, c.duration), (US_PER_SEC, 3 * US_PER_SEC));
    assert_eq!(
        d.project.timeline().main_track().clips[1].start,
        3 * US_PER_SEC,
        "the next clip followed"
    );
    d.seal_history();
    assert!(d.undo());
    assert_eq!(lengths(&d), vec![4, 2], "the whole drag is one undo step");
}

#[test]
fn a_left_trim_drag_can_come_back() {
    let (mut d, ids) = doc(&[4]);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 5,
        };
        drag_to(ctx, drag.clone(), GEO, Pos2::new(200.0, 0.0));
        drag_to(ctx, drag, GEO, Pos2::new(50.0, 0.0));
    });
    let c = &d.project.timeline().main_track().clips[0];
    assert_eq!(c.source_in, FrameRate::FPS_30.snap(US_PER_SEC / 2));
}

#[test]
fn a_trim_drag_stops_at_the_ends_of_the_clip_and_file() {
    let (mut d, ids) = doc(&[4]);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        // far to the right on the left edge: cannot remove the whole clip
        let left = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 1,
        };
        drag_to(ctx, left, GEO, Pos2::new(5000.0, 0.0));
    });
    let c = d.project.timeline().main_track().clips[0].clone();
    assert!(
        c.duration >= FrameRate::FPS_30.frame_start(1),
        "at least a frame is left"
    );
    // the right edge cannot go past the 60 s file
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let right = TimelineDrag::TrimRight {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 2,
        };
        drag_to(ctx, right, GEO, Pos2::new(900_000.0, 0.0));
    });
    let c = &d.project.timeline().main_track().clips[0];
    assert_eq!(c.source_in + c.duration, 60 * US_PER_SEC);
}

#[test]
fn seeking_snaps_to_frames_and_stays_inside_the_timeline() {
    let (mut d, _) = doc(&[4]);
    with_ctx(&mut d, |ctx| {
        ctx.seek(1_017_000);
        assert_eq!(ctx.state.playhead, FrameRate::FPS_30.snap(1_017_000));
        ctx.seek(99 * US_PER_SEC);
        assert_eq!(ctx.state.playhead, 4 * US_PER_SEC);
        ctx.seek(-5);
        assert_eq!(ctx.state.playhead, 0);
    });
}
