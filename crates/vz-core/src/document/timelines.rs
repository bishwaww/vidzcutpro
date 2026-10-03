//! Several timelines in one project, and compound clips (a group of clips folded into one clip that plays another timeline).

use super::{Command, DocError, Document};
use crate::edit::{self, EditError};
use crate::ids::{ClipId, TimelineId, TrackId};
use crate::time::Micros;
use crate::timeline::{Clip, Timeline, Track, TrackKind};
use std::collections::HashMap;

impl Timeline {
    /// A copy with a new name and new ids for the timeline, its tracks and its clips.
    pub fn copied_as(&self, name: impl Into<String>) -> Self {
        let mut copy = self.clone();
        copy.id = TimelineId::new();
        copy.name = name.into();
        for track in &mut copy.tracks {
            track.id = TrackId::new();
            let mut ids: HashMap<ClipId, ClipId> = HashMap::new();
            for clip in &mut track.clips {
                let new = ClipId::new();
                ids.insert(clip.id, new);
                clip.id = new;
            }
            for tr in &mut track.transitions {
                if let Some(id) = ids.get(&tr.after) {
                    tr.after = *id;
                }
            }
        }
        copy
    }
}

impl Document {
    /// A name for a new timeline that no other timeline has: "Timeline 02", "Compound 01", ...
    fn free_timeline_name(&self, prefix: &str) -> String {
        (1..)
            .map(|n| format!("{prefix} {n:02}"))
            .find(|name| !self.project.timelines.iter().any(|t| &t.name == name))
            .unwrap_or_else(|| prefix.to_owned())
    }

    /// Add an empty timeline. It is not opened. One undo step.
    pub fn add_timeline(&mut self) -> Result<TimelineId, DocError> {
        let timeline = Timeline::new(self.free_timeline_name("Timeline"));
        let id = timeline.id;
        self.run(Command::AddTimeline(Box::new(timeline)), None)?;
        Ok(id)
    }

    /// Add a copy of a timeline (new ids everywhere). It is not opened. One undo step.
    pub fn duplicate_timeline(&mut self, id: TimelineId) -> Result<TimelineId, DocError> {
        let source = self
            .project
            .timeline_by_id(id)
            .ok_or(EditError::TimelineNotFound(id))?;
        let name = self.free_timeline_name(source.name.trim_end_matches(char::is_numeric).trim());
        let copy = source.copied_as(name);
        let new = copy.id;
        self.run(Command::AddTimeline(Box::new(copy)), None)?;
        Ok(new)
    }

    pub fn rename_timeline(&mut self, id: TimelineId, name: &str) -> Result<(), DocError> {
        let before = self
            .project
            .timeline_by_id(id)
            .ok_or(EditError::TimelineNotFound(id))?
            .name
            .clone();
        let name = name.trim();
        if name.is_empty() || name == before {
            return Ok(());
        }
        self.run(
            Command::RenameTimeline {
                id,
                before,
                after: name.to_owned(),
            },
            None,
        )
    }

    /// Delete a timeline. Not the last one, and not one a compound clip still plays. One undo step.
    pub fn delete_timeline(&mut self, id: TimelineId) -> Result<(), DocError> {
        let index = self
            .project
            .timelines
            .iter()
            .position(|t| t.id == id)
            .ok_or(EditError::TimelineNotFound(id))?;
        if self.project.timelines.len() < 2 {
            return Err(EditError::LastTimeline.into());
        }
        if !self.project.compound_users(id).is_empty() {
            return Err(EditError::TimelineInUse(id).into());
        }
        let timeline = Box::new(self.project.timelines[index].clone());
        self.run(Command::RemoveTimeline { timeline, index }, None)
    }

    /// Edit another timeline from now on (not an undo step). Returns false when there is no such timeline.
    pub fn set_active_timeline(&mut self, id: TimelineId) -> bool {
        self.seal_history();
        self.project.set_active(id)
    }

