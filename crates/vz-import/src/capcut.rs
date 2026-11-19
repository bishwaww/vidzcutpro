//! CapCut drafts (`draft_content.json`) → a vidzcut project.
//!
//! A draft is plain JSON: tracks of segments, each segment pointing at a "material" (a video, a sound, a text, a
//! shape or a compound clip). Times are microseconds, like in vidzcut. What carries over: cuts and trims, speed,
//! volume, position / size / turn / flip / opacity, texts (font, colours per word, outline, shadow, box), rectangles
//! and compound clips. What does not (CapCut's own effects, filters, transitions, animations, keyframes, stickers)
//! is counted and reported in the notes so nobody is surprised.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use vz_anim::Animatable;
use vz_core::{
    Canvas, Clip, Color, Content, FrameRate, MaskDef, MaskShape, MediaAsset, MediaId, MediaKind,
    Micros, Project, ShapeContent, ShapeKind, StyleSpan, TextAlign, TextBackground, TextContent,
    TextShadow, TextStroke, Timeline, Track, TrackKind,
};

/// Reference pixels (short canvas side = 1080) of LINE HEIGHT per CapCut font-size unit: CapCut sizes text so that the
/// font's ascent plus descent equals the size. Found by comparing a draft's cover picture
/// with what vidzcut draws.
const TEXT_REF_PER_UNIT: f32 = 6.2;
/// The line height of a font that cannot be read, as a multiple of its size (most fonts are near this).
const DEFAULT_LINE_RATIO: f32 = 1.2;
/// The same for the width and height of a CapCut shape.
const SHAPE_REF_PER_UNIT: f32 = 1.5;
/// How deep compound clips inside compound clips are followed.
const MAX_DEPTH: u8 = 4;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("The project file could not be read ({0}).")]
    Read(String),
    #[error(
        "This project file is locked (encrypted) by CapCut, so no other program can read it. Export the video and the captions from CapCut instead."
    )]
    Locked,
    #[error("This does not look like a CapCut project ({0}).")]
    Format(String),
}

/// What an import gives back.
pub struct Imported {
    pub project: Project,
    /// Plain sentences about what was left out or could not be found.
    pub notes: Vec<String>,
    /// How many media files the draft names that are no longer on this computer.
    pub missing: usize,
}

/// A CapCut project found on this computer.
#[derive(Clone, Debug)]
pub struct CapCutProject {
    pub name: String,
    pub folder: PathBuf,
    pub modified: SystemTime,
    /// CapCut's own preview picture of the project, when there is one.
    pub cover: Option<PathBuf>,
}

/// Where CapCut keeps its projects, if it is installed for this user.
pub fn capcut_projects_dir() -> Option<PathBuf> {
    let tail = ["CapCut", "User Data", "Projects", "com.lveditor.draft"];
    let bases: Vec<PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|p| vec![PathBuf::from(p)])
            .unwrap_or_default()
    } else {
        std::env::var_os("HOME")
            .map(|h| vec![PathBuf::from(h).join("Movies")])
            .unwrap_or_default()
    };
    bases
        .into_iter()
        .map(|b| tail.iter().fold(b, |p, part| p.join(part)))
        .find(|p| p.is_dir())
}

/// The CapCut projects in CapCut's folder, newest first.
pub fn find_capcut_projects() -> Vec<CapCutProject> {
    let Some(dir) = capcut_projects_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<CapCutProject> = entries
        .flatten()
        .filter_map(|e| {
            let folder = e.path();
            let file = folder.join("draft_content.json");
            let modified = std::fs::metadata(&file).ok()?.modified().ok()?;
            let cover = folder.join("draft_cover.jpg");
            Some(CapCutProject {
                name: e.file_name().to_string_lossy().into_owned(),
                cover: cover.is_file().then_some(cover),
                folder,
                modified,
            })
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.modified));
    out
}

