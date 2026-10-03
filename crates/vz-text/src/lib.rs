//! Fonts and text: turns a piece of styled text into a picture with transparent background.
//!
//! Letters are shaped by cosmic-text (Sinhala and Tamil conjuncts, vowel signs and Latin all come out
//! right, and a letter group is never cut in the middle). On top of the plain letters we draw the
//! outline, the shadow and the background box. The renderer puts that picture on the canvas like any
//! other layer, so a text layer can be moved, scaled, turned, faded and blended.
//!
//! Sizes in a [`TextStyle`] are "reference pixels": the size they would have on a canvas whose short
//! side is 1080 pixels. [`unit`] converts them to the real canvas.

mod mask;

use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    UnderlineStyle, Weight, Wrap, fontdb,
};
use mask::Mask;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use vz_core::{TextAlign, TextContent};

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-text";

/// The fonts that come with vidzcut (all under the SIL Open Font Licence, see THIRD_PARTY_LICENSES.md).
const BUNDLED: [&[u8]; 4] = [
    include_bytes!("../../../assets/fonts/Inter-Variable.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansSinhala-Variable.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerifSinhala-Variable.ttf"),
    include_bytes!("../../../assets/fonts/NotoSansTamil-Variable.ttf"),
];

/// The font used when a text style names none.
pub const DEFAULT_FONT: &str = "Inter";

/// The fonts we always offer first in the font list.
pub const BUNDLED_FAMILIES: [&str; 4] = [
    "Inter",
    "Noto Sans Sinhala",
    "Noto Serif Sinhala",
    "Noto Sans Tamil",
];

/// Line height as a multiple of the font size (Sinhala and Tamil need more room than Latin).
const LINE_HEIGHT: f32 = 1.4;

/// How many reference pixels become one canvas pixel: the canvas's short side divided by 1080.
pub fn unit(canvas_width: u32, canvas_height: u32) -> f32 {
    canvas_width.min(canvas_height).max(1) as f32 / 1080.0
}

/// A finished text picture: straight (not pre-multiplied) RGBA with transparent surroundings.
#[derive(Debug)]
pub struct TextImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Size of the text box itself (text plus background padding), without the room kept around it
    /// for outline and shadow. The picture is centred on this box.
    pub content: (f32, f32),
}

/// Makes text pictures and measures text. Remembers recent pictures, because the same text is drawn on
/// every frame it is on screen.
#[derive(Default)]
pub struct TextEngine {
    fonts: Option<FontSystem>,
    swash: Option<SwashCache>,
    images: HashMap<String, Arc<TextImage>>,
    sizes: HashMap<String, (f32, f32)>,
}

struct Laid {
    buffer: Buffer,
    /// Size of the letters' area.
    width: f32,
    height: f32,
}

impl TextEngine {
    pub fn new() -> Self {
        Self::default()
    }

    fn system(&mut self) -> &mut FontSystem {
        self.fonts.get_or_insert_with(|| {
            let started = std::time::Instant::now();
            let mut db = fontdb::Database::new();
            for data in BUNDLED {
                db.load_font_data(data.to_vec());
            }
            db.load_system_fonts();
            db.set_sans_serif_family(DEFAULT_FONT);
            tracing::info!(
                "fonts ready: {} faces in {:?}",
                db.faces().count(),
                started.elapsed()
            );
            FontSystem::new_with_locale_and_db("en-US".into(), db)
        })
    }

    /// Font family names to offer in a list: ours first, then the ones installed on this computer.
    pub fn families(&mut self) -> Vec<String> {
        let all: BTreeSet<String> = self
            .system()
            .db()
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .collect();
        let mut out: Vec<String> = BUNDLED_FAMILIES.iter().map(|s| (*s).to_owned()).collect();
        out.extend(
            all.into_iter()
                .filter(|n| !BUNDLED_FAMILIES.contains(&n.as_str())),
        );
        out
    }

