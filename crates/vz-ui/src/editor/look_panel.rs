//! The Filters, Effects and Adjustment tabs of the asset panel: pick a look and put it on a clip or on the timeline.
//! Every tile shows the look on a little sample picture; effects play when the mouse is over them.

use super::{Ctx, inspector};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind,
    TextureOptions, vec2,
};
use vz_core::{Adjust, ClipId, EffectRef, US_PER_SEC};
use vz_media::VideoFrame;
use vz_presets::effects::{EffectPreset, Group, effects};
use vz_presets::filters::{FilterPreset, filters};
use vz_render::lut::LutCache;

const TILE_W: f32 = 108.0;
const TILE_H: f32 = 64.0;
const SAMPLE: (u32, u32) = (108, 64);
/// How long a filter or effect clip put on the timeline lasts.
const LAYER_LENGTH: i64 = 5 * US_PER_SEC;

/// A colourful little picture to show looks on: sky, sun, hills, a face and a few bright objects.
fn sample_scene() -> VideoFrame {
    let (w, h) = SAMPLE;
    let mut f = VideoFrame::black(w, h);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            // sky: deep blue at the top, warm near the horizon
            let mut c = [
                60.0 + 190.0 * v.powf(1.6),
                110.0 + 80.0 * v.powf(1.3),
                200.0 - 90.0 * v,
            ];
            // sun
            let d = ((u - 0.78).powi(2) * 2.4 + (v - 0.3).powi(2)).sqrt();
            if d < 0.12 {
                c = [255.0, 235.0, 150.0];
            }
            // hills
            let hill = 0.62 + 0.08 * (u * 9.0).sin() + 0.05 * (u * 17.0 + 1.0).sin();
            if v > hill {
                c = [40.0 + 40.0 * u, 140.0 - 40.0 * (v - hill) * 4.0, 50.0];
            }
            // a face (skin tone) with a dark hair cap
            let face = ((u - 0.25).powi(2) * 2.4 + (v - 0.55).powi(2)).sqrt();
            if face < 0.17 {
                c = [224.0, 172.0, 140.0];
            } else if face < 0.2 && v < 0.5 {
                c = [50.0, 35.0, 30.0];
            }
            // a red and a blue block
            if (0.5..0.6).contains(&u) && v > 0.7 {
                c = [210.0, 40.0, 40.0];
            }
            if (0.62..0.72).contains(&u) && v > 0.72 {
                c = [40.0, 70.0, 200.0];
            }
            let i = ((y * w + x) * 4) as usize;
            f.rgba[i] = c[0].clamp(0.0, 255.0) as u8;
            f.rgba[i + 1] = c[1].clamp(0.0, 255.0) as u8;
            f.rgba[i + 2] = c[2].clamp(0.0, 255.0) as u8;
        }
    }
    f
}

thread_local! {
    static SCENE: VideoFrame = sample_scene();
}

fn to_image(f: &VideoFrame) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba)
}

fn filter_image(preset: &FilterPreset) -> ColorImage {
    SCENE.with(|scene| {
        let mut f = scene.clone();
        vz_render::color::apply(&mut f, &preset.adjust, 0, &mut LutCache::default());
        to_image(&f)
    })
}

fn effect_image(preset: &EffectPreset, time: f32) -> ColorImage {
    SCENE.with(|scene| {
        let mut f = scene.clone();
        vz_render::effects::apply(preset.kind, &preset.defaults(), &mut f, time, 0);
        to_image(&f)
    })
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    use super::Category;
    match ctx.state.category {
        Category::Filters => filters_grid(ui, ctx),
        Category::Effects => effects_grid(ui, ctx),
        _ => adjustment_panel(ui, ctx),
    }
}

/// The clip a filter or effect should go on: the one selected clip, if it is a picture clip.
fn selected_visual(ctx: &Ctx<'_>) -> Option<ClipId> {
    let [id] = ctx.state.selection[..] else {
        return None;
    };
    let tl = ctx.doc.project.timeline();
    let clip = tl.clip(id)?;
    let track = tl.track_of(id)?;
    (track.kind.is_visual() && !clip.is_adjustment()).then_some(id)
}

/// Put an adjustment layer holding `adjust` and `effects` on the timeline at the playhead and select it.
fn add_layer(ctx: &mut Ctx<'_>, adjust: Adjust, effects: Vec<EffectRef>, open: inspector::Tab) {
    match ctx
        .doc
        .add_adjustment_auto(ctx.state.playhead, LAYER_LENGTH, adjust, effects)
    {
        Ok((_, id)) => {
            ctx.state.selection = vec![id];
            ctx.state.inspector_tab = open;
        }
        Err(e) => ctx.state.say(format!("Could not add it: {e}")),
    }
}

fn apply_filter(ctx: &mut Ctx<'_>, preset: &FilterPreset) {
    match selected_visual(ctx) {
        Some(id) => {
            let adjust = preset.adjust.clone();
            let _ = ctx
                .doc
                .update_clip(id, "Filter", None, |c| c.adjust = adjust);
            ctx.state.inspector_tab = inspector::Tab::Adjust;
        }
        None => add_layer(
            ctx,
            preset.adjust.clone(),
            Vec::new(),
            inspector::Tab::Adjust,
        ),
    }
}

