//! Draws what a timeline looks like at one moment. The player and the exporter both call this,
//! so what you see is what you export.
//!
//! Phase 2 draws on the CPU, spread over all processor cores: the background, then every layer from the
//! bottom up with its position, size, turn, crop, opacity and blend mode. The GPU compositor with
//! effects and transitions replaces the inside of [`Renderer::render`] in later phases; the question
//! it answers ("what does frame `t` look like?") stays the same.

pub mod blend;
pub mod color;
pub mod composite;
pub mod effects;
pub mod keying;
pub mod lut;
pub mod scopes;
pub mod transition;

use composite::{Image, Placement};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use vz_core::{
    Background, Clip, Content, MediaKind, Micros, Project, Timeline, TimelineId, TrackId,
};
use vz_media::{FrameSource, MaxSize, VideoFrame};
use vz_text::{TextEngine, TextImage};
use vz_vector::{ShapeCache, ShapeImage};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-render";

/// Which reader of a file: the track and whether it is the arriving side of a transition.
type ReaderSlot = (TrackId, bool);

struct OpenSource {
    size: MaxSize,
    source: FrameSource,
    last_used: u64,
}

/// How many readers may stay open when they are not needed any more.
const KEEP_OPEN: usize = 6;

/// Draws frames. Keeps video files open between frames, because opening one is slow.
#[derive(Default)]
pub struct Renderer {
    /// Open files with the picture size they were opened for.
    /// One reader per file and place on the timeline (track, and whether it is the arriving side of a transition):
    /// clips that follow each other on a track share one, so playing across a cut stays smooth, while two clips of
    /// the same file at the same time never fight over one reader.
    sources: HashMap<(PathBuf, ReaderSlot), OpenSource>,
    /// Counts rendered frames, to find readers that are no longer used.
    frame_counter: u64,
    /// Files that could not be opened or decoded, so we do not retry (and log) on every frame.
    broken: HashMap<PathBuf, String>,
    text: TextEngine,
    shapes: ShapeCache,
    luts: lut::LutCache,
}

/// One layer ready to be drawn.
struct Drawn<'a> {
    source: Source,
    placement: Placement,
    clip: &'a Clip,
    opacity: f32,
    crop: vz_core::Crop,
    /// While a transition runs, the clip that is arriving.
    incoming: Option<Box<Incoming<'a>>>,
}

struct Incoming<'a> {
    layer: Drawn<'a>,
    recipe: vz_presets::transition::Recipe,
    /// Progress with the easing already applied.
    progress: f32,
}

/// What a frame being drawn is about.
struct Frame<'a> {
    project: &'a Project,
    t: Micros,
    size: (u32, u32),
    to_output: f32,
    text_unit: f32,
    /// How many compound clips deep this frame is (0 for the timeline being shown).
    depth: u8,
}

/// Compound clips inside compound clips stop here, so a clip that (by mistake) contains itself cannot hang the program.
const MAX_DEPTH: u8 = 6;

/// The time and place of the frame being drawn, for the things that change from frame to frame.
fn seed_of(frame: &Frame<'_>) -> u32 {
    (frame.t / 1000) as u32
}

/// Colour settings, effects and masks of a clip, applied to a picture (in this order: key, colour, effects, masks).
fn process(
    picture: &mut VideoFrame,
    clip: &Clip,
    frame: &Frame<'_>,
    luts: &mut lut::LutCache,
    with_masks: bool,
) {
    let local = clip.local_time(frame.t);
    let seconds = local as f32 / 1_000_000.0;
    if let Some(key) = &clip.chroma {
        keying::apply_chroma(picture, key);
    }
    color::apply(picture, &clip.adjust, seed_of(frame), luts);
    for fx in clip.effects.iter().filter(|e| e.enabled) {
        if let Some(preset) = vz_presets::effects::find(&fx.preset) {
            effects::apply(preset.kind, &fx.params, picture, seconds, seed_of(frame));
        }
    }
    if with_masks {
        keying::apply_masks(picture, &clip.masks);
    }
}

