//! Editing the main track, as plain functions: old clip list in, new clip list out.
//!
//! The main track is *magnetic*: clips always sit end to end starting at 0, so after every change
//! the list is "packed". These functions never change the project themselves; commands
//! (see `command.rs`) wrap them so every edit can be undone.

use crate::ids::{ClipId, MediaId, TrackId};
use crate::time::{FrameRate, Micros};
use crate::timeline::Clip;

pub mod free;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("that clip no longer exists")]
    ClipNotFound(ClipId),
    #[error("a clip cannot be shorter than one frame")]
    TooShort,
    #[error("that point is outside the clip")]
    OutOfRange,
    #[error("the media file does not have that much video")]
    BeyondMedia,
    #[error("the track is locked")]
    Locked,
    #[error("that track no longer exists")]
    TrackNotFound(TrackId),
    #[error("that media file is not in the project")]
    MediaNotFound(MediaId),
    #[error("there is already a clip there")]
    Overlap,
    #[error("the main track cannot be removed")]
    MainTrack,
    #[error("that kind of clip does not go on that kind of track")]
    WrongTrack,
}

fn position(clips: &[Clip], id: ClipId) -> Result<usize, EditError> {
    clips
        .iter()
        .position(|c| c.id == id)
        .ok_or(EditError::ClipNotFound(id))
}

/// The shortest a clip may be: one frame.
fn min_duration(rate: FrameRate) -> Micros {
    rate.frame_start(1)
}

/// Make clips sit end to end from time 0 (the magnet).
pub fn pack(clips: &mut [Clip]) {
    let mut t = 0;
    for clip in clips {
        clip.start = t;
        t += clip.duration;
    }
}

fn packed(mut clips: Vec<Clip>) -> Vec<Clip> {
    pack(&mut clips);
    clips
}

/// Put `clip` at position `index` (counting clips, not time). An index past the end means "at the end".
pub fn insert(clips: &[Clip], index: usize, clip: Clip) -> Vec<Clip> {
    let mut out = clips.to_vec();
    out.insert(index.min(out.len()), clip);
    packed(out)
}

/// Which clip index a time falls on, for "insert at the playhead": before the first clip whose middle
/// is after `t`, or at the end.
pub fn index_for_time(clips: &[Clip], t: Micros) -> usize {
    clips
        .iter()
        .position(|c| t < c.start + c.duration / 2)
        .unwrap_or(clips.len())
}

/// Cut a clip in two at timeline time `at` (snapped to a frame). Both halves must be at least one frame long.
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
    let left = Clip {
        duration: at - clip.start,
        ..clip.clone()
    };
    let right = Clip {
        id: new_id,
        start: at,
        source_in: clip.source_in + (at - clip.start),
        duration: clip.end() - at,
        ..clip.clone()
    };
    let mut out = clips.to_vec();
    out[i] = left;
    out.insert(i + 1, right);
    Ok(out)
}

/// Remove clips and close the gaps (ripple delete). Unknown ids are an error.
pub fn delete(clips: &[Clip], ids: &[ClipId]) -> Result<Vec<Clip>, EditError> {
    for id in ids {
        position(clips, *id)?;
    }
    Ok(packed(
        clips
            .iter()
            .filter(|c| !ids.contains(&c.id))
            .cloned()
            .collect(),
    ))
}

/// Move the left edge of a clip by `delta` (positive = shorten from the left, negative = reveal more).
/// The out point stays where it is. Later clips shift to follow (the magnet).
pub fn trim_left(
    clips: &[Clip],
    id: ClipId,
    delta: Micros,
    rate: FrameRate,
) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let delta = rate.snap(delta.abs()) * delta.signum();
    let c = &clips[i];
    if c.source_in + delta < 0 {
        return Err(EditError::BeyondMedia);
    }
    if c.duration - delta < min_duration(rate) {
        return Err(EditError::TooShort);
    }
    let mut out = clips.to_vec();
    out[i].source_in += delta;
    out[i].duration -= delta;
    Ok(packed(out))
}

/// Set a clip's length (moves the right edge). `media_len` is how long the media file is, or `None` for
/// still images, which can be any length.
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
    if media_len.is_some_and(|len| clips[i].source_in + new_duration > len) {
        return Err(EditError::BeyondMedia);
    }
    let mut out = clips.to_vec();
    out[i].duration = new_duration;
    Ok(packed(out))
}

