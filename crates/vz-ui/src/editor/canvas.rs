//! Editing directly on the picture in the player: click to select, drag to move, corner handles to
//! resize, the round handle to turn, with guides that stick to the canvas edges and middle.

use super::Ctx;
use super::geometry::{Placed, pick, placed};
use crate::theme;
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, vec2,
};
use vz_core::ClipId;

/// What the mouse is doing to a layer on the canvas.
#[derive(Clone, Debug)]
pub enum CanvasDrag {
    Move {
        clip: ClipId,
        start: Pos2,
        press: Pos2,
    },
    Scale {
        clip: ClipId,
        start: Vec2,
        center: Pos2,
        rotation: f32,
        press_local: Vec2,
        press_dist: f32,
        uniform: bool,
    },
    Rotate {
        clip: ClipId,
        start_rotation: f32,
        start_angle: f32,
    },
}

/// A line the dragged layer is sticking to, in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    Vertical(f32),
    Horizontal(f32),
}

const HANDLE: f32 = 8.0;
const ROTATE_OFFSET: f32 = 30.0;
/// How close (in screen points) a layer's edge or middle must be to a guide line to stick to it.
const SNAP_PX: f32 = 7.0;
const GUIDE_COLOR: Color32 = Color32::from_rgb(255, 120, 200);

/// The unique key joining the changes of one drag (or slider) of one property of one clip into one undo step.
pub fn merge_key(clip: ClipId, property: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (clip, property).hash(&mut h);
    h.finish() | (1 << 63)
}

struct View {
    rect: Rect,
    /// Screen points per canvas pixel.
    scale: f32,
}

impl View {
    fn screen(&self, p: Pos2) -> Pos2 {
        self.rect.min + p.to_vec2() * self.scale
    }

    fn canvas(&self, p: Pos2) -> Pos2 {
        ((p - self.rect.min) / self.scale).to_pos2()
    }
}

fn selected_placed(ctx: &Ctx<'_>) -> Option<(ClipId, Placed)> {
    let [id] = ctx.state.selection[..] else {
        return None;
    };
    let clip = ctx.doc.project.timeline().clip(id)?;
    let locked = ctx
        .doc
        .project
        .timeline()
        .track_of(id)
        .is_some_and(|t| t.locked);
    if locked || !clip.contains(ctx.state.playhead) {
        return None;
    }
    placed(&ctx.doc.project, clip, ctx.state.playhead).map(|p| (id, p))
}

/// Handle positions on the canvas: four corners and the turn handle above the top edge.
fn handle_points(p: &Placed, view: &View) -> ([Pos2; 4], Pos2) {
    let corners = p.corners();
    let top_mid = Pos2::new(
        (corners[0].x + corners[1].x) / 2.0,
        (corners[0].y + corners[1].y) / 2.0,
    );
    // "up" in the layer's own turned frame
    let (sin, cos) = p.rotation.to_radians().sin_cos();
    let up = vec2(sin, -cos);
    let rotate = top_mid + up * (ROTATE_OFFSET / view.scale);
    (corners, rotate)
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, rect: Rect) {
    if rect.width() < 20.0 {
        return;
    }
    let canvas = ctx.doc.project.canvas;
    let view = View {
        rect,
        scale: rect.width() / canvas.width as f32,
    };
    let resp = ui.interact(rect, ui.id().with("canvas_area"), Sense::click_and_drag());
    let painter = ui.painter_at(rect.expand(40.0));

    overlays(&painter, ctx, &view);

    // handles on the selected layer
    let selected = selected_placed(ctx);
    if let Some((_, p)) = &selected {
        let (corners, rotate) = handle_points(p, &view);
        let outline: Vec<Pos2> = corners.iter().map(|c| view.screen(*c)).collect();
        painter.add(Shape::closed_line(outline, Stroke::new(1.5, theme::ACCENT)));
        for c in corners {
            let r = Rect::from_center_size(view.screen(c), vec2(HANDLE, HANDLE));
            painter.rect_filled(r, CornerRadius::same(2), Color32::WHITE);
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                Stroke::new(1.0, theme::ACCENT),
                StrokeKind::Inside,
            );
        }
        let top_mid = view.screen(Pos2::new(
            (corners[0].x + corners[1].x) / 2.0,
            (corners[0].y + corners[1].y) / 2.0,
        ));
        painter.line_segment(
            [top_mid, view.screen(rotate)],
            Stroke::new(1.0, theme::ACCENT),
        );
        painter.circle_filled(view.screen(rotate), 5.0, Color32::WHITE);
        painter.circle_stroke(view.screen(rotate), 5.0, Stroke::new(1.0, theme::ACCENT));
    }

    // guides while dragging
    for g in &ctx.state.guides {
        match *g {
            Guide::Vertical(x) => {
                let sx = view.screen(Pos2::new(x, 0.0)).x;
                painter.vline(sx, rect.y_range(), Stroke::new(1.0, GUIDE_COLOR));
            }
            Guide::Horizontal(y) => {
                let sy = view.screen(Pos2::new(0.0, y)).y;
                painter.hline(rect.x_range(), sy, Stroke::new(1.0, GUIDE_COLOR));
            }
        }
    }

    if ctx.state.playing {
        return; // no editing while it plays
    }

    // cursor hints
    if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
        && rect.contains(pos)
        && ctx.state.canvas_drag.is_none()
    {
        let icon = match &selected {
            Some((_, p)) => hover_icon(p, &view, pos, canvas_point(&view, pos)),
            None => None,
        };
        if let Some(icon) = icon {
            ui.ctx().set_cursor_icon(icon);
        }
    }

    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        start(ui, ctx, &view, &selected, origin);
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some(drag) = ctx.state.canvas_drag.clone()
    {
        drag_to(ctx, &view, drag, pos, ui.input(|i| i.modifiers.shift));
    }
    if resp.drag_stopped() {
        ctx.doc.seal_history();
        ctx.state.canvas_drag = None;
        ctx.state.guides.clear();
    }

    // a plain click selects (or clears)
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let toggle = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        match pick(&ctx.doc.project, ctx.state.playhead, view.canvas(pos)) {
            Some(id) if toggle => {
                if let Some(i) = ctx.state.selection.iter().position(|s| *s == id) {
                    ctx.state.selection.remove(i);
                } else {
                    ctx.state.selection.push(id);
                }
            }
            Some(id) => ctx.state.selection = vec![id],
            None if !toggle => ctx.state.selection.clear(),
            None => {}
        }
    }
}

