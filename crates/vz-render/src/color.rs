//! Colour adjustments: exposure, white balance, contrast, tone, saturation, HSL bands, curves, colour wheels, looks
//! (LUTs), vignette, grain and sharpening. One pass over the picture, spread over all processor cores.

use crate::lut::{Lut3, LutCache};
use rayon::prelude::*;
use std::sync::Arc;
use vz_core::{Adjust, Band};
use vz_media::VideoFrame;

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    [h, s.clamp(0.0, 1.0), l]
}

pub fn hsl_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let (h, s, l) = (
        c[0].rem_euclid(360.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
    );
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - chroma / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    [r + m, g + m, b + m]
}

/// A cheap repeatable "random" number from 0 to 1 for grain.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut n =
        x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    n = n.wrapping_mul(0x297A_2D39);
    n ^= n >> 15;
    (n & 0xFFFF) as f32 / 65535.0
}

/// Everything worked out once, so each pixel only does the arithmetic.
struct Prepared {
    exposure: f32,
    warm: f32,
    tint: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    brilliance: f32,
    saturation: f32,
    vibrance: f32,
    hsl: Option<[[f32; 3]; 8]>,
    fade: f32,
    curves: Option<Box<[[f32; 256]; 3]>>,
    wheels: Option<Wheels>,
    lut: Option<(Arc<Lut3>, f32)>,
    vignette: f32,
    grain: f32,
}

/// Wheels turned into the numbers the pixel loop uses: out = (gain * (x + lift * (1 - x)))^(1/gamma) + offset.
struct Wheels {
    lift: [f32; 3],
    gain: [f32; 3],
    inv_gamma: [f32; 3],
    offset: [f32; 3],
}

fn wheel_values(w: &vz_core::Wheel) -> [f32; 3] {
    [w.red + w.luma, w.green + w.luma, w.blue + w.luma]
}

impl Prepared {
    fn new(adj: &Adjust, luts: &mut LutCache) -> Self {
        let curves = (!adj.curves.is_identity()).then(|| {
            let mut t = Box::new([[0.0f32; 256]; 3]);
            for i in 0..256 {
                let x = i as f32 / 255.0;
                let m = adj.curves.master.value(x);
                t[0][i] = adj.curves.red.value(m);
                t[1][i] = adj.curves.green.value(m);
                t[2][i] = adj.curves.blue.value(m);
            }
            t
        });
        let wheels = (!adj.wheels.is_zero()).then(|| {
            let (l, ga, g, o) = (
                wheel_values(&adj.wheels.lift),
                wheel_values(&adj.wheels.gamma),
                wheel_values(&adj.wheels.gain),
                wheel_values(&adj.wheels.offset),
            );
            Wheels {
                lift: l.map(|v| v * 0.3),
                gain: g.map(|v| 1.0 + v * 0.6),
                inv_gamma: ga.map(|v| 1.0 / (1.0 + v * 0.6).clamp(0.3, 3.0)),
                offset: o.map(|v| v * 0.4),
            }
        });
        let hsl = adj
            .hsl
            .iter()
            .any(|b| b.hue != 0.0 || b.saturation != 0.0 || b.lightness != 0.0)
            .then(|| {
                let mut out = [[0.0; 3]; 8];
                for (o, b) in out.iter_mut().zip(&adj.hsl) {
                    *o = [
                        b.hue / 100.0 * 30.0,
                        b.saturation / 100.0,
                        b.lightness / 100.0 * 0.25,
                    ];
                }
                out
            });
        let lut = adj
            .lut
            .as_ref()
            .and_then(|r| luts.get(&r.source).map(|t| (t, r.strength.clamp(0.0, 1.0))));
        Self {
            exposure: 2.0f32.powf(adj.exposure / 100.0 * 2.0),
            warm: adj.temperature / 100.0 * 0.12,
            tint: adj.tint / 100.0 * 0.08,
            brightness: adj.brightness / 100.0 * 0.5,
            contrast: 1.0 + adj.contrast / 100.0 * 0.8,
            highlights: adj.highlights / 100.0 * 0.35,
            shadows: adj.shadows / 100.0 * 0.35,
            whites: adj.whites / 100.0 * 0.25,
            blacks: adj.blacks / 100.0 * 0.25,
            brilliance: adj.brilliance / 100.0,
            saturation: adj.saturation / 100.0,
            vibrance: adj.vibrance / 100.0,
            hsl,
            fade: adj.fade / 100.0 * 0.3,
            curves,
            wheels,
            lut,
            vignette: adj.vignette / 100.0,
            grain: adj.grain / 100.0 * 0.25,
        }
    }

