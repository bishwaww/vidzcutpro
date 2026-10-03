//! Bending a line of text along an arc.

/// A picture of pre-multiplied colours (4 floats per pixel).
pub struct Pixels {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>,
}

impl Pixels {
    fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        // bilinear, transparent outside
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let at = |xx: f32, yy: f32| -> [f32; 4] {
            if xx < 0.0 || yy < 0.0 || xx >= self.w as f32 || yy >= self.h as f32 {
                [0.0; 4]
            } else {
                self.px[yy as usize * self.w + xx as usize]
            }
        };
        let (a, b, c, d) = (
            at(ix, iy),
            at(ix + 1.0, iy),
            at(ix, iy + 1.0),
            at(ix + 1.0, iy + 1.0),
        );
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bottom = c[i] + (d[i] - c[i]) * tx;
            out[i] = top + (bottom - top) * ty;
        }
        out
    }

    fn flipped(&self) -> Self {
        let mut px = Vec::with_capacity(self.px.len());
        for y in (0..self.h).rev() {
            px.extend_from_slice(&self.px[y * self.w..(y + 1) * self.w]);
        }
        Self {
            w: self.w,
            h: self.h,
            px,
        }
    }
}

/// Bend `img` along an arc. `amount` runs from -100 (the ends turn up, a smile) to 100 (the ends turn down, an arch).
/// The middle of the picture stays where it is; the picture grows to hold the bend.
pub fn bend(img: &Pixels, amount: f32) -> Pixels {
    let theta = amount.abs().min(100.0) / 100.0 * std::f32::consts::PI * 0.9;
    if theta < 1e-3 || img.w < 2 || img.h < 2 {
        return Pixels {
            w: img.w,
            h: img.h,
            px: img.px.clone(),
        };
    }
    // a smile is an upside-down arch of upside-down text, turned over again at the end
    let src = if amount < 0.0 {
        img.flipped()
    } else {
        Pixels {
            w: img.w,
            h: img.h,
            px: img.px.clone(),
        }
    };
    let (w, h) = (src.w as f32, src.h as f32);
    let radius = w / theta;
    let half = theta / 2.0;
    let outer = radius + h / 2.0;
    let out_w = (2.0
        * outer
        * if theta > std::f32::consts::PI {
            1.0
        } else {
            half.sin()
        })
    .ceil() as usize
        + 2;
    let cy = radius + h / 2.0;
    let bottom = cy - (radius - h / 2.0) * half.cos();
    let out_h = bottom.max(h).ceil() as usize + 2;
    let cx = out_w as f32 / 2.0;
    let mut out = Pixels {
        w: out_w,
        h: out_h,
        px: vec![[0.0; 4]; out_w * out_h],
    };
    for y in 0..out_h {
        for x in 0..out_w {
            let (dx, dy) = (x as f32 + 0.5 - cx, cy - (y as f32 + 0.5));
            let r = (dx * dx + dy * dy).sqrt();
            let phi = dx.atan2(dy);
            if phi.abs() > half + 0.02 {
                continue;
            }
            let (u, v) = (w / 2.0 + radius * phi, h / 2.0 - (r - radius));
            out.px[y * out_w + x] = src.sample(u, v);
        }
    }
    if amount < 0.0 { out.flipped() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(w: usize, h: usize) -> Pixels {
        // a solid white bar
        Pixels {
            w,
            h,
            px: vec![[1.0, 1.0, 1.0, 1.0]; w * h],
        }
    }

    fn rows_with_ink(p: &Pixels, x: usize) -> Vec<usize> {
        (0..p.h).filter(|y| p.px[y * p.w + x][3] > 0.5).collect()
    }

    #[test]
    fn no_bend_changes_nothing() {
        let b = bar(100, 20);
        let out = bend(&b, 0.0);
        assert_eq!((out.w, out.h), (100, 20));
    }

    #[test]
    fn an_arch_lifts_the_middle_above_the_ends_and_makes_the_picture_taller() {
        let out = bend(&bar(300, 30), 60.0);
        assert!(out.h > 40, "{}", out.h);
        let mid = rows_with_ink(&out, out.w / 2);
        let end = rows_with_ink(&out, 8);
        assert!(!mid.is_empty() && !end.is_empty());
        assert!(
            mid[0] < end[0],
            "the middle is higher: {} vs {}",
            mid[0],
            end[0]
        );
    }

    #[test]
    fn a_smile_does_the_opposite() {
        let out = bend(&bar(300, 30), -60.0);
        let mid = rows_with_ink(&out, out.w / 2);
        let end = rows_with_ink(&out, 8);
        assert!(!mid.is_empty() && !end.is_empty());
        assert!(
            mid[0] > end[0],
            "the middle is lower: {} vs {}",
            mid[0],
            end[0]
        );
    }

    #[test]
    fn the_bend_keeps_the_amount_of_ink_about_the_same() {
        let b = bar(300, 30);
        let before: f32 = b.px.iter().map(|p| p[3]).sum();
        let after: f32 = bend(&b, 80.0).px.iter().map(|p| p[3]).sum();
        assert!((after / before - 1.0).abs() < 0.2, "{before} {after}");
    }
}
