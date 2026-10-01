//! Draws what a timeline looks like at one moment. The player and the exporter both call this,
//! so what you see is what you export.
//!
//! Phase 1 draws on the CPU (video and still images, fitted into the canvas with black bars).
//! The GPU compositor with effects, transitions and blend modes replaces the inside of
//! [`Renderer::render`] in later phases; the question it answers stays the same.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use vz_core::{MediaKind, Micros, Project};
use vz_media::{FrameSource, MaxSize, VideoFrame};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-render";

/// Draws frames. Keeps video files open between frames, because opening one is slow.
#[derive(Default)]
pub struct Renderer {
    sources: HashMap<PathBuf, (MaxSize, FrameSource)>,
    /// Files that could not be opened or decoded, so we do not retry (and log) on every frame.
    broken: HashMap<PathBuf, String>,
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
        let wanted = MaxSize::fill_box(w, h);

        for layer in project.timeline().layers_at(t) {
            let Some(asset) = project.asset(layer.clip.media) else {
                continue;
            };
            if asset.kind == MediaKind::Audio {
                continue;
            }
            let path = &asset.path;
            if self.broken.contains_key(path) {
                continue;
            }
            // (re)open when new, or when the picture size we need has changed
            if !matches!(self.sources.get(path), Some((size, _)) if *size == wanted) {
                match FrameSource::open(path, wanted) {
                    Ok(source) => {
                        self.sources.insert(path.clone(), (wanted, source));
                    }
                    Err(e) => {
                        tracing::warn!("cannot open {}: {e}", path.display());
                        self.broken.insert(path.clone(), e.to_string());
                        continue;
                    }
                }
            }
            let Some((_, source)) = self.sources.get_mut(path) else {
                continue;
            };
            match source.frame_at(layer.source_time) {
                Ok(Some(frame)) => paste_centered(&mut canvas, &frame),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("cannot decode {}: {e}", path.display());
                    self.broken.insert(path.clone(), e.to_string());
                }
            }
        }
        Arc::new(canvas)
    }
}

/// Copy `src` into the middle of `dst` (the picture is already the right size to fit; the rest stays black).
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