fn effect_ref(preset: &EffectPreset) -> EffectRef {
    EffectRef {
        preset: preset.id.to_owned(),
        params: preset.defaults(),
        enabled: true,
    }
}

fn apply_effect(ctx: &mut Ctx<'_>, preset: &EffectPreset, as_layer: bool) {
    match (selected_visual(ctx), as_layer) {
        (Some(id), false) => {
            let fx = effect_ref(preset);
            let _ = ctx
                .doc
                .update_clip(id, "Effect", None, |c| c.effects.push(fx));
            ctx.state.inspector_tab = inspector::Tab::Effects;
        }
        _ => add_layer(
            ctx,
            Adjust::default(),
            vec![effect_ref(preset)],
            inspector::Tab::Effects,
        ),
    }
}

/// One tile: the picture, the name under it. Returns the response and whether the mouse was on it.
fn tile(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    key: String,
    name: &str,
    animated: bool,
    image: impl Fn(f32) -> ColorImage,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE_W, TILE_H));
    // the picture is remade only while the mouse is on an animated tile (and once more when it leaves)
    let was_hovered = ctx.state.tile_hover.as_deref() == Some(key.as_str());
    let redraw = animated && (resp.hovered() || was_hovered);
    if resp.hovered() {
        ctx.state.tile_hover = Some(key.clone());
        if animated {
            ui.ctx().request_repaint();
        }
    } else if was_hovered {
        ctx.state.tile_hover = None;
    }
    let time = if resp.hovered() {
        ui.input(|i| i.time) as f32
    } else {
        0.5
    };
    let texture = match ctx.state.preset_tiles.get_mut(&key) {
        Some(tex) => {
            if redraw {
                tex.set(image(time), TextureOptions::LINEAR);
            }
            tex.clone()
        }
        None => {
            let tex = ui
                .ctx()
                .load_texture(key.clone(), image(time), TextureOptions::LINEAR);
            ctx.state.preset_tiles.insert(key, tex.clone());
            tex
        }
    };
    let painter = ui.painter();
    painter.image(
        texture.id(),
        card,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if resp.hovered() {
        painter.rect_stroke(
            card,
            CornerRadius::same(4),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(11.0),
        theme::TEXT,
    );
    resp.on_hover_text(name)
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED).small());
    ui.add_space(8.0);
}

fn filters_grid(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    hint(
        ui,
        "Click a filter to give it to the selected clip. With nothing selected it goes on the timeline as a layer over everything below.",
    );
    let group = ctx
        .state
        .sub_menu
        .min(vz_presets::filters::GROUPS.len() - 1);
    let search = ctx.state.search.to_lowercase();
    let list: Vec<&'static FilterPreset> = filters()
        .iter()
        .filter(|p| p.group == group)
        .filter(|p| search.is_empty() || p.name.to_lowercase().contains(&search))
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for p in list {
                    if tile(ui, ctx, format!("filter:{}", p.id), p.name, false, |_| {
                        filter_image(p)
                    })
                    .clicked()
                    {
                        apply_filter(ctx, p);
                    }
                }
            });
        });
}

fn effects_grid(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let Some(group) = Group::ALL.get(ctx.state.sub_menu).copied() else {
        hint(ui, "Picture frames arrive in a later version.");
        return;
    };
    hint(
        ui,
        "Click an effect to put it on the selected clip. With nothing selected it goes on the timeline as an effect clip. Right-click for more choices.",
    );
    let search = ctx.state.search.to_lowercase();
    let list: Vec<&'static EffectPreset> = effects()
        .iter()
        .filter(|e| e.group == group)
        .filter(|e| search.is_empty() || e.name.to_lowercase().contains(&search))
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for p in list {
                    let resp = tile(ui, ctx, format!("effect:{}", p.id), p.name, true, |t| {
                        effect_image(p, t)
                    });
                    if resp.clicked() {
                        apply_effect(ctx, p, false);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Put on the timeline as an effect clip").clicked() {
                            apply_effect(ctx, p, true);
                            ui.close();
                        }
                        if selected_visual(ctx).is_some()
                            && ui.button("Add to the selected clip").clicked()
                        {
                            apply_effect(ctx, p, false);
                            ui.close();
                        }
                    });
                }
            });
        });
}

fn adjustment_panel(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    if ctx.state.sub_menu != 0 {
        hint(ui, "Your own saved presets arrive in a later version.");
        return;
    }
    hint(
        ui,
        "An adjustment layer changes the colours (and effects) of everything below it for as long as it lasts.",
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add(theme::primary_button("＋ Adjustment layer"))
            .on_hover_text(
                "Add an empty adjustment layer at the playhead; change it on the Adjust tab",
            )
            .clicked()
        {
            add_layer(ctx, Adjust::default(), Vec::new(), inspector::Tab::Adjust);
        }
    });
}