/// Does a clip change its picture (beyond placing it)?
fn has_look(clip: &Clip) -> bool {
    clip.chroma.is_some()
        || !clip.adjust.is_identity()
        || clip.effects.iter().any(|e| e.enabled)
        || !clip.masks.is_empty()
}

/// An adjustment layer: everything drawn so far is changed (colour, effects), more or less, inside its masks.
fn draw_pass(canvas: &mut VideoFrame, d: &Drawn<'_>, frame: &Frame<'_>, luts: &mut lut::LutCache) {
    if d.opacity <= 0.0 || !has_look(d.clip) {
        return;
    }
    let mut changed = canvas.clone();
    process(&mut changed, d.clip, frame, luts, false);
    // how much of the change shows at each pixel: the masks, times the opacity
    let mut amount = VideoFrame::black(canvas.width, canvas.height);
    for p in amount.rgba.as_chunks_mut::<4>().0 {
        *p = [255, 255, 255, 255];
    }
    keying::apply_masks(&mut amount, &d.clip.masks);
    let opacity = d.opacity.clamp(0.0, 1.0);
    canvas
        .rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(changed.rgba.as_chunks::<4>().0)
        .zip(amount.rgba.as_chunks::<4>().0)
        .for_each(|((dst, new), m)| {
            let k = f32::from(m[3]) / 255.0 * opacity;
            if k <= 0.0 {
                return;
            }
            for i in 0..3 {
                dst[i] =
                    (f32::from(dst[i]) + (f32::from(new[i]) - f32::from(dst[i])) * k + 0.5) as u8;
            }
        });
}

/// Draw one layer onto the canvas.
fn draw_layer(canvas: &mut VideoFrame, d: &Drawn<'_>, frame: &Frame<'_>, luts: &mut lut::LutCache) {
    if matches!(d.source, Source::Pass) {
        draw_pass(canvas, d, frame, luts);
        return;
    }
    if d.crop.left + d.crop.right >= 1.0 || d.crop.top + d.crop.bottom >= 1.0 {
        return; // wiped away completely
    }
    match &d.source {
        Source::Frame(frame)
            if is_plain(&d.placement, d.clip, d.opacity, &d.crop, canvas, frame) =>
        {
            paste_centered(canvas, frame);
        }
        _ => composite::draw(
            canvas,
            d.source.image(),
            &d.placement,
            d.crop,
            d.opacity,
            d.clip.blend,
        ),
    }
}

/// A softened copy of a picture (a gaussian blur of `sigma` pixels). Works on a smaller copy for speed.
pub(crate) fn blurred(src: &Image<'_>, sigma: f32) -> VideoFrame {
    use image::imageops::{FilterType, blur, resize};
    let (w, h) = (src.width, src.height);
    let k = (sigma / 4.0).floor().max(1.0) as u32;
    let (sw, sh) = ((w / k).max(1), (h / k).max(1));
    let Some(img) = image::RgbaImage::from_raw(w, h, src.rgba.to_vec()) else {
        return VideoFrame {
            width: w,
            height: h,
            rgba: src.rgba.to_vec(),
            pts: 0,
        };
    };
    let small = resize(&img, sw, sh, FilterType::Triangle);
    let soft = blur(&small, (sigma / k as f32).max(0.3));
    let big = resize(&soft, w, h, FilterType::Triangle);
    VideoFrame {
        width: w,
        height: h,
        rgba: big.into_raw(),
        pts: 0,
    }
}

/// What a layer draws.
enum Source {
    Frame(Arc<VideoFrame>),
    Text(Arc<TextImage>),
    Shape(Arc<ShapeImage>),
    /// An adjustment layer: no picture of its own, it changes what is below.
    Pass,
}

impl Source {
    fn image(&self) -> Image<'_> {
        match self {
            Self::Frame(f) => Image::from(f.as_ref()),
            Self::Text(t) => Image {
                width: t.width,
                height: t.height,
                rgba: &t.rgba,
            },
            Self::Shape(s) => Image {
                width: s.width,
                height: s.height,
                rgba: &s.rgba,
            },
            Self::Pass => Image {
                width: 0,
                height: 0,
                rgba: &[],
            },
        }
    }
}

