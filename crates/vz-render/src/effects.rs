//! The effects: one routine per effect in `vz-presets`. Each changes a picture in place, using a copy of the original
//! where it needs to look at other places (stretching, blurring, shifting colours).

use crate::color::{hsl_to_rgb, rgb_to_hsl};
use rayon::prelude::*;
use std::f32::consts::{PI, TAU};
use vz_media::VideoFrame;
use vz_presets::effects::Kind;

type Px = [f32; 4];

/// A smooth sample of `src` at pixel position (x, y); positions outside are clamped to the edge.
fn sample(src: &VideoFrame, x: f32, y: f32) -> Px {
    let (w, h) = (src.width as usize, src.height as usize);
    let (fx, fy) = (
        (x - 0.5).clamp(0.0, w as f32 - 1.0),
        (y - 0.5).clamp(0.0, h as f32 - 1.0),
    );
    let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
    let (tx, ty) = (fx - ix as f32, fy - iy as f32);
    let (ix2, iy2) = ((ix + 1).min(w - 1), (iy + 1).min(h - 1));
    let at = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        [
            f32::from(src.rgba[i]),
            f32::from(src.rgba[i + 1]),
            f32::from(src.rgba[i + 2]),
            f32::from(src.rgba[i + 3]),
        ]
    };
    let (a, b, c, d) = (at(ix, iy), at(ix2, iy), at(ix, iy2), at(ix2, iy2));
    let mut out = [0.0; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bottom = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bottom - top) * ty;
    }
    out
}

fn put(dst: &mut [u8], c: Px) {
    dst[0] = c[0].clamp(0.0, 255.0) as u8;
    dst[1] = c[1].clamp(0.0, 255.0) as u8;
    dst[2] = c[2].clamp(0.0, 255.0) as u8;
    dst[3] = c[3].clamp(0.0, 255.0) as u8;
}

/// Rebuild the picture by asking, for every pixel, which place of the original it should show.
fn warp(frame: &mut VideoFrame, f: impl Fn(f32, f32) -> (f32, f32) + Sync) {
    let src = frame.clone();
    let w = frame.width as usize;
    frame
        .rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, dst) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (sx, sy) = f(x as f32 + 0.5, y as f32 + 0.5);
                put(dst, sample(&src, sx, sy));
            }
        });
}

/// Average several looks at the original: `f(x, y, i)` gives the place for look number `i`.
fn warp_average(
    frame: &mut VideoFrame,
    looks: usize,
    f: impl Fn(f32, f32, usize) -> (f32, f32) + Sync,
) {
    let src = frame.clone();
    let w = frame.width as usize;
    frame
        .rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, dst) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let mut sum = [0.0; 4];
                for i in 0..looks {
                    let (sx, sy) = f(x as f32 + 0.5, y as f32 + 0.5, i);
                    let c = sample(&src, sx, sy);
                    for k in 0..4 {
                        sum[k] += c[k];
                    }
                }
                put(dst, sum.map(|v| v / looks as f32));
            }
        });
}

/// Change every pixel's colour (`f(x, y, rgb)`, colours 0 to 255).
fn recolor(frame: &mut VideoFrame, f: impl Fn(usize, usize, [f32; 3]) -> [f32; 3] + Sync) {
    let w = frame.width as usize;
    frame
        .rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, p) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let o = f(x, y, [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]);
                p[0] = o[0].clamp(0.0, 255.0) as u8;
                p[1] = o[1].clamp(0.0, 255.0) as u8;
                p[2] = o[2].clamp(0.0, 255.0) as u8;
            }
        });
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn lum(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// A cheap repeatable "random" number from 0 to 1.
fn rnd(a: u32, b: u32, c: u32) -> f32 {
    let mut n =
        a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B) ^ c.wrapping_mul(0xC2B2_AE35);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    n = n.wrapping_mul(0x297A_2D39);
    n ^= n >> 15;
    (n & 0xFFFF) as f32 / 65535.0
}

