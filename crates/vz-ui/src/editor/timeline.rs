//! The timeline: tabs, toolbar, track headers, ruler and (empty) tracks.

use super::EditorState;
use crate::theme;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense,
    Stroke, vec2,
};
use vz_core::{Project, US_PER_SEC};

const HEADER_W: f32 = 132.0;
const RULER_H: f32 = 26.0;

/// Pixels per second at a zoom level 0..1 (about 20 px/s zoomed out, 400 px/s zoomed in).
fn pixels_per_second(zoom: f32) -> f32 {
    20.0 * 20f32.powf(zoom.clamp(0.0, 1.0))
}

pub fn show(ui: &mut egui::Ui, state: &mut EditorState, project: &Project) {
    ui.add_space(6.0);
    tabs(ui);
    toolbar(ui, state);
    ui.add_space(2.0);
    tracks(ui, state, project);
}

fn tabs(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let tab = ui.label(RichText::new("Timeline 01").strong());
        ui.painter().hline(
            tab.rect.x_range(),
            tab.rect.bottom() + 3.0,
            Stroke::new(2.0, theme::ACCENT),
        );
        ui.add_space(6.0);
        ui.add_enabled(false, egui::Button::new("+").small())
            .on_disabled_hover_text("Multiple timelines arrive in Phase 5");
    });
}

fn toolbar(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        for name in [
            "Select",
            "Undo",
            "Redo",
            "Split",
            "Delete left",
            "Delete right",
            "Delete",
            "Marker",
            "Freeze",
        ] {
            ui.add_enabled(
                false,
                egui::Button::new(RichText::new(name).small()).small(),
            )
            .on_disabled_hover_text("Editing arrives in Phase 1");
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.add(
                egui::Slider::new(&mut state.zoom, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .on_hover_text("Zoom the timeline");
            ui.label(RichText::new("Zoom").color(theme::MUTED).small());
            ui.add_space(8.0);
            ui.toggle_value(
                &mut state.preview_axis,
                RichText::new("Preview axis").small(),
            );
            ui.toggle_value(&mut state.linkage, RichText::new("Linkage").small());
            ui.toggle_value(&mut state.snapping, RichText::new("Snap").small());
            ui.toggle_value(&mut state.magnet, RichText::new("Magnet").small());
        });
    });
}

