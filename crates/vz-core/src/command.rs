//! Commands: every change to a project is one of these, so every change can be undone and redone.
//!
//! The screen never edits the project directly. It builds a [`Command`] and gives it to
//! [`History::execute`].

use crate::edit::EditError;
use crate::ids::{MediaId, TrackId};
use crate::media::MediaAsset;
use crate::project::{Canvas, Project};
use crate::time::FrameRate;
use crate::timeline::Clip;

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
    /// Several commands that undo and redo as one step.
    Batch {
        label: &'static str,
        commands: Vec<Command>,
    },
}

impl Command {
    /// Name shown in "Undo …" menus.
    pub fn label(&self) -> &'static str {
        match self {
            Self::AddMedia(_) => "Import media",
            Self::EditTrack { label, .. } => label,
            Self::RelinkMedia { .. } => "Relink media",
            Self::Rename { .. } => "Rename project",
            Self::SetCanvas { .. } => "Change canvas",
            Self::SetFrameRate { .. } => "Change frame rate",
            Self::SetAutoSettings { .. } => "Change project settings",
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
            Self::RelinkMedia { id, before, .. } => {
                if let Some(asset) = project.media.iter_mut().find(|m| m.id == *id) {
                    asset.path = before.clone();
                }
            }
            Self::Rename { before, .. } => project.name = before.clone(),
            Self::SetCanvas { before, .. } => project.canvas = *before,
            Self::SetFrameRate { before, .. } => project.frame_rate = *before,
            Self::SetAutoSettings { before, .. } => project.auto_settings = *before,
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
            (Self::Rename { after, .. }, Self::Rename { after: a2, .. }) => {
                *after = a2.clone();
                true
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
    undo: Vec<Command>,
    redo: Vec<Command>,
    merge_key: Option<MergeKey>,
    /// Goes up with every change, undo and redo. Compared with the value at the last save to know if there is unsaved work.
    revision: u64,
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
        self.undo.last().map(Command::label)
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(Command::label)
    }

    /// Apply a command. With `merge: Some(key)`, a run of commands with the same key (a drag) is one undo step.
    pub fn execute(
        &mut self,
        project: &mut Project,
        command: Command,
        merge: Option<MergeKey>,
    ) -> Result<(), EditError> {
        command.apply(project)?;
        self.revision += 1;
        self.redo.clear();
        if let (Some(key), Some(prev)) = (merge, self.undo.last_mut())
            && self.merge_key == Some(key)
            && prev.absorb(&command)
        {
            return Ok(());
        }
        self.merge_key = merge;
        self.undo.push(command);
        Ok(())
    }

    /// End the current drag: the next command starts a new undo step even if it has the same key.
    pub fn seal(&mut self) {
        self.merge_key = None;
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        self.seal();
        let Some(command) = self.undo.pop() else {
            return false;
        };
        command.revert(project);
        self.redo.push(command);
        self.revision += 1;
        true
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        self.seal();
        let Some(command) = self.redo.pop() else {
            return false;
        };
        if command.apply(project).is_err() {
            // cannot happen after a successful undo; drop the redo history rather than leave a broken one
            self.redo.clear();
            return false;
        }
        self.undo.push(command);
        self.revision += 1;
        true
    }
}