    /// Fold the clips into one compound clip: they move into a new timeline, and one clip that plays it takes their
    /// place. Clips on the main track must touch each other (the compound then sits on the main track, so nothing
    /// shifts); otherwise the compound goes on an overlay track. One undo step. Returns the compound clip.
    pub fn make_compound(&mut self, ids: &[ClipId]) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let tl = self.project.timeline();
        // the chosen clips, track by track
        let mut picked: Vec<(usize, Vec<Clip>)> = Vec::new();
        for (i, track) in tl.tracks.iter().enumerate() {
            let chosen: Vec<Clip> = track
                .clips
                .iter()
                .filter(|c| ids.contains(&c.id))
                .cloned()
                .collect();
            if chosen.is_empty() {
                continue;
            }
            if track.locked {
                return Err(EditError::Locked.into());
            }
            picked.push((i, chosen));
        }
        if picked.is_empty() {
            return Err(EditError::NothingSelected.into());
        }
        let main_index = tl.tracks.iter().position(|t| t.kind == TrackKind::Main);
        let main_chosen = picked.iter().find(|(i, _)| Some(*i) == main_index);
        let all_start = picked
            .iter()
            .flat_map(|(_, c)| c)
            .map(|c| c.start)
            .min()
            .unwrap_or(0);
        let all_end = picked
            .iter()
            .flat_map(|(_, c)| c)
            .map(Clip::end)
            .max()
            .unwrap_or(0);
        let (span_start, first_main) = match (main_index, main_chosen) {
            (Some(m), Some((_, chosen))) => {
                let clips = &tl.tracks[m].clips;
                let at: Vec<usize> = chosen
                    .iter()
                    .filter_map(|c| clips.iter().position(|k| k.id == c.id))
                    .collect();
                if at.windows(2).any(|w| w[1] != w[0] + 1) {
                    return Err(EditError::NotTogether.into());
                }
                let start = chosen[0].start;
                if all_start < start {
                    return Err(EditError::NotTogether.into());
                }
                (start, Some(at[0]))
            }
            _ => (all_start, None),
        };
        let span = rate.snap(all_end - span_start).max(rate.frame_start(1));

        // the inside: a new timeline with the clips moved to start at zero
        let mut inside = Timeline::new(self.free_timeline_name("Compound"));
        for (i, chosen) in &picked {
            let source = &tl.tracks[*i];
            let mut moved = chosen.clone();
            for c in &mut moved {
                c.start -= span_start;
            }
            if source.kind == TrackKind::Main {
                inside.tracks[0].clips = moved;
            } else {
                let mut track = Track::new(source.kind, source.name.clone());
                track.clips = moved;
                inside.tracks.push(track);
            }
        }
        let compound = Clip::compound_clip(inside.id, span_start, span);
        let compound_id = compound.id;

        // the outside: the chosen clips leave their tracks, the compound takes their place
        let mut work: Vec<(TrackId, Vec<Clip>, Vec<Clip>)> = tl
            .tracks
            .iter()
            .map(|t| (t.id, t.clips.clone(), t.clips.clone()))
            .collect();
        for (i, _) in &picked {
            work[*i].2.retain(|c| !ids.contains(&c.id));
        }
        let mut new_track: Option<Track> = None;
        if let (Some(m), Some(at)) = (main_index, first_main) {
            work[m].2 = edit::insert(&work[m].2, at, compound);
        } else {
            let room = tl.tracks.iter().enumerate().find(|(i, t)| {
                t.kind == TrackKind::Overlay
                    && !t.locked
                    && !edit::free::overlaps(&work[*i].2, span_start, span_start + span, None)
            });
            match room {
                Some((i, _)) => work[i].2.push(compound),
                None => {
                    let mut track =
                        Track::new(TrackKind::Overlay, tl.next_track_name(TrackKind::Overlay));
                    track.clips = vec![compound];
                    new_track = Some(track);
                }
            }
        }

