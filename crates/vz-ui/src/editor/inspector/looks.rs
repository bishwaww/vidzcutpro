//! Masks, background removal (chroma key) and the list of effects on a clip.

use super::{Ctx, change, key, row, section_title};
use crate::theme;
use eframe::egui::{self, Color32, RichText, vec2};
use vz_core::{ChromaKey, Clip, ClipId, EffectRef, MaskDef, MaskShape};
use vz_presets::effects::{Group, effects, find};

/// A slider for a number somewhere in a clip, changed through `apply`. Changes of one drag are one undo step.
#[allow(clippy::too_many_arguments)]
fn num(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    id: ClipId,
    merge: &str,
    label: &str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    apply: impl FnOnce(&mut Clip, f32),
) {
    row(ui, label, |ui| {
        let mut v = value;
        let resp = ui.add(
            egui::Slider::new(&mut v, range)
                .suffix(suffix)
                .max_decimals(1)
                .clamping(egui::SliderClamping::Never),
        );
        if resp.changed() {
            let _ = ctx
                .doc
                .update_clip(id, "Change", Some(key(id, merge)), |c| apply(c, v));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            ctx.doc.seal_history();
        }
    });
}

// ---- masks ------------------------------------------------------------------------------------------------------------

pub fn mask_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Mask");
        ui.menu_button(RichText::new("+ Add mask").small(), |ui| {
            for shape in MaskShape::ALL {
                if ui.button(shape.label()).clicked() {
                    change(ctx, id, "Add mask", move |c| {
                        c.masks.push(MaskDef::new(shape))
                    });
                    ui.close();
                }
            }
        });
    });
    if clip.masks.is_empty() {
        ui.label(
            RichText::new("Only the part inside a mask is shown.")
                .color(theme::MUTED)
                .small(),
        );
        return;
    }
    for (i, mask) in clip.masks.iter().enumerate() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new(format!("{} mask", mask.shape.label())).strong())
            .id_salt(("mask", id, i))
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(("mask_grid", id, i))
                    .num_columns(2)
                    .spacing(vec2(14.0, 6.0))
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        row(ui, "Shape", |ui| {
                            egui::ComboBox::from_id_salt(("mask_shape", id, i))
                                .selected_text(mask.shape.label())
                                .show_ui(ui, |ui| {
                                    for s in MaskShape::ALL {
                                        if ui.selectable_label(mask.shape == s, s.label()).clicked()
                                        {
                                            change(ctx, id, "Mask shape", move |c| {
                                                c.masks[i].shape = s
                                            });
                                        }
                                    }
                                });
                        });
                        let m = |name: &str| format!("mask{i}.{name}");
                        num(
                            ui,
                            ctx,
                            id,
                            &m("x"),
                            "Position X",
                            mask.x * 100.0,
                            -100.0..=100.0,
                            " %",
                            move |c, v| c.masks[i].x = v / 100.0,
                        );
                        num(
                            ui,
                            ctx,
                            id,
                            &m("y"),
                            "Position Y",
                            mask.y * 100.0,
                            -100.0..=100.0,
                            " %",
                            move |c, v| c.masks[i].y = v / 100.0,
                        );
                        num(
                            ui,
                            ctx,
                            id,
                            &m("w"),
                            "Width",
                            mask.width * 100.0,
                            1.0..=300.0,
                            " %",
                            move |c, v| c.masks[i].width = v / 100.0,
                        );
                        num(
                            ui,
                            ctx,
                            id,
                            &m("h"),
                            "Height",
                            mask.height * 100.0,
                            1.0..=300.0,
                            " %",
                            move |c, v| c.masks[i].height = v / 100.0,
                        );
                        num(
                            ui,
                            ctx,
                            id,
                            &m("r"),
                            "Rotation",
                            mask.rotation,
                            -180.0..=180.0,
                            "°",
                            move |c, v| c.masks[i].rotation = v,
                        );
                        num(
                            ui,
                            ctx,
                            id,
                            &m("f"),
                            "Feather",
                            mask.feather * 100.0,
                            0.0..=100.0,
                            " %",
                            move |c, v| c.masks[i].feather = v / 100.0,
                        );
                        if mask.shape == MaskShape::Rectangle {
                            num(
                                ui,
                                ctx,
                                id,
                                &m("c"),
                                "Corners",
                                mask.corner * 100.0,
                                0.0..=100.0,
                                " %",
                                move |c, v| c.masks[i].corner = v / 100.0,
                            );
                        }
                    });
                ui.horizontal(|ui| {
                    let mut invert = mask.invert;
                    if ui.checkbox(&mut invert, "Invert").changed() {
                        change(ctx, id, "Invert mask", move |c| c.masks[i].invert = invert);
                    }
                    if ui.small_button("Remove").clicked() {
                        change(ctx, id, "Remove mask", move |c| {
                            c.masks.remove(i);
                        });
                    }
                });
            });
    }
}

// ---- chroma key -------------------------------------------------------------------------------------------------------

