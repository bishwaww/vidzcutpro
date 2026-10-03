//! "Text templates" in the Text tab: ready-made groups of text and shapes with animation. Tiles are drawn with the
//! same renderer as the player, on a small canvas.

use super::{Ctx, inspector};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind, vec2,
};
use vz_core::{Background, Document, Project};
use vz_presets::templates::{Template, templates};
use vz_render::Renderer;

const TILE_W: f32 = 200.0;
const TILE_H: f32 = TILE_W * 9.0 / 16.0;

thread_local! {
    static RENDERER: std::cell::RefCell<Renderer> = std::cell::RefCell::new(Renderer::new());
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(
        RichText::new("Click one to add it at the playhead. Its parts are grouped: click one to select them all, Alt+click to pick a single part.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(8.0);
    let search = ctx.state.search.to_lowercase();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for t in templates()
                    .into_iter()
                    .filter(|t| search.is_empty() || t.name.to_lowercase().contains(&search))
                {
                    if tile(ui, ctx, &t).clicked() {
                        add(ctx, &t);
                    }
                }
            });
        });
}

/// Put a template on the timeline at the playhead (its first text is selected so it can be typed over).
pub fn add(ctx: &mut Ctx<'_>, template: &Template) {
    let canvas = ctx.doc.project.canvas;
    let parts = template.build(canvas.width, canvas.height);
    match ctx.doc.add_group(parts, ctx.state.playhead, "Add template") {
        Ok(ids) => {
            // select the first text so the inspector shows its words
            let first_text = ids.iter().copied().find(|id| {
                ctx.doc
                    .project
                    .timeline()
                    .clip(*id)
                    .is_some_and(vz_core::Clip::is_text)
            });
            ctx.state.selection = first_text.map(|id| vec![id]).unwrap_or(ids);
            ctx.state.inspector_tab = inspector::Tab::Text;
            ctx.state.focus_text = true;
        }
        Err(e) => ctx.state.say(format!("Could not add the template: {e}")),
    }
}

fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, template: &Template) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE_W, TILE_H));
    let painter = ui.painter();
    painter.rect_filled(card, CornerRadius::same(8), Color32::from_rgb(30, 30, 30));
    if let Some(texture) = picture(ui.ctx(), ctx, template) {
        painter.image(
            texture.id(),
            card.shrink(3.0),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if resp.hovered() {
        painter.rect_stroke(
            card,
            CornerRadius::same(8),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        template.name,
        FontId::proportional(11.0),
        theme::TEXT,
    );
    resp.on_hover_text(template.name)
}

/// The template as it looks a little after it has come in, on a plain blue backdrop.
fn picture(
    egui_ctx: &egui::Context,
    ctx: &mut Ctx<'_>,
    template: &Template,
) -> Option<egui::TextureHandle> {
    let key = format!("template:{}", template.id);
    if let Some(t) = ctx.state.preset_tiles.get(&key) {
        return Some(t.clone());
    }
    let mut project = Project::new("tile");
    project.background = Background::Color([52, 66, 96, 255]);
    let mut doc = Document::new(project);
    let parts = template.build(1920, 1080);
    doc.add_group(parts, 0, "tile").ok()?;
    let at = template.duration * 6 / 10;
    let (w, h) = (TILE_W as u32 * 2, TILE_H as u32 * 2);
    let frame = RENDERER.with(|r| r.borrow_mut().render(&doc.project, at, (w, h)));
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    );
    let texture = egui_ctx.load_texture(key.clone(), image, egui::TextureOptions::LINEAR);
    ctx.state.preset_tiles.insert(key, texture.clone());
    Some(texture)
}
