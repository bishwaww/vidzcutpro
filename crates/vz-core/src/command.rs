//! Commands: every change to a project is one of these, so every change can be undone and redone.
//!
//! The screen never edits the project directly. It builds a [`Command`] and gives it to
//! [`History::execute`].

use crate::edit::EditError;
use crate::ids::{ClipId, MediaId, TimelineId, TrackId};
use crate::media::MediaAsset;
use crate::project::{Background, Canvas, Project};
use crate::time::FrameRate;
use crate::timeline::{Clip, Marker, Timeline, Track};

/// The three switches on a track header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackFlags {
    pub locked: bool,
    pub hidden: bool,
    pub muted: bool,
}

impl TrackFlags {
    pub fn of(track: &Track) -> Self {
        Self {
            locked: track.locked,
            hidden: track.hidden,
            muted: track.muted,
        }
    }
}

/// One undoable change.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Import a media file into the project.
    AddMedia(MediaAsset),
    /// Replace the clips of a track. `before` and `after` are the whole clip lists, so undo is exact.
    EditTrack {
        track: TrackId,
        before: Vec<Clip>,
        after: Vec<Clip>,
        label: &'static str,
    },
    /// Change one clip's properties (position, size, opacity, text, …). Wherever the clip is.
    SetClip {
        id: ClipId,
        before: Box<Clip>,
        after: Box<Clip>,
        label: &'static str,
    },
    /// Add a track at this position in the track list.
    AddTrack {
        track: Box<Track>,
        index: usize,
    },
    /// Remove a track (it is kept here so undo can put it back, clips and all).
    RemoveTrack {
        track: Box<Track>,
        index: usize,
    },
    SetTrackFlags {
        track: TrackId,
        before: TrackFlags,
        after: TrackFlags,
    },
    SetMarkers {
        before: Vec<Marker>,
        after: Vec<Marker>,
    },
    /// Replace the transitions of a track.
    SetTransitions {
        track: TrackId,
        before: Vec<crate::timeline::Transition>,
        after: Vec<crate::timeline::Transition>,
    },
    SetBackground {
        before: Background,
        after: Background,
    },
    /// Point a media file at a new location on disk (after the file was moved).
    RelinkMedia {
        id: MediaId,
        before: std::path::PathBuf,
        after: std::path::PathBuf,
    },
    Rename {
        before: String,
        after: String,
    },
    SetCanvas {
        before: Canvas,
        after: Canvas,
    },
    SetFrameRate {
        before: FrameRate,
        after: FrameRate,
    },
    /// Whether the project still follows the first clip's size and frame rate.
    SetAutoSettings {
        before: bool,
        after: bool,
    },
    /// Add a timeline (a compound clip's inside, a new tab) at the end of the list.
    AddTimeline(Box<Timeline>),
    /// Remove a timeline (kept here so undo can put it back).
    RemoveTimeline {
        timeline: Box<Timeline>,
        index: usize,
    },
    RenameTimeline {
        id: TimelineId,
        before: String,
        after: String,
    },
    /// Several commands that undo and redo as one step.
    Batch {
        label: &'static str,
        commands: Vec<Command>,
    },
}

fn find_clip_mut(project: &mut Project, id: ClipId) -> Option<&mut Clip> {
    project
        .timeline_mut()
        .tracks
        .iter_mut()
        .flat_map(|t| t.clips.iter_mut())
        .find(|c| c.id == id)
}

