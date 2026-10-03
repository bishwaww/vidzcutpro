//! Fonts and text: turns a piece of styled text into a picture with transparent background.
//!
//! Letters are shaped by cosmic-text (Sinhala and Tamil conjuncts, vowel signs and Latin all come out
//! right, and a letter group is never cut in the middle). On top of the plain letters we draw gradients,
//! outlines (up to three), glow, shadow and the background box, bend the line along an arc if asked, and
//! move letters, words or lines about for text animators. The renderer puts that picture on the canvas
//! like any other layer, so a text layer can be moved, scaled, turned, faded and blended.
//!
//! Sizes in a [`TextStyle`] are "reference pixels": the size they would have on a canvas whose short
//! side is 1080 pixels. [`unit`] converts them to the real canvas.

pub mod animate;
mod curve;
mod mask;
pub mod rich;

use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    SwashContent, Weight, Wrap, fontdb,
};
use mask::Mask;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use vz_core::{Micros, TextAlign, TextContent, TextUnit};

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
    /// The text after case changes (what was really laid out).
    text: String,
}

/// One pixel of a letter, and which unit of text (letter group, word, line) it belongs to.
struct Pix {
    x: i32,
    y: i32,
    coverage: f32,
    rgb: [u8; 3],
    /// Indices of the letter group, word and line this pixel is part of.
    unit: [usize; 3],
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
        let line_height = size * LINE_HEIGHT * style.line_spacing.max(0.2);
        let metrics = Metrics::new(size, line_height);
        let cased = rich::apply_case(&text.text, style.case);
        let segments = rich::segments(&cased, &text.spans, style.color);
        let base_family = if style.font.is_empty() {
            DEFAULT_FONT
        } else {
            style.font.as_str()
        };
        let base_color = Color::rgba(
            style.color[0],
            style.color[1],
            style.color[2],
            style.color[3],
        );
        let default_attrs = Attrs::new()
            .family(Family::Name(base_family))
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
            .color(base_color)
            .letter_spacing(style.letter_spacing);
        let pieces: Vec<(&str, Attrs)> = segments
            .iter()
            .map(|seg| {
                let bold = seg.bold.unwrap_or(style.bold);
                let italic = seg.italic.unwrap_or(style.italic);
                let color = seg
                    .color
                    .map_or(base_color, |c| Color::rgba(c[0], c[1], c[2], c[3]));
                let family = seg.font.as_deref().unwrap_or(base_family);
                let mut attrs = Attrs::new()
                    .family(Family::Name(family))
                    .weight(if bold { Weight::BOLD } else { Weight::NORMAL })
                    .style(if italic { Style::Italic } else { Style::Normal })
                    .color(color)
                    .letter_spacing(style.letter_spacing);
                if (seg.size - 1.0).abs() > 1e-3 {
                    attrs = attrs.metrics(Metrics::new(size * seg.size, line_height * seg.size));
                }
                (seg.text.as_str(), attrs)
            })
            .collect();
        let align = match style.align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
            TextAlign::Justify => Align::Justified,
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
            if pieces.is_empty() {
                b.set_text("", &default_attrs, Shaping::Advanced, Some(align));
            } else {
                b.set_rich_text(pieces, &default_attrs, Shaping::Advanced, Some(align));
            }
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
            let heights: f32 = b.layout_runs().map(|r| r.line_height).sum();
            (width, heights.max(line_height))
        };
        Laid {
            buffer,
            width: width.max(1.0),
            height: height.max(1.0),
            text: cased,
        }
    }

    fn key(text: &TextContent, factor: f32) -> String {
        format!("{:?}|{:08x}", text, factor.to_bits())
    }

    /// Size of the text box (letters plus background padding) in canvas pixels at `factor`.
    pub fn measure(&mut self, text: &TextContent, factor: f32) -> (f32, f32) {
        let mut plain = text.clone();
        plain.words = vz_core::WordMode::Off;
        let key = Self::key(&plain, factor);
        if let Some(s) = self.sizes.get(&key) {
            return *s;
        }
        let padding = padding(text, factor);
        let laid = self.lay_out(&plain, factor);
        let size = (laid.width + 2.0 * padding, laid.height + 2.0 * padding);
        if self.sizes.len() > 256 {
            self.sizes.clear();
        }
        self.sizes.insert(key, size);
        size
    }

    /// Draw the text at `factor` canvas pixels per reference pixel (as it looks at the start of its clip).
    pub fn render(&mut self, text: &TextContent, factor: f32) -> Arc<TextImage> {
        self.render_at(text, factor, 0, 0)
    }

    /// Draw the text as it looks `local` microseconds into a clip that lasts `duration`: caption word effects and text
    /// animators depend on the time.
    pub fn render_at(
        &mut self,
        text: &TextContent,
        factor: f32,
        local: Micros,
        duration: Micros,
    ) -> Arc<TextImage> {
        let timed = text.words != vz_core::WordMode::Off || !text.animators.is_empty();
        let effective = rich::effective(text, local, duration);
        let mut key = Self::key(&effective, factor);
        if !text.animators.is_empty() {
            key.push_str(&format!("|{local}|{duration}"));
        }
        let _ = timed;
        if let Some(image) = self.images.get(&key) {
            return image.clone();
        }
        let image = Arc::new(self.draw(&effective, factor, local, duration));
        if self.images.len() > 48 {
            self.images.clear();
        }
        self.images.insert(key, image.clone());
        image
    }

    /// Every pixel of every letter, with the colour it has and the unit it belongs to.
    fn letter_pixels(
        &mut self,
        laid: &mut Laid,
        style: &vz_core::TextStyle,
        factor: f32,
        origin: (i32, i32),
    ) -> Vec<Pix> {
        let base = [
            style.color[0],
            style.color[1],
            style.color[2],
            style.color[3],
        ];
        // where each line starts in the whole text, for working out the number of a letter group and a word
        let lines: Vec<&str> = laid.text.split('\n').collect();
        let mut line_start = Vec::with_capacity(lines.len());
        let mut at = 0;
        for l in &lines {
            line_start.push(at);
            at += l.chars().count() + 1;
        }
        let words = rich::word_ranges(&laid.text);
        let word_of = |char_index: usize| -> usize {
            words
                .iter()
                .rposition(|(s, _)| *s <= char_index)
                .unwrap_or(0)
        };
        let fs = self.fonts.as_mut().expect("lay_out made the fonts");
        let swash = self.swash.get_or_insert_with(SwashCache::new);
        let mut out = Vec::new();
        // letter groups in order of appearance: (line, start) -> number
        let mut groups: Vec<(usize, usize)> = Vec::new();
        let mut group_of = |line: usize, start: usize| -> usize {
            match groups.iter().position(|g| *g == (line, start)) {
                Some(i) => i,
                None => {
                    groups.push((line, start));
                    groups.len() - 1
                }
            }
        };
        let mut underline: Vec<(f32, f32, f32, f32)> = Vec::new(); // x0, x1, baseline, glyph size
        for run in laid.buffer.layout_runs() {
            let line_text = lines.get(run.line_i).copied().unwrap_or("");
            for glyph in run.glyphs {
                let phys = glyph.physical((0.0, run.line_y), 1.0);
                let color = glyph
                    .color_opt
                    .unwrap_or(Color::rgba(base[0], base[1], base[2], base[3]));
                let alpha = f32::from(color.a()) / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let char_in_line = line_text
                    .get(..glyph.start.min(line_text.len()))
                    .map_or(0, |s| s.chars().count());
                let char_index = line_start.get(run.line_i).copied().unwrap_or(0) + char_in_line;
                let unit = [
                    group_of(run.line_i, glyph.start),
                    word_of(char_index),
                    run.line_i,
                ];
                let Some(image) = swash.get_image(fs, phys.cache_key).as_ref() else {
                    continue;
                };
                let (px0, py0) = (
                    phys.x + image.placement.left + origin.0,
                    phys.y - image.placement.top + origin.1,
                );
                let (iw, ih) = (image.placement.width as i32, image.placement.height as i32);
                for iy in 0..ih {
                    for ix in 0..iw {
                        let i = (iy * iw + ix) as usize;
                        let (cov, rgb) = match image.content {
                            SwashContent::Mask => (
                                f32::from(image.data[i]) / 255.0,
                                [color.r(), color.g(), color.b()],
                            ),
                            SwashContent::Color => {
                                let d = &image.data[i * 4..i * 4 + 4];
                                (f32::from(d[3]) / 255.0, [d[0], d[1], d[2]])
                            }
                            SwashContent::SubpixelMask => {
                                let d = &image.data[i * 3..i * 3 + 3];
                                (
                                    (f32::from(d[0]) + f32::from(d[1]) + f32::from(d[2])) / 765.0,
                                    [color.r(), color.g(), color.b()],
                                )
                            }
                        };
                        if cov > 0.0 {
                            out.push(Pix {
                                x: px0 + ix,
                                y: py0 + iy,
                                coverage: cov * alpha,
                                rgb,
                                unit,
                            });
                        }
                    }
                }
            }
            if style.underline && !run.glyphs.is_empty() {
                let first = run.glyphs[0].x;
                let last = run.glyphs.iter().map(|g| g.x + g.w).fold(first, f32::max);
                underline.push((first, last, run.line_y, run.glyphs[0].font_size));
            }
        }
        for (x0, x1, y, size) in underline {
            let thickness = (size * 0.06).max(1.0);
            let top = y + size * 0.12;
            for yy in 0..thickness.ceil() as i32 {
                for xx in x0.floor() as i32..x1.ceil() as i32 {
                    out.push(Pix {
                        x: xx + origin.0,
                        y: top as i32 + yy + origin.1,
                        coverage: 1.0,
                        rgb: [base[0], base[1], base[2]],
                        unit: [usize::MAX, usize::MAX, usize::MAX],
                    });
                }
            }
        }
        let _ = factor;
        out
    }

    fn draw(
        &mut self,
        text: &TextContent,
        factor: f32,
        local: Micros,
        duration: Micros,
    ) -> TextImage {
        let style = &text.style;
        let mut laid = self.lay_out(text, factor);
        let pad = padding(text, factor);
        let (box_w, box_h) = (laid.width + 2.0 * pad, laid.height + 2.0 * pad);
        // outlines from the inside out: radius of each ring counted from the letters
        let mut radii = Vec::new();
        let mut colours = Vec::new();
        let mut r = 0.0;
        for s in style.stroke.iter().chain(style.more_strokes.iter().take(2)) {
            r += s.width * factor;
            radii.push(r);
            colours.push(s.color);
        }
        let stroke_w = r;
        let (shadow_blur, shadow_dx, shadow_dy) = style.shadow.map_or((0.0, 0.0, 0.0), |s| {
            (s.blur * factor, s.dx * factor, s.dy * factor)
        });
        let glow_r = style.glow.map_or(0.0, |g| g.radius * factor);
        let animated = !text.animators.is_empty();
        let moves = text
            .animators
            .iter()
            .map(|a| {
                a.dx.abs().max(a.dy.abs()) * factor + style.size * factor * 0.4 * a.scale.max(1.0)
            })
            .fold(0.0_f32, f32::max);
        // room around the box for the outlines, glow, shadow, moving letters and letters that lean out of their box
        let margin = (stroke_w + 2.0)
            .max(shadow_blur * 1.5 + shadow_dx.abs().max(shadow_dy.abs()) + 2.0)
            .max(glow_r * 2.0 + 2.0)
            .max(style.size * factor * 0.15)
            .max(if animated { moves } else { 0.0 })
            .max(stroke_w + glow_r * 2.0 + 2.0)
            .ceil();
        let (w, h) = (
            (box_w + 2.0 * margin).ceil() as usize,
            (box_h + 2.0 * margin).ceil() as usize,
        );
        // where the letters' area starts inside the picture
        let origin = (margin + pad, margin + pad);
        let (ox, oy) = (origin.0.round() as i32, origin.1.round() as i32);

        let pixels = self.letter_pixels(&mut laid, style, factor, (ox, oy));

        // the letters as coverage, plus the colour of each pixel
        let mut letters = Mask::new(w, h);
        let mut colors = vec![[255u8; 3]; w * h];
        if animated {
            place_animated(
                &pixels,
                text,
                factor,
                local,
                duration,
                &mut letters,
                &mut colors,
                w,
            );
        } else {
            for p in &pixels {
                if p.x >= 0 && p.y >= 0 && (p.x as usize) < w && (p.y as usize) < h {
                    letters.add(p.x, p.y, p.coverage);
                    colors[p.y as usize * w + p.x as usize] = p.rgb;
                }
            }
        }
        if let Some(g) = &style.gradient {
            let (bx, by) = (margin, margin);
            for y in 0..h {
                for x in 0..w {
                    if letters.a[y * w + x] > 0.0 {
                        let (u, v) = (
                            (x as f32 - bx) / box_w.max(1.0),
                            (y as f32 - by) / box_h.max(1.0),
                        );
                        let c = g.at(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                        colors[y * w + x] = [c[0], c[1], c[2]];
                    }
                }
            }
        }

        let mut canvas = Layers::new(w, h);
        if let Some(bg) = style.background {
            canvas.rounded_box((margin, margin, box_w, box_h), bg.radius * factor, bg.color);
        }
        let outermost = radii
            .last()
            .copied()
            .filter(|r| *r > 0.0)
            .map(|r| letters.grown(r));
        if let Some(glow) = style.glow {
            let source = outermost.as_ref().unwrap_or(&letters);
            let soft = source
                .grown(glow.radius * factor * 0.35)
                .blurred(glow.radius * factor * 0.45);
            let mut color = glow.color;
            let strength = glow.strength.clamp(0.0, 1.0);
            color[3] = (f32::from(color[3]) * strength.min(1.0)) as u8;
            canvas.fill(&soft, color);
            if strength > 0.6 {
                canvas.fill(&soft, color);
            }
        }
        if let Some(shadow) = style.shadow {
            let source = outermost.as_ref().unwrap_or(&letters);
            let moved = source.shifted(shadow_dx.round() as i32, shadow_dy.round() as i32);
            canvas.fill(&moved.blurred(shadow_blur / 2.0), shadow.color);
        }
        for (radius, color) in radii.iter().zip(&colours).rev() {
            if *radius > 0.0 {
                canvas.fill(&letters.grown(*radius), *color);
            }
        }
        canvas.fill_colored(&letters, &colors, style.color[3]);

        let mut content = (box_w, box_h);
        let (mut out_w, mut out_h) = (w, h);
        let mut px = canvas.px;
        if style.curve.abs() > 0.5 {
            let bent = curve::bend(&curve::Pixels { w, h, px }, style.curve);
            content = (
                (bent.w as f32 - 2.0 * margin).max(1.0),
                (bent.h as f32 - 2.0 * margin).max(1.0),
            );
            (out_w, out_h) = (bent.w, bent.h);
            px = bent.px;
        }
        TextImage {
            width: out_w as u32,
            height: out_h as u32,
            rgba: pixels_to_rgba(px),
            content,
        }
    }
}

