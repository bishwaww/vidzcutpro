//! The 18 blend modes, following the standard (W3C compositing) formulas.
//!
//! Colours here are `[red, green, blue]` with each value from 0.0 to 1.0.

use vz_core::BlendMode;

type Rgb = [f32; 3];

fn lum(c: Rgb) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: Rgb) -> Rgb {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        for v in &mut out {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut out {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    out
}

fn set_lum(c: Rgb, l: f32) -> Rgb {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn sat(c: Rgb) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: Rgb, s: f32) -> Rgb {
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|a, b| c[*a].total_cmp(&c[*b]));
    let (min, mid, max) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0; 3];
    if c[max] > c[min] {
        out[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        out[max] = s;
    }
    out
}

fn separable(mode: BlendMode, b: f32, s: f32) -> f32 {
    match mode {
        BlendMode::Normal => s,
        BlendMode::Darken => b.min(s),
        BlendMode::Multiply => b * s,
        BlendMode::ColorBurn => {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        BlendMode::LinearBurn => (b + s - 1.0).max(0.0),
        BlendMode::Lighten => b.max(s),
        BlendMode::Screen => b + s - b * s,
        BlendMode::ColorDodge => {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        BlendMode::Add => (b + s).min(1.0),
        BlendMode::Overlay => separable(BlendMode::HardLight, s, b),
        BlendMode::HardLight => {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                let s2 = 2.0 * s - 1.0;
                b + s2 - b * s2
            }
        }
        BlendMode::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        // the non-separable modes are handled in `blend`
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => s,
    }
}

/// Mix a source colour over a backdrop colour using `mode` (before opacity is applied).
pub fn blend(mode: BlendMode, backdrop: Rgb, source: Rgb) -> Rgb {
    match mode {
        BlendMode::Normal => source,
        BlendMode::Hue => set_lum(set_sat(source, sat(backdrop)), lum(backdrop)),
        BlendMode::Saturation => set_lum(set_sat(backdrop, sat(source)), lum(backdrop)),
        BlendMode::Color => set_lum(source, lum(backdrop)),
        BlendMode::Luminosity => set_lum(backdrop, lum(source)),
        _ => [
            separable(mode, backdrop[0], source[0]),
            separable(mode, backdrop[1], source[1]),
            separable(mode, backdrop[2], source[2]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: Rgb = [0.5, 0.5, 0.5];

    fn close(a: Rgb, b: Rgb) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn normal_gives_the_source() {
        assert_eq!(
            blend(BlendMode::Normal, [0.1, 0.2, 0.3], [0.9, 0.8, 0.7]),
            [0.9, 0.8, 0.7]
        );
    }

    #[test]
    fn simple_modes_match_their_definitions() {
        let (b, s) = ([0.8, 0.4, 0.2], [0.5, 0.5, 0.5]);
        assert!(close(blend(BlendMode::Multiply, b, s), [0.4, 0.2, 0.1]));
        assert!(close(blend(BlendMode::Screen, b, s), [0.9, 0.7, 0.6]));
        assert!(close(blend(BlendMode::Darken, b, s), [0.5, 0.4, 0.2]));
        assert!(close(blend(BlendMode::Lighten, b, s), [0.8, 0.5, 0.5]));
        assert!(close(blend(BlendMode::Difference, b, s), [0.3, 0.1, 0.3]));
        assert!(close(blend(BlendMode::Exclusion, b, s), [0.5, 0.5, 0.5]));
        assert!(close(blend(BlendMode::Add, b, s), [1.0, 0.9, 0.7]));
        assert!(close(blend(BlendMode::LinearBurn, b, s), [0.3, 0.0, 0.0]));
    }

    #[test]
    fn black_and_white_are_special_for_multiply_and_screen() {
        for c in [[0.2, 0.5, 0.9], [0.0, 1.0, 0.5]] {
            assert!(
                close(blend(BlendMode::Multiply, c, [1.0; 3]), c),
                "multiply by white keeps it"
            );
            assert!(
                close(blend(BlendMode::Multiply, c, [0.0; 3]), [0.0; 3]),
                "multiply by black is black"
            );
            assert!(
                close(blend(BlendMode::Screen, c, [0.0; 3]), c),
                "screen with black keeps it"
            );
            assert!(
                close(blend(BlendMode::Screen, c, [1.0; 3]), [1.0; 3]),
                "screen with white is white"
            );
        }
    }

    #[test]
    fn dodge_and_burn_handle_the_edges_without_dividing_by_zero() {
        assert_eq!(blend(BlendMode::ColorDodge, [0.0; 3], [1.0; 3]), [0.0; 3]);
        assert_eq!(blend(BlendMode::ColorDodge, [0.5; 3], [1.0; 3]), [1.0; 3]);
        assert_eq!(blend(BlendMode::ColorBurn, [1.0; 3], [0.0; 3]), [1.0; 3]);
        assert_eq!(blend(BlendMode::ColorBurn, [0.5; 3], [0.0; 3]), [0.0; 3]);
    }

    #[test]
    fn overlay_and_hard_light_are_each_others_mirror() {
        let (b, s) = ([0.3, 0.6, 0.9], [0.2, 0.7, 0.4]);
        assert!(close(
            blend(BlendMode::Overlay, b, s),
            blend(BlendMode::HardLight, s, b)
        ));
        // overlay keeps mid grey as it is
        assert!(close(blend(BlendMode::Overlay, GREY, GREY), GREY));
    }

    #[test]
    fn soft_light_with_grey_changes_nothing() {
        for b in [0.1, 0.3, 0.5, 0.8] {
            let r = blend(BlendMode::SoftLight, [b; 3], GREY);
            assert!(close(r, [b; 3]), "{b}");
        }
    }

    #[test]
    fn every_mode_stays_inside_zero_to_one_and_never_gives_nan() {
        let values = [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
        for mode in BlendMode::ALL {
            for &r in &values {
                for &g in &values {
                    for &sb in &values {
                        let b = [r, g, 0.3];
                        let s = [sb, 1.0 - r, g];
                        let out = blend(mode, b, s);
                        for v in out {
                            assert!(
                                v.is_finite() && (-1e-4..=1.0001).contains(&v),
                                "{mode:?} gave {v}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn non_separable_modes_keep_the_right_ingredients() {
        // Luminosity: the backdrop's hue and saturation with the source's brightness
        let b = [0.9, 0.2, 0.2];
        let white = blend(BlendMode::Luminosity, b, [1.0; 3]);
        assert!(close(white, [1.0; 3]), "full brightness clips to white");
        // Color: the source's colour with the backdrop's brightness
        let out = blend(BlendMode::Color, [0.5; 3], [1.0, 0.0, 0.0]);
        assert!((lum(out) - 0.5).abs() < 1e-3);
        assert!(out[0] > out[1] && out[0] > out[2], "still red");
        // Saturation: grey source has no saturation, so the result is grey
        let out = blend(BlendMode::Saturation, [0.9, 0.2, 0.2], [0.4; 3]);
        assert!(sat(out) < 1e-4);
        // Hue: a grey backdrop gives a grey result (no saturation to apply)
        let out = blend(BlendMode::Hue, [0.5; 3], [1.0, 0.0, 0.0]);
        assert!(sat(out) < 1e-4);
    }
}
