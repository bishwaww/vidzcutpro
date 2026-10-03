//! Shapes: draws a [`ShapeContent`] into a picture with transparent surroundings (anti-aliased),
//! the same way `vz-text` does for text, so the renderer treats both as ordinary layers.
//!
//! Drawing is done by tiny-skia (BSD-3-Clause).

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform,
};
use vz_core::{ShapeContent, ShapeKind};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-vector";

/// A finished shape picture: straight (not pre-multiplied) RGBA.
#[derive(Debug)]
pub struct ShapeImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The size of the shape itself, without the room kept around it for the outline. The picture is centred on it.
    pub content: (f32, f32),
}

/// The size of a shape in canvas pixels at `factor` canvas pixels per reference pixel.
pub fn measure(shape: &ShapeContent, factor: f32) -> (f32, f32) {
    (
        (shape.width * factor).max(1.0),
        (shape.height * factor).max(1.0),
    )
}

/// Draws shapes and remembers recent pictures.
#[derive(Default)]
pub struct ShapeCache {
    images: HashMap<String, Arc<ShapeImage>>,
}

impl ShapeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&mut self, shape: &ShapeContent, factor: f32) -> Arc<ShapeImage> {
        let key = format!("{shape:?}|{:08x}", factor.to_bits());
        if let Some(image) = self.images.get(&key) {
            return image.clone();
        }
        let image = Arc::new(draw(shape, factor));
        if self.images.len() > 48 {
            self.images.clear();
        }
        self.images.insert(key, image.clone());
        image
    }
}

fn polygon(points: &[(f32, f32)]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (first, rest) = points.split_first()?;
    pb.move_to(first.0, first.1);
    for p in rest {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    pb.finish()
}

/// A regular polygon (or star) inscribed in the box.
fn ring(w: f32, h: f32, corners: usize, inner: Option<f32>) -> Option<Path> {
    let (cx, cy) = (w / 2.0, h / 2.0);
    let steps = if inner.is_some() {
        corners * 2
    } else {
        corners
    };
    let points: Vec<(f32, f32)> = (0..steps)
        .map(|i| {
            let angle = -PI / 2.0 + i as f32 * 2.0 * PI / steps as f32;
            let k = match inner {
                Some(k) if i % 2 == 1 => k,
                _ => 1.0,
            };
            (cx + angle.cos() * cx * k, cy + angle.sin() * cy * k)
        })
        .collect();
    polygon(&points)
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let r = radius.clamp(0.0, w.min(h) / 2.0);
    if r <= 0.0 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    // corners as cubic curves (0.5523 is the usual circle approximation)
    let k = r * (1.0 - 0.552_285);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - k, y, x + w, y + k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + k, y + h, x, y + h - k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + k, x + k, y, x + r, y);
    pb.close();
    pb.finish()
}

fn heart(w: f32, h: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(w * 0.5, h * 0.95);
    pb.cubic_to(w * 0.05, h * 0.62, w * -0.05, h * 0.2, w * 0.25, h * 0.08);
    pb.cubic_to(w * 0.4, h * 0.02, w * 0.5, h * 0.15, w * 0.5, h * 0.25);
    pb.cubic_to(w * 0.5, h * 0.15, w * 0.6, h * 0.02, w * 0.75, h * 0.08);
    pb.cubic_to(w * 1.05, h * 0.2, w * 0.95, h * 0.62, w * 0.5, h * 0.95);
    pb.close();
    pb.finish()
}

