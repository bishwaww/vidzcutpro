//! A document: the project, its undo history, and where it is saved.
//!
//! This is what the screen holds. All edits go through the methods here, so each one is a [`Command`].

use crate::command::{Command, History, MergeKey, TrackFlags};
use crate::content::{Content, ShapeContent, TextContent};
use crate::edit::{self, EditError};
use crate::ids::{ClipId, MediaId, TrackId};
use crate::media::{MediaAsset, MediaKind};
use crate::project::{AspectPreset, Background, Canvas, LoadError, Project};
use crate::time::{FrameRate, Micros};
use crate::timeline::{Clip, Track, TrackKind};
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

    pub(crate) fn run(
        &mut self,
        command: Command,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
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

    // ---- tracks ---------------------------------------------------------------------------------------------

    /// Add an empty track on top (overlay and text) or at the end (audio). Returns its id.
    pub fn add_track(&mut self, kind: TrackKind) -> Result<TrackId, DocError> {
        if kind == TrackKind::Main {
            return Err(EditError::MainTrack.into());
        }
        let tl = self.project.timeline();
        let track = Track::new(kind, tl.next_track_name(kind));
        let id = track.id;
        let index = tl.tracks.len();
        self.run(
            Command::AddTrack {
                track: Box::new(track),
                index,
            },
            None,
        )?;
        Ok(id)
    }

    /// Remove a track and everything on it (undo brings it all back). The main track stays.
    pub fn remove_track(&mut self, id: TrackId) -> Result<(), DocError> {
        let tl = self.project.timeline();
        let index = tl
            .tracks
            .iter()
            .position(|t| t.id == id)
            .ok_or(EditError::TrackNotFound(id))?;
        let track = tl.tracks[index].clone();
        self.run(
            Command::RemoveTrack {
                track: Box::new(track),
                index,
            },
            None,
        )
    }

    /// Change a track's lock, hide or mute switch.
    pub fn set_track_flags(
        &mut self,
        id: TrackId,
        change: impl FnOnce(&mut TrackFlags),
    ) -> Result<(), DocError> {
        let track = self
            .project
            .timeline()
            .track(id)
            .ok_or(EditError::TrackNotFound(id))?;
        let before = TrackFlags::of(track);
        let mut after = before;
        change(&mut after);
        if after == before {
            return Ok(());
        }
        self.run(
            Command::SetTrackFlags {
                track: id,
                before,
                after,
            },
            None,
        )
    }

    pub fn set_background(&mut self, background: Background) -> Result<(), DocError> {
        if background == self.project.background {
            return Ok(());
        }
        self.run(
            Command::SetBackground {
                before: self.project.background,
                after: background,
            },
            None,
        )
    }

    /// Put a marker at `t`, or take away the one that is already there (within a frame). One undo step.
    /// Returns true if a marker was added.
    pub fn toggle_marker(&mut self, t: Micros) -> Result<bool, DocError> {
        let rate = self.project.frame_rate;
        let t = rate.snap(t.max(0));
        let before = self.project.timeline().markers.clone();
        let mut after = before.clone();
        let near = rate.frame_start(1);
        let added = match after.iter().position(|m| (m.time - t).abs() < near) {
            Some(i) => {
                after.remove(i);
                false
            }
            None => {
                after.push(crate::timeline::Marker {
                    id: crate::ids::MarkerId::new(),
                    time: t,
                    color: [252, 198, 16, 255],
                    note: String::new(),
                });
                after.sort_by_key(|m| m.time);
                true
            }
        };
        self.run(Command::SetMarkers { before, after }, None)?;
        Ok(added)
    }

    // ---- editing clips ---------------------------------------------------------------------------------------------

    /// Which track holds a clip.
    pub fn track_of(&self, clip: ClipId) -> Option<TrackId> {
        self.project.timeline().track_of(clip).map(|t| t.id)
    }

    fn kind_of(&self, clip: ClipId) -> Result<(TrackId, TrackKind), EditError> {
        let t = self
            .project
            .timeline()
            .track_of(clip)
            .ok_or(EditError::ClipNotFound(clip))?;
        Ok((t.id, t.kind))
    }

    /// Replace a track's clip list with the result of `f`, as one undoable command.
    fn edit_track(
        &mut self,
        track: TrackId,
        label: &'static str,
        merge: Option<MergeKey>,
        f: impl FnOnce(&Track) -> Result<Vec<Clip>, EditError>,
    ) -> Result<(), DocError> {
        let command = {
            let t = self
                .project
                .timeline()
                .track(track)
                .ok_or(EditError::TrackNotFound(track))?;
            Command::EditTrack {
                track,
                before: t.clips.clone(),
                after: f(t)?,
                label,
            }
        };
        self.run(command, merge)
    }

    fn edit_main(
        &mut self,
        label: &'static str,
        merge: Option<MergeKey>,
        f: impl FnOnce(&[Clip]) -> Result<Vec<Clip>, EditError>,
    ) -> Result<(), DocError> {
        let main = self.project.timeline().main_track().id;
        self.edit_track(main, label, merge, |t| f(&t.clips))
    }

    /// Put a media file on the main track at list position `index` (past the end = at the end).
    /// Returns the new clip's id.
    pub fn add_clip(&mut self, media: MediaId, index: usize) -> Result<ClipId, DocError> {
        let asset = self
            .project
            .asset(media)
            .ok_or(EditError::MediaNotFound(media))?;
        if asset.kind == MediaKind::Audio {
            return Err(EditError::WrongTrack.into());
        }
        // The first clip on an empty timeline sets the shape and frame rate of the project (unless the
        // user already chose them). All of it is one undo step.
        let track = self.project.timeline().main_track();
        let adopt = self.project.auto_settings && track.clips.is_empty() && asset.has_picture();
        let rate = if adopt {
            frame_rate_for(asset).unwrap_or(self.project.frame_rate)
        } else {
            self.project.frame_rate
        };
        let duration = rate
            .snap(asset.default_clip_duration())
            .max(rate.frame_start(1));
        let clip = Clip::media_clip(media, 0, 0, duration);
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

    /// Add a shape at `start` on the first overlay track with room there (and not locked), or on a new overlay
    /// track. One undo step.
    pub fn add_shape_auto(
        &mut self,
        start: Micros,
        duration: Micros,
        shape: ShapeContent,
    ) -> Result<(TrackId, ClipId), DocError> {
        let rate = self.project.frame_rate;
        let start = rate.snap(start.max(0));
        let duration = rate.snap(duration).max(rate.frame_start(1));
        let tl = self.project.timeline();
        let room = tl.tracks.iter().find(|t| {
            t.kind == TrackKind::Overlay
                && !t.locked
                && !edit::free::overlaps(&t.clips, start, start + duration, None)
        });
        if let Some(track) = room.map(|t| t.id) {
            let mut clip = Clip::shape_clip(shape, 0, duration);
            clip.start = start;
            let id = self.place_free(track, clip, start)?;
            return Ok((track, id));
        }
        let mut clip = Clip::shape_clip(shape, 0, duration);
        clip.start = start;
        let track = Track::new(TrackKind::Overlay, tl.next_track_name(TrackKind::Overlay));
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
                label: "Add shape",
            },
        ];
        self.run(
            Command::Batch {
                label: "Add shape",
                commands,
            },
            None,
        )?;
        Ok((track_id, clip_id))
    }

    /// What "add to timeline" does for a media file: pictures and videos join the main track at the cut nearest to
    /// `t`; sound goes on the first audio track with room at `t` (or on a new audio track).
    pub fn add_media_auto(&mut self, media: MediaId, t: Micros) -> Result<ClipId, DocError> {
        let asset = self
            .project
            .asset(media)
            .ok_or(EditError::MediaNotFound(media))?;
        if asset.kind != MediaKind::Audio {
            return self.add_clip_at_time(media, t);
        }
        let rate = self.project.frame_rate;
        let start = rate.snap(t.max(0));
        let end = start + asset.default_clip_duration();
        let room = self.project.timeline().tracks.iter().find(|tr| {
            tr.kind == TrackKind::Audio
                && !tr.locked
                && !edit::free::overlaps(&tr.clips, start, end, None)
        });
        match room.map(|tr| tr.id) {
            Some(track) => self.place_media(track, media, start),
            None => self
                .place_media_new_track(media, start)
                .map(|(_, clip)| clip),
        }
    }

    /// Put a media file on a track at about `start` (moved later if something is in the way).
    /// Pictures and videos go on the main or an overlay track; sound files on an audio track.
    pub fn place_media(
        &mut self,
        track: TrackId,
        media: MediaId,
        start: Micros,
    ) -> Result<ClipId, DocError> {
        let kind = self
            .project
            .timeline()
            .track(track)
            .ok_or(EditError::TrackNotFound(track))?
            .kind;
        let asset = self
            .project
            .asset(media)
            .ok_or(EditError::MediaNotFound(media))?;
        if !kind_accepts_media(kind, asset.kind) {
            return Err(EditError::WrongTrack.into());
        }
        if kind == TrackKind::Main {
            return self.add_clip_at_time(media, start);
        }
        let rate = self.project.frame_rate;
        let duration = rate
            .snap(asset.default_clip_duration())
            .max(rate.frame_start(1));
        let clip = Clip::media_clip(media, 0, 0, duration);
        self.place_free(track, clip, start)
    }

    /// Put a media file on a brand-new track (overlay for pictures and videos, audio for sound), as one undo step.
    pub fn place_media_new_track(
        &mut self,
        media: MediaId,
        start: Micros,
    ) -> Result<(TrackId, ClipId), DocError> {
        let asset = self
            .project
            .asset(media)
            .ok_or(EditError::MediaNotFound(media))?;
        let kind = if asset.kind == MediaKind::Audio {
            TrackKind::Audio
        } else {
            TrackKind::Overlay
        };
        let rate = self.project.frame_rate;
        let duration = rate
            .snap(asset.default_clip_duration())
            .max(rate.frame_start(1));
        let mut clip = Clip::media_clip(media, rate.snap(start.max(0)), 0, duration);
        clip.start = rate.snap(start.max(0));
        let tl = self.project.timeline();
        let mut track = Track::new(kind, tl.next_track_name(kind));
        let (track_id, clip_id) = (track.id, clip.id);
        track.clips.push(clip.clone());
        // the track is added empty and then filled, so undo removes both together
        let empty = Track {
            clips: Vec::new(),
            ..track
        };
        let commands = vec![
            Command::AddTrack {
                track: Box::new(empty),
                index: tl.tracks.len(),
            },
            Command::EditTrack {
                track: track_id,
                before: Vec::new(),
                after: vec![clip],
                label: "Add clip",
            },
        ];
        self.run(
            Command::Batch {
                label: "Add clip",
                commands,
            },
            None,
        )?;
        Ok((track_id, clip_id))
    }

    /// Add text at `start` on the first text track that has room there (and is not locked); if there is none,
    /// on a new text track. One undo step.
    pub fn add_text_auto(
        &mut self,
        start: Micros,
        duration: Micros,
        text: TextContent,
    ) -> Result<(TrackId, ClipId), DocError> {
        let rate = self.project.frame_rate;
        let start = rate.snap(start.max(0));
        let duration = rate.snap(duration).max(rate.frame_start(1));
        let tl = self.project.timeline();
        let room = tl.tracks.iter().find(|t| {
            t.kind == TrackKind::Text
                && !t.locked
                && !edit::free::overlaps(&t.clips, start, start + duration, None)
        });
        if let Some(track) = room {
            let track = track.id;
            return self
                .add_text(track, start, duration, text)
                .map(|id| (track, id));
        }
        let clip = Clip::text_clip(text, 0, duration);
        let mut clip = clip;
        clip.start = start;
        let mut track = Track::new(TrackKind::Text, tl.next_track_name(TrackKind::Text));
        let (track_id, clip_id) = (track.id, clip.id);
        track.clips.clear();
        let commands = vec![
            Command::AddTrack {
                track: Box::new(track),
                index: tl.tracks.len(),
            },
            Command::EditTrack {
                track: track_id,
                before: Vec::new(),
                after: vec![clip],
                label: "Add text",
            },
        ];
        self.run(
            Command::Batch {
                label: "Add text",
                commands,
            },
            None,
        )?;
        Ok((track_id, clip_id))
    }

    /// Move a clip onto a brand-new track (overlay or audio) at about `start`, as one undo step.
    pub fn move_clip_to_new_track(
        &mut self,
        clip: ClipId,
        kind: TrackKind,
        start: Micros,
        merge: Option<MergeKey>,
    ) -> Result<TrackId, DocError> {
        let rate = self.project.frame_rate;
        let (from_track, from_kind) = self.kind_of(clip)?;
        let tl = self.project.timeline();
        let mut moving = tl.clip(clip).ok_or(EditError::ClipNotFound(clip))?.clone();
        if !kind_accepts_content(
            kind,
            &moving.content,
            self.project.asset_kind(&moving.content),
        ) || kind == TrackKind::Main
        {
            return Err(EditError::WrongTrack.into());
        }
        let from_clips = tl
            .track(from_track)
            .ok_or(EditError::TrackNotFound(from_track))?
            .clips
            .clone();
        let removed = if from_kind.is_magnetic() {
            edit::delete(&from_clips, &[clip])?
        } else {
            edit::free::delete(&from_clips, &[clip])?
        };
        moving.start = rate.snap(start.max(0));
        let new_track = Track::new(kind, tl.next_track_name(kind));
        let new_id = new_track.id;
        let index = tl.tracks.len();
        let commands = vec![
            Command::AddTrack {
                track: Box::new(new_track),
                index,
            },
            Command::EditTrack {
                track: from_track,
                before: from_clips,
                after: removed,
                label: "Move clip",
            },
            Command::EditTrack {
                track: new_id,
                before: Vec::new(),
                after: vec![moving],
                label: "Move clip",
            },
        ];
        self.run(
            Command::Batch {
                label: "Move clip",
                commands,
            },
            merge,
        )?;
        Ok(new_id)
    }

    /// Move clips along the timeline by `delta` (they stay on their own tracks). Main-track clips are left alone,
    /// because that track places its clips by itself. Nothing moves if any clip would land on another.
    pub fn shift_clips(
        &mut self,
        ids: &[ClipId],
        delta: Micros,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let delta = if delta >= 0 {
            rate.snap(delta)
        } else {
            -rate.snap(-delta)
        };
        let mut commands = Vec::new();
        for track in self
            .project
            .timeline()
            .tracks
            .iter()
            .filter(|t| !t.kind.is_magnetic())
        {
            if !ids.iter().any(|id| track.clip(*id).is_some()) {
                continue;
            }
            let mut clips = track.clips.clone();
            for c in clips.iter_mut().filter(|c| ids.contains(&c.id)) {
                c.start += delta;
                if c.start < 0 {
                    return Err(EditError::Overlap.into());
                }
            }
            clips.sort_by_key(|c| c.start);
            if clips.windows(2).any(|w| w[0].end() > w[1].start) {
                return Err(EditError::Overlap.into());
            }
            commands.push(Command::EditTrack {
                track: track.id,
                before: track.clips.clone(),
                after: clips,
                label: "Move clips",
            });
        }
        match commands.len() {
            0 => Ok(()),
            1 => self.run(commands.remove(0), merge),
            _ => self.run(
                Command::Batch {
                    label: "Move clips",
                    commands,
                },
                merge,
            ),
        }
    }

    /// Put a piece of text on a text (or overlay) track at about `start`.
    pub fn add_text(
        &mut self,
        track: TrackId,
        start: Micros,
        duration: Micros,
        text: TextContent,
    ) -> Result<ClipId, DocError> {
        let kind = self
            .project
            .timeline()
            .track(track)
            .ok_or(EditError::TrackNotFound(track))?
            .kind;
        if !matches!(kind, TrackKind::Text | TrackKind::Overlay) {
            return Err(EditError::WrongTrack.into());
        }
        let rate = self.project.frame_rate;
        let clip = Clip::text_clip(text, 0, rate.snap(duration).max(rate.frame_start(1)));
        self.place_free(track, clip, start)
    }

    /// Insert a ready-made clip on a free track at the first free spot at or after `start`.
    pub fn place_free(
        &mut self,
        track: TrackId,
        mut clip: Clip,
        start: Micros,
    ) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let id = clip.id;
        self.edit_track(track, "Add clip", None, |t| {
            clip.start =
                edit::free::first_free_start(&t.clips, rate.snap(start.max(0)), clip.duration);
            edit::free::insert(&t.clips, clip)
        })?;
        Ok(id)
    }

    /// Change several clips in one undo step (the same change for each).
    pub fn update_clips(
        &mut self,
        ids: &[ClipId],
        label: &'static str,
        mut change: impl FnMut(&mut Clip),
    ) -> Result<(), DocError> {
        let mut commands = Vec::new();
        for id in ids {
            let before = self
                .project
                .timeline()
                .clip(*id)
                .ok_or(EditError::ClipNotFound(*id))?
                .clone();
            let mut after = before.clone();
            change(&mut after);
            if after != before {
                commands.push(Command::SetClip {
                    id: *id,
                    before: Box::new(before),
                    after: Box::new(after),
                    label,
                });
            }
        }
        match commands.len() {
            0 => Ok(()),
            1 => self.run(commands.remove(0), None),
            _ => self.run(Command::Batch { label, commands }, None),
        }
    }

    /// Change a clip's properties (position, size, opacity, text, …). `merge` joins a run of small
    /// changes (a slider drag) into one undo step. Does nothing if nothing changed.
    pub fn update_clip(
        &mut self,
        id: ClipId,
        label: &'static str,
        merge: Option<MergeKey>,
        change: impl FnOnce(&mut Clip),
    ) -> Result<(), DocError> {
        let before = self
            .project
            .timeline()
            .clip(id)
            .ok_or(EditError::ClipNotFound(id))?
            .clone();
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return Ok(());
        }
        self.run(
            Command::SetClip {
                id,
                before: Box::new(before),
                after: Box::new(after),
                label,
            },
            merge,
        )
    }

    /// Cut the clip showing at time `t` on the main track in two. Returns the new (right-hand) clip's id.
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

    /// Cut any clip in two at timeline time `t`. Returns the new (right-hand) clip's id.
    pub fn split_clip(&mut self, clip: ClipId, t: Micros) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let new_id = ClipId::new();
        let (track, kind) = self.kind_of(clip)?;
        self.edit_track(track, "Split", None, |tr| {
            if kind.is_magnetic() {
                edit::split(&tr.clips, clip, t, rate, new_id)
            } else {
                edit::free::split(&tr.clips, clip, t, rate, new_id)
            }
        })?;
        Ok(new_id)
    }

    /// Cut every clip in `ids` that covers time `t` (clips `t` does not cut through are left alone), as one
    /// undo step. Returns the new right-hand clips.
    pub fn split_clips(&mut self, ids: &[ClipId], t: Micros) -> Result<Vec<ClipId>, DocError> {
        let rate = self.project.frame_rate;
        let mut commands = Vec::new();
        let mut created = Vec::new();
        for track in self.project.timeline().tracks.iter().filter(|t| !t.locked) {
            let mut clips = track.clips.clone();
            let mut changed = false;
            for id in ids.iter().filter(|id| track.clip(**id).is_some()) {
                let new_id = ClipId::new();
                let result = if track.kind.is_magnetic() {
                    edit::split(&clips, *id, t, rate, new_id)
                } else {
                    edit::free::split(&clips, *id, t, rate, new_id)
                };
                match result {
                    Ok(next) => {
                        clips = next;
                        created.push(new_id);
                        changed = true;
                    }
                    Err(EditError::OutOfRange) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if changed {
                commands.push(Command::EditTrack {
                    track: track.id,
                    before: track.clips.clone(),
                    after: clips,
                    label: "Split",
                });
            }
        }
        match commands.len() {
            0 => Err(EditError::OutOfRange.into()),
            1 => self.run(commands.remove(0), None).map(|()| created),
            _ => self
                .run(
                    Command::Batch {
                        label: "Split",
                        commands,
                    },
                    None,
                )
                .map(|()| created),
        }
    }

    /// Remove the part of each clip in `ids` before (`keep_right`) or after (`!keep_right`) time `t`
    /// (the Q and W keys), as one undo step. Clips `t` does not cut through are left alone.
    pub fn cut_clips_at(
        &mut self,
        ids: &[ClipId],
        t: Micros,
        keep_right: bool,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let t = rate.snap(t);
        let mut commands = Vec::new();
        for track in self.project.timeline().tracks.iter().filter(|t| !t.locked) {
            let mut clips = track.clips.clone();
            let mut changed = false;
            for id in ids.iter().filter(|id| track.clip(**id).is_some()) {
                let Some(clip) = clips.iter().find(|c| c.id == *id).cloned() else {
                    continue;
                };
                if !(clip.start < t && t < clip.end()) {
                    continue;
                }
                let media_len = clip
                    .media()
                    .and_then(|m| self.project.asset(m))
                    .and_then(MediaAsset::max_clip_duration);
                let result = match (keep_right, track.kind.is_magnetic()) {
                    (true, true) => edit::trim_left(&clips, *id, t - clip.start, media_len, rate),
                    (true, false) => {
                        edit::free::trim_left(&clips, *id, t - clip.start, media_len, rate)
                    }
                    (false, true) => edit::trim_right(&clips, *id, t - clip.start, media_len, rate),
                    (false, false) => {
                        edit::free::trim_right(&clips, *id, t - clip.start, media_len, rate)
                    }
                };
                match result {
                    Ok(next) => {
                        clips = next;
                        changed = true;
                    }
                    Err(EditError::TooShort) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if changed {
                let label = if keep_right {
                    "Delete left"
                } else {
                    "Delete right"
                };
                commands.push(Command::EditTrack {
                    track: track.id,
                    before: track.clips.clone(),
                    after: clips,
                    label,
                });
            }
        }
        let label = if keep_right {
            "Delete left"
        } else {
            "Delete right"
        };
        match commands.len() {
            0 => Err(EditError::OutOfRange.into()),
            1 => self.run(commands.remove(0), None),
            _ => self.run(Command::Batch { label, commands }, None),
        }
    }

    /// Delete clips. On the main track the gap closes; on the other tracks it stays.
    pub fn delete_clips(&mut self, ids: &[ClipId]) -> Result<(), DocError> {
        let mut commands = Vec::new();
        for track in &self.project.timeline().tracks {
            let mine: Vec<ClipId> = ids
                .iter()
                .copied()
                .filter(|id| track.clip(*id).is_some())
                .collect();
            if mine.is_empty() {
                continue;
            }
            let after = if track.kind.is_magnetic() {
                edit::delete(&track.clips, &mine)?
            } else {
                edit::free::delete(&track.clips, &mine)?
            };
            commands.push(Command::EditTrack {
                track: track.id,
                before: track.clips.clone(),
                after,
                label: "Delete",
            });
        }
        if let Some(missing) = ids
            .iter()
            .find(|id| self.project.timeline().track_of(**id).is_none())
        {
            return Err(EditError::ClipNotFound(*missing).into());
        }
        match commands.len() {
            0 => Ok(()),
            1 => self.run(commands.remove(0), None),
            _ => self.run(
                Command::Batch {
                    label: "Delete",
                    commands,
                },
                None,
            ),
        }
    }

    /// Drag the left edge of a clip by `delta` (positive = shorter).
    pub fn trim_left(
        &mut self,
        clip: ClipId,
        delta: Micros,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let (track, kind) = self.kind_of(clip)?;
        let media_len = self
            .project
            .timeline()
            .clip(clip)
            .and_then(|c| c.media())
            .and_then(|m| self.project.asset(m))
            .and_then(MediaAsset::max_clip_duration);
        self.edit_track(track, "Trim", merge, |t| {
            if kind.is_magnetic() {
                edit::trim_left(&t.clips, clip, delta, media_len, rate)
            } else {
                edit::free::trim_left(&t.clips, clip, delta, media_len, rate)
            }
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
        let (track, kind) = self.kind_of(clip)?;
        let media_len = self
            .project
            .timeline()
            .clip(clip)
            .and_then(|c| c.media())
            .and_then(|m| self.project.asset(m))
            .and_then(MediaAsset::max_clip_duration);
        self.edit_track(track, "Trim", merge, |t| {
            if kind.is_magnetic() {
                edit::trim_right(&t.clips, clip, new_duration, media_len, rate)
            } else {
                edit::free::trim_right(&t.clips, clip, new_duration, media_len, rate)
            }
        })
    }

    /// Move a main-track clip to list position `to` (counted after it is taken out).
    pub fn move_clip(&mut self, clip: ClipId, to: usize) -> Result<(), DocError> {
        self.edit_main("Move clip", None, |clips| edit::move_clip(clips, clip, to))
    }

    /// Move a clip to a track at (about) time `start`. Works within a track and between tracks.
    /// On the main track `start` only chooses the place in the order.
    pub fn move_clip_to(
        &mut self,
        clip: ClipId,
        to_track: TrackId,
        start: Micros,
        merge: Option<MergeKey>,
    ) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let (from_track, from_kind) = self.kind_of(clip)?;
        let (moving, to_kind, to_clips, from_clips) = {
            let tl = self.project.timeline();
            let moving = tl.clip(clip).ok_or(EditError::ClipNotFound(clip))?.clone();
            let to = tl
                .track(to_track)
                .ok_or(EditError::TrackNotFound(to_track))?;
            let from = tl
                .track(from_track)
                .ok_or(EditError::TrackNotFound(from_track))?;
            (moving, to.kind, to.clips.clone(), from.clips.clone())
        };
        let media_kind = self.project.asset_kind(&moving.content);
        if !kind_accepts_content(to_kind, &moving.content, media_kind) {
            return Err(EditError::WrongTrack.into());
        }
        if from_track == to_track {
            return if to_kind.is_magnetic() {
                let others: Vec<Clip> = to_clips.iter().filter(|c| c.id != clip).cloned().collect();
                let index = edit::index_for_time(&others, start + moving.duration / 2);
                self.edit_track(to_track, "Move clip", merge, |t| {
                    edit::move_clip(&t.clips, clip, index)
                })
            } else {
                self.edit_track(to_track, "Move clip", merge, |t| {
                    edit::free::move_to(&t.clips, clip, start, rate)
                })
            };
        }
        // between two tracks: take it out of one and put it into the other, as one step
        let removed = if from_kind.is_magnetic() {
            edit::delete(&from_clips, &[clip])?
        } else {
            edit::free::delete(&from_clips, &[clip])?
        };
        let mut placed = moving;
        let inserted = if to_kind.is_magnetic() {
            let index = edit::index_for_time(&to_clips, start + placed.duration / 2);
            edit::insert(&to_clips, index, placed)
        } else {
            placed.start = rate.snap(start.max(0));
            edit::free::insert(&to_clips, placed)?
        };
        let commands = vec![
            Command::EditTrack {
                track: from_track,
                before: from_clips,
                after: removed,
                label: "Move clip",
            },
            Command::EditTrack {
                track: to_track,
                before: to_clips,
                after: inserted,
                label: "Move clip",
            },
        ];
        self.run(
            Command::Batch {
                label: "Move clip",
                commands,
            },
            merge,
        )
    }
}

/// Which kinds of media a kind of track takes.
pub fn kind_accepts_media(track: TrackKind, media: MediaKind) -> bool {
    match track {
        TrackKind::Main | TrackKind::Overlay => {
            matches!(media, MediaKind::Video | MediaKind::Image)
        }
        TrackKind::Audio => matches!(media, MediaKind::Audio),
        TrackKind::Text | TrackKind::Effect => false,
    }
}

fn kind_accepts_content(track: TrackKind, content: &Content, media: Option<MediaKind>) -> bool {
    match content {
        Content::Text(_) => matches!(track, TrackKind::Text | TrackKind::Overlay),
        Content::Shape(_) => track == TrackKind::Overlay,
        Content::Adjustment => track == TrackKind::Effect,
        Content::Media(_) => media.is_some_and(|m| kind_accepts_media(track, m)),
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

mod clipboard;
mod layers_auto;
mod retime;
mod transitions;
pub use clipboard::ClipboardItem;
pub use retime::{FREEZE_LENGTH, mean_speed};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_tracks;

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

/// Can this clip live on a track of this kind? (Pictures and videos on the main or an overlay track,
/// sound on an audio track, text on a text or overlay track.)
pub fn track_accepts(project: &Project, track: TrackKind, clip: &Clip) -> bool {
    kind_accepts_content(track, &clip.content, project.asset_kind(&clip.content))
}
