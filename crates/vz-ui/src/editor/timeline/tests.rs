use super::input::{drag_to, finish_drag};
use super::layout::{Frame, Geo, RULER_H, Zone, build_rows};
use super::*;
use crate::editor::{EditorState, Images};
use crate::settings::UiSettings;
use std::collections::{HashMap, HashSet};
use vz_core::{Document, FrameRate, MediaAsset, MediaId, MediaKind, Project, TextContent};

#[test]
fn zoom_range_is_sensible() {
    assert!((pixels_per_second(0.0) - 20.0).abs() < 0.01);
    assert!((pixels_per_second(1.0) - 400.0).abs() < 0.01);
    assert!(pixels_per_second(0.5) > pixels_per_second(0.2));
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

const GEO: Geo = Geo {
    lane_x0: 0.0,
    pps: 100.0,
    scroll_x: 0.0,
};

fn media(kind: MediaKind, name: &str, secs: i64) -> MediaAsset {
    MediaAsset {
        id: MediaId::new(),
        name: name.into(),
        path: format!("/fake/{name}").into(),
        relative_path: None,
        kind,
        duration: if kind == MediaKind::Image {
            0
        } else {
            secs * US_PER_SEC
        },
        width: 1920,
        height: 1080,
        frame_rate: Some(FrameRate::FPS_30),
        has_audio: kind != MediaKind::Image,
        file_size: 1,
    }
}

/// A document with main-track clips of the given lengths (each cut from its own 60 s file).
fn doc(lengths: &[i64]) -> (Document, Vec<ClipId>) {
    let mut doc = Document::new(Project::new("t"));
    let mut ids = Vec::new();
    for (i, secs) in lengths.iter().enumerate() {
        let m = doc
            .import_media(media(MediaKind::Video, &format!("v{i}.mp4"), 60))
            .unwrap();
        let id = doc.add_clip(m, usize::MAX).unwrap();
        doc.trim_right(id, secs * US_PER_SEC, None).unwrap();
        ids.push(id);
    }
    doc.seal_history();
    (doc, ids)
}

fn with_ctx<R>(doc: &mut Document, f: impl FnOnce(&mut Ctx<'_>) -> R) -> (R, EditorState) {
    with_state(doc, EditorState::default(), f)
}

fn with_state<R>(
    doc: &mut Document,
    mut state: EditorState,
    f: impl FnOnce(&mut Ctx<'_>) -> R,
) -> (R, EditorState) {
    let egui_ctx = egui::Context::default();
    let texture = egui_ctx.load_texture("t", egui::ColorImage::example(), Default::default());
    let (thumbs, missing) = (HashMap::new(), HashSet::new());
    let mut settings = UiSettings::default();
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

/// The screen layout for a document: 100 points per second, timeline starting at x = 0, ruler at the top.
fn frame_for(doc: &Document) -> Frame {
    let (rows, _) = build_rows(doc.project.timeline());
    Frame {
        area: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(3000.0, 800.0)),
        ruler: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(3000.0, RULER_H)),
        geo: GEO,
        origin_y: RULER_H,
        rows,
    }
}

fn row_y(frame: &Frame, track: TrackId) -> f32 {
    let row = frame.rows.iter().find(|r| r.track == track).unwrap();
    frame.row_rect(row).center().y
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
fn split_cuts_the_selected_clip_at_the_playhead_and_selects_the_new_piece() {
    let (mut d, ids) = doc(&[4]);
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.playhead = US_PER_SEC;
        ctx.state.selection = vec![ids[0]];
        split_at_playhead(ctx);
    });
    assert_eq!(lengths(&d), vec![1, 3]);
    assert_eq!(
        state.selection,
        vec![d.project.timeline().main_track().clips[1].id]
    );
}

#[test]
fn with_nothing_selected_split_cuts_the_main_clip_under_the_playhead() {
    let (mut d, _) = doc(&[4]);
    with_ctx(&mut d, |ctx| {
        ctx.state.playhead = 2 * US_PER_SEC;
        split_at_playhead(ctx);
    });
    assert_eq!(lengths(&d), vec![2, 2]);
}

#[test]
fn splitting_at_a_clip_edge_says_why_it_did_nothing() {
    let (mut d, ids) = doc(&[4]);
    let (_, state) = with_ctx(&mut d, |ctx| {
        ctx.state.selection = vec![ids[0]];
        ctx.state.playhead = 0;
        split_at_playhead(ctx);
    });
    assert_eq!(lengths(&d), vec![4]);
    assert!(state.toast.is_some());
}

#[test]
fn split_works_on_several_selected_clips_across_tracks_in_one_undo() {
    let (mut d, ids) = doc(&[6]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let pic = d.place_media(overlay, m, 0).unwrap();
    let before = d.project.timeline().clone();
    with_ctx(&mut d, |ctx| {
        ctx.state.playhead = 2 * US_PER_SEC;
        ctx.state.selection = vec![ids[0], pic];
        split_at_playhead(ctx);
        assert_eq!(ctx.state.selection.len(), 2, "both new pieces are selected");
    });
    assert_eq!(d.project.timeline().track(overlay).unwrap().clips.len(), 2);
    assert!(d.undo());
    assert_eq!(d.project.timeline(), &before);
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
fn delete_removes_the_selection_and_closes_the_gap_on_the_main_track() {
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
fn dragging_a_main_clip_over_the_main_row_targets_that_track() {
    let (mut d, ids) = doc(&[2, 2, 2]);
    let frame = frame_for(&d);
    let main = d.project.timeline().main_track().id;
    let orig = d.project.timeline().main_track().clips[0].clone();
    let (_, state) = with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::Move {
            clip: ids[0],
            orig: orig.clone(),
            from: main,
            grab_dx: 50.0,
            target: None,
        };
        drag_to(ctx, drag, &frame, Pos2::new(400.0, row_y(&frame, main)));
    });
    match state.drag {
        Some(TimelineDrag::Move {
            target: Some(t), ..
        }) => {
            assert_eq!(t.track, main);
            assert_eq!(
                t.start,
                3 * US_PER_SEC + US_PER_SEC / 2,
                "left edge is at x = 350"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        lengths(&d),
        vec![2, 2, 2],
        "nothing changes until the mouse is released"
    );
}

#[test]
fn releasing_a_main_clip_in_a_later_slot_reorders_the_track() {
    let (mut d, ids) = doc(&[2, 2, 2]);
    let frame = frame_for(&d);
    let main = d.project.timeline().main_track().id;
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        ctx.state.drag = Some(TimelineDrag::Move {
            clip: ids[0],
            orig: orig.clone(),
            from: main,
            grab_dx: 50.0,
            target: None,
        });
        let drag = ctx.state.drag.clone().unwrap();
        // the dragged clip's centre ends up at 4.5 s: after B (centre 3 s), before C (centre 5 s)
        let pos = Pos2::new(400.0, row_y(&frame, main));
        drag_to(ctx, drag, &frame, pos);
        finish_drag(ctx, &frame, Some(pos));
    });
    let order: Vec<_> = d
        .project
        .timeline()
        .main_track()
        .clips
        .iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(order, vec![ids[1], ids[0], ids[2]]);
}

#[test]
fn dragging_onto_another_kind_of_track_finds_no_valid_target() {
    let (mut d, ids) = doc(&[4]);
    let audio = d.add_track(TrackKind::Audio).unwrap();
    let frame = frame_for(&d);
    let main = d.project.timeline().main_track().id;
    let orig = d.project.timeline().main_track().clips[0].clone();
    let (_, state) = with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::Move {
            clip: ids[0],
            orig,
            from: main,
            grab_dx: 0.0,
            target: None,
        };
        drag_to(ctx, drag, &frame, Pos2::new(200.0, row_y(&frame, audio)));
    });
    assert!(
        matches!(state.drag, Some(TimelineDrag::Move { target: None, .. })),
        "a video clip does not go on an audio track"
    );
}

