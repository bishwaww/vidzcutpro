//! The project: the one thing that is saved to a `.vzp` file.

use crate::ids::{MediaId, TimelineId};
use crate::layer::{BLACK, Color};
use crate::media::MediaAsset;
use crate::time::{FrameRate, Micros};
use crate::timeline::Timeline;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version of the `.vzp` file layout. Raise it when the layout changes and add a migration.
pub const FORMAT_VERSION: u32 = 2;

/// Why a project file could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("this is not a valid vidzcut project file: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error(
        "this project was made by a newer vidzcut (file version {found}, this vidzcut understands up to {supported}). Please update vidzcut."
    )]
    TooNew { found: u32, supported: u32 },
}

/// Common canvas shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectPreset {
    Wide16x9,
    Tall9x16,
    Square1x1,
    Photo4x3,
    Portrait3x4,
    Portrait4x5,
    Ultrawide21x9,
    Cinema235,
}

impl AspectPreset {
    pub const ALL: [Self; 8] = [
        Self::Wide16x9,
        Self::Tall9x16,
        Self::Square1x1,
        Self::Photo4x3,
        Self::Portrait3x4,
        Self::Portrait4x5,
        Self::Ultrawide21x9,
        Self::Cinema235,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wide16x9 => "16:9",
            Self::Tall9x16 => "9:16",
            Self::Square1x1 => "1:1",
            Self::Photo4x3 => "4:3",
            Self::Portrait3x4 => "3:4",
            Self::Portrait4x5 => "4:5",
            Self::Ultrawide21x9 => "21:9",
            Self::Cinema235 => "2.35:1",
        }
    }

    /// Canvas size in pixels with a short side of 1080 (as in most editors).
    pub fn size(self) -> (u32, u32) {
        match self {
            Self::Wide16x9 => (1920, 1080),
            Self::Tall9x16 => (1080, 1920),
            Self::Square1x1 => (1080, 1080),
            Self::Photo4x3 => (1440, 1080),
            Self::Portrait3x4 => (1080, 1440),
            Self::Portrait4x5 => (1080, 1350),
            Self::Ultrawide21x9 => (2520, 1080),
            Self::Cinema235 => (2538, 1080),
        }
    }
}

/// What shows behind the clips.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Background {
    Color(Color),
    /// A blurred, enlarged copy of the picture underneath (0.0 to 1.0 = how much blur).
    Blur(f32),
}

impl Default for Background {
    fn default() -> Self {
        Self::Color(BLACK)
    }
}

/// Size of the picture in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

impl Canvas {
    pub fn aspect(self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

/// Project settings, imported media and the timelines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub id: Uuid,
    pub name: String,
    pub canvas: Canvas,
    pub frame_rate: FrameRate,
    /// While true, the first clip put on an empty timeline sets the canvas shape and frame rate
    /// (like "Original" in other editors). Any manual change to either turns it off.
    #[serde(default)]
    pub auto_settings: bool,
    /// What shows behind the clips where the picture does not cover the canvas.
    #[serde(default)]
    pub background: Background,
    #[serde(default)]
    pub media: Vec<MediaAsset>,
    pub timelines: Vec<Timeline>,
    /// The timeline being edited (the first one when not set).
    #[serde(default)]
    pub active: Option<TimelineId>,
}

impl Project {
    /// A new, empty project: 16:9, 1920x1080, 30 fps, one empty timeline.
    pub fn new(name: impl Into<String>) -> Self {
        let (width, height) = AspectPreset::Wide16x9.size();
        Self {
            format_version: FORMAT_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            canvas: Canvas { width, height },
            frame_rate: FrameRate::FPS_30,
            auto_settings: true,
            background: Background::default(),
            media: Vec::new(),
            timelines: vec![Timeline::new("Timeline 01")],
            active: None,
        }
    }

    fn active_index(&self) -> usize {
        self.active
            .and_then(|id| self.timelines.iter().position(|t| t.id == id))
            .unwrap_or(0)
    }

    /// The timeline being edited.
    pub fn timeline(&self) -> &Timeline {
        &self.timelines[self.active_index()]
    }

    pub fn timeline_mut(&mut self) -> &mut Timeline {
        let i = self.active_index();
        &mut self.timelines[i]
    }

    pub fn timeline_by_id(&self, id: TimelineId) -> Option<&Timeline> {
        self.timelines.iter().find(|t| t.id == id)
    }

    /// Edit another timeline from now on. Returns false (and changes nothing) if there is no such timeline.
    pub fn set_active(&mut self, id: TimelineId) -> bool {
        let found = self.timelines.iter().any(|t| t.id == id);
        if found {
            self.active = Some(id);
        }
        found
    }

