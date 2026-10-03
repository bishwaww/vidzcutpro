//! Shapes made of coverage values (0 = nothing, 1 = fully covered), and what we do with them:
//! grow them (for outlines), soften them (for shadows), move them.

/// A rectangle of coverage values.
#[derive(Clone, Debug)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
}

impl Mask {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            a: vec![0.0; w * h],
        }
    }

    /// Add coverage `c` at (x, y), as one layer drawn over what is there.
    pub fn add(&mut self, x: i32, y: i32, c: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let v = &mut self.a[y as usize * self.w + x as usize];
        *v += c * (1.0 - *v);
    }

    /// The shape grown outwards by `radius` pixels, with a soft edge. The original is kept as it is.
    pub fn grown(&self, radius: f32) -> Self {
        if radius <= 0.0 {
            return self.clone();
        }
        let big = 1.0e10_f32;
        // squared distance to the nearest covered pixel, columns then rows
        let mut d: Vec<f32> = self
            .a
            .iter()
            .map(|v| if *v >= 0.5 { 0.0 } else { big })
            .collect();
        let mut column = vec![0.0; self.h];
        let mut out = vec![0.0; self.h.max(self.w)];
        let mut scratch = Scratch::new(self.h.max(self.w));
        for x in 0..self.w {
            for y in 0..self.h {
                column[y] = d[y * self.w + x];
            }
            distance_1d(&column, &mut out[..self.h], &mut scratch);
            for y in 0..self.h {
                d[y * self.w + x] = out[y];
            }
        }
        let mut row = vec![0.0; self.w];
        for y in 0..self.h {
            row.copy_from_slice(&d[y * self.w..(y + 1) * self.w]);
            distance_1d(&row, &mut out[..self.w], &mut scratch);
            d[y * self.w..(y + 1) * self.w].copy_from_slice(&out[..self.w]);
        }
        let a = d
            .iter()
            .zip(&self.a)
            .map(|(d2, keep)| (radius + 0.5 - d2.sqrt()).clamp(0.0, 1.0).max(*keep))
            .collect();
        Self {
            w: self.w,
            h: self.h,
            a,
        }
    }

    /// The same shape moved by whole pixels (what moves off the edge is lost).
    pub fn shifted(&self, dx: i32, dy: i32) -> Self {
        let mut out = Self::new(self.w, self.h);
        for y in 0..self.h as i32 {
            let sy = y - dy;
            if sy < 0 || sy >= self.h as i32 {
                continue;
            }
            for x in 0..self.w as i32 {
                let sx = x - dx;
                if sx < 0 || sx >= self.w as i32 {
                    continue;
                }
                out.a[y as usize * self.w + x as usize] =
                    self.a[sy as usize * self.w + sx as usize];
            }
        }
        out
    }

    /// Softened by roughly a gaussian blur of `sigma` pixels (three box blurs).
    pub fn blurred(&self, sigma: f32) -> Self {
        if sigma < 0.3 {
            return self.clone();
        }
        // three boxes of this width approximate a gaussian
        let width = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt()).round().max(1.0) as usize;
        let mut cur = self.a.clone();
        let mut tmp = vec![0.0; cur.len()];
        for _ in 0..3 {
            box_blur_rows(&cur, &mut tmp, self.w, self.h, width);
            transpose(&tmp, &mut cur, self.w, self.h);
            box_blur_rows(&cur, &mut tmp, self.h, self.w, width);
            transpose(&tmp, &mut cur, self.h, self.w);
        }
        Self {
            w: self.w,
            h: self.h,
            a: cur,
        }
    }
}

struct Scratch {
    v: Vec<usize>,
    z: Vec<f32>,
}

impl Scratch {
    fn new(n: usize) -> Self {
        Self {
            v: vec![0; n],
            z: vec![0.0; n + 1],
        }
    }
}

/// Squared distance transform of one line (Felzenszwalb & Huttenlocher): for each position, the smallest
/// `(q - p)² + f[p]` over all positions `p`.
fn distance_1d(f: &[f32], out: &mut [f32], s: &mut Scratch) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let (v, z) = (&mut s.v, &mut s.z);
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::MIN;
    z[1] = f32::MAX;
    for q in 1..n {
        loop {
            let p = v[k];
            let cross =
                ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if cross <= z[k] {
                // k can never be 0 here because z[0] is the lowest possible value
                k -= 1;
            } else {
                k += 1;
                v[k] = q;
                z[k] = cross;
                z[k + 1] = f32::MAX;
                break;
            }
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let diff = q as f32 - p as f32;
        *o = diff * diff + f[p];
    }
}

/// Average each row over a window `width` pixels wide (outside the picture counts as empty).
fn box_blur_rows(src: &[f32], dst: &mut [f32], w: usize, h: usize, width: usize) {
    let (before, after) = (width / 2, width - width / 2 - 1);
    let norm = 1.0 / width as f32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut dst[y * w..(y + 1) * w];
        let mut sum: f32 = row.iter().take(after + 1).sum();
        for x in 0..w {
            out[x] = sum * norm;
            if x + after + 1 < w {
                sum += row[x + after + 1];
            }
            if x >= before {
                sum -= row[x - before];
            }
        }
    }
}

/// `dst` (h wide, w tall) = `src` (w wide, h tall) turned on its side.
fn transpose(src: &[f32], dst: &mut [f32], w: usize, h: usize) {
    for y in 0..h {
        for x in 0..w {
            dst[x * h + y] = src[y * w + x];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(w: usize, h: usize, x: usize, y: usize) -> Mask {
        let mut m = Mask::new(w, h);
        m.a[y * w + x] = 1.0;
        m
    }

    #[test]
    fn growing_makes_a_round_shape_of_the_wanted_radius() {
        let g = dot(41, 41, 20, 20).grown(8.0);
        let at = |x: usize, y: usize| g.a[y * 41 + x];
        assert_eq!(at(20, 20), 1.0);
        assert!(at(27, 20) > 0.9, "7 pixels to the right is inside");
        assert!(at(20, 13) > 0.9, "7 pixels up is inside");
        assert!(
            (at(28, 20) - 0.5).abs() < 0.1,
            "8 pixels away is the soft edge"
        );
        assert!(
            at(27, 27) < 0.1,
            "the diagonal corner (about 9.9 away) is outside"
        );
        assert!(at(30, 20) < 0.01, "10 pixels away is outside");
    }

    #[test]
    fn growing_by_nothing_changes_nothing() {
        let m = dot(5, 5, 2, 2);
        assert_eq!(m.grown(0.0).a, m.a);
    }

    #[test]
    fn blurring_keeps_the_total_and_spreads_it_out() {
        let m = dot(41, 41, 20, 20);
        let b = m.blurred(3.0);
        let total: f32 = b.a.iter().sum();
        assert!((total - 1.0).abs() < 0.02, "total {total}");
        assert!(b.a[20 * 41 + 20] < 0.3);
        assert!(b.a[20 * 41 + 22] > 0.0);
    }

    #[test]
    fn shifting_moves_the_shape_and_drops_what_falls_off() {
        let m = dot(5, 5, 1, 1).shifted(2, 1);
        assert_eq!(m.a[2 * 5 + 3], 1.0);
        let gone = dot(5, 5, 4, 4).shifted(1, 0);
        assert_eq!(gone.a.iter().sum::<f32>(), 0.0);
    }
}
