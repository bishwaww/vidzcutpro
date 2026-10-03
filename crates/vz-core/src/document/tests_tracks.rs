//! Tests for overlay, text and audio tracks.

use super::*;
use crate::content::TextContent;
use crate::media::MediaKind;
use crate::time::US_PER_SEC;

fn asset(name: &str, kind: MediaKind, secs: i64) -> MediaAsset {
    MediaAsset {
        id: MediaId::new(),
        name: name.into(),
        path: PathBuf::from(format!("/m/{name}")),
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

struct Setup {
    doc: Document,
    video: MediaId,
    photo: MediaId,
    music: MediaId,
}

fn setup() -> Setup {
    let mut doc = Document::new(Project::new("t"));
    let video = doc
        .import_media(asset("v.mp4", MediaKind::Video, 20))
        .unwrap();
    let photo = doc
        .import_media(asset("p.png", MediaKind::Image, 0))
        .unwrap();
    let music = doc
        .import_media(asset("m.mp3", MediaKind::Audio, 60))
        .unwrap();
    doc.add_clip(video, 0).unwrap();
    doc.seal_history();
    Setup {
        doc,
        video,
        photo,
        music,
    }
}

fn clips_of(doc: &Document, track: TrackId) -> Vec<Clip> {
    doc.project.timeline().track(track).unwrap().clips.clone()
}

#[test]
fn tracks_can_be_added_and_removed_and_undo_brings_everything_back() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let text = s.doc.add_track(TrackKind::Text).unwrap();
    let audio = s.doc.add_track(TrackKind::Audio).unwrap();
    let names: Vec<_> = s
        .doc
        .project
        .timeline()
        .tracks
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names, vec!["Main", "Overlay 1", "Text 1", "Audio 1"]);
    assert!(
        s.doc.add_track(TrackKind::Main).is_err(),
        "there is only one main track"
    );

    s.doc.place_media(overlay, s.photo, US_PER_SEC).unwrap();
    let with_clip = s.doc.project.timeline().clone();
    s.doc.remove_track(overlay).unwrap();
    assert!(s.doc.project.timeline().track(overlay).is_none());
    assert!(s.doc.undo());
    assert_eq!(
        s.doc.project.timeline(),
        &with_clip,
        "the removed track comes back with its clip, in place"
    );
    let _ = (text, audio);
}

#[test]
fn the_main_track_cannot_be_removed() {
    let mut s = setup();
    let main = s.doc.project.timeline().main_track().id;
    assert!(matches!(
        s.doc.remove_track(main),
        Err(DocError::Edit(EditError::MainTrack))
    ));
}

