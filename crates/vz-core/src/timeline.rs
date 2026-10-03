//! Timelines, tracks and clips.

use crate::content::{Content, ShapeContent, TextContent};
use crate::grade::{Adjust, ChromaKey, EffectRef, MaskDef};
use crate::ids::{ClipId, GroupId, LinkId, MarkerId, MediaId, TimelineId, TrackId};
use crate::layer::{AnimRef, BlendMode, Color, Crop, Transform};
use crate::time::Micros;
use serde::{Deserialize, Serialize};
use vz_anim::{Animatable, Easing};

fn yes() -> bool {
    true
}

fn one() -> Animatable<f32> {
    Animatable::Static(1.0)
}

/// A piece of media (or text) placed on a track.
///
/// The clip shows the media from `source_in` for `duration`, starting at `start` on the timeline.
/// (Speed changes arrive in Phase 3; until then one microsecond on the timeline is one microsecond of the file.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub content: Content,
    /// Where the clip starts on the timeline.
    pub start: Micros,
    /// Where in the media file the clip begins (zero for text).
    pub source_in: Micros,
    /// How long the clip lasts on the timeline.
    pub duration: Micros,
    #[serde(default)]
    pub transform: Transform,
    /// 0.0 (invisible) to 1.0 (solid).
    #[serde(default = "one")]
    pub opacity: Animatable<f32>,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default)]
    pub crop: Crop,
    /// Sound level as a factor (1.0 = unchanged, 0.0 = silent, 2.0 = twice as loud).
    #[serde(default = "one")]
    pub volume: Animatable<f32>,
    /// Sound fades at the start and end of the clip.
    #[serde(default)]
    pub fade_in: Micros,
    #[serde(default)]
    pub fade_out: Micros,
    #[serde(default)]
    pub group: Option<GroupId>,
    #[serde(default)]
    pub link: Option<LinkId>,
    /// Colour adjustments (for an adjustment layer: applied to everything below).
    #[serde(default)]
    pub adjust: Adjust,
    /// Effects applied after the colour adjustments, in order.
    #[serde(default)]
    pub effects: Vec<EffectRef>,
    /// Masks: only the part inside all of them is shown.
    #[serde(default)]
    pub masks: Vec<MaskDef>,
    /// Removal of a background colour.
    #[serde(default)]
    pub chroma: Option<ChromaKey>,
    /// How fast the media plays: 1.0 normal, 2.0 twice as fast, 0.0 a freeze frame. Can change along the clip.
    #[serde(default = "one")]
    pub speed: Animatable<f32>,
    /// Play the part of the file backwards.
    #[serde(default)]
    pub reverse: bool,
    /// Keep the voice at its normal pitch when the speed changes (sound only).
    #[serde(default = "yes")]
    pub keep_pitch: bool,
    /// Ready-made animation when the clip appears, when it leaves, and a looping one while it is shown
    /// (a loop replaces in and out).
    #[serde(default)]
    pub anim_in: Option<AnimRef>,
    #[serde(default)]
    pub anim_out: Option<AnimRef>,
    #[serde(default)]
    pub anim_combo: Option<AnimRef>,
}

impl Clip {
    fn with_content(content: Content, start: Micros, source_in: Micros, duration: Micros) -> Self {
        Self {
            id: ClipId::new(),
            content,
            start,
            source_in,
            duration,
            transform: Transform::default(),
            opacity: one(),
            blend: BlendMode::Normal,
            crop: Crop::default(),
            volume: one(),
            fade_in: 0,
            fade_out: 0,
            group: None,
            link: None,
            anim_in: None,
            anim_out: None,
            anim_combo: None,
            adjust: Adjust::default(),
            effects: Vec::new(),
            masks: Vec::new(),
            chroma: None,
            speed: one(),
            reverse: false,
            keep_pitch: true,
        }
    }

    /// A clip of a media file.
    pub fn media_clip(media: MediaId, start: Micros, source_in: Micros, duration: Micros) -> Self {
        Self::with_content(Content::Media(media), start, source_in, duration)
    }

    /// A clip of text.
    pub fn text_clip(text: TextContent, start: Micros, duration: Micros) -> Self {
        Self::with_content(Content::Text(text), start, 0, duration)
    }

    /// A clip that plays another timeline.
    pub fn compound_clip(timeline: TimelineId, start: Micros, duration: Micros) -> Self {
        Self::with_content(Content::Compound(timeline), start, 0, duration)
    }

    pub fn is_compound(&self) -> bool {
        matches!(self.content, Content::Compound(_))
    }

    /// An adjustment layer (colour settings and effects for everything below).
    pub fn adjustment_clip(start: Micros, duration: Micros) -> Self {
        Self::with_content(Content::Adjustment, start, 0, duration)
    }