fn canvas_point(view: &View, p: Pos2) -> Pos2 {
    view.canvas(p)
}

fn hover_icon(p: &Placed, view: &View, screen: Pos2, canvas: Pos2) -> Option<egui::CursorIcon> {
    let (corners, rotate) = handle_points(p, view);
    if view.screen(rotate).distance(screen) < 9.0 {
        return Some(egui::CursorIcon::Alias);
    }
    if corners
        .iter()
        .any(|c| view.screen(*c).distance(screen) < 9.0)
    {
        return Some(egui::CursorIcon::ResizeNeSw);
    }
    p.contains(canvas).then_some(egui::CursorIcon::Move)
}

fn start(
    ui: &egui::Ui,
    ctx: &mut Ctx<'_>,
    view: &View,
    selected: &Option<(ClipId, Placed)>,
    origin: Pos2,
) {
    let press = view.canvas(origin);
    // a layer that is not selected yet becomes selected by pressing on it
    let (id, p) = match selected {
        Some((id, p)) if hits_selected(p, view, origin) => (*id, *p),
        _ => {
            let Some(id) = pick(&ctx.doc.project, ctx.state.playhead, press) else {
                return;
            };
            ctx.state.selection = vec![id];
            let locked = ctx
                .doc
                .project
                .timeline()
                .track_of(id)
                .is_some_and(|t| t.locked);
            let Some(clip) = ctx.doc.project.timeline().clip(id) else {
                return;
            };
            let Some(p) = placed(&ctx.doc.project, clip, ctx.state.playhead) else {
                return;
            };
            if locked {
                return;
            }
            (id, p)
        }
    };
    let clip = ctx.doc.project.timeline().clip(id).cloned();
    let Some(clip) = clip else { return };
    let tr = clip.transform.at(clip.local_time(ctx.state.playhead));
    let (corners, rotate) = handle_points(&p, view);
    let _ = ui;
    ctx.state.canvas_drag = Some(if view.screen(rotate).distance(origin) < 10.0 {
        CanvasDrag::Rotate {
            clip: id,
            start_rotation: tr.rotation,
            start_angle: angle_of(p.center, press),
        }
    } else if corners
        .iter()
        .any(|c| view.screen(*c).distance(origin) < 10.0)
    {
        let (sin, cos) = p.rotation.to_radians().sin_cos();
        let d = press - p.center;
        CanvasDrag::Scale {
            clip: id,
            start: vec2(tr.scale_x, tr.scale_y),
            center: p.center,
            rotation: p.rotation,
            press_local: vec2(d.x * cos + d.y * sin, -d.x * sin + d.y * cos),
            press_dist: d.length().max(1.0),
            uniform: clip.transform.uniform_scale,
        }
    } else {
        CanvasDrag::Move {
            clip: id,
            start: Pos2::new(tr.x, tr.y),
            press,
        }
    });
}

