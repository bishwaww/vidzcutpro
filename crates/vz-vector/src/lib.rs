//! Shapes: draws a [`ShapeContent`] into a picture with transparent surroundings (anti-aliased),
//! the same way `vz-text` does for text, so the renderer treats both as ordinary layers.
//!
//! Drawing is done by tiny-skia (BSD-3-Clause).

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, PathSegment, Pixmap,
    Point, RadialGradient, Rect, SpreadMode, Stroke, StrokeDash, Transform,
};
use vz_core::{GradientKind, LineCap, LineJoin, PenPath, ShapeContent, ShapeKind, StrokePlace};

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

/// A path from the pen tool, scaled to a box of `w` by `h`.
pub fn pen_path(path: &PenPath, w: f32, h: f32) -> Option<Path> {
    let nodes = &path.nodes;
    let (first, rest) = nodes.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x * w, first.y * h);
    let mut segment = |a: &vz_core::PenNode, b: &vz_core::PenNode| {
        if a.has_handles() || b.has_handles() {
            pb.cubic_to(
                (a.x + a.out_x) * w,
                (a.y + a.out_y) * h,
                (b.x + b.in_x) * w,
                (b.y + b.in_y) * h,
                b.x * w,
                b.y * h,
            );
        } else {
            pb.line_to(b.x * w, b.y * h);
        }
    };
    let mut prev = first;
    for n in rest {
        segment(prev, n);
        prev = n;
    }
    if path.closed && nodes.len() > 2 {
        segment(prev, first);
        pb.close();
    }
    if nodes.len() < 2 {
        return None;
    }
    pb.finish()
}

/// The outline as lists of points (curves cut into short straight pieces), and whether each list is closed.
fn flatten(path: &Path) -> Vec<(Vec<(f32, f32)>, bool)> {
    let mut out: Vec<(Vec<(f32, f32)>, bool)> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut last = (0.0f32, 0.0f32);
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur = vec![(p.x, p.y)];
                last = (p.x, p.y);
            }
            PathSegment::LineTo(p) => {
                cur.push((p.x, p.y));
                last = (p.x, p.y);
            }
            PathSegment::QuadTo(c, p) => {
                for i in 1..=16 {
                    let t = i as f32 / 16.0;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * last.0 + 2.0 * u * t * c.x + t * t * p.x,
                        u * u * last.1 + 2.0 * u * t * c.y + t * t * p.y,
                    ));
                }
                last = (p.x, p.y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                for i in 1..=24 {
                    let t = i as f32 / 24.0;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * u * last.0
                            + 3.0 * u * u * t * c1.x
                            + 3.0 * u * t * t * c2.x
                            + t * t * t * p.x,
                        u * u * u * last.1
                            + 3.0 * u * u * t * c1.y
                            + 3.0 * u * t * t * c2.y
                            + t * t * t * p.y,
                    ));
                }
                last = (p.x, p.y);
            }
            PathSegment::Close => {
                if cur.len() > 1 {
                    let first = cur[0];
                    cur.push(first);
                    out.push((std::mem::take(&mut cur), true));
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

/// Keep only the part of the outline from `start` to `end` (shares of its length, 0 to 1) shifted by `offset`; the part
/// wraps round the end of a closed outline.
fn trimmed(path: &Path, start: f32, end: f32, offset: f32) -> Option<Path> {
    let lines = flatten(path);
    let lengths: Vec<f32> = lines
        .iter()
        .map(|(pts, _)| {
            pts.windows(2)
                .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
                .sum()
        })
        .collect();
    let total: f32 = lengths.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let (a, b) = (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
    if b <= a {
        return None;
    }
    // the wanted stretches along the whole outline, in lengths (more than one if it wraps)
    let span = b - a;
    let from = (a + offset).rem_euclid(1.0);
    let mut ranges = Vec::new();
    if span >= 0.9999 {
        ranges.push((0.0, total));
    } else if from + span <= 1.0 {
        ranges.push((from * total, (from + span) * total));
    } else {
        ranges.push((from * total, total));
        ranges.push((0.0, (from + span - 1.0) * total));
    }
    let mut pb = PathBuilder::new();
    let mut begin = 0.0;
    for ((pts, _), len) in lines.iter().zip(&lengths) {
        let mut at = begin;
        for (lo, hi) in &ranges {
            let (lo, hi) = (lo.max(begin), hi.min(begin + len));
            if lo >= hi {
                continue;
            }
            // walk the points, starting a new piece where the wanted range begins
            let mut run = false;
            let mut dist = begin;
            for w in pts.windows(2) {
                let seg = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
                let (s0, s1) = (dist, dist + seg);
                dist = s1;
                if s1 < lo || s0 > hi || seg <= 0.0 {
                    continue;
                }
                let (t0, t1) = (
                    ((lo - s0) / seg).clamp(0.0, 1.0),
                    ((hi - s0) / seg).clamp(0.0, 1.0),
                );
                let p0 = (
                    w[0].0 + (w[1].0 - w[0].0) * t0,
                    w[0].1 + (w[1].1 - w[0].1) * t0,
                );
                let p1 = (
                    w[0].0 + (w[1].0 - w[0].0) * t1,
                    w[0].1 + (w[1].1 - w[0].1) * t1,
                );
                if !run {
                    pb.move_to(p0.0, p0.1);
                    run = true;
                }
                pb.line_to(p1.0, p1.1);
            }
        }
        let _ = &mut at;
        begin += len;
    }
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
        ShapeKind::Polygon => ring(w, h, shape.sides.clamp(3, 24) as usize, None),
        ShapeKind::Star => ring(
            w,
            h,
            shape.points.clamp(3, 24) as usize,
            Some(shape.inner.clamp(0.1, 0.9)),
        ),
        ShapeKind::Parallelogram => polygon(&[(w * 0.25, 0.0), (w, 0.0), (w * 0.75, h), (0.0, h)]),
        ShapeKind::Trapezoid => polygon(&[(w * 0.2, 0.0), (w * 0.8, 0.0), (w, h), (0.0, h)]),
        ShapeKind::Path => pen_path(shape.path.as_ref()?, w, h),
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

fn solid_paint(c: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
    paint.anti_alias = true;
    paint
}

/// The paint for a gradient across a `w` by `h` box.
fn gradient_paint(
    g: &vz_core::Gradient,
    w: f32,
    h: f32,
    place: Transform,
) -> Option<Paint<'static>> {
    let stops: Vec<GradientStop> = g
        .stops
        .iter()
        .map(|(pos, c)| {
            GradientStop::new(
                pos.clamp(0.0, 1.0),
                tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]),
            )
        })
        .collect();
    let shader = match g.kind {
        GradientKind::Linear => {
            let (sin, cos) = g.angle.to_radians().sin_cos();
            let half = (cos.abs() * w + sin.abs() * h) / 2.0;
            let (cx, cy) = (w / 2.0, h / 2.0);
            LinearGradient::new(
                Point::from_xy(cx - cos * half, cy - sin * half),
                Point::from_xy(cx + cos * half, cy + sin * half),
                stops,
                SpreadMode::Pad,
                place,
            )?
        }
        GradientKind::Radial => RadialGradient::new(
            Point::from_xy(w / 2.0, h / 2.0),
            Point::from_xy(w / 2.0, h / 2.0),
            (w.max(h) / 2.0).max(1.0),
            stops,
            SpreadMode::Pad,
            place,
        )?,
    };
    Some(Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    })
}

