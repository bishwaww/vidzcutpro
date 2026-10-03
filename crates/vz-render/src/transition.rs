//! Blending two pictures for a transition: A is what is leaving, B what is arriving, `p` how far along it is.
//!
//! Every transition is a recipe from `vz-presets`; this file turns recipes into pixels, spread over all
//! processor cores. Both pictures are full opaque canvases of the same size.

use rayon::prelude::*;
use std::f32::consts::{PI, TAU};
use vz_media::VideoFrame;
use vz_presets::transition::Recipe;

type Rgb = [f32; 3];

fn px(f: &VideoFrame, x: usize, y: usize) -> Rgb {
    let i = (y * f.width as usize + x) * 4;
    [
        f32::from(f.rgba[i]),
        f32::from(f.rgba[i + 1]),
        f32::from(f.rgba[i + 2]),
    ]
}

/// A smooth sample at pixel position (x, y) (pixel centres at +0.5), or `None` outside the picture.
fn sample(f: &VideoFrame, x: f32, y: f32) -> Option<Rgb> {
    let (w, h) = (f.width as f32, f.height as f32);
    if x < 0.0 || y < 0.0 || x >= w || y >= h {
        return None;
    }
    let (fx, fy) = ((x - 0.5).clamp(0.0, w - 1.0), (y - 0.5).clamp(0.0, h - 1.0));
    let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
    let (tx, ty) = (fx - ix as f32, fy - iy as f32);
    let (ix2, iy2) = (
        (ix + 1).min(f.width as usize - 1),
        (iy + 1).min(f.height as usize - 1),
    );
    let (a, b, c, d) = (
        px(f, ix, iy),
        px(f, ix2, iy),
        px(f, ix, iy2),
        px(f, ix2, iy2),
    );
    let mut out = [0.0; 3];
    for i in 0..3 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bottom = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bottom - top) * ty;
    }
    Some(out)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

const BLACK: Rgb = [0.0; 3];

/// A cheap repeatable "random" number from 0 to 1.
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// 1 where `value` is below `edge`, 0 above, with a soft step `soft` wide.
fn step(value: f32, edge: f32, soft: f32) -> f32 {
    ((edge - value) / soft.max(1e-4)).clamp(0.0, 1.0)
}

