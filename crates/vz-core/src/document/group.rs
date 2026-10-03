//! Adding several clips as one group (a text template: a box, a title, a line, each with its own animation).

use super::{Command, DocError, Document};
use crate::ids::{ClipId, GroupId};
use crate::time::Micros;
use crate::timeline::{Clip, Track, TrackKind};

impl Document {
    /// Add `parts` (bottom layer first; each clip's `start` is relative to `at`) as one group. Every part gets a
    /// track of its own, added above the existing ones in that order, so the first part is drawn below the last.
    /// The clips get new ids and share one group id. One undo step. Returns the new clips in the same order.
    pub fn add_group(
        &mut self,
        parts: Vec<(TrackKind, Clip)>,
        at: Micros,
        label: &'static str,
    ) -> Result<Vec<ClipId>, DocError> {
        if parts.is_empty() {
            return Ok(Vec::new());
        }
        let rate = self.project.frame_rate;
        let at = rate.snap(at.max(0));
        let group = GroupId::new();
        let tl = self.project.timeline();
        let mut counts = [
            TrackKind::Main,
            TrackKind::Overlay,
            TrackKind::Text,
            TrackKind::Audio,
            TrackKind::Effect,
        ]
        .map(|k| (k, tl.tracks.iter().filter(|t| t.kind == k).count()));
        let mut commands = Vec::new();
        let mut ids = Vec::new();
        for (offset, (kind, mut clip)) in parts.into_iter().enumerate() {
            clip.id = ClipId::new();
            clip.group = Some(group);
            clip.link = None;
            clip.start = rate.snap(at + clip.start.max(0));
            clip.duration = rate.snap(clip.duration).max(rate.frame_start(1));
            ids.push(clip.id);
            let n = counts.iter_mut().find(|(k, _)| *k == kind).map(|(_, n)| {
                *n += 1;
                *n
            });
            let track = Track::new(kind, format!("{} {}", kind.label(), n.unwrap_or(1)));
            let track_id = track.id;
            commands.push(Command::AddTrack {
                track: Box::new(track),
                index: tl.tracks.len() + offset,
            });
            commands.push(Command::EditTrack {
                track: track_id,
                before: Vec::new(),
                after: vec![clip],
                label,
            });
        }
        self.run(Command::Batch { label, commands }, None)?;
        Ok(ids)
    }
}

impl Document {
    /// Every clip that is grouped with `id` (including `id` itself); just `id` for a clip that is not grouped.
    pub fn group_mates(&self, id: ClipId) -> Vec<ClipId> {
        let tl = self.project.timeline();
        match tl.clip(id).and_then(|c| c.group) {
            Some(g) => tl
                .tracks
                .iter()
                .flat_map(|t| t.clips.iter())
                .filter(|c| c.group == Some(g))
                .map(|c| c.id)
                .collect(),
            None => vec![id],
        }
    }

    /// Group the clips so they are selected and moved together. One undo step.
    pub fn group_clips(&mut self, ids: &[ClipId]) -> Result<(), DocError> {
        if ids.len() < 2 {
            return Ok(());
        }
        let group = GroupId::new();
        self.update_clips(ids, "Group", |c| c.group = Some(group))
    }

    /// Take the clips (and everything grouped with them) out of their groups. One undo step.
    pub fn ungroup_clips(&mut self, ids: &[ClipId]) -> Result<(), DocError> {
        let mut all: Vec<ClipId> = ids.iter().flat_map(|id| self.group_mates(*id)).collect();
        all.sort();
        all.dedup();
        self.update_clips(&all, "Ungroup", |c| c.group = None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ShapeContent, ShapeKind, TextContent};
    use crate::project::Project;
    use crate::time::US_PER_SEC;

    fn parts() -> Vec<(TrackKind, Clip)> {
        let mut bar = Clip::shape_clip(ShapeContent::new(ShapeKind::Rectangle), 0, 3 * US_PER_SEC);
        bar.start = 0;
        let mut title = Clip::text_clip(TextContent::new("Hello"), 0, 2 * US_PER_SEC);
        title.start = US_PER_SEC / 2;
        vec![(TrackKind::Overlay, bar), (TrackKind::Text, title)]
    }

    #[test]
    fn a_group_goes_on_new_tracks_in_order_and_shares_a_group_id() {
        let mut doc = Document::new(Project::new("t"));
        let before = doc.project.timeline().tracks.len();
        let ids = doc
            .add_group(parts(), 2 * US_PER_SEC, "Add template")
            .unwrap();
        let tl = doc.project.timeline();
        assert_eq!(tl.tracks.len(), before + 2);
        let (a, b) = (tl.clip(ids[0]).unwrap(), tl.clip(ids[1]).unwrap());
        assert_eq!(a.group, b.group);
        assert!(a.group.is_some());
        assert_eq!(
            (a.start, b.start),
            (2 * US_PER_SEC, 2 * US_PER_SEC + US_PER_SEC / 2)
        );
        // the bar's track comes before the text's, so the bar is drawn below the text
        let pos = |id| {
            tl.tracks
                .iter()
                .position(|t| t.clips.iter().any(|c| c.id == id))
                .unwrap()
        };
        assert!(pos(ids[0]) < pos(ids[1]));
    }

    #[test]
    fn adding_a_group_twice_never_overlaps_and_one_undo_removes_it() {
        let mut doc = Document::new(Project::new("t"));
        let first = doc.add_group(parts(), 0, "Add template").unwrap();
        let second = doc.add_group(parts(), 0, "Add template").unwrap();
        let tl = doc.project.timeline();
        assert_ne!(
            tl.clip(first[0]).unwrap().group,
            tl.clip(second[0]).unwrap().group
        );
        assert_eq!(
            tl.tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Text)
                .count(),
            2
        );
        doc.undo();
        assert!(doc.project.timeline().clip(second[0]).is_none());
        assert!(doc.project.timeline().clip(first[0]).is_some());
    }

    #[test]
    fn grouping_and_ungrouping_are_undoable_and_reach_every_mate() {
        let mut doc = Document::new(Project::new("t"));
        let ids = doc.add_group(parts(), 0, "Add template").unwrap();
        assert_eq!(doc.group_mates(ids[0]).len(), 2);
        doc.ungroup_clips(&ids[..1]).unwrap();
        assert_eq!(doc.group_mates(ids[0]), vec![ids[0]]);
        assert_eq!(
            doc.group_mates(ids[1]),
            vec![ids[1]],
            "the mate is freed too"
        );
        doc.undo();
        assert_eq!(doc.group_mates(ids[1]).len(), 2);
        doc.ungroup_clips(&ids).unwrap();
        doc.group_clips(&ids).unwrap();
        assert_eq!(doc.group_mates(ids[1]).len(), 2);
        assert!(
            doc.group_clips(&ids[..1]).is_ok(),
            "one clip cannot be grouped"
        );
    }

    #[test]
    fn nothing_to_add_changes_nothing() {
        let mut doc = Document::new(Project::new("t"));
        assert!(doc.add_group(Vec::new(), 0, "x").unwrap().is_empty());
    }
}
