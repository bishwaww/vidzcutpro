//! A document: the project, its undo history, and where it is saved.
//!
//! This is what the screen holds. All edits go through the methods here, so each one is a [`Command`].

use crate::command::{Command, History, MergeKey};
use crate::edit::{self, EditError};
use crate::ids::{ClipId, MediaId};
use crate::media::MediaAsset;
use crate::project::{AspectPreset, Canvas, LoadError, Project};
use crate::time::{FrameRate, Micros};
use crate::timeline::Clip;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// File extension of vidzcut projects.
pub const EXTENSION: &str = "vzp";

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("could not read or write {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error(transparent)]
    Edit(#[from] EditError),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> DocError + '_ {
    move |source| DocError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub struct Document {
    pub project: Project,
    history: History,
    /// Where the project is saved, or `None` for a project that was never saved.
    pub path: Option<PathBuf>,
    saved_revision: u64,
}

impl Document {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            history: History::default(),
            path: None,
            saved_revision: 0,
        }
    }

    // ---- saving and opening ------------------------------------------------------------------

    /// True when there are changes that are not in the saved file.
    pub fn is_dirty(&self) -> bool {
        self.history.revision() != self.saved_revision
    }

    /// Save to `path` (or to the current path). The file is written to a temporary name first and then
    /// renamed, so a crash while saving never destroys the old file.
    pub fn save(&mut self, path: Option<&Path>) -> Result<(), DocError> {
        let target = match (path, &self.path) {
            (Some(p), _) => p.to_path_buf(),
            (None, Some(p)) => p.clone(),
            (None, None) => {
                return Err(DocError::Io {
                    path: PathBuf::new(),
                    source: io::Error::new(io::ErrorKind::InvalidInput, "no save location chosen"),
                });
            }
        };
        let dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
        for asset in &mut self.project.media {
            asset.relative_path = relative_path(&dir, &asset.path);
        }
        write_atomic(&target, self.project.to_json().as_bytes())?;
        self.path = Some(target);
        self.saved_revision = self.history.revision();
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self, DocError> {
        let text = std::fs::read_to_string(path).map_err(io_err(path))?;
        let mut project = Project::from_json(&text)?;
        let dir = path.parent().unwrap_or_else(|| Path::new(""));
        for asset in &mut project.media {
            // The project folder may have been moved: look for the file next to the project.
            if !asset.path.exists()
                && let Some(found) = asset
                    .relative_path
                    .as_ref()
                    .map(|r| dir.join(r))
                    .filter(|p| p.exists())
            {
                asset.path = found;
            }
        }
        let mut doc = Self::new(project);
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    // ---- undo / redo ---------------------------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        self.history.undo_label()
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.history.redo_label()
    }

    pub fn undo(&mut self) -> bool {
        self.history.undo(&mut self.project)
    }

    pub fn redo(&mut self) -> bool {
        self.history.redo(&mut self.project)
    }

    /// Call when a drag ends, so the next change is a new undo step.
    pub fn seal_history(&mut self) {
        self.history.seal();
    }

    /// Changes that happened since the last save, for deciding whether to autosave.
    pub fn revision(&self) -> u64 {
        self.history.revision()
    }

    fn run(&mut self, command: Command, merge: Option<MergeKey>) -> Result<(), DocError> {
        self.history.execute(&mut self.project, command, merge)?;
        Ok(())
    }

    // ---- project settings ------------------------------------------------------------------------

    pub fn rename(
        &mut self,
        name: impl Into<String>,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let after = name.into();
        if after == self.project.name {
            return Ok(());
        }
        self.run(
            Command::Rename {
                before: self.project.name.clone(),
                after,
            },
            merge,
        )
    }

    pub fn set_aspect(&mut self, preset: AspectPreset) -> Result<(), DocError> {
        let (width, height) = preset.size();
        let after = Canvas { width, height };
        if after == self.project.canvas {
            return Ok(());
        }
        let change = Command::SetCanvas {
            before: self.project.canvas,
            after,
        };
        self.run(self.manual_setting(change, "Change canvas"), None)
    }

    /// A change the user chose by hand also stops the project from following the first clip.
    fn manual_setting(&self, change: Command, label: &'static str) -> Command {
        if self.project.auto_settings {
            Command::Batch {
                label,
                commands: vec![
                    change,
                    Command::SetAutoSettings {
                        before: true,
                        after: false,
                    },
                ],
            }
        } else {
            change
        }
    }

    pub fn set_frame_rate(&mut self, rate: FrameRate) -> Result<(), DocError> {
        if rate == self.project.frame_rate {
            return Ok(());
        }
        let change = Command::SetFrameRate {
            before: self.project.frame_rate,
            after: rate,
        };
        self.run(self.manual_setting(change, "Change frame rate"), None)
    }

    // ---- media ---------------------------------------------------------------------------------------

    /// Add a media file to the project. If the same file is already there, returns the existing id.
    pub fn import_media(&mut self, asset: MediaAsset) -> Result<MediaId, DocError> {
        if let Some(existing) = self.project.media.iter().find(|m| m.path == asset.path) {
            return Ok(existing.id);
        }
        let id = asset.id;
        self.run(Command::AddMedia(asset), None)?;
        Ok(id)
    }

    /// Point a media file at a new place on disk (the old one is gone).
    pub fn relink_media(&mut self, id: MediaId, new_path: PathBuf) -> Result<(), DocError> {
        let before = self
            .project
            .asset(id)
            .ok_or(EditError::MediaNotFound(id))?
            .path
            .clone();
        self.run(
            Command::RelinkMedia {
                id,
                before,
                after: new_path,
            },
            None,
        )
    }

    // ---- editing the main track --------------------------------------------------------------------

    fn edit_main(
        &mut self,
        label: &'static str,
        merge: Option<MergeKey>,
        f: impl FnOnce(&[Clip]) -> Result<Vec<Clip>, EditError>,
    ) -> Result<(), DocError> {
        let track = self.project.timeline().main_track();
        let after = f(&track.clips)?;
        let command = Command::EditTrack {
            track: track.id,
            before: track.clips.clone(),
            after,
            label,
        };
        self.run(command, merge)
    }

    /// Put a media file on the main track at list position `index` (past the end = at the end).
    /// Returns the new clip's id.
    pub fn add_clip(&mut self, media: MediaId, index: usize) -> Result<ClipId, DocError> {
        let asset = self
            .project
            .asset(media)
            .ok_or(EditError::MediaNotFound(media))?;
        // The first clip on an empty timeline sets the shape and frame rate of the project (unless the
        // user already chose them). All of it is one undo step.
        let track = self.project.timeline().main_track();
        let adopt = self.project.auto_settings && track.clips.is_empty() && asset.has_picture();
        let rate = if adopt {
            frame_rate_for(asset).unwrap_or(self.project.frame_rate)
        } else {
            self.project.frame_rate
        };
        let clip = Clip {
            id: ClipId::new(),
            media,
            start: 0,
            source_in: 0,
            duration: rate
                .snap(asset.default_clip_duration())
                .max(rate.frame_start(1)),
        };
        let id = clip.id;
        let canvas = Project::canvas_for_picture(asset.width, asset.height);
        let edit = Command::EditTrack {
            track: track.id,
            before: track.clips.clone(),
            after: edit::insert(&track.clips, index, clip),
            label: "Add clip",
        };
        if adopt {
            let mut commands = Vec::new();
            if canvas != self.project.canvas {
                commands.push(Command::SetCanvas {
                    before: self.project.canvas,
                    after: canvas,
                });
            }
            if rate != self.project.frame_rate {
                commands.push(Command::SetFrameRate {
                    before: self.project.frame_rate,
                    after: rate,
                });
            }
            commands.push(Command::SetAutoSettings {
                before: true,
                after: false,
            });
            commands.push(edit);
            self.run(
                Command::Batch {
                    label: "Add clip",
                    commands,
                },
                None,
            )?;
        } else {
            self.run(edit, None)?;
        }
        Ok(id)
    }

    /// Put a media file on the main track at the cut nearest to time `t`.
    pub fn add_clip_at_time(&mut self, media: MediaId, t: Micros) -> Result<ClipId, DocError> {
        let index = edit::index_for_time(&self.project.timeline().main_track().clips, t);
        self.add_clip(media, index)
    }

    /// Cut the clip showing at time `t` in two. Returns the new (right-hand) clip's id.
    pub fn split_at(&mut self, t: Micros) -> Result<ClipId, DocError> {
        let id = self
            .project
            .timeline()
            .main_track()
            .clip_at(t)
            .map(|c| c.id)
            .ok_or(EditError::OutOfRange)?;
        self.split_clip(id, t)
    }

    pub fn split_clip(&mut self, clip: ClipId, t: Micros) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let new_id = ClipId::new();
        self.edit_main("Split", None, |clips| {
            edit::split(clips, clip, t, rate, new_id)
        })?;
        Ok(new_id)
    }

    /// Delete clips and close the gap (ripple delete).
    pub fn delete_clips(&mut self, ids: &[ClipId]) -> Result<(), DocError> {
        self.edit_main("Delete", None, |clips| edit::delete(clips, ids))
    }

    /// Drag the left edge of a clip by `delta` (positive = shorter).
    pub fn trim_left(
        &mut self,
        clip: ClipId,
        delta: Micros,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        self.edit_main("Trim", merge, |clips| {
            edit::trim_left(clips, clip, delta, rate)
        })
    }

    /// Set a clip's length (drag its right edge).
    pub fn trim_right(
        &mut self,
        clip: ClipId,
        new_duration: Micros,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let media_len = self
            .project
            .timeline()
            .main_track()
            .clip(clip)
            .and_then(|c| self.project.asset(c.media))
            .and_then(MediaAsset::max_clip_duration);
        self.edit_main("Trim", merge, |clips| {
            edit::trim_right(clips, clip, new_duration, media_len, rate)
        })
    }

    /// Move a clip to list position `to` (counted after it is taken out).
    pub fn move_clip(&mut self, clip: ClipId, to: usize) -> Result<(), DocError> {
        self.edit_main("Move clip", None, |clips| edit::move_clip(clips, clip, to))
    }
}

