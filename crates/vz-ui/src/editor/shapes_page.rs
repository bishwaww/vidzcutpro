//! The Shapes page of the Stickers tab: line icons on dark tiles (the shapes people expect first, the pen tool among
//! them), then the other shapes.

use super::sticker_panel::add;
use super::{Ctx, pen};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, RichText, Sense, Stroke, StrokeKind, vec2,
};
use vz_core::ShapeKind as K;
use vz_presets::{ShapePreset, basic_shapes};

/// The shapes everybody expects, in the order people know them. The pen tool comes last.
const CORE: [K; 7] = [
    K::Rectangle,
    K::Ellipse,
    K::Triangle,
    K::Parallelogram,
    K::Trapezoid,
    K::Line,
    K::Arrow,
];

const TILE: f32 = 88.0;

/// What an icon is drawn with: closed shapes and open lines, in a box from -1 to 1.
fn outline(kind: K) -> Vec<(Vec<egui::Vec2>, bool)> {
    let p = |x: f32, y: f32| vec2(x, y);
    let regular = |sides: usize, radius: f32| -> Vec<egui::Vec2> {
        (0..sides)
            .map(|i| {
                let a =
                    -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / sides as f32;
                p(radius * a.cos(), radius * a.sin())
            })
            .collect()
    };
    match kind {
        K::Rectangle => vec![(
            vec![p(-0.8, -0.8), p(0.8, -0.8), p(0.8, 0.8), p(-0.8, 0.8)],
            true,
        )],
        K::Ellipse => vec![(regular(56, 0.92), true)],
        K::Triangle => vec![(vec![p(0.0, -0.85), p(0.95, 0.75), p(-0.95, 0.75)], true)],
        K::Parallelogram => vec![(
            vec![p(-0.55, -0.6), p(0.95, -0.6), p(0.55, 0.6), p(-0.95, 0.6)],
            true,
        )],
        K::Trapezoid => vec![(
            vec![p(-0.5, -0.6), p(0.5, -0.6), p(0.95, 0.6), p(-0.95, 0.6)],
            true,
        )],
        K::Line => vec![(vec![p(-0.95, 0.0), p(0.95, 0.0)], false)],
        K::Arrow => vec![
            (vec![p(-0.95, 0.0), p(0.9, 0.0)], false),
            (vec![p(0.4, -0.45), p(0.92, 0.0), p(0.4, 0.45)], false),
        ],
        K::Diamond => vec![(
            vec![p(0.0, -0.95), p(0.75, 0.0), p(0.0, 0.95), p(-0.75, 0.0)],
            true,
        )],
        K::Pentagon => vec![(regular(5, 0.95), true)],
        K::Hexagon => vec![(regular(6, 0.95), true)],
        K::Polygon => vec![(regular(8, 0.95), true)],
        K::Star => vec![(
            (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / 10.0;
                    let r = if i % 2 == 0 { 0.98 } else { 0.4 };
                    p(r * a.cos(), r * a.sin())
                })
                .collect(),
            true,
        )],
        K::Heart => vec![(
            (0..64)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 64.0;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos()
                        - 5.0 * (2.0 * t).cos()
                        - 2.0 * (3.0 * t).cos()
                        - (4.0 * t).cos();
                    p(x / 17.5, -y / 17.5 + 0.05)
                })
                .collect(),
            true,
        )],
        K::Plus => {
            let a = 0.3;
            vec![(
                vec![
                    p(-a, -0.9),
                    p(a, -0.9),
                    p(a, -a),
                    p(0.9, -a),
                    p(0.9, a),
                    p(a, a),
                    p(a, 0.9),
                    p(-a, 0.9),
                    p(-a, a),
                    p(-0.9, a),
                    p(-0.9, -a),
                    p(-a, -a),
                ],
                true,
            )]
        }
        K::Bubble => vec![(
            vec![
                p(-0.9, -0.7),
                p(0.9, -0.7),
                p(0.9, 0.3),
                p(-0.05, 0.3),
                p(-0.5, 0.8),
                p(-0.4, 0.3),
                p(-0.9, 0.3),
            ],
            true,
        )],
        K::Path => Vec::new(),
    }
}

/// One dark tile with an icon on it. `selected` lights it up.
fn icon_tile(
    ui: &mut egui::Ui,
    hint: &str,
    selected: bool,
    draw: impl FnOnce(&egui::Painter, egui::Rect),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click());
    let painter = ui.painter();
    let fill = if resp.hovered() {
        Color32::from_rgb(70, 70, 70)
    } else {
        Color32::from_rgb(54, 54, 54)
    };
    painter.rect_filled(rect, CornerRadius::same(10), fill);
    if selected || resp.hovered() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(10),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    draw(painter, rect);
    resp.on_hover_text(hint)
}

fn shape_icon(kind: K) -> impl FnOnce(&egui::Painter, egui::Rect) {
    move |painter, rect| {
        let half = rect.width() * 0.22;
        let stroke = Stroke::new(2.2, Color32::from_rgb(235, 235, 235));
        for (points, closed) in outline(kind) {
            let mut pts: Vec<Pos2> = points.iter().map(|v| rect.center() + *v * half).collect();
            if closed {
                pts.push(pts[0]);
            }
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(
        RichText::new("Click one to add it at the playhead.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(8.0);
    let search = ctx.state.search.to_lowercase();
    let all = basic_shapes();
    let matches = |name: &str| search.is_empty() || name.to_lowercase().contains(&search);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for kind in CORE {
                    if !matches(kind.label()) {
                        continue;
                    }
                    if icon_tile(ui, kind.label(), false, shape_icon(kind)).clicked()
                        && let Some(preset) = all.iter().find(|p| p.shape.kind == kind)
                    {
                        add(ctx, preset);
                    }
                }
                if matches("pen tool draw your own shape") {
                    let drawing = ctx.state.pen.as_ref().is_some_and(|p| p.clip.is_none());
                    let hint = "Pen tool: click on the picture to add points, drag to make curves, click the first point to close the shape";
                    if icon_tile(ui, hint, drawing, |painter, rect| {
                        painter.text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            theme::icons::PEN,
                            theme::icons::font(34.0),
                            Color32::from_rgb(235, 235, 235),
                        );
                    })
                    .clicked()
                    {
                        ctx.state.pen = (!drawing).then(pen::PenSession::draw);
                        ctx.state.selection.clear();
                    }
                }
            });
            let more: Vec<&ShapePreset> = all
                .iter()
                .filter(|p| !CORE.contains(&p.shape.kind) && matches(p.name))
                .collect();
            if !more.is_empty() {
                ui.add_space(16.0);
                ui.label(RichText::new("More shapes").color(theme::MUTED));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                    for preset in more {
                        if icon_tile(ui, preset.name, false, shape_icon(preset.shape.kind))
                            .clicked()
                        {
                            add(ctx, preset);
                        }
                    }
                });
            }
        });
}