    /// Does this clip change what is below it instead of drawing something?
    pub fn is_adjustment(&self) -> bool {
        matches!(self.content, Content::Adjustment)
    }

    /// A clip of a drawn shape.
    pub fn shape_clip(shape: ShapeContent, start: Micros, duration: Micros) -> Self {
        Self::with_content(Content::Shape(shape), start, 0, duration)
    }

    /// The media file this clip plays, if it is a media clip.
    pub fn media(&self) -> Option<MediaId> {
        self.content.media()
    }

    pub fn is_text(&self) -> bool {
        matches!(self.content, Content::Text(_))
    }

    pub fn end(&self) -> Micros {
        self.start + self.duration
    }

    /// Does this clip cover time `t`? (The start counts, the end does not.)
    pub fn contains(&self, t: Micros) -> bool {
        t >= self.start && t < self.end()
    }

    /// Move every keyframe `delta` microseconds later in the clip (negative: earlier). Used when the start of a clip
    /// changes, so an animation stays attached to the same pictures.
    pub fn shift_keys(&mut self, delta: Micros) {
        self.transform.x.shift(delta);
        self.transform.y.shift(delta);
        self.transform.scale_x.shift(delta);
        self.transform.scale_y.shift(delta);
        self.transform.rotation.shift(delta);
        self.opacity.shift(delta);
        self.volume.shift(delta);
        self.speed.shift(delta);
    }

    /// How far into the clip timeline time `t` is (what keyframes are measured from).
    pub fn local_time(&self, t: Micros) -> Micros {
        t - self.start
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    /// The main video track. Magnetic: clips always sit end to end from 0 with no gaps.
    Main,
    /// Videos, photos and stickers placed freely above the main track.
    Overlay,
    /// Text and captions.
    Text,
    /// Music, voice-over and sound effects.
    Audio,
    /// Effects and adjustment layers: they change everything on the tracks below them.
    Effect,
}

impl TrackKind {
    /// Clips on this kind of track are drawn on the picture.
    pub fn is_visual(self) -> bool {
        !matches!(self, Self::Audio)
    }

    /// Only the main track closes gaps by itself.
    pub fn is_magnetic(self) -> bool {
        matches!(self, Self::Main)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "Main",
            Self::Overlay => "Overlay",
            Self::Text => "Text",
            Self::Audio => "Audio",
            Self::Effect => "Effects",
        }
    }
}

/// A transition between a clip and the clip right after it on the same track. It is centred on the cut: the
/// second half of it is shown over the end of the first clip, the first half over the start of the next.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// The clip that ends at the cut (the outgoing one).
    pub after: ClipId,
    /// Which transition (see `vz-presets`).
    pub preset: String,
    pub duration: Micros,
    #[serde(default)]
    pub easing: Easing,
}

impl Transition {
    /// The time the transition really takes between these two clips: as long as asked, but never longer than
    /// either clip, so the transitions at the two ends of a clip never overlap.
    pub fn effective(&self, outgoing: &Clip, incoming: &Clip) -> Micros {
        self.duration
            .min(outgoing.duration)
            .min(incoming.duration)
            .max(0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub transitions: Vec<Transition>,
    /// A text track that holds captions (the caption list shows these clips).
    #[serde(default)]
    pub captions: bool,
}

impl Track {
    pub fn new(kind: TrackKind, name: impl Into<String>) -> Self {
        Self {
            id: TrackId::new(),
            kind,
            name: name.into(),
            locked: false,
            hidden: false,
            muted: false,
            clips: Vec::new(),
            transitions: Vec::new(),
            captions: false,
        }
    }

    pub fn main() -> Self {
        Self::new(TrackKind::Main, "Main")
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

    /// The transition at the end of `clip`, if there is one and a clip follows right after it.
    pub fn transition_after(&self, clip: ClipId) -> Option<(&Transition, &Clip, &Clip)> {
        let tr = self.transitions.iter().find(|t| t.after == clip)?;
        let i = self.clips.iter().position(|c| c.id == clip)?;
        let a = &self.clips[i];
        let b = self.clips.get(i + 1)?;
        (b.start == a.end()).then_some((tr, a, b))
    }
}

/// The two clips of a transition that is running, and how far along it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTransition<'a> {
    /// The incoming clip (the layer's own clip is the outgoing one).
    pub incoming: &'a Clip,
    /// The moment of the incoming clip's media to show (can be before its start: the media is held at its first picture).
    pub incoming_source_time: Micros,
    /// How far along the transition is, 0 to 1, in a straight line (the easing is applied by whoever draws).
    pub progress: f32,
    pub preset: &'a str,
    pub easing: Easing,
}

/// What is visible at one moment: a clip, and which moment of its media to show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer<'a> {
    pub track: &'a Track,
    pub clip: &'a Clip,
    pub source_time: Micros,
    /// Set while a transition from this clip to the next one is running.
    pub transition: Option<LayerTransition<'a>>,
}

/// A flag on the timeline ruler.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub time: Micros,
    pub color: Color,
    #[serde(default)]
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub id: TimelineId,
    pub name: String,
    /// Bottom layer first: the main track, then overlays and text above it. Audio tracks are mixed, not drawn.
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub markers: Vec<Marker>,
}