fn shape_path(shape: &ShapeContent, w: f32, h: f32, factor: f32) -> Option<Path> {
    match shape.kind {
        ShapeKind::Rectangle => rounded_rect(0.0, 0.0, w, h, shape.corner_radius * factor),
        ShapeKind::Line => rounded_rect(0.0, 0.0, w, h, h / 2.0),
        ShapeKind::Ellipse => PathBuilder::from_oval(Rect::from_xywh(0.0, 0.0, w, h)?),
        ShapeKind::Triangle => polygon(&[(w / 2.0, 0.0), (w, h), (0.0, h)]),
        ShapeKind::Diamond => {
            polygon(&[(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)])
        }
        ShapeKind::Pentagon => ring(w, h, 5, None),
        ShapeKind::Hexagon => ring(w, h, 6, None),
        ShapeKind::Star => ring(w, h, 5, Some(0.42)),
        ShapeKind::Heart => heart(w, h),
        ShapeKind::Arrow => polygon(&[
            (0.0, h * 0.3),
            (w * 0.58, h * 0.3),
            (w * 0.58, 0.0),
            (w, h / 2.0),
            (w * 0.58, h),
            (w * 0.58, h * 0.7),
            (0.0, h * 0.7),
        ]),
        ShapeKind::Plus => polygon(&[
            (w / 3.0, 0.0),
            (w * 2.0 / 3.0, 0.0),
            (w * 2.0 / 3.0, h / 3.0),
            (w, h / 3.0),
            (w, h * 2.0 / 3.0),
            (w * 2.0 / 3.0, h * 2.0 / 3.0),
            (w * 2.0 / 3.0, h),
            (w / 3.0, h),
            (w / 3.0, h * 2.0 / 3.0),
            (0.0, h * 2.0 / 3.0),
            (0.0, h / 3.0),
            (w / 3.0, h / 3.0),
        ]),
        ShapeKind::Bubble => {
            // a rounded box with a tail; both parts are filled together
            let body_h = h * 0.78;
            let mut pb = PathBuilder::new();
            pb.push_path(&rounded_rect(0.0, 0.0, w, body_h, w.min(body_h) * 0.18)?);
            pb.push_path(&polygon(&[
                (w * 0.2, body_h - 2.0),
                (w * 0.14, h),
                (w * 0.46, body_h - 2.0),
            ])?);
            pb.finish()
        }
    }
}