#[test]
fn media_goes_on_the_right_kind_of_track() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let audio = s.doc.add_track(TrackKind::Audio).unwrap();
    let text = s.doc.add_track(TrackKind::Text).unwrap();
    assert!(s.doc.place_media(overlay, s.photo, 0).is_ok());
    assert!(s.doc.place_media(audio, s.music, 0).is_ok());
    assert!(matches!(
        s.doc.place_media(audio, s.photo, 0),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
    assert!(matches!(
        s.doc.place_media(overlay, s.music, 0),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
    assert!(matches!(
        s.doc.place_media(text, s.video, 0),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
    assert!(matches!(
        s.doc.add_clip(s.music, 0),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
}

#[test]
fn a_clip_placed_on_something_lands_on_the_next_free_spot() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    s.doc.place_media(overlay, s.photo, 0).unwrap(); // 0..5 s
    let second = s.doc.place_media(overlay, s.photo, 2 * US_PER_SEC).unwrap();
    let clips = clips_of(&s.doc, overlay);
    assert_eq!(clips[1].id, second);
    assert_eq!(clips[1].start, 5 * US_PER_SEC);
}

#[test]
fn text_goes_on_text_tracks_with_its_own_style() {
    let mut s = setup();
    let text = s.doc.add_track(TrackKind::Text).unwrap();
    let mut content = TextContent::new("ශ්‍රී ලංකාව");
    content.style.size = 96.0;
    let id = s
        .doc
        .add_text(text, 2 * US_PER_SEC, 3 * US_PER_SEC, content)
        .unwrap();
    let clip = s.doc.project.timeline().clip(id).unwrap();
    assert_eq!(
        (clip.start, clip.duration),
        (2 * US_PER_SEC, 3 * US_PER_SEC)
    );
    assert!(
        matches!(&clip.content, Content::Text(t) if t.text == "ශ්‍රී ලංකාව" && t.style.size == 96.0)
    );
    let main = s.doc.project.timeline().main_track().id;
    assert!(matches!(
        s.doc.add_text(main, 0, US_PER_SEC, TextContent::new("x")),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
}

#[test]
fn changing_a_property_is_one_undo_step_even_for_a_whole_drag() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let id = s.doc.place_media(overlay, s.photo, 0).unwrap();
    s.doc.seal_history();
    let original = s.doc.project.timeline().clip(id).unwrap().clone();
    for i in 1..=10 {
        s.doc
            .update_clip(id, "Move", Some(9), |c| {
                c.transform.x = (i as f32 * 10.0).into()
            })
            .unwrap();
    }
    s.doc.seal_history();
    assert_eq!(
        s.doc
            .project
            .timeline()
            .clip(id)
            .unwrap()
            .transform
            .x
            .value_at(0),
        100.0
    );
    assert!(s.doc.undo());
    assert_eq!(s.doc.project.timeline().clip(id).unwrap(), &original);
    assert!(s.doc.redo());
    assert_eq!(
        s.doc
            .project
            .timeline()
            .clip(id)
            .unwrap()
            .transform
            .x
            .value_at(0),
        100.0
    );
}

#[test]
fn an_unchanged_property_makes_no_undo_step() {
    let mut s = setup();
    let id = s.doc.project.timeline().main_track().clips[0].id;
    let before = s.doc.revision();
    s.doc.update_clip(id, "Nothing", None, |_| {}).unwrap();
    assert_eq!(s.doc.revision(), before);
}

#[test]
fn locked_tracks_refuse_property_changes() {
    let mut s = setup();
    let main = s.doc.project.timeline().main_track().id;
    let id = s.doc.project.timeline().main_track().clips[0].id;
    s.doc.set_track_flags(main, |f| f.locked = true).unwrap();
    assert!(matches!(
        s.doc.update_clip(id, "x", None, |c| c.opacity = 0.5.into()),
        Err(DocError::Edit(EditError::Locked))
    ));
    s.doc.set_track_flags(main, |f| f.locked = false).unwrap();
    assert!(
        s.doc
            .update_clip(id, "x", None, |c| c.opacity = 0.5.into())
            .is_ok()
    );
}

#[test]
fn track_switches_can_be_undone() {
    let mut s = setup();
    let main = s.doc.project.timeline().main_track().id;
    s.doc
        .set_track_flags(main, |f| {
            f.hidden = true;
            f.muted = true;
        })
        .unwrap();
    assert!(s.doc.project.timeline().main_track().hidden);
    assert!(s.doc.undo());
    let t = s.doc.project.timeline().main_track();
    assert!(!t.hidden && !t.muted);
}

#[test]
fn splitting_and_trimming_work_on_overlay_clips_without_moving_others() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap(); // 0..5
    let b = s.doc.place_media(overlay, s.photo, 8 * US_PER_SEC).unwrap(); // 8..13
    let right = s.doc.split_clip(a, 2 * US_PER_SEC).unwrap();
    let clips = clips_of(&s.doc, overlay);
    assert_eq!(clips.len(), 3);
    assert_eq!(clips[2].id, b);
    assert_eq!(clips[2].start, 8 * US_PER_SEC, "later clips do not move");
    s.doc.trim_right(right, US_PER_SEC, None).unwrap();
    let clips = clips_of(&s.doc, overlay);
    assert_eq!(clips[2].start, 8 * US_PER_SEC);
    assert_eq!(clips[1].duration, US_PER_SEC);
}

#[test]
fn deleting_overlay_clips_leaves_a_gap_but_main_clips_close_up() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap();
    let b = s.doc.place_media(overlay, s.photo, 6 * US_PER_SEC).unwrap();
    s.doc.delete_clips(&[a]).unwrap();
    assert_eq!(clips_of(&s.doc, overlay)[0].start, 6 * US_PER_SEC);

    let m2 = s.doc.add_clip(s.video, 1).unwrap();
    let main = s.doc.project.timeline().main_track().id;
    let first = clips_of(&s.doc, main)[0].id;
    s.doc.delete_clips(&[first]).unwrap();
    assert_eq!(clips_of(&s.doc, main)[0].id, m2);
    assert_eq!(
        clips_of(&s.doc, main)[0].start,
        0,
        "the main track closed the gap"
    );
    let _ = b;
}

#[test]
fn one_delete_across_two_tracks_is_one_undo() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap();
    let main_clip = s.doc.project.timeline().main_track().clips[0].id;
    let before = s.doc.project.timeline().clone();
    s.doc.delete_clips(&[a, main_clip]).unwrap();
    assert_eq!(s.doc.project.duration(), 0);
    assert!(s.doc.undo());
    assert_eq!(s.doc.project.timeline(), &before);
    assert!(s.doc.delete_clips(&[ClipId::new()]).is_err());
}

#[test]
fn clips_move_between_free_tracks_and_refuse_collisions() {
    let mut s = setup();
    let o1 = s.doc.add_track(TrackKind::Overlay).unwrap();
    let o2 = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(o1, s.photo, 0).unwrap();
    let blocker = s.doc.place_media(o2, s.photo, 0).unwrap();
    // o2 is busy from 0 to 5 s
    assert!(matches!(
        s.doc.move_clip_to(a, o2, US_PER_SEC, None),
        Err(DocError::Edit(EditError::Overlap))
    ));
    s.doc.move_clip_to(a, o2, 6 * US_PER_SEC, None).unwrap();
    assert!(clips_of(&s.doc, o1).is_empty());
    let on_o2 = clips_of(&s.doc, o2);
    assert_eq!((on_o2[0].id, on_o2[1].id), (blocker, a));
    assert_eq!(on_o2[1].start, 6 * US_PER_SEC);
    assert!(s.doc.undo());
    assert_eq!(
        clips_of(&s.doc, o1).len(),
        1,
        "one undo puts it back on the first track"
    );
}

#[test]
fn a_clip_can_move_along_its_own_free_track() {
    let mut s = setup();
    let o = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(o, s.photo, 0).unwrap();
    s.doc
        .move_clip_to(a, o, 9 * US_PER_SEC + 13_000, None)
        .unwrap();
    assert_eq!(clips_of(&s.doc, o)[0].start, 9 * US_PER_SEC);
}

#[test]
fn a_video_clip_can_move_from_the_main_track_to_an_overlay_and_back() {
    let mut s = setup();
    let o = s.doc.add_track(TrackKind::Overlay).unwrap();
    let main = s.doc.project.timeline().main_track().id;
    let id = s.doc.project.timeline().main_track().clips[0].id;
    s.doc.move_clip_to(id, o, 3 * US_PER_SEC, None).unwrap();
    assert!(clips_of(&s.doc, main).is_empty());
    assert_eq!(clips_of(&s.doc, o)[0].start, 3 * US_PER_SEC);
    s.doc.move_clip_to(id, main, 0, None).unwrap();
    assert_eq!(
        clips_of(&s.doc, main)[0].start,
        0,
        "on the magnetic track it sits at the start"
    );
    assert!(clips_of(&s.doc, o).is_empty());
}

#[test]
fn text_cannot_go_on_the_audio_track_and_sound_not_on_the_picture_tracks() {
    let mut s = setup();
    let text_track = s.doc.add_track(TrackKind::Text).unwrap();
    let audio = s.doc.add_track(TrackKind::Audio).unwrap();
    let t = s
        .doc
        .add_text(text_track, 0, US_PER_SEC, TextContent::new("x"))
        .unwrap();
    assert!(matches!(
        s.doc.move_clip_to(t, audio, 0, None),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
    let m = s.doc.place_media(audio, s.music, 0).unwrap();
    let main = s.doc.project.timeline().main_track().id;
    assert!(matches!(
        s.doc.move_clip_to(m, main, 0, None),
        Err(DocError::Edit(EditError::WrongTrack))
    ));
}

#[test]
fn a_whole_project_with_every_kind_of_track_saves_and_opens_again() {
    let dir = std::env::temp_dir().join(format!("vidzcut-tracks-{}", uuid::Uuid::new_v4()));
    let mut s = setup();
    let o = s.doc.add_track(TrackKind::Overlay).unwrap();
    let t = s.doc.add_track(TrackKind::Text).unwrap();
    let a = s.doc.add_track(TrackKind::Audio).unwrap();
    let pic = s.doc.place_media(o, s.photo, US_PER_SEC).unwrap();
    s.doc
        .update_clip(pic, "x", None, |c| {
            c.transform.x = 120.0.into();
            c.transform.rotation = 15.0.into();
            c.opacity = 0.6.into();
            c.blend = crate::layer::BlendMode::Multiply;
            c.crop.left = 0.1;
        })
        .unwrap();
    s.doc
        .add_text(t, 0, 2 * US_PER_SEC, TextContent::new("Hello ශ්‍රී"))
        .unwrap();
    let music = s.doc.place_media(a, s.music, 0).unwrap();
    s.doc
        .update_clip(music, "x", None, |c| {
            c.volume = 0.5.into();
            c.fade_in = US_PER_SEC;
        })
        .unwrap();
    let file = dir.join("p.vzp");
    s.doc.save(Some(&file)).unwrap();
    let back = Document::open(&file).unwrap();
    assert_eq!(back.project.timelines, s.doc.project.timelines);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn splitting_several_clips_at_once_is_one_undo_step() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap(); // 0..5 s on the overlay
    let main_clip = s.doc.project.timeline().main_track().clips[0].id; // 0..20 s on main
    let before = s.doc.project.timeline().clone();
    let created = s.doc.split_clips(&[a, main_clip], 2 * US_PER_SEC).unwrap();
    assert_eq!(created.len(), 2);
    assert_eq!(clips_of(&s.doc, overlay).len(), 2);
    assert_eq!(s.doc.project.timeline().main_track().clips.len(), 2);
    assert!(s.doc.undo());
    assert_eq!(s.doc.project.timeline(), &before);
}

#[test]
fn splitting_skips_clips_the_time_does_not_cut_through() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap(); // 0..5 s
    let main_clip = s.doc.project.timeline().main_track().clips[0].id;
    let created = s.doc.split_clips(&[a, main_clip], 8 * US_PER_SEC).unwrap();
    assert_eq!(created.len(), 1, "only the main clip reaches 8 s");
    assert_eq!(clips_of(&s.doc, overlay).len(), 1);
    assert!(
        s.doc.split_clips(&[a], 8 * US_PER_SEC).is_err(),
        "nothing to cut at all is reported"
    );
}

#[test]
fn cutting_away_one_side_of_overlay_and_main_clips_in_one_step() {
    let mut s = setup();
    let overlay = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(overlay, s.photo, 0).unwrap(); // 0..5
    let main_clip = s.doc.project.timeline().main_track().clips[0].id; // 0..20
    let before = s.doc.project.timeline().clone();
    s.doc
        .cut_clips_at(&[a, main_clip], 2 * US_PER_SEC, true)
        .unwrap(); // delete left
    assert_eq!(clips_of(&s.doc, overlay)[0].start, 2 * US_PER_SEC);
    assert_eq!(clips_of(&s.doc, overlay)[0].duration, 3 * US_PER_SEC);
    assert_eq!(
        s.doc.project.timeline().main_track().clips[0].duration,
        18 * US_PER_SEC
    );
    assert!(s.doc.undo());
    assert_eq!(s.doc.project.timeline(), &before);
    s.doc
        .cut_clips_at(&[a, main_clip], 2 * US_PER_SEC, false)
        .unwrap(); // delete right
    assert_eq!(clips_of(&s.doc, overlay)[0].duration, 2 * US_PER_SEC);
    assert_eq!(
        s.doc.project.timeline().main_track().clips[0].duration,
        2 * US_PER_SEC
    );
}

#[test]
fn dropping_media_on_a_new_track_is_one_undo_step() {
    let mut s = setup();
    let before = s.doc.project.timeline().clone();
    let (track, clip) = s
        .doc
        .place_media_new_track(s.photo, 3 * US_PER_SEC)
        .unwrap();
    let tl = s.doc.project.timeline();
    assert_eq!(tl.track(track).unwrap().kind, TrackKind::Overlay);
    assert_eq!(tl.clip(clip).unwrap().start, 3 * US_PER_SEC);
    let (audio_track, _) = s.doc.place_media_new_track(s.music, 0).unwrap();
    assert_eq!(
        s.doc.project.timeline().track(audio_track).unwrap().kind,
        TrackKind::Audio
    );
    assert!(s.doc.undo());
    assert!(s.doc.undo());
    assert_eq!(
        s.doc.project.timeline(),
        &before,
        "everything, track included, is gone"
    );
}

#[test]
fn several_clips_shift_together_and_never_onto_each_other() {
    let mut s = setup();
    let o = s.doc.add_track(TrackKind::Overlay).unwrap();
    let a = s.doc.place_media(o, s.photo, 0).unwrap(); // 0..5
    let b = s.doc.place_media(o, s.photo, 6 * US_PER_SEC).unwrap(); // 6..11
    s.doc.shift_clips(&[a, b], 2 * US_PER_SEC, None).unwrap();
    let c = clips_of(&s.doc, o);
    assert_eq!((c[0].start, c[1].start), (2 * US_PER_SEC, 8 * US_PER_SEC));
    // shifting only the first one into the second is refused and changes nothing
    assert!(matches!(
        s.doc.shift_clips(&[a], 5 * US_PER_SEC, None),
        Err(DocError::Edit(EditError::Overlap))
    ));
    assert_eq!(clips_of(&s.doc, o)[0].start, 2 * US_PER_SEC);
    // before the start of the timeline is refused too
    assert!(s.doc.shift_clips(&[a, b], -3 * US_PER_SEC, None).is_err());
    assert!(s.doc.undo());
    assert_eq!(clips_of(&s.doc, o)[0].start, 0);
}

#[test]
fn main_track_clips_are_not_shifted() {
    let mut s = setup();
    let main_clip = s.doc.project.timeline().main_track().clips[0].id;
    let before = s.doc.project.timeline().clone();
    s.doc
        .shift_clips(&[main_clip], 3 * US_PER_SEC, None)
        .unwrap();
    assert_eq!(s.doc.project.timeline(), &before);
}

#[test]
fn track_acceptance_rules() {
    let s = setup();
    let pic = Clip::media_clip(s.photo, 0, 0, 10);
    let snd = Clip::media_clip(s.music, 0, 0, 10);
    let txt = Clip::text_clip(TextContent::new("x"), 0, 10);
    for (clip, main, overlay, text, audio) in [
        (&pic, true, true, false, false),
        (&snd, false, false, false, true),
        (&txt, false, true, true, false),
    ] {
        assert_eq!(track_accepts(&s.doc.project, TrackKind::Main, clip), main);
        assert_eq!(
            track_accepts(&s.doc.project, TrackKind::Overlay, clip),
            overlay
        );
        assert_eq!(track_accepts(&s.doc.project, TrackKind::Text, clip), text);
        assert_eq!(track_accepts(&s.doc.project, TrackKind::Audio, clip), audio);
    }
}

#[test]
fn text_is_added_to_the_first_text_track_with_room_or_a_new_one() {
    let mut doc = Document::new(Project::new("t"));
    let (track, first) = doc
        .add_text_auto(0, 3 * US_PER_SEC, TextContent::new("One"))
        .unwrap();
    assert_eq!(
        doc.project.timeline().track(track).unwrap().kind,
        TrackKind::Text
    );
    // at the same time the first track is busy, so a second text track appears
    let (second_track, second) = doc
        .add_text_auto(US_PER_SEC, 3 * US_PER_SEC, TextContent::new("Two"))
        .unwrap();
    assert_ne!(track, second_track);
    // later, after the first text ends, the first track is reused
    let (third_track, _) = doc
        .add_text_auto(4 * US_PER_SEC, US_PER_SEC, TextContent::new("Three"))
        .unwrap();
    assert_eq!(third_track, track);
    assert!(doc.project.timeline().clip(first).is_some());
    assert!(doc.project.timeline().clip(second).is_some());
    // adding a track and its text is one undo step
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(doc.project.timeline().track(second_track).is_none());
}

#[test]
fn locked_text_tracks_are_skipped() {
    let mut doc = Document::new(Project::new("t"));
    let (track, _) = doc
        .add_text_auto(0, US_PER_SEC, TextContent::new("One"))
        .unwrap();
    doc.set_track_flags(track, |f| f.locked = true).unwrap();
    let (other, _) = doc
        .add_text_auto(5 * US_PER_SEC, US_PER_SEC, TextContent::new("Two"))
        .unwrap();
    assert_ne!(other, track);
}

#[test]
fn sound_added_to_the_timeline_goes_on_audio_tracks() {
    let mut doc = Document::new(Project::new("t"));
    let a = doc.import_media(asset("a", MediaKind::Audio, 4)).unwrap();
    let b = doc.import_media(asset("b", MediaKind::Audio, 4)).unwrap();
    let first = doc.add_media_auto(a, 0).unwrap();
    let track_of_first = doc.project.timeline().track_of(first).unwrap().id;
    assert_eq!(
        doc.project.timeline().track_of(first).unwrap().kind,
        TrackKind::Audio
    );
    // a second sound at the same time needs another track; one after the first fits on the same track
    let second = doc.add_media_auto(b, US_PER_SEC).unwrap();
    assert_ne!(
        doc.project.timeline().track_of(second).unwrap().id,
        track_of_first
    );
    let third = doc.add_media_auto(b, 6 * US_PER_SEC).unwrap();
    assert_eq!(
        doc.project.timeline().track_of(third).unwrap().id,
        track_of_first
    );
}

#[test]
fn shapes_go_on_overlay_tracks_only() {
    use crate::content::{ShapeContent, ShapeKind};
    let mut doc = Document::new(Project::new("t"));
    let (track, first) = doc
        .add_shape_auto(0, 3 * US_PER_SEC, ShapeContent::new(ShapeKind::Star))
        .unwrap();
    assert_eq!(
        doc.project.timeline().track(track).unwrap().kind,
        TrackKind::Overlay
    );
    // busy at the same time: a second overlay track
    let (other, _) = doc
        .add_shape_auto(
            US_PER_SEC,
            3 * US_PER_SEC,
            ShapeContent::new(ShapeKind::Heart),
        )
        .unwrap();
    assert_ne!(track, other);
    assert!(doc.project.timeline().clip(first).is_some());
    // a shape cannot be dropped on the text or audio tracks
    let text_track = doc.add_track(TrackKind::Text).unwrap();
    let clip = doc.project.timeline().clip(first).unwrap().clone();
    assert!(!track_accepts(&doc.project, TrackKind::Text, &clip));
    assert!(!track_accepts(&doc.project, TrackKind::Audio, &clip));
    assert!(track_accepts(&doc.project, TrackKind::Overlay, &clip));
    let _ = text_track;
}

fn doc_with_text_and_main() -> (Document, ClipId, ClipId) {
    let mut doc = Document::new(Project::new("t"));
    let video = doc.import_media(asset("v", MediaKind::Video, 10)).unwrap();
    let main_clip = doc.add_clip(video, 0).unwrap();
    let (_, text) = doc
        .add_text_auto(2 * US_PER_SEC, 3 * US_PER_SEC, TextContent::new("Hi"))
        .unwrap();
    (doc, main_clip, text)
}

#[test]
fn a_pasted_text_lands_at_the_playhead_on_its_own_track_as_a_new_clip() {
    let (mut doc, _, text) = doc_with_text_and_main();
    let track = doc.project.timeline().track_of(text).unwrap().id;
    let items = doc.copy_clips(&[text]);
    let created = doc.paste_clips(&items, 6 * US_PER_SEC).unwrap();
    assert_eq!(created.len(), 1);
    assert_ne!(created[0], text);
    let pasted = doc.project.timeline().clip(created[0]).unwrap();
    assert_eq!(pasted.start, 6 * US_PER_SEC);
    assert_eq!(
        doc.project.timeline().track_of(created[0]).unwrap().id,
        track
    );
    // one undo removes the paste and nothing else
    assert!(doc.undo());
    assert!(doc.project.timeline().clip(created[0]).is_none());
    assert!(doc.project.timeline().clip(text).is_some());
}

#[test]
fn pasting_over_something_moves_the_copy_to_the_next_free_spot() {
    let (mut doc, _, text) = doc_with_text_and_main();
    let items = doc.copy_clips(&[text]);
    let created = doc.paste_clips(&items, 3 * US_PER_SEC).unwrap();
    let pasted = doc.project.timeline().clip(created[0]).unwrap();
    assert_eq!(
        pasted.start,
        5 * US_PER_SEC,
        "right after the original, which ends at 5 s"
    );
}

#[test]
fn main_track_clips_are_pasted_in_order_and_the_track_stays_packed() {
    let (mut doc, main_clip, _) = doc_with_text_and_main();
    let items = doc.copy_clips(&[main_clip]);
    let created = doc.paste_clips(&items, 10 * US_PER_SEC).unwrap();
    let main = doc.project.timeline().main_track();
    assert_eq!(main.clips.len(), 2);
    assert_eq!(main.clips[1].id, created[0]);
    assert_eq!(main.clips[1].start, main.clips[0].end());
}

#[test]
fn duplicate_puts_the_copy_right_after_the_original() {
    let (mut doc, _, text) = doc_with_text_and_main();
    let created = doc.duplicate_clips(&[text]).unwrap();
    let copy = doc.project.timeline().clip(created[0]).unwrap();
    assert_eq!(copy.start, 5 * US_PER_SEC);
    assert_eq!(copy.duration, 3 * US_PER_SEC);
}

#[test]
fn copies_of_linked_clips_are_linked_to_each_other_not_to_the_originals() {
    use crate::ids::LinkId;
    let (mut doc, main_clip, text) = doc_with_text_and_main();
    let link = LinkId::new();
    for id in [main_clip, text] {
        doc.update_clip(id, "link", None, |c| c.link = Some(link))
            .unwrap();
    }
    let items = doc.copy_clips(&[main_clip, text]);
    let created = doc.paste_clips(&items, 12 * US_PER_SEC).unwrap();
    let links: Vec<_> = created
        .iter()
        .map(|id| doc.project.timeline().clip(*id).unwrap().link.unwrap())
        .collect();
    assert_eq!(links[0], links[1]);
    assert_ne!(links[0], link);
}

#[test]
fn paste_into_a_locked_track_goes_to_another_track_of_the_same_kind() {
    let (mut doc, _, text) = doc_with_text_and_main();
    let items = doc.copy_clips(&[text]);
    let track = doc.project.timeline().track_of(text).unwrap().id;
    doc.set_track_flags(track, |f| f.locked = true).unwrap();
    let created = doc.paste_clips(&items, 8 * US_PER_SEC).unwrap();
    assert_ne!(
        doc.project.timeline().track_of(created[0]).unwrap().id,
        track
    );
}

#[test]
fn a_marker_is_added_then_removed_by_the_same_key_and_both_can_be_undone() {
    let mut doc = Document::new(Project::new("t"));
    assert!(doc.toggle_marker(2 * US_PER_SEC).unwrap());
    assert!(doc.toggle_marker(5 * US_PER_SEC).unwrap());
    assert_eq!(doc.project.timeline().markers.len(), 2);
    assert!(!doc.toggle_marker(2 * US_PER_SEC).unwrap());
    assert_eq!(doc.project.timeline().markers.len(), 1);
    assert!(doc.undo());
    assert_eq!(doc.project.timeline().markers.len(), 2);
    assert!(doc.project.timeline().markers[0].time < doc.project.timeline().markers[1].time);
}

fn doc_with_two_main_clips() -> (Document, ClipId, ClipId) {
    let mut doc = Document::new(Project::new("t"));
    let v = doc.import_media(asset("v", MediaKind::Video, 10)).unwrap();
    let a = doc.add_clip(v, 0).unwrap();
    let b = doc.add_clip(v, 1).unwrap();
    (doc, a, b)
}

fn dissolve(after: ClipId, seconds: i64) -> Transition {
    Transition {
        after,
        preset: "dissolve".into(),
        duration: seconds * US_PER_SEC,
        easing: vz_anim::Easing::Linear,
    }
}

use crate::timeline::Transition;

#[test]
fn a_transition_joins_two_clips_and_undo_removes_it() {
    let (mut doc, a, _) = doc_with_two_main_clips();
    doc.set_transition(a, Some(dissolve(a, 1)), None).unwrap();
    assert_eq!(doc.project.timeline().main_track().transitions.len(), 1);
    assert!(doc.undo());
    assert!(doc.project.timeline().main_track().transitions.is_empty());
}

#[test]
fn the_last_clip_cannot_have_a_transition_after_it() {
    let (mut doc, _, b) = doc_with_two_main_clips();
    assert!(doc.set_transition(b, Some(dissolve(b, 1)), None).is_err());
}

#[test]
fn replacing_and_removing_a_transition() {
    let (mut doc, a, _) = doc_with_two_main_clips();
    doc.set_transition(a, Some(dissolve(a, 1)), None).unwrap();
    doc.set_transition(a, Some(dissolve(a, 2)), None).unwrap();
    let t = &doc.project.timeline().main_track().transitions;
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].duration, 2 * US_PER_SEC);
    doc.set_transition(a, None, None).unwrap();
    assert!(doc.project.timeline().main_track().transitions.is_empty());
}

#[test]
fn dragging_a_duration_slider_is_one_undo_step() {
    let (mut doc, a, _) = doc_with_two_main_clips();
    doc.set_transition(a, Some(dissolve(a, 1)), None).unwrap();
    for secs in [2, 3, 4] {
        doc.set_transition(a, Some(dissolve(a, secs)), Some(77))
            .unwrap();
    }
    doc.seal_history();
    assert!(doc.undo());
    assert_eq!(
        doc.project.timeline().main_track().transitions[0].duration,
        US_PER_SEC
    );
}

#[test]
fn a_running_transition_shows_both_clips_around_the_cut() {
    let (mut doc, a, b) = doc_with_two_main_clips();
    doc.set_transition(a, Some(dissolve(a, 2)), None).unwrap();
    let cut = doc.project.timeline().clip(a).unwrap().end();
    let tl = doc.project.timeline();
    // well before the transition: just the first clip
    let layers = tl.layers_at(cut - 2 * US_PER_SEC);
    assert!(layers[0].transition.is_none() && layers[0].clip.id == a);
    // at the start of the one second before the cut: running, progress 0, both clips known
    let layers = tl.layers_at(cut - US_PER_SEC);
    let tr = layers[0].transition.unwrap();
    assert_eq!(tr.incoming.id, b);
    assert!(tr.progress.abs() < 1e-6);
    // exactly on the cut: halfway
    let layers = tl.layers_at(cut);
    assert!((layers[0].transition.unwrap().progress - 0.5).abs() < 1e-3);
    assert_eq!(
        layers[0].clip.id, a,
        "the outgoing clip stays the layer's clip"
    );
    // the incoming clip is shown from before its start: its media time is held at the first picture
    assert_eq!(layers[0].transition.unwrap().incoming_source_time, 0);
    // after the transition: only the second clip
    let layers = tl.layers_at(cut + US_PER_SEC);
    assert!(layers[0].transition.is_none() && layers[0].clip.id == b);
}

#[test]
fn a_transition_never_lasts_longer_than_the_shorter_clip() {
    let (mut doc, a, b) = doc_with_two_main_clips();
    doc.trim_right(b, US_PER_SEC, None).ok();
    doc.set_transition(a, Some(dissolve(a, 60)), None).unwrap();
    let main = doc.project.timeline().main_track();
    let (tr, ca, cb) = main.transition_after(a).unwrap();
    assert!(tr.effective(ca, cb) <= cb.duration);
}

#[test]
fn apply_to_all_puts_one_on_every_cut_in_one_step() {
    let mut doc = Document::new(Project::new("t"));
    let v = doc.import_media(asset("v", MediaKind::Video, 10)).unwrap();
    for _ in 0..4 {
        doc.add_clip(v, usize::MAX).unwrap();
    }
    let n = doc
        .apply_transition_to_all("dissolve", US_PER_SEC, vz_anim::Easing::Linear)
        .unwrap();
    assert_eq!(n, 3);
    assert_eq!(doc.project.timeline().main_track().transitions.len(), 3);
    assert!(doc.undo());
    assert!(doc.project.timeline().main_track().transitions.is_empty());
}

#[test]
fn transitions_are_saved_with_the_track_and_old_projects_load_without() {
    let (mut doc, a, _) = doc_with_two_main_clips();
    doc.set_transition(a, Some(dissolve(a, 1)), None).unwrap();
    let json = serde_json::to_string(&doc.project).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.timeline().main_track().transitions.len(), 1);
}

fn doc_with_one_clip(seconds: i64) -> (Document, ClipId) {
    let mut doc = Document::new(Project::new("t"));
    let v = doc
        .import_media(asset("v", MediaKind::Video, seconds))
        .unwrap();
    let id = doc.add_clip(v, 0).unwrap();
    (doc, id)
}

#[test]
fn double_speed_halves_the_clip_and_keeps_the_same_part_of_the_file() {
    let (mut doc, id) = doc_with_one_clip(10);
    doc.set_speed(id, 2.0, None).unwrap();
    let c = doc.project.timeline().clip(id).unwrap();
    assert_eq!(c.duration, 5 * US_PER_SEC);
    assert_eq!(c.source_span(), 10 * US_PER_SEC);
    // half speed doubles it again, relative to the original 10 s of media
    doc.set_speed(id, 0.5, None).unwrap();
    assert_eq!(
        doc.project.timeline().clip(id).unwrap().duration,
        20 * US_PER_SEC
    );
    assert!(doc.undo());
    assert_eq!(
        doc.project.timeline().clip(id).unwrap().duration,
        5 * US_PER_SEC
    );
}

#[test]
fn the_main_track_closes_up_after_a_speed_change() {
    let mut doc = Document::new(Project::new("t"));
    let v = doc.import_media(asset("v", MediaKind::Video, 10)).unwrap();
    let a = doc.add_clip(v, 0).unwrap();
    let b = doc.add_clip(v, 1).unwrap();
    doc.set_speed(a, 2.0, None).unwrap();
    let main = doc.project.timeline().main_track();
    assert_eq!(main.clips[1].id, b);
    assert_eq!(main.clips[1].start, 5 * US_PER_SEC);
}

#[test]
fn a_speed_change_that_would_run_into_the_next_clip_on_a_free_track_is_refused() {
    let mut doc = Document::new(Project::new("t"));
    let v = doc.import_media(asset("v", MediaKind::Video, 4)).unwrap();
    doc.add_clip(v, 0).unwrap();
    let track = doc.add_track(TrackKind::Overlay).unwrap();
    let a = doc.place_media(track, v, 0).unwrap();
    doc.place_media(track, v, 4 * US_PER_SEC).unwrap();
    assert!(
        doc.set_speed(a, 0.5, None).is_err(),
        "twice as long would overlap"
    );
    assert!(doc.set_speed(a, 2.0, None).is_ok());
}

#[test]
fn a_speed_curve_keeps_the_media_and_sets_the_length_from_the_average() {
    let (mut doc, id) = doc_with_one_clip(10);
    // 1x to 3x: the average is 2x, so 10 s of media take 5 s
    doc.set_speed_curve(id, &[(0.0, 1.0), (1.0, 3.0)]).unwrap();
    let c = doc.project.timeline().clip(id).unwrap();
    assert!(
        (c.duration - 5 * US_PER_SEC).abs() < 50_000,
        "{}",
        c.duration
    );
    assert!(
        (c.source_span() - 10 * US_PER_SEC).abs() < 100_000,
        "{}",
        c.source_span()
    );
    assert!(c.speed.is_animated());
    // a flat 1x curve is the original length
    doc.set_speed_curve(id, &[(0.0, 1.0), (1.0, 1.0)]).unwrap();
    assert!((doc.project.timeline().clip(id).unwrap().duration - 10 * US_PER_SEC).abs() < 100_000);
    assert!(doc.set_speed_curve(id, &[(0.0, 0.0), (1.0, 0.0)]).is_err());
}

#[test]
fn reverse_plays_the_same_part_backwards_and_cuts_still_work() {
    let (mut doc, id) = doc_with_one_clip(10);
    doc.set_reverse(id, true).unwrap();
    let c = doc.project.timeline().clip(id).unwrap().clone();
    assert_eq!(c.source_time(0), 10 * US_PER_SEC);
    // cut in the middle: the first half plays the top of the file, the second half the bottom
    let new = doc.split_clip(id, 4 * US_PER_SEC).unwrap();
    let left = doc.project.timeline().clip(id).unwrap().clone();
    let right = doc.project.timeline().clip(new).unwrap().clone();
    assert_eq!(left.source_time(0), 10 * US_PER_SEC);
    assert_eq!(left.source_time(4 * US_PER_SEC), 6 * US_PER_SEC);
    assert_eq!(
        right.source_time(4 * US_PER_SEC),
        6 * US_PER_SEC,
        "the second half carries on where the first ends"
    );
    assert_eq!(right.source_time(10 * US_PER_SEC), 0);
}

#[test]
fn a_cut_in_a_sped_up_clip_continues_seamlessly() {
    let (mut doc, id) = doc_with_one_clip(10);
    doc.set_speed(id, 2.0, None).unwrap(); // 5 s long, uses 0..10 s of the file
    let new = doc.split_clip(id, 2 * US_PER_SEC).unwrap();
    let left = doc.project.timeline().clip(id).unwrap().clone();
    let right = doc.project.timeline().clip(new).unwrap().clone();
    assert_eq!(left.source_time(2 * US_PER_SEC), 4 * US_PER_SEC);
    assert_eq!(right.source_time(2 * US_PER_SEC), 4 * US_PER_SEC);
    assert_eq!(right.source_time(5 * US_PER_SEC), 10 * US_PER_SEC);
}

#[test]
fn trimming_a_sped_up_clip_moves_the_media_by_the_speed() {
    let (mut doc, id) = doc_with_one_clip(10);
    doc.set_speed(id, 2.0, None).unwrap();
    doc.trim_left(id, US_PER_SEC, None).unwrap(); // one timeline second = two seconds of media
    let c = doc.project.timeline().clip(id).unwrap();
    assert_eq!(c.source_in, 2 * US_PER_SEC);
    assert_eq!(c.duration, 4 * US_PER_SEC);
    // the end cannot be dragged past the end of the file: 8 s of media left = 4 s
    assert!(doc.trim_right(id, 4 * US_PER_SEC + 200_000, None).is_err());
    assert!(doc.trim_right(id, 4 * US_PER_SEC, None).is_ok());
}

#[test]
fn trimming_a_reversed_clip_cuts_the_right_end_of_the_part_of_the_file() {
    let (mut doc, id) = doc_with_one_clip(10);
    doc.trim_right(id, 6 * US_PER_SEC, None).unwrap(); // uses 0..6 s
    doc.set_reverse(id, true).unwrap();
    // shorten from the left: the clip starts later, which is lower in the file (the top end is removed)
    doc.trim_left(id, 2 * US_PER_SEC, None).unwrap();
    let c = doc.project.timeline().clip(id).unwrap();
    assert_eq!(c.source_in, 0);
    assert_eq!(
        c.source_time(0),
        4 * US_PER_SEC,
        "now starts at 4 s of the file, playing down to 0"
    );
    // shorten from the right: the bottom end is removed
    doc.trim_right(id, 3 * US_PER_SEC, None).unwrap();
    let c = doc.project.timeline().clip(id).unwrap();
    assert_eq!(c.source_in, US_PER_SEC);
    assert_eq!(c.source_time(0), 4 * US_PER_SEC);
    assert_eq!(c.source_time(3 * US_PER_SEC), US_PER_SEC);
}

#[test]
fn a_freeze_frame_cuts_the_clip_and_inserts_a_still() {
    let (mut doc, id) = doc_with_one_clip(10);
    let still = doc
        .freeze_frame(id, 4 * US_PER_SEC, 3 * US_PER_SEC)
        .unwrap();
    let main = doc.project.timeline().main_track();
    assert_eq!(main.clips.len(), 3);
    assert_eq!(main.clips[1].id, still);
    assert_eq!(main.clips[1].start, 4 * US_PER_SEC);
    assert_eq!(main.clips[1].duration, 3 * US_PER_SEC);
    assert_eq!(
        main.clips[2].start,
        7 * US_PER_SEC,
        "what follows moves later"
    );
    // the still shows the picture at 4 s of the file for its whole length
    assert_eq!(main.clips[1].source_time(4 * US_PER_SEC), 4 * US_PER_SEC);
    assert_eq!(main.clips[1].source_time(6 * US_PER_SEC), 4 * US_PER_SEC);
    assert_eq!(
        main.clips[1].volume.value_at(0),
        0.0,
        "no sound from a still"
    );
    // the second half carries on from 4 s
    assert_eq!(main.clips[2].source_in, 4 * US_PER_SEC);
    // everything is one undo step
    assert!(doc.undo());
    assert_eq!(doc.project.timeline().main_track().clips.len(), 1);
}

#[test]
fn a_freeze_at_the_very_start_or_on_text_is_refused() {
    let (mut doc, id) = doc_with_one_clip(10);
    assert!(doc.freeze_frame(id, 0, US_PER_SEC).is_err());
    let (_, text) = doc
        .add_text_auto(0, US_PER_SEC, TextContent::new("t"))
        .unwrap();
    assert!(doc.freeze_frame(text, US_PER_SEC / 2, US_PER_SEC).is_err());
}