#[test]
fn dragging_a_main_clip_onto_an_overlay_track_moves_it_there() {
    let (mut d, ids) = doc(&[4, 4]);
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let frame = frame_for(&d);
    let main = d.project.timeline().main_track().id;
    let orig = d.project.timeline().main_track().clips[1].clone();
    with_ctx(&mut d, |ctx| {
        ctx.state.drag = Some(TimelineDrag::Move {
            clip: ids[1],
            orig,
            from: main,
            grab_dx: 0.0,
            target: None,
        });
        let drag = ctx.state.drag.clone().unwrap();
        let pos = Pos2::new(1000.0, row_y(&frame, overlay)); // 10 s
        drag_to(ctx, drag, &frame, pos);
        finish_drag(ctx, &frame, Some(pos));
    });
    assert_eq!(d.track_of(ids[1]), Some(overlay));
    assert_eq!(
        d.project.timeline().clip(ids[1]).unwrap().start,
        10 * US_PER_SEC
    );
    assert_eq!(d.project.timeline().main_track().clips.len(), 1);
}

#[test]
fn dropping_a_clip_above_every_track_makes_a_new_overlay_track() {
    let (mut d, ids) = doc(&[4]);
    let frame = frame_for(&d);
    let main = d.project.timeline().main_track().id;
    let orig = d.project.timeline().main_track().clips[0].clone();
    let y = frame.origin_y + 4.0; // in the strip above the first track
    assert_eq!(frame.zone_at(y), Some(Zone::Top));
    with_ctx(&mut d, |ctx| {
        ctx.state.drag = Some(TimelineDrag::Move {
            clip: ids[0],
            orig,
            from: main,
            grab_dx: 0.0,
            target: None,
        });
        let drag = ctx.state.drag.clone().unwrap();
        drag_to(ctx, drag, &frame, Pos2::new(300.0, y));
        finish_drag(ctx, &frame, Some(Pos2::new(300.0, y)));
    });
    let track = d.project.timeline().track_of(ids[0]).unwrap();
    assert_eq!(track.kind, TrackKind::Overlay);
    assert_eq!(d.project.timeline().tracks.len(), 2);
    assert!(d.undo());
    assert_eq!(
        d.project.timeline().tracks.len(),
        1,
        "one undo removes the new track and puts the clip back"
    );
    assert_eq!(d.project.timeline().main_track().clips.len(), 1);
}