        let mut commands = vec![Command::AddTimeline(Box::new(inside))];
        for (id, before, after) in work {
            if before != after {
                commands.push(Command::EditTrack {
                    track: id,
                    before,
                    after,
                    label: "Make compound clip",
                });
            }
        }
        if let Some(track) = new_track {
            let (id, after) = (track.id, track.clips.clone());
            let mut empty = track;
            empty.clips = Vec::new();
            commands.push(Command::AddTrack {
                track: Box::new(empty),
                index: self.project.timeline().tracks.len(),
            });
            commands.push(Command::EditTrack {
                track: id,
                before: Vec::new(),
                after,
                label: "Make compound clip",
            });
        }
        self.run(
            Command::Batch {
                label: "Make compound clip",
                commands,
            },
            None,
        )?;
        Ok(compound_id)
    }

    /// The timeline a compound clip plays.
    pub fn compound_timeline(&self, clip: ClipId) -> Option<TimelineId> {
        match self.project.timeline().clip(clip)?.content {
            crate::content::Content::Compound(id) => Some(id),
            _ => None,
        }
    }

    /// Where a compound clip's inside starts at a given time of the outside timeline.
    pub fn compound_time(&self, clip: ClipId, t: Micros) -> Micros {
        self.project
            .timeline()
            .clip(clip)
            .map_or(0, |c| c.source_time(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Content, ShapeContent, ShapeKind, TextContent};
    use crate::media::{MediaAsset, MediaKind};
    use crate::project::Project;
    use crate::time::{FrameRate, US_PER_SEC};

    fn video(secs: i64) -> MediaAsset {
        MediaAsset {
            id: crate::ids::MediaId::new(),
            name: "v".into(),
            path: "/v.mp4".into(),
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

    fn doc_with_two_clips_and_a_title() -> (Document, Vec<ClipId>, ClipId) {
        let mut doc = Document::new(Project::new("t"));
        let m = doc.import_media(video(10)).unwrap();
        let a = doc.add_clip(m, 0).unwrap();
        let b = doc.add_clip(m, 1).unwrap();
        let (_, title) = doc
            .add_text_auto(2 * US_PER_SEC, 3 * US_PER_SEC, TextContent::new("hi"))
            .unwrap();
        (doc, vec![a, b], title)
    }

    #[test]
    fn a_compound_of_main_clips_takes_their_place_and_keeps_the_length() {
        let (mut doc, mains, title) = doc_with_two_clips_and_a_title();
        let before = doc.project.duration();
        let id = doc.make_compound(&[mains[0], mains[1], title]).unwrap();
        let tl = doc.project.timeline();
        assert_eq!(doc.project.duration(), before);
        let main = tl.main_track();
        assert_eq!(main.clips.len(), 1);
        assert_eq!(main.clips[0].id, id);
        assert!(main.clips[0].is_compound());
        // the title moved inside
        assert!(tl.clip(title).is_none());
        let inner = doc
            .project
            .timeline_by_id(doc.compound_timeline(id).unwrap())
            .unwrap();
        assert_eq!(inner.main_track().clips.len(), 2);
        assert!(inner.clip(title).is_some());
        assert_eq!(inner.main_track().clips[0].start, 0);
        assert_eq!(inner.clip(title).unwrap().start, 2 * US_PER_SEC);
    }

    #[test]
    fn one_undo_brings_everything_back_and_redo_makes_it_again() {
        let (mut doc, mains, title) = doc_with_two_clips_and_a_title();
        let timelines = doc.project.timelines.len();
        let id = doc.make_compound(&[mains[0], mains[1], title]).unwrap();
        assert_eq!(doc.project.timelines.len(), timelines + 1);
        doc.undo();
        assert_eq!(doc.project.timelines.len(), timelines);
        assert!(doc.project.timeline().clip(mains[0]).is_some());
        assert!(doc.project.timeline().clip(title).is_some());
        assert!(doc.project.timeline().clip(id).is_none());
        doc.redo();
        assert!(doc.project.timeline().clip(id).is_some());
        assert_eq!(doc.project.timelines.len(), timelines + 1);
    }

    #[test]
    fn main_clips_that_do_not_touch_cannot_be_folded_together() {
        let mut doc = Document::new(Project::new("t"));
        let m = doc.import_media(video(10)).unwrap();
        let a = doc.add_clip(m, 0).unwrap();
        let _b = doc.add_clip(m, 1).unwrap();
        let c = doc.add_clip(m, 2).unwrap();
        let err = doc.make_compound(&[a, c]).unwrap_err();
        assert!(
            matches!(err, DocError::Edit(EditError::NotTogether)),
            "{err}"
        );
        assert!(matches!(
            doc.make_compound(&[]).unwrap_err(),
            DocError::Edit(EditError::NothingSelected)
        ));
    }

    #[test]
    fn overlay_clips_become_an_overlay_compound_where_they_were() {
        let mut doc = Document::new(Project::new("t"));
        let (_, a) = doc
            .add_shape_auto(
                US_PER_SEC,
                2 * US_PER_SEC,
                ShapeContent::new(ShapeKind::Star),
            )
            .unwrap();
        let (_, b) = doc
            .add_shape_auto(
                2 * US_PER_SEC,
                2 * US_PER_SEC,
                ShapeContent::new(ShapeKind::Heart),
            )
            .unwrap();
        let id = doc.make_compound(&[a, b]).unwrap();
        let tl = doc.project.timeline();
        let c = tl.clip(id).unwrap();
        assert_eq!((c.start, c.duration), (US_PER_SEC, 3 * US_PER_SEC));
        assert!(tl.clip(a).is_none() && tl.clip(b).is_none());
        let inner = doc
            .project
            .timeline_by_id(doc.compound_timeline(id).unwrap())
            .unwrap();
        assert_eq!(inner.clip(a).unwrap().start, 0);
        assert_eq!(inner.clip(b).unwrap().start, US_PER_SEC);
        assert!(matches!(c.content, Content::Compound(_)));
    }

    #[test]
    fn timelines_can_be_added_renamed_duplicated_and_deleted_with_undo() {
        let mut doc = Document::new(Project::new("t"));
        let first = doc.project.timeline().id;
        let second = doc.add_timeline().unwrap();
        assert_eq!(doc.project.timelines.len(), 2);
        assert_eq!(
            doc.project.timeline_by_id(second).unwrap().name,
            "Timeline 02"
        );
        doc.rename_timeline(second, "Intro").unwrap();
        assert_eq!(doc.project.timeline_by_id(second).unwrap().name, "Intro");
        let copy = doc.duplicate_timeline(second).unwrap();
        assert_ne!(copy, second);
        assert_eq!(doc.project.timelines.len(), 3);
        doc.delete_timeline(copy).unwrap();
        assert_eq!(doc.project.timelines.len(), 2);
        doc.undo();
        assert_eq!(doc.project.timelines.len(), 3);
        // switching is not an edit
        assert!(doc.set_active_timeline(second));
        assert_eq!(doc.project.timeline().id, second);
        assert!(doc.set_active_timeline(first));
        assert!(!doc.set_active_timeline(TimelineId::new()));
    }

    #[test]
    fn the_last_timeline_and_a_timeline_in_use_cannot_be_deleted() {
        let (mut doc, mains, title) = doc_with_two_clips_and_a_title();
        let outer = doc.project.timeline().id;
        let id = doc.make_compound(&[mains[0], mains[1], title]).unwrap();
        let inner = doc.compound_timeline(id).unwrap();
        assert!(matches!(
            doc.delete_timeline(inner).unwrap_err(),
            DocError::Edit(EditError::TimelineInUse(_))
        ));
        assert!(doc.project.contains_timeline(outer, inner));
        assert!(!doc.project.contains_timeline(inner, outer));
        let mut single = Document::new(Project::new("s"));
        let only = single.project.timeline().id;
        assert!(matches!(
            single.delete_timeline(only).unwrap_err(),
            DocError::Edit(EditError::LastTimeline)
        ));
    }

    #[test]
    fn undo_goes_back_to_the_timeline_the_change_was_made_in() {
        let mut doc = Document::new(Project::new("t"));
        let first = doc.project.timeline().id;
        let second = doc.add_timeline().unwrap();
        // a title in the first timeline, then look at the second
        let (_, title) = doc
            .add_text_auto(0, US_PER_SEC, TextContent::new("one"))
            .unwrap();
        doc.set_active_timeline(second);
        doc.undo();
        assert_eq!(
            doc.project.timeline().id,
            first,
            "undo opens the timeline it changed"
        );
        assert!(doc.project.timeline().clip(title).is_none());
        doc.redo();
        assert!(doc.project.timeline().clip(title).is_some());
    }

    #[test]
    fn a_copied_timeline_has_new_ids_but_the_same_content() {
        let (mut doc, ..) = doc_with_two_clips_and_a_title();
        let source = doc.project.timeline().id;
        let copy = doc.duplicate_timeline(source).unwrap();
        let (a, b) = (
            doc.project.timeline_by_id(source).unwrap(),
            doc.project.timeline_by_id(copy).unwrap(),
        );
        assert_eq!(a.tracks.len(), b.tracks.len());
        for (x, y) in a.tracks.iter().zip(&b.tracks) {
            assert_ne!(x.id, y.id);
            assert_eq!(x.clips.len(), y.clips.len());
            assert!(
                x.clips
                    .iter()
                    .zip(&y.clips)
                    .all(|(p, q)| p.id != q.id && p.start == q.start)
            );
        }
    }
}