/// Read a CapCut project: `path` is the project's folder or its `draft_content.json`.
pub fn import_capcut(path: &Path) -> Result<Imported, ImportError> {
    let file = if path.is_dir() {
        path.join("draft_content.json")
    } else {
        path.to_path_buf()
    };
    let bytes = std::fs::read(&file).map_err(|e| ImportError::Read(e.to_string()))?;
    let text = String::from_utf8_lossy(&bytes);
    let draft: Value = match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(e) => {
            return Err(if bytes.first().is_some_and(|b| *b == b'{') {
                ImportError::Format(e.to_string())
            } else {
                ImportError::Locked
            });
        }
    };
    if !draft["tracks"].is_array() || !draft["materials"].is_object() {
        return Err(ImportError::Format("no tracks or materials".into()));
    }
    let name = path
        .parent()
        .filter(|_| !path.is_dir())
        .unwrap_or(path)
        .file_name()
        .map_or_else(
            || "CapCut project".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
    Ok(convert(&draft, &name))
}

/// Build the vidzcut project for an already parsed draft.
pub fn convert(draft: &Value, name: &str) -> Imported {
    let width = draft["canvas_config"]["width"].as_u64().unwrap_or(1920) as u32;
    let height = draft["canvas_config"]["height"].as_u64().unwrap_or(1080) as u32;
    let mut project = Project::new(name);
    project.canvas = Canvas {
        width: width.max(16),
        height: height.max(16),
    };
    project.frame_rate = nearest_rate(draft["fps"].as_f64().unwrap_or(30.0));
    project.auto_settings = false;
    let mut conv = Converter {
        canvas: (project.canvas.width as f32, project.canvas.height as f32),
        rate: project.frame_rate,
        project,
        assets: HashMap::new(),
        nested: Vec::new(),
        fonts: HashMap::new(),
        ratios: HashMap::new(),
        engine: None,
        unsupported: BTreeMap::new(),
        missing: 0,
        missing_fonts: Vec::new(),
    };
    let main = conv.timeline(draft, "Timeline 01", 0);
    let mut project = conv.project;
    project.active = Some(main.id);
    project.timelines = std::iter::once(main).chain(conv.nested).collect();

    let mut notes = Vec::new();
    for (what, count) in &conv.unsupported {
        notes.push(format!(
            "{count} {what} from CapCut were not brought over (vidzcut has nothing like them yet)."
        ));
    }
    if conv.missing > 0 {
        notes.push(format!(
            "{} media file(s) were not found on this computer. They show as missing: right-click them in the Media tab to find them again.",
            conv.missing
        ));
    }
    if !conv.missing_fonts.is_empty() {
        conv.missing_fonts.sort();
        conv.missing_fonts.dedup();
        notes.push(format!(
            "Fonts used by the texts: {}. If one of them is not installed on this computer, vidzcut picks another font; choose the font you want in the Text tab.",
            conv.missing_fonts.join(", ")
        ));
    }
    Imported {
        project,
        notes,
        missing: conv.missing,
    }
}

/// A CapCut mask as a vidzcut mask (`None` for kinds vidzcut does not have).
fn mask_of(mask: &Value) -> Option<MaskDef> {
    let shape = match text(mask, "resource_type") {
        "circle" => MaskShape::Circle,
        "rectangle" => MaskShape::Rectangle,
        "heart" => MaskShape::Heart,
        "star" => MaskShape::Star,
        "mirror" => MaskShape::Mirror,
        "linear" | "line" => MaskShape::Linear,
        _ => return None,
    };
    let config = &mask["config"];
    // CapCut sizes the feather and the rounded corners 0 to 100 in the program but may store them as 0 to 1
    let share = |key: &str| {
        let v = num(config, key, 0.0) as f32;
        (if v > 1.0 { v / 100.0 } else { v }).clamp(0.0, 1.0)
    };
    let mut def = MaskDef::new(shape);
    def.width = (num(config, "width", 0.6) as f32).clamp(0.01, 4.0);
    def.height = (num(config, "height", 0.6) as f32).clamp(0.01, 4.0);
    // CapCut centres from the middle in half pictures, upwards
    def.x = num(config, "centerX", 0.0) as f32 / 2.0;
    def.y = -(num(config, "centerY", 0.0) as f32) / 2.0;
    def.rotation = num(config, "rotation", 0.0) as f32;
    def.feather = share("feather");
    def.corner = share("roundCorner");
    def.invert = config["invert"].as_bool().unwrap_or(false);
    Some(def)
}