/// Write a file without ever leaving a half-written one: write `name.tmp`, then rename over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), DocError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io_err(path))?;
    Ok(())
}

/// `to` written relative to the folder `from_dir` (e.g. `../media/a.mp4`), or `None` when that is not possible
/// (different drives).
pub fn relative_path(from_dir: &Path, to: &Path) -> Option<PathBuf> {
    let from: Vec<_> = from_dir.components().collect();
    let to_c: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to_c).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None; // nothing in common, e.g. different drives
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to_c[common..] {
        out.push(c.as_os_str());
    }
    Some(out)
}

// ---- crash recovery -------------------------------------------------------------------------------

/// What an autosave file holds: the project plus where it was meant to be saved.
#[derive(Serialize, Deserialize)]
struct AutosaveFile {
    original_path: Option<PathBuf>,
    saved_at_unix: u64,
    project: Project,
}

/// An autosaved project found after a crash.
#[derive(Debug, Clone)]
pub struct RecoveryItem {
    pub file: PathBuf,
    pub original_path: Option<PathBuf>,
    pub name: String,
    pub saved_at_unix: u64,
}

impl Document {
    fn autosave_file(dir: &Path, project: &Project) -> PathBuf {
        dir.join(format!("{}.autosave.json", project.id))
    }

    /// Write a recovery copy into `dir`. Called every 30 seconds while there are unsaved changes.
    pub fn autosave(&self, dir: &Path) -> Result<(), DocError> {
        let file = AutosaveFile {
            original_path: self.path.clone(),
            saved_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            project: self.project.clone(),
        };
        let json = serde_json::to_string(&file).expect("autosave data can always be encoded");
        write_atomic(&Self::autosave_file(dir, &self.project), json.as_bytes())
    }

