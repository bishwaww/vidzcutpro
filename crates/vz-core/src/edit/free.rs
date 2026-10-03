//! Editing the free tracks (overlay, text, audio), as plain functions like the main-track ones.
//!
//! Free tracks do not close gaps: a clip stays exactly where it is put. Two clips on the same
//! track may not overlap (put one on another track instead). The list is always sorted by start time.

use super::{EditError, min_duration, position};
use crate::ids::ClipId;
use crate::time::{FrameRate, Micros};
use crate::timeline::Clip;

fn sorted(mut clips: Vec<Clip>) -> Vec<Clip> {
    clips.sort_by_key(|c| c.start);
    clips
}

/// Would a clip from `start` to `end` collide with another clip? (`ignore` is the clip being moved itself.)
pub fn overlaps(clips: &[Clip], start: Micros, end: Micros, ignore: Option<ClipId>) -> bool {
    clips
        .iter()
        .any(|c| Some(c.id) != ignore && c.start < end && start < c.end())
}

/// The nearest start time at or after `wanted` where a clip of `duration` fits without overlapping.
pub fn first_free_start(clips: &[Clip], wanted: Micros, duration: Micros) -> Micros {
    let mut start = wanted.max(0);
    for c in sorted(clips.to_vec()) {
        if c.end() <= start {
            continue;
        }
        if c.start >= start + duration {
            break;
        }
        start = c.end();
    }
    start
}

/// Add a clip. Fails with [`EditError::Overlap`] if something is already there.
pub fn insert(clips: &[Clip], clip: Clip) -> Result<Vec<Clip>, EditError> {
    if clip.start < 0 || overlaps(clips, clip.start, clip.end(), None) {
        return Err(EditError::Overlap);
    }
    let mut out = clips.to_vec();
    out.push(clip);
    Ok(sorted(out))
}

/// Move a clip to start at `new_start` (snapped to a frame, never before 0).
pub fn move_to(
    clips: &[Clip],
    id: ClipId,
    new_start: Micros,
    rate: FrameRate,
) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let start = rate.snap(new_start.max(0));
    if overlaps(clips, start, start + clips[i].duration, Some(id)) {
        return Err(EditError::Overlap);
    }
    let mut out = clips.to_vec();
    out[i].start = start;
    Ok(sorted(out))
}

/// Cut a clip in two at `at` (snapped to a frame). Both halves must be at least one frame long.
pub fn split(
    clips: &[Clip],
    id: ClipId,
    at: Micros,
    rate: FrameRate,
    new_id: ClipId,
) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let clip = &clips[i];
    let at = rate.snap(at);
    let min = min_duration(rate);
    if at - clip.start < min || clip.end() - at < min {
        return Err(EditError::OutOfRange);
    }
    let (left_in, right_in) = clip.split_sources(at - clip.start);
    let has_media = clip.media().is_some();
    let left = Clip {
        duration: at - clip.start,
        source_in: if has_media { left_in } else { 0 },
        ..clip.clone()
    };
    let mut right = Clip {
        id: new_id,
        start: at,
        source_in: if has_media { right_in } else { 0 },
        duration: clip.end() - at,
        ..clip.clone()
    };
    right.shift_keys(-(at - clip.start));
    let mut out = clips.to_vec();
    out[i] = left;
    out.insert(i + 1, right);
    Ok(out)
}

/// Remove clips; nothing else moves.
pub fn delete(clips: &[Clip], ids: &[ClipId]) -> Result<Vec<Clip>, EditError> {
    for id in ids {
        position(clips, *id)?;
    }
    Ok(clips
        .iter()
        .filter(|c| !ids.contains(&c.id))
        .cloned()
        .collect())
}

/// Move the left edge by `delta` (positive = shorter from the left). The right edge stays put.
/// Revealing more footage (negative delta) cannot go past the start of the file or into the clip before.
pub fn trim_left(
    clips: &[Clip],
    id: ClipId,
    delta: Micros,
    media_len: Option<Micros>,
    rate: FrameRate,
) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let delta = rate.snap(delta.abs()) * delta.signum();
    let c = &clips[i];
    if c.duration - delta < min_duration(rate) {
        return Err(EditError::TooShort);
    }
    let new_start = c.start + delta;
    if new_start < 0 || overlaps(clips, new_start, c.end(), Some(id)) {
        return Err(EditError::Overlap);
    }
    let mut out = clips.to_vec();
    out[i] = c.trimmed_left(delta, media_len)?;
    out[i].start = new_start;
    Ok(sorted(out))
}

