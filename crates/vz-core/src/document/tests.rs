use super::*;
use crate::media::MediaKind;
use crate::time::US_PER_SEC;
use uuid::Uuid;

fn video(name: &str, secs: i64, path: &Path) -> MediaAsset {
    MediaAsset {
        id: MediaId::new(),
        name: name.into(),
        path: path.to_path_buf(),
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

fn image(path: &Path) -> MediaAsset {
    MediaAsset {
        kind: MediaKind::Image,
        duration: 0,
        has_audio: false,
        frame_rate: None,
        ..video("pic", 0, path)
    }
}

/// A scratch folder that is removed when it goes out of scope.
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("vidzcut-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn doc_with_clips(lengths: &[i64]) -> (Document, Vec<ClipId>) {
    let mut doc = Document::new(Project::new("test"));
    let mut ids = Vec::new();
    for (i, secs) in lengths.iter().enumerate() {
        let id = doc
            .import_media(video(
                &format!("v{i}"),
                60,
                Path::new(&format!("/media/v{i}.mp4")),
            ))
            .unwrap();
        let clip = doc.add_clip(id, usize::MAX).unwrap();
        doc.trim_right(clip, secs * US_PER_SEC, None).unwrap();
        ids.push(clip);
    }
    (doc, ids)
}

fn clips(doc: &Document) -> Vec<Clip> {
    doc.project.timeline().main_track().clips.clone()
}

#[test]
fn adding_a_clip_uses_the_media_length() {
    let mut doc = Document::new(Project::new("t"));
    let id = doc
        .import_media(video("a", 7, Path::new("/m/a.mp4")))
        .unwrap();
    doc.add_clip(id, 0).unwrap();
    let c = &clips(&doc)[0];
    assert_eq!((c.start, c.source_in, c.duration), (0, 0, 7 * US_PER_SEC));
    assert_eq!(doc.project.duration(), 7 * US_PER_SEC);
}

#[test]
fn images_get_the_default_length() {
    let mut doc = Document::new(Project::new("t"));
    let id = doc.import_media(image(Path::new("/m/a.png"))).unwrap();
    doc.add_clip(id, 0).unwrap();
    assert_eq!(clips(&doc)[0].duration, 5 * US_PER_SEC);
}

#[test]
fn importing_the_same_file_twice_gives_one_asset() {
    let mut doc = Document::new(Project::new("t"));
    let a = doc
        .import_media(video("a", 3, Path::new("/m/a.mp4")))
        .unwrap();
    let b = doc
        .import_media(video("a again", 3, Path::new("/m/a.mp4")))
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(doc.project.media.len(), 1);
}

#[test]
fn adding_unknown_media_fails_cleanly() {
    let mut doc = Document::new(Project::new("t"));
    assert!(doc.add_clip(MediaId::new(), 0).is_err());
    assert!(!doc.can_undo());
}

#[test]
fn split_at_the_playhead_cuts_the_clip_under_it() {
    let (mut doc, ids) = doc_with_clips(&[4, 4]);
    let right = doc.split_at(6 * US_PER_SEC).unwrap();
    let c = clips(&doc);
    assert_eq!(c.len(), 3);
    assert_eq!(c[1].duration, 2 * US_PER_SEC);
    assert_eq!(c[2].id, right);
    assert_eq!(c[0].id, ids[0]);
    assert!(doc.split_at(99 * US_PER_SEC).is_err());
}

#[test]
fn every_edit_can_be_undone_and_redone() {
    let (mut doc, ids) = doc_with_clips(&[4, 4, 4]);
    let start = clips(&doc);
    let mut states = vec![start.clone()];

    doc.split_at(2 * US_PER_SEC).unwrap();
    states.push(clips(&doc));
    doc.trim_left(ids[1], US_PER_SEC, None).unwrap();
    states.push(clips(&doc));
    doc.move_clip(ids[2], 0).unwrap();
    states.push(clips(&doc));
    doc.delete_clips(&[ids[0]]).unwrap();
    states.push(clips(&doc));

    for expected in states.iter().rev().skip(1) {
        assert!(doc.undo());
        assert_eq!(&clips(&doc), expected);
    }
    assert_eq!(clips(&doc), start);
    for expected in states.iter().skip(1) {
        assert!(doc.redo());
        assert_eq!(&clips(&doc), expected);
    }
    assert!(!doc.redo());
}

#[test]
fn a_new_edit_clears_redo() {
    let (mut doc, ids) = doc_with_clips(&[4]);
    doc.split_at(US_PER_SEC).unwrap();
    doc.undo();
    assert!(doc.can_redo());
    doc.delete_clips(&[ids[0]]).unwrap();
    assert!(!doc.can_redo());
}

#[test]
fn a_drag_is_one_undo_step() {
    let (mut doc, ids) = doc_with_clips(&[10]);
    let before = clips(&doc);
    for i in 1..=8 {
        doc.trim_left(ids[0], 0, Some(7)).unwrap_or(());
        doc.trim_right(ids[0], (10 - i) * US_PER_SEC, Some(7))
            .unwrap();
    }
    assert_eq!(clips(&doc)[0].duration, 2 * US_PER_SEC);
    doc.seal_history();
    assert!(doc.undo());
    assert_eq!(clips(&doc), before);
}

#[test]
fn two_drags_are_two_steps() {
    let (mut doc, ids) = doc_with_clips(&[10]);
    doc.trim_right(ids[0], 8 * US_PER_SEC, Some(1)).unwrap();
    doc.trim_right(ids[0], 6 * US_PER_SEC, Some(1)).unwrap();
    doc.seal_history();
    doc.trim_right(ids[0], 4 * US_PER_SEC, Some(2)).unwrap();
    doc.undo();
    assert_eq!(clips(&doc)[0].duration, 6 * US_PER_SEC);
    doc.undo();
    assert_eq!(clips(&doc)[0].duration, 10 * US_PER_SEC);
}

#[test]
fn locked_tracks_refuse_edits_and_nothing_changes() {
    let (mut doc, ids) = doc_with_clips(&[4]);
    doc.project.timeline_mut().main_track_mut().locked = true;
    let before = clips(&doc);
    assert!(matches!(
        doc.delete_clips(&ids),
        Err(DocError::Edit(EditError::Locked))
    ));
    assert_eq!(clips(&doc), before);
}

#[test]
fn dirty_flag_follows_saving() {
    let dir = TempDir::new();
    let mut doc = Document::new(Project::new("t"));
    assert!(!doc.is_dirty());
    doc.rename("Changed", None).unwrap();
    assert!(doc.is_dirty());
    doc.save(Some(&dir.0.join("p.vzp"))).unwrap();
    assert!(!doc.is_dirty());
    doc.undo();
    assert!(doc.is_dirty());
}

#[test]
fn save_without_a_path_is_an_error() {
    assert!(Document::new(Project::new("t")).save(None).is_err());
}

#[test]
fn save_and_open_round_trip() {
    let dir = TempDir::new();
    let (mut doc, _) = doc_with_clips(&[4, 3]);
    doc.rename("ශ්‍රී ලංකාව", None).unwrap();
    let file = dir.0.join("my project.vzp");
    doc.save(Some(&file)).unwrap();
    assert!(
        !file.with_extension("vzp.tmp").exists(),
        "no temporary file left behind"
    );
    let back = Document::open(&file).unwrap();
    assert_eq!(back.project.name, "ශ්‍රී ලංකාව");
    assert_eq!(back.project.timeline(), doc.project.timeline());
    assert_eq!(back.path.as_deref(), Some(file.as_path()));
    assert!(!back.is_dirty());
}

#[test]
fn opening_a_missing_or_broken_file_is_an_error_not_a_crash() {
    let dir = TempDir::new();
    assert!(matches!(
        Document::open(&dir.0.join("nope.vzp")),
        Err(DocError::Io { .. })
    ));
    let bad = dir.0.join("bad.vzp");
    std::fs::write(&bad, "{ this is not valid").unwrap();
    assert!(matches!(Document::open(&bad), Err(DocError::Load(_))));
}

#[test]
fn media_is_found_again_when_the_project_folder_moves() {
    let root = TempDir::new();
    let old = root.0.join("old");
    std::fs::create_dir_all(old.join("media")).unwrap();
    let media_file = old.join("media").join("clip.mp4");
    std::fs::write(&media_file, b"x").unwrap();

    let mut doc = Document::new(Project::new("t"));
    doc.import_media(video("clip", 5, &media_file)).unwrap();
    doc.save(Some(&old.join("p.vzp"))).unwrap();
    assert_eq!(
        doc.project.media[0].relative_path.as_deref(),
        Some(Path::new("media/clip.mp4"))
    );

    let new = root.0.join("new");
    std::fs::rename(&old, &new).unwrap();
    let back = Document::open(&new.join("p.vzp")).unwrap();
    assert_eq!(
        back.project.media[0].path,
        new.join("media").join("clip.mp4")
    );
    assert!(back.project.missing_media().is_empty());
}

#[test]
fn missing_media_is_reported() {
    let mut doc = Document::new(Project::new("t"));
    let id = doc
        .import_media(video("gone", 5, Path::new("/definitely/not/here.mp4")))
        .unwrap();
    assert_eq!(doc.project.missing_media(), vec![id]);
}

#[test]
fn relative_paths() {
    assert_eq!(
        relative_path(Path::new("/a/b"), Path::new("/a/b/c/d.mp4")),
        Some(PathBuf::from("c/d.mp4"))
    );
    assert_eq!(
        relative_path(Path::new("/a/b"), Path::new("/a/x/d.mp4")),
        Some(PathBuf::from("../x/d.mp4"))
    );
    assert_eq!(
        relative_path(Path::new("/a/b"), Path::new("/a/b/d.mp4")),
        Some(PathBuf::from("d.mp4"))
    );
    if cfg!(windows) {
        assert_eq!(
            relative_path(Path::new(r"C:\a"), Path::new(r"D:\a\b.mp4")),
            None
        );
    }
}

#[test]
fn autosave_and_recovery() {
    let dir = TempDir::new();
    let (mut doc, _) = doc_with_clips(&[4]);
    doc.rename("Unsaved work", None).unwrap();
    doc.autosave(&dir.0).unwrap();

    let found = Document::recovery_items(&dir.0);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Unsaved work");

    let restored = Document::restore(&found[0]).unwrap();
    assert_eq!(restored.project, doc.project);
    assert!(
        restored.is_dirty(),
        "a restored project must be saved again"
    );

    doc.discard_autosave(&dir.0);
    assert!(Document::recovery_items(&dir.0).is_empty());
}

#[test]
fn random_editing_can_always_be_fully_undone() {
    let (mut doc, _) = doc_with_clips(&[5, 5, 5]);
    let start = doc.project.clone();
    let mut seed = 99u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    let mut done = 0;
    for _ in 0..200 {
        let c = clips(&doc);
        if c.is_empty() {
            break;
        }
        let pick = c[next() % c.len()].clone();
        let ok = match next() % 5 {
            0 => doc
                .split_clip(pick.id, pick.start + (next() % 4_000_000) as i64)
                .is_ok(),
            1 => doc
                .trim_left(pick.id, (next() % 2_000_000) as i64, None)
                .is_ok(),
            2 => doc
                .trim_right(pick.id, (next() % 5_000_000) as i64 + 40_000, None)
                .is_ok(),
            3 => doc.move_clip(pick.id, next() % 5).is_ok(),
            _ => {
                if c.len() > 1 {
                    doc.delete_clips(&[pick.id]).is_ok()
                } else {
                    false
                }
            }
        };
        if ok {
            done += 1;
        }
    }
    assert!(done > 20, "the random run should do real work, did {done}");
    let end = doc.project.clone();
    while doc.undo() {}
    // everything was undone back to an empty main track (the setup clips were undone too)
    assert!(clips(&doc).is_empty());
    while doc.redo() {}
    assert_eq!(doc.project.timeline(), end.timeline());
    let _ = start;
}

fn portrait_24fps(path: &Path) -> MediaAsset {
    MediaAsset {
        width: 1080,
        height: 1920,
        frame_rate: Some(FrameRate::FPS_24),
        ..video("phone", 10, path)
    }
}

#[test]
fn the_first_clip_sets_the_shape_and_frame_rate_in_one_undo_step() {
    let mut doc = Document::new(Project::new("t"));
    let id = doc
        .import_media(portrait_24fps(Path::new("/m/p.mp4")))
        .unwrap();
    doc.add_clip(id, 0).unwrap();
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1080, 1920)
    );
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_24);
    assert!(!doc.project.auto_settings);
    assert_eq!(doc.project.duration(), 10 * US_PER_SEC);

    // a second clip leaves the settings alone
    let other = doc
        .import_media(video("wide", 5, Path::new("/m/w.mp4")))
        .unwrap();
    doc.add_clip(other, usize::MAX).unwrap();
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1080, 1920)
    );

    // undo: second clip, its import, then the first clip (which also gives the settings back)
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(
        doc.project.frame_rate,
        FrameRate::FPS_24,
        "the first clip is still there"
    );
    assert!(doc.undo());
    assert!(doc.project.auto_settings);
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1920, 1080)
    );
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_30);
    assert!(clips(&doc).is_empty());
    assert!(doc.redo());
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_24);
}