/// Move a clip so it ends up at list position `to` (counted after the clip has been taken out).
pub fn move_clip(clips: &[Clip], id: ClipId, to: usize) -> Result<Vec<Clip>, EditError> {
    let i = position(clips, id)?;
    let mut out = clips.to_vec();
    let clip = out.remove(i);
    out.insert(to.min(out.len()), clip);
    Ok(packed(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::time::US_PER_SEC;

    const RATE: FrameRate = FrameRate::FPS_25; // one frame = 40 000 us

    fn clip(seconds: i64) -> Clip {
        Clip::media_clip(MediaId::new(), 0, 0, seconds * US_PER_SEC)
    }

    fn track(lengths: &[i64]) -> Vec<Clip> {
        packed(lengths.iter().map(|s| clip(*s)).collect())
    }

    fn is_packed(clips: &[Clip]) -> bool {
        let mut t = 0;
        clips.iter().all(|c| {
            let ok = c.start == t;
            t += c.duration;
            ok
        })
    }

    #[test]
    fn pack_closes_gaps() {
        let mut clips = track(&[2, 3]);
        clips[1].start = 10 * US_PER_SEC;
        pack(&mut clips);
        assert_eq!(clips[1].start, 2 * US_PER_SEC);
    }

    #[test]
    fn insert_in_the_middle_shifts_the_rest() {
        let clips = track(&[2, 3]);
        let out = insert(&clips, 1, clip(4));
        assert_eq!(out.len(), 3);
        assert_eq!(out[1].start, 2 * US_PER_SEC);
        assert_eq!(out[2].start, 6 * US_PER_SEC);
        assert_eq!(out[2].id, clips[1].id);
        assert!(is_packed(&out));
    }

    #[test]
    fn insert_past_the_end_appends() {
        let out = insert(&track(&[2]), 99, clip(1));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].start, 2 * US_PER_SEC);
    }

    #[test]
    fn index_for_time_picks_the_nearest_cut() {
        let clips = track(&[2, 2]); // cuts at 0, 2 s, 4 s
        assert_eq!(index_for_time(&clips, 0), 0);
        assert_eq!(index_for_time(&clips, 900_000), 0);
        assert_eq!(index_for_time(&clips, 1_100_000), 1);
        assert_eq!(index_for_time(&clips, 3_500_000), 2);
        assert_eq!(index_for_time(&clips, 99 * US_PER_SEC), 2);
    }

    #[test]
    fn split_makes_two_clips_that_show_the_same_footage() {
        let mut clips = track(&[4]);
        clips[0].source_in = US_PER_SEC; // clip starts 1 s into the file
        let new_id = ClipId::new();
        let out = split(&clips, clips[0].id, 2 * US_PER_SEC, RATE, new_id).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].start, out[0].source_in, out[0].duration),
            (0, US_PER_SEC, 2 * US_PER_SEC)
        );
        assert_eq!(
            (out[1].start, out[1].source_in, out[1].duration),
            (2 * US_PER_SEC, 3 * US_PER_SEC, 2 * US_PER_SEC)
        );
        assert_eq!(out[1].id, new_id);
        // the picture at any time is unchanged by the cut
        assert_eq!(
            out[0].source_time(US_PER_SEC),
            clips[0].source_time(US_PER_SEC)
        );
        assert_eq!(
            out[1].source_time(3 * US_PER_SEC),
            clips[0].source_time(3 * US_PER_SEC)
        );
    }

    #[test]
    fn split_snaps_to_a_frame() {
        let clips = track(&[4]);
        let out = split(
            &clips,
            clips[0].id,
            2 * US_PER_SEC + 13_000,
            RATE,
            ClipId::new(),
        )
        .unwrap();
        assert_eq!(out[1].start, 2 * US_PER_SEC); // 13 ms is under half a frame
    }

    #[test]
    fn split_at_the_edges_is_refused() {
        let clips = track(&[4]);
        let id = clips[0].id;
        for at in [
            0,
            19_000,
            4 * US_PER_SEC - 19_000,
            4 * US_PER_SEC,
            9 * US_PER_SEC,
        ] {
            assert_eq!(
                split(&clips, id, at, RATE, ClipId::new()),
                Err(EditError::OutOfRange),
                "at {at}"
            );
        }
        assert!(split(&clips, id, 40_000, RATE, ClipId::new()).is_ok()); // exactly one frame in is fine
    }

    #[test]
    fn split_unknown_clip() {
        let id = ClipId::new();
        assert_eq!(
            split(&[], id, 0, RATE, ClipId::new()),
            Err(EditError::ClipNotFound(id))
        );
    }

    #[test]
    fn delete_ripples() {
        let clips = track(&[2, 3, 4]);
        let out = delete(&clips, &[clips[1].id]).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].start, 2 * US_PER_SEC);
        assert_eq!(out[1].id, clips[2].id);
    }

    #[test]
    fn delete_several_and_all() {
        let clips = track(&[2, 3, 4]);
        let ids: Vec<_> = clips.iter().map(|c| c.id).collect();
        assert_eq!(delete(&clips, &ids[..2]).unwrap().len(), 1);
        assert!(delete(&clips, &ids).unwrap().is_empty());
        assert!(delete(&clips, &[ClipId::new()]).is_err());
    }

    #[test]
    fn trim_left_shortens_and_moves_the_in_point() {
        let clips = track(&[4, 2]);
        let out = trim_left(&clips, clips[0].id, US_PER_SEC, RATE).unwrap();
        assert_eq!(
            (out[0].source_in, out[0].duration),
            (US_PER_SEC, 3 * US_PER_SEC)
        );
        assert_eq!(out[1].start, 3 * US_PER_SEC); // the next clip followed
        // out point unchanged
        assert_eq!(
            out[0].source_in + out[0].duration,
            clips[0].source_in + clips[0].duration
        );
    }

    #[test]
    fn trim_left_can_reveal_footage_but_not_before_the_start_of_the_file() {
        let mut clips = track(&[4]);
        clips[0].source_in = 2 * US_PER_SEC;
        let id = clips[0].id;
        let out = trim_left(&clips, id, -US_PER_SEC, RATE).unwrap();
        assert_eq!(
            (out[0].source_in, out[0].duration),
            (US_PER_SEC, 5 * US_PER_SEC)
        );
        assert_eq!(
            trim_left(&clips, id, -3 * US_PER_SEC, RATE),
            Err(EditError::BeyondMedia)
        );
    }

    #[test]
    fn trim_left_cannot_remove_the_whole_clip() {
        let clips = track(&[1]);
        assert_eq!(
            trim_left(&clips, clips[0].id, US_PER_SEC, RATE),
            Err(EditError::TooShort)
        );
        assert!(trim_left(&clips, clips[0].id, US_PER_SEC - 40_000, RATE).is_ok());
    }

    #[test]
    fn trim_right_respects_the_media_length() {
        let clips = track(&[2, 2]);
        let id = clips[0].id;
        let shorter = trim_right(&clips, id, US_PER_SEC, Some(5 * US_PER_SEC), RATE).unwrap();
        assert_eq!(shorter[1].start, US_PER_SEC);
        let longer = trim_right(&clips, id, 5 * US_PER_SEC, Some(5 * US_PER_SEC), RATE).unwrap();
        assert_eq!(longer[1].start, 5 * US_PER_SEC);
        assert_eq!(
            trim_right(&clips, id, 6 * US_PER_SEC, Some(5 * US_PER_SEC), RATE),
            Err(EditError::BeyondMedia)
        );
        // images have no length limit
        assert!(trim_right(&clips, id, 600 * US_PER_SEC, None, RATE).is_ok());
        assert_eq!(
            trim_right(&clips, id, 10_000, None, RATE),
            Err(EditError::TooShort)
        );
    }

    #[test]
    fn move_reorders_and_repacks() {
        let clips = track(&[1, 2, 3]);
        let out = move_clip(&clips, clips[0].id, 2).unwrap();
        let order: Vec<_> = out.iter().map(|c| c.id).collect();
        assert_eq!(order, vec![clips[1].id, clips[2].id, clips[0].id]);
        assert!(is_packed(&out));
        assert_eq!(out[2].start, 5 * US_PER_SEC);
    }

    #[test]
    fn every_operation_leaves_the_track_packed() {
        let mut clips = track(&[3, 3, 3, 3]);
        let mut seed = 12345u64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..300 {
            let id = clips[next() % clips.len()].id;
            let result = match next() % 5 {
                0 => split(
                    &clips,
                    id,
                    (next() % 3_000) as i64 * 1000
                        + clips.iter().find(|c| c.id == id).unwrap().start,
                    RATE,
                    ClipId::new(),
                ),
                1 => trim_left(&clips, id, (next() % 1_000_000) as i64, RATE),
                2 => trim_right(&clips, id, (next() % 3_000_000) as i64 + 40_000, None, RATE),
                3 => move_clip(&clips, id, next() % 6),
                _ => insert(&clips, next() % 6, clip(1))
                    .into_iter()
                    .map(Ok)
                    .collect(),
            };
            if let Ok(new) = result {
                assert!(is_packed(&new));
                assert!(new.iter().all(|c| c.duration >= 40_000));
                clips = new;
            }
            if clips.len() > 12 {
                clips = delete(&clips, &[clips[0].id, clips[1].id]).unwrap();
            }
        }
    }
}
