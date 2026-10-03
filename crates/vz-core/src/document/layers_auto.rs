//! Putting adjustment layers and effects on the timeline.

use super::{Command, DocError, Document, edit};
use crate::grade::{Adjust, EffectRef};
use crate::ids::{ClipId, TrackId};
use crate::time::Micros;
use crate::timeline::{Clip, Track, TrackKind};

impl Document {
    /// Add an adjustment layer (colour settings and effects for everything below) at `start` on the first effect track
    /// with room there, or on a new effect track. One undo step.
    pub fn add_adjustment_auto(
        &mut self,
        start: Micros,
        duration: Micros,
        adjust: Adjust,
        effects: Vec<EffectRef>,
    ) -> Result<(TrackId, ClipId), DocError> {
        let rate = self.project.frame_rate;
        let start = rate.snap(start.max(0));
        let duration = rate.snap(duration).max(rate.frame_start(1));
        let mut clip = Clip::adjustment_clip(start, duration);
        clip.adjust = adjust;
        clip.effects = effects;
        let tl = self.project.timeline();
        let room = tl.tracks.iter().find(|t| {
            t.kind == TrackKind::Effect
                && !t.locked
                && !edit::free::overlaps(&t.clips, start, start + duration, None)
        });
        if let Some(track) = room.map(|t| t.id) {
            let id = self.place_free(track, clip, start)?;
            return Ok((track, id));
        }
        let track = Track::new(TrackKind::Effect, tl.next_track_name(TrackKind::Effect));
        let (track_id, clip_id) = (track.id, clip.id);
        let commands = vec![
            Command::AddTrack {
                track: Box::new(track),
                index: tl.tracks.len(),
            },
            Command::EditTrack {
                track: track_id,
                before: Vec::new(),
                after: vec![clip],
                label: "Add effect",
            },
        ];
        self.run(
            Command::Batch {
                label: "Add effect",
                commands,
            },
            None,
        )?;
        Ok((track_id, clip_id))
    }
}