#[test]
fn choosing_a_shape_by_hand_turns_off_the_automatic_settings() {
    let mut doc = Document::new(Project::new("t"));
    doc.set_aspect(AspectPreset::Square1x1).unwrap();
    assert!(!doc.project.auto_settings);
    let id = doc
        .import_media(portrait_24fps(Path::new("/m/p.mp4")))
        .unwrap();
    doc.add_clip(id, 0).unwrap();
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1080, 1080),
        "the user's choice stays"
    );
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_30);
    // take back the clip, the import and then the shape choice itself
    doc.undo();
    doc.undo();
    doc.undo();
    assert!(doc.project.auto_settings);
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1920, 1080)
    );
}

#[test]
fn a_picture_sets_the_shape_but_not_the_frame_rate() {
    let mut doc = Document::new(Project::new("t"));
    let pic = MediaAsset {
        width: 1000,
        height: 1000,
        ..image(Path::new("/m/a.png"))
    };
    let id = doc.import_media(pic).unwrap();
    doc.add_clip(id, 0).unwrap();
    assert_eq!(
        (doc.project.canvas.width, doc.project.canvas.height),
        (1080, 1080)
    );
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_30);
}

#[test]
fn odd_frame_rates_pick_the_nearest_standard_one() {
    let mut doc = Document::new(Project::new("t"));
    let a = MediaAsset {
        frame_rate: FrameRate::new(2997, 100),
        ..video("a", 4, Path::new("/m/a.mp4"))
    };
    let id = doc.import_media(a).unwrap();
    doc.add_clip(id, 0).unwrap();
    assert_eq!(doc.project.frame_rate, FrameRate::FPS_29_97);
}