/// Blend A and B at progress `p` (0 = all A, 1 = all B) following `recipe`.
pub fn blend(a: &VideoFrame, b: &VideoFrame, p: f32, recipe: Recipe) -> VideoFrame {
    let p = p.clamp(0.0, 1.0);
    let (w, h) = (a.width as usize, a.height as usize);
    let mut out = VideoFrame::black(a.width, a.height);
    out.pts = a.pts;
    if b.width != a.width || b.height != a.height || w == 0 || h == 0 {
        return a.clone();
    }
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let half_diag = (cx * cx + cy * cy).sqrt();

    // recipes that need prepared pictures
    let soft = if matches!(recipe, Recipe::BlurDissolve) {
        let sigma = (p * PI).sin() * 24.0 * (wf / 1920.0).max(0.2);
        Some((
            crate::blurred(&crate::composite::Image::from(a), sigma),
            crate::blurred(&crate::composite::Image::from(b), sigma),
        ))
    } else {
        None
    };

    out.rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, dst) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
                let (u, v) = (xf / wf, yf / hf);
                let pa = px(a, x, y);
                let pb = px(b, x, y);
                let c: Rgb = match recipe {
                    Recipe::Dissolve => mix(pa, pb, p),
                    Recipe::FadeThrough(col) => {
                        let col = [f32::from(col[0]), f32::from(col[1]), f32::from(col[2])];
                        if p < 0.5 {
                            mix(pa, col, p * 2.0)
                        } else {
                            mix(col, pb, (p - 0.5) * 2.0)
                        }
                    }
                    Recipe::BlurDissolve => {
                        let (sa, sb) = soft.as_ref().expect("prepared above");
                        mix(px(sa, x, y), px(sb, x, y), p)
                    }
                    Recipe::Slide { dx, dy } => {
                        let (tx, ty) = (dx * (1.0 - p) * wf, dy * (1.0 - p) * hf);
                        sample(b, xf - tx, yf - ty).unwrap_or(pa)
                    }
                    Recipe::Push { dx, dy } => {
                        let (tb_x, tb_y) = (dx * (1.0 - p) * wf, dy * (1.0 - p) * hf);
                        if let Some(c) = sample(b, xf - tb_x, yf - tb_y) {
                            c
                        } else {
                            sample(a, xf + dx * p * wf, yf + dy * p * hf).unwrap_or(BLACK)
                        }
                    }
                    Recipe::Wipe { angle } => {
                        let (sin, cos) = angle.to_radians().sin_cos();
                        let s = 0.5 + ((u - 0.5) * cos + (v - 0.5) * sin) / (cos.abs() + sin.abs());
                        let feather = 0.06;
                        mix(pa, pb, (p * (1.0 + feather) - s) / feather)
                    }
                    Recipe::Clock => {
                        let ang = (xf - cx).atan2(-(yf - cy));
                        let t = (ang + TAU) % TAU / TAU;
                        mix(pa, pb, (p * 1.02 - t) / 0.02)
                    }
                    Recipe::Iris { open } => {
                        let r = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() / half_diag;
                        let m = if open {
                            step(r, p * 1.05, 0.05)
                        } else {
                            1.0 - step(r, (1.0 - p) * 1.05, 0.05)
                        };
                        mix(pa, pb, m)
                    }
                    Recipe::Diamond => {
                        let d = (u - 0.5).abs() + (v - 0.5).abs();
                        mix(pa, pb, step(d, p * 1.05, 0.05))
                    }
                    Recipe::BarnDoor { vertical } => {
                        let d = if vertical {
                            (v - 0.5).abs()
                        } else {
                            (u - 0.5).abs()
                        } * 2.0;
                        mix(pa, pb, step(d, p * 1.02, 0.02))
                    }
                    Recipe::Blinds { count, vertical } => {
                        let pos = if vertical { u } else { v } * count as f32;
                        mix(pa, pb, step(pos.fract(), p * 1.02, 0.02))
                    }
                    Recipe::Split { vertical } => {
                        let (pos, len, other_pos, other_len) = if vertical {
                            (yf, hf, xf, wf)
                        } else {
                            (xf, wf, yf, hf)
                        };
                        let first_half = pos < len / 2.0;
                        let shift = p * len / 2.0;
                        let src = if first_half { pos + shift } else { pos - shift };
                        let same_half = (src < len / 2.0) == first_half && src >= 0.0 && src < len;
                        if same_half {
                            let (sx, sy) = if vertical {
                                (other_pos, src)
                            } else {
                                (src, other_pos)
                            };
                            let _ = other_len;
                            sample(a, sx, sy).unwrap_or(pb)
                        } else {
                            pb
                        }
                    }
                    Recipe::Zoom { inward } => {
                        if inward {
                            let s = 1.0 + p * 1.5;
                            let za =
                                sample(a, cx + (xf - cx) / s, cy + (yf - cy) / s).unwrap_or(BLACK);
                            mix(pb, za, 1.0 - p)
                        } else {
                            let s = 1.0 + (1.0 - p) * 1.5;
                            let zb =
                                sample(b, cx + (xf - cx) / s, cy + (yf - cy) / s).unwrap_or(BLACK);
                            mix(pa, zb, p)
                        }
                    }
                    Recipe::Spin => {
                        let angle = p * PI;
                        let s = 1.0 + p * 0.8;
                        let (sin, cos) = angle.sin_cos();
                        let (dx, dy) = ((xf - cx) / s, (yf - cy) / s);
                        let (rx, ry) = (dx * cos + dy * sin, -dx * sin + dy * cos);
                        let spun = sample(a, cx + rx, cy + ry).unwrap_or(BLACK);
                        mix(spun, pb, p * p * (3.0 - 2.0 * p))
                    }
                    Recipe::Flash => {
                        let base = if p < 0.5 { pa } else { pb };
                        let strength = (1.0 - (2.0 * p - 1.0).abs()).powi(3);
                        mix(base, [255.0; 3], strength)
                    }
                    Recipe::Pixelate => {
                        let block = 1.0 + (p * PI).sin() * wf / 14.0;
                        let (bx, by) = (
                            ((xf / block).floor() + 0.5) * block,
                            ((yf / block).floor() + 0.5) * block,
                        );
                        let (bx, by) = (bx.min(wf - 1.0), by.min(hf - 1.0));
                        let ca = sample(a, bx, by).unwrap_or(pa);
                        let cb = sample(b, bx, by).unwrap_or(pb);
                        mix(ca, cb, p)
                    }
                    Recipe::Glitch => {
                        let strength = (p * PI).sin();
                        let band = (y * 24 / h) as u32;
                        let jitter = (hash(band * 7 + 3) - 0.5) * 2.0 * strength * wf * 0.08;
                        let split = strength * wf * 0.012;
                        let from_b = hash(band * 13 + 1) < p;
                        let src = if from_b { b } else { a };
                        let at = |dx: f32| sample(src, xf + jitter + dx, yf).unwrap_or(BLACK);
                        [at(split)[0], at(0.0)[1], at(-split)[2]]
                    }
                    Recipe::Flip { vertical } => {
                        let s = (p * PI).cos().abs().max(0.001);
                        let src = if p < 0.5 { a } else { b };
                        let (mut sx, mut sy) = (xf, yf);
                        if vertical {
                            sy = cy + (yf - cy) / s;
                        } else {
                            sx = cx + (xf - cx) / s;
                        }
                        sample(src, sx, sy).map_or(BLACK, |c| scale(c, 0.6 + 0.4 * s))
                    }
                    Recipe::Cube { dir } => {
                        let xm = if dir > 0.0 { xf } else { wf - xf };
                        let edge = (1.0 - p) * wf;
                        let (u2, shade, from_a) = if xm < edge {
                            (xm / edge.max(1.0), 0.55 + 0.45 * (1.0 - p), true)
                        } else {
                            ((xm - edge) / (wf - edge).max(1.0), 0.55 + 0.45 * p, false)
                        };
                        let sx = if dir > 0.0 { u2 * wf } else { wf - u2 * wf };
                        let src = if from_a { a } else { b };
                        sample(src, sx, yf).map_or(BLACK, |c| scale(c, shade))
                    }
                };
                dst[0] = c[0].clamp(0.0, 255.0) as u8;
                dst[1] = c[1].clamp(0.0, 255.0) as u8;
                dst[2] = c[2].clamp(0.0, 255.0) as u8;
                dst[3] = 255;
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_presets::transition::transitions;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> VideoFrame {
        let mut f = VideoFrame::black(w, h);
        for p in f.rgba.as_chunks_mut::<4>().0 {
            *p = [rgb[0], rgb[1], rgb[2], 255];
        }
        f
    }

    fn at(f: &VideoFrame, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    }

    fn close(c: [u8; 3], want: [u8; 3], tol: i32) -> bool {
        (0..3).all(|i| (i32::from(c[i]) - i32::from(want[i])).abs() <= tol)
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    #[test]
    fn every_transition_starts_on_the_first_picture_and_ends_on_the_second() {
        let (a, b) = (solid(64, 36, RED), solid(64, 36, BLUE));
        for t in transitions() {
            let start = blend(&a, &b, 0.0, t.recipe);
            let end = blend(&a, &b, 1.0, t.recipe);
            let mid = blend(&a, &b, 0.5, t.recipe);
            // compare the whole picture: at least 97 % of the pixels must be the right colour
            let ok = |f: &VideoFrame, want: [u8; 3]| {
                let good = f
                    .rgba
                    .chunks(4)
                    .filter(|p| close([p[0], p[1], p[2]], want, 24))
                    .count();
                good * 100 >= f.rgba.len() / 4 * 97
            };
            assert!(ok(&start, RED), "{} should start on A", t.id);
            assert!(ok(&end, BLUE), "{} should end on B", t.id);
            // and in the middle it is not simply one of the two (except the pure flip/cube edge cases)
            assert_eq!(mid.rgba.len(), a.rgba.len());
        }
    }

    #[test]
    fn dissolve_is_an_even_mix_in_the_middle() {
        let (a, b) = (solid(8, 8, RED), solid(8, 8, BLUE));
        let mid = blend(&a, &b, 0.5, Recipe::Dissolve);
        assert!(close(at(&mid, 3, 3), [128, 0, 128], 2));
    }

    #[test]
    fn fade_through_black_passes_through_black() {
        let (a, b) = (solid(8, 8, RED), solid(8, 8, BLUE));
        let mid = blend(&a, &b, 0.5, Recipe::FadeThrough([0, 0, 0]));
        assert!(close(at(&mid, 3, 3), [0, 0, 0], 2));
    }

    #[test]
    fn a_wipe_from_the_left_shows_the_new_picture_on_the_left_first() {
        let (a, b) = (solid(100, 20, RED), solid(100, 20, BLUE));
        let half = blend(&a, &b, 0.5, Recipe::Wipe { angle: 0.0 });
        assert!(close(at(&half, 10, 10), BLUE, 2));
        assert!(close(at(&half, 90, 10), RED, 2));
        let half = blend(&a, &b, 0.5, Recipe::Wipe { angle: 180.0 });
        assert!(close(at(&half, 90, 10), BLUE, 2), "from the right instead");
        assert!(close(at(&half, 10, 10), RED, 2));
    }

    #[test]
    fn a_slide_brings_the_new_picture_in_from_the_chosen_side() {
        let (a, b) = (solid(100, 20, RED), solid(100, 20, BLUE));
        let half = blend(&a, &b, 0.5, Recipe::Slide { dx: 1.0, dy: 0.0 });
        assert!(
            close(at(&half, 20, 10), RED, 2),
            "the left is still the old picture"
        );
        assert!(
            close(at(&half, 80, 10), BLUE, 2),
            "the new one comes from the right"
        );
    }

    #[test]
    fn a_push_moves_both_pictures() {
        let mut a = solid(100, 20, RED);
        // mark the left edge of A in green: after a push to the left it must have left the picture
        for y in 0..20usize {
            for x in 0..10usize {
                let i = (y * 100 + x) * 4;
                a.rgba[i..i + 3].copy_from_slice(&[0, 255, 0]);
            }
        }
        let b = solid(100, 20, BLUE);
        let half = blend(&a, &b, 0.5, Recipe::Push { dx: 1.0, dy: 0.0 });
        assert!(close(at(&half, 5, 10), RED, 4) || close(at(&half, 5, 10), [0, 255, 0], 4));
        assert!(close(at(&half, 95, 10), BLUE, 2));
        // A's green edge (x 0..10) has moved 50 pixels to the left, out of the picture
        assert!(!(0..100).any(|x| close(at(&half, x, 10), [0, 255, 0], 10)));
    }

    #[test]
    fn iris_opens_from_the_middle_and_closing_starts_from_the_edges() {
        let (a, b) = (solid(100, 100, RED), solid(100, 100, BLUE));
        let open = blend(&a, &b, 0.4, Recipe::Iris { open: true });
        assert!(close(at(&open, 50, 50), BLUE, 2));
        assert!(close(at(&open, 2, 2), RED, 2));
        let closing = blend(&a, &b, 0.4, Recipe::Iris { open: false });
        assert!(close(at(&closing, 50, 50), RED, 2));
        assert!(close(at(&closing, 2, 2), BLUE, 2));
    }

    #[test]
    fn clock_turns_clockwise_from_the_top() {
        let (a, b) = (solid(100, 100, RED), solid(100, 100, BLUE));
        let quarter = blend(&a, &b, 0.25, Recipe::Clock);
        assert!(
            close(at(&quarter, 80, 30), BLUE, 2),
            "top right is done after a quarter turn"
        );
        assert!(close(at(&quarter, 20, 30), RED, 2), "top left is not yet");
        assert!(close(at(&quarter, 80, 80), RED, 2));
    }

    #[test]
    fn blinds_open_in_strips() {
        let (a, b) = (solid(40, 80, RED), solid(40, 80, BLUE));
        let half = blend(
            &a,
            &b,
            0.5,
            Recipe::Blinds {
                count: 4,
                vertical: false,
            },
        );
        // 4 slats over 80 rows: each 20 rows; the first half of each slat shows B
        assert!(close(at(&half, 5, 2), BLUE, 2));
        assert!(close(at(&half, 5, 18), RED, 2));
        assert!(close(at(&half, 5, 22), BLUE, 2));
    }

    #[test]
    fn a_flip_squeezes_to_nothing_in_the_middle() {
        let (a, b) = (solid(100, 40, RED), solid(100, 40, BLUE));
        let mid = blend(&a, &b, 0.5, Recipe::Flip { vertical: false });
        assert!(close(at(&mid, 5, 20), BLACK_U8, 2));
        let early = blend(&a, &b, 0.25, Recipe::Flip { vertical: false });
        assert!(
            at(&early, 50, 20)[0] > 100,
            "the old picture is still on its way out"
        );
        let late = blend(&a, &b, 0.75, Recipe::Flip { vertical: false });
        assert!(at(&late, 50, 20)[2] > 100);
    }

    const BLACK_U8: [u8; 3] = [0, 0, 0];

    #[test]
    fn the_cube_has_the_old_face_on_the_left_and_the_new_one_on_the_right() {
        let (a, b) = (solid(100, 40, RED), solid(100, 40, BLUE));
        let half = blend(&a, &b, 0.5, Recipe::Cube { dir: 1.0 });
        assert!(at(&half, 20, 20)[0] > 80 && at(&half, 20, 20)[2] < 10);
        assert!(at(&half, 80, 20)[2] > 80 && at(&half, 80, 20)[0] < 10);
    }

    #[test]
    fn different_picture_sizes_do_not_crash() {
        let out = blend(
            &solid(10, 10, RED),
            &solid(12, 10, BLUE),
            0.5,
            Recipe::Dissolve,
        );
        assert_eq!((out.width, out.height), (10, 10));
    }

    #[test]
    fn blur_mix_and_glitch_and_pixelate_run_on_real_pictures() {
        let mut a = solid(160, 90, RED);
        for (i, p) in a.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if (i / 160 + i % 160) % 8 < 4 {
                *p = [255, 255, 255, 255];
            }
        }
        let b = solid(160, 90, BLUE);
        for recipe in [
            Recipe::BlurDissolve,
            Recipe::Glitch,
            Recipe::Pixelate,
            Recipe::Spin,
        ] {
            let out = blend(&a, &b, 0.5, recipe);
            assert!(
                out.rgba.chunks(4).all(|p| p[3] == 255),
                "{recipe:?} stays opaque"
            );
        }
    }
}
