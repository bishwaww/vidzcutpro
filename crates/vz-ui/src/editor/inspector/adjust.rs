//! The Adjust tab: colour correction and grading (basic, HSL, curves, colour wheels, looks).

use super::{Ctx, change, key, row, section_title, slider};
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, vec2};
use vz_core::{Adjust, Band, Clip, ClipId, Curve, LutRef, LutSource, Wheel};

/// Which part of the tab is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Part {
    #[default]
    Basic,
    Hsl,
    Curves,
    Wheels,
    Look,
}

impl Part {
    const ALL: [Self; 5] = [
        Self::Basic,
        Self::Hsl,
        Self::Curves,
        Self::Wheels,
        Self::Look,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Hsl => "HSL",
            Self::Curves => "Curves",
            Self::Wheels => "Wheels",
            Self::Look => "Look (LUT)",
        }
    }
}

pub fn adjust_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Adjust");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !clip.adjust.is_identity(),
                    egui::Button::new("↺ Reset all").small(),
                )
                .clicked()
            {
                change(ctx, id, "Reset colours", |c| c.adjust = Adjust::default());
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        for part in Part::ALL {
            if ui
                .selectable_label(ctx.state.adjust_part == part, part.label())
                .clicked()
            {
                ctx.state.adjust_part = part;
            }
        }
    });
    ui.add_space(6.0);
    match ctx.state.adjust_part {
        Part::Basic => basic(ui, ctx, clip),
        Part::Hsl => hsl(ui, ctx, clip),
        Part::Curves => curves(ui, ctx, clip),
        Part::Wheels => wheels(ui, ctx, clip),
        Part::Look => look(ui, ctx, clip),
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(4.0);
    if ui
        .button("Apply to all clips")
        .on_hover_text("Copy these colour settings to every picture clip on the timeline")
        .clicked()
    {
        let adjust = clip.adjust.clone();
        let ids: Vec<ClipId> = ctx
            .doc
            .project
            .timeline()
            .tracks
            .iter()
            .filter(|t| t.kind.is_visual())
            .flat_map(|t| t.clips.iter().filter(|c| !c.is_adjustment()).map(|c| c.id))
            .collect();
        match ctx
            .doc
            .update_clips(&ids, "Apply colours to all", |c| c.adjust = adjust.clone())
        {
            Ok(()) => ctx
                .state
                .say(format!("Colours copied to {} clips", ids.len())),
            Err(e) => ctx.state.say(format!("Could not do that: {e}")),
        }
    }
}

type Setter = fn(&mut Adjust, f32);

fn basic(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let a = &clip.adjust;
    // (name, value, from, to, setter)
    let rows: [(&str, f32, f32, f32, Setter); 17] = [
        ("Temperature", a.temperature, -100.0, 100.0, |a, v| {
            a.temperature = v
        }),
        ("Tint", a.tint, -100.0, 100.0, |a, v| a.tint = v),
        ("Saturation", a.saturation, -100.0, 100.0, |a, v| {
            a.saturation = v
        }),
        ("Vibrance", a.vibrance, -100.0, 100.0, |a, v| a.vibrance = v),
        ("Exposure", a.exposure, -100.0, 100.0, |a, v| a.exposure = v),
        ("Brightness", a.brightness, -100.0, 100.0, |a, v| {
            a.brightness = v
        }),
        ("Contrast", a.contrast, -100.0, 100.0, |a, v| a.contrast = v),
        ("Highlights", a.highlights, -100.0, 100.0, |a, v| {
            a.highlights = v
        }),
        ("Shadows", a.shadows, -100.0, 100.0, |a, v| a.shadows = v),
        ("Whites", a.whites, -100.0, 100.0, |a, v| a.whites = v),
        ("Blacks", a.blacks, -100.0, 100.0, |a, v| a.blacks = v),
        ("Brilliance", a.brilliance, -100.0, 100.0, |a, v| {
            a.brilliance = v
        }),
        ("Sharpen", a.sharpen, 0.0, 100.0, |a, v| a.sharpen = v),
        ("Clarity", a.clarity, -100.0, 100.0, |a, v| a.clarity = v),
        ("Fade", a.fade, 0.0, 100.0, |a, v| a.fade = v),
        ("Vignette", a.vignette, -100.0, 100.0, |a, v| a.vignette = v),
        ("Grain", a.grain, 0.0, 100.0, |a, v| a.grain = v),
    ];
    egui::Grid::new("adjust_basic")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(86.0)
        .show(ui, |ui| {
            for (name, value, lo, hi, set) in rows {
                slider(
                    ui,
                    ctx,
                    id,
                    name,
                    name,
                    value,
                    lo..=hi,
                    "",
                    0,
                    move |c, v| set(&mut c.adjust, v),
                );
            }
        });
}

