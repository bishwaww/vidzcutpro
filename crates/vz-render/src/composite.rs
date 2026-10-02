//! Drawing one layer onto the canvas: move, scale, turn, flip, crop, fade and blend.
//!
//! Every canvas pixel looks backwards into the layer's picture ("inverse mapping") and takes a smooth
//! (bilinear) sample, so edges are soft and nothing leaves gaps. Rows are drawn in parallel.

use crate::blend::blend;
use rayon::prelude::*;
use vz_core::{BlendMode, Crop};
use vz_media::VideoFrame;

/// Where a layer sits on the canvas, in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Where the middle of the layer is.
    pub center: (f32, f32),
    /// The size of the picture before `scale` is applied.
    pub size: (f32, f32),
    pub scale: (f32, f32),
    /// Degrees, clockwise.
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// Draw `src` onto `canvas`. The canvas must be opaque (every pixel's alpha 255), which it always is here.
pub fn draw(
    canvas: &mut VideoFrame,
    src: &VideoFrame,
    p: &Placement,
    crop: Crop,
    opacity: f32,
    mode: BlendMode,
) {
    let (cw, ch) = (canvas.width as usize, canvas.height as usize);
    let (sw, sh) = (src.width as usize, src.height as usize);
    let opacity = opacity.clamp(0.0, 1.0);
    if cw == 0
        || ch == 0
        || sw == 0
        || sh == 0
        || opacity <= 0.0
        || p.scale.0 == 0.0
        || p.scale.1 == 0.0
        || p.size.0 <= 0.0
        || p.size.1 <= 0.0
    {
        return;
    }
    let (w, h) = (p.size.0 * p.scale.0.abs(), p.size.1 * p.scale.1.abs());
    let theta = p.rotation.to_radians();
    let (sin, cos) = theta.sin_cos();

    // the part of the canvas the layer can touch
    let corners = [
        (-w / 2.0, -h / 2.0),
        (w / 2.0, -h / 2.0),
        (w / 2.0, h / 2.0),
        (-w / 2.0, h / 2.0),
    ];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        let (rx, ry) = (
            p.center.0 + x * cos - y * sin,
            p.center.1 + x * sin + y * cos,
        );
        min_x = min_x.min(rx);
        max_x = max_x.max(rx);
        min_y = min_y.min(ry);
        max_y = max_y.max(ry);
    }
    let x0 = ((min_x - 1.0).floor().max(0.0)) as usize;
    let y0 = ((min_y - 1.0).floor().max(0.0)) as usize;
    let x1 = (((max_x + 1.0).ceil()).min(cw as f32)).max(0.0) as usize;
    let y1 = (((max_y + 1.0).ceil()).min(ch as f32)).max(0.0) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    // canvas offset -> source pixel: turn back, undo the scale and flips, then stretch to the source size
    let flip_x = if p.flip_h { -1.0 } else { 1.0 };
    let flip_y = if p.flip_v { -1.0 } else { 1.0 };
    let to_u = sw as f32 / (p.scale.0.abs() * p.size.0) * flip_x;
    let to_v = sh as f32 / (p.scale.1.abs() * p.size.1) * flip_y;
    // how many source pixels one canvas pixel covers (for soft edges)
    let (src_per_px_x, src_per_px_y) = (to_u.abs(), to_v.abs());
    let (left, right) = (crop.left * sw as f32, (1.0 - crop.right) * sw as f32);
    let (top, bottom) = (crop.top * sh as f32, (1.0 - crop.bottom) * sh as f32);
    let src_data = &src.rgba;
    let (sw_max, sh_max) = ((sw - 1) as f32, (sh - 1) as f32);

    let rows = &mut canvas.rgba[y0 * cw * 4..y1 * cw * 4];
    rows.par_chunks_mut(cw * 4)
        .enumerate()
        .for_each(|(row, line)| {
            let py = (y0 + row) as f32 + 0.5 - p.center.1;
            for x in x0..x1 {
                let px = x as f32 + 0.5 - p.center.0;
                let (dx, dy) = (px * cos + py * sin, -px * sin + py * cos);
                let (u, v) = (dx * to_u + sw as f32 / 2.0, dy * to_v + sh as f32 / 2.0);

                // how much of this canvas pixel is inside the (cropped) picture
                let cover = (soft(u - left, src_per_px_x)
                    * soft(right - u, src_per_px_x)
                    * soft(v - top, src_per_px_y)
                    * soft(bottom - v, src_per_px_y))
                .clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }

                // smooth sample of the picture (colours weighted by their alpha)
                let (fx, fy) = ((u - 0.5).clamp(0.0, sw_max), (v - 0.5).clamp(0.0, sh_max));
                let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
                let (tx, ty) = (fx - ix as f32, fy - iy as f32);
                let (ix2, iy2) = ((ix + 1).min(sw - 1), (iy + 1).min(sh - 1));
                let mut acc = [0.0f32; 3];
                let mut alpha = 0.0f32;
                for (xi, yi, wgt) in [
                    (ix, iy, (1.0 - tx) * (1.0 - ty)),
                    (ix2, iy, tx * (1.0 - ty)),
                    (ix, iy2, (1.0 - tx) * ty),
                    (ix2, iy2, tx * ty),
                ] {
                    let i = (yi * sw + xi) * 4;
                    let a = f32::from(src_data[i + 3]) / 255.0 * wgt;
                    acc[0] += f32::from(src_data[i]) * a;
                    acc[1] += f32::from(src_data[i + 1]) * a;
                    acc[2] += f32::from(src_data[i + 2]) * a;
                    alpha += a;
                }
                if alpha <= 0.0 {
                    continue;
                }
                let source = [
                    acc[0] / alpha / 255.0,
                    acc[1] / alpha / 255.0,
                    acc[2] / alpha / 255.0,
                ];
                let a = alpha * cover * opacity;

                let o = x * 4;
                let backdrop = [
                    f32::from(line[o]) / 255.0,
                    f32::from(line[o + 1]) / 255.0,
                    f32::from(line[o + 2]) / 255.0,
                ];
                let mixed = if mode == BlendMode::Normal {
                    source
                } else {
                    blend(mode, backdrop, source)
                };
                for c in 0..3 {
                    let value = backdrop[c] + a * (mixed[c] - backdrop[c]);
                    line[o + c] = (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        });
}

/// Coverage of a pixel whose centre is `distance` source pixels inside an edge, when one canvas pixel
/// is `pixel` source pixels wide: 0 well outside, 1 well inside, smooth in between.
fn soft(distance: f32, pixel: f32) -> f32 {
    (distance / pixel.max(1e-6) + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> VideoFrame {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend(f(x, y));
            }
        }
        VideoFrame {
            width: w,
            height: h,
            rgba,
            pts: 0,
        }
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> VideoFrame {
        frame(w, h, |_, _| c)
    }

    fn at(f: &VideoFrame, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], f.rgba[i + 3]]
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    fn place(center: (f32, f32), size: (f32, f32)) -> Placement {
        Placement {
            center,
            size,
            scale: (1.0, 1.0),
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    fn black(w: u32, h: u32) -> VideoFrame {
        VideoFrame::black(w, h)
    }

    #[test]
    fn a_layer_covers_exactly_its_own_pixels() {
        let mut c = black(20, 20);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &place((10.0, 10.0), (10.0, 10.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 5, 5), RED);
        assert_eq!(at(&c, 14, 14), RED);
        assert_eq!(at(&c, 4, 10), [0, 0, 0, 255]);
        assert_eq!(at(&c, 15, 10), [0, 0, 0, 255]);
        assert_eq!(at(&c, 10, 4), [0, 0, 0, 255]);
    }

    #[test]
    fn scale_makes_the_layer_bigger_or_smaller() {
        let mut c = black(20, 20);
        let mut p = place((10.0, 10.0), (10.0, 10.0));
        p.scale = (2.0, 2.0);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &p,
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 0, 0), RED);
        assert_eq!(at(&c, 19, 19), RED);
        let mut c = black(20, 20);
        p.scale = (0.5, 0.5);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &p,
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        // half size: 5 pixels wide, covering x from 7.5 to 12.5
        assert_eq!(at(&c, 10, 10), RED);
        assert_eq!(at(&c, 9, 10), RED);
        assert_eq!(at(&c, 11, 10), RED);
        assert_eq!(at(&c, 6, 10), [0, 0, 0, 255]);
        assert_eq!(at(&c, 13, 10), [0, 0, 0, 255]);
    }

    #[test]
    fn position_moves_the_layer() {
        let mut c = black(20, 20);
        draw(
            &mut c,
            &solid(4, 4, RED),
            &place((15.0, 5.0), (4.0, 4.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 15, 5), RED);
        assert_eq!(at(&c, 5, 15), [0, 0, 0, 255]);
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height_clockwise() {
        // a wide bar, red on its left half and blue on its right half
        let bar = frame(10, 4, |x, _| if x < 5 { RED } else { BLUE });
        let mut c = black(20, 20);
        let mut p = place((10.0, 10.0), (10.0, 4.0));
        p.rotation = 90.0;
        draw(&mut c, &bar, &p, Crop::default(), 1.0, BlendMode::Normal);
        // now 4 wide and 10 tall; turning clockwise puts the left (red) half on top
        assert_eq!(at(&c, 10, 6), RED);
        assert_eq!(at(&c, 10, 13), BLUE);
        assert_eq!(at(&c, 4, 10), [0, 0, 0, 255], "no longer wide");
    }

    #[test]
    fn flipping_swaps_the_halves() {
        let bar = frame(10, 4, |x, _| if x < 5 { RED } else { BLUE });
        let mut c = black(20, 20);
        let mut p = place((10.0, 10.0), (10.0, 4.0));
        p.flip_h = true;
        draw(&mut c, &bar, &p, Crop::default(), 1.0, BlendMode::Normal);
        assert_eq!(at(&c, 6, 10), BLUE);
        assert_eq!(at(&c, 13, 10), RED);
        let tall = frame(4, 10, |_, y| if y < 5 { RED } else { BLUE });
        let mut c = black(20, 20);
        let mut p = place((10.0, 10.0), (4.0, 10.0));
        p.flip_v = true;
        draw(&mut c, &tall, &p, Crop::default(), 1.0, BlendMode::Normal);
        assert_eq!(at(&c, 10, 6), BLUE);
    }

    #[test]
    fn opacity_fades_towards_what_is_behind() {
        let mut c = black(10, 10);
        draw(
            &mut c,
            &solid(10, 10, [255, 255, 255, 255]),
            &place((5.0, 5.0), (10.0, 10.0)),
            Crop::default(),
            0.5,
            BlendMode::Normal,
        );
        let v = at(&c, 5, 5)[0];
        assert!(
            (126..=129).contains(&v),
            "half white on black is mid grey, was {v}"
        );
        let mut c = black(10, 10);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &place((5.0, 5.0), (10.0, 10.0)),
            Crop::default(),
            0.0,
            BlendMode::Normal,
        );
        assert_eq!(
            at(&c, 5, 5),
            [0, 0, 0, 255],
            "fully faded means nothing is drawn"
        );
    }

    #[test]
    fn transparent_parts_of_a_picture_let_the_background_through() {
        let src = frame(10, 10, |x, _| {
            if x < 5 {
                [255, 0, 0, 0]
            } else {
                [0, 255, 0, 255]
            }
        });
        let mut c = solid(10, 10, [10, 20, 30, 255]);
        draw(
            &mut c,
            &src,
            &place((5.0, 5.0), (10.0, 10.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(
            at(&c, 2, 5),
            [10, 20, 30, 255],
            "the transparent half shows the background"
        );
        assert_eq!(at(&c, 7, 5), [0, 255, 0, 255]);
    }

    #[test]
    fn cropping_hides_the_cut_off_part_without_moving_the_rest() {
        let src = frame(10, 10, |x, _| if x < 5 { RED } else { BLUE });
        let mut c = black(10, 10);
        let crop = Crop {
            left: 0.5,
            ..Crop::default()
        };
        draw(
            &mut c,
            &src,
            &place((5.0, 5.0), (10.0, 10.0)),
            crop,
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 2, 5), [0, 0, 0, 255], "the left half is gone");
        assert_eq!(at(&c, 7, 5), BLUE, "the right half stays where it was");
        let mut c = black(10, 10);
        let crop = Crop {
            top: 0.5,
            bottom: 0.2,
            ..Crop::default()
        };
        draw(
            &mut c,
            &solid(10, 10, RED),
            &place((5.0, 5.0), (10.0, 10.0)),
            crop,
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 5, 2), [0, 0, 0, 255]);
        assert_eq!(at(&c, 5, 6), RED);
        assert_eq!(at(&c, 5, 9), [0, 0, 0, 255]);
    }

    #[test]
    fn blend_modes_are_used() {
        let mut c = solid(10, 10, [0, 255, 0, 255]);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &place((5.0, 5.0), (10.0, 10.0)),
            Crop::default(),
            1.0,
            BlendMode::Multiply,
        );
        assert_eq!(at(&c, 5, 5), [0, 0, 0, 255], "red times green is black");
        let mut c = solid(10, 10, [0, 255, 0, 255]);
        draw(
            &mut c,
            &solid(10, 10, RED),
            &place((5.0, 5.0), (10.0, 10.0)),
            Crop::default(),
            1.0,
            BlendMode::Screen,
        );
        assert_eq!(
            at(&c, 5, 5),
            [255, 255, 0, 255],
            "red screened over green is yellow"
        );
    }

    #[test]
    fn a_layer_outside_the_canvas_or_with_no_size_does_nothing_and_does_not_crash() {
        let mut c = black(10, 10);
        let before = c.clone();
        draw(
            &mut c,
            &solid(4, 4, RED),
            &place((500.0, 5.0), (4.0, 4.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        let mut p = place((5.0, 5.0), (4.0, 4.0));
        p.scale = (0.0, 1.0);
        draw(
            &mut c,
            &solid(4, 4, RED),
            &p,
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        draw(
            &mut c,
            &solid(4, 4, RED),
            &place((5.0, 5.0), (0.0, 0.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(c, before);
        // partly outside is fine
        draw(
            &mut c,
            &solid(4, 4, RED),
            &place((0.0, 0.0), (8.0, 8.0)),
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 1, 1), RED);
    }

    #[test]
    fn a_slightly_turned_layer_has_soft_edges_and_a_solid_middle() {
        let mut c = black(40, 40);
        let mut p = place((20.0, 20.0), (20.0, 20.0));
        p.rotation = 30.0;
        draw(
            &mut c,
            &solid(20, 20, RED),
            &p,
            Crop::default(),
            1.0,
            BlendMode::Normal,
        );
        assert_eq!(at(&c, 20, 20), RED);
        assert_eq!(at(&c, 1, 1), [0, 0, 0, 255]);
        // somewhere along the edge there are in-between pixels (anti-aliasing)
        let soft_pixels = c
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 10 && p[0] < 245)
            .count();
        assert!(
            soft_pixels > 10,
            "expected smooth edges, found {soft_pixels} in-between pixels"
        );
    }
}