impl Command {
    /// Name shown in "Undo …" menus.
    pub fn label(&self) -> &'static str {
        match self {
            Self::AddMedia(_) => "Import media",
            Self::EditTrack { label, .. } | Self::SetClip { label, .. } => label,
            Self::AddTrack { .. } => "Add track",
            Self::RemoveTrack { .. } => "Remove track",
            Self::SetTrackFlags { .. } => "Change track",
            Self::SetMarkers { .. } => "Change markers",
            Self::SetTransitions { .. } => "Change transition",
            Self::SetBackground { .. } => "Change background",
            Self::RelinkMedia { .. } => "Relink media",
            Self::Rename { .. } => "Rename project",
            Self::SetCanvas { .. } => "Change canvas",
            Self::SetFrameRate { .. } => "Change frame rate",
            Self::SetAutoSettings { .. } => "Change project settings",
            Self::AddTimeline(_) => "Add timeline",
            Self::RemoveTimeline { .. } => "Delete timeline",
            Self::RenameTimeline { .. } => "Rename timeline",
            Self::Batch { label, .. } => label,
        }
    }

    pub fn apply(&self, project: &mut Project) -> Result<(), EditError> {
        match self {
            Self::AddMedia(asset) => project.media.push(asset.clone()),
            Self::EditTrack { track, after, .. } => {
                let t = project
                    .timeline_mut()
                    .track_mut(*track)
                    .ok_or(EditError::TrackNotFound(*track))?;
                if t.locked {
                    return Err(EditError::Locked);
                }
                t.clips = after.clone();
            }
            Self::SetClip { id, after, .. } => {
                let locked = project.timeline().track_of(*id).map(|t| t.locked);
                if locked == Some(true) {
                    return Err(EditError::Locked);
                }
                let clip = find_clip_mut(project, *id).ok_or(EditError::ClipNotFound(*id))?;
                *clip = (**after).clone();
            }
            Self::AddTrack { track, index } => {
                let tracks = &mut project.timeline_mut().tracks;
                tracks.insert((*index).min(tracks.len()), (**track).clone());
            }
            Self::RemoveTrack { track, .. } => {
                if track.kind == crate::timeline::TrackKind::Main {
                    return Err(EditError::MainTrack);
                }
                let tracks = &mut project.timeline_mut().tracks;
                let i = tracks
                    .iter()
                    .position(|t| t.id == track.id)
                    .ok_or(EditError::TrackNotFound(track.id))?;
                tracks.remove(i);
            }
            Self::SetTrackFlags { track, after, .. } => {
                let t = project
                    .timeline_mut()
                    .track_mut(*track)
                    .ok_or(EditError::TrackNotFound(*track))?;
                (t.locked, t.hidden, t.muted) = (after.locked, after.hidden, after.muted);
            }
            Self::SetMarkers { after, .. } => project.timeline_mut().markers = after.clone(),
            Self::SetTransitions { track, after, .. } => {
                project
                    .timeline_mut()
                    .track_mut(*track)
                    .ok_or(EditError::TrackNotFound(*track))?
                    .transitions = after.clone();
            }
            Self::SetBackground { after, .. } => project.background = *after,
            Self::RelinkMedia { id, after, .. } => {
                let asset = project
                    .media
                    .iter_mut()
                    .find(|m| m.id == *id)
                    .ok_or(EditError::MediaNotFound(*id))?;
                asset.path = after.clone();
            }
            Self::Rename { after, .. } => project.name = after.clone(),
            Self::SetCanvas { after, .. } => project.canvas = *after,
            Self::SetFrameRate { after, .. } => project.frame_rate = *after,
            Self::SetAutoSettings { after, .. } => project.auto_settings = *after,
            Self::AddTimeline(timeline) => project.timelines.push((**timeline).clone()),
            Self::RemoveTimeline { timeline, .. } => {
                if project.timelines.len() < 2 {
                    return Err(EditError::LastTimeline);
                }
                let i = project
                    .timelines
                    .iter()
                    .position(|t| t.id == timeline.id)
                    .ok_or(EditError::TimelineNotFound(timeline.id))?;
                project.timelines.remove(i);
                if project.active == Some(timeline.id) {
                    project.active = None;
                }
            }
            Self::RenameTimeline { id, after, .. } => {
                project
                    .timelines
                    .iter_mut()
                    .find(|t| t.id == *id)
                    .ok_or(EditError::TimelineNotFound(*id))?
                    .name = after.clone();
            }
            Self::Batch { commands, .. } => {
                for (i, c) in commands.iter().enumerate() {
                    if let Err(e) = c.apply(project) {
                        // undo what was already done, so a failed batch changes nothing
                        for done in commands[..i].iter().rev() {
                            done.revert(project);
                        }
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    /// Undo this command. Only called after it was applied, so it cannot fail in normal use.
    pub fn revert(&self, project: &mut Project) {
        match self {
            Self::AddMedia(asset) => project.media.retain(|m| m.id != asset.id),
            Self::EditTrack { track, before, .. } => {
                if let Some(t) = project.timeline_mut().track_mut(*track) {
                    t.clips = before.clone();
                }
            }
            Self::SetClip { id, before, .. } => {
                if let Some(clip) = find_clip_mut(project, *id) {
                    *clip = (**before).clone();
                }
            }
            Self::AddTrack { track, .. } => {
                project.timeline_mut().tracks.retain(|t| t.id != track.id)
            }
            Self::RemoveTrack { track, index } => {
                let tracks = &mut project.timeline_mut().tracks;
                tracks.insert((*index).min(tracks.len()), (**track).clone());
            }
            Self::SetTrackFlags { track, before, .. } => {
                if let Some(t) = project.timeline_mut().track_mut(*track) {
                    (t.locked, t.hidden, t.muted) = (before.locked, before.hidden, before.muted);
                }
            }
            Self::SetMarkers { before, .. } => project.timeline_mut().markers = before.clone(),
            Self::SetTransitions { track, before, .. } => {
                if let Some(t) = project.timeline_mut().track_mut(*track) {
                    t.transitions = before.clone();
                }
            }
            Self::SetBackground { before, .. } => project.background = *before,
            Self::RelinkMedia { id, before, .. } => {
                if let Some(asset) = project.media.iter_mut().find(|m| m.id == *id) {
                    asset.path = before.clone();
                }
            }
            Self::Rename { before, .. } => project.name = before.clone(),
            Self::SetCanvas { before, .. } => project.canvas = *before,
            Self::SetFrameRate { before, .. } => project.frame_rate = *before,
            Self::SetAutoSettings { before, .. } => project.auto_settings = *before,
            Self::AddTimeline(timeline) => {
                project.timelines.retain(|t| t.id != timeline.id);
                if project.active == Some(timeline.id) {
                    project.active = None;
                }
            }
            Self::RemoveTimeline { timeline, index } => {
                let at = (*index).min(project.timelines.len());
                project.timelines.insert(at, (**timeline).clone());
            }
            Self::RenameTimeline { id, before, .. } => {
                if let Some(t) = project.timelines.iter_mut().find(|t| t.id == *id) {
                    t.name = before.clone();
                }
            }
            Self::Batch { commands, .. } => {
                for c in commands.iter().rev() {
                    c.revert(project);
                }
            }
        }
    }

    /// Fold a newer command into this one when they are two moments of the same drag.
    fn absorb(&mut self, newer: &Self) -> bool {
        match (self, newer) {
            (
                Self::EditTrack { track, after, .. },
                Self::EditTrack {
                    track: t2,
                    after: a2,
                    ..
                },
            ) if track == t2 => {
                *after = a2.clone();
                true
            }
            (
                Self::SetClip { id, after, .. },
                Self::SetClip {
                    id: id2, after: a2, ..
                },
            ) if id == id2 => {
                *after = a2.clone();
                true
            }
            (
                Self::SetTransitions { track, after, .. },
                Self::SetTransitions {
                    track: t2,
                    after: a2,
                    ..
                },
            ) if track == t2 => {
                *after = a2.clone();
                true
            }
            (Self::Rename { after, .. }, Self::Rename { after: a2, .. }) => {
                *after = a2.clone();
                true
            }
            (
                Self::Batch { commands, .. },
                Self::Batch {
                    commands: newer_commands,
                    ..
                },
            ) if commands.len() == newer_commands.len() => {
                // two moments of a drag that touched several tracks: merge command by command
                let mut merged = commands.clone();
                let all = merged
                    .iter_mut()
                    .zip(newer_commands)
                    .all(|(old, new)| old.absorb(new));
                if all {
                    *commands = merged;
                }
                all
            }
            _ => false,
        }
    }
}

/// Identifies one drag or typing session so its many small changes become one undo step.
pub type MergeKey = u64;

/// The undo and redo stacks.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    merge_key: Option<MergeKey>,
    /// Goes up with every change, undo and redo. Compared with the value at the last save to know if there is unsaved work.
    revision: u64,
}

/// A command and the timeline that was being edited when it was made (undo and redo go back there first).
#[derive(Debug)]
struct Entry {
    command: Command,
    timeline: TimelineId,
}

impl History {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|e| e.command.label())
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|e| e.command.label())
    }

    /// Apply a command. With `merge: Some(key)`, a run of commands with the same key (a drag) is one undo step.
    pub fn execute(
        &mut self,
        project: &mut Project,
        command: Command,
        merge: Option<MergeKey>,
    ) -> Result<(), EditError> {
        let timeline = project.timeline().id;
        command.apply(project)?;
        self.revision += 1;
        self.redo.clear();
        if let (Some(key), Some(prev)) = (merge, self.undo.last_mut())
            && self.merge_key == Some(key)
            && prev.timeline == timeline
            && prev.command.absorb(&command)
        {
            return Ok(());
        }
        self.merge_key = merge;
        self.undo.push(Entry { command, timeline });
        Ok(())
    }

    /// End the current drag: the next command starts a new undo step even if it has the same key.
    pub fn seal(&mut self) {
        self.merge_key = None;
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        self.seal();
        let Some(entry) = self.undo.pop() else {
            return false;
        };
        project.set_active(entry.timeline);
        entry.command.revert(project);
        // undoing the removal of a timeline brings it back: go there
        project.set_active(entry.timeline);
        self.redo.push(entry);
        self.revision += 1;
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        self.seal();
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        project.set_active(entry.timeline);
        if entry.command.apply(project).is_err() {
            // cannot happen after a successful undo; drop the redo history rather than leave a broken one
            self.redo.clear();
            return false;
        }
        self.undo.push(entry);
        self.revision += 1;
        true
    }
}