fn hits_selected(p: &Placed, view: &View, screen: Pos2) -> bool {
    let (corners, rotate) = handle_points(p, view);
    view.screen(rotate).distance(screen) < 10.0
        || corners
            .iter()
            .any(|c| view.screen(*c).distance(screen) < 10.0)
        || p.contains(view.canvas(screen))
}

fn angle_of(center: Pos2, p: Pos2) -> f32 {
    (p.y - center.y).atan2(p.x - center.x).to_degrees()
}

fn wrap_degrees(mut a: f32) -> f32 {
    while a > 180.0 {
        a -= 360.0;
    }
    while a < -180.0 {
        a += 360.0;
    }
    a
}

/// One step of a drag with the mouse at screen position `pos`.
fn drag_to(ctx: &mut Ctx<'_>, view: &View, drag: CanvasDrag, pos: Pos2, shift: bool) {
    let point = view.canvas(pos);
    let canvas = ctx.doc.project.canvas;
    match drag {
        CanvasDrag::Move { clip, start, press } => {
            let Some(c) = ctx.doc.project.timeline().clip(clip).cloned() else {
                return;
            };
            let Some(mut p) = placed(&ctx.doc.project, &c, ctx.state.playhead) else {
                return;
            };
            let delta = point - press;
            let (mut x, mut y) = (start.x + delta.x, start.y + delta.y);
            // where the layer would be: find what to stick to
            p.center = Pos2::new(
                canvas.width as f32 / 2.0 + x,
                canvas.height as f32 / 2.0 + y,
            );
            let (dx, dy, guides) = snap_to_canvas(
                &p,
                canvas.width as f32,
                canvas.height as f32,
                SNAP_PX / view.scale,
            );
            x += dx;
            y += dy;
            ctx.state.guides = guides;
            let _ = ctx
                .doc
                .update_clip(clip, "Move", Some(merge_key(clip, "canvas-move")), |c| {
                    c.transform.x = x.into();
                    c.transform.y = y.into();
                });
        }
        CanvasDrag::Scale {
            clip,
            start,
            center,
            rotation,
            press_local,
            press_dist,
            uniform,
        } => {
            let d = point - center;
            let (sin, cos) = rotation.to_radians().sin_cos();
            let local = vec2(d.x * cos + d.y * sin, -d.x * sin + d.y * cos);
            let (kx, ky) = if uniform || shift {
                let k = d.length() / press_dist;
                (k, k)
            } else {
                (
                    (local.x / press_local.x.abs().max(1.0)).abs(),
                    (local.y / press_local.y.abs().max(1.0)).abs(),
                )
            };
            let (sx, sy) = (
                (start.x * kx).clamp(0.02, 20.0),
                (start.y * ky).clamp(0.02, 20.0),
            );
            let _ =
                ctx.doc
                    .update_clip(clip, "Resize", Some(merge_key(clip, "canvas-scale")), |c| {
                        c.transform.scale_x = sx.into();
                        c.transform.scale_y = sy.into();
                    });
        }
        CanvasDrag::Rotate {
            clip,
            start_rotation,
            start_angle,
        } => {
            let Some(c) = ctx.doc.project.timeline().clip(clip).cloned() else {
                return;
            };
            let Some(p) = placed(&ctx.doc.project, &c, ctx.state.playhead) else {
                return;
            };
            let mut rotation =
                wrap_degrees(start_rotation + angle_of(p.center, point) - start_angle);
            if shift {
                rotation = (rotation / 15.0).round() * 15.0;
            }
            let _ = ctx.doc.update_clip(
                clip,
                "Rotate",
                Some(merge_key(clip, "canvas-rotate")),
                |c| {
                    c.transform.rotation = rotation.into();
                },
            );
        }
    }
}

