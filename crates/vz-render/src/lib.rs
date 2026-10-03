//! Draws what a timeline looks like at one moment. The player and the exporter both call this,
//! so what you see is what you export.
//!
//! Phase 2 draws on the CPU, spread over all processor cores: the background, then every layer from the
//! bottom up with its position, size, turn, crop, opacity and blend mode. The GPU compositor with
//! effects and transitions replaces the inside of [`Renderer::render`] in later phases; the question
//! it answers ("what does frame `t` look like?") stays the same.

pub mod blend;
pub mod composite;

use composite::{Image, Placement};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use vz_core::{Background, Clip, Content, MediaKind, Micros, Project};
use vz_media::{FrameSource, MaxSize, VideoFrame};
use vz_text::{TextEngine, TextImage};
use vz_vector::{ShapeCache, ShapeImage};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-render";

/// Draws frames. Keeps video files open between frames, because opening one is slow.
#[derive(Default)]
pub struct Renderer {
    /// Open files with the picture size they were opened for.
    sources: HashMap<PathBuf, (MaxSize, FrameSource)>,
    /// Files that could not be opened or decoded, so we do not retry (and log) on every frame.
    broken: HashMap<PathBuf, String>,
    text: TextEngine,
    shapes: ShapeCache,
}

/// What a layer draws.
enum Source {
    Frame(Arc<VideoFrame>),
    Text(Arc<TextImage>),
    Shape(Arc<ShapeImage>),
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
        let (w, h) = size;
        let mut canvas = VideoFrame::black(w, h);
        canvas.pts = t;
        // canvas pixels (what transforms are measured in) to output pixels
        let to_output = w as f32 / project.canvas.width.max(1) as f32;

        // pictures first, so a blurred background can use the lowest one
        let mut drawn: Vec<(Source, Placement, &Clip, f32)> = Vec::new();
        let text_unit = vz_text::unit(project.canvas.width, project.canvas.height);
        for layer in project.timeline().layers_at(t) {
            let clip = layer.clip;
            let local = clip.local_time(t);
            let transform = clip.transform.at(local);
            let largest = transform.scale_x.abs().max(transform.scale_y.abs());
            let bucket = decode_bucket(largest);
            let (source, display) = match &clip.content {
                Content::Media(media) => {
                    let Some(asset) = project.asset(*media) else {
                        continue;
                    };
                    if asset.kind == MediaKind::Audio {
                        continue;
                    }
                    let Some(frame) = self.picture(&asset.path, layer.source_time, (w, h), bucket)
                    else {
                        continue;
                    };
                    let display = (
                        frame.width as f32 / bucket as f32,
                        frame.height as f32 / bucket as f32,
                    );
                    (Source::Frame(frame), display)
                }
                Content::Text(text) => {
                    if text.text.trim().is_empty() {
                        continue;
                    }
                    let image = self
                        .text
                        .render(text, text_unit * to_output * bucket as f32);
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
            };
            let placement = Placement {
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
            let opacity = clip.opacity.value_at(local);
            drawn.push((source, placement, clip, opacity));
        }

        if let Background::Color(c) = project.background {
            if c != vz_core::BLACK {
                for px in canvas.rgba.as_chunks_mut::<4>().0 {
                    px.copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        } else if let (Background::Blur(amount), Some(frame)) = (
            project.background,
            drawn.iter().find_map(|(s, ..)| match s {
                Source::Frame(f) => Some(f),
                Source::Text(_) | Source::Shape(_) => None,
            }),
        ) {
            blurred_cover(&mut canvas, frame, amount);
        }

        for (source, placement, clip, opacity) in &drawn {
            match source {
                Source::Frame(frame) if is_plain(placement, clip, *opacity, &canvas, frame) => {
                    paste_centered(&mut canvas, frame);
                }
                _ => composite::draw(
                    &mut canvas,
                    source.image(),
                    placement,
                    clip.crop.clamped(),
                    *opacity,
                    clip.blend,
                ),
            }
        }
        Arc::new(canvas)
    }

    /// The picture of a media file at file time `at`, decoded for an output of `size`.
    fn picture(
        &mut self,
        path: &PathBuf,
        at: Micros,
        size: (u32, u32),
        bucket: u32,
    ) -> Option<Arc<VideoFrame>> {
        if self.broken.contains_key(path) {
            return None;
        }
        let wanted = MaxSize::fill_box(size.0 * bucket, size.1 * bucket);
        // (re)open when new, or when the picture size we need has changed
        if !matches!(self.sources.get(path), Some((open_for, _)) if *open_for == wanted) {
            match FrameSource::open(path, wanted) {
                Ok(source) => {
                    self.sources.insert(path.clone(), (wanted, source));
                }
                Err(e) => {
                    tracing::warn!("cannot open {}: {e}", path.display());
                    self.broken.insert(path.clone(), e.to_string());
                    return None;
                }
            }
        }
        let (_, source) = self.sources.get_mut(path)?;
        match source.frame_at(at) {
            Ok(frame) => frame,
            Err(e) => {
                tracing::warn!("cannot decode {}: {e}", path.display());
                self.broken.insert(path.clone(), e.to_string());
                None
            }
        }
    }
}

/// A layer that is only fitted into the canvas (no move, size, turn, crop, fade or blend) can be copied directly.
fn is_plain(
    p: &Placement,
    clip: &Clip,
    opacity: f32,
    canvas: &VideoFrame,
    frame: &VideoFrame,
) -> bool {
    clip.transform.is_identity()
        && clip.blend == vz_core::BlendMode::Normal
        && clip.crop.is_none()
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
mod tests_layers;