fn nearest_rate(fps: f64) -> FrameRate {
    [
        (FrameRate::FPS_23_976, 23.976),
        (FrameRate::FPS_24, 24.0),
        (FrameRate::FPS_25, 25.0),
        (FrameRate::FPS_29_97, 29.97),
        (FrameRate::FPS_30, 30.0),
        (FrameRate::FPS_50, 50.0),
        (FrameRate::FPS_59_94, 59.94),
        (FrameRate::FPS_60, 60.0),
    ]
    .into_iter()
    .min_by(|a, b| (a.1 - fps).abs().total_cmp(&(b.1 - fps).abs()))
    .map_or(FrameRate::FPS_30, |(rate, _)| rate)
}

// ---- reading the JSON ------------------------------------------------------------------------------------------------

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v[key].as_f64().unwrap_or(default)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map_or(&[], Vec::as_slice)
}

/// `#rrggbb` (or `#rrggbbaa`) with an extra opacity.
fn hex(s: &str, opacity: f32) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() < 6 || !h.is_ascii() {
        return None;
    }
    let part = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    let own = if h.len() >= 8 {
        f32::from(part(6)?) / 255.0
    } else {
        1.0
    };
    Some([
        part(0)?,
        part(2)?,
        part(4)?,
        (own * opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    ])
}

/// `[r, g, b]` as 0 to 1 numbers.
fn rgb(v: &Value) -> Option<Color> {
    let a = v.as_array()?;
    let c = |i: usize| {
        a.get(i)?
            .as_f64()
            .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    Some([c(0)?, c(1)?, c(2)?, 255])
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Video,
    Audio,
    Text,
    Shape,
    Sticker,
    Draft,
    Animation,
    Mask,
}

/// Every material of a draft by its id.
struct Mats<'a> {
    items: HashMap<&'a str, (Kind, &'a Value)>,
}

impl<'a> Mats<'a> {
    fn new(materials: &'a Value) -> Self {
        let mut items = HashMap::new();
        for (key, kind) in [
            ("videos", Kind::Video),
            ("audios", Kind::Audio),
            ("texts", Kind::Text),
            ("shapes", Kind::Shape),
            ("stickers", Kind::Sticker),
            ("drafts", Kind::Draft),
            ("material_animations", Kind::Animation),
            ("common_mask", Kind::Mask),
        ] {
            for m in list(materials, key) {
                if let Some(id) = m["id"].as_str() {
                    items.insert(id, (kind, m));
                }
            }
        }
        Self { items }
    }

    fn get(&self, id: &str) -> Option<(Kind, &'a Value)> {
        self.items.get(id).copied()
    }
}

// ---- the conversion ----------------------------------------------------------------------------------------------------

struct Converter {
    canvas: (f32, f32),
    rate: FrameRate,
    project: Project,
    /// File path → the asset made for it, so a file used ten times is listed once.
    assets: HashMap<String, MediaId>,
    /// Timelines of compound clips.
    nested: Vec<Timeline>,
    /// Font file path → family name (`None` when the file cannot be read).
    fonts: HashMap<String, Option<String>>,
    /// Font file path → line height ratio.
    ratios: HashMap<String, f32>,
    /// Made when the first font file is asked about (it loads the fonts of this computer).
    engine: Option<vz_text::TextEngine>,
    unsupported: BTreeMap<String, usize>,
    missing: usize,
    missing_fonts: Vec<String>,
}

