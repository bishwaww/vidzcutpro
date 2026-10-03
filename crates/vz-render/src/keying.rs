//! Masks (only a shape of the picture stays) and chroma key (a colour such as a green screen is removed).
//! Both work by changing the transparency of the picture.

use rayon::prelude::*;
use std::f32::consts::{PI, TAU};
use vz_core::{ChromaKey, MaskDef, MaskShape};
use vz_media::VideoFrame;

/// How far `p` (pixels from the middle of the shape, already turned back upright) is from the edge of the shape:
/// negative inside, positive outside, in pixels (roughly, for the curvy shapes).
fn distance(shape: MaskShape, p: (f32, f32), hw: f32, hh: f32, corner: f32) -> f32 {
    let small = hw.min(hh);
    match shape {
        MaskShape::Linear => p.1,
        MaskShape::Mirror => p.1.abs() - hh,
        MaskShape::Circle => {
            let q = (p.0 / hw.max(1e-3), p.1 / hh.max(1e-3));
            ((q.0 * q.0 + q.1 * q.1).sqrt() - 1.0) * small
        }
        MaskShape::Rectangle => {
            let r = (corner * small).clamp(0.0, small);
            let q = (p.0.abs() - (hw - r), p.1.abs() - (hh - r));
            let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
            outside + q.0.max(q.1).min(0.0) - r
        }
        MaskShape::Heart => {
            let (x, y) = (p.0 / hw.max(1e-3), -p.1 / hh.max(1e-3) * 1.15 + 0.2);
            // the classic heart curve: inside where this is negative
            let f = (x * x + y * y - 1.0).powi(3) - x * x * y * y * y;
            f.signum() * f.abs().cbrt() * small * 0.45
        }
        MaskShape::Star => {
            let (x, y) = (p.0 / hw.max(1e-3), p.1 / hh.max(1e-3));
            let r = (x * x + y * y).sqrt();
            let sector = TAU / 5.0;
            // the angle away from the nearest tip (the first tip points up)
            let s = ((y.atan2(x) + PI / 2.0).rem_euclid(sector) - sector / 2.0).abs();
            // the outline between a tip (radius 1) and the next valley (radius 0.45), seen along this direction
            let (vx, vy) = (0.45 * (sector / 2.0).cos(), 0.45 * (sector / 2.0).sin());
            let e = (vx - 1.0, vy);
            let denom = s.cos() * e.1 - s.sin() * e.0;
            let boundary = (e.1 / denom.max(1e-4)).clamp(0.2, 1.0);
            (r - boundary) * small
        }
    }
}

/// Multiply the transparency of the picture by the masks (a pixel must be inside all of them to stay).
pub fn apply_masks(frame: &mut VideoFrame, masks: &[MaskDef]) {
    if masks.is_empty() || frame.width == 0 || frame.height == 0 {
        return;
    }
    let (w, h) = (frame.width as usize, frame.height as usize);
    let (wf, hf) = (w as f32, h as f32);
    let prepared: Vec<_> = masks
        .iter()
        .map(|m| {
            let (sin, cos) = m.rotation.to_radians().sin_cos();
            let (hw, hh) = (m.width.max(0.01) * wf / 2.0, m.height.max(0.01) * hf / 2.0);
            let band = (m.feather * hw.min(hh) * 0.5).max(1.0);
            (m, sin, cos, hw, hh, band)
        })
        .collect();
    frame
        .rgba
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, p) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                if p[3] == 0 {
                    continue;
                }
                let mut keep = 1.0f32;
                for (m, sin, cos, hw, hh, band) in &prepared {
                    // pixel relative to the middle of the shape, turned back upright
                    let dx = x as f32 + 0.5 - wf / 2.0 - m.x * wf;
                    let dy = y as f32 + 0.5 - hf / 2.0 - m.y * hf;
                    let q = (dx * cos + dy * sin, -dx * sin + dy * cos);
                    let d = distance(m.shape, q, *hw, *hh, m.corner);
                    let inside = (0.5 - d / band).clamp(0.0, 1.0);
                    keep *= if m.invert { 1.0 - inside } else { inside };
                    if keep <= 0.0 {
                        break;
                    }
                }
                p[3] = (f32::from(p[3]) * keep) as u8;
            }
        });
}