fn band_color(band: Band) -> Color32 {
    let rgb = vz_render::color::hsl_to_rgb([band.centre(), 0.85, 0.55]);
    Color32::from_rgb(
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
    )
}

fn hsl(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        for (i, band) in Band::ALL.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            let on = ctx.state.hsl_band == i;
            ui.painter()
                .circle_filled(rect.center(), 9.0, band_color(*band));
            if on {
                ui.painter()
                    .circle_stroke(rect.center(), 12.0, Stroke::new(2.0, Color32::WHITE));
            }
            if resp.on_hover_text(band.label()).clicked() {
                ctx.state.hsl_band = i;
            }
        }
    });
    ui.add_space(6.0);
    let i = ctx.state.hsl_band.min(7);
    let band = clip.adjust.hsl[i];
    ui.label(RichText::new(Band::ALL[i].label()).strong());
    egui::Grid::new("adjust_hsl")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(86.0)
        .show(ui, |ui| {
            slider(
                ui,
                ctx,
                id,
                "hsl.hue",
                "Hue",
                band.hue,
                -100.0..=100.0,
                "",
                0,
                move |c, v| c.adjust.hsl[i].hue = v,
            );
            slider(
                ui,
                ctx,
                id,
                "hsl.sat",
                "Saturation",
                band.saturation,
                -100.0..=100.0,
                "",
                0,
                move |c, v| c.adjust.hsl[i].saturation = v,
            );
            slider(
                ui,
                ctx,
                id,
                "hsl.light",
                "Lightness",
                band.lightness,
                -100.0..=100.0,
                "",
                0,
                move |c, v| c.adjust.hsl[i].lightness = v,
            );
        });
    if ui.small_button("↺ Reset this colour").clicked() {
        change(ctx, id, "Reset colour band", move |c| {
            c.adjust.hsl[i] = vz_core::HslBand::default()
        });
    }
}

fn curve_of(a: &mut Adjust, which: usize) -> &mut Curve {
    match which {
        0 => &mut a.curves.master,
        1 => &mut a.curves.red,
        2 => &mut a.curves.green,
        _ => &mut a.curves.blue,
    }
}

fn curves(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let which = ctx.state.curve_channel.min(3);
    ui.horizontal(|ui| {
        for (i, (label, color)) in [
            ("RGB", Color32::WHITE),
            ("Red", Color32::from_rgb(240, 80, 80)),
            ("Green", Color32::from_rgb(90, 220, 90)),
            ("Blue", Color32::from_rgb(90, 140, 255)),
        ]
        .into_iter()
        .enumerate()
        {
            if ui
                .selectable_label(which == i, RichText::new(label).color(color))
                .clicked()
            {
                ctx.state.curve_channel = i;
            }
        }
    });
    ui.add_space(4.0);
    let size = ui.available_width().min(260.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(16, 16, 16));
    for k in 1..4 {
        let f = k as f32 / 4.0;
        painter.vline(
            rect.left() + rect.width() * f,
            rect.y_range(),
            Stroke::new(1.0, Color32::from_rgb(34, 34, 34)),
        );
        painter.hline(
            rect.x_range(),
            rect.top() + rect.height() * f,
            Stroke::new(1.0, Color32::from_rgb(34, 34, 34)),
        );
    }
    painter.line_segment(
        [rect.left_bottom(), rect.right_top()],
        Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
    );
    let mut adj = clip.adjust.clone();
    let curve = curve_of(&mut adj, which).clone();
    let to_screen = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let from_screen = |p: Pos2| {
        (
            ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0),
        )
    };
    let line_color = [
        Color32::WHITE,
        Color32::from_rgb(240, 80, 80),
        Color32::from_rgb(90, 220, 90),
        Color32::from_rgb(90, 140, 255),
    ][which];
    let samples: Vec<Pos2> = (0..=64)
        .map(|i| {
            let x = i as f32 / 64.0;
            to_screen(x, curve.value(x))
        })
        .collect();
    painter.add(egui::Shape::line(samples, Stroke::new(2.0, line_color)));
    let handles: Vec<Pos2> = curve
        .points
        .iter()
        .map(|(x, y)| to_screen(*x, *y))
        .collect();
    for h in &handles {
        painter.circle_filled(*h, 5.0, Color32::WHITE);
        painter.circle_stroke(*h, 5.0, Stroke::new(1.5, theme::ACCENT));
    }
    let nearest = |p: Pos2| {
        handles
            .iter()
            .enumerate()
            .map(|(i, h)| (i, h.distance(p)))
            .filter(|(_, d)| *d < 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let drag_id = ui.id().with(("curve_point", id));
    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        let hit = nearest(origin);
        // dragging empty space starts a new point there
        let index = match hit {
            Some(i) => i,
            None => {
                let (x, y) = from_screen(origin);
                let mut points = curve.points.clone();
                points.push((x, y));
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                let index = points.iter().position(|p| p.0 == x).unwrap_or(0);
                change(ctx, id, "Add curve point", move |c| {
                    curve_of(&mut c.adjust, which).points = points
                });
                index
            }
        };
        ui.memory_mut(|m| m.data.insert_temp(drag_id, index));
    }
    if resp.dragged()
        && let Some(index) = ui.memory(|m| m.data.get_temp::<usize>(drag_id))
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let (x, y) = from_screen(pos);
        let _ = ctx
            .doc
            .update_clip(id, "Curve", Some(key(id, "curve")), |c| {
                let pts = &mut curve_of(&mut c.adjust, which).points;
                if let Some(p) = pts.get_mut(index) {
                    *p = (x, y);
                }
            });
    }
    if resp.drag_stopped() {
        ctx.doc.seal_history();
        // keep the points in order of x once the drag is over
        change(ctx, id, "Curve", move |c| {
            curve_of(&mut c.adjust, which)
                .points
                .sort_by(|a, b| a.0.total_cmp(&b.0))
        });
    }
    if resp.secondary_clicked()
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && let Some(i) = nearest(pos)
    {
        change(ctx, id, "Remove curve point", move |c| {
            curve_of(&mut c.adjust, which).points.remove(i);
        });
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new("Drag on the graph to add a point. Right-click a point to remove it.")
            .color(theme::MUTED)
            .small(),
    );
    if ui.small_button("↺ Reset this curve").clicked() {
        change(ctx, id, "Reset curve", move |c| {
            curve_of(&mut c.adjust, which).points.clear()
        });
    }
}