impl Converter {
    fn skip(&mut self, what: &str) {
        *self.unsupported.entry(what.to_owned()).or_default() += 1;
    }

    fn timeline(&mut self, draft: &Value, name: &str, depth: u8) -> Timeline {
        let materials = &draft["materials"];
        let mats = Mats::new(materials);
        for (key, label) in [
            ("video_effects", "effects"),
            ("effects", "effects"),
            ("transitions", "transitions"),
            ("filters", "filters"),
        ] {
            let n = list(materials, key).len();
            if n > 0 {
                *self.unsupported.entry(label.to_owned()).or_default() += n;
            }
        }
        if list(materials, "speeds")
            .iter()
            .any(|s| !s["curve_speed"].is_null())
        {
            self.skip("speed curves");
        }
        let mut timeline = Timeline::new(name);
        let mut main_taken = false;
        let mut others: Vec<Track> = Vec::new();
        for track in list(draft, "tracks") {
            let kind = text(track, "type");
            let segments = list(track, "segments");
            if segments.is_empty() {
                continue;
            }
            let (mut pictures, mut words, mut audio) = (Vec::new(), Vec::new(), Vec::new());
            for seg in segments {
                let Some(clip) = self.segment(seg, kind, &mats, depth) else {
                    continue;
                };
                match &clip.content {
                    Content::Text(_) => words.push(clip),
                    Content::Media(id)
                        if self
                            .project
                            .asset(*id)
                            .is_some_and(|a| a.kind == MediaKind::Audio) =>
                    {
                        audio.push(clip);
                    }
                    _ => pictures.push(clip),
                }
            }
            let only_pictures = words.is_empty() && audio.is_empty();
            // CapCut's own captions are texts of the "subtitle" sort: they go to a caption track
            let is_caption_track = segments.iter().any(|s| {
                s["material_id"]
                    .as_str()
                    .and_then(|id| mats.get(id))
                    .is_some_and(|(k, m)| k == Kind::Text && text(m, "type") == "subtitle")
            });
            self.fit_in_a_row(&mut pictures);
            self.fit_in_a_row(&mut words);
            self.fit_in_a_row(&mut audio);
            if kind == "video" && !main_taken && only_pictures && self.is_gapless(&pictures) {
                let mut at = 0;
                for clip in &mut pictures {
                    clip.start = at;
                    at = clip.end();
                }
                timeline.tracks[0].clips = pictures;
                main_taken = true;
                continue;
            }
            for (track_kind, clips) in [
                (TrackKind::Overlay, pictures),
                (TrackKind::Text, words),
                (TrackKind::Audio, audio),
            ] {
                if clips.is_empty() {
                    continue;
                }
                let n = timeline
                    .tracks
                    .iter()
                    .filter(|t| t.kind == track_kind)
                    .count()
                    + others.iter().filter(|t| t.kind == track_kind).count()
                    + 1;
                let mut t = Track::new(track_kind, format!("{} {n}", track_kind.label()));
                t.clips = clips;
                t.captions = track_kind == TrackKind::Text && is_caption_track;
                others.push(t);
            }
        }
        // sound tracks go last
        let (sounds, others): (Vec<Track>, Vec<Track>) =
            others.into_iter().partition(|t| t.kind == TrackKind::Audio);
        timeline.tracks.extend(others);
        timeline.tracks.extend(sounds);
        timeline
    }

    /// Start and length of a clip, on whole pictures.
    fn snap_range(&self, start: Micros, length: Micros) -> (Micros, Micros) {
        let s = self.rate.snap(start.max(0));
        let e = self.rate.snap((start + length).max(0));
        let min = self.rate.frame_start(1);
        (s, (e - s).max(min))
    }

    /// Clips of one track must not overlap (rounding to whole pictures can make them touch by a hair).
    fn fit_in_a_row(&self, clips: &mut [Clip]) {
        clips.sort_by_key(|c| c.start);
        let min = self.rate.frame_start(1);
        let mut at = 0;
        for c in clips.iter_mut() {
            if c.start < at {
                let end = c.end();
                c.start = at;
                c.duration = (end - at).max(min);
            }
            at = c.end();
        }
    }

