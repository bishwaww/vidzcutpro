//! The player: where the video is shown, with the transport controls underneath.

use super::{Action, Ctx};
use crate::theme;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, vec2,
};
use vz_core::{AspectPreset, Micros};

#[derive(Clone, Copy)]
enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new("Player – Timeline 01")
                .color(theme::MUTED)
                .small(),
        );
    });

    // the controls take the bottom strip, the picture gets the rest
    let bar_h = 44.0;
    let avail = ui.available_rect_before_wrap();
    let canvas_area = Rect::from_min_max(avail.min, Pos2::new(avail.max.x, avail.max.y - bar_h));
    let bar_area = Rect::from_min_max(Pos2::new(avail.min.x, avail.max.y - bar_h), avail.max);

    ui.allocate_exact_size(canvas_area.size(), Sense::hover());
    if let Some(rect) = draw_canvas(ui, canvas_area.shrink2(vec2(14.0, 8.0)), ctx) {
        super::canvas::show(ui, ctx, rect);
    }

    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar_area)
            .layout(Layout::left_to_right(Align::Center)),
    );
    controls(&mut bar, ctx);
}

fn draw_canvas(ui: &egui::Ui, area: Rect, ctx: &Ctx<'_>) -> Option<Rect> {
    if area.width() < 20.0 || area.height() < 20.0 {
        return None;
    }
    let canvas = ctx.doc.project.canvas;
    let aspect = canvas.aspect();
    let (mut w, mut h) = (area.width(), area.width() / aspect);
    if h > area.height() {
        h = area.height();
        w = h * aspect;
    }
    let rect = Rect::from_center_size(area.center(), vec2(w, h));
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), Color32::BLACK);
    match ctx.player_texture {
        Some(texture) if ctx.doc.project.duration() > 0 => {
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        _ => {
            painter.text(
                rect.center() - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                "Your video appears here",
                FontId::proportional(14.0),
                theme::MUTED,
            );
            painter.text(
                rect.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                format!("{} × {}", canvas.width, canvas.height),
                FontId::proportional(11.0),
                Color32::from_rgb(90, 90, 90),
            );
        }
    }
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Outside,
    );
    Some(rect)
}

fn controls(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let rate = ctx.doc.project.frame_rate;
    let has_content = ctx.doc.project.duration() > 0;
    ui.add_space(14.0);
    ui.label(
        RichText::new(rate.timecode(ctx.state.playhead))
            .monospace()
            .color(theme::ACCENT),
    );
    ui.label(
        RichText::new(format!("/ {}", rate.timecode(ctx.doc.project.duration())))
            .monospace()
            .color(theme::MUTED),
    );

    ui.add_space(20.0);
    if transport(ui, Glyph::Previous, has_content, "Previous cut") {
        let t = previous_cut(ctx);
        ctx.seek(t);
    }
    let play_glyph = if ctx.state.playing {
        Glyph::Pause
    } else {
        Glyph::Play
    };
    if transport(ui, play_glyph, has_content, "Play / pause (Space)") {
        ctx.actions.push(Action::TogglePlay);
    }
    if transport(ui, Glyph::Next, has_content, "Next cut") {
        let t = next_cut(ctx);
        ctx.seek(t);
    }

    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add_space(14.0);
        let current = ctx.doc.project.aspect_preset();
        let text = current.map_or("Custom", AspectPreset::label);
        egui::ComboBox::from_id_salt("player_ratio")
            .selected_text(text)
            .width(80.0)
            .show_ui(ui, |ui| {
                for preset in AspectPreset::ALL {
                    if ui
                        .selectable_label(current == Some(preset), preset.label())
                        .clicked()
                    {
                        let _ = ctx.doc.set_aspect(preset);
                    }
                }
            });
        ui.label(RichText::new("Ratio").color(theme::MUTED).small());
        ui.add_space(10.0);
        if ui
            .selectable_label(ctx.state.show_scopes, RichText::new("Scopes").small())
            .on_hover_text("Show histogram, waveform and vectorscope of the picture")
            .clicked()
        {
            ctx.state.show_scopes = !ctx.state.show_scopes;
        }
    });
}

/// The cut before the playhead (or the start).
fn previous_cut(ctx: &Ctx<'_>) -> Micros {
    let t = ctx.state.playhead;
    let margin = ctx.doc.project.frame_rate.frame_start(1);
    cuts(ctx)
        .into_iter()
        .filter(|c| *c < t - margin)
        .max()
        .unwrap_or(0)
}

/// The cut after the playhead (or the end).
fn next_cut(ctx: &Ctx<'_>) -> Micros {
    let t = ctx.state.playhead;
    cuts(ctx)
        .into_iter()
        .filter(|c| *c > t)
        .min()
        .unwrap_or(ctx.doc.project.duration())
}

fn cuts(ctx: &Ctx<'_>) -> Vec<Micros> {
    let mut v = vec![0];
    v.extend(
        ctx.doc
            .project
            .timeline()
            .main_track()
            .clips
            .iter()
            .map(vz_core::Clip::end),
    );
    v
}

/// A round transport button drawn with simple shapes (no icon font needed). Returns true when clicked.
fn transport(ui: &mut egui::Ui, glyph: Glyph, enabled: bool, hint: &str) -> bool {
    let size = if matches!(glyph, Glyph::Play | Glyph::Pause) {
        34.0
    } else {
        26.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let painter = ui.painter();
    let hot = enabled && resp.hovered();
    let fg = if !enabled {
        Color32::from_rgb(95, 95, 95)
    } else if hot {
        theme::ACCENT
    } else {
        theme::TEXT
    };
    painter.circle_stroke(
        rect.center(),
        size / 2.0 - 1.0,
        Stroke::new(
            if hot { 1.5 } else { 1.0 },
            if hot { theme::ACCENT } else { theme::BORDER },
        ),
    );
    let c = rect.center();
    let tri = |dir: f32, s: f32| {
        egui::Shape::convex_polygon(
            vec![
                Pos2::new(c.x - dir * s * 0.6, c.y - s),
                Pos2::new(c.x - dir * s * 0.6, c.y + s),
                Pos2::new(c.x + dir * s * 0.9, c.y),
            ],
            fg,
            Stroke::NONE,
        )
    };
    match glyph {
        Glyph::Play => {
            painter.add(tri(1.0, 6.5));
        }
        Glyph::Pause => {
            for dx in [-4.0, 4.0] {
                painter.rect_filled(
                    Rect::from_center_size(Pos2::new(c.x + dx, c.y), vec2(3.5, 13.0)),
                    1.0,
                    fg,
                );
            }
        }
        Glyph::Previous => {
            painter.add(tri(-1.0, 5.0));
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - 5.0, c.y), vec2(2.0, 10.0)),
                0.0,
                fg,
            );
        }
        Glyph::Next => {
            painter.add(tri(1.0, 5.0));
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + 5.0, c.y), vec2(2.0, 10.0)),
                0.0,
                fg,
            );
        }
    }
    enabled && resp.on_hover_text(hint).clicked()
}
