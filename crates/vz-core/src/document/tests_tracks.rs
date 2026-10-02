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