fn tracks(ui: &mut egui::Ui, state: &EditorState, project: &Project) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    let painter = ui.painter_at(area);

    // headers column
    let header = Rect::from_min_max(area.min, Pos2::new(area.min.x + HEADER_W, area.max.y));
    painter.rect_filled(header, CornerRadius::ZERO, Color32::from_rgb(17, 17, 17));
    painter.vline(
        header.right(),
        area.y_range(),
        Stroke::new(1.0, theme::BORDER),
    );

    // ruler
    let lane_x0 = header.right();
    let ruler = Rect::from_min_max(
        Pos2::new(lane_x0, area.min.y),
        Pos2::new(area.max.x, area.min.y + RULER_H),
    );
    painter.rect_filled(ruler, CornerRadius::ZERO, Color32::from_rgb(14, 14, 14));
    painter.hline(
        area.x_range(),
        ruler.bottom(),
        Stroke::new(1.0, theme::BORDER),
    );
    draw_ruler(&painter, ruler, state, project);

    // track rows
    let rows: [(&str, f32); 3] = [("Text", 34.0), ("Main", 62.0), ("Audio", 40.0)];
    let mut y = ruler.bottom() + 6.0;
    for (name, h) in rows {
        let lane = Rect::from_min_max(Pos2::new(lane_x0, y), Pos2::new(area.max.x, y + h));
        let head = Rect::from_min_max(Pos2::new(area.min.x, y), Pos2::new(header.right(), y + h));
        painter.rect_filled(lane, CornerRadius::ZERO, Color32::from_rgb(19, 19, 19));
        painter.hline(
            area.x_range(),
            lane.bottom() + 3.0,
            Stroke::new(1.0, Color32::from_rgb(26, 26, 26)),
        );
        painter.text(
            Pos2::new(head.left() + 12.0, head.center().y - 8.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(12.0),
            theme::TEXT,
        );
        // lock / eye / speaker placeholders
        for i in 0..3 {
            let c = Pos2::new(head.left() + 18.0 + i as f32 * 22.0, head.center().y + 10.0);
            painter.circle_stroke(c, 6.0, Stroke::new(1.0, Color32::from_rgb(80, 80, 80)));
        }
        if name == "Main" {
            let cover = Rect::from_min_size(
                Pos2::new(head.right() - 62.0, head.top() + 8.0),
                vec2(52.0, head.height() - 16.0),
            );
            painter.rect_stroke(
                cover,
                CornerRadius::same(4),
                Stroke::new(1.0, theme::BORDER),
                egui::StrokeKind::Inside,
            );
            painter.text(
                cover.center(),
                Align2::CENTER_CENTER,
                "Cover",
                FontId::proportional(11.0),
                theme::MUTED,
            );
        }
        y += h + 6.0;
    }

    // hint in the middle of the lanes
    let lanes_mid = Pos2::new(
        (lane_x0 + area.max.x) / 2.0,
        (ruler.bottom() + area.max.y) / 2.0 + 10.0,
    );
    painter.text(
        lanes_mid,
        Align2::CENTER_CENTER,
        "Drag material here and start to create",
        FontId::proportional(14.0),
        Color32::from_rgb(110, 110, 110),
    );

    // playhead
    let px = lane_x0 + (state.playhead as f32 / US_PER_SEC as f32) * pixels_per_second(state.zoom);
    if px < area.max.x {
        painter.vline(px, area.min.y..=area.max.y, Stroke::new(1.5, theme::TEXT));
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(px - 6.0, area.min.y),
                Pos2::new(px + 6.0, area.min.y),
                Pos2::new(px, area.min.y + 10.0),
            ],
            theme::TEXT,
            Stroke::NONE,
        ));
    }
}

fn draw_ruler(painter: &egui::Painter, ruler: Rect, state: &EditorState, project: &Project) {
    let pps = pixels_per_second(state.zoom);
    // the first "nice" step that leaves at least 90 px between labels
    let steps = [
        1.0f32, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
    ];
    let step = steps
        .into_iter()
        .find(|s| s * pps >= 90.0)
        .unwrap_or(3600.0);
    let mut t = 0.0f32;
    loop {
        let x = ruler.left() + t * pps;
        if x > ruler.right() {
            break;
        }
        painter.vline(
            x,
            (ruler.bottom() - 9.0)..=ruler.bottom(),
            Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
        );
        let micros = (t * US_PER_SEC as f32) as i64;
        painter.text(
            Pos2::new(x + 4.0, ruler.top() + 8.0),
            Align2::LEFT_CENTER,
            project.frame_rate.timecode(micros),
            FontId::monospace(10.0),
            theme::MUTED,
        );
        // minor ticks
        for k in 1..5 {
            let mx = x + step * pps * k as f32 / 5.0;
            if mx < ruler.right() {
                painter.vline(
                    mx,
                    (ruler.bottom() - 4.0)..=ruler.bottom(),
                    Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
                );
            }
        }
        t += step;
    }
}

#[cfg(test)]
mod tests {
    use super::pixels_per_second;

    #[test]
    fn zoom_range_is_sensible() {
        assert!((pixels_per_second(0.0) - 20.0).abs() < 0.01);
        assert!((pixels_per_second(1.0) - 400.0).abs() < 0.01);
        assert!(pixels_per_second(0.5) > pixels_per_second(0.2));
        // out-of-range values are clamped
        assert_eq!(pixels_per_second(-3.0), pixels_per_second(0.0));
        assert_eq!(pixels_per_second(9.0), pixels_per_second(1.0));
    }
}