    /// Lay the text out at `factor` canvas pixels per reference pixel.
    fn lay_out(&mut self, text: &TextContent, factor: f32) -> Laid {
        let style = &text.style;
        let size = (style.size * factor).max(1.0);
        let metrics = Metrics::new(size, size * LINE_HEIGHT * style.line_spacing.max(0.2));
        let family = if style.font.is_empty() {
            DEFAULT_FONT
        } else {
            style.font.as_str()
        };
        let mut attrs = Attrs::new()
            .family(Family::Name(family))
            .weight(if style.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            })
            .style(if style.italic {
                Style::Italic
            } else {
                Style::Normal
            })
            .letter_spacing(style.letter_spacing);
        if style.underline {
            attrs = attrs.underline(UnderlineStyle::Single);
        }
        let align = match style.align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        };

        let fs = self.system();
        let mut buffer = Buffer::new(fs, metrics);
        let (width, height) = {
            let mut b = buffer.borrow_with(fs);
            match style.box_width {
                Some(w) => {
                    b.set_wrap(Wrap::WordOrGlyph);
                    b.set_size(Some((w * factor).max(size)), None);
                }
                None => {
                    b.set_wrap(Wrap::None);
                    b.set_size(None, None);
                }
            }
            b.set_text(&text.text, &attrs, Shaping::Advanced, Some(align));
            b.shape_until_scroll(true);
            let widest = b.layout_runs().map(|r| r.line_w).fold(0.0_f32, f32::max);
            let width = match style.box_width {
                Some(w) => (w * factor).max(size),
                None => {
                    // line up the lines inside the widest one
                    let w = widest.ceil();
                    b.set_size(Some(w.max(1.0)), None);
                    b.shape_until_scroll(true);
                    w
                }
            };
            let lines = b.layout_runs().count().max(1);
            (width, lines as f32 * metrics.line_height)
        };
        Laid {
            buffer,
            width: width.max(1.0),
            height: height.max(1.0),
        }
    }

    fn key(text: &TextContent, factor: f32) -> String {
        format!("{:?}|{:08x}", text, factor.to_bits())
    }

    /// Size of the text box (letters plus background padding) in canvas pixels at `factor`.
    pub fn measure(&mut self, text: &TextContent, factor: f32) -> (f32, f32) {
        let key = Self::key(text, factor);
        if let Some(s) = self.sizes.get(&key) {
            return *s;
        }
        let padding = padding(text, factor);
        let laid = self.lay_out(text, factor);
        let size = (laid.width + 2.0 * padding, laid.height + 2.0 * padding);
        if self.sizes.len() > 256 {
            self.sizes.clear();
        }
        self.sizes.insert(key, size);
        size
    }

    /// Draw the text at `factor` canvas pixels per reference pixel.
    pub fn render(&mut self, text: &TextContent, factor: f32) -> Arc<TextImage> {
        let key = Self::key(text, factor);
        if let Some(image) = self.images.get(&key) {
            return image.clone();
        }
        let image = Arc::new(self.draw(text, factor));
        if self.images.len() > 48 {
            self.images.clear();
        }
        self.images.insert(key, image.clone());
        image
    }

    fn draw(&mut self, text: &TextContent, factor: f32) -> TextImage {
        let style = &text.style;
        let laid = self.lay_out(text, factor);
        let pad = padding(text, factor);
        let (box_w, box_h) = (laid.width + 2.0 * pad, laid.height + 2.0 * pad);
        let stroke_w = style.stroke.map_or(0.0, |s| s.width * factor);
        let (shadow_blur, shadow_dx, shadow_dy) = style.shadow.map_or((0.0, 0.0, 0.0), |s| {
            (s.blur * factor, s.dx * factor, s.dy * factor)
        });
        // room around the box for the outline, the shadow and letters that lean out of their box
        let margin = (stroke_w + 2.0)
            .max(shadow_blur * 1.5 + shadow_dx.abs().max(shadow_dy.abs()) + 2.0)
            .max(style.size * factor * 0.15)
            .ceil();
        let (w, h) = (
            (box_w + 2.0 * margin).ceil() as usize,
            (box_h + 2.0 * margin).ceil() as usize,
        );
        // where the letters' area starts inside the picture
        let origin = (margin + pad, margin + pad);

        // the letters as coverage
        let mut letters = Mask::new(w, h);
        let Laid { mut buffer, .. } = laid;
        let fs = self.fonts.as_mut().expect("lay_out made the fonts");
        let swash = self.swash.get_or_insert_with(SwashCache::new);
        let (ox, oy) = (origin.0.round() as i32, origin.1.round() as i32);
        buffer.draw(fs, swash, Color::rgb(255, 255, 255), |x, y, rw, rh, c| {
            let coverage = f32::from(c.a()) / 255.0;
            for dy in 0..rh as i32 {
                for dx in 0..rw as i32 {
                    letters.add(x + dx + ox, y + dy + oy, coverage);
                }
            }
        });

        let mut canvas = Layers::new(w, h);
        if let Some(bg) = style.background {
            canvas.rounded_box((margin, margin, box_w, box_h), bg.radius * factor, bg.color);
        }
        let outline = (stroke_w > 0.0).then(|| letters.grown(stroke_w));
        if let Some(shadow) = style.shadow {
            let source = outline.as_ref().unwrap_or(&letters);
            let moved = source.shifted(shadow_dx.round() as i32, shadow_dy.round() as i32);
            canvas.fill(&moved.blurred(shadow_blur / 2.0), shadow.color);
        }
        if let (Some(outline), Some(stroke)) = (&outline, style.stroke) {
            canvas.fill(outline, stroke.color);
        }
        canvas.fill(&letters, style.color);

        TextImage {
            width: w as u32,
            height: h as u32,
            rgba: canvas.into_rgba(),
            content: (box_w, box_h),
        }
    }
}

