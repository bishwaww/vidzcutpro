//! The Shape tab of the inspector: size, fill (plain or gradient), outline (place, dashes, ends, corners), trim paths,
//! the corners of polygons and stars, and the points of pen paths.

use super::{Ctx, change, key, row, section_title, slider};
use crate::editor::pen::PenSession;
use crate::theme;
use eframe::egui::{self, RichText, vec2};
use std::sync::Arc;
use vz_core::{
    BLACK, Clip, ClipId, Color, Content, Gradient, GradientKind, LineCap, LineJoin, ShapeContent,
    ShapeKind, StrokePlace, TextStroke, WHITE,
};

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
    apply: impl Fn(&mut ShapeContent, Color) + Send + Sync + 'static,
) {
    let apply = Arc::new(move |c: &mut Clip, col: Color| with_shape(c, |s| apply(s, col)));
    super::super::widgets::pick_color(ui, ctx, id, property, color, apply);
}

/// A slider for a number of the shape that is not a plain clip property (so it is written through `with_shape`).
#[allow(clippy::too_many_arguments)]
fn num(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    id: ClipId,
    merge: &'static str,
    label: &str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    apply: impl FnOnce(&mut ShapeContent, f32),
) {
    row(ui, label, |ui| {
        let mut v = value;
        let resp = ui.add(
            egui::Slider::new(&mut v, range)
                .suffix(suffix)
                .max_decimals(2)
                .clamping(egui::SliderClamping::Always),
        );
        if resp.changed() {
            let _ = ctx.doc.update_clip(id, "Shape", Some(key(id, merge)), |c| {
                with_shape(c, |s| apply(s, v))
            });
        }
        if resp.drag_stopped() || resp.lost_focus() {
            ctx.doc.seal_history();
        }
    });
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
            if shape.kind != ShapeKind::Path {
                row(ui, "Kind", |ui| {
                    egui::ComboBox::from_id_salt("shape_kind")
                        .selected_text(shape.kind.label())
                        .show_ui(ui, |ui| {
                            // a pen path is made with the pen tool, not chosen from the list
                            for kind in ShapeKind::ALL.into_iter().filter(|k| *k != ShapeKind::Path)
                            {
                                if ui
                                    .selectable_label(shape.kind == kind, kind.label())
                                    .clicked()
                                {
                                    change(ctx, id, "Shape kind", |c| {
                                        with_shape(c, |s| s.kind = kind)
                                    });
                                }
                            }
                        });
                });
            }
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
            match shape.kind {
                ShapeKind::Rectangle => slider(
                    ui,
                    ctx,
                    id,
                    "shape.r",
                    "Corners",
                    shape.corner_radius,
                    0.0..=300.0,
                    " px",
                    0,
                    |c, v| with_shape(c, |s| s.corner_radius = v),
                ),
                ShapeKind::Polygon => num(
                    ui,
                    ctx,
                    id,
                    "shape.sides",
                    "Sides",
                    shape.sides as f32,
                    3.0..=24.0,
                    "",
                    |s, v| {
                        s.sides = v.round() as u32;
                    },
                ),
                ShapeKind::Star => {
                    num(
                        ui,
                        ctx,
                        id,
                        "shape.points",
                        "Points",
                        shape.points as f32,
                        3.0..=24.0,
                        "",
                        |s, v| {
                            s.points = v.round() as u32;
                        },
                    );
                    num(
                        ui,
                        ctx,
                        id,
                        "shape.inner",
                        "Depth",
                        shape.inner * 100.0,
                        10.0..=90.0,
                        " %",
                        |s, v| {
                            s.inner = v / 100.0;
                        },
                    );
                }
                _ => {}
            }
        });

    if shape.kind == ShapeKind::Path {
        path_section(ui, ctx, clip, shape);
    }

    ui.add_space(8.0);
    fill_section(ui, ctx, id, shape);
    ui.add_space(6.0);
    outline_section(ui, ctx, id, shape);
    if shape.stroke.is_some() {
        ui.add_space(6.0);
        trim_section(ui, ctx, id, shape);
    }
    if ui.input(|i| i.pointer.any_released()) {
        ctx.doc.seal_history();
    }
}

// ---- pen paths ---------------------------------------------------------------------------------------------------------

