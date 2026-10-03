//! Copy, paste and duplicate clips.

use super::{Command, DocError, Document, Micros, edit, track_accepts};
use crate::ids::{ClipId, GroupId, LinkId, TrackId};
use crate::timeline::{Clip, Track, TrackKind};
use std::collections::HashMap;

/// A copied clip and where it came from.
#[derive(Clone, Debug)]
pub struct ClipboardItem {
    pub track: TrackId,
    pub kind: TrackKind,
    pub clip: Clip,
}

impl Document {
    /// Copy the clips in `ids` (in timeline order).
    pub fn copy_clips(&self, ids: &[ClipId]) -> Vec<ClipboardItem> {
        let mut items: Vec<ClipboardItem> = self
            .project
            .timeline()
            .tracks
            .iter()
            .flat_map(|t| {
                t.clips
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .map(|c| ClipboardItem {
                        track: t.id,
                        kind: t.kind,
                        clip: c.clone(),
                    })
            })
            .collect();
        items.sort_by_key(|i| i.clip.start);
        items
    }

    /// Paste copied clips so that the earliest one starts at `at`, keeping their spacing. Each goes on the track it
    /// came from (or the first free track of the same kind, or a new one). Pasted clips are new clips; links and
    /// groups between pasted clips are kept among the copies. One undo step. Returns the new clips.
    pub fn paste_clips(
        &mut self,
        items: &[ClipboardItem],
        at: Micros,
    ) -> Result<Vec<ClipId>, DocError> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let rate = self.project.frame_rate;
        let base = items.iter().map(|i| i.clip.start).min().unwrap_or(0);
        let shift = rate.snap(at.max(0)) - base;

        let mut links: HashMap<LinkId, LinkId> = HashMap::new();
        let mut groups: HashMap<GroupId, GroupId> = HashMap::new();
        let mut working: HashMap<TrackId, (Vec<Clip>, Vec<Clip>)> = HashMap::new(); // (before, after)
        let mut new_tracks: Vec<(TrackId, Track)> = Vec::new();
        let mut created = Vec::new();
        let mut main_cursor: Option<usize> = None;

        let mut ordered: Vec<&ClipboardItem> = items.iter().collect();
        ordered.sort_by_key(|i| i.clip.start);
        for item in ordered {
            let tl = self.project.timeline();
            // where it goes
            let dest = match tl.track(item.track) {
                Some(t) if !t.locked && track_accepts(&self.project, t.kind, &item.clip) => t.id,
                _ => match tl.tracks.iter().find(|t| {
                    t.kind == item.kind
                        && !t.locked
                        && track_accepts(&self.project, t.kind, &item.clip)
                }) {
                    Some(t) => t.id,
                    None => match new_tracks.iter().find(|(_, t)| t.kind == item.kind) {
                        Some((id, _)) => *id,
                        None if item.kind != TrackKind::Main => {
                            let track = Track::new(item.kind, tl.next_track_name(item.kind));
                            let id = track.id;
                            new_tracks.push((id, track));
                            id
                        }
                        None => return Err(super::EditError::WrongTrack.into()),
                    },
                },
            };
            let existing = tl.track(dest).map(|t| t.clips.clone()).unwrap_or_default();
            let kind = tl
                .track(dest)
                .map(|t| t.kind)
                .or_else(|| {
                    new_tracks
                        .iter()
                        .find(|(id, _)| *id == dest)
                        .map(|(_, t)| t.kind)
                })
                .unwrap_or(item.kind);
            let entry = working
                .entry(dest)
                .or_insert_with(|| (existing.clone(), existing));

            let mut clip = item.clip.clone();
            clip.id = ClipId::new();
            clip.link = clip.link.map(|l| *links.entry(l).or_default());
            clip.group = clip.group.map(|g| *groups.entry(g).or_default());
            let wanted = rate.snap((item.clip.start + shift).max(0));
            created.push(clip.id);
            if kind.is_magnetic() {
                let index = match main_cursor {
                    Some(i) => i + 1,
                    None => edit::index_for_time(&entry.1, wanted),
                };
                entry.1 = edit::insert(&entry.1, index, clip);
                main_cursor = Some(index);
            } else {
                clip.start = edit::free::first_free_start(&entry.1, wanted, clip.duration);
                entry.1 = edit::free::insert(&entry.1, clip)?;
            }
        }

        let tl = self.project.timeline();
        let mut commands = Vec::new();
        let first_new = tl.tracks.len();
        for (offset, (_, track)) in new_tracks.into_iter().enumerate() {
            let id = track.id;
            commands.push(Command::AddTrack {
                track: Box::new(track),
                index: first_new + offset,
            });
            let (_, after) = working.remove(&id).unwrap_or_default();
            commands.push(Command::EditTrack {
                track: id,
                before: Vec::new(),
                after,
                label: "Paste",
            });
        }
        for track in &tl.tracks {
            if let Some((before, after)) = working.remove(&track.id) {
                commands.push(Command::EditTrack {
                    track: track.id,
                    before,
                    after,
                    label: "Paste",
                });
            }
        }
        match commands.len() {
            0 => {}
            1 => self.run(commands.remove(0), None)?,
            _ => self.run(
                Command::Batch {
                    label: "Paste",
                    commands,
                },
                None,
            )?,
        }
        Ok(created)
    }

    /// Copy the clips in `ids` and paste them right after the last of them. One undo step.
    pub fn duplicate_clips(&mut self, ids: &[ClipId]) -> Result<Vec<ClipId>, DocError> {
        let items = self.copy_clips(ids);
        let end = items.iter().map(|i| i.clip.end()).max().unwrap_or(0);
        self.paste_clips(&items, end)
    }
}