    /// Does `timeline` play `target` inside itself, however deep? (A compound clip must never contain itself.)
    pub fn contains_timeline(&self, timeline: TimelineId, target: TimelineId) -> bool {
        let Some(t) = self.timeline_by_id(timeline) else {
            return false;
        };
        t.tracks
            .iter()
            .flat_map(|t| &t.clips)
            .any(|c| match c.content {
                crate::content::Content::Compound(inner) => {
                    inner == target || self.contains_timeline(inner, target)
                }
                _ => false,
            })
    }

    /// The compound clips that play `timeline`: where they are is (timeline, clip).
    pub fn compound_users(&self, timeline: TimelineId) -> Vec<(TimelineId, crate::ids::ClipId)> {
        self.timelines
            .iter()
            .flat_map(|t| {
                t.tracks
                    .iter()
                    .flat_map(|k| &k.clips)
                    .filter_map(move |c| match c.content {
                        crate::content::Content::Compound(id) if id == timeline => {
                            Some((t.id, c.id))
                        }
                        _ => None,
                    })
            })
            .collect()
    }

    /// Length of the timeline being edited. Zero while it is empty.
    pub fn duration(&self) -> Micros {
        self.timeline().duration()
    }

    pub fn asset(&self, id: MediaId) -> Option<&MediaAsset> {
        self.media.iter().find(|m| m.id == id)
    }

    /// The kind of media a clip's content plays (`None` for text).
    pub fn asset_kind(&self, content: &crate::content::Content) -> Option<crate::media::MediaKind> {
        content.media().and_then(|m| self.asset(m)).map(|a| a.kind)
    }

    /// Imported files that can no longer be found on disk.
    pub fn missing_media(&self) -> Vec<MediaId> {
        self.media
            .iter()
            .filter(|m| !m.path.exists())
            .map(|m| m.id)
            .collect()
    }

    /// The canvas that fits a picture of `width` x `height`: a standard shape when it is close to one,
    /// otherwise the exact shape with a short side of 1080.
    pub fn canvas_for_picture(width: u32, height: u32) -> Canvas {
        if width == 0 || height == 0 {
            let (w, h) = AspectPreset::Wide16x9.size();
            return Canvas {
                width: w,
                height: h,
            };
        }
        let aspect = f64::from(width) / f64::from(height);
        let near = AspectPreset::ALL.into_iter().min_by(|a, b| {
            let da = (f64::from(a.size().0) / f64::from(a.size().1) / aspect - 1.0).abs();
            let db = (f64::from(b.size().0) / f64::from(b.size().1) / aspect - 1.0).abs();
            da.total_cmp(&db)
        });
        if let Some(p) = near
            .filter(|p| (f64::from(p.size().0) / f64::from(p.size().1) / aspect - 1.0).abs() < 0.04)
        {
            let (w, h) = p.size();
            return Canvas {
                width: w,
                height: h,
            };
        }
        let even = |v: f64| ((v.round() as u32).max(2) + 1) & !1;
        if width >= height {
            Canvas {
                width: even(1080.0 * aspect),
                height: 1080,
            }
        } else {
            Canvas {
                width: 1080,
                height: even(1080.0 / aspect),
            }
        }
    }

    pub fn set_aspect(&mut self, preset: AspectPreset) {
        let (width, height) = preset.size();
        self.canvas = Canvas { width, height };
    }

    /// The preset that matches the current canvas, if any.
    pub fn aspect_preset(&self) -> Option<AspectPreset> {
        AspectPreset::ALL
            .into_iter()
            .find(|p| p.size() == (self.canvas.width, self.canvas.height))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a project can always be turned into JSON")
    }

    /// Read a project from JSON, upgrading old file versions. Refuses files from a newer vidzcut.
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        let mut value: serde_json::Value = serde_json::from_str(text)?;
        let found = value
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if found > FORMAT_VERSION {
            return Err(LoadError::TooNew {
                found,
                supported: FORMAT_VERSION,
            });
        }
        migrate(&mut value, found);
        Ok(serde_json::from_value(value)?)
    }
}

