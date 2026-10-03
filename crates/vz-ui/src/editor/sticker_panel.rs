//! The Stickers tab of the asset panel: ready-made stickers and plain shapes, added as layers.

use super::{Ctx, inspector};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind, vec2,
};
use vz_core::US_PER_SEC;
use vz_presets::{ShapePreset, basic_shapes, sticker_presets};
use vz_vector::ShapeCache;

const TILE: f32 = 84.0;
/// How long a new shape stays on screen.
const NEW_SHAPE_LENGTH: i64 = 4 * US_PER_SEC;
/// The size of a new shape on the canvas, as a share of the canvas's short side.
const NEW_SHAPE_SHARE: f32 = 0.3;

thread_local! {
    static CACHE: std::cell::RefCell<ShapeCache> = std::cell::RefCell::new(ShapeCache::new());
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let presets = if ctx.state.sub_menu == 0 {
        sticker_presets()
    } else {
        basic_shapes()
    };
    ui.label(
        RichText::new("Click one to add it at the playhead.")
            .color(theme::MUTED)
            .small(),
    );
    if ctx.state.sub_menu != 0 {
        ui.add_space(6.0);
        let drawing = ctx.state.pen.as_ref().is_some_and(|p| p.clip.is_none());
        if ui
            .selectable_label(drawing, RichText::new("Pen tool: draw your own shape").strong())
            .on_hover_text("Click on the picture to add points, drag to make curves, click the first point to close the shape")
            .clicked()
        {
            ctx.state.pen = (!drawing).then(super::pen::PenSession::draw);
            ctx.state.selection.clear();
        }
    }
    ui.add_space(8.0);
    let search = ctx.state.search.to_lowercase();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for preset in presets
                    .iter()
                    .filter(|p| search.is_empty() || p.name.to_lowercase().contains(&search))
                {
                    if tile(ui, ctx, preset).clicked() {
                        add(ctx, preset);
                    }
                }
            });
        });
}

/// Add a shape at the playhead, sized to a good part of the picture.
fn add(ctx: &mut Ctx<'_>, preset: &ShapePreset) {
    let mut shape = preset.shape.clone();
    // presets are drawn for a 1080 canvas; the same size looks right on any canvas because sizes are in reference pixels
    let k = (NEW_SHAPE_SHARE * 1080.0 / shape.width.max(shape.height).max(1.0)).min(1.4);
    // lines keep their length
    if preset.shape.kind != vz_core::ShapeKind::Line {
        shape.width *= k;
        shape.height *= k;
        if let Some(s) = &mut shape.stroke {
            s.width *= k;
        }
    }
    match ctx
        .doc
        .add_shape_auto(ctx.state.playhead, NEW_SHAPE_LENGTH, shape)
    {
        Ok((_, id)) => {
            ctx.state.selection = vec![id];
            ctx.state.inspector_tab = inspector::Tab::Shape;
        }
        Err(e) => ctx.state.say(format!("Could not add the shape: {e}")),
    }
}

fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, preset: &ShapePreset) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE, TILE + 18.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE, TILE));
    let painter = ui.painter();
    painter.rect_filled(card, CornerRadius::same(8), Color32::from_rgb(30, 30, 30));
    if resp.hovered() {
        painter.rect_stroke(
            card,
            CornerRadius::same(8),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    if let Some(texture) = sample(ui.ctx(), ctx, preset) {
        let size = texture.size_vec2();
        let k = ((TILE - 16.0) / size.x).min((TILE - 16.0) / size.y);
        painter.image(
            texture.id(),
            egui::Rect::from_center_size(card.center(), size * k),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 7.0),
        Align2::CENTER_CENTER,
        preset.name,
        FontId::proportional(10.0),
        theme::TEXT,
    );
    resp.on_hover_text(preset.name)
}

fn sample(
    egui_ctx: &egui::Context,
    ctx: &mut Ctx<'_>,
    preset: &ShapePreset,
) -> Option<egui::TextureHandle> {
    let key = format!("shape:{}:{}", ctx.state.sub_menu, preset.name);
    if let Some(t) = ctx.state.preset_tiles.get(&key) {
        return Some(t.clone());
    }
    // about 64 pixels on the longest side
    let factor = 64.0 / preset.shape.width.max(preset.shape.height).max(1.0);
    let image = CACHE.with(|c| c.borrow_mut().render(&preset.shape, factor));
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.rgba,
    );
    let texture = egui_ctx.load_texture(key.clone(), color, egui::TextureOptions::LINEAR);
    ctx.state.preset_tiles.insert(key, texture.clone());
    Some(texture)
}