    /// One pixel (`c` in 0..1) at position (`u`, `v`) from -1 to 1 across the picture.
    fn pixel(&self, mut c: [f32; 3], u: f32, v: f32, px: (u32, u32), seed: u32) -> [f32; 3] {
        if self.exposure != 1.0 {
            c = c.map(|x| x * self.exposure);
        }
        if self.warm != 0.0 || self.tint != 0.0 {
            c[0] += self.warm + self.tint * 0.5;
            c[2] += -self.warm + self.tint * 0.5;
            c[1] -= self.tint;
        }
        if self.brightness != 0.0 {
            c = c.map(|x| x + self.brightness);
        }
        if self.contrast != 1.0 {
            c = c.map(|x| (x - 0.5) * self.contrast + 0.5);
        }
        if self.highlights != 0.0
            || self.shadows != 0.0
            || self.whites != 0.0
            || self.blacks != 0.0
            || self.brilliance != 0.0
        {
            let l = luma(c).clamp(0.0, 1.0);
            let dark = (1.0 - l).powi(2);
            let bright = l * l;
            let delta = self.shadows * dark
                + self.highlights * bright
                + self.whites * l.powi(4)
                + self.blacks * (1.0 - l).powi(4)
                + self.brilliance * (0.22 * dark - 0.12 * bright);
            c = c.map(|x| x + delta);
        }
        if self.saturation != 0.0 || self.vibrance != 0.0 {
            let l = luma(c);
            let chroma = c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
            let k =
                (1.0 + self.saturation) * (1.0 + self.vibrance * (1.0 - chroma.clamp(0.0, 1.0)));
            c = c.map(|x| l + (x - l) * k.max(0.0));
        }
        if let Some(bands) = &self.hsl {
            let clamped = c.map(|x| x.clamp(0.0, 1.0));
            let mut hsl = rgb_to_hsl(clamped);
            if hsl[1] > 0.01 {
                let (mut dh, mut ds, mut dl) = (0.0, 0.0, 0.0);
                for (band, d) in Band::ALL.iter().zip(bands) {
                    let mut dist = (hsl[0] - band.centre()).abs();
                    if dist > 180.0 {
                        dist = 360.0 - dist;
                    }
                    let w = (1.0 - dist / 45.0).max(0.0) * (hsl[1] * 4.0).min(1.0);
                    dh += d[0] * w;
                    ds += d[1] * w;
                    dl += d[2] * w;
                }
                hsl[0] += dh;
                hsl[1] = (hsl[1] * (1.0 + ds)).clamp(0.0, 1.0);
                hsl[2] = (hsl[2] + dl).clamp(0.0, 1.0);
                c = hsl_to_rgb(hsl);
            }
        }
        if self.fade != 0.0 {
            c = c.map(|x| x * (1.0 - self.fade) + self.fade * 0.8);
        }
        if let Some(t) = &self.curves {
            for i in 0..3 {
                let idx = (c[i].clamp(0.0, 1.0) * 255.0).round() as usize;
                c[i] = t[i][idx];
            }
        }
        if let Some(w) = &self.wheels {
            for (i, v) in c.iter_mut().enumerate() {
                let x = v.clamp(0.0, 1.0);
                let lifted = x + w.lift[i] * (1.0 - x);
                let gained = (lifted * w.gain[i]).max(0.0);
                *v = gained.powf(w.inv_gamma[i]) + w.offset[i];
            }
        }
        if let Some((lut, strength)) = &self.lut {
            let looked = lut.sample(c);
            c = [
                c[0] + (looked[0] - c[0]) * strength,
                c[1] + (looked[1] - c[1]) * strength,
                c[2] + (looked[2] - c[2]) * strength,
            ];
        }
        if self.vignette != 0.0 {
            let d = (u * u + v * v).sqrt() / std::f32::consts::SQRT_2;
            let edge = ((d - 0.35) / 0.65).clamp(0.0, 1.0);
            let k = 1.0 - self.vignette * edge * edge * 1.1;
            c = c.map(|x| {
                x * k.max(0.0)
                    + if self.vignette < 0.0 {
                        -self.vignette * edge * edge * 0.6
                    } else {
                        0.0
                    }
            });
        }
        if self.grain != 0.0 {
            let n = (noise(px.0, px.1, seed) - 0.5) * self.grain;
            c = c.map(|x| x + n);
        }
        c
    }
}