fn wheel_of(a: &mut Adjust, which: usize) -> &mut Wheel {
    match which {
        0 => &mut a.wheels.lift,
        1 => &mut a.wheels.gamma,
        2 => &mut a.wheels.gain,
        _ => &mut a.wheels.offset,
    }
}

/// A colour disc: the direction from the middle is the colour (hue), the distance is how strong it is.
fn disc(ui: &mut egui::Ui, size: f32, wheel: &Wheel) -> (egui::Response, Rect) {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let (c, r) = (rect.center(), size / 2.0 - 2.0);
    // a ring of coloured wedges, fading to grey in the middle
    let steps = 48;
    for i in 0..steps {
        let (a0, a1) = (
            i as f32 / steps as f32 * std::f32::consts::TAU,
            (i + 1) as f32 / steps as f32 * std::f32::consts::TAU,
        );
        let rgb = vz_render::color::hsl_to_rgb([i as f32 / steps as f32 * 360.0, 0.8, 0.5]);
        let col = Color32::from_rgb(
            (rgb[0] * 255.0) as u8,
            (rgb[1] * 255.0) as u8,
            (rgb[2] * 255.0) as u8,
        );
        let mut mesh = egui::Mesh::default();
        let centre = Color32::from_rgb(70, 70, 70);
        mesh.colored_vertex(c, centre);
        mesh.colored_vertex(c + vec2(a0.cos(), a0.sin()) * r, col);
        mesh.colored_vertex(c + vec2(a1.cos(), a1.sin()) * r, col);
        mesh.add_triangle(0, 1, 2);
        painter.add(egui::Shape::mesh(mesh));
    }
    painter.circle_stroke(c, r, Stroke::new(1.0, theme::BORDER));
    // the handle
    let tint = [wheel.red, wheel.green, wheel.blue];
    let mean = (tint[0] + tint[1] + tint[2]) / 3.0;
    let (dr, dg, db) = (tint[0] - mean, tint[1] - mean, tint[2] - mean);
    // the hue and strength that make this tint
    let strength = ((dr * dr + dg * dg + db * db) / 1.5).sqrt().min(1.0);
    let max = dr.max(dg).max(db);
    let min = dr.min(dg).min(db);
    let hue = if (max - min).abs() < 1e-5 {
        0.0
    } else {
        vz_render::color::rgb_to_hsl([
            (dr - min) / (max - min),
            (dg - min) / (max - min),
            (db - min) / (max - min),
        ])[0]
    };
    let ang = hue.to_radians();
    let pos = c + vec2(ang.cos(), ang.sin()) * r * strength;
    painter.circle_filled(pos, 5.0, Color32::WHITE);
    painter.circle_stroke(pos, 5.0, Stroke::new(1.5, Color32::BLACK));
    (resp, rect)
}