/// Smooth wandering value between -1 and 1 (for camera shake and light leaks).
fn wander(t: f32, seed: u32) -> f32 {
    let (i, f) = (t.floor(), t - t.floor());
    let a = rnd(i as i32 as u32, seed, 7) * 2.0 - 1.0;
    let b = rnd(i as i32 as u32 + 1, seed, 7) * 2.0 - 1.0;
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// Apply the effect `kind` with control values `p` to the picture. `time` is seconds since the start of the clip.
pub fn apply(kind: Kind, p: &[f32], frame: &mut VideoFrame, time: f32, seed: u32) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w < 2 || h < 2 {
        return;
    }
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let half_min = wf.min(hf) / 2.0;
    let a = p.first().copied().unwrap_or(60.0) / 100.0;
    let b = p.get(1).copied().unwrap_or(50.0);
    let speed = b / 100.0;
    let frame_no = (time * 24.0).floor() as u32;
    match kind {
        Kind::Blur => {
            let soft = crate::blurred(&crate::composite::Image::from(&*frame), a * 0.025 * wf);
            frame.rgba = soft.rgba;
        }
        Kind::ZoomBlur => {
            let s = a * 0.25;
            warp_average(frame, 14, |x, y, i| {
                let k = 1.0 - s * i as f32 / 13.0;
                (cx + (x - cx) * k, cy + (y - cy) * k)
            });
        }
        Kind::RadialBlur => {
            let s = a * 0.12;
            warp_average(frame, 14, |x, y, i| {
                let ang = (i as f32 / 13.0 - 0.5) * s;
                let (sin, cos) = ang.sin_cos();
                let (dx, dy) = (x - cx, y - cy);
                (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
            });
        }
        Kind::MotionBlur => {
            let angle = p.get(1).copied().unwrap_or(0.0).to_radians();
            let len = a * 0.08 * wf;
            let (sin, cos) = angle.sin_cos();
            warp_average(frame, 16, |x, y, i| {
                let d = (i as f32 / 15.0 - 0.5) * len;
                (x + cos * d, y + sin * d)
            });
        }
        Kind::Sharpen => crate::color::apply(
            frame,
            &vz_core::Adjust {
                sharpen: a * 100.0,
                ..vz_core::Adjust::default()
            },
            seed,
            &mut crate::lut::LutCache::default(),
        ),
        Kind::Vignette => recolor(frame, |x, y, c| {
            let (u, v) = ((x as f32 + 0.5 - cx) / cx, (y as f32 + 0.5 - cy) / cy);
            let d = (u * u + v * v).sqrt() / std::f32::consts::SQRT_2;
            let edge = ((d - 0.3) / 0.7).clamp(0.0, 1.0);
            let k = 1.0 - a * 1.1 * edge * edge;
            c.map(|v| v * k.max(0.0))
        }),
        Kind::FilmGrain => recolor(frame, |x, y, c| {
            let n = (rnd(x as u32, y as u32, seed.wrapping_add(frame_no)) - 0.5) * a * 90.0;
            c.map(|v| v + n)
        }),
        Kind::Letterbox => {
            let bar = (a * 0.25 * hf) as usize;
            recolor(
                frame,
                |_, y, c| if y < bar || y + bar >= h { [0.0; 3] } else { c },
            );
        }
        Kind::Mirror => {
            let dir = p.first().copied().unwrap_or(0.0).round() as i32;
            warp(frame, |x, y| match dir {
                0 => (if x > cx { wf - x } else { x }, y),
                1 => (if x < cx { wf - x } else { x }, y),
                2 => (x, if y > cy { hf - y } else { y }),
                _ => (x, if y < cy { hf - y } else { y }),
            });
        }
        Kind::Flip => {
            let dir = p.first().copied().unwrap_or(0.0).round() as i32;
            warp(frame, |x, y| match dir {
                0 => (wf - x, y),
                1 => (x, hf - y),
                _ => (wf - x, hf - y),
            });
        }
        Kind::SplitScreen => {
            let parts = p.first().copied().unwrap_or(4.0).round() as i32;
            let (cols, rows) = match parts {
                ..=2 => (2.0, 1.0),
                3 => (3.0, 1.0),
                4..=5 => (2.0, 2.0),
                _ => (3.0, 3.0),
            };
            warp(frame, |x, y| (x * cols % wf, y * rows % hf));
        }
        Kind::Tile => {
            let n = p.first().copied().unwrap_or(3.0).round().max(1.0);
            warp(frame, |x, y| (x * n % wf, y * n % hf));
        }
        Kind::Glow => {
            let spread = p.get(1).copied().unwrap_or(50.0) / 100.0;
            let soft = crate::blurred(
                &crate::composite::Image::from(&*frame),
                (0.01 + spread * 0.04) * wf,
            );
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .zip(soft.rgba.par_chunks(w4))
                .for_each(|(line, soft_line)| {
                    for (d, s) in line
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(soft_line.as_chunks::<4>().0)
                    {
                        for i in 0..3 {
                            let glow = (f32::from(s[i]) - 90.0).max(0.0) * 1.6;
                            let v = f32::from(d[i]) + glow * a;
                            d[i] = v.clamp(0.0, 255.0) as u8;
                        }
                    }
                });
        }
        Kind::Flash => {
            let pulse = ((time * (0.5 + speed * 4.0) * TAU).sin().max(0.0)).powi(10) * a;
            recolor(frame, |_, _, c| mix3(c, [255.0; 3], pulse));
        }
        Kind::Strobe => {
            let on = ((time * (1.0 + speed * 12.0)).fract()) < 0.5;
            recolor(frame, |_, _, c| {
                if on {
                    mix3(c, [255.0; 3], a * 0.6)
                } else {
                    c.map(|v| v * (1.0 - a * 0.7))
                }
            });
        }
        Kind::ChromaticAberration => {
            let k = a * 0.03;
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let (dx, dy) = (px - cx, py - cy);
                        let r = sample(&src, cx + dx * (1.0 + k), cy + dy * (1.0 + k));
                        let g = sample(&src, px, py);
                        let bl = sample(&src, cx + dx * (1.0 - k), cy + dy * (1.0 - k));
                        put(d, [r[0], g[1], bl[2], g[3]]);
                    }
                });
        }
        Kind::Prism => {
            let off = a * 0.04 * wf;
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let c = sample(&src, px, py);
                        let l = sample(&src, px - off, py - off * 0.4);
                        let r = sample(&src, px + off, py + off * 0.4);
                        // three copies side by side: the middle one is kept, the others tint the edges
                        put(
                            d,
                            [
                                c[0] * 0.55 + l[0] * 0.55,
                                c[1] * 0.7 + (l[1] + r[1]) * 0.18,
                                c[2] * 0.55 + r[2] * 0.55,
                                c[3],
                            ],
                        );
                    }
                });
        }
        Kind::LightLeak => {
            let t = time * (0.1 + speed * 0.6);
            let (lx, ly) = (cx + wander(t, 1) * wf * 0.45, cy + wander(t, 2) * hf * 0.35);
            let radius = wf.max(hf) * 0.55;
            recolor(frame, |x, y, c| {
                let d = ((x as f32 - lx).powi(2) + (y as f32 - ly).powi(2)).sqrt() / radius;
                let glow = (1.0 - d).clamp(0.0, 1.0).powi(2) * a;
                let leak = [255.0, 140.0 + 60.0 * (t * 2.0).sin(), 50.0];
                // "screen" blend: 255 - (255 - c)(255 - leak * glow) / 255
                let mut o = c;
                for i in 0..3 {
                    o[i] = 255.0 - (255.0 - c[i]) * (255.0 - leak[i] * glow) / 255.0;
                }
                o
            });
        }
        Kind::LensFlare => {
            let t = time * (0.1 + speed * 0.5);
            let light = (
                cx + wander(t, 3) * wf * 0.4,
                cy - hf * 0.25 + wander(t, 4) * hf * 0.15,
            );
            // blobs along the line from the light through the middle of the picture
            let blobs: Vec<(f32, f32, f32, [f32; 3])> = (0..6)
                .map(|i| {
                    let k = [0.0, 0.35, 0.7, 1.2, 1.6, 2.0][i];
                    let pos = (light.0 + (cx - light.0) * k, light.1 + (cy - light.1) * k);
                    let r = [0.16, 0.05, 0.08, 0.04, 0.1, 0.06][i] * wf;
                    let col = [
                        [255.0, 240.0, 200.0],
                        [255.0, 120.0, 60.0],
                        [100.0, 200.0, 255.0],
                        [255.0, 255.0, 120.0],
                        [255.0, 100.0, 200.0],
                        [120.0, 255.0, 160.0],
                    ][i];
                    (pos.0, pos.1, r, col)
                })
                .collect();
            recolor(frame, |x, y, c| {
                let mut out = c;
                for (bx, by, r, col) in &blobs {
                    let d = ((x as f32 - bx).powi(2) + (y as f32 - by).powi(2)).sqrt() / r;
                    let g = (1.0 - d).clamp(0.0, 1.0).powi(2) * a * 0.9;
                    for i in 0..3 {
                        out[i] = 255.0 - (255.0 - out[i]) * (255.0 - col[i] * g) / 255.0;
                    }
                }
                out
            });
        }
        Kind::Vhs => {
            let t = time * (0.5 + speed * 3.0);
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    let band = y / 6;
                    let jitter = (rnd(band as u32, (t * 8.0) as u32, 5) - 0.5)
                        * a
                        * 0.03
                        * wf
                        * (rnd(band as u32, (t * 8.0) as u32, 9) > 0.8) as i32 as f32;
                    let tracking = ((y as f32 / hf * 3.0 + t).sin() * a * 0.004 * wf).max(0.0);
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let px = x as f32 + 0.5 + jitter + tracking;
                        let py = y as f32 + 0.5;
                        let bleed = a * 0.006 * wf;
                        let r = sample(&src, px + bleed, py);
                        let g = sample(&src, px, py);
                        let bl = sample(&src, px - bleed, py);
                        let mut c = [r[0], g[1], bl[2]];
                        let scan = if y % 2 == 0 { 1.0 } else { 1.0 - a * 0.18 };
                        let n = (rnd(x as u32, y as u32, frame_no) - 0.5) * a * 40.0;
                        let l = lum(c);
                        c = mix3(c, [l; 3], a * 0.25);
                        for v in &mut c {
                            *v = *v * scan + n;
                        }
                        put(d, [c[0], c[1], c[2], g[3]]);
                    }
                });
        }
        Kind::OldFilm => {
            let flicker = 1.0 - a * 0.25 * rnd(frame_no, 3, 1);
            let scratch_x = |i: u32| (rnd(frame_no / 2, i, 11) * wf) as usize;
            let scratches: Vec<usize> = (0..3)
                .filter(|i| rnd(frame_no / 2, *i, 12) < 0.4 * a + 0.1)
                .map(scratch_x)
                .collect();
            recolor(frame, |x, y, c| {
                let l = lum(c);
                let sepia = [l * 1.07, l * 0.92, l * 0.72];
                let mut o = mix3(c, sepia, a * 0.85);
                let (u, v) = ((x as f32 - cx) / cx, (y as f32 - cy) / cy);
                let edge =
                    ((u * u + v * v).sqrt() / std::f32::consts::SQRT_2 - 0.35).clamp(0.0, 1.0);
                let n = (rnd(x as u32, y as u32, frame_no) - 0.5) * a * 60.0;
                for ch in &mut o {
                    *ch = *ch * flicker * (1.0 - a * 0.8 * edge * edge) + n;
                }
                if scratches.contains(&x) {
                    o = mix3(o, [235.0; 3], 0.5 * a);
                }
                // dust
                if rnd(x as u32 / 3, y as u32 / 3, frame_no) > 0.9995 {
                    o = [20.0; 3];
                }
                o
            });
        }
        Kind::BlackWhite => recolor(frame, |_, _, c| mix3(c, [lum(c); 3], a)),
        Kind::Sepia => recolor(frame, |_, _, c| {
            let l = lum(c);
            mix3(c, [l * 1.07, l * 0.92, l * 0.72], a)
        }),
        Kind::Crt => recolor(frame, |x, y, c| {
            let scan = 1.0 - a * 0.35 * (0.5 + 0.5 * (y as f32 * PI).cos());
            let mask = match x % 3 {
                0 => [1.0, 1.0 - a * 0.25, 1.0 - a * 0.25],
                1 => [1.0 - a * 0.25, 1.0, 1.0 - a * 0.25],
                _ => [1.0 - a * 0.25, 1.0 - a * 0.25, 1.0],
            };
            let (u, v) = ((x as f32 - cx) / cx, (y as f32 - cy) / cy);
            let corner = 1.0 - a * 0.4 * ((u * u + v * v) / 2.0).powi(2);
            [
                c[0] * scan * mask[0] * corner,
                c[1] * scan * mask[1] * corner,
                c[2] * scan * mask[2] * corner,
            ]
        }),
        Kind::Pixelate => {
            let block = 1.0 + a * 0.12 * wf * 0.9;
            warp(frame, |x, y| {
                (
                    ((x / block).floor() + 0.5) * block,
                    ((y / block).floor() + 0.5) * block,
                )
            });
        }
        Kind::Halftone => {
            let cell = 3.0 + p.get(1).copied().unwrap_or(30.0) / 100.0 * 0.04 * wf;
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let (gx, gy) = (
                            ((px / cell).floor() + 0.5) * cell,
                            ((py / cell).floor() + 0.5) * cell,
                        );
                        let c = sample(&src, gx, gy);
                        let l = lum([c[0], c[1], c[2]]) / 255.0;
                        let radius = (1.0 - l).max(0.0).sqrt() * cell * 0.75;
                        let dist = ((px - gx).powi(2) + (py - gy).powi(2)).sqrt();
                        let dot = (radius - dist + 0.5).clamp(0.0, 1.0);
                        let o = mix3(
                            [245.0, 240.0, 230.0],
                            [c[0] * 0.55, c[1] * 0.55, c[2] * 0.55],
                            dot,
                        );
                        let orig = sample(&src, px, py);
                        let m = mix3([orig[0], orig[1], orig[2]], o, a);
                        put(d, [m[0], m[1], m[2], orig[3]]);
                    }
                });
        }
        Kind::Shake => {
            let t = time * (1.0 + speed * 12.0);
            let amp = a * 0.03 * wf;
            let (ox, oy, rot) = (
                wander(t, 5) * amp,
                wander(t, 6) * amp,
                wander(t, 7) * a * 0.03,
            );
            let zoom = 1.0 + a * 0.06;
            warp(frame, |x, y| {
                let (dx, dy) = ((x - cx) / zoom, (y - cy) / zoom);
                let (sin, cos) = rot.sin_cos();
                (cx + dx * cos - dy * sin + ox, cy + dx * sin + dy * cos + oy)
            });
        }
        Kind::Wave => {
            let t = time * (0.5 + speed * 5.0);
            let amp = a * 0.03 * wf;
            warp(frame, |x, y| {
                (x + (y / hf * TAU * 4.0 + t * 3.0).sin() * amp, y)
            });
        }
        Kind::Swirl => warp(frame, |x, y| {
            let (dx, dy) = (x - cx, y - cy);
            let r = (dx * dx + dy * dy).sqrt() / half_min;
            let ang = a * 3.0 * (1.0 - r).max(0.0).powi(2);
            let (sin, cos) = ang.sin_cos();
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        }),
        Kind::Fisheye => warp(frame, |x, y| {
            let (dx, dy) = ((x - cx) / half_min, (y - cy) / half_min);
            let r = (dx * dx + dy * dy).sqrt();
            if r < 1e-5 {
                return (x, y);
            }
            // barrel: points near the edge are pulled from further in
            let k = r.powf(1.0 + a * 0.9) / r;
            (cx + dx * k * half_min, cy + dy * k * half_min)
        }),
        Kind::Bulge => {
            let amount = p.first().copied().unwrap_or(60.0) / 100.0;
            warp(frame, |x, y| {
                let (dx, dy) = ((x - cx) / half_min, (y - cy) / half_min);
                let r = (dx * dx + dy * dy).sqrt();
                if r >= 1.0 || r < 1e-5 {
                    return (x, y);
                }
                let k = 1.0 - amount * (1.0 - r) * (1.0 - r);
                (cx + dx * k * half_min, cy + dy * k * half_min)
            });
        }
        Kind::Ripple => {
            let t = time * (0.5 + speed * 4.0);
            let amp = a * 0.012;
            warp(frame, |x, y| {
                let (dx, dy) = (x - cx, y - cy);
                let r = (dx * dx + dy * dy).sqrt();
                if r < 1e-5 {
                    return (x, y);
                }
                let off = (r / half_min * 28.0 - t * 6.0).sin() * amp * half_min;
                (x + dx / r * off, y + dy / r * off)
            });
        }
        Kind::Glitch => {
            let step = (time * (2.0 + speed * 14.0)).floor() as u32;
            let bands = 18u32;
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    let band = (y as u32 * bands / h as u32) + (rnd(step, 1, 3) * 7.0) as u32;
                    let active = rnd(band, step, 21) < 0.25 + a * 0.3;
                    let shift = if active {
                        (rnd(band, step, 22) - 0.5) * a * 0.25 * wf
                    } else {
                        0.0
                    };
                    let split = if active { a * 0.02 * wf } else { 0.0 };
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (px, py) = (x as f32 + 0.5 + shift, y as f32 + 0.5);
                        let r = sample(&src, px + split, py);
                        let g = sample(&src, px, py);
                        let bl = sample(&src, px - split, py);
                        put(d, [r[0], g[1], bl[2], g[3]]);
                    }
                });
        }
        Kind::RgbSplit => {
            let angle = p.get(1).copied().unwrap_or(0.0).to_radians();
            let off = a * 0.03 * wf;
            let (sin, cos) = angle.sin_cos();
            let src = frame.clone();
            let w4 = w * 4;
            frame
                .rgba
                .par_chunks_mut(w4)
                .enumerate()
                .for_each(|(y, line)| {
                    for (x, d) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let r = sample(&src, px + cos * off, py + sin * off);
                        let g = sample(&src, px, py);
                        let bl = sample(&src, px - cos * off, py - sin * off);
                        put(d, [r[0], g[1], bl[2], g[3]]);
                    }
                });
        }
        Kind::DigitalNoise => {
            let block = (wf / 80.0).max(2.0);
            recolor(frame, |x, y, c| {
                let (bx, by) = ((x as f32 / block) as u32, (y as f32 / block) as u32);
                let n = rnd(bx, by, seed.wrapping_add(frame_no));
                if n < a * 0.25 {
                    let v = rnd(bx, by, 99 + frame_no) * 255.0;
                    mix3(c, [v, v * rnd(by, bx, 5), v * rnd(bx, 2, by)], 0.85)
                } else {
                    c
                }
            });
        }
        Kind::Kaleidoscope => {
            let segments = p.first().copied().unwrap_or(6.0).round().max(2.0);
            let seg = TAU / segments;
            let spin = time * speed * 0.5;
            warp(frame, |x, y| {
                let (dx, dy) = (x - cx, y - cy);
                let r = (dx * dx + dy * dy).sqrt();
                let ang = (dy.atan2(dx) + spin).rem_euclid(seg);
                let folded = if ang > seg / 2.0 { seg - ang } else { ang };
                (cx + r * folded.cos(), cy + r * folded.sin())
            });
        }
        Kind::PulseZoom => {
            let beat = (time * (0.5 + speed * 4.0)).fract();
            let zoom = 1.0 + a * 0.18 * (1.0 - beat).powi(3);
            warp(frame, |x, y| (cx + (x - cx) / zoom, cy + (y - cy) / zoom));
        }
        Kind::Flicker => {
            let step = (time * (4.0 + speed * 30.0)) as u32;
            let k = 1.0 - a * 0.6 * rnd(step, 4, 2);
            recolor(frame, |_, _, c| c.map(|v| v * k));
        }
        Kind::ColorCycle => {
            let shift = time * (10.0 + speed * 200.0);
            recolor(frame, |_, _, c| {
                let mut hsl = rgb_to_hsl(c.map(|v| v / 255.0));
                hsl[0] += shift;
                let o = hsl_to_rgb(hsl).map(|v| v * 255.0);
                mix3(c, o, a)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_presets::effects::effects;

    /// A picture with a horizontal ramp and a vertical ramp, so shifts and stretches show.
    fn scene(w: u32, h: u32) -> VideoFrame {
        let mut f = VideoFrame::black(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                f.rgba[i] = (x * 255 / w) as u8;
                f.rgba[i + 1] = (y * 255 / h) as u8;
                f.rgba[i + 2] = 90 + ((x / 8 + y / 8) % 2 * 80) as u8;
            }
        }
        f
    }

    fn run(id: &str, params: &[f32]) -> VideoFrame {
        let preset = vz_presets::effects::find(id).unwrap();
        let mut f = scene(64, 36);
        apply(preset.kind, params, &mut f, 0.7, 3);
        f
    }

    fn defaults(id: &str) -> Vec<f32> {
        vz_presets::effects::find(id).unwrap().defaults()
    }

    #[test]
    fn every_effect_runs_keeps_the_size_and_stays_opaque() {
        for e in effects() {
            for (time, params) in [
                (0.0, e.defaults()),
                (1.3, e.params.iter().map(|p| p.max).collect::<Vec<_>>()),
                (2.0, e.params.iter().map(|p| p.min).collect()),
            ] {
                let mut f = scene(80, 45);
                apply(e.kind, &params, &mut f, time, 1);
                assert_eq!((f.width, f.height), (80, 45), "{}", e.id);
                assert_eq!(f.rgba.len(), 80 * 45 * 4);
                assert!(
                    f.rgba.chunks(4).all(|p| p[3] == 255),
                    "{} keeps pictures opaque",
                    e.id
                );
            }
        }
    }

    #[test]
    fn every_effect_with_its_defaults_changes_the_picture() {
        let original = scene(64, 36);
        for e in effects() {
            let mut f = scene(64, 36);
            apply(e.kind, &e.defaults(), &mut f, 0.45, 1);
            // some effects are quiet at some moments (a flash between pulses); try two times
            let mut g = scene(64, 36);
            apply(e.kind, &e.defaults(), &mut g, 1.1, 1);
            assert!(
                f.rgba != original.rgba || g.rgba != original.rgba,
                "{} does something",
                e.id
            );
        }
    }

    #[test]
    fn blur_smooths_and_sharpen_does_the_opposite() {
        // the sum of squared steps between neighbours: soft pictures have small steps
        let detail = |f: &VideoFrame| -> f32 {
            f.rgba
                .chunks(4)
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| (f32::from(w[0][2]) - f32::from(w[1][2])).powi(2))
                .sum()
        };
        let base = scene(64, 36);
        assert!(detail(&run("blur", &[80.0])) < detail(&base) * 0.6);
        assert!(detail(&run("sharpen", &[100.0])) > detail(&base));
    }

    #[test]
    fn black_and_white_removes_the_colour_and_sepia_tints() {
        let bw = run("black_white", &[100.0]);
        assert!(bw.rgba.chunks(4).all(
            |p| p[0] == p[1] && p[1] == p[2] || (i32::from(p[0]) - i32::from(p[2])).abs() <= 1
        ));
        let sepia = run("sepia", &[100.0]);
        assert!(
            sepia
                .rgba
                .chunks(4)
                .filter(|p| p[0] > 60)
                .all(|p| p[0] >= p[2])
        );
    }

    #[test]
    fn mirror_flip_and_split_are_exact_rearrangements() {
        let base = scene(64, 36);
        let px = |f: &VideoFrame, x: usize, y: usize| f.rgba[(y * 64 + x) * 4];
        let mirror = run("mirror", &[0.0]);
        assert!(
            (i32::from(px(&mirror, 5, 10)) - i32::from(px(&mirror, 58, 10))).abs() <= 6,
            "left and right match"
        );
        let flip = run("flip", &[0.0]);
        assert!((i32::from(px(&flip, 10, 10)) - i32::from(px(&base, 53, 10))).abs() <= 6);
        let split = run("split_screen", &[4.0]);
        assert!(
            (i32::from(px(&split, 5, 5)) - i32::from(px(&split, 37, 5))).abs() <= 12,
            "the second picture repeats the first"
        );
    }

    #[test]
    fn letterbox_bars_are_black_at_the_top_and_bottom_only() {
        let f = run("letterbox", &[100.0]);
        let row = |y: usize| f.rgba[(y * 64 + 32) * 4..][..3].to_vec();
        assert_eq!(row(0), vec![0, 0, 0]);
        assert_eq!(row(35), vec![0, 0, 0]);
        assert_ne!(row(18), vec![0, 0, 0]);
    }

    #[test]
    fn pixelate_makes_flat_blocks() {
        let f = run("pixelate", &[50.0]);
        // 64 wide: block = 1 + 0.5*0.12*64*0.9 = 4.4: neighbours inside a block match
        let px = |x: usize| f.rgba[(10 * 64 + x) * 4..][..3].to_vec();
        assert_eq!(px(1), px(2));
        let distinct = (0..64)
            .filter(|x| px(*x) != px(x.saturating_sub(1)))
            .count();
        assert!(distinct < 25, "{distinct}");
    }

    #[test]
    fn vignette_darkens_corners_and_grain_and_noise_add_variation() {
        let v = run("vignette", &[100.0]);
        assert!(v.rgba[0] < scene(64, 36).rgba[0] + 1 && v.rgba[(18 * 64 + 32) * 4] >= v.rgba[0]);
        let before = scene(64, 36);
        assert_ne!(run("film_grain", &[100.0]).rgba, before.rgba);
        assert_ne!(run("digital_noise", &[100.0]).rgba, before.rgba);
    }

    #[test]
    fn animated_effects_change_with_time_and_still_ones_do_not() {
        let preset = vz_presets::effects::find("shake").unwrap();
        let (mut a, mut b) = (scene(64, 36), scene(64, 36));
        apply(preset.kind, &preset.defaults(), &mut a, 0.2, 1);
        apply(preset.kind, &preset.defaults(), &mut b, 0.9, 1);
        assert_ne!(a.rgba, b.rgba);
        let still = vz_presets::effects::find("mirror").unwrap();
        let (mut a, mut b) = (scene(64, 36), scene(64, 36));
        apply(still.kind, &still.defaults(), &mut a, 0.2, 1);
        apply(still.kind, &still.defaults(), &mut b, 0.9, 1);
        assert_eq!(a.rgba, b.rgba);
    }

    #[test]
    fn zero_intensity_leaves_most_effects_alone() {
        let original = scene(64, 36);
        for id in [
            "blur",
            "sharpen",
            "vignette",
            "black_white",
            "sepia",
            "glow",
            "chromatic",
            "rgb_split",
            "swirl",
            "fisheye",
            "wave",
        ] {
            let mut p = defaults(id);
            p[0] = 0.0;
            let mut f = scene(64, 36);
            apply(
                vz_presets::effects::find(id).unwrap().kind,
                &p,
                &mut f,
                0.5,
                1,
            );
            let diff: i64 = f
                .rgba
                .iter()
                .zip(&original.rgba)
                .map(|(a, b)| (i64::from(*a) - i64::from(*b)).abs())
                .sum();
            assert!(diff < (64 * 36 * 4) as i64 / 2, "{id}: {diff}");
        }
    }

    #[test]
    fn tiny_pictures_do_not_crash() {
        for e in effects() {
            let mut f = VideoFrame::black(2, 2);
            apply(e.kind, &e.defaults(), &mut f, 0.3, 1);
            let mut g = VideoFrame::black(1, 1);
            apply(e.kind, &e.defaults(), &mut g, 0.3, 1);
        }
    }
}

#[cfg(test)]
mod speed_check {
    use super::*;

    /// How long the effects take on a 1080p picture (run with `--ignored --nocapture`).
    #[test]
    #[ignore = "a measurement, not a check"]
    fn effects_on_a_1080p_picture() {
        let mut f = VideoFrame::black(1920, 1080);
        for (i, p) in f.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *p = [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255];
        }
        let mut total = 0.0;
        for e in vz_presets::effects::effects() {
            let started = std::time::Instant::now();
            apply(e.kind, &e.defaults(), &mut f.clone(), 0.5, 1);
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            total += ms;
            println!("{:>22}: {ms:6.1} ms", e.name);
        }
        let started = std::time::Instant::now();
        let adj = vz_core::Adjust {
            exposure: 10.0,
            saturation: 20.0,
            contrast: 10.0,
            vignette: 30.0,
            ..vz_core::Adjust::default()
        };
        crate::color::apply(&mut f, &adj, 0, &mut crate::lut::LutCache::default());
        println!(
            "{:>22}: {:6.1} ms",
            "colour adjust",
            started.elapsed().as_secs_f32() * 1000.0
        );
        println!("all effects one after another: {total:.0} ms");
    }
}