#[test]
fn a_dragged_overlay_clip_sticks_to_the_playhead() {
    let (mut d, _) = doc(&[10]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let pic = d.place_media(overlay, m, 7 * US_PER_SEC).unwrap();
    let frame = frame_for(&d);
    let orig = d.project.timeline().clip(pic).unwrap().clone();
    let state = EditorState {
        playhead: 2 * US_PER_SEC,
        ..EditorState::default()
    };
    let (_, state) = with_state(&mut d, state, |ctx| {
        let drag = TimelineDrag::Move {
            clip: pic,
            orig,
            from: overlay,
            grab_dx: 0.0,
            target: None,
        };
        // the clip's left edge is at 2.04 s: within 8 points of the playhead at 2 s
        drag_to(ctx, drag, &frame, Pos2::new(204.0, row_y(&frame, overlay)));
    });
    assert_eq!(state.snap_guide, Some(2 * US_PER_SEC));
    match state.drag {
        Some(TimelineDrag::Move {
            target: Some(t), ..
        }) => assert_eq!(t.start, 2 * US_PER_SEC),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn snapping_can_be_switched_off() {
    let (mut d, _) = doc(&[10]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let pic = d.place_media(overlay, m, 7 * US_PER_SEC).unwrap();
    let frame = frame_for(&d);
    let orig = d.project.timeline().clip(pic).unwrap().clone();
    let mut state = EditorState {
        playhead: 2 * US_PER_SEC,
        ..EditorState::default()
    };
    state.snapping = false;
    let (_, state) = with_state(&mut d, state, |ctx| {
        let drag = TimelineDrag::Move {
            clip: pic,
            orig,
            from: overlay,
            grab_dx: 0.0,
            target: None,
        };
        drag_to(ctx, drag, &frame, Pos2::new(204.0, row_y(&frame, overlay)));
    });
    assert_eq!(state.snap_guide, None);
}

#[test]
fn several_selected_overlay_clips_move_together() {
    let (mut d, _) = doc(&[20]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let a = d.place_media(overlay, m, 0).unwrap(); // 0..5
    let b = d.place_media(overlay, m, 7 * US_PER_SEC).unwrap(); // 7..12
    let frame = frame_for(&d);
    let orig = d.project.timeline().clip(a).unwrap().clone();
    let mut state = EditorState {
        snapping: false,
        ..EditorState::default()
    };
    state.selection = vec![a, b];
    with_state(&mut d, state, |ctx| {
        ctx.state.drag = Some(TimelineDrag::Move {
            clip: a,
            orig,
            from: overlay,
            grab_dx: 0.0,
            target: None,
        });
        let drag = ctx.state.drag.clone().unwrap();
        let pos = Pos2::new(300.0, row_y(&frame, overlay)); // a's left edge at 3 s
        drag_to(ctx, drag, &frame, pos);
        finish_drag(ctx, &frame, Some(pos));
    });
    let clips = &d.project.timeline().track(overlay).unwrap().clips;
    assert_eq!(
        (clips[0].start, clips[1].start),
        (3 * US_PER_SEC, 10 * US_PER_SEC),
        "both moved by 3 s"
    );
}

#[test]
fn dragging_an_edge_trims_step_by_step_and_is_one_undo() {
    let (mut d, ids) = doc(&[4, 2]);
    let frame = frame_for(&d);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 77,
        };
        for x in [20.0, 60.0, 100.0] {
            drag_to(ctx, drag.clone(), &frame, Pos2::new(x, 0.0));
        }
    });
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
    let frame = frame_for(&d);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let drag = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 5,
        };
        drag_to(ctx, drag.clone(), &frame, Pos2::new(200.0, 0.0));
        drag_to(ctx, drag, &frame, Pos2::new(50.0, 0.0));
    });
    assert_eq!(
        d.project.timeline().main_track().clips[0].source_in,
        FrameRate::FPS_30.snap(US_PER_SEC / 2)
    );
}

