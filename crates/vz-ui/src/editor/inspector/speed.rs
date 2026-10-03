//! The Speed tab of the inspector: constant speed, speed curves, reverse, freeze frame.

use super::{Ctx, key, row, section_title};
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, vec2};
use vz_anim::{Animatable, Easing, Keyframe};
use vz_core::{Clip, MediaKind, Micros, US_PER_SEC};
use vz_presets::speed::speed_curves;

/// The speeds the curve editor can show: from a tenth to ten times.
const LOW: f32 = 0.1;
const HIGH: f32 = 10.0;

fn has_sound(ctx: &Ctx<'_>, clip: &Clip) -> bool {
    clip.media()
        .and_then(|m| ctx.doc.project.asset(m))
        .is_some_and(|a| a.has_audio || a.kind == MediaKind::Audio)
}

pub fn speed_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let curve_mode = clip.speed.is_animated() || ctx.state.speed_curve_mode;
    section_title(ui, "Speed");
    ui.horizontal(|ui| {
        if ui.selectable_label(!curve_mode, "Standard").clicked() {
            ctx.state.speed_curve_mode = false;
            if clip.speed.is_animated() {
                let _ = ctx.doc.set_speed(id, 1.0, None);
            }
        }
        if ui.selectable_label(curve_mode, "Curve").clicked() {
            ctx.state.speed_curve_mode = true;
            if !clip.speed.is_animated() {
                let v = clip.speed_at(0) as f32;
                let d = clip.duration;
                let _ = ctx.doc.update_clip(id, "Speed curve", None, |c| {
                    c.speed = Animatable::Keyframed(vec![
                        Keyframe {
                            time: 0,
                            value: v,
                            easing: Easing::Linear,
                        },
                        Keyframe {
                            time: d,
                            value: v,
                            easing: Easing::Linear,
                        },
                    ]);
                });
            }
        }
    });
    ui.add_space(6.0);
    if curve_mode {
        curve_tab(ui, ctx, clip);
    } else {
        standard_tab(ui, ctx, clip);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let mut reverse = clip.reverse;
        if ui
            .checkbox(&mut reverse, "Reverse")
            .on_hover_text("Play this part of the file backwards")
            .changed()
        {
            let _ = ctx.doc.set_reverse(id, reverse);
        }
        if has_sound(ctx, clip) {
            let mut keep = clip.keep_pitch;
            if ui
                .checkbox(&mut keep, "Keep pitch")
                .on_hover_text("Voices keep their normal pitch when the speed changes")
                .changed()
            {
                let _ = ctx.doc.set_keep_pitch(id, keep);
            }
        }
    });
    ui.add_space(6.0);
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    let inside = ctx.state.playhead > clip.start + frame && ctx.state.playhead < clip.end() - frame;
    if ui
        .add_enabled(
            inside && clip.media().is_some(),
            egui::Button::new("Freeze frame here (3 s)"),
        )
        .on_hover_text("Cut the clip at the playhead and insert a still picture of that moment")
        .clicked()
    {
        match ctx
            .doc
            .freeze_frame(id, ctx.state.playhead, vz_core::document::FREEZE_LENGTH)
        {
            Ok(still) => ctx.state.selection = vec![still],
            Err(e) => ctx.state.say(format!("Could not freeze: {e}")),
        }
    }
    if clip.speed_at(0) == 0.0 && !clip.speed.is_animated() {
        ui.add_space(4.0);
        ui.label(
            RichText::new("This clip is a freeze frame: one picture, no sound.")
                .color(theme::MUTED)
                .small(),
        );
    }
}