impl Timeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TimelineId::new(),
            name: name.into(),
            tracks: vec![Track::main()],
            markers: Vec::new(),
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

    /// The track that holds a clip.
    pub fn track_of(&self, clip: ClipId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.clip(clip).is_some())
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.tracks.iter().find_map(|t| t.clip(id))
    }

    /// Length of the timeline: where the last clip ends.
    pub fn duration(&self) -> Micros {
        self.tracks.iter().map(Track::end).max().unwrap_or(0)
    }

    /// Everything drawn at time `t`, bottom layer first. This is the one question both the player
    /// and the exporter ask ("what does frame `t` consist of?").
    pub fn layers_at(&self, t: Micros) -> Vec<Layer<'_>> {
        self.tracks
            .iter()
            .filter(|track| track.kind.is_visual() && !track.hidden)
            .filter_map(|track| {
                if let Some(layer) = Self::transition_layer(track, t) {
                    return Some(layer);
                }
                track.clip_at(t).map(|clip| Layer {
                    track,
                    clip,
                    source_time: clip.source_time(t),
                    transition: None,
                })
            })
            .collect()
    }

    /// The layer of a track at `t` when `t` is inside one of its transitions.
    fn transition_layer(track: &Track, t: Micros) -> Option<Layer<'_>> {
        for tr in &track.transitions {
            let Some((tr, a, b)) = track.transition_after(tr.after) else {
                continue;
            };
            let d = tr.effective(a, b);
            if d <= 0 {
                continue;
            }
            let (from, to) = (a.end() - d / 2, a.end() + d - d / 2);
            if t >= from && t < to {
                return Some(Layer {
                    track,
                    clip: a,
                    source_time: a.source_time(t),
                    transition: Some(LayerTransition {
                        incoming: b,
                        incoming_source_time: b.source_time(t).max(0),
                        progress: (t - from) as f32 / d as f32,
                        preset: &tr.preset,
                        easing: tr.easing,
                    }),
                });
            }
        }
        None
    }

    /// A free name such as "Overlay 2" for a new track.
    pub fn next_track_name(&self, kind: TrackKind) -> String {
        let n = self.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        format!("{} {n}", kind.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::TextContent;

    #[test]
    fn a_new_clip_changes_nothing_about_the_picture() {
        let c = Clip::media_clip(MediaId::new(), 0, 0, 1_000_000);
        assert!(c.transform.is_identity());
        assert_eq!(c.opacity.value_at(0), 1.0);
        assert_eq!(c.blend, BlendMode::Normal);
        assert!(c.crop.is_none());
        assert_eq!(c.volume.value_at(0), 1.0);
    }

    #[test]
    fn layers_come_bottom_first_and_skip_hidden_and_audio_tracks() {
        let mut tl = Timeline::new("t");
        let mut overlay = Track::new(TrackKind::Overlay, "o");
        overlay
            .clips
            .push(Clip::text_clip(TextContent::new("hi"), 0, 1_000_000));
        let mut hidden = Track::new(TrackKind::Text, "h");
        hidden.hidden = true;
        hidden
            .clips
            .push(Clip::text_clip(TextContent::new("no"), 0, 1_000_000));
        let mut audio = Track::new(TrackKind::Audio, "a");
        audio
            .clips
            .push(Clip::media_clip(MediaId::new(), 0, 0, 1_000_000));
        tl.main_track_mut()
            .clips
            .push(Clip::media_clip(MediaId::new(), 0, 0, 1_000_000));
        tl.tracks.extend([overlay, hidden, audio]);
        let layers = tl.layers_at(500_000);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].track.kind, TrackKind::Main);
        assert_eq!(layers[1].track.kind, TrackKind::Overlay);
        assert!(tl.layers_at(2_000_000).is_empty());
    }

    #[test]
    fn finds_the_track_of_a_clip_and_names_new_tracks() {
        let mut tl = Timeline::new("t");
        let c = Clip::media_clip(MediaId::new(), 0, 0, 10);
        let id = c.id;
        tl.main_track_mut().clips.push(c);
        assert_eq!(tl.track_of(id).unwrap().kind, TrackKind::Main);
        assert!(tl.clip(id).is_some());
        assert_eq!(tl.next_track_name(TrackKind::Overlay), "Overlay 1");
        assert_eq!(tl.next_track_name(TrackKind::Main), "Main 2");
    }
}