/// Set a clip's length (moves the right edge). `media_len` is the file's length, or `None` for pictures and text.
pub fn trim_right(
    clips: &[Clip],
    id: ClipId,
    new_duration: Micros,
    media_len: Option<Micros>,
    rate: FrameRate,
) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let new_duration = rate.snap(new_duration);
    if new_duration < min_duration(rate) {
        return Err(EditError::TooShort);
    }
    if overlaps(
        clips,
        clips[i].start,
        clips[i].start + new_duration,
        Some(id),
    ) {
        return Err(EditError::Overlap);
    }
    let mut out = clips.to_vec();
    out[i] = clips[i].with_duration(new_duration, media_len)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::time::US_PER_SEC;

    const RATE: FrameRate = FrameRate::FPS_25;

    fn clip(start: i64, secs: i64) -> Clip {
        Clip::media_clip(
            MediaId::new(),
            start * US_PER_SEC,
            10 * US_PER_SEC,
            secs * US_PER_SEC,
        )
    }

    #[test]
    fn clips_stay_where_they_are_and_stay_sorted() {
        let a = clip(5, 2);
        let out = insert(std::slice::from_ref(&a), clip(1, 2)).unwrap();
        assert_eq!(
            out.iter().map(|c| c.start / US_PER_SEC).collect::<Vec<_>>(),
            vec![1, 5]
        );
    }

    #[test]
    fn overlapping_clips_are_refused_but_touching_ones_are_fine() {
        let a = clip(2, 3); // 2..5
        assert_eq!(
            insert(std::slice::from_ref(&a), clip(4, 2)),
            Err(EditError::Overlap)
        );
        assert_eq!(
            insert(std::slice::from_ref(&a), clip(1, 2)),
            Err(EditError::Overlap)
        );
        assert!(insert(std::slice::from_ref(&a), clip(5, 2)).is_ok());
        assert!(insert(&[a], clip(0, 2)).is_ok());
    }

    #[test]
    fn first_free_start_skips_over_busy_stretches() {
        let clips = vec![clip(0, 2), clip(2, 2), clip(10, 2)];
        assert_eq!(first_free_start(&clips, 0, 3 * US_PER_SEC), 4 * US_PER_SEC);
        assert_eq!(
            first_free_start(&clips, 7 * US_PER_SEC, 3 * US_PER_SEC),
            7 * US_PER_SEC
        );
        assert_eq!(
            first_free_start(&clips, 8 * US_PER_SEC, 3 * US_PER_SEC),
            12 * US_PER_SEC
        );
        assert_eq!(first_free_start(&[], -5, 10), 0);
    }

    #[test]
    fn moving_snaps_to_frames_and_refuses_collisions() {
        let (a, b) = (clip(0, 2), clip(5, 2));
        let clips = vec![a.clone(), b.clone()];
        let out = move_to(&clips, a.id, 2 * US_PER_SEC + 13_000, RATE).unwrap();
        assert_eq!(out[0].start, 2 * US_PER_SEC);
        assert_eq!(
            move_to(&clips, a.id, 4 * US_PER_SEC, RATE),
            Err(EditError::Overlap)
        );
        assert_eq!(
            move_to(&clips, a.id, -3 * US_PER_SEC, RATE).unwrap()[0].start,
            0,
            "never before 0"
        );
        // moving past another clip re-sorts
        let out = move_to(&clips, a.id, 7 * US_PER_SEC, RATE).unwrap();
        assert_eq!(out[0].id, b.id);
    }

    #[test]
    fn split_keeps_the_footage_continuous_and_changes_nothing_else() {
        let c = clip(3, 4);
        let new_id = ClipId::new();
        let out = split(std::slice::from_ref(&c), c.id, 5 * US_PER_SEC, RATE, new_id).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].start, out[0].duration),
            (3 * US_PER_SEC, 2 * US_PER_SEC)
        );
        assert_eq!(
            (out[1].start, out[1].source_in),
            (5 * US_PER_SEC, c.source_in + 2 * US_PER_SEC)
        );
        assert_eq!(out[1].id, new_id);
        assert_eq!(out[1].transform, c.transform);
    }

    #[test]
    fn delete_leaves_a_gap() {
        let (a, b, c) = (clip(0, 2), clip(3, 2), clip(6, 2));
        let out = delete(&[a, b.clone(), c], &[b.id]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].start, 6 * US_PER_SEC);
        assert!(delete(&out, &[ClipId::new()]).is_err());
    }

    #[test]
    fn trimming_the_left_edge_moves_the_start_but_not_the_end() {
        let c = clip(4, 4); // 4..8, source_in 10 s
        let out = trim_left(std::slice::from_ref(&c), c.id, US_PER_SEC, None, RATE).unwrap();
        assert_eq!(
            (out[0].start, out[0].end()),
            (5 * US_PER_SEC, 8 * US_PER_SEC)
        );
        assert_eq!(out[0].source_in, c.source_in + US_PER_SEC);
        // reveal more: starts earlier, shows earlier footage
        let out = trim_left(std::slice::from_ref(&c), c.id, -US_PER_SEC, None, RATE).unwrap();
        assert_eq!(
            (out[0].start, out[0].source_in),
            (3 * US_PER_SEC, c.source_in - US_PER_SEC)
        );
    }

    #[test]
    fn trimming_cannot_run_into_a_neighbour_or_off_the_file() {
        let (a, b) = (clip(0, 3), clip(4, 3));
        let clips = vec![a, b.clone()];
        assert_eq!(
            trim_left(&clips, b.id, -2 * US_PER_SEC, None, RATE),
            Err(EditError::Overlap)
        );
        assert!(trim_left(&clips, b.id, -US_PER_SEC, None, RATE).is_ok());
        let mut early = clip(4, 3);
        early.source_in = US_PER_SEC / 2;
        assert_eq!(
            trim_left(&[early.clone()], early.id, -US_PER_SEC, None, RATE),
            Err(EditError::BeyondMedia)
        );
        assert_eq!(
            trim_right(&clips, clips[0].id, 5 * US_PER_SEC, None, RATE),
            Err(EditError::Overlap)
        );
        assert!(trim_right(&clips, clips[0].id, 4 * US_PER_SEC, None, RATE).is_ok());
    }

    #[test]
    fn text_has_no_footage_to_run_out_of() {
        let t = Clip::text_clip(
            crate::content::TextContent::new("hello"),
            US_PER_SEC,
            2 * US_PER_SEC,
        );
        let out = trim_left(std::slice::from_ref(&t), t.id, -US_PER_SEC, None, RATE).unwrap();
        assert_eq!((out[0].start, out[0].source_in), (0, 0));
        let out = trim_right(std::slice::from_ref(&t), t.id, 600 * US_PER_SEC, None, RATE).unwrap();
        assert_eq!(out[0].duration, 600 * US_PER_SEC);
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use crate::content::TextContent;
    use crate::time::{FrameRate, US_PER_SEC};

    #[test]
    fn keyframes_stay_on_the_same_picture_when_a_clip_is_cut_or_trimmed() {
        let mut c = Clip::text_clip(TextContent::new("t"), 0, 4 * US_PER_SEC);
        c.transform.x.set_key(0, 0.0, 0);
        c.transform.x.set_key(3 * US_PER_SEC, 90.0, 0);
        let rate = FrameRate::FPS_25;
        // cut at 2 s: the second half starts at its own time 0, and its key (the one at 3 s) is now at 1 s
        let new_id = ClipId::new();
        let out = split(std::slice::from_ref(&c), c.id, 2 * US_PER_SEC, rate, new_id).unwrap();
        let right = &out[1];
        assert_eq!(right.transform.x.keys().last().unwrap().time, US_PER_SEC);
        // the picture at 2 s had x = 60 (two thirds of the way to 90); the second half starts with exactly that
        assert!((right.transform.x.value_at(0) - 60.0).abs() < 0.01);
        // the first half keeps its keys
        assert_eq!(out[0].transform.x.keys().len(), 2);
    }
}