/// How much bigger than the canvas a picture is decoded, so a layer scaled up stays sharp.
/// Only three steps, so dragging a scale slider does not make files reopen on every change.
fn decode_bucket(largest_scale: f32) -> u32 {
    if largest_scale > 2.0 {
        4
    } else if largest_scale > 1.0 {
        2
    } else {
        1
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Why a file could not be drawn, if it could not.
    pub fn problem_with(&self, path: &PathBuf) -> Option<&str> {
        self.broken.get(path).map(String::as_str)
    }

    /// Forget a file's problem so it is tried again (after the user relinks or fixes it).
    pub fn retry(&mut self, path: &PathBuf) {
        self.broken.remove(path);
    }

    /// Render the picture of `project`'s timeline at time `t`, exactly `size` pixels (width, height), RGBA.
    pub fn render(&mut self, project: &Project, t: Micros, size: (u32, u32)) -> Arc<VideoFrame> {
        self.frame_counter += 1;
        self.drop_unused_readers();
        Arc::new(self.render_timeline(project, project.timeline(), t, size, None, 0))
    }

    /// A compound clip's picture: its timeline drawn twice, on black and on white, so the see-through parts can be
    /// told from the dark and the light ones. Straight RGBA, see-through where nothing is drawn.
    fn nested(
        &mut self,
        project: &Project,
        id: TimelineId,
        t: Micros,
        size: (u32, u32),
        depth: u8,
    ) -> Option<VideoFrame> {
        if depth > MAX_DEPTH {
            return None;
        }
        let timeline = project.timeline_by_id(id)?;
        let on_black = self.render_timeline(project, timeline, t, size, Some([0, 0, 0]), depth);
        let on_white =
            self.render_timeline(project, timeline, t, size, Some([255, 255, 255]), depth);
        Some(unmatte(&on_black, &on_white))
    }

    /// Draw `timeline` at time `t`. With `backdrop`, that colour is behind everything (compound clips use black and white).
    fn render_timeline(
        &mut self,
        project: &Project,
        timeline: &Timeline,
        t: Micros,
        size: (u32, u32),
        backdrop: Option<[u8; 3]>,
        depth: u8,
    ) -> VideoFrame {
        let (w, h) = size;
        let mut canvas = VideoFrame::black(w, h);
        canvas.pts = t;
        let frame = Frame {
            project,
            t,
            size,
            // canvas pixels (what transforms are measured in) to output pixels
            to_output: w as f32 / project.canvas.width.max(1) as f32,
            text_unit: vz_text::unit(project.canvas.width, project.canvas.height),
            depth,
        };

        // pictures first, so a blurred background can use the lowest one
        let mut drawn: Vec<Drawn> = Vec::new();
        for layer in timeline.layers_at(t) {
            let Some(mut d) = self.layer_for(
                &frame,
                layer.clip,
                layer.source_time,
                (layer.track.id, false),
            ) else {
                continue;
            };
            if let Some(tr) = layer.transition
                && let Some(preset) = vz_presets::transition::find(tr.preset)
                && let Some(incoming) = self.layer_for(
                    &frame,
                    tr.incoming,
                    tr.incoming_source_time,
                    (layer.track.id, true),
                )
            {
                d.incoming = Some(Box::new(Incoming {
                    layer: incoming,
                    recipe: preset.recipe,
                    progress: tr.easing.apply(tr.progress),
                }));
            }
            drawn.push(d);
        }

        if let Some(c) = backdrop {
            for px in canvas.rgba.as_chunks_mut::<4>().0 {
                px.copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        } else if let Background::Color(c) = project.background {
            if c != vz_core::BLACK {
                for px in canvas.rgba.as_chunks_mut::<4>().0 {
                    px.copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        } else if let (Background::Blur(amount), Some(frame)) = (
            project.background,
            drawn.iter().find_map(|d| match &d.source {
                Source::Frame(f) => Some(f),
                Source::Text(_) | Source::Shape(_) | Source::Pass => None,
            }),
        ) {
            blurred_cover(&mut canvas, frame, amount);
        }

        for d in &drawn {
            match &d.incoming {
                None => draw_layer(&mut canvas, d, &frame, &mut self.luts),
                Some(inc) => {
                    // both clips are drawn over what is below, then blended
                    let mut from = canvas.clone();
                    draw_layer(&mut from, d, &frame, &mut self.luts);
                    let mut to = canvas.clone();
                    draw_layer(&mut to, &inc.layer, &frame, &mut self.luts);
                    canvas = transition::blend(&from, &to, inc.progress, inc.recipe);
                }
            }
        }
        canvas
    }

    /// Close readers that have not been used for a while (a clip that was deleted or is far away on the timeline).
    fn drop_unused_readers(&mut self) {
        if self.sources.len() <= KEEP_OPEN {
            return;
        }
        let now = self.frame_counter;
        self.sources.retain(|_, open| now - open.last_used < 90);
    }

    /// Everything needed to draw one clip at the moment of `frame`: its picture, placement, opacity and crop
    /// (with its own keyframes and ready-made animations applied). `None` if there is nothing to draw.
    fn layer_for<'a>(
        &mut self,
        frame: &Frame<'_>,
        clip: &'a Clip,
        source_time: Micros,
        slot: ReaderSlot,
    ) -> Option<Drawn<'a>> {
        let project = frame.project;
        let (w, h) = frame.size;
        let to_output = frame.to_output;
        let text_unit = frame.text_unit;
        let local = clip.local_time(frame.t);
        let transform = clip.transform.at(local);
        let largest = transform.scale_x.abs().max(transform.scale_y.abs());
        let bucket = decode_bucket(largest);
        let (source, display) = match &clip.content {
            Content::Media(media) => {
                let asset = project.asset(*media)?;
                if asset.kind == MediaKind::Audio {
                    return None;
                }
                // a transition can ask for a moment just past the end of the file: hold the last picture
                let at = if asset.kind == MediaKind::Video && asset.duration > 1 {
                    source_time.clamp(0, asset.duration - 1)
                } else {
                    source_time.max(0)
                };
                let picture = self.picture(&asset.path, slot, at, (w, h), bucket)?;
                let display = (
                    picture.width as f32 / bucket as f32,
                    picture.height as f32 / bucket as f32,
                );
                (Source::Frame(picture), display)
            }
            Content::Text(text) => {
                if text.text.trim().is_empty() {
                    return None;
                }
                let image = self.text.render_at(
                    text,
                    text_unit * to_output * bucket as f32,
                    local,
                    clip.duration,
                );
                let display = (
                    image.width as f32 / bucket as f32,
                    image.height as f32 / bucket as f32,
                );
                (Source::Text(image), display)
            }
            Content::Shape(shape) => {
                let image = self
                    .shapes
                    .render(shape, text_unit * to_output * bucket as f32);
                let display = (
                    image.width as f32 / bucket as f32,
                    image.height as f32 / bucket as f32,
                );
                (Source::Shape(image), display)
            }
            Content::Adjustment => (Source::Pass, (0.0, 0.0)),
            Content::Compound(id) => {
                let picture = self.nested(project, *id, source_time, (w, h), frame.depth + 1)?;
                (Source::Frame(Arc::new(picture)), (w as f32, h as f32))
            }
        };
        let mut placement = Placement {
            center: (
                w as f32 / 2.0 + transform.x * to_output,
                h as f32 / 2.0 + transform.y * to_output,
            ),
            size: display,
            scale: (transform.scale_x, transform.scale_y),
            rotation: transform.rotation,
            flip_h: transform.flip_h,
            flip_v: transform.flip_v,
        };
        let mut opacity = clip.opacity.value_at(local);
        let mut crop = clip.crop.clamped();
        let mut source = source;
        // key, colour, effects and masks change the picture itself (the picture is copied first: the decoder keeps its own)
        if has_look(clip) && !matches!(source, Source::Pass) {
            let image = source.image();
            let mut picture = VideoFrame {
                width: image.width,
                height: image.height,
                rgba: image.rgba.to_vec(),
                pts: 0,
            };
            process(&mut picture, clip, frame, &mut self.luts, true);
            source = Source::Frame(Arc::new(picture));
        }
        // ready-made animations (in / out / combo) on top of the clip's own settings
        let anim = vz_presets::animation::state_at(clip, local);
        if !anim.is_neutral() {
            placement.center.0 += anim.dx * w as f32;
            placement.center.1 += anim.dy * h as f32;
            placement.scale.0 *= anim.scale_x;
            placement.scale.1 *= anim.scale_y;
            placement.rotation += anim.rotation;
            opacity *= anim.opacity;
            crop.left = crop.left.max(anim.hide_left);
            crop.right = crop.right.max(anim.hide_right);
            if anim.blur > 0.05 {
                let sigma = anim.blur * text_unit * to_output * bucket as f32;
                source = Source::Frame(Arc::new(blurred(&source.image(), sigma)));
            }
        }
        Some(Drawn {
            source,
            placement,
            clip,
            opacity,
            crop,
            incoming: None,
        })
    }

    /// The picture of a media file at file time `at`, decoded for an output of `size`.
    fn picture(
        &mut self,
        path: &PathBuf,
        slot: ReaderSlot,
        at: Micros,
        size: (u32, u32),
        bucket: u32,
    ) -> Option<Arc<VideoFrame>> {
        if self.broken.contains_key(path) {
            return None;
        }
        let wanted = MaxSize::fill_box(size.0 * bucket, size.1 * bucket);
        let key = (path.clone(), slot);
        // (re)open when new, or when the picture size we need has changed
        if !matches!(self.sources.get(&key), Some(open) if open.size == wanted) {
            match FrameSource::open(path, wanted) {
                Ok(source) => {
                    self.sources.insert(
                        key.clone(),
                        OpenSource {
                            size: wanted,
                            source,
                            last_used: self.frame_counter,
                        },
                    );
                }
                Err(e) => {
                    tracing::warn!("cannot open {}: {e}", path.display());
                    self.broken.insert(path.clone(), e.to_string());
                    return None;
                }
            }
        }
        let open = self.sources.get_mut(&key)?;
        open.last_used = self.frame_counter;
        match open.source.frame_at(at) {
            Ok(frame) => frame,
            Err(e) => {
                tracing::warn!("cannot decode {}: {e}", path.display());
                self.broken.insert(path.clone(), e.to_string());
                None
            }
        }
    }
}

/// Tell the picture and its see-through parts apart: `black` and `white` are the same picture drawn over black and
/// over white. Where they differ, the picture lets the backdrop through.
fn unmatte(black: &VideoFrame, white: &VideoFrame) -> VideoFrame {
    let mut out = VideoFrame {
        width: black.width,
        height: black.height,
        rgba: vec![0; black.rgba.len()],
        pts: black.pts,
    };
    for ((dst, b), w) in out
        .rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(black.rgba.as_chunks::<4>().0)
        .zip(white.rgba.as_chunks::<4>().0)
    {
        let gap = (i32::from(w[0]) - i32::from(b[0]) + i32::from(w[1]) - i32::from(b[1])
            + i32::from(w[2])
            - i32::from(b[2]))
            / 3;
        let alpha = (255 - gap).clamp(0, 255);
        if alpha == 0 {
            continue;
        }
        for i in 0..3 {
            dst[i] = (i32::from(b[i]) * 255 / alpha).clamp(0, 255) as u8;
        }
        dst[3] = alpha as u8;
    }
    out
}

/// A layer that is only fitted into the canvas (no move, size, turn, crop, fade or blend) can be copied directly.
fn is_plain(
    p: &Placement,
    clip: &Clip,
    opacity: f32,
    crop: &vz_core::Crop,
    canvas: &VideoFrame,
    frame: &VideoFrame,
) -> bool {
    // a keyed or masked picture has see-through parts, so it must be blended, not copied
    clip.chroma.is_none()
        && !clip.is_compound()
        && clip.masks.is_empty()
        && clip.transform.is_identity()
        && (p.center.0 - canvas.width as f32 / 2.0).abs() < 0.01
        && (p.center.1 - canvas.height as f32 / 2.0).abs() < 0.01
        && p.scale == (1.0, 1.0)
        && p.rotation == 0.0
        && clip.blend == vz_core::BlendMode::Normal
        && crop.is_none()
        && opacity >= 1.0
        && frame.width <= canvas.width
        && frame.height <= canvas.height
        && (p.size.0 - frame.width as f32).abs() < 0.5
        && (p.size.1 - frame.height as f32).abs() < 0.5
}

/// Copy `src` into the middle of `dst` (the picture is already the right size to fit; the rest stays as it was).
fn paste_centered(dst: &mut VideoFrame, src: &VideoFrame) {
    let (dw, dh, sw, sh) = (
        dst.width as usize,
        dst.height as usize,
        src.width as usize,
        src.height as usize,
    );
    let copy_w = sw.min(dw);
    let copy_h = sh.min(dh);
    let off_x = (dw - copy_w) / 2;
    let off_y = (dh - copy_h) / 2;
    let src_x = (sw - copy_w) / 2;
    let src_y = (sh - copy_h) / 2;
    for row in 0..copy_h {
        let s = ((src_y + row) * sw + src_x) * 4;
        let d = ((off_y + row) * dw + off_x) * 4;
        dst.rgba[d..d + copy_w * 4].copy_from_slice(&src.rgba[s..s + copy_w * 4]);
    }
}

/// Fill the canvas with a blurred, enlarged copy of `frame` (the "blur" background).
fn blurred_cover(canvas: &mut VideoFrame, frame: &VideoFrame, amount: f32) {
    use image::imageops::{FilterType, blur, resize};
    let (cw, ch) = (canvas.width, canvas.height);
    // work small: a heavy blur on a tiny picture is cheap, and enlarging it back keeps it smooth
    let (small_w, small_h) = ((cw / 8).max(8), (ch / 8).max(8));
    let Some(src) = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
    else {
        return;
    };
    // "cover": scale up until the picture fills the canvas, then cut the overhang
    let scale = (small_w as f32 / frame.width as f32).max(small_h as f32 / frame.height as f32);
    let (sw, sh) = (
        ((frame.width as f32 * scale).ceil() as u32).max(small_w),
        ((frame.height as f32 * scale).ceil() as u32).max(small_h),
    );
    let scaled = resize(&src, sw, sh, FilterType::Triangle);
    let cropped = image::imageops::crop_imm(
        &scaled,
        (sw - small_w) / 2,
        (sh - small_h) / 2,
        small_w,
        small_h,
    )
    .to_image();
    let blurred = blur(&cropped, 1.0 + amount.clamp(0.0, 1.0) * 6.0);
    let big = resize(&blurred, cw, ch, FilterType::Triangle);
    canvas.rgba.copy_from_slice(big.as_raw());
    for px in canvas.rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{AspectPreset, Document, MediaAsset, US_PER_SEC};
    use vz_media::testutil::{self, Pattern, Spec};

    fn doc_with(files: &[(&std::path::Path, i64)]) -> Document {
        let mut doc = Document::new(Project::new("render test"));
        for (path, _) in files {
            let asset = vz_media::probe(path).unwrap();
            let id = doc.import_media(asset).unwrap();
            doc.add_clip(id, usize::MAX).unwrap();
        }
        doc
    }

    fn pixel(f: &VideoFrame, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
    }

    fn solid(name: &str, rgb: u32, w: u32, h: u32) -> PathBuf {
        testutil::video(
            name,
            Spec {
                pattern: Pattern::Solid(rgb),
                width: w,
                height: h,
                audio_hz: None,
                seconds: 2,
                ..Spec::default()
            },
        )
    }

    #[test]
    fn empty_timeline_is_black() {
        let doc = Document::new(Project::new("empty"));
        let f = Renderer::new().render(&doc.project, 0, (320, 180));
        assert_eq!((f.width, f.height), (320, 180));
        assert_eq!(pixel(&f, 160, 90), [0, 0, 0, 255]);
    }

    #[test]
    fn shows_the_clip_that_is_playing_and_cuts_between_clips() {
        let red = solid("r_320x180.mp4", 0xFF0000, 320, 180);
        let blue = solid("b_320x180.mp4", 0x0000FF, 320, 180);
        let doc = doc_with(&[(&red, 2), (&blue, 2)]);
        let mut r = Renderer::new();
        let a = r.render(&doc.project, US_PER_SEC, (320, 180));
        let b = r.render(&doc.project, 3 * US_PER_SEC, (320, 180));
        let pa = pixel(&a, 160, 90);
        let pb = pixel(&b, 160, 90);
        assert!(
            pa[0] > 200 && pa[2] < 60,
            "first clip should be red, was {pa:?}"
        );
        assert!(
            pb[2] > 200 && pb[0] < 60,
            "second clip should be blue, was {pb:?}"
        );
        // past the end of the timeline: black
        assert_eq!(
            pixel(
                &r.render(&doc.project, 10 * US_PER_SEC, (320, 180)),
                160,
                90
            ),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn a_small_video_is_enlarged_to_fill_the_canvas() {
        let red = solid("r_160x90.mp4", 0xFF0000, 160, 90);
        let doc = doc_with(&[(&red, 2)]);
        let f = Renderer::new().render(&doc.project, 0, (640, 360));
        for (x, y) in [(2, 2), (637, 357), (320, 180)] {
            assert!(pixel(&f, x, y)[0] > 200, "pixel {x},{y} should be red");
        }
    }

    #[test]
    fn a_wide_video_on_a_tall_canvas_gets_black_bars() {
        let red = solid("r_320x180b.mp4", 0xFF0000, 320, 180);
        let mut doc = doc_with(&[(&red, 2)]);
        doc.set_aspect(AspectPreset::Tall9x16).unwrap();
        let f = Renderer::new().render(&doc.project, 0, (180, 320));
        assert_eq!(pixel(&f, 90, 5), [0, 0, 0, 255], "top bar");
        assert_eq!(pixel(&f, 90, 315), [0, 0, 0, 255], "bottom bar");
        assert!(pixel(&f, 90, 160)[0] > 200, "picture in the middle");
    }

    #[test]
    fn trimmed_clips_show_the_right_part_of_the_file() {
        let chart = testutil::standard();
        let mut doc = doc_with(&[(&chart, 4)]);
        let id = doc.project.timeline().main_track().clips[0].id;
        doc.trim_left(id, 2 * US_PER_SEC, None).unwrap();
        let mut r = Renderer::new();
        let at_clip_start = r.render(&doc.project, 0, (320, 180));
        // timeline 0 now shows file time 2 s: same picture as rendering the untrimmed clip at 2 s
        let untrimmed = doc_with(&[(&chart, 4)]);
        let expected = Renderer::new().render(&untrimmed.project, 2 * US_PER_SEC, (320, 180));
        assert_eq!(at_clip_start.rgba, expected.rgba);
    }

    #[test]
    fn hidden_tracks_and_missing_files_do_not_crash() {
        let red = solid("r_320x180c.mp4", 0xFF0000, 320, 180);
        let mut doc = doc_with(&[(&red, 2)]);
        let asset = doc.project.media[0].clone();
        let mut r = Renderer::new();
        doc.project.timeline_mut().main_track_mut().hidden = true;
        assert_eq!(
            pixel(&r.render(&doc.project, 0, (64, 36)), 32, 18),
            [0, 0, 0, 255]
        );
        doc.project.timeline_mut().main_track_mut().hidden = false;

        let mut gone = doc.project.clone();
        gone.media[0] = MediaAsset {
            path: "/no/such/file.mp4".into(),
            ..asset
        };
        let f = r.render(&gone, 0, (64, 36));
        assert_eq!(pixel(&f, 32, 18), [0, 0, 0, 255]);
        assert!(r.problem_with(&"/no/such/file.mp4".into()).is_some());
    }
}

#[cfg(test)]
mod tests_captions;
#[cfg(test)]
mod tests_compound;
#[cfg(test)]
mod tests_layers;