/// Upgrade an older file to the current layout, one version at a time.
///
/// * 1 → 2: a clip's `media` id became `content: {"Media": id}` (clips can now also be text).
fn migrate(value: &mut serde_json::Value, from: u32) {
    if from < 2 {
        let timelines = value
            .get_mut("timelines")
            .and_then(serde_json::Value::as_array_mut);
        for timeline in timelines.into_iter().flatten() {
            let tracks = timeline
                .get_mut("tracks")
                .and_then(serde_json::Value::as_array_mut);
            for track in tracks.into_iter().flatten() {
                let clips = track
                    .get_mut("clips")
                    .and_then(serde_json::Value::as_array_mut);
                for clip in clips.into_iter().flatten() {
                    let Some(object) = clip.as_object_mut() else {
                        continue;
                    };
                    if let Some(media) = object.remove("media") {
                        object.insert("content".into(), serde_json::json!({ "Media": media }));
                    }
                }
            }
        }
        value["format_version"] = serde_json::json!(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_defaults() {
        let p = Project::new("My first video");
        assert_eq!((p.canvas.width, p.canvas.height), (1920, 1080));
        assert_eq!(p.frame_rate, FrameRate::FPS_30);
        assert_eq!(p.duration(), 0);
        assert_eq!(p.aspect_preset(), Some(AspectPreset::Wide16x9));
        assert_eq!(p.timeline().main_track().clips.len(), 0);
        assert!(p.auto_settings);
    }

    #[test]
    fn the_canvas_follows_the_shape_of_a_picture() {
        let c = |w, h| {
            let c = Project::canvas_for_picture(w, h);
            (c.width, c.height)
        };
        assert_eq!(c(1920, 1080), (1920, 1080));
        assert_eq!(c(3840, 2160), (1920, 1080));
        assert_eq!(c(1080, 1920), (1080, 1920));
        assert_eq!(c(1000, 1000), (1080, 1080));
        assert_eq!(c(1440, 1080), (1440, 1080));
        // an unusual shape keeps its exact shape with a 1080 short side
        assert_eq!(c(2000, 1000), (2160, 1080));
        assert_eq!(c(0, 0), (1920, 1080));
    }

    #[test]
    fn save_then_load_gives_the_same_project() {
        let mut p = Project::new("ශ්‍රී ලංකාව — ක්‍රීඩාව");
        p.set_aspect(AspectPreset::Tall9x16);
        p.frame_rate = FrameRate::FPS_29_97;
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
        assert_eq!(back.name, "ශ්‍රී ලංකාව — ක්‍රීඩාව");
    }

    #[test]
    fn newer_files_are_refused_with_a_clear_message() {
        let mut p = Project::new("x");
        p.format_version = FORMAT_VERSION + 1;
        match Project::from_json(&p.to_json()) {
            Err(LoadError::TooNew { found, supported }) => {
                assert_eq!((found, supported), (FORMAT_VERSION + 1, FORMAT_VERSION));
            }
            other => panic!("expected TooNew, got {other:?}"),
        }
    }

    #[test]
    fn a_phase_1_project_is_upgraded_when_opened() {
        // a file as vidzcut saved it before text and overlays existed (clips had a plain `media` id)
        let old = r#"{
            "format_version": 1,
            "id": "6b2f3a4e-0c43-4c7a-9d57-0a3b6d9a1111",
            "name": "old project",
            "canvas": {"width": 1080, "height": 1920},
            "frame_rate": {"num": 24, "den": 1},
            "auto_settings": false,
            "media": [],
            "timelines": [{
                "id": "6b2f3a4e-0c43-4c7a-9d57-0a3b6d9a2222",
                "name": "Timeline 01",
                "tracks": [{
                    "id": "6b2f3a4e-0c43-4c7a-9d57-0a3b6d9a3333",
                    "kind": "Main", "name": "Main",
                    "clips": [{
                        "id": "6b2f3a4e-0c43-4c7a-9d57-0a3b6d9a4444",
                        "media": "6b2f3a4e-0c43-4c7a-9d57-0a3b6d9a5555",
                        "start": 0, "source_in": 1000000, "duration": 2000000
                    }]
                }]
            }]
        }"#;
        let p = Project::from_json(old).unwrap();
        assert_eq!(p.format_version, 2);
        assert_eq!(p.name, "old project");
        let clip = &p.timeline().main_track().clips[0];
        assert!(matches!(clip.content, crate::content::Content::Media(_)));
        assert_eq!(
            (clip.start, clip.source_in, clip.duration),
            (0, 1_000_000, 2_000_000)
        );
        assert!(clip.transform.is_identity());
        assert_eq!(clip.opacity.value_at(0), 1.0);
        assert_eq!(p.background, Background::default());
    }

    #[test]
    fn garbage_is_refused() {
        assert!(matches!(
            Project::from_json("not json"),
            Err(LoadError::Invalid(_))
        ));
    }

    #[test]
    fn aspect_presets_have_a_1080_short_side() {
        for p in AspectPreset::ALL {
            let (w, h) = p.size();
            assert_eq!(w.min(h), 1080, "{p:?}");
        }
    }
}