fn wheels(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let names = [
        "Shadows (lift)",
        "Midtones (gamma)",
        "Highlights (gain)",
        "Everything (offset)",
    ];
    ui.horizontal_wrapped(|ui| {
        for (which, name) in names.iter().enumerate() {
            ui.vertical(|ui| {
                ui.label(RichText::new(*name).small().color(theme::MUTED));
                let mut adj = clip.adjust.clone();
                let wheel = *wheel_of(&mut adj, which);
                let (resp, rect) = disc(ui, 104.0, &wheel);
                if (resp.dragged() || resp.clicked())
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let (c, r) = (rect.center(), rect.width() / 2.0 - 2.0);
                    let d = p - c;
                    let strength = (d.length() / r).min(1.0);
                    let hue = d.y.atan2(d.x).to_degrees().rem_euclid(360.0);
                    let rgb = vz_render::color::hsl_to_rgb([hue, 1.0, 0.5]);
                    let mean = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
                    let (red, green, blue) = (
                        (rgb[0] - mean) * strength * 1.5,
                        (rgb[1] - mean) * strength * 1.5,
                        (rgb[2] - mean) * strength * 1.5,
                    );
                    let _ =
                        ctx.doc
                            .update_clip(id, "Colour wheel", Some(key(id, names[which])), |c| {
                                let w = wheel_of(&mut c.adjust, which);
                                (w.red, w.green, w.blue) = (red, green, blue);
                            });
                }
                if resp.drag_stopped() {
                    ctx.doc.seal_history();
                }
                if resp.double_clicked() {
                    change(ctx, id, "Reset wheel", move |c| {
                        let w = wheel_of(&mut c.adjust, which);
                        (w.red, w.green, w.blue) = (0.0, 0.0, 0.0);
                    });
                }
                let mut luma = wheel.luma * 100.0;
                let r = ui.add(egui::Slider::new(&mut luma, -100.0..=100.0).show_value(false));
                if r.changed() {
                    let v = luma / 100.0;
                    let _ = ctx.doc.update_clip(
                        id,
                        "Wheel brightness",
                        Some(key(id, names[which])),
                        |c| wheel_of(&mut c.adjust, which).luma = v,
                    );
                }
                if r.drag_stopped() {
                    ctx.doc.seal_history();
                }
            });
        }
    });
    ui.label(RichText::new("Drag in a wheel to tint, the slider under it changes brightness. Double-click a wheel to reset it.").color(theme::MUTED).small());
}

fn look(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let current = clip.adjust.lut.clone();
    let name = match current.as_ref().map(|l| &l.source) {
        None => "None".to_string(),
        Some(LutSource::Builtin(n)) => n.replace('_', " "),
        Some(LutSource::File(p)) => p
            .file_name()
            .map_or("file".into(), |n| n.to_string_lossy().into_owned()),
    };
    row_grid(ui, |ui| {
        row(ui, "Look", |ui| {
            egui::ComboBox::from_id_salt("look_lut").selected_text(name).width(170.0).show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "None").clicked() {
                    change(ctx, id, "Remove look", |c| c.adjust.lut = None);
                }
                for look in vz_render::lut::BUILTIN_LOOKS {
                    let on = matches!(current.as_ref().map(|l| &l.source), Some(LutSource::Builtin(n)) if n == look);
                    if ui.selectable_label(on, look.replace('_', " ")).clicked() {
                        let strength = current.as_ref().map_or(1.0, |l| l.strength);
                        change(ctx, id, "Look", move |c| {
                            c.adjust.lut = Some(LutRef { source: LutSource::Builtin(look.to_owned()), strength });
                        });
                    }
                }
            });
        });
        if let Some(l) = &current {
            row(ui, "Strength", |ui| {
                let mut v = l.strength * 100.0;
                let resp = ui.add(
                    egui::Slider::new(&mut v, 0.0..=100.0)
                        .suffix(" %")
                        .max_decimals(0),
                );
                if resp.changed() {
                    let _ = ctx
                        .doc
                        .update_clip(id, "Look strength", Some(key(id, "lut")), |c| {
                            if let Some(l) = &mut c.adjust.lut {
                                l.strength = v / 100.0;
                            }
                        });
                }
                if resp.drag_stopped() || resp.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
        }
    });
    ui.add_space(6.0);
    if ui.button("Load a .cube file…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("LUT", &["cube"])
            .pick_file()
    {
        change(ctx, id, "Load look", move |c| {
            c.adjust.lut = Some(LutRef {
                source: LutSource::File(path),
                strength: 1.0,
            });
        });
    }
    ui.add_space(4.0);
    ui.label(RichText::new("A look changes the colours of the whole picture, like a film or camera style. The ones here are made by vidzcut; you can also load .cube files.").color(theme::MUTED).small());
}

fn row_grid(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new("adjust_look")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, f);
}