/// How far to nudge a layer so its middle or edges land on the canvas middle or edges, if any is close.
/// Returns the nudge and the guide lines to draw.
pub(super) fn snap_to_canvas(
    p: &Placed,
    canvas_w: f32,
    canvas_h: f32,
    tolerance: f32,
) -> (f32, f32, Vec<Guide>) {
    let corners = p.corners();
    let (min_x, max_x) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| {
        (lo.min(c.x), hi.max(c.x))
    });
    let (min_y, max_y) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| {
        (lo.min(c.y), hi.max(c.y))
    });
    let best = |mine: [f32; 3], targets: [f32; 3]| -> Option<(f32, f32)> {
        mine.iter()
            .flat_map(|m| targets.iter().map(move |t| (t - m, *t)))
            .filter(|(d, _)| d.abs() <= tolerance)
            .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
    };
    let mut guides = Vec::new();
    let dx = best([min_x, p.center.x, max_x], [0.0, canvas_w / 2.0, canvas_w]).map_or(
        0.0,
        |(d, line)| {
            guides.push(Guide::Vertical(line));
            d
        },
    );
    let dy = best([min_y, p.center.y, max_y], [0.0, canvas_h / 2.0, canvas_h]).map_or(
        0.0,
        |(d, line)| {
            guides.push(Guide::Horizontal(line));
            d
        },
    );
    (dx, dy, guides)
}

/// Safe zones and the thirds grid (shown on request, never exported).
fn overlays(painter: &egui::Painter, ctx: &Ctx<'_>, view: &View) {
    let rect = view.rect;
    if ctx.state.show_thirds {
        let stroke = Stroke::new(1.0, Color32::from_white_alpha(70));
        for i in 1..3 {
            let f = i as f32 / 3.0;
            painter.vline(rect.left() + rect.width() * f, rect.y_range(), stroke);
            painter.hline(rect.x_range(), rect.top() + rect.height() * f, stroke);
        }
    }
    if ctx.state.show_safe {
        // title-safe (90 %) and action-safe (95 %) boxes
        for (share, color) in [
            (0.95, Color32::from_rgba_unmultiplied(120, 200, 255, 120)),
            (0.90, Color32::from_rgba_unmultiplied(255, 200, 80, 140)),
        ] {
            let inset = vec2(
                rect.width() * (1.0 - share) / 2.0,
                rect.height() * (1.0 - share) / 2.0,
            );
            painter.rect_stroke(
                rect.shrink2(inset),
                CornerRadius::ZERO,
                Stroke::new(1.0, color),
                StrokeKind::Inside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed_at(x: f32, y: f32, w: f32, h: f32) -> Placed {
        Placed {
            center: Pos2::new(x, y),
            size: vec2(w, h),
            scale: vec2(1.0, 1.0),
            rotation: 0.0,
        }
    }

    #[test]
    fn a_layer_near_the_middle_sticks_to_it() {
        let p = placed_at(965.0, 538.0, 400.0, 200.0);
        let (dx, dy, guides) = snap_to_canvas(&p, 1920.0, 1080.0, 10.0);
        assert_eq!((dx, dy), (-5.0, 2.0));
        assert!(guides.contains(&Guide::Vertical(960.0)));
        assert!(guides.contains(&Guide::Horizontal(540.0)));
    }

    #[test]
    fn an_edge_near_the_canvas_edge_sticks_to_it() {
        // left edge at 6 px from the canvas edge
        let p = placed_at(206.0, 300.0, 400.0, 200.0);
        let (dx, _, guides) = snap_to_canvas(&p, 1920.0, 1080.0, 10.0);
        assert_eq!(dx, -6.0);
        assert!(guides.contains(&Guide::Vertical(0.0)));
    }

    #[test]
    fn far_from_everything_nothing_sticks() {
        let p = placed_at(500.0, 300.0, 100.0, 100.0);
        let (dx, dy, guides) = snap_to_canvas(&p, 1920.0, 1080.0, 10.0);
        assert_eq!((dx, dy), (0.0, 0.0));
        assert!(guides.is_empty());
    }

    #[test]
    fn the_closest_of_several_candidates_wins() {
        // middle is 8 away from the canvas middle, the right edge is 2 away from the canvas edge
        let p = placed_at(960.0 + 8.0, 300.0, 1904.0 - 8.0 * 2.0 + 4.0, 100.0);
        let (dx, _, guides) = snap_to_canvas(&p, 1920.0, 1080.0, 10.0);
        assert!(dx.abs() <= 8.0);
        assert_eq!(
            guides
                .iter()
                .filter(|g| matches!(g, Guide::Vertical(_)))
                .count(),
            1
        );
    }

    #[test]
    fn angles_wrap_into_a_half_turn_each_way() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(45.0), 45.0);
    }

    #[test]
    fn merge_keys_are_per_clip_and_property() {
        let a = ClipId::new();
        assert_eq!(merge_key(a, "m"), merge_key(a, "m"));
        assert_ne!(merge_key(a, "m"), merge_key(a, "s"));
    }
}