pub fn chroma_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let mut on = clip.chroma.is_some();
    if ui
        .checkbox(
            &mut on,
            RichText::new("Remove background colour (chroma key)").strong(),
        )
        .changed()
    {
        change(ctx, id, "Chroma key", move |c| {
            c.chroma = on.then(ChromaKey::green)
        });
    }
    let Some(key_settings) = &clip.chroma else {
        return;
    };
    egui::Grid::new(("chroma_grid", id))
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Colour", |ui| {
                let k = key_settings.color;
                let mut c32 = Color32::from_rgb(k[0], k[1], k[2]);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    let _ =
                        ctx.doc
                            .update_clip(id, "Key colour", Some(key(id, "chroma.color")), |c| {
                                if let Some(ck) = &mut c.chroma {
                                    ck.color = [c32.r(), c32.g(), c32.b(), 255];
                                }
                            });
                }
                for (name, rgb) in [("Green", [0, 255, 0, 255]), ("Blue", [0, 0, 255, 255])] {
                    if ui.small_button(name).clicked() {
                        change(ctx, id, "Key colour", move |c| {
                            if let Some(ck) = &mut c.chroma {
                                ck.color = rgb;
                            }
                        });
                    }
                }
            });
            num(
                ui,
                ctx,
                id,
                "chroma.intensity",
                "Intensity",
                key_settings.intensity,
                0.0..=100.0,
                "",
                |c, v| {
                    if let Some(ck) = &mut c.chroma {
                        ck.intensity = v;
                    }
                },
            );
            num(
                ui,
                ctx,
                id,
                "chroma.shadow",
                "Shadow",
                key_settings.shadow,
                0.0..=100.0,
                "",
                |c, v| {
                    if let Some(ck) = &mut c.chroma {
                        ck.shadow = v;
                    }
                },
            );
            num(
                ui,
                ctx,
                id,
                "chroma.feather",
                "Edge feather",
                key_settings.feather,
                0.0..=100.0,
                "",
                |c, v| {
                    if let Some(ck) = &mut c.chroma {
                        ck.feather = v;
                    }
                },
            );
            num(
                ui,
                ctx,
                id,
                "chroma.spill",
                "Remove spill",
                key_settings.spill,
                0.0..=100.0,
                "",
                |c, v| {
                    if let Some(ck) = &mut c.chroma {
                        ck.spill = v;
                    }
                },
            );
        });
    ui.label(RichText::new("Pick the colour of the screen behind your subject, then raise Intensity until it disappears.").color(theme::MUTED).small());
}

// ---- effects on a clip ------------------------------------------------------------------------------------------------

pub fn effects_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Effects");
        ui.menu_button(RichText::new("+ Add effect").small(), |ui| {
            for group in Group::ALL {
                ui.menu_button(group.label(), |ui| {
                    for preset in effects().iter().filter(|e| e.group == group) {
                        if ui.button(preset.name).clicked() {
                            let fx = EffectRef {
                                preset: preset.id.to_owned(),
                                params: preset.defaults(),
                                enabled: true,
                            };
                            change(ctx, id, "Add effect", move |c| c.effects.push(fx));
                            ui.close();
                        }
                    }
                });
            }
        });
    });
    if clip.effects.is_empty() {
        let hint = if clip.is_adjustment() {
            "Everything below this layer gets these effects."
        } else {
            "Effects change this clip. To change everything, use the Effects tab at the top left."
        };
        ui.label(RichText::new(hint).color(theme::MUTED).small());
        return;
    }
    let count = clip.effects.len();
    for (i, fx) in clip.effects.iter().enumerate() {
        let Some(preset) = find(&fx.preset) else {
            continue;
        };
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 24, 24))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut enabled = fx.enabled;
                    if ui
                        .checkbox(&mut enabled, RichText::new(preset.name).strong())
                        .changed()
                    {
                        change(ctx, id, "Effect on/off", move |c| {
                            c.effects[i].enabled = enabled
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Remove")
                            .on_hover_text("Remove this effect")
                            .clicked()
                        {
                            change(ctx, id, "Remove effect", move |c| {
                                c.effects.remove(i);
                            });
                        }
                        if ui
                            .add_enabled(i + 1 < count, egui::Button::new("Down").small())
                            .clicked()
                        {
                            change(ctx, id, "Move effect", move |c| c.effects.swap(i, i + 1));
                        }
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Up").small())
                            .clicked()
                        {
                            change(ctx, id, "Move effect", move |c| c.effects.swap(i, i - 1));
                        }
                    });
                });
                egui::Grid::new(("fx_grid", id, i))
                    .num_columns(2)
                    .spacing(vec2(14.0, 6.0))
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        for (k, param) in preset.params.iter().enumerate() {
                            let value = fx.params.get(k).copied().unwrap_or(param.default);
                            num(
                                ui,
                                ctx,
                                id,
                                &format!("fx{i}.{k}"),
                                param.name,
                                value,
                                param.min..=param.max,
                                "",
                                move |c, v| {
                                    let params = &mut c.effects[i].params;
                                    if params.len() <= k {
                                        params.resize(k + 1, 0.0);
                                    }
                                    params[k] = v;
                                },
                            );
                        }
                    });
            });
    }
}
