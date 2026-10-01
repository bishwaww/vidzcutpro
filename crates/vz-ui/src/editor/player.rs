//! The player: where the video will be shown, with the transport controls underneath.

use super::EditorState;
use crate::theme;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, vec2,
};
use vz_core::{AspectPreset, Project};

#[derive(Clone, Copy)]
enum Glyph {
    Previous,
    Play,
    Next,
}

pub fn show(ui: &mut egui::Ui, state: &EditorState, project: &mut Project) {
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

    let (_, _) = ui.allocate_exact_size(canvas_area.size(), Sense::hover());
    draw_canvas(ui, canvas_area.shrink2(vec2(14.0, 8.0)), project);

    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar_area)
            .layout(Layout::left_to_right(Align::Center)),
    );
    controls(&mut bar, state, project);
}

fn draw_canvas(ui: &egui::Ui, area: Rect, project: &Project) {
    if area.width() < 20.0 || area.height() < 20.0 {
        return;
    }
    let aspect = project.canvas.aspect();
    let (mut w, mut h) = (area.width(), area.width() / aspect);
    if h > area.height() {
        h = area.height();
        w = h * aspect;
    }
    let rect = Rect::from_center_size(area.center(), vec2(w, h));
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), Color32::BLACK);
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Outside,
    );
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
        format!("{} × {}", project.canvas.width, project.canvas.height),
        FontId::proportional(11.0),
        Color32::from_rgb(90, 90, 90),
    );
}

fn controls(ui: &mut egui::Ui, state: &EditorState, project: &mut Project) {
    let tc = project.frame_rate.timecode(state.playhead);
    let total = project.frame_rate.timecode(project.duration());
    ui.add_space(14.0);
    ui.label(RichText::new(tc).monospace().color(theme::ACCENT));
    ui.label(
        RichText::new(format!("/ {total}"))
            .monospace()
            .color(theme::MUTED),
    );

    ui.add_space(20.0);
    transport(ui, Glyph::Previous);
    transport(ui, Glyph::Play);
    transport(ui, Glyph::Next);

    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add_space(14.0);
        let current = project.aspect_preset();
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
                        project.set_aspect(preset);
                    }
                }
            });
        ui.label(RichText::new("Ratio").color(theme::MUTED).small());
    });
}

/// A round transport button drawn with simple shapes (no icon font needed). Disabled until Phase 1.
fn transport(ui: &mut egui::Ui, glyph: Glyph) {
    let size = if matches!(glyph, Glyph::Play) {
        32.0
    } else {
        26.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    let fg = Color32::from_rgb(95, 95, 95);
    painter.circle_stroke(
        rect.center(),
        size / 2.0 - 1.0,
        Stroke::new(1.0, theme::BORDER),
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
            painter.add(tri(1.0, 6.0));
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
    resp.on_hover_text("Playback arrives in Phase 1");
}