    /// Do the clips follow each other from the very start with no hole (what the main track needs)?
    fn is_gapless(&self, clips: &[Clip]) -> bool {
        let hair = self.rate.frame_start(1);
        let mut at = 0;
        for c in clips {
            if (c.start - at).abs() > hair {
                return false;
            }
            at = c.end();
        }
        !clips.is_empty()
    }

    fn segment(&mut self, seg: &Value, track_type: &str, mats: &Mats, depth: u8) -> Option<Clip> {
        if seg["visible"].as_bool() == Some(false) {
            self.skip("hidden clips");
            return None;
        }
        let target = &seg["target_timerange"];
        let (start, length) =
            self.snap_range(target["start"].as_i64()?, target["duration"].as_i64()?);
        let source_in = seg["source_timerange"]["start"]
            .as_i64()
            .unwrap_or(0)
            .max(0);
        let Some((kind, material)) = seg["material_id"].as_str().and_then(|id| mats.get(id)) else {
            self.skip(&format!("{track_type} items"));
            return None;
        };
        let mut clip = match kind {
            Kind::Video => self.video(seg, material, mats, start, length, source_in, depth)?,
            Kind::Audio => {
                let id = self.asset(material, MediaKind::Audio)?;
                Clip::media_clip(id, start, source_in, length)
            }
            Kind::Text => self.text_clip(material, seg, start, length)?,
            Kind::Shape => self.shape(material, start, length)?,
            Kind::Sticker | Kind::Draft | Kind::Animation | Kind::Mask => {
                self.skip("stickers");
                return None;
            }
        };
        self.place(&mut clip, seg, kind, mats);
        Some(clip)
    }

