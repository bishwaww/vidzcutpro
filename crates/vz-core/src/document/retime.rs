//! Changing how fast clips play: speed, speed curves, reverse and freeze frames.

use super::{DocError, Document, EditError, MergeKey, edit};
use crate::ids::ClipId;
use crate::time::{FrameRate, Micros, US_PER_SEC};
use crate::timeline::{Clip, TrackKind};
use vz_anim::{Animatable, Easing, Keyframe};

/// How long a freeze frame lasts when it is first made.
pub const FREEZE_LENGTH: Micros = 3 * US_PER_SEC;

/// The average of a speed curve given as points `(position 0..=1 along the clip, speed)`.
pub fn mean_speed(points: &[(f32, f32)]) -> f64 {
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            f64::from(b.0 - a.0) * f64::from(a.1 + b.1) / 2.0
        })
        .sum()
}

impl Document {
    /// Put a changed copy of a clip back in its track (the track keeps its rules: the main track closes gaps, other
    /// tracks must not overlap).
    fn replace_clip(
        &mut self,
        id: ClipId,
        label: &'static str,
        merge: Option<MergeKey>,
        new: Clip,
    ) -> Result<(), DocError> {
        let (track, kind) = self.kind_of(id)?;
        self.edit_track(track, label, merge, |t| {
            let i = t
                .clips
                .iter()
                .position(|c| c.id == id)
                .ok_or(EditError::ClipNotFound(id))?;
            let mut clips = t.clips.clone();
            if !kind.is_magnetic() && edit::free::overlaps(&clips, new.start, new.end(), Some(id)) {
                return Err(EditError::Overlap);
            }
            clips[i] = new;
            if kind.is_magnetic() {
                edit::pack(&mut clips);
            }
            Ok(clips)
        })
    }

    fn snapped_length(rate: FrameRate, d: f64) -> Micros {
        rate.snap(d.round() as Micros).max(rate.frame_start(1))
    }

    /// Play a clip at a constant speed (1.0 normal, 2.0 twice as fast). It keeps showing the same part of its file, so
    /// it gets shorter or longer. A speed curve is replaced.
    pub fn set_speed(
        &mut self,
        id: ClipId,
        speed: f32,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let old = self
            .project
            .timeline()
            .clip(id)
            .ok_or(EditError::ClipNotFound(id))?
            .clone();
        let speed = speed.clamp(0.1, 100.0);
        let span = old.source_span();
        let mut new = Clip {
            speed: Animatable::Static(speed),
            ..old.clone()
        };
        if span > 0 {
            new.duration = Self::snapped_length(rate, span as f64 / f64::from(speed));
        }
        if new == old {
            return Ok(());
        }
        self.replace_clip(id, "Speed", merge, new)
    }

    /// Use a speed curve given as points `(position 0..=1 along the clip, speed)`; the first point is at 0 and the last
    /// at 1. The clip keeps showing the same part of its file, so its length follows from the average speed.
    pub fn set_speed_curve(&mut self, id: ClipId, points: &[(f32, f32)]) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let old = self
            .project
            .timeline()
            .clip(id)
            .ok_or(EditError::ClipNotFound(id))?
            .clone();
        let mean = mean_speed(points);
        if points.len() < 2 || mean <= 0.0 {
            return Err(EditError::OutOfRange.into());
        }
        let span = old.source_span().max(old.duration);
        let duration = Self::snapped_length(rate, span as f64 / mean);
        let keys = points
            .iter()
            .map(|(u, s)| Keyframe {
                time: (f64::from(*u) * duration as f64).round() as Micros,
                value: s.clamp(0.0, 100.0),
                easing: Easing::Linear,
            })
            .collect();
        let new = Clip {
            speed: Animatable::Keyframed(keys),
            duration,
            ..old
        };
        self.replace_clip(id, "Speed curve", None, new)
    }

    /// Play a clip backwards (or forwards again). The same part of the file is shown.
    pub fn set_reverse(&mut self, id: ClipId, reverse: bool) -> Result<(), DocError> {
        self.update_clip(id, "Reverse", None, |c| c.reverse = reverse)
    }

    /// Keep the pitch of the sound when the speed changes (or let it change with the speed).
    pub fn set_keep_pitch(&mut self, id: ClipId, keep: bool) -> Result<(), DocError> {
        self.update_clip(id, "Keep pitch", None, |c| c.keep_pitch = keep)
    }

    /// Insert a still picture of the clip at time `at` (a freeze frame) lasting `length`. The clip is cut there; what
    /// comes after moves later. One undo step. Returns the freeze clip.
    pub fn freeze_frame(
        &mut self,
        id: ClipId,
        at: Micros,
        length: Micros,
    ) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let (track, kind) = self.kind_of(id)?;
        if kind == TrackKind::Audio {
            return Err(EditError::WrongTrack.into());
        }
        let original = self
            .project
            .timeline()
            .clip(id)
            .ok_or(EditError::ClipNotFound(id))?
            .clone();
        if original.media().is_none() {
            return Err(EditError::WrongTrack.into());
        }
        let at = rate.snap(at);
        let length = rate.snap(length).max(rate.frame_start(1));
        let mut still = Clip {
            id: ClipId::new(),
            start: at,
            duration: length,
            source_in: original.source_time(at),
            speed: Animatable::Static(0.0),
            reverse: false,
            anim_in: None,
            anim_out: None,
            anim_combo: None,
            fade_in: 0,
            fade_out: 0,
            volume: Animatable::Static(0.0),
            ..original.clone()
        };
        still.link = None;
        let still_id = still.id;
        self.edit_track(track, "Freeze frame", None, |t| {
            let right_id = ClipId::new();
            let mut clips = if kind.is_magnetic() {
                edit::split(&t.clips, id, at, rate, right_id)?
            } else {
                edit::free::split(&t.clips, id, at, rate, right_id)?
            };
            let i = clips
                .iter()
                .position(|c| c.id == id)
                .ok_or(EditError::ClipNotFound(id))?;
            if kind.is_magnetic() {
                clips.insert(i + 1, still);
                edit::pack(&mut clips);
            } else {
                // everything from the cut on moves later to make room
                for c in clips.iter_mut().filter(|c| c.start >= at) {
                    c.start += length;
                }
                clips.insert(i + 1, still);
                clips.sort_by_key(|c| c.start);
            }
            Ok(clips)
        })?;
        Ok(still_id)
    }
}
