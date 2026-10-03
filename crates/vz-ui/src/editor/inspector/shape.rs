//! The Shape tab of the inspector: size, fill and outline of a drawn shape.

use super::{Ctx, change, key, row, section_title, slider};
use eframe::egui::{self, Color32, RichText, vec2};
use vz_core::{BLACK, Clip, ClipId, Color, Content, ShapeContent, ShapeKind, TextStroke};

fn with_shape(c: &mut Clip, f: impl FnOnce(&mut ShapeContent)) {
    if let Content::Shape(s) = &mut c.content {
        f(s);
    }
}

fn color_button(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    id: ClipId,
    property: &'static str,
    color: Color,
    apply: impl FnOnce(&mut ShapeContent, Color),
) {
    let mut c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let resp = ui.color_edit_button_srgba(&mut c);
    if resp.changed() {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        let _ = ctx
            .doc
            .update_clip(id, property, Some(key(id, property)), |clip| {
                with_shape(clip, |s| apply(s, [r, g, b, a]));
            });
    }
}

pub fn shape_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let Content::Shape(shape) = &clip.content else {
        return;
    };
    let id = clip.id;
    section_title(ui, "Shape");
    egui::Grid::new("shape_look")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Kind", |ui| {
                egui::ComboBox::from_id_salt("shape_kind")
                    .selected_text(shape.kind.label())
                    .show_ui(ui, |ui| {
                        for kind in ShapeKind::ALL {
                            if ui
                                .selectable_label(shape.kind == kind, kind.label())
                                .clicked()
                            {
                                change(ctx, id, "Shape kind", |c| with_shape(c, |s| s.kind = kind));
                            }
                        }
                    });
            });
            slider(
                ui,
                ctx,
                id,
                "shape.w",
                "Width",
                shape.width,
                4.0..=2000.0,
                " px",
                0,
                |c, v| {
                    with_shape(c, |s| s.width = v);
                },
            );
            slider(
                ui,
                ctx,
                id,
                "shape.h",
                "Height",
                shape.height,
                4.0..=2000.0,
                " px",
                0,
                |c, v| {
                    with_shape(c, |s| s.height = v);
                },
            );
            if shape.kind == ShapeKind::Rectangle {
                slider(
                    ui,
                    ctx,
                    id,
                    "shape.r",
                    "Corners",
                    shape.corner_radius,
                    0.0..=300.0,
                    " px",
                    0,
                    |c, v| {
                        with_shape(c, |s| s.corner_radius = v);
                    },
                );
            }
        });

    ui.add_space(8.0);
    let mut fill_on = shape.fill.is_some();
    if ui
        .checkbox(&mut fill_on, RichText::new("Fill").strong())
        .changed()
    {
        change(ctx, id, "Shape fill", |c| {
            with_shape(c, |s| s.fill = fill_on.then_some([252, 198, 16, 255]));
        });
    }
    if let Some(fill) = shape.fill {
        ui.horizontal(|ui| {
            ui.label("Colour");
            color_button(ui, ctx, id, "shape.fill", fill, |s, c| s.fill = Some(c));
        });
    }
    ui.add_space(6.0);
    let mut stroke_on = shape.stroke.is_some();
    if ui
        .checkbox(&mut stroke_on, RichText::new("Outline").strong())
        .changed()
    {
        change(ctx, id, "Shape outline", |c| {
            with_shape(c, |s| {
                s.stroke = stroke_on.then_some(TextStroke {
                    color: BLACK,
                    width: 8.0,
                });
            });
        });
    }
    if let Some(stroke) = shape.stroke {
        egui::Grid::new("shape_outline")
            .num_columns(2)
            .spacing(vec2(14.0, 8.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                row(ui, "Colour", |ui| {
                    color_button(ui, ctx, id, "shape.stroke", stroke.color, |s, c| {
                        if let Some(st) = &mut s.stroke {
                            st.color = c;
                        }
                    });
                });
                slider(
                    ui,
                    ctx,
                    id,
                    "shape.sw",
                    "Width",
                    stroke.width,
                    0.0..=60.0,
                    " px",
                    1,
                    |c, v| {
                        with_shape(c, |s| {
                            if let Some(st) = &mut s.stroke {
                                st.width = v;
                            }
                        });
                    },
                );
            });
    }
    if ui.input(|i| i.pointer.any_released()) {
        ctx.doc.seal_history();
    }
}