/// Apply the adjustments to a picture in place (straight RGBA; transparency is kept).
/// `seed` makes the film grain different on every frame.
pub fn apply(frame: &mut VideoFrame, adj: &Adjust, seed: u32, luts: &mut LutCache) {
    if adj.is_identity() || frame.width == 0 || frame.height == 0 {
        return;
    }
    let prep = Prepared::new(adj, luts);
    let (w, h) = (frame.width as usize, frame.height as usize);
    frame
        .rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            let v = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            for (x, p) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if p[3] == 0 {
                    continue;
                }
                let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                let c = [
                    f32::from(p[0]) / 255.0,
                    f32::from(p[1]) / 255.0,
                    f32::from(p[2]) / 255.0,
                ];
                let o = prep.pixel(c, u, v, (x as u32, y as u32), seed);
                p[0] = (o[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                p[1] = (o[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                p[2] = (o[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        });
    if adj.sharpen != 0.0 {
        unsharp(frame, 1.0, adj.sharpen / 100.0 * 1.5);
    }
    if adj.clarity != 0.0 {
        let radius = (w.max(h) as f32 / 160.0).max(2.0);
        unsharp(frame, radius, adj.clarity / 100.0 * 0.9);
    }
}

/// Add (or take away, if `amount` is negative) the difference between the picture and a softened copy of it.
pub(crate) fn unsharp(frame: &mut VideoFrame, sigma: f32, amount: f32) {
    let soft = crate::blurred(&crate::composite::Image::from(&*frame), sigma);
    frame
        .rgba
        .par_chunks_mut(4)
        .zip(soft.rgba.par_chunks(4))
        .for_each(|(p, s)| {
            for i in 0..3 {
                let v = f32::from(p[i]) + (f32::from(p[i]) - f32::from(s[i])) * amount;
                p[i] = v.clamp(0.0, 255.0) as u8;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{Curve, LutRef, LutSource, Wheel};

    fn solid(rgb: [u8; 3]) -> VideoFrame {
        let mut f = VideoFrame::black(16, 16);
        for p in f.rgba.as_chunks_mut::<4>().0 {
            *p = [rgb[0], rgb[1], rgb[2], 255];
        }
        f
    }

    fn graded(rgb: [u8; 3], f: impl FnOnce(&mut Adjust)) -> [u8; 3] {
        let mut frame = solid(rgb);
        let mut adj = Adjust::default();
        f(&mut adj);
        apply(&mut frame, &adj, 0, &mut LutCache::default());
        let p = frame.rgba[8 * 16 * 4 + 8 * 4..][..3].to_vec();
        [p[0], p[1], p[2]]
    }

    #[test]
    fn nothing_set_changes_nothing() {
        assert_eq!(graded([10, 120, 250], |_| {}), [10, 120, 250]);
    }

    #[test]
    fn hsl_conversion_round_trips() {
        for c in [
            [0.2, 0.5, 0.8],
            [1.0, 0.0, 0.0],
            [0.3, 0.3, 0.3],
            [0.9, 0.7, 0.1],
        ] {
            let back = hsl_to_rgb(rgb_to_hsl(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-4, "{c:?} {back:?}");
            }
        }
    }

    #[test]
    fn exposure_and_brightness_brighten_and_darken() {
        let base = [100, 100, 100];
        assert!(graded(base, |a| a.exposure = 50.0)[0] > 150);
        assert!(graded(base, |a| a.exposure = -50.0)[0] < 60);
        assert!(graded(base, |a| a.brightness = 40.0)[0] > 130);
    }

    #[test]
    fn contrast_pushes_tones_apart() {
        assert!(graded([180, 180, 180], |a| a.contrast = 60.0)[0] > 200);
        assert!(graded([70, 70, 70], |a| a.contrast = 60.0)[0] < 50);
        assert_eq!(graded([128, 128, 128], |a| a.contrast = 60.0)[0], 128);
    }

    #[test]
    fn warm_is_redder_and_cool_is_bluer() {
        let warm = graded([128, 128, 128], |a| a.temperature = 60.0);
        assert!(warm[0] > warm[2]);
        let cool = graded([128, 128, 128], |a| a.temperature = -60.0);
        assert!(cool[2] > cool[0]);
        let magenta = graded([128, 128, 128], |a| a.tint = 60.0);
        assert!(magenta[1] < magenta[0]);
    }

    #[test]
    fn saturation_zero_to_minus_100_makes_grey_and_plus_makes_colour() {
        let grey = graded([200, 60, 30], |a| a.saturation = -100.0);
        assert!((i32::from(grey[0]) - i32::from(grey[2])).abs() <= 1);
        let vivid = graded([160, 100, 90], |a| a.saturation = 60.0);
        assert!(i32::from(vivid[0]) - i32::from(vivid[2]) > 70);
    }

    #[test]
    fn shadows_and_highlights_work_on_their_own_part_of_the_range() {
        let dark = graded([40, 40, 40], |a| a.shadows = 80.0)[0];
        let bright_unchanged = graded([230, 230, 230], |a| a.shadows = 80.0)[0];
        assert!(
            dark > 60 && bright_unchanged <= 232,
            "{dark} {bright_unchanged}"
        );
        let bright = graded([230, 230, 230], |a| a.highlights = -80.0)[0];
        assert!(bright < 215, "{bright}");
    }

    #[test]
    fn an_hsl_band_only_changes_its_own_colours() {
        // push the greens towards a different hue: red stays, green moves
        let red = graded([220, 40, 40], |a| a.hsl[3].hue = 100.0);
        assert!((i32::from(red[0]) - 220).abs() < 6, "{red:?}");
        let green = graded([40, 200, 40], |a| a.hsl[3].hue = 100.0);
        assert!(green[2] > 80 || green[0] > 80, "{green:?}");
        // desaturate only the blues
        let blue = graded([40, 40, 220], |a| a.hsl[5].saturation = -100.0);
        assert!(
            (i32::from(blue[0]) - i32::from(blue[2])).abs() < 40,
            "{blue:?}"
        );
    }

    #[test]
    fn a_curve_brightens_the_middle_and_leaves_the_ends() {
        let mid = graded([128, 128, 128], |a| {
            a.curves.master.points = vec![(0.0, 0.0), (0.5, 0.7), (1.0, 1.0)]
        });
        assert!(mid[0] > 160);
        let ends = graded([255, 255, 255], |a| {
            a.curves.master.points = vec![(0.0, 0.0), (0.5, 0.7), (1.0, 1.0)]
        });
        assert_eq!(ends[0], 255);
        // a single colour channel curve changes only that channel
        let red_only = graded([128, 128, 128], |a| {
            a.curves.red = Curve {
                points: vec![(0.5, 0.9)],
            }
        });
        assert!(red_only[0] > 200 && red_only[1] == 128);
    }

    #[test]
    fn the_colour_wheels_tint_shadows_midtones_and_highlights() {
        // gain pushes highlights towards red
        let g = graded([200, 200, 200], |a| {
            a.wheels.gain = Wheel {
                red: 0.6,
                ..Wheel::default()
            }
        });
        assert!(g[0] > g[2] + 20, "{g:?}");
        // lift changes the dark parts more than the bright
        let dark = graded([30, 30, 30], |a| {
            a.wheels.lift = Wheel {
                blue: 0.8,
                ..Wheel::default()
            }
        });
        let bright = graded([230, 230, 230], |a| {
            a.wheels.lift = Wheel {
                blue: 0.8,
                ..Wheel::default()
            }
        });
        assert!(
            i32::from(dark[2]) - 30 > i32::from(bright[2]) - 230 + 15,
            "{dark:?} {bright:?}"
        );
        // offset moves everything
        let o = graded([100, 100, 100], |a| {
            a.wheels.offset = Wheel {
                luma: 0.5,
                ..Wheel::default()
            }
        });
        assert!(o[0] > 140);
    }

    #[test]
    fn a_look_is_mixed_in_by_its_strength() {
        let full = graded([128, 128, 128], |a| {
            a.lut = Some(LutRef {
                source: LutSource::Builtin("bleach_bypass".into()),
                strength: 1.0,
            });
        });
        let none = graded([128, 128, 128], |a| {
            a.lut = Some(LutRef {
                source: LutSource::Builtin("faded_film".into()),
                strength: 0.0,
            });
        });
        assert_eq!(none, [128, 128, 128]);
        let faded = graded([20, 20, 20], |a| {
            a.lut = Some(LutRef {
                source: LutSource::Builtin("faded_film".into()),
                strength: 1.0,
            });
        });
        assert!(faded[0] > 40, "blacks are lifted: {faded:?}");
        let _ = full;
    }

    #[test]
    fn a_vignette_darkens_the_corners_not_the_middle() {
        let mut f = solid([200, 200, 200]);
        let adj = Adjust {
            vignette: 80.0,
            ..Adjust::default()
        };
        apply(&mut f, &adj, 0, &mut LutCache::default());
        let at = |x: usize, y: usize| f.rgba[(y * 16 + x) * 4];
        assert_eq!(at(8, 8), 200);
        assert!(at(0, 0) < 140, "{}", at(0, 0));
    }

    #[test]
    fn grain_adds_noise_that_changes_from_frame_to_frame() {
        let adj = Adjust {
            grain: 80.0,
            ..Adjust::default()
        };
        let (mut a, mut b) = (solid([128, 128, 128]), solid([128, 128, 128]));
        apply(&mut a, &adj, 1, &mut LutCache::default());
        apply(&mut b, &adj, 2, &mut LutCache::default());
        assert_ne!(a.rgba, b.rgba);
        assert!(a.rgba.chunks(4).any(|p| p[0] != 128));
        let mean: f32 = a.rgba.chunks(4).map(|p| f32::from(p[0])).sum::<f32>() / 256.0;
        assert!((mean - 128.0).abs() < 8.0, "{mean}");
    }

    #[test]
    fn sharpen_increases_the_difference_at_an_edge() {
        let mut f = VideoFrame::black(32, 8);
        for p in f.rgba.as_chunks_mut::<4>().0.chunks_mut(32) {
            for (x, px) in p.iter_mut().enumerate() {
                *px = if x < 16 {
                    [100, 100, 100, 255]
                } else {
                    [150, 150, 150, 255]
                };
            }
        }
        let before = f.rgba[(4 * 32 + 17) * 4];
        let adj = Adjust {
            sharpen: 100.0,
            ..Adjust::default()
        };
        apply(&mut f, &adj, 0, &mut LutCache::default());
        assert!(
            f.rgba[(4 * 32 + 17) * 4] > before,
            "bright side of the edge gets brighter"
        );
        assert!(f.rgba[(4 * 32 + 14) * 4] < 100, "dark side gets darker");
    }

    #[test]
    fn transparent_pixels_stay_untouched() {
        let mut f = VideoFrame::black(4, 4);
        f.rgba.as_chunks_mut::<4>().0[0] = [10, 20, 30, 0];
        let adj = Adjust {
            exposure: 100.0,
            ..Adjust::default()
        };
        apply(&mut f, &adj, 0, &mut LutCache::default());
        assert_eq!(&f.rgba[..4], &[10, 20, 30, 0]);
    }
}