fn path_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip, shape: &ShapeContent) {
    let id = clip.id;
    let editing = ctx.state.pen.as_ref().is_some_and(|p| p.clip == Some(id));
    ui.horizontal(|ui| {
        if ui
            .selectable_label(editing, "Edit points")
            .on_hover_text("Drag points and handles on the picture")
            .clicked()
        {
            if editing {
                ctx.state.pen = None;
            } else if let Some(p) =
                crate::editor::geometry::placed(&ctx.doc.project, clip, ctx.state.playhead)
            {
                ctx.state.pen = PenSession::edit(id, shape, &p);
            } else {
                ctx.state
                    .say("Move the playhead onto this path to edit its points.");
            }
        }
        let mut closed = shape.path.as_ref().is_some_and(|p| p.closed);
        if ui
            .checkbox(&mut closed, "Closed")
            .on_hover_text("Join the last point back to the first so the path can be filled")
            .changed()
        {
            ctx.state.pen = None;
            change(ctx, id, "Close path", move |c| {
                with_shape(c, |s| {
                    if let Some(p) = &mut s.path {
                        p.closed = closed;
                    }
                    if closed && s.fill.is_none() && s.gradient.is_none() {
                        s.fill = Some([252, 198, 16, 255]);
                    }
                });
            });
        }
    });
    let points = shape.path.as_ref().map_or(0, |p| p.nodes.len());
    ui.label(
        RichText::new(format!("{points} points"))
            .color(theme::MUTED)
            .small(),
    );
}

// ---- fill --------------------------------------------------------------------------------------------------------------

fn fill_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, shape: &ShapeContent) {
    let open_path = shape.kind == ShapeKind::Path && shape.path.as_ref().is_some_and(|p| !p.closed);
    if open_path {
        return; // an open line has no inside
    }
    let mut fill_on = shape.fill.is_some() || shape.gradient.is_some();
    if ui
        .checkbox(&mut fill_on, RichText::new("Fill").strong())
        .changed()
    {
        change(ctx, id, "Shape fill", |c| {
            with_shape(c, |s| {
                s.fill = fill_on.then_some([252, 198, 16, 255]);
                if !fill_on {
                    s.gradient = None;
                }
            });
        });
    }
    if !fill_on {
        return;
    }
    let mut gradient_on = shape.gradient.is_some();
    ui.horizontal(|ui| {
        if ui.selectable_label(!gradient_on, "Colour").clicked() && gradient_on {
            gradient_on = false;
            change(ctx, id, "Plain fill", |c| {
                with_shape(c, |s| s.gradient = None)
            });
        }
        if ui.selectable_label(gradient_on, "Gradient").clicked() && !gradient_on {
            let colour = shape.fill.unwrap_or([252, 198, 16, 255]);
            change(ctx, id, "Gradient fill", move |c| {
                with_shape(c, |s| {
                    s.gradient = Some(Gradient::two(
                        colour,
                        [colour[2], colour[0], colour[1], 255],
                        90.0,
                    ));
                });
            });
        }
    });
    match &shape.gradient {
        None => {
            if let Some(fill) = shape.fill {
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    color_button(ui, ctx, id, "shape.fill", fill, |s, c| s.fill = Some(c));
                });
            }
        }
        Some(g) => gradient_controls(ui, ctx, id, g),
    }
}

fn gradient_controls(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, g: &Gradient) {
    ui.horizontal(|ui| {
        for (label, kind) in [
            ("Linear", GradientKind::Linear),
            ("Radial", GradientKind::Radial),
        ] {
            if ui.selectable_label(g.kind == kind, label).clicked() {
                change(ctx, id, "Gradient kind", move |c| {
                    with_shape(c, |s| {
                        if let Some(g) = &mut s.gradient {
                            g.kind = kind;
                        }
                    });
                });
            }
        }
        let first = g.stops.first().map_or(WHITE, |s| s.1);
        let last = g.stops.last().map_or(WHITE, |s| s.1);
        color_button(ui, ctx, id, "shape.grad1", first, |s, col| {
            if let Some(stop) = s.gradient.as_mut().and_then(|g| g.stops.first_mut()) {
                stop.1 = col;
            }
        });
        color_button(ui, ctx, id, "shape.grad2", last, |s, col| {
            if let Some(stop) = s.gradient.as_mut().and_then(|g| g.stops.last_mut()) {
                stop.1 = col;
            }
        });
    });
    if g.kind == GradientKind::Linear {
        egui::Grid::new("shape_grad")
            .num_columns(2)
            .spacing(vec2(14.0, 6.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                num(
                    ui,
                    ctx,
                    id,
                    "shape.gangle",
                    "Angle",
                    g.angle,
                    0.0..=360.0,
                    "°",
                    |s, v| {
                        if let Some(g) = &mut s.gradient {
                            g.angle = v;
                        }
                    },
                );
            });
    }
}