    /// Remove the recovery copy (after a successful save or a clean close).
    pub fn discard_autosave(&self, dir: &Path) {
        let _ = std::fs::remove_file(Self::autosave_file(dir, &self.project));
    }

    /// Recovery copies left behind (the app did not close cleanly), newest first.
    pub fn recovery_items(dir: &Path) -> Vec<RecoveryItem> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut items: Vec<RecoveryItem> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".autosave.json"))
            .filter_map(|e| {
                let text = std::fs::read_to_string(e.path()).ok()?;
                let f: AutosaveFile = serde_json::from_str(&text).ok()?;
                Some(RecoveryItem {
                    file: e.path(),
                    original_path: f.original_path,
                    name: f.project.name,
                    saved_at_unix: f.saved_at_unix,
                })
            })
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.saved_at_unix));
        items
    }

    /// Open a recovery copy. The document counts as unsaved and keeps its original path.
    pub fn restore(item: &RecoveryItem) -> Result<Self, DocError> {
        let text = std::fs::read_to_string(&item.file).map_err(io_err(&item.file))?;
        let f: AutosaveFile = serde_json::from_str(&text).map_err(LoadError::from)?;
        let mut doc = Self::new(f.project);
        doc.path = f.original_path;
        doc.saved_revision = u64::MAX; // never equal to a real revision: needs saving
        Ok(doc)
    }
}

#[cfg(test)]
mod tests;

/// The project frame rate that suits a video: the standard rate nearest to the file's own.
/// `None` for pictures and for files that do not say.
fn frame_rate_for(asset: &MediaAsset) -> Option<FrameRate> {
    if asset.kind != crate::media::MediaKind::Video {
        return None;
    }
    let wanted = asset.frame_rate?.as_f64();
    FrameRate::PRESETS.into_iter().min_by(|a, b| {
        (a.as_f64() - wanted)
            .abs()
            .total_cmp(&(b.as_f64() - wanted).abs())
    })
}
