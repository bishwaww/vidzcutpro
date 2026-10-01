//! Sinhala spike (Phase 0, step 0c). THROWAWAY test, not part of the real app.
//!
//! Question: does egui's own text engine show Sinhala correctly, compared with
//! cosmic-text (which is what vidzcut will use for text drawn into the video)?
//!
//! Left column  = egui's own labels + an editable box.
//! Right column = the same words shaped by cosmic-text and shown as a picture.
//! The window saves a screenshot to `out/sinhala_spike.png` and closes itself
//! (pass `--stay` to keep it open and type into the box).

use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use eframe::egui;

const LINES: [&str; 5] = [
    "ශ්‍රී ලංකාව",
    "ක්‍රීඩාව",
    "ඇස් සම්බන්ධ රෝග",
    "Hello vidzcut — සිංහල + English",
    "தமிழ் எழுத்து",
];

struct App {
    cosmic_image: egui::ColorImage,
    cosmic_texture: Option<egui::TextureHandle>,
    editable: String,
    frames: u32,
    stay: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, stay: bool) -> Self {
        install_system_font(&cc.egui_ctx);
        Self {
            cosmic_image: render_with_cosmic(),
            cosmic_texture: None,
            editable: LINES[0].to_string(),
            frames: 0,
            stay,
        }
    }
}

/// egui has no Sinhala glyphs of its own, so give it Windows' Nirmala UI as a fallback.
fn install_system_font(ctx: &egui::Context) {
    let path = r"C:\Windows\Fonts\Nirmala.ttc";
    if let Ok(bytes) = std::fs::read(path) {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("nirmala".into(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("nirmala".into());
        }
        ctx.set_fonts(fonts);
    } else {
        eprintln!("Nirmala.ttf not found at {path}");
    }
}

/// Shape the same lines with cosmic-text and draw them into an RGBA picture.
fn render_with_cosmic() -> egui::ColorImage {
    let (w, h) = (620usize, 400usize);
    let mut fs = FontSystem::new();
    let mut swash = SwashCache::new();
    let mut buffer = Buffer::new(&mut fs, Metrics::new(34.0, 52.0));
    let mut buffer = buffer.borrow_with(&mut fs);
    buffer.set_size(Some(w as f32), Some(h as f32));
    let text = LINES.join("\n");
    buffer.set_text(&text, &Attrs::new().family(Family::Name("Nirmala UI")), Shaping::Advanced, None);
    buffer.shape_until_scroll(true);

    let mut pixels = vec![egui::Color32::from_rgb(21, 21, 21); w * h];
    buffer.draw(&mut swash, Color::rgb(252, 198, 16), |x, y, bw, bh, c| {
        for dy in 0..bh as i32 {
            for dx in 0..bw as i32 {
                let (px, py) = (x + dx, y + dy);
                if px < 0 || py < 0 || px as usize >= w || py as usize >= h {
                    continue;
                }
                let a = c.a() as f32 / 255.0;
                let dst = &mut pixels[py as usize * w + px as usize];
                let mix = |s: u8, d: u8| (s as f32 * a + d as f32 * (1.0 - a)) as u8;
                *dst = egui::Color32::from_rgb(mix(c.r(), dst.r()), mix(c.g(), dst.g()), mix(c.b(), dst.b()));
            }
        }
    });
    egui::ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        let texture = self
            .cosmic_texture
            .get_or_insert_with(|| ctx.load_texture("cosmic", self.cosmic_image.clone(), Default::default()))
            .clone();

        egui::CentralPanel::default().show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].heading("egui's own text");
                for line in LINES {
                    cols[0].label(egui::RichText::new(line).size(30.0));
                }
                cols[0].separator();
                cols[0].label("Editable egui box (type Sinhala here):");
                cols[0].add(egui::TextEdit::multiline(&mut self.editable).font(egui::FontId::proportional(30.0)));

                cols[1].heading("cosmic-text (used for video text)");
                cols[1].image((texture.id(), texture.size_vec2()));
            });
        });

        // Take a screenshot after a few frames, save it, and close (unless --stay).
        self.frames += 1;
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let _ = std::fs::create_dir_all("out");
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(img.width() as u32, img.height() as u32, rgba) {
                let _ = buf.save("out/sinhala_spike.png");
                println!("saved out/sinhala_spike.png");
            }
            if !self.stay {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint();
    }
}

fn main() -> eframe::Result {
    let stay = std::env::args().any(|a| a == "--stay");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native("vidzcut Sinhala spike", options, Box::new(move |cc| Ok(Box::new(App::new(cc, stay)))))
}