    /// Position, size, turn, flips, opacity, speed, sound level.
    fn place(&mut self, clip: &mut Clip, seg: &Value, kind: Kind, mats: &Mats) {
        let at = &seg["clip"];
        if at.is_object() {
            // CapCut counts position in half canvases from the middle, upwards; vidzcut in canvas pixels, downwards
            let (w, h) = self.canvas;
            clip.transform.x = Animatable::Static(num(&at["transform"], "x", 0.0) as f32 * w / 2.0);
            clip.transform.y =
                Animatable::Static(-(num(&at["transform"], "y", 0.0) as f32) * h / 2.0);
            let (sx, sy) = (
                num(&at["scale"], "x", 1.0) as f32,
                num(&at["scale"], "y", 1.0) as f32,
            );
            clip.transform.scale_x = Animatable::Static(sx);
            clip.transform.scale_y = Animatable::Static(sy);
            clip.transform.uniform_scale = (sx - sy).abs() < 1e-4;
            clip.transform.rotation = Animatable::Static(num(at, "rotation", 0.0) as f32);
            clip.transform.flip_h = at["flip"]["horizontal"].as_bool().unwrap_or(false);
            clip.transform.flip_v = at["flip"]["vertical"].as_bool().unwrap_or(false);
            clip.opacity = Animatable::Static(num(at, "alpha", 1.0).clamp(0.0, 1.0) as f32);
        }
        if matches!(kind, Kind::Video | Kind::Audio) {
            let speed = num(seg, "speed", 1.0) as f32;
            if (speed - 1.0).abs() > 1e-3 && speed > 0.0 {
                clip.speed = Animatable::Static(speed);
            }
            clip.reverse = seg["reverse"].as_bool().unwrap_or(false);
            clip.volume = Animatable::Static(num(seg, "volume", 1.0).max(0.0) as f32);
        }
        if list(seg, "common_keyframes").iter().any(|k| !k.is_null())
            || !list(seg, "keyframe_refs").is_empty()
        {
            self.skip("keyframes");
        }
        let animated = list(seg, "extra_material_refs")
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|r| mats.get(r))
            .any(|(k, m)| k == Kind::Animation && !list(m, "animations").is_empty());
        if animated {
            self.skip("animations");
        }
        let masks: Vec<&Value> = list(seg, "extra_material_refs")
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|r| mats.get(r))
            .filter(|(k, _)| *k == Kind::Mask)
            .map(|(_, m)| m)
            .collect();
        for mask in masks {
            match mask_of(mask) {
                Some(def) => clip.masks.push(def),
                None => self.skip("masks of a kind vidzcut does not have"),
            }
        }
    }

    /// The vidzcut asset for a draft material (the file is looked at once; a missing file still gets an entry).
    fn asset(&mut self, material: &Value, wanted: MediaKind) -> Option<MediaId> {
        let path = text(material, "path");
        if path.is_empty() {
            self.skip("clips without a file");
            return None;
        }
        if let Some(id) = self.assets.get(path) {
            return Some(*id);
        }
        let file = PathBuf::from(path);
        let asset = match vz_media::probe(&file) {
            Ok(a) => a,
            Err(_) => {
                self.missing += 1;
                MediaAsset {
                    id: MediaId::new(),
                    name: file
                        .file_name()
                        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned()),
                    path: file,
                    relative_path: None,
                    kind: wanted,
                    duration: if wanted == MediaKind::Image {
                        0
                    } else {
                        material["duration"].as_i64().unwrap_or(0)
                    },
                    width: material["width"].as_u64().unwrap_or(0) as u32,
                    height: material["height"].as_u64().unwrap_or(0) as u32,
                    frame_rate: None,
                    has_audio: material["has_audio"].as_bool().unwrap_or(false),
                    file_size: 0,
                }
            }
        };
        let id = asset.id;
        self.project.media.push(asset);
        self.assets.insert(path.to_owned(), id);
        Some(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn video(
        &mut self,
        seg: &Value,
        material: &Value,
        mats: &Mats,
        start: Micros,
        length: Micros,
        source_in: Micros,
        depth: u8,
    ) -> Option<Clip> {
        // a compound clip: a "video" without a file whose segment points at a nested draft
        let nested = list(seg, "extra_material_refs")
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|r| mats.get(r))
            .find(|(k, _)| *k == Kind::Draft)
            .map(|(_, m)| m);
        if let Some(draft) = nested {
            if depth >= MAX_DEPTH {
                self.skip("compound clips (nested too deep)");
                return None;
            }
            let name = [text(draft, "name"), text(material, "material_name")]
                .into_iter()
                .find(|n| !n.is_empty())
                .unwrap_or("Compound clip");
            let timeline = self.timeline(&draft["draft"], name, depth + 1);
            let id = timeline.id;
            self.nested.push(timeline);
            let mut clip = Clip::compound_clip(id, start, length);
            clip.source_in = source_in;
            return Some(clip);
        }
        let photo = text(material, "type") == "photo";
        let id = self.asset(
            material,
            if photo {
                MediaKind::Image
            } else {
                MediaKind::Video
            },
        )?;
        Some(Clip::media_clip(
            id,
            start,
            if photo { 0 } else { source_in },
            length,
        ))
    }

    fn shape(&mut self, material: &Value, start: Micros, length: Micros) -> Option<Clip> {
        let kind = match material["shape_type"].as_i64() {
            Some(4) => ShapeKind::Rectangle,
            _ => {
                self.skip("shapes other than rectangles");
                return None;
            }
        };
        let mut shape = ShapeContent::new(kind);
        if let [w, h, ..] = list(material, "shape_size") {
            shape.width = w.as_f64().unwrap_or(100.0) as f32 * SHAPE_REF_PER_UNIT;
            shape.height = h.as_f64().unwrap_or(100.0) as f32 * SHAPE_REF_PER_UNIT;
        }
        let solid = &material["fill_render_style"]["color"]["solid"];
        let opacity = (num(solid, "alpha", 1.0) * num(material, "global_alpha", 1.0)) as f32;
        shape.fill = Some(hex(text(solid, "color"), opacity).unwrap_or([204, 204, 204, 255]));
        shape.stroke = None;
        Some(Clip::shape_clip(shape, start, length))
    }

    /// Height of a line of the font at `path` as a multiple of its size (a typical value when the file cannot be read).
    fn line_ratio(&mut self, path: &str) -> f32 {
        if path.is_empty() {
            return DEFAULT_LINE_RATIO;
        }
        *self.ratios.entry(path.to_owned()).or_insert_with(|| {
            vz_text::line_ratio_of_file(Path::new(path)).unwrap_or(DEFAULT_LINE_RATIO)
        })
    }

    fn family(&mut self, path: &str) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        let Self { fonts, engine, .. } = self;
        fonts
            .entry(path.to_owned())
            .or_insert_with(|| {
                engine
                    .get_or_insert_with(vz_text::TextEngine::new)
                    .family_of_path(Path::new(path))
            })
            .clone()
    }

    fn text_clip(
        &mut self,
        material: &Value,
        seg: &Value,
        start: Micros,
        length: Micros,
    ) -> Option<Clip> {
        let raw = text(material, "content");
        let parsed: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
        let words = parsed["text"]
            .as_str()
            .map_or_else(|| raw.to_owned(), str::to_owned);
        if words.trim().is_empty() {
            return None;
        }
        let count = words.chars().count();
        let font_size = (num(material, "font_size", 15.0) as f32).max(0.5);
        let opacity = num(material, "text_alpha", 1.0) as f32;

        // the pieces CapCut gives its own look
        struct Piece {
            from: usize,
            to: usize,
            color: Color,
            font: Option<String>,
            path: String,
            size: f32,
        }
        let mut pieces = Vec::new();
        for style in list(&parsed, "styles") {
            let range = style["range"].as_array();
            let (Some(from), Some(to)) = (
                range.and_then(|r| r.first()).and_then(Value::as_u64),
                range.and_then(|r| r.get(1)).and_then(Value::as_u64),
            ) else {
                continue;
            };
            let color = rgb(&style["fill"]["content"]["solid"]["color"])
                .or_else(|| hex(text(material, "text_color"), 1.0))
                .unwrap_or([255, 255, 255, 255]);
            let path = text(&style["font"], "path").to_owned();
            let font = self.family(&path);
            pieces.push(Piece {
                from: from as usize,
                to: to as usize,
                color,
                font,
                path,
                size: num(style, "size", f64::from(font_size)) as f32,
            });
        }
        // the look most of the text has becomes the text's own; the rest are spans
        let most = |pick: &dyn Fn(&Piece) -> Option<String>| {
            let mut votes: Vec<(String, usize)> = Vec::new();
            for p in &pieces {
                if let Some(key) = pick(p) {
                    match votes.iter_mut().find(|v| v.0 == key) {
                        Some(v) => v.1 += p.to.saturating_sub(p.from),
                        None => votes.push((key, p.to.saturating_sub(p.from))),
                    }
                }
            }
            votes.into_iter().max_by_key(|v| v.1).map(|v| v.0)
        };
        let base_color = most(&|p| Some(format!("{:?}", p.color)))
            .and_then(|k| {
                pieces
                    .iter()
                    .map(|p| p.color)
                    .find(|c| format!("{c:?}") == k)
            })
            .or_else(|| hex(text(material, "text_color"), 1.0))
            .unwrap_or([255, 255, 255, 255]);
        let base_font = self
            .family(text(material, "font_path"))
            .or_else(|| most(&|p| p.font.clone()));
        // sizes: CapCut's size is the height of a line, so a font with a short line shows bigger letters
        let base_path = Some(text(material, "font_path"))
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .or_else(|| most(&|p| Some(p.path.clone()).filter(|p| !p.is_empty())))
            .unwrap_or_default();
        let base_ratio = self.line_ratio(&base_path);
        let size = font_size * TEXT_REF_PER_UNIT / base_ratio;

        let mut content = TextContent::new(words);
        let style = &mut content.style;
        style.size = size;
        style.color = [
            base_color[0],
            base_color[1],
            base_color[2],
            (f32::from(base_color[3]) * opacity).round() as u8,
        ];
        style.font = base_font.clone().unwrap_or_default();
        if let Some(f) = &base_font {
            self.missing_fonts.push(f.clone());
        }
        style.bold = num(material, "bold_width", 0.0) > 0.0;
        style.italic = num(material, "italic_degree", 0.0) != 0.0;
        style.underline = material["underline"].as_bool().unwrap_or(false);
        style.align = match material["alignment"].as_i64() {
            Some(0) => TextAlign::Left,
            Some(2) => TextAlign::Right,
            _ => TextAlign::Center,
        };
        style.line_spacing = 1.0 + num(material, "line_spacing", 0.0) as f32;
        let (cw, ch) = self.canvas;
        let to_ref = 1080.0 / cw.min(ch);
        let width_share = num(material, "line_max_width", 0.0) as f32;
        if width_share > 0.0 && width_share <= 1.0 {
            let scale = (num(&seg["clip"]["scale"], "x", 1.0) as f32)
                .abs()
                .max(0.05);
            style.box_width = Some(width_share * cw * to_ref / scale);
        }
        let border = hex(
            text(material, "border_color"),
            num(material, "border_alpha", 1.0) as f32,
        );
        if let Some(color) = border.filter(|_| num(material, "border_width", 0.0) > 0.0) {
            style.stroke = Some(TextStroke {
                color,
                width: num(material, "border_width", 0.0) as f32 * size,
            });
        }
        if material["has_shadow"].as_bool() == Some(true) {
            let distance = num(material, "shadow_distance", 5.0) as f32;
            let point = &material["shadow_point"];
            style.shadow = Some(TextShadow {
                color: hex(
                    text(material, "shadow_color"),
                    num(material, "shadow_alpha", 0.9) as f32,
                )
                .unwrap_or([0, 0, 0, 200]),
                dx: num(point, "x", 0.6) as f32 * distance,
                dy: -(num(point, "y", -0.6) as f32) * distance,
                blur: num(material, "shadow_smoothing", 0.45) as f32 * distance * 2.0,
            });
        }
        if let Some(color) = hex(
            text(material, "background_color"),
            num(material, "background_alpha", 1.0) as f32,
        ) {
            style.background = Some(TextBackground {
                color,
                padding: num(material, "background_width", 0.14) as f32 * size,
                radius: num(material, "background_round_radius", 0.0) as f32 * size,
            });
        }
        if num(material, "letter_spacing", 0.0) != 0.0 {
            self.skip("letter spacings");
        }
        for p in &pieces {
            let mut span = StyleSpan {
                start: p.from.min(count),
                end: p.to.min(count),
                ..StyleSpan::default()
            };
            let mut differs = false;
            if p.color != base_color {
                span.color = Some([
                    p.color[0],
                    p.color[1],
                    p.color[2],
                    (f32::from(p.color[3]) * opacity).round() as u8,
                ]);
                differs = true;
            }
            if p.font.is_some() && p.font != base_font {
                span.font = p.font.clone();
                if let Some(f) = &p.font {
                    self.missing_fonts.push(f.clone());
                }
                differs = true;
            }
            let piece_path = if p.path.is_empty() {
                &base_path
            } else {
                &p.path
            };
            let piece_size = p.size * TEXT_REF_PER_UNIT / self.line_ratio(piece_path);
            if (piece_size / size - 1.0).abs() > 0.01 {
                span.size = Some(piece_size / size);
                differs = true;
            }
            if differs && span.start < span.end {
                content.spans.push(span);
            }
        }
        Some(Clip::text_clip(content, start, length))
    }
}

#[cfg(test)]
mod tests;
