//! Timelines, tracks and clips.

use crate::ids::{ClipId, MediaId, TimelineId, TrackId};
use crate::time::Micros;
use serde::{Deserialize, Serialize};

/// A piece of media placed on a track.
///
/// The clip shows the media from `source_in` for `duration`, starting at `start` on the timeline.
/// (Speed changes arrive in Phase 3; until then one microsecond on the timeline is one microsecond of the file.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub media: MediaId,
    /// Where the clip starts on the timeline.
    pub start: Micros,
    /// Where in the media file the clip begins.
    pub source_in: Micros,
    /// How long the clip lasts on the timeline.
    pub duration: Micros,
}

impl Clip {
    pub fn end(&self) -> Micros {
        self.start + self.duration
    }

    /// Does this clip cover time `t`? (The start counts, the end does not.)
    pub fn contains(&self, t: Micros) -> bool {
        t >= self.start && t < self.end()
    }

    /// The time inside the media file that is shown at timeline time `t`.
    pub fn source_time(&self, t: Micros) -> Micros {
        self.source_in + (t - self.start)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    /// The main video track. Magnetic: clips always sit end to end from 0 with no gaps.
    Main,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub muted: bool,
    /// Clips in timeline order.
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn main() -> Self {
        Self {
            id: TrackId::new(),
            kind: TrackKind::Main,
            name: "Main".into(),
            locked: false,
            hidden: false,
            muted: false,
            clips: Vec::new(),
        }
    }

    pub fn end(&self) -> Micros {
        self.clips.iter().map(Clip::end).max().unwrap_or(0)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// The clip showing at time `t`.
    pub fn clip_at(&self, t: Micros) -> Option<&Clip> {
        self.clips.iter().find(|c| c.contains(t))
    }
}

/// What is visible at one moment: a clip, and which moment of its media to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer<'a> {
    pub track: &'a Track,
    pub clip: &'a Clip,
    pub source_time: Micros,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    pub id: TimelineId,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Timeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TimelineId::new(),
            name: name.into(),
            tracks: vec![Track::main()],
        }
    }

    pub fn main_track(&self) -> &Track {
        self.tracks
            .iter()
            .find(|t| t.kind == TrackKind::Main)
            .expect("every timeline has a main track")
    }

    pub fn main_track_mut(&mut self) -> &mut Track {
        self.tracks
            .iter_mut()
            .find(|t| t.kind == TrackKind::Main)
            .expect("every timeline has a main track")
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    /// Length of the timeline: where the last clip ends.
    pub fn duration(&self) -> Micros {
        self.tracks.iter().map(Track::end).max().unwrap_or(0)
    }

    /// Everything visible at time `t`, bottom layer first. This is the one question both the player
    /// and the exporter ask ("what does frame `t` consist of?").
    pub fn layers_at(&self, t: Micros) -> Vec<Layer<'_>> {
        self.tracks
            .iter()
            .filter(|track| !track.hidden)
            .filter_map(|track| {
                track.clip_at(t).map(|clip| Layer {
                    track,
                    clip,
                    source_time: clip.source_time(t),
                })
            })
            .collect()
    }
}
