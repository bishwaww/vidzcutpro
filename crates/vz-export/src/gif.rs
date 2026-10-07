//! Animated GIF export: one shared 256-colour palette for the whole clip, optional dithering.

use crate::encode::{ExportError, ExportReport, Progress};
use color_quant::NeuQuant;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vz_core::{Micros, Project};
use vz_render::Renderer;

/// How the colours that are not in the palette are faked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dither {
    /// Plain nearest colour: flat, can show bands.
    None,
    /// A fixed pattern: fast and steady between frames (best for moving pictures).
    #[default]
    Ordered,
    /// Spreads the error to neighbours: smooth stills, a bit noisy when moving.
    FloydSteinberg,
}

impl Dither {
    pub const ALL: [Self; 3] = [Self::None, Self::Ordered, Self::FloydSteinberg];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Ordered => "Pattern",
            Self::FloydSteinberg => "Smooth (Floyd-Steinberg)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GifSettings {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Pictures per second (1 to 50).
    pub fps: u32,
    pub dither: Dither,
    pub looping: bool,
    pub range: Option<(Micros, Micros)>,
}

impl GifSettings {
    pub fn for_project(project: &Project, path: PathBuf, short_side: u32) -> Self {
        let (width, height) =
            crate::size_for_short_side(project.canvas.width, project.canvas.height, short_side);
        Self {
            path,
            width,
            height,
            fps: 15,
            dither: Dither::Ordered,
            looping: true,
            range: None,
        }
    }
}

const BAYER4: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// The palette plus a lookup from a colour to its nearest palette entry.
struct Palette {
    quant: NeuQuant,
    rgb: Vec<u8>,
    /// Nearest entry for every colour with 6 bits per channel (filled on demand; `u16::MAX` = not yet).
    cache: Vec<u16>,
}

impl Palette {
    fn new(sample_rgba: &[u8]) -> Self {
        let quant = NeuQuant::new(10, 256, sample_rgba);
        let rgb = quant.color_map_rgb();
        Self {
            quant,
            rgb,
            cache: vec![u16::MAX; 1 << 18],
        }
    }

    fn nearest(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let key = (usize::from(r >> 2) << 12) | (usize::from(g >> 2) << 6) | usize::from(b >> 2);
        let hit = self.cache[key];
        if hit != u16::MAX {
            return hit as u8;
        }
        // look up with the middle of the 6-bit cell so the answer is the same for the whole cell
        let index =
            self.quant
                .index_of(&[(r & 0xFC) | 2, (g & 0xFC) | 2, (b & 0xFC) | 2, 255]) as u8;
        self.cache[key] = u16::from(index);
        index
    }