fn chroma(c: [f32; 3]) -> (f32, f32) {
    // the colour part of a pixel (Cb, Cr), ignoring brightness
    (
        -0.1146 * c[0] - 0.3854 * c[1] + 0.5 * c[2],
        0.5 * c[0] - 0.4542 * c[1] - 0.0458 * c[2],
    )
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Remove the key colour: make it transparent, soften the edge, keep shadows, and take colour spill out of what is left.
pub fn apply_chroma(frame: &mut VideoFrame, key: &ChromaKey) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w == 0 || h == 0 {
        return;
    }
    let kc = [
        f32::from(key.color[0]) / 255.0,
        f32::from(key.color[1]) / 255.0,
        f32::from(key.color[2]) / 255.0,
    ];
    let (kcb, kcr) = chroma(kc);
    let key_luma = (0.2126 * kc[0] + 0.7152 * kc[1] + 0.0722 * kc[2]).max(0.2);
    let tol = key.intensity / 100.0 * 0.45;
    let soft = (key.feather / 100.0 * 0.3).max(0.01);
    let shadow = key.shadow / 100.0;
    let spill = key.spill / 100.0;
    // which channel dominates the key colour (for green and blue screens, that is where spill shows)
    let dominant = if kc[1] >= kc[0] && kc[1] >= kc[2] {
        1
    } else if kc[2] >= kc[0] {
        2
    } else {
        0
    };
    frame.rgba.par_chunks_mut(w * 4).for_each(|line| {
        for p in line.as_chunks_mut::<4>().0.iter_mut() {
            if p[3] == 0 {
                continue;
            }
            let mut c = [
                f32::from(p[0]) / 255.0,
                f32::from(p[1]) / 255.0,
                f32::from(p[2]) / 255.0,
            ];
            // colour only: a darker version of the same colour (a shadow) has the same colour
            let top = c[0].max(c[1]).max(c[2]).max(0.05);
            let (cb, cr) = chroma(c.map(|v| v / top));
            let d = ((cb - kcb).powi(2) + (cr - kcr).powi(2)).sqrt();
            let mut alpha = smoothstep(tol, tol + soft, d);
            if alpha < 1.0 && shadow > 0.0 {
                // a darker version of the key colour is a shadow on the screen: keep it as a soft dark layer
                let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                let darkness = (1.0 - l / key_luma).clamp(0.0, 1.0);
                let shade = darkness * shadow * 0.85 * (1.0 - alpha);
                if shade > alpha {
                    alpha = shade;
                    c = [0.0; 3];
                }
            }
            if alpha > 0.0 && spill > 0.0 {
                let others: Vec<f32> = (0..3).filter(|i| *i != dominant).map(|i| c[i]).collect();
                let limit = (others[0] + others[1]) / 2.0;
                if c[dominant] > limit {
                    c[dominant] -= (c[dominant] - limit) * spill;
                }
            }
            p[0] = (c[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            p[1] = (c[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            p[2] = (c[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            p[3] = (f32::from(p[3]) * alpha) as u8;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> VideoFrame {
        let mut f = VideoFrame::black(w, h);
        for p in f.rgba.as_chunks_mut::<4>().0 {
            *p = [rgb[0], rgb[1], rgb[2], 255];
        }
        f
    }

    fn alpha(f: &VideoFrame, x: usize, y: usize) -> u8 {
        f.rgba[(y * f.width as usize + x) * 4 + 3]
    }

    fn masked(m: MaskDef) -> VideoFrame {
        let mut f = solid(100, 100, [200, 100, 50]);
        apply_masks(&mut f, &[m]);
        f
    }

    #[test]
    fn a_circle_keeps_the_middle_and_cuts_the_corners() {
        let f = masked(MaskDef::new(MaskShape::Circle));
        assert_eq!(alpha(&f, 50, 50), 255);
        assert_eq!(alpha(&f, 2, 2), 0);
        assert_eq!(alpha(&f, 97, 97), 0);
        // the circle is 60 % of the picture across: 50 +- 30
        assert_eq!(alpha(&f, 50, 75), 255);
        assert_eq!(alpha(&f, 50, 90), 0);
    }

    #[test]
    fn a_rectangle_can_be_moved_and_turned_and_inverted() {
        let mut m = MaskDef::new(MaskShape::Rectangle);
        m.x = 0.25;
        m.width = 0.3;
        m.height = 0.3;
        let f = masked(m.clone());
        assert_eq!(alpha(&f, 75, 50), 255);
        assert_eq!(alpha(&f, 25, 50), 0);
        m.invert = true;
        let f = masked(m.clone());
        assert_eq!(alpha(&f, 75, 50), 0);
        assert_eq!(alpha(&f, 25, 50), 255);
        // turned 90 degrees, a wide rectangle becomes tall
        let mut m = MaskDef::new(MaskShape::Rectangle);
        m.width = 0.8;
        m.height = 0.2;
        m.rotation = 90.0;
        let f = masked(m);
        assert_eq!(alpha(&f, 50, 15), 255, "tall now");
        assert_eq!(alpha(&f, 15, 50), 0);
    }

    #[test]
    fn rounded_corners_cut_the_corner_of_a_rectangle() {
        let mut m = MaskDef::new(MaskShape::Rectangle);
        m.width = 0.8;
        m.height = 0.8;
        m.corner = 1.0;
        let f = masked(m);
        assert_eq!(alpha(&f, 50, 50), 255);
        assert_eq!(
            alpha(&f, 11, 11),
            0,
            "the corner of the box is rounded away"
        );
    }

    #[test]
    fn a_linear_mask_splits_the_picture_and_a_mirror_mask_keeps_a_band() {
        let linear = masked(MaskDef::new(MaskShape::Linear));
        assert_eq!(alpha(&linear, 50, 10), 255, "above the line stays");
        assert_eq!(alpha(&linear, 50, 90), 0);
        let mut m = MaskDef::new(MaskShape::Mirror);
        m.height = 0.4;
        let band = masked(m);
        assert_eq!(alpha(&band, 50, 50), 255);
        assert_eq!(alpha(&band, 50, 10), 0);
        assert_eq!(alpha(&band, 50, 90), 0);
    }

    #[test]
    fn feathering_makes_a_soft_edge() {
        let mut m = MaskDef::new(MaskShape::Circle);
        m.feather = 0.8;
        let f = masked(m);
        let soft = (0..100)
            .filter(|x| (1..255).contains(&alpha(&f, *x, 50)))
            .count();
        assert!(soft > 8, "{soft} soft pixels across the middle row");
    }

    #[test]
    fn heart_and_star_keep_their_middle_and_lose_the_far_corners() {
        for shape in [MaskShape::Heart, MaskShape::Star] {
            let mut m = MaskDef::new(shape);
            m.width = 0.8;
            m.height = 0.8;
            let f = masked(m);
            assert_eq!(alpha(&f, 50, 50), 255, "{shape:?}");
            assert_eq!(alpha(&f, 1, 1), 0, "{shape:?}");
            assert_eq!(alpha(&f, 98, 98), 0, "{shape:?}");
            let covered = f.rgba.chunks(4).filter(|p| p[3] == 255).count();
            assert!(
                covered > 1500 && covered < 7000,
                "{shape:?} covers {covered}"
            );
        }
    }

    #[test]
    fn several_masks_must_all_agree_and_none_means_no_change() {
        let mut f = solid(100, 100, [10, 10, 10]);
        apply_masks(&mut f, &[]);
        assert_eq!(alpha(&f, 0, 0), 255);
        let mut a = MaskDef::new(MaskShape::Rectangle);
        a.x = -0.2;
        let mut b = MaskDef::new(MaskShape::Rectangle);
        b.x = 0.2;
        apply_masks(&mut f, &[a, b]);
        assert_eq!(alpha(&f, 50, 50), 255, "the overlap stays");
        assert_eq!(alpha(&f, 25, 50), 0, "only in the first one");
    }

    fn keyed(pixel: [u8; 3], tweak: impl FnOnce(&mut ChromaKey)) -> (u8, [u8; 3]) {
        let mut f = solid(4, 4, pixel);
        let mut key = ChromaKey::green();
        tweak(&mut key);
        apply_chroma(&mut f, &key);
        (f.rgba[3], [f.rgba[0], f.rgba[1], f.rgba[2]])
    }

    #[test]
    fn the_key_colour_disappears_and_other_colours_stay() {
        assert_eq!(keyed([0, 255, 0], |_| {}).0, 0);
        assert!(
            keyed([20, 230, 30], |_| {}).0 <= 4,
            "a slightly different green too"
        );
        assert_eq!(keyed([200, 120, 90], |_| {}).0, 255, "skin tone stays");
        assert_eq!(keyed([30, 40, 220], |_| {}).0, 255, "blue stays");
    }

    #[test]
    fn intensity_decides_how_far_from_the_key_colour_still_disappears() {
        let near_green = [90, 200, 90];
        let strict = keyed(near_green, |k| k.intensity = 5.0).0;
        let loose = keyed(near_green, |k| k.intensity = 90.0).0;
        assert!(strict > loose, "{strict} {loose}");
    }

    #[test]
    fn feather_softens_the_edge() {
        let in_between = [100, 190, 100];
        let hard = keyed(in_between, |k| {
            k.feather = 0.0;
            k.intensity = 30.0;
        })
        .0;
        let soft = keyed(in_between, |k| {
            k.feather = 100.0;
            k.intensity = 30.0;
        })
        .0;
        assert!(
            soft < 255 && soft > 0 || hard == 255 || hard == 0,
            "{hard} {soft}"
        );
        assert!(i32::from(soft) != i32::from(hard) || hard == 0 || hard == 255);
    }

    #[test]
    fn spill_suppression_removes_green_from_what_is_left() {
        let hair = [180, 200, 170]; // greenish light on a light colour
        let (a_none, c_none) = keyed(hair, |k| {
            k.spill = 0.0;
            k.intensity = 10.0;
        });
        let (a_spill, c_spill) = keyed(hair, |k| {
            k.spill = 100.0;
            k.intensity = 10.0;
        });
        assert!(a_none == 255 && a_spill == 255);
        assert!(c_spill[1] < c_none[1], "{c_spill:?} vs {c_none:?}");
    }

    #[test]
    fn a_dark_green_shadow_can_be_kept_as_a_soft_shadow() {
        let dark_green = [0, 90, 0];
        let (without, _) = keyed(dark_green, |k| k.shadow = 0.0);
        let (with, colour) = keyed(dark_green, |k| k.shadow = 100.0);
        assert!(with > without, "{with} {without}");
        assert_eq!(colour, [0, 0, 0], "the shadow is dark, not green");
    }
}
