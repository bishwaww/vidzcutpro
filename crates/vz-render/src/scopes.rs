//! Scopes: pictures that show the colours of a frame as measurements, the way colourists check them.
//!
//! * histogram: how many pixels there are at each brightness, for red, green and blue
//! * waveform: brightness (up) for every column of the picture (left to right)
//! * vectorscope: the colours of the picture on a colour wheel (hue around, strength from the middle)

use vz_media::VideoFrame;

/// A finished scope picture (RGBA, fully opaque).
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl ScopeImage {
    fn new(width: usize, height: usize) -> Self {
        let mut rgba = vec![0u8; width * height * 4];
        for p in rgba.as_chunks_mut::<4>().0 {
            *p = [12, 12, 12, 255];
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    fn set(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x < self.width && y < self.height {
            let i = (y * self.width + x) * 4;
            self.rgba[i..i + 3].copy_from_slice(&c);
        }
    }

    fn add(&mut self, x: usize, y: usize, c: [f32; 3]) {
        if x < self.width && y < self.height {
            let i = (y * self.width + x) * 4;
            for (k, add) in c.iter().enumerate() {
                self.rgba[i + k] = (f32::from(self.rgba[i + k]) + add).min(255.0) as u8;
            }
        }
    }
}

/// How many pixels there are at each brightness 0..=255 for red, green, blue and luma.
pub fn histogram_counts(frame: &VideoFrame) -> [[u32; 256]; 4] {
    let mut counts = [[0u32; 256]; 4];
    for p in frame.rgba.as_chunks::<4>().0 {
        counts[0][usize::from(p[0])] += 1;
        counts[1][usize::from(p[1])] += 1;
        counts[2][usize::from(p[2])] += 1;
        let l = 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
        counts[3][l as usize] += 1;
    }
    counts
}

/// The histogram drawn as three overlapping coloured areas.
pub fn histogram(frame: &VideoFrame, width: usize, height: usize) -> ScopeImage {
    let counts = histogram_counts(frame);
    let mut img = ScopeImage::new(width, height);
    // the tallest bar (ignoring the pure black and white ends, which are often huge) sets the scale
    let peak = counts[..3]
        .iter()
        .flat_map(|c| c[1..255].iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    for x in 0..width {
        let bin = x * 256 / width;
        for (ch, color) in [
            [210.0, 40.0, 40.0],
            [40.0, 190.0, 40.0],
            [50.0, 90.0, 230.0],
        ]
        .iter()
        .enumerate()
        {
            let value = counts[ch][bin] as f32 / peak;
            let h = (value.min(1.0) * (height - 2) as f32) as usize;
            for y in 0..h {
                img.add(x, height - 1 - y, color.map(|c| c * 0.7));
            }
        }
    }
    img
}

/// Brightness against position across the picture: every pixel adds a dot at (its column, its brightness).
pub fn waveform(frame: &VideoFrame, width: usize, height: usize) -> ScopeImage {
    let mut img = ScopeImage::new(width, height);
    // faint guide lines at 0, 25, 50, 75 and 100 %
    for k in 0..=4 {
        let y = (height - 1) - (height - 1) * k / 4;
        for x in 0..width {
            img.set(x, y, [34, 34, 34]);
        }
    }
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    if fw == 0 || fh == 0 {
        return img;
    }
    // a pixel adds only a little, so busy places glow
    let strength =
        (width as f32 * height as f32 / (fw as f32 * fh as f32)).clamp(0.02, 1.0) * 220.0;
    let step_y = (fh / 180).max(1);
    for y in (0..fh).step_by(step_y) {
        for x in 0..fw {
            let i = (y * fw + x) * 4;
            let (r, g, b) = (
                f32::from(frame.rgba[i]),
                f32::from(frame.rgba[i + 1]),
                f32::from(frame.rgba[i + 2]),
            );
            let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let (sx, sy) = (
                x * width / fw,
                (height - 1) - (l / 255.0 * (height - 1) as f32) as usize,
            );
            let k = strength * step_y as f32 / 255.0;
            img.add(
                sx,
                sy,
                [r * k + 20.0 * k, g * k + 40.0 * k, b * k + 20.0 * k],
            );
        }
    }
    img
}

/// Where a colour sits on the vectorscope: (x, y) from -1 to 1 (red to the upper right, blue to the lower right...).
pub fn vector_position(rgb: [u8; 3]) -> (f32, f32) {
    let (r, g, b) = (
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
    );
    let cb = -0.1146 * r - 0.3854 * g + 0.5 * b;
    let cr = 0.5 * r - 0.4542 * g - 0.0458 * b;
    // fully saturated primaries reach about 0.5 from the middle; scale so they reach near the edge
    (cb * 1.8, -cr * 1.8)
}

/// The colours of the picture as dots on a colour wheel.
pub fn vectorscope(frame: &VideoFrame, size: usize) -> ScopeImage {
    let mut img = ScopeImage::new(size, size);
    let c = (size as f32 - 1.0) / 2.0;
    let r = c - 2.0;
    // the circle and the cross
    for step in 0..360 {
        let a = (step as f32).to_radians();
        img.set(
            (c + a.cos() * r) as usize,
            (c + a.sin() * r) as usize,
            [60, 60, 60],
        );
        img.set(
            (c + a.cos() * r * 0.5) as usize,
            (c + a.sin() * r * 0.5) as usize,
            [38, 38, 38],
        );
    }
    for i in 0..size {
        img.set(i, c as usize, [38, 38, 38]);
        img.set(c as usize, i, [38, 38, 38]);
    }
    // marks where the pure colours land
    for rgb in [
        [255, 0, 0],
        [255, 255, 0],
        [0, 255, 0],
        [0, 255, 255],
        [0, 0, 255],
        [255, 0, 255],
    ] {
        let (x, y) = vector_position(rgb);
        let (px, py) = ((c + x * r) as usize, (c + y * r) as usize);
        for d in 0..3 {
            img.set(px + d, py, [rgb[0] / 2, rgb[1] / 2, rgb[2] / 2]);
            img.set(px, py + d, [rgb[0] / 2, rgb[1] / 2, rgb[2] / 2]);
        }
    }
    let total = (frame.width as usize * frame.height as usize).max(1) as f32;
    let k = (size as f32 * size as f32 / total).clamp(0.02, 1.0) * 120.0;
    let step = ((frame.width as usize * frame.height as usize) / 40_000).max(1);
    for (i, p) in frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .step_by(step)
    {
        let (x, y) = vector_position([p[0], p[1], p[2]]);
        let (px, py) = ((c + x * r).round(), (c + y * r).round());
        if px >= 0.0 && py >= 0.0 {
            let _ = i;
            img.add(
                px as usize,
                py as usize,
                [
                    f32::from(p[0]) * k / 255.0 + 30.0 * k / 120.0,
                    f32::from(p[1]) * k / 255.0 + 30.0 * k / 120.0,
                    f32::from(p[2]) * k / 255.0 + 30.0 * k / 120.0,
                ]
                .map(|v| v * step as f32),
            );
        }
    }
    img
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

    fn lit(img: &ScopeImage, x: usize, y: usize) -> u32 {
        let i = (y * img.width + x) * 4;
        u32::from(img.rgba[i]) + u32::from(img.rgba[i + 1]) + u32::from(img.rgba[i + 2])
    }

    #[test]
    fn the_histogram_of_a_flat_picture_has_one_bin_per_channel() {
        let counts = histogram_counts(&solid(10, 10, [10, 128, 250]));
        assert_eq!(counts[0][10], 100);
        assert_eq!(counts[1][128], 100);
        assert_eq!(counts[2][250], 100);
        assert_eq!(counts[0].iter().sum::<u32>(), 100);
        let img = histogram(&solid(10, 10, [10, 128, 250]), 256, 100);
        // a tall bar at 128 in green, nothing at 200
        assert!(lit(&img, 128, 5) > lit(&img, 200, 5));
    }

    #[test]
    fn the_waveform_of_a_ramp_runs_from_the_bottom_left_to_the_top_right() {
        let mut f = VideoFrame::black(256, 64);
        for y in 0..64usize {
            for x in 0..256usize {
                let i = (y * 256 + x) * 4;
                f.rgba[i] = x as u8;
                f.rgba[i + 1] = x as u8;
                f.rgba[i + 2] = x as u8;
            }
        }
        let img = waveform(&f, 256, 128);
        let low = (118..128).map(|y| lit(&img, 10, y)).max().unwrap();
        assert!(low > 80, "dark at the left is low: {low}");
        let high = (0..10).map(|y| lit(&img, 245, y)).max().unwrap();
        assert!(high > 80, "bright at the right is high: {high}");
        assert!(lit(&img, 10, 8) < 60 && lit(&img, 245, 120) < 60);
    }

    #[test]
    fn the_vectorscope_puts_pure_colours_in_their_own_directions() {
        let red = vector_position([255, 0, 0]);
        let blue = vector_position([0, 0, 255]);
        let green = vector_position([0, 255, 0]);
        let grey = vector_position([128, 128, 128]);
        assert!(
            red.0 < 0.0 + 0.5 && red.1 < 0.0,
            "red is in the upper half: {red:?}"
        );
        assert!(
            blue.0 > 0.5 && blue.1 > -0.1,
            "blue is to the right: {blue:?}"
        );
        assert!(
            green.0 < 0.0 && green.1 > 0.0,
            "green is lower left: {green:?}"
        );
        assert!(
            grey.0.abs() < 0.01 && grey.1.abs() < 0.01,
            "grey is in the middle"
        );
        // a picture of pure red lights up the red spot, not the blue one
        let img = vectorscope(&solid(32, 32, [255, 0, 0]), 160);
        let spot = |pos: (f32, f32)| {
            let (c, r) = (79.5, 77.5);
            let (x, y) = ((c + pos.0 * r) as usize, (c + pos.1 * r) as usize);
            (0..3)
                .flat_map(|dx| (0..3).map(move |dy| (dx, dy)))
                .map(|(dx, dy)| lit(&img, x + dx - 1, y + dy - 1))
                .max()
                .unwrap()
        };
        assert!(spot(red) > 200, "{}", spot(red));
        assert!(spot(blue) < 200);
    }

    #[test]
    fn scopes_cope_with_empty_or_tiny_pictures() {
        let tiny = VideoFrame::black(1, 1);
        let _ = histogram(&tiny, 64, 32);
        let _ = waveform(&tiny, 64, 32);
        let _ = vectorscope(&tiny, 64);
        let empty = VideoFrame::black(0, 0);
        let _ = waveform(&empty, 64, 32);
        let _ = vectorscope(&empty, 64);
    }
}
