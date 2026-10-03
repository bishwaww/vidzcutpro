//! Adding, changing and removing transitions between clips.

use super::{Command, DocError, Document, EditError, MergeKey};
use crate::ids::ClipId;
use crate::timeline::{TrackKind, Transition};
use vz_anim::Easing;

impl Document {
    /// Put a transition at the end of `after` (it joins that clip to the next one), replacing the one that is there,
    /// or take it away with `None`. The clip must have another clip right after it on its track.
    pub fn set_transition(
        &mut self,
        after: ClipId,
        transition: Option<Transition>,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let track = self
            .project
            .timeline()
            .track_of(after)
            .ok_or(EditError::ClipNotFound(after))?;
        if transition.is_some() && !matches!(track.kind, TrackKind::Main | TrackKind::Overlay) {
            return Err(EditError::WrongTrack.into());
        }
        let before = track.transitions.clone();
        let mut next = before.clone();
        next.retain(|t| t.after != after);
        if let Some(t) = transition {
            let i = track
                .clips
                .iter()
                .position(|c| c.id == after)
                .ok_or(EditError::ClipNotFound(after))?;
            match track.clips.get(i + 1) {
                Some(b) if b.start == track.clips[i].end() => {}
                _ => return Err(EditError::OutOfRange.into()),
            }
            next.push(Transition { after, ..t });
        }
        if next == before {
            return Ok(());
        }
        self.run(
            Command::SetTransitions {
                track: track.id,
                before,
                after: next,
            },
            merge,
        )
    }

    /// Put the same transition at every cut of the main track (replacing the ones there). One undo step.
    pub fn apply_transition_to_all(
        &mut self,
        preset: &str,
        duration: i64,
        easing: Easing,
    ) -> Result<usize, DocError> {
        let track = self.project.timeline().main_track();
        let before = track.transitions.clone();
        let next: Vec<Transition> = track
            .clips
            .windows(2)
            .filter(|w| w[1].start == w[0].end())
            .map(|w| Transition {
                after: w[0].id,
                preset: preset.to_owned(),
                duration,
                easing,
            })
            .collect();
        let count = next.len();
        if next == before {
            return Ok(count);
        }
        let id = track.id;
        self.run(
            Command::SetTransitions {
                track: id,
                before,
                after: next,
            },
            None,
        )?;
        Ok(count)
    }
}