fn draw(shape: &ShapeContent, factor: f32) -> ShapeImage {
    let (w, h) = measure(shape, factor);
    let stroke_w = shape.stroke.map_or(0.0, |s| s.width * factor);
    let margin = (stroke_w + 2.0).ceil();
    let pw = ((w + 2.0 * margin).ceil() as u32).max(1);
    let ph = ((h + 2.0 * margin).ceil() as u32).max(1);
    let out = ShapeImage {
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
    let open = shape.kind == ShapeKind::Path && shape.path.as_ref().is_some_and(|p| !p.closed);
    // the inside
    if !open {
        let paint = match &shape.gradient {
            Some(g) => gradient_paint(g, w, h, place),
            None => shape.fill.map(solid_paint),
        };
        if let Some(paint) = paint {
            pixmap.fill_path(&path, &paint, FillRule::Winding, place, None);
        }
    }
    // the outline
    if let Some(s) = shape.stroke
        && stroke_w > 0.0
    {
        let trimmed_path =
            (shape.trim_start > 0.0 || shape.trim_end < 1.0 || shape.trim_offset != 0.0)
                .then(|| trimmed(&path, shape.trim_start, shape.trim_end, shape.trim_offset));
        let is_trimmed = trimmed_path.is_some();
        let Some(line_path) = trimmed_path.unwrap_or_else(|| Some(path.clone())) else {
            return finish(out, pixmap);
        };
        // inside and outside outlines are drawn twice as wide and cut to the shape (or what is outside it)
        let place_kind = if open || is_trimmed {
            StrokePlace::Centre
        } else {
            shape.stroke_place
        };
        let width = if place_kind == StrokePlace::Centre {
            stroke_w
        } else {
            stroke_w * 2.0
        };
        let mut line = Stroke {
            width,
            line_join: match shape.join {
                LineJoin::Round => tiny_skia::LineJoin::Round,
                LineJoin::Miter => tiny_skia::LineJoin::Miter,
                LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            line_cap: match shape.cap {
                LineCap::Round => tiny_skia::LineCap::Round,
                LineCap::Butt => tiny_skia::LineCap::Butt,
                LineCap::Square => tiny_skia::LineCap::Square,
            },
            ..Stroke::default()
        };
        if let Some((dash, gap)) = shape.dash
            && dash > 0.0
            && gap >= 0.0
        {
            line.dash = StrokeDash::new(vec![dash * factor, (gap * factor).max(0.01)], 0.0);
        }
        let paint = solid_paint(s.color);
        let mask = match place_kind {
            StrokePlace::Centre => None,
            kind => Mask::new(pw, ph).map(|mut m| {
                m.fill_path(&path, FillRule::Winding, true, place);
                if kind == StrokePlace::Outside {
                    m.invert();
                }
                m
            }),
        };
        pixmap.stroke_path(&line_path, &paint, &line, place, mask.as_ref());
    }
    finish(out, pixmap)
}

/// tiny-skia keeps colours multiplied by their alpha; we hand on straight colours.
fn finish(mut out: ShapeImage, pixmap: Pixmap) -> ShapeImage {
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
        // a pen path draws nothing until it has nodes (tested below)
        for kind in ShapeKind::ALL.into_iter().filter(|k| *k != ShapeKind::Path) {
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

    // ---- Phase 5: polygons, gradients, outline options, trim paths, pen paths ----

    use vz_core::{Gradient, PenNode, TextStroke};

    fn count(img: &ShapeImage) -> usize {
        img.rgba.chunks(4).filter(|p| p[3] > 128).count()
    }

    #[test]
    fn polygons_stars_and_the_new_kinds_follow_their_settings() {
        let mut cache = ShapeCache::new();
        let tri = ShapeContent {
            sides: 3,
            ..filled(ShapeKind::Polygon)
        };
        let many = ShapeContent {
            sides: 12,
            ..filled(ShapeKind::Polygon)
        };
        assert!(
            count(&cache.render(&many, 1.0)) > count(&cache.render(&tri, 1.0)) * 13 / 10,
            "more sides fill more of the circle"
        );
        let thin = ShapeContent {
            inner: 0.15,
            ..filled(ShapeKind::Star)
        };
        let fat = ShapeContent {
            inner: 0.8,
            ..filled(ShapeKind::Star)
        };
        assert!(count(&cache.render(&fat, 1.0)) > count(&cache.render(&thin, 1.0)) * 2);
        let few = ShapeContent {
            points: 3,
            ..filled(ShapeKind::Star)
        };
        let lots = ShapeContent {
            points: 12,
            ..filled(ShapeKind::Star)
        };
        assert_ne!(cache.render(&few, 1.0).rgba, cache.render(&lots, 1.0).rgba);
        for kind in [ShapeKind::Parallelogram, ShapeKind::Trapezoid] {
            assert!(count(&cache.render(&filled(kind), 1.0)) > 2000, "{kind:?}");
        }
    }

    #[test]
    fn a_gradient_fill_changes_colour_across_the_shape() {
        let shape = ShapeContent {
            gradient: Some(Gradient::two([255, 0, 0, 255], [0, 0, 255, 255], 0.0)),
            ..filled(ShapeKind::Rectangle)
        };
        let img = ShapeCache::new().render(&shape, 1.0);
        let m = (img.width - 100) / 2;
        let left = at(&img, m + 4, m + 50);
        let right = at(&img, m + 95, m + 50);
        assert!(left[0] > 200 && left[2] < 60, "{left:?}");
        assert!(right[2] > 200 && right[0] < 60, "{right:?}");
        let radial = ShapeContent {
            gradient: Some(Gradient {
                kind: GradientKind::Radial,
                ..Gradient::two([255, 255, 255, 255], [0, 0, 0, 255], 0.0)
            }),
            ..filled(ShapeKind::Rectangle)
        };
        let r = ShapeCache::new().render(&radial, 1.0);
        assert!(
            middle(&r)[0] > at(&r, m + 3, m + 3)[0] + 80,
            "bright in the middle, dark at the corner"
        );
    }

    fn outlined(place: StrokePlace) -> ShapeContent {
        ShapeContent {
            fill: None,
            stroke: Some(TextStroke {
                color: [0, 255, 0, 255],
                width: 10.0,
            }),
            stroke_place: place,
            ..filled(ShapeKind::Rectangle)
        }
    }

    #[test]
    fn the_outline_can_sit_inside_on_the_edge_or_outside() {
        let mut cache = ShapeCache::new();
        let width = |img: &ShapeImage| {
            let (mut l, mut r) = (u32::MAX, 0);
            for y in 0..img.height {
                for x in 0..img.width {
                    if at(img, x, y)[3] > 128 {
                        l = l.min(x);
                        r = r.max(x);
                    }
                }
            }
            i64::from(r - l + 1)
        };
        let inside = width(&cache.render(&outlined(StrokePlace::Inside), 1.0));
        let centre = width(&cache.render(&outlined(StrokePlace::Centre), 1.0));
        let outside = width(&cache.render(&outlined(StrokePlace::Outside), 1.0));
        // the box is 100 wide and the outline 10: inside stays at 100, centre grows by 10, outside by 20
        assert!((inside - 100).abs() <= 2, "{inside}");
        assert!((centre - 110).abs() <= 2, "{centre}");
        assert!((outside - 120).abs() <= 2, "{outside}");
    }

    #[test]
    fn dashes_leave_gaps() {
        let solid = ShapeContent {
            fill: None,
            stroke: Some(TextStroke {
                color: [255; 4],
                width: 6.0,
            }),
            ..filled(ShapeKind::Rectangle)
        };
        let dashed = ShapeContent {
            dash: Some((12.0, 12.0)),
            cap: LineCap::Butt,
            ..solid.clone()
        };
        let mut cache = ShapeCache::new();
        let (a, b) = (
            count(&cache.render(&solid, 1.0)),
            count(&cache.render(&dashed, 1.0)),
        );
        assert!(b < a * 6 / 10 && b > a / 4, "{b} vs {a}");
    }

    #[test]
    fn trim_paths_draw_only_part_of_the_outline_and_can_wrap() {
        let full = ShapeContent {
            fill: None,
            stroke: Some(TextStroke {
                color: [255; 4],
                width: 6.0,
            }),
            ..filled(ShapeKind::Ellipse)
        };
        let half = ShapeContent {
            trim_end: 0.5,
            cap: LineCap::Butt,
            ..full.clone()
        };
        let shifted = ShapeContent {
            trim_offset: 0.75,
            ..half.clone()
        };
        let mut cache = ShapeCache::new();
        let (a, b, c) = (
            count(&cache.render(&full, 1.0)),
            count(&cache.render(&half, 1.0)),
            count(&cache.render(&shifted, 1.0)),
        );
        assert!(b > a * 4 / 10 && b < a * 6 / 10, "{b} vs {a}");
        assert!(
            (b as f64 / c as f64 - 1.0).abs() < 0.15,
            "wrapping around keeps the length: {b} {c}"
        );
        assert_ne!(
            cache.render(&half, 1.0).rgba,
            cache.render(&shifted, 1.0).rgba,
            "but it is somewhere else"
        );
        let none = ShapeContent {
            trim_start: 0.5,
            trim_end: 0.5,
            ..full
        };
        assert_eq!(count(&cache.render(&none, 1.0)), 0);
    }

    fn zigzag(closed: bool) -> ShapeContent {
        ShapeContent {
            path: Some(PenPath {
                nodes: vec![
                    PenNode::corner(0.0, 1.0),
                    PenNode::corner(0.5, 0.0),
                    PenNode::corner(1.0, 1.0),
                ],
                closed,
            }),
            stroke: Some(TextStroke {
                color: [255; 4],
                width: 8.0,
            }),
            ..filled(ShapeKind::Path)
        }
    }

    #[test]
    fn a_pen_path_is_drawn_open_as_a_line_and_closed_as_a_filled_shape() {
        let mut cache = ShapeCache::new();
        let open = cache.render(&zigzag(false), 1.0);
        let closed = cache.render(&zigzag(true), 1.0);
        // the middle of the triangle is empty for an open line and filled when closed
        assert_eq!(middle(&open)[3], 0);
        assert_eq!(middle(&closed)[0], 255);
        assert!(count(&open) > 300, "the line is drawn");
        let none = ShapeContent {
            path: None,
            ..zigzag(true)
        };
        assert_eq!(count(&cache.render(&none, 1.0)), 0);
    }

    #[test]
    fn curved_pen_nodes_follow_their_handles() {
        let straight = zigzag(false);
        let mut bent = zigzag(false);
        if let Some(p) = &mut bent.path {
            p.nodes[0].out_y = -0.8;
        }
        let mut cache = ShapeCache::new();
        assert_ne!(
            cache.render(&straight, 1.0).rgba,
            cache.render(&bent, 1.0).rgba
        );
    }
}