#[test]
fn a_trim_drag_stops_at_the_ends_of_the_clip_and_file() {
    let (mut d, ids) = doc(&[4]);
    let frame = frame_for(&d);
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let left = TimelineDrag::TrimLeft {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 1,
        };
        drag_to(ctx, left, &frame, Pos2::new(5000.0, 0.0));
    });
    let c = d.project.timeline().main_track().clips[0].clone();
    assert!(
        c.duration >= FrameRate::FPS_30.frame_start(1),
        "at least a frame is left"
    );
    let orig = d.project.timeline().main_track().clips[0].clone();
    with_ctx(&mut d, |ctx| {
        let right = TimelineDrag::TrimRight {
            clip: ids[0],
            orig: orig.clone(),
            press_x: 0.0,
            key: 2,
        };
        drag_to(ctx, right, &frame, Pos2::new(900_000.0, 0.0));
    });
    let c = &d.project.timeline().main_track().clips[0];
    assert_eq!(c.source_in + c.duration, 60 * US_PER_SEC);
}

#[test]
fn trimming_an_overlay_clip_leaves_the_other_clips_where_they_are() {
    let (mut d, _) = doc(&[10]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let a = d.place_media(overlay, m, 2 * US_PER_SEC).unwrap(); // 2..7
    let b = d.place_media(overlay, m, 9 * US_PER_SEC).unwrap(); // 9..14
    let frame = frame_for(&d);
    let orig = d.project.timeline().clip(a).unwrap().clone();
    let state = EditorState {
        snapping: false,
        ..EditorState::default()
    };
    with_state(&mut d, state, |ctx| {
        let right = TimelineDrag::TrimRight {
            clip: a,
            orig: orig.clone(),
            press_x: 700.0,
            key: 3,
        };
        drag_to(ctx, right, &frame, Pos2::new(600.0, 0.0)); // 1 s shorter
        let left = TimelineDrag::TrimLeft {
            clip: a,
            orig: orig.clone(),
            press_x: 200.0,
            key: 4,
        };
        drag_to(ctx, left, &frame, Pos2::new(250.0, 0.0)); // starts 0.5 s later
    });
    let clips = &d.project.timeline().track(overlay).unwrap().clips;
    assert_eq!(clips[0].start, 2 * US_PER_SEC + US_PER_SEC / 2);
    assert_eq!(clips[1].id, b);
    assert_eq!(
        clips[1].start,
        9 * US_PER_SEC,
        "the other clip did not move"
    );
}

#[test]
fn a_text_clip_can_be_stretched_as_far_as_you_like() {
    let (mut d, _) = doc(&[10]);
    let text = d.add_track(TrackKind::Text).unwrap();
    let t = d
        .add_text(text, 0, 2 * US_PER_SEC, TextContent::new("hello"))
        .unwrap();
    let frame = frame_for(&d);
    let orig = d.project.timeline().clip(t).unwrap().clone();
    let state = EditorState {
        snapping: false,
        ..EditorState::default()
    };
    with_state(&mut d, state, |ctx| {
        let right = TimelineDrag::TrimRight {
            clip: t,
            orig,
            press_x: 200.0,
            key: 9,
        };
        drag_to(ctx, right, &frame, Pos2::new(5200.0, 0.0)); // 50 s longer
    });
    assert_eq!(
        d.project.timeline().clip(t).unwrap().duration,
        52 * US_PER_SEC
    );
}

#[test]
fn a_rubber_band_selects_the_clips_it_touches() {
    let (mut d, ids) = doc(&[2, 2, 2]);
    let m = d.import_media(media(MediaKind::Image, "p.png", 0)).unwrap();
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let pic = d.place_media(overlay, m, 0).unwrap();
    let frame = frame_for(&d);
    let top = frame.row_rect(frame.rows.first().unwrap()).top();
    let bottom = frame.row_rect(frame.rows.last().unwrap()).bottom();
    let (_, state) = with_ctx(&mut d, |ctx| {
        // from 1 s to 3 s across both rows: touches the overlay clip, the first and the second main clip
        let drag = TimelineDrag::BoxSelect {
            origin: Pos2::new(100.0, top),
            additive: false,
            base: Vec::new(),
        };
        drag_to(ctx, drag, &frame, Pos2::new(300.0, bottom));
    });
    assert_eq!(state.selection.len(), 3);
    for id in [ids[0], ids[1], pic] {
        assert!(state.selection.contains(&id));
    }
    assert!(!state.selection.contains(&ids[2]));
}

#[test]
fn the_tracks_are_found_by_screen_position() {
    let (mut d, _) = doc(&[2]);
    let overlay = d.add_track(TrackKind::Overlay).unwrap();
    let frame = frame_for(&d);
    assert_eq!(frame.row_at(row_y(&frame, overlay)).unwrap().track, overlay);
    assert_eq!(frame.zone_at(frame.origin_y + 2.0), Some(Zone::Top));
    let last = frame.rows.last().unwrap();
    assert_eq!(
        frame.zone_at(frame.origin_y + last.top + last.height + 6.0),
        Some(Zone::Bottom)
    );
    assert_eq!(frame.zone_at(row_y(&frame, overlay)), None);
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