fn standard_tab(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    if clip.speed_at(0) == 0.0 {
        return; // a freeze frame has no speed to set
    }
    egui::Grid::new("speed_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Speed", |ui| {
                let mut v = clip.speed_at(0) as f32;
                let resp = ui.add(
                    egui::Slider::new(&mut v, 0.1..=10.0)
                        .logarithmic(true)
                        .suffix("×")
                        .max_decimals(2)
                        .clamping(egui::SliderClamping::Never),
                );
                if resp.changed() {
                    let _ = ctx.doc.set_speed(id, v, Some(key(id, "speed")));
                }
                if resp.drag_stopped() || resp.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
            row(ui, "Quick", |ui| {
                ui.horizontal(|ui| {
                    for v in [0.5f32, 1.0, 2.0, 3.0] {
                        let on = (clip.speed_at(0) as f32 - v).abs() < 0.001;
                        if ui.selectable_label(on, format!("{v}×")).clicked() {
                            let _ = ctx.doc.set_speed(id, v, None);
                        }
                    }
                });
            });
            row(ui, "Length", |ui| {
                let mut seconds = clip.duration as f32 / US_PER_SEC as f32;
                let resp = ui.add(
                    egui::DragValue::new(&mut seconds)
                        .speed(0.05)
                        .range(0.04..=3600.0)
                        .suffix(" s")
                        .max_decimals(2),
                );
                if resp.changed() && seconds > 0.0 {
                    // choosing a length picks the speed that fits the same part of the file into it
                    let span = clip.source_span() as f64 / US_PER_SEC as f64;
                    let _ = ctx.doc.set_speed(
                        id,
                        (span / f64::from(seconds)) as f32,
                        Some(key(id, "speed-length")),
                    );
                }
                if resp.drag_stopped() || resp.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
        });
}

fn curve_tab(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.label(RichText::new("Presets").color(theme::MUTED).small());
    ui.horizontal_wrapped(|ui| {
        for curve in speed_curves() {
            if ui.button(curve.name).clicked() {
                let _ = ctx.doc.set_speed_curve(id, &curve.points);
            }
        }
    });
    ui.add_space(6.0);
    curve_editor(ui, ctx, clip);
    ui.add_space(4.0);
    ui.label(
        RichText::new("Drag a point. Double-click to add one, right-click a point to remove it. Up is faster. The dotted line is normal speed.")
            .color(theme::MUTED)
            .small(),
    );
    let seconds = clip.duration as f32 / US_PER_SEC as f32;
    let used = clip.source_span() as f32 / US_PER_SEC as f32;
    ui.label(
        RichText::new(format!(
            "The clip lasts {seconds:.1} s and uses {used:.1} s of the file."
        ))
        .color(theme::MUTED)
        .small(),
    );
}

fn y_of(rect: Rect, speed: f32) -> f32 {
    let t = ((speed.clamp(LOW, HIGH) / LOW).ln() / (HIGH / LOW).ln()).clamp(0.0, 1.0);
    rect.bottom() - 6.0 - t * (rect.height() - 12.0)
}

fn speed_of(rect: Rect, y: f32) -> f32 {
    let t = ((rect.bottom() - 6.0 - y) / (rect.height() - 12.0)).clamp(0.0, 1.0);
    LOW * (HIGH / LOW).powf(t)
}

fn x_of(rect: Rect, time: Micros, duration: Micros) -> f32 {
    rect.left() + 6.0 + (time as f32 / duration.max(1) as f32) * (rect.width() - 12.0)
}

fn time_of(rect: Rect, x: f32, duration: Micros) -> Micros {
    let t = ((x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0);
    (f64::from(t) * duration as f64) as Micros
}

fn curve_editor(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let width = ui.available_width().min(380.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 130.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(16, 16, 16));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    // the guide lines at 0.5x, 1x, 2x
    for (s, dotted) in [(0.5f32, false), (1.0, true), (2.0, false)] {
        let y = y_of(rect, s);
        if dotted {
            painter.add(egui::Shape::dashed_line(
                &[Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, Color32::from_rgb(110, 110, 110)),
                4.0,
                4.0,
            ));
        } else {
            painter.hline(
                rect.x_range(),
                y,
                Stroke::new(1.0, Color32::from_rgb(40, 40, 40)),
            );
        }
        painter.text(
            Pos2::new(rect.left() + 3.0, y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{s}×"),
            egui::FontId::proportional(9.0),
            theme::MUTED,
        );
    }
    // the curve itself
    let points: Vec<Pos2> = (0..=100)
        .map(|i| {
            let t = (clip.duration as f64 * f64::from(i) / 100.0) as Micros;
            Pos2::new(
                x_of(rect, t, clip.duration),
                y_of(rect, clip.speed_at(t) as f32),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, theme::ACCENT)));
    // the playhead
    if clip.contains(ctx.state.playhead) {
        let x = x_of(rect, ctx.state.playhead - clip.start, clip.duration);
        painter.vline(x, rect.y_range(), Stroke::new(1.0, Color32::WHITE));
    }

    let keys: Vec<Keyframe<f32>> = clip.speed.keys().to_vec();
    let handles: Vec<Pos2> = keys
        .iter()
        .map(|k| Pos2::new(x_of(rect, k.time, clip.duration), y_of(rect, k.value)))
        .collect();
    let drag_id = ui.id().with(("speed_point", id));
    let hover = ui.input(|i| i.pointer.hover_pos());
    for (i, h) in handles.iter().enumerate() {
        let hot = hover.is_some_and(|p| p.distance(*h) < 9.0);
        painter.circle_filled(*h, if hot { 6.0 } else { 5.0 }, Color32::WHITE);
        painter.circle_stroke(*h, 5.0, Stroke::new(1.5, theme::ACCENT));
        let _ = i;
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
    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        ui.memory_mut(|m| match nearest(origin) {
            Some(i) => {
                m.data.insert_temp(drag_id, i);
            }
            None => m.data.remove::<usize>(drag_id),
        });
    }
    if resp.dragged()
        && let Some(i) = ui.memory(|m| m.data.get_temp::<usize>(drag_id))
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let last = keys.len().saturating_sub(1);
        let speed = speed_of(rect, pos.y);
        let min_gap = ctx.doc.project.frame_rate.frame_start(1);
        let lo = if i == 0 {
            0
        } else {
            keys[i - 1].time + min_gap
        };
        let hi = if i == last {
            clip.duration
        } else {
            keys[i + 1].time - min_gap
        };
        let time = if i == 0 {
            0
        } else if i == last {
            clip.duration
        } else {
            time_of(rect, pos.x, clip.duration).clamp(lo, hi.max(lo))
        };
        let _ = ctx
            .doc
            .update_clip(id, "Speed curve", Some(key(id, "speed-curve")), |c| {
                if let Animatable::Keyframed(k) = &mut c.speed
                    && let Some(point) = k.get_mut(i)
                {
                    point.time = time;
                    point.value = speed;
                }
            });
    }
    if resp.drag_stopped() {
        ctx.doc.seal_history();
        ui.memory_mut(|m| m.data.remove::<usize>(drag_id));
    }
    // double-click adds a point on the curve; right-click removes a point
    if resp.double_clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && nearest(pos).is_none()
    {
        let time = time_of(rect, pos.x, clip.duration);
        let value = clip.speed_at(time) as f32;
        let _ = ctx.doc.update_clip(id, "Add speed point", None, |c| {
            if let Animatable::Keyframed(k) = &mut c.speed {
                let at = k.partition_point(|p| p.time < time);
                k.insert(
                    at,
                    Keyframe {
                        time,
                        value,
                        easing: Easing::Linear,
                    },
                );
            }
        });
    }
    if resp.secondary_clicked()
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && let Some(i) = nearest(pos)
        && keys.len() > 2
        && i != 0
        && i != keys.len() - 1
    {
        let _ = ctx.doc.update_clip(id, "Remove speed point", None, |c| {
            if let Animatable::Keyframed(k) = &mut c.speed {
                k.remove(i);
            }
        });
    }
}