    fn colour(&self, index: u8) -> [u8; 3] {
        let i = usize::from(index) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// Turn a picture into palette indices.
fn quantise(rgba: &[u8], w: usize, h: usize, palette: &mut Palette, dither: Dither) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    match dither {
        Dither::None => {
            for (o, px) in out.iter_mut().zip(rgba.as_chunks::<4>().0) {
                *o = palette.nearest(px[0], px[1], px[2]);
            }
        }
        Dither::Ordered => {
            for y in 0..h {
                for x in 0..w {
                    let px = &rgba[(y * w + x) * 4..];
                    let bias = (BAYER4[y & 3][x & 3] - 8) * 2; // about +-16
                    let add = |v: u8| (i32::from(v) + bias).clamp(0, 255) as u8;
                    out[y * w + x] = palette.nearest(add(px[0]), add(px[1]), add(px[2]));
                }
            }
        }
        Dither::FloydSteinberg => {
            // two rows of error (in sixteenths) per colour channel
            let mut now = vec![[0i32; 3]; w + 2];
            let mut next = vec![[0i32; 3]; w + 2];
            for y in 0..h {
                next.iter_mut().for_each(|e| *e = [0; 3]);
                for x in 0..w {
                    let px = &rgba[(y * w + x) * 4..];
                    let e = now[x + 1];
                    let want: [i32; 3] =
                        std::array::from_fn(|c| (i32::from(px[c]) + e[c] / 16).clamp(0, 255));
                    let index = palette.nearest(want[0] as u8, want[1] as u8, want[2] as u8);
                    out[y * w + x] = index;
                    let got = palette.colour(index);
                    let err: [i32; 3] = std::array::from_fn(|c| want[c] - i32::from(got[c]));
                    for c in 0..3 {
                        now[x + 2][c] += err[c] * 7;
                        next[x][c] += err[c] * 3;
                        next[x + 1][c] += err[c] * 5;
                        next[x + 2][c] += err[c];
                    }
                }
                std::mem::swap(&mut now, &mut next);
            }
        }
    }
    out
}

/// Write the timeline (or `settings.range`) as an animated GIF.
pub fn export_gif(
    project: &Project,
    settings: &GifSettings,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExportReport, ExportError> {
    let began = Instant::now();
    let duration = project.duration();
    let (start, end) = settings.range.unwrap_or((0, duration));
    let (start, end) = (start.clamp(0, duration), end.clamp(0, duration));
    if end <= start {
        return Err(ExportError::Empty);
    }
    let fps = settings.fps.clamp(1, 50);
    let (w, h) = (settings.width as usize, settings.height as usize);
    if w == 0 || h == 0 || w > usize::from(u16::MAX) || h > usize::from(u16::MAX) {
        return Err(ExportError::Message("the GIF size is not valid".into()));
    }
    let total = (((end - start) as f64 / 1e6 * f64::from(fps)).ceil() as u64).max(1);
    let time_of = |i: u64| start + (i as f64 * 1e6 / f64::from(fps)) as Micros;

    let mut renderer = Renderer::new();
    let size = (settings.width, settings.height);

    // palette from up to 12 pictures spread over the clip
    let samples = total.min(12);
    let mut sample_rgba = Vec::new();
    for s in 0..samples {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        let i = if samples > 1 {
            s * (total - 1) / (samples - 1)
        } else {
            0
        };
        let picture = renderer.render(project, time_of(i), size);
        // every 7th pixel is plenty
        for px in picture.rgba.as_chunks::<4>().0.iter().step_by(7) {
            sample_rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    let mut palette = Palette::new(&sample_rgba);

    let partial = {
        let mut p = settings.path.as_os_str().to_owned();
        p.push(".partial");
        PathBuf::from(p)
    };
    let result = (|| -> Result<(), ExportError> {
        let file = std::fs::File::create(&partial)?;
        let mut encoder = gif::Encoder::new(file, w as u16, h as u16, &palette.rgb)
            .map_err(|e| ExportError::Message(e.to_string()))?;
        if settings.looping {
            encoder
                .set_repeat(gif::Repeat::Infinite)
                .map_err(|e| ExportError::Message(e.to_string()))?;
        }
        let mut shown_cs = 0u32; // centiseconds already used by earlier pictures
        for i in 0..total {
            if cancel.load(Ordering::Relaxed) {
                return Err(ExportError::Cancelled);
            }
            let picture = renderer.render(project, time_of(i), size);
            let indices = quantise(&picture.rgba, w, h, &mut palette, settings.dither);
            // spread the rounding of the delay so the length stays right
            let until = ((i + 1) as f64 * 100.0 / f64::from(fps)).round() as u32;
            let delay = until.saturating_sub(shown_cs).max(2);
            shown_cs += delay;
            let mut frame = gif::Frame::from_indexed_pixels(w as u16, h as u16, indices, None);
            frame.delay = delay as u16;
            encoder
                .write_frame(&frame)
                .map_err(|e| ExportError::Message(e.to_string()))?;
            if i % 4 == 0 || i + 1 == total {
                progress(Progress {
                    frames_done: i + 1,
                    frames_total: total,
                });
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            std::fs::rename(&partial, &settings.path)?;
            Ok(ExportReport {
                encoder: "gif".to_string(),
                frames: total,
                seconds: began.elapsed().as_secs_f64(),
                bytes: std::fs::metadata(&settings.path).map_or(0, |m| m.len()),
                path: settings.path.clone(),
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}
