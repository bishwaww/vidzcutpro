//! The Text tab of the asset panel: add text, pick a ready-made style, pick a font.

use super::geometry::with_text;
use super::{Ctx, inspector};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind, vec2,
};
use vz_core::{ClipId, Content, TextContent, US_PER_SEC};
use vz_presets::{TextPreset, text_presets};

const TILE_W: f32 = 108.0;
const TILE_H: f32 = 72.0;
/// How long a new text stays on screen.
const NEW_TEXT_LENGTH: i64 = 3 * US_PER_SEC;

/// Which sub-menu entries this panel draws (the others are not built yet).
pub fn handles(sub_menu: usize) -> bool {
    matches!(sub_menu, 0 | 1 | 3)
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    match ctx.state.sub_menu {
        0 => presets_grid(ui, ctx, true),
        1 => presets_grid(ui, ctx, false),
        _ => fonts_list(ui, ctx),
    }
}

/// The text clip that is selected, if exactly one is.
fn selected_text(ctx: &Ctx<'_>) -> Option<ClipId> {
    let [id] = ctx.state.selection[..] else {
        return None;
    };
    ctx.doc
        .project
        .timeline()
        .clip(id)
        .filter(|c| c.is_text())
        .map(|c| c.id)
}

/// Add a new text at the playhead.
pub fn add_text(ctx: &mut Ctx<'_>, preset: &TextPreset) {
    let text = TextContent {
        text: preset.text.to_owned(),
        style: preset.style.clone(),
        ..TextContent::default()
    };
    match ctx
        .doc
        .add_text_auto(ctx.state.playhead, NEW_TEXT_LENGTH, text)
    {
        Ok((_, id)) => {
            ctx.state.selection = vec![id];
            ctx.state.inspector_tab = inspector::Tab::Text;
            ctx.state.focus_text = true;
        }
        Err(e) => ctx.state.say(format!("Could not add the text: {e}")),
    }
}

fn presets_grid(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, adding: bool) {
    let hint = if adding {
        "Click a style to add text at the playhead."
    } else if selected_text(ctx).is_some() {
        "Click a style to give it to the selected text."
    } else {
        "Select a text on the timeline, then click a style."
    };
    ui.label(RichText::new(hint).color(theme::MUTED).small());
    ui.add_space(8.0);
    let search = ctx.state.search.to_lowercase();
    let presets: Vec<TextPreset> = text_presets()
        .into_iter()
        .filter(|p| search.is_empty() || p.name.to_lowercase().contains(&search))
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for preset in &presets {
                    if tile(ui, ctx, preset).clicked() {
                        if adding {
                            add_text(ctx, preset);
                        } else if let Some(id) = selected_text(ctx) {
                            let style = preset.style.clone();
                            let _ = ctx.doc.update_clip(id, "Text style", None, |c| {
                                if let Content::Text(t) = &mut c.content {
                                    t.style = style;
                                }
                            });
                        } else {
                            ctx.state.say("Select a text first.");
                        }
                    }
                }
            });
        });
}

/// One style tile: a small picture of the style on a dark card, and its name underneath.
fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, preset: &TextPreset) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE_W, TILE_H));
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
    if let Some(texture) = sample_texture(ui.ctx(), ctx, preset) {
        let size = texture.size_vec2();
        let k = ((TILE_W - 12.0) / size.x)
            .min((TILE_H - 12.0) / size.y)
            .min(1.0);
        let shown = egui::Rect::from_center_size(card.center(), size * k);
        painter.image(
            texture.id(),
            shown,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        preset.name,
        FontId::proportional(11.0),
        theme::TEXT,
    );
    resp.on_hover_text(preset.name)
}

/// The picture on a tile, made the first time it is needed.
fn sample_texture(
    egui_ctx: &egui::Context,
    ctx: &mut Ctx<'_>,
    preset: &TextPreset,
) -> Option<egui::TextureHandle> {
    let key = format!("text:{}", preset.name);
    if let Some(t) = ctx.state.preset_tiles.get(&key) {
        return Some(t.clone());
    }
    let sample = TextContent {
        text: preset.sample.to_owned(),
        style: preset.style.clone(),
        ..TextContent::default()
    };
    // about 30 pixels tall letters, whatever the style's own size
    let factor = (30.0 / preset.style.size).clamp(0.1, 1.0);
    let image = with_text(|e| e.render(&sample, factor));
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.rgba,
    );
    let texture = egui_ctx.load_texture(key.clone(), color, egui::TextureOptions::LINEAR);
    ctx.state.preset_tiles.insert(key, texture.clone());
    Some(texture)
}

fn fonts_list(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    if ctx.state.fonts.is_empty() {
        ctx.state.fonts = with_text(|e| e.families());
    }
    let target = selected_text(ctx);
    ui.label(
        RichText::new(if target.is_some() {
            "Click a font to use it for the selected text."
        } else {
            "Select a text on the timeline, then click a font."
        })
        .color(theme::MUTED)
        .small(),
    );
    ui.add_space(6.0);
    let search = ctx.state.search.to_lowercase();
    let fonts: Vec<String> = ctx
        .state
        .fonts
        .iter()
        .filter(|f| search.is_empty() || f.to_lowercase().contains(&search))
        .cloned()
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for name in fonts {
                let resp = ui.selectable_label(false, RichText::new(&name).size(15.0));
                if resp.clicked() {
                    match target {
                        Some(id) => {
                            let _ = ctx.doc.update_clip(id, "Font", None, |c| {
                                if let Content::Text(t) = &mut c.content {
                                    t.style.font = name.clone();
                                }
                            });
                        }
                        None => ctx.state.say("Select a text first."),
                    }
                }
            }
        });
}