/// Move the letter pixels about as the animators say: each unit of text is turned, sized, faded and shifted around its middle.
#[allow(clippy::too_many_arguments)]
fn place_animated(
    pixels: &[Pix],
    text: &TextContent,
    factor: f32,
    local: Micros,
    duration: Micros,
    letters: &mut Mask,
    colors: &mut [[u8; 3]],
    w: usize,
) {
    let finest = text
        .animators
        .iter()
        .map(|a| match a.unit {
            TextUnit::Character => 0,
            TextUnit::Word => 1,
            TextUnit::Line => 2,
        })
        .min()
        .unwrap_or(0);
    // group the pixels by the unit that moves together
    let mut groups: Vec<(usize, Vec<&Pix>)> = Vec::new();
    for p in pixels {
        let id = p.unit[finest];
        match groups.iter_mut().find(|g| g.0 == id) {
            Some(g) => g.1.push(p),
            None => groups.push((id, vec![p])),
        }
    }
    let counts = [
        pixels
            .iter()
            .map(|p| p.unit[0])
            .filter(|u| *u != usize::MAX)
            .max()
            .map_or(1, |m| m + 1),
        pixels
            .iter()
            .map(|p| p.unit[1])
            .filter(|u| *u != usize::MAX)
            .max()
            .map_or(1, |m| m + 1),
        pixels
            .iter()
            .map(|p| p.unit[2])
            .filter(|u| *u != usize::MAX)
            .max()
            .map_or(1, |m| m + 1),
    ];
    for (_, group) in &groups {
        let first = group[0];
        let st = if first.unit[0] == usize::MAX {
            animate::UnitState::default() // an underline is not animated
        } else {
            animate::combined(&text.animators, first.unit, counts, local, duration)
        };
        let (min_x, max_x) = (
            group.iter().map(|p| p.x).min().unwrap_or(0),
            group.iter().map(|p| p.x).max().unwrap_or(0),
        );
        let (min_y, max_y) = (
            group.iter().map(|p| p.y).min().unwrap_or(0),
            group.iter().map(|p| p.y).max().unwrap_or(0),
        );
        if st.is_neutral() {
            for p in group {
                if p.x >= 0 && p.y >= 0 && (p.x as usize) < w && (p.y as usize) < letters.h {
                    letters.add(p.x, p.y, p.coverage);
                    colors[p.y as usize * w + p.x as usize] = p.rgb;
                }
            }
            continue;
        }
        if st.opacity <= 0.0 || st.scale.abs() < 1e-3 {
            continue;
        }
        let (gw, gh) = ((max_x - min_x + 1) as usize, (max_y - min_y + 1) as usize);
        let mut cov = vec![0.0f32; gw * gh];
        let mut rgb = vec![[255u8; 3]; gw * gh];
        for p in group {
            let i = (p.y - min_y) as usize * gw + (p.x - min_x) as usize;
            cov[i] = (cov[i] + p.coverage).min(1.0);
            rgb[i] = p.rgb;
        }
        let centre = (
            (min_x + max_x) as f32 / 2.0 + 0.5,
            (min_y + max_y) as f32 / 2.0 + 0.5,
        );
        let (sin, cos) = st.rotation.to_radians().sin_cos();
        let (move_x, move_y) = (st.dx * factor, st.dy * factor);
        // the area the moved group can touch
        let reach = (gw.max(gh) as f32 * st.scale.abs() * 1.5 + 4.0).ceil() as i32;
        let (cx, cy) = (centre.0 + move_x, centre.1 + move_y);
        for y in (cy as i32 - reach)..=(cy as i32 + reach) {
            for x in (cx as i32 - reach)..=(cx as i32 + reach) {
                if x < 0 || y < 0 || x as usize >= w || y as usize >= letters.h {
                    continue;
                }
                // back from the moved position to the original place
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let (rx, ry) = (
                    (dx * cos + dy * sin) / st.scale,
                    (-dx * sin + dy * cos) / st.scale,
                );
                let (sx, sy) = (centre.0 + rx - min_x as f32, centre.1 + ry - min_y as f32);
                if sx < 0.0 || sy < 0.0 || sx >= gw as f32 || sy >= gh as f32 {
                    continue;
                }
                let (fx, fy) = ((sx - 0.5).max(0.0), (sy - 0.5).max(0.0));
                let (ix, iy) = (
                    (fx.floor() as usize).min(gw - 1),
                    (fy.floor() as usize).min(gh - 1),
                );
                let (ix2, iy2) = ((ix + 1).min(gw - 1), (iy + 1).min(gh - 1));
                let (tx, ty) = (fx - ix as f32, fy - iy as f32);
                let at = |a: usize, b: usize| cov[b * gw + a];
                let c = (at(ix, iy) * (1.0 - tx) + at(ix2, iy) * tx) * (1.0 - ty)
                    + (at(ix, iy2) * (1.0 - tx) + at(ix2, iy2) * tx) * ty;
                if c > 0.0 {
                    letters.add(x, y, c * st.opacity.clamp(0.0, 1.0));
                    let nearest = rgb[(sy as usize).min(gh - 1) * gw + (sx as usize).min(gw - 1)];
                    colors[y as usize * w + x as usize] = nearest;
                }
            }
        }
    }
}

/// Space between the letters and the edge of the background box, in canvas pixels.
fn padding(text: &TextContent, factor: f32) -> f32 {
    text.style.background.map_or(0.0, |b| b.padding * factor)
}

fn pixels_to_rgba(px: Vec<[f32; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for p in px {
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

    /// Fill with a colour for each pixel, scaled by `alpha`.
    fn fill_colored(&mut self, mask: &Mask, colors: &[[u8; 3]], alpha: u8) {
        for ((px, c), rgb) in self.px.iter_mut().zip(&mask.a).zip(colors) {
            if *c > 0.0 {
                Self::over(px, [rgb[0], rgb[1], rgb[2], alpha], *c);
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
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_rich;