/// Space between the letters and the edge of the background box, in canvas pixels.
fn padding(text: &TextContent, factor: f32) -> f32 {
    text.style.background.map_or(0.0, |b| b.padding * factor)
}

/// A picture being built up from layers (pre-multiplied colours, so layers can be stacked simply).
struct Layers {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Layers {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }

    fn over(px: &mut [f32; 4], color: [u8; 4], coverage: f32) {
        let a = (f32::from(color[3]) / 255.0 * coverage).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let keep = 1.0 - a;
        for i in 0..3 {
            px[i] = px[i] * keep + f32::from(color[i]) / 255.0 * a;
        }
        px[3] = px[3] * keep + a;
    }

    fn fill(&mut self, mask: &Mask, color: [u8; 4]) {
        for (px, c) in self.px.iter_mut().zip(&mask.a) {
            if *c > 0.0 {
                Self::over(px, color, *c);
            }
        }
    }

    /// A rounded rectangle `(x, y, width, height)` with soft edges.
    fn rounded_box(&mut self, rect: (f32, f32, f32, f32), radius: f32, color: [u8; 4]) {
        let (x, y, bw, bh) = rect;
        let r = radius.clamp(0.0, bw.min(bh) / 2.0);
        let (cx, cy) = (x + bw / 2.0, y + bh / 2.0);
        let inner = (bw / 2.0 - r, bh / 2.0 - r);
        for py in 0..self.h {
            for px in 0..self.w {
                let (dx, dy) = (
                    ((px as f32 + 0.5) - cx).abs() - inner.0,
                    ((py as f32 + 0.5) - cy).abs() - inner.1,
                );
                let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
                let dist = outside + dx.max(dy).min(0.0) - r;
                let coverage = (0.5 - dist).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    Self::over(&mut self.px[py * self.w + px], color, coverage);
                }
            }
        }
    }

    /// Straight RGBA bytes.
    fn into_rgba(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in self.px {
            let a = p[3];
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for c in &p[..3] {
                out.push((c / a * 255.0 + 0.5).clamp(0.0, 255.0) as u8);
            }
            out.push((a * 255.0 + 0.5).clamp(0.0, 255.0) as u8);
        }
        out
    }
}

#[cfg(test)]
mod tests;