// ---- outline -----------------------------------------------------------------------------------------------------------

fn outline_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, shape: &ShapeContent) {
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
    let Some(stroke) = shape.stroke else {
        return;
    };
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
            let open_path =
                shape.kind == ShapeKind::Path && shape.path.as_ref().is_some_and(|p| !p.closed);
            if !open_path {
                row(ui, "Placed", |ui| {
                    for (label, place) in [
                        ("Inside", StrokePlace::Inside),
                        ("Centre", StrokePlace::Centre),
                        ("Outside", StrokePlace::Outside),
                    ] {
                        if ui
                            .selectable_label(shape.stroke_place == place, label)
                            .clicked()
                        {
                            change(ctx, id, "Outline place", move |c| {
                                with_shape(c, |s| s.stroke_place = place)
                            });
                        }
                    }
                });
            }
            row(ui, "Line ends", |ui| {
                for (label, cap) in [
                    ("Round", LineCap::Round),
                    ("Flat", LineCap::Butt),
                    ("Square", LineCap::Square),
                ] {
                    if ui.selectable_label(shape.cap == cap, label).clicked() {
                        change(ctx, id, "Line ends", move |c| {
                            with_shape(c, |s| s.cap = cap)
                        });
                    }
                }
            });
            row(ui, "Corners", |ui| {
                for (label, join) in [
                    ("Round", LineJoin::Round),
                    ("Sharp", LineJoin::Miter),
                    ("Bevel", LineJoin::Bevel),
                ] {
                    if ui.selectable_label(shape.join == join, label).clicked() {
                        change(ctx, id, "Line corners", move |c| {
                            with_shape(c, |s| s.join = join)
                        });
                    }
                }
            });
            let dashed = shape.dash.is_some();
            row(ui, "Dashes", |ui| {
                let mut on = dashed;
                if ui.checkbox(&mut on, "").changed() {
                    change(ctx, id, "Dashes", move |c| {
                        with_shape(c, |s| {
                            s.dash = on.then_some((
                                stroke.width.max(2.0) * 2.0,
                                stroke.width.max(2.0) * 2.0,
                            ))
                        });
                    });
                }
            });
            if let Some((dash, gap)) = shape.dash {
                num(
                    ui,
                    ctx,
                    id,
                    "shape.dash",
                    "Dash",
                    dash,
                    1.0..=200.0,
                    " px",
                    move |s, v| {
                        s.dash = Some((v, gap));
                    },
                );
                num(
                    ui,
                    ctx,
                    id,
                    "shape.gap",
                    "Gap",
                    gap,
                    0.0..=200.0,
                    " px",
                    move |s, v| {
                        s.dash = Some((dash, v));
                    },
                );
            }
        });
}

// ---- trim paths --------------------------------------------------------------------------------------------------------

fn trim_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, shape: &ShapeContent) {
    section_title(ui, "Trim the outline");
    ui.label(
        RichText::new("Draw only part of the outline: from Start to End of its length. Move Offset to slide it round.")
            .color(theme::MUTED)
            .small(),
    );
    egui::Grid::new("shape_trim")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            num(
                ui,
                ctx,
                id,
                "shape.ts",
                "Start",
                shape.trim_start * 100.0,
                0.0..=100.0,
                " %",
                |s, v| {
                    s.trim_start = v / 100.0;
                },
            );
            num(
                ui,
                ctx,
                id,
                "shape.te",
                "End",
                shape.trim_end * 100.0,
                0.0..=100.0,
                " %",
                |s, v| {
                    s.trim_end = v / 100.0;
                },
            );
            num(
                ui,
                ctx,
                id,
                "shape.to",
                "Offset",
                shape.trim_offset * 360.0,
                -360.0..=360.0,
                "°",
                |s, v| {
                    s.trim_offset = v / 360.0;
                },
            );
        });
}