fn draw(shape: &ShapeContent, factor: f32) -> ShapeImage {
    let (w, h) = measure(shape, factor);
    let stroke_w = shape.stroke.map_or(0.0, |s| s.width * factor);
    let margin = (stroke_w / 2.0 + 2.0).ceil();
    let pw = ((w + 2.0 * margin).ceil() as u32).max(1);
    let ph = ((h + 2.0 * margin).ceil() as u32).max(1);
    let mut out = ShapeImage {
        width: pw,
        height: ph,
        rgba: vec![0; (pw * ph * 4) as usize],
        content: (w, h),
    };
    let (Some(mut pixmap), Some(path)) = (Pixmap::new(pw, ph), shape_path(shape, w, h, factor))
    else {
        return out;
    };
    let place = Transform::from_translate(margin, margin);
    if let Some(c) = shape.fill {
        let mut paint = Paint::default();
        paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, place, None);
    }
    if let Some(s) = shape.stroke
        && stroke_w > 0.0
    {
        let mut paint = Paint::default();
        paint.set_color_rgba8(s.color[0], s.color[1], s.color[2], s.color[3]);
        paint.anti_alias = true;
        let line = Stroke {
            width: stroke_w,
            line_join: LineJoin::Round,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &line, place, None);
    }
    // tiny-skia keeps colours multiplied by their alpha; we hand on straight colours
    for (dst, src) in out
        .rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixmap.pixels())
    {
        let c = src.demultiply();
        *dst = [c.red(), c.green(), c.blue(), c.alpha()];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(img: &ShapeImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [
            img.rgba[i],
            img.rgba[i + 1],
            img.rgba[i + 2],
            img.rgba[i + 3],
        ]
    }

    fn filled(kind: ShapeKind) -> ShapeContent {
        ShapeContent {
            fill: Some([255, 0, 0, 255]),
            width: 100.0,
            height: 100.0,
            ..ShapeContent::new(kind)
        }
    }

    fn middle(img: &ShapeImage) -> [u8; 4] {
        at(img, img.width / 2, img.height / 2)
    }

    #[test]
    fn every_shape_draws_something_in_its_own_colour() {
        let mut cache = ShapeCache::new();
        for kind in ShapeKind::ALL {
            let shape = ShapeContent {
                height: if kind == ShapeKind::Line { 30.0 } else { 100.0 },
                ..filled(kind)
            };
            let img = cache.render(&shape, 1.0);
            let covered = img.rgba.chunks(4).filter(|p| p[3] > 200).count();
            assert!(covered > 100, "{kind:?} draws {covered} pixels");
            let solid = img.rgba.chunks(4).find(|p| p[3] == 255).unwrap();
            assert_eq!(&solid[..3], &[255, 0, 0], "{kind:?} keeps its colour");
        }
    }

    #[test]
    fn a_rectangle_fills_its_box_and_round_corners_cut_the_corner() {
        let mut cache = ShapeCache::new();
        let square = cache.render(&filled(ShapeKind::Rectangle), 1.0);
        let (m, w) = ((square.width - 100) / 2, 100);
        assert_eq!(at(&square, m + 1, m + 1)[3], 255);
        assert_eq!(at(&square, m + w - 2, m + w - 2)[3], 255);
        assert_eq!(at(&square, 0, 0)[3], 0);
        let rounded = cache.render(
            &ShapeContent {
                corner_radius: 40.0,
                ..filled(ShapeKind::Rectangle)
            },
            1.0,
        );
        assert_eq!(at(&rounded, m + 1, m + 1)[3], 0, "the corner is cut away");
        assert_eq!(middle(&rounded)[3], 255);
    }

    #[test]
    fn an_ellipse_leaves_its_corners_empty() {
        let img = ShapeCache::new().render(&filled(ShapeKind::Ellipse), 1.0);
        let m = (img.width - 100) / 2;
        assert_eq!(middle(&img)[3], 255);
        assert_eq!(at(&img, m + 3, m + 3)[3], 0);
    }

    #[test]
    fn a_star_has_a_hollow_between_its_points() {
        let img = ShapeCache::new().render(&filled(ShapeKind::Star), 1.0);
        let m = (img.width - 100) / 2;
        assert_eq!(middle(&img)[3], 255);
        // the top left of the box is outside a five-pointed star
        assert_eq!(at(&img, m + 8, m + 8)[3], 0);
    }

    #[test]
    fn an_outline_adds_its_own_colour_around_the_fill() {
        let shape = ShapeContent {
            stroke: Some(vz_core::TextStroke {
                color: [0, 0, 255, 255],
                width: 10.0,
            }),
            ..filled(ShapeKind::Rectangle)
        };
        let img = ShapeCache::new().render(&shape, 1.0);
        assert!(
            img.rgba
                .chunks(4)
                .any(|p| p[3] == 255 && p[2] == 255 && p[0] == 0),
            "blue outline"
        );
        assert!(
            img.rgba.chunks(4).any(|p| p[3] == 255 && p[0] == 255),
            "red fill"
        );
        // room is kept around the box for the outline
        assert!(img.width >= 100 + 10);
        assert_eq!(img.content, (100.0, 100.0));
    }

    #[test]
    fn a_shape_with_only_an_outline_is_hollow() {
        let shape = ShapeContent {
            fill: None,
            stroke: Some(vz_core::TextStroke {
                color: [0, 255, 0, 255],
                width: 6.0,
            }),
            ..filled(ShapeKind::Ellipse)
        };
        let img = ShapeCache::new().render(&shape, 1.0);
        assert_eq!(middle(&img)[3], 0);
        assert!(img.rgba.chunks(4).any(|p| p[3] == 255));
    }

    #[test]
    fn the_factor_scales_the_picture_and_pictures_are_cached() {
        let a = measure(&filled(ShapeKind::Heart), 1.0);
        let b = measure(&filled(ShapeKind::Heart), 2.0);
        assert_eq!(b.0, a.0 * 2.0);
        let mut cache = ShapeCache::new();
        let big = cache.render(&filled(ShapeKind::Heart), 2.0);
        assert!(big.width >= 200);
        assert!(Arc::ptr_eq(
            &big,
            &cache.render(&filled(ShapeKind::Heart), 2.0)
        ));
    }

    #[test]
    fn semi_transparent_fills_keep_their_colour() {
        let shape = ShapeContent {
            fill: Some([0, 200, 0, 128]),
            ..filled(ShapeKind::Rectangle)
        };
        let img = ShapeCache::new().render(&shape, 1.0);
        let p = middle(&img);
        assert!((i32::from(p[3]) - 128).abs() <= 2);
        assert!(
            p[1] >= 195 && p[0] <= 3,
            "straight colour, not darkened: {p:?}"
        );
    }
}
