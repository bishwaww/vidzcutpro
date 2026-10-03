//! The pen tool: draw a path on the picture with clicks (corners) and drags (curves), or change the points of a path
//! that is already on the timeline. While a session is open the picture's own handles are off and the mouse belongs
//! to the pen.
//!
//! Points are kept in canvas pixels while editing (handles are offsets from their point). Writing them into a clip
//! fits the shape's box around the curve and stores points as shares of that box (`vz_core::PenPath`).

use super::Ctx;
use super::canvas::{View, merge_key};
use super::geometry::Placed;
use super::inspector;
use crate::theme;
use eframe::egui::{self, Color32, Key, Modifiers, Pos2, Rect, Shape, Stroke, Vec2, vec2};
use vz_core::{ClipId, Content, PenNode, PenPath, ShapeContent, ShapeKind, TextStroke, US_PER_SEC};

/// How long a new path stays on screen.
const NEW_PATH_LENGTH: i64 = 4 * US_PER_SEC;
/// How close (screen points) the mouse must be to grab a point or handle.
const GRAB_PX: f32 = 9.0;
/// Pieces a curve is cut into when measuring it or testing a click against it.
const STEPS: usize = 24;
/// A path box is never thinner than this (canvas pixels), so a straight line still has a box to select.
const MIN_BOX: f32 = 4.0;

/// What the mouse is dragging.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// Pulling the handles out of a point that was just added.
    NewHandles(usize),
    /// Moving a point; the offset keeps the grab spot under the mouse.
    Node(usize, Vec2),
    In(usize),
    Out(usize),
}

/// A pen session: drawing a new path, or editing the points of a path clip.
#[derive(Clone, Debug)]
pub struct PenSession {
    /// `Some` while editing a clip that is already on the timeline.
    pub clip: Option<ClipId>,
    pub nodes: Vec<PenNode>,
    pub closed: bool,
    pub selected: Option<usize>,
    grab: Option<Grab>,
}

impl PenSession {
    pub fn draw() -> Self {
        Self {
            clip: None,
            nodes: Vec::new(),
            closed: false,
            selected: None,
            grab: None,
        }
    }

    pub fn edit(clip: ClipId, shape: &ShapeContent, placed: &Placed) -> Option<Self> {
        let path = shape.path.as_ref()?;
        Some(Self {
            clip: Some(clip),
            nodes: to_canvas(path, placed),
            closed: path.closed,
            selected: None,
            grab: None,
        })
    }
}

// ---- geometry (pure, tested) --------------------------------------------------------------------------------------------

fn pos(n: &PenNode) -> Pos2 {
    Pos2::new(n.x, n.y)
}

fn out_point(n: &PenNode) -> Pos2 {
    Pos2::new(n.x + n.out_x, n.y + n.out_y)
}

fn in_point(n: &PenNode) -> Pos2 {
    Pos2::new(n.x + n.in_x, n.y + n.in_y)
}

/// The four control points of the piece of path from `a` to `b` (a straight piece has its handles on its ends).
fn cubic(a: &PenNode, b: &PenNode) -> [Pos2; 4] {
    if a.has_handles() || b.has_handles() {
        [pos(a), out_point(a), in_point(b), pos(b)]
    } else {
        [pos(a), pos(a), pos(b), pos(b)]
    }
}

fn at(c: &[Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Pos2::new(
        a * c[0].x + b * c[1].x + cc * c[2].x + d * c[3].x,
        a * c[0].y + b * c[1].y + cc * c[2].y + d * c[3].y,
    )
}

/// Pairs of neighbouring points along the path (and the closing pair when closed).
fn pairs(len: usize, closed: bool) -> impl Iterator<Item = (usize, usize)> {
    let count = if len < 2 {
        0
    } else if closed && len > 2 {
        len
    } else {
        len - 1
    };
    (0..count).map(move |i| (i, (i + 1) % len))
}

/// The curve as a list of points.
pub fn sample(nodes: &[PenNode], closed: bool) -> Vec<Pos2> {
    let mut out = Vec::new();
    if nodes.len() == 1 {
        out.push(pos(&nodes[0]));
    }
    for (i, j) in pairs(nodes.len(), closed) {
        let c = cubic(&nodes[i], &nodes[j]);
        if out.is_empty() {
            out.push(c[0]);
        }
        for s in 1..=STEPS {
            out.push(at(&c, s as f32 / STEPS as f32));
        }
    }
    out
}

/// A path ready to be stored in a shape: its box in reference pixels, and where the box is on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    pub path: PenPath,
    pub width: f32,
    pub height: f32,
    pub center: Pos2,
}

/// Fit a box around points given in canvas pixels. `unit` is canvas pixels per reference pixel.
pub fn fit(nodes: &[PenNode], closed: bool, unit: f32) -> Option<Fitted> {
    let points = sample(nodes, closed);
    let first = points.first()?;
    let (mut lo, mut hi) = (*first, *first);
    for p in &points {
        lo = Pos2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pos2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let centre = Pos2::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    let (w, h) = ((hi.x - lo.x).max(MIN_BOX), (hi.y - lo.y).max(MIN_BOX));
    let nodes = nodes
        .iter()
        .map(|n| PenNode {
            x: (n.x - centre.x) / w + 0.5,
            y: (n.y - centre.y) / h + 0.5,
            in_x: n.in_x / w,
            in_y: n.in_y / h,
            out_x: n.out_x / w,
            out_y: n.out_y / h,
        })
        .collect();
    Some(Fitted {
        path: PenPath { nodes, closed },
        width: w / unit,
        height: h / unit,
        center: centre,
    })
}

/// The points of a stored path as they are on the canvas, with the layer's scale and turn applied.
pub fn to_canvas(path: &PenPath, placed: &Placed) -> Vec<PenNode> {
    let (sin, cos) = placed.rotation.to_radians().sin_cos();
    let (w, h) = (
        placed.size.x * placed.scale.x,
        placed.size.y * placed.scale.y,
    );
    let turn = |x: f32, y: f32| (x * cos - y * sin, x * sin + y * cos);
    path.nodes
        .iter()
        .map(|n| {
            let (x, y) = turn((n.x - 0.5) * w, (n.y - 0.5) * h);
            let (ix, iy) = turn(n.in_x * w, n.in_y * h);
            let (ox, oy) = turn(n.out_x * w, n.out_y * h);
            PenNode {
                x: placed.center.x + x,
                y: placed.center.y + y,
                in_x: ix,
                in_y: iy,
                out_x: ox,
                out_y: oy,
            }
        })
        .collect()
}

/// Split the piece from `a` to `b` at `t` (0 to 1), keeping its shape: returns the changed `a`, the new point, and the changed `b`.
pub fn split(a: &PenNode, b: &PenNode, t: f32) -> (PenNode, PenNode, PenNode) {
    let curved = a.has_handles() || b.has_handles();
    let c = cubic(a, b);
    let lerp = |p: Pos2, q: Pos2| Pos2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
    let (q0, q1, q2) = (lerp(c[0], c[1]), lerp(c[1], c[2]), lerp(c[2], c[3]));
    let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
    let s = lerp(r0, r1);
    let mut left = *a;
    let mut right = *b;
    let mut mid = PenNode::corner(s.x, s.y);
    if curved {
        left.out_x = q0.x - a.x;
        left.out_y = q0.y - a.y;
        mid.in_x = r0.x - s.x;
        mid.in_y = r0.y - s.y;
        mid.out_x = r1.x - s.x;
        mid.out_y = r1.y - s.y;
        right.in_x = q2.x - b.x;
        right.in_y = q2.y - b.y;
    }
    (left, mid, right)
}

/// Where a point on the canvas touches the path: the piece (index of its first point) and the share along it.
pub fn segment_at(
    nodes: &[PenNode],
    closed: bool,
    p: Pos2,
    tolerance: f32,
) -> Option<(usize, f32)> {
    let mut best: Option<(f32, usize, f32)> = None;
    for (i, j) in pairs(nodes.len(), closed) {
        let c = cubic(&nodes[i], &nodes[j]);
        for s in 0..=STEPS {
            let t = s as f32 / STEPS as f32;
            let d = at(&c, t).distance(p);
            if d <= tolerance && best.is_none_or(|b| d < b.0) {
                best = Some((d, i, t));
            }
        }
    }
    best.map(|(_, i, t)| (i, t))
}

/// Give a corner point smooth handles along the line between its neighbours, or take its handles away.
pub fn toggle_smooth(nodes: &mut [PenNode], closed: bool, i: usize) {
    let n = nodes.len();
    if i >= n {
        return;
    }
    if nodes[i].has_handles() {
        let node = &mut nodes[i];
        (node.in_x, node.in_y, node.out_x, node.out_y) = (0.0, 0.0, 0.0, 0.0);
        return;
    }
    let prev = if i > 0 {
        Some(pos(&nodes[i - 1]))
    } else if closed && n > 2 {
        Some(pos(&nodes[n - 1]))
    } else {
        None
    };
    let next = if i + 1 < n {
        Some(pos(&nodes[i + 1]))
    } else if closed && n > 2 {
        Some(pos(&nodes[0]))
    } else {
        None
    };
    let here = pos(&nodes[i]);
    let (p, q) = (prev.unwrap_or(here), next.unwrap_or(here));
    let along = q - p;
    if along.length() < 1.0 {
        return;
    }
    let reach = (here.distance(p).min(here.distance(q)) / 3.0).max(1.0);
    let dir = along.normalized() * reach;
    let node = &mut nodes[i];
    (node.out_x, node.out_y) = (dir.x, dir.y);
    (node.in_x, node.in_y) = (-dir.x, -dir.y);
}

/// Drop a point added by the second click of a double click.
fn dedupe_tail(nodes: &mut Vec<PenNode>) {
    while nodes.len() >= 2 {
        let n = nodes.len();
        if pos(&nodes[n - 1]).distance(pos(&nodes[n - 2])) < 3.0 {
            nodes.pop();
        } else {
            break;
        }
    }
}

// ---- the tool -----------------------------------------------------------------------------------------------------------

fn unit(ctx: &Ctx<'_>) -> f32 {
    let c = ctx.doc.project.canvas;
    vz_text::unit(c.width, c.height)
}

/// Write the session's points into the clip being edited (one undo step per drag).
fn commit(ctx: &mut Ctx<'_>, s: &PenSession) {
    let Some(clip) = s.clip else { return };
    let Some(fit) = fit(&s.nodes, s.closed, unit(ctx)) else {
        return;
    };
    let canvas = ctx.doc.project.canvas;
    let (cx, cy) = (
        fit.center.x - canvas.width as f32 / 2.0,
        fit.center.y - canvas.height as f32 / 2.0,
    );
    let _ = ctx
        .doc
        .update_clip(clip, "Edit path", Some(merge_key(clip, "pen")), |c| {
            if let Content::Shape(sh) = &mut c.content {
                sh.kind = ShapeKind::Path;
                sh.path = Some(fit.path);
                sh.width = fit.width;
                sh.height = fit.height;
            }
            // the points are stored on the canvas as they are now, so the layer's own scale and turn are folded in
            c.transform.x = cx.into();
            c.transform.y = cy.into();
            c.transform.scale_x = 1.0.into();
            c.transform.scale_y = 1.0.into();
            c.transform.rotation = 0.0.into();
        });
}

/// Finish drawing: put the new path on the timeline as a shape.
fn finish_draft(ctx: &mut Ctx<'_>, mut s: PenSession) {
    dedupe_tail(&mut s.nodes);
    if s.nodes.len() < 2 {
        ctx.state.say("A path needs at least two points.");
        return;
    }
    let Some(fit) = fit(&s.nodes, s.closed, unit(ctx)) else {
        return;
    };
    let colour = [252, 198, 16, 255];
    let closed = fit.path.closed && fit.path.nodes.len() > 2;
    let shape = ShapeContent {
        kind: ShapeKind::Path,
        width: fit.width,
        height: fit.height,
        fill: closed.then_some(colour),
        stroke: (!closed).then_some(TextStroke {
            color: colour,
            width: 8.0,
        }),
        path: Some(fit.path),
        ..ShapeContent::new(ShapeKind::Path)
    };
    let canvas = ctx.doc.project.canvas;
    let (cx, cy) = (
        fit.center.x - canvas.width as f32 / 2.0,
        fit.center.y - canvas.height as f32 / 2.0,
    );
    match ctx
        .doc
        .add_shape_auto(ctx.state.playhead, NEW_PATH_LENGTH, shape)
    {
        Ok((_, id)) => {
            let _ = ctx.doc.update_clip(id, "Place path", None, |c| {
                c.transform.x = cx.into();
                c.transform.y = cy.into();
            });
            ctx.state.selection = vec![id];
            ctx.state.inspector_tab = inspector::Tab::Shape;
        }
        Err(e) => ctx.state.say(format!("Could not add the path: {e}")),
    }
}

/// Keys of the pen tool; they are taken before the other shortcuts so Delete or Esc do not hit the timeline.
pub fn keys(egui_ctx: &egui::Context, ctx: &mut Ctx<'_>) {
    if ctx.state.pen.is_none() || egui_ctx.egui_wants_keyboard_input() {
        return;
    }
    let pressed = |mods: Modifiers, key: Key| egui_ctx.input_mut(|i| i.consume_key(mods, key));
    if pressed(Modifiers::NONE, Key::Escape) {
        // drawing: throw the draft away; editing: leave (every change is already in the clip)
        ctx.state.pen = None;
    } else if pressed(Modifiers::NONE, Key::Enter) {
        if let Some(s) = ctx.state.pen.take()
            && s.clip.is_none()
        {
            finish_draft(ctx, s);
        }
    } else if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
        let Some(s) = &mut ctx.state.pen else { return };
        let which = s.selected.or(s.nodes.len().checked_sub(1));
        if let Some(i) = which
            && (s.clip.is_none() || s.nodes.len() > 2)
        {
            s.nodes.remove(i);
            s.selected = None;
            let s = s.clone();
            commit(ctx, &s);
            ctx.doc.seal_history();
        }
    } else if pressed(Modifiers::COMMAND, Key::Z)
        && let Some(s) = &mut ctx.state.pen
        && s.clip.is_none()
    {
        s.nodes.pop();
        s.selected = None;
    }
}

/// Draw the path and its points, and handle the mouse. Called instead of the layer handles while a session is open.
pub fn show(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    view: &View,
    resp: &egui::Response,
    painter: &egui::Painter,
) {
    let Some(mut s) = ctx.state.pen.clone() else {
        return;
    };
    let hover = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| view.rect.contains(*p));
    let canvas_pt = |p: Pos2| view.canvas(p);
    let tolerance = GRAB_PX / view.scale;

    if !ctx.state.playing {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let alt = ui.input(|i| i.modifiers.alt);
        let (pressed, down, released) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
            )
        });
        if pressed && resp.contains_pointer() {
            if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                press(ctx, &mut s, canvas_pt(origin), tolerance);
            }
        } else if down && let (Some(grab), Some(p)) = (s.grab, resp.interact_pointer_pos()) {
            let p = canvas_pt(p);
            match grab {
                Grab::NewHandles(i) => {
                    let d = p - pos(&s.nodes[i]);
                    let node = &mut s.nodes[i];
                    if d.length() > 3.0 / view.scale {
                        (node.out_x, node.out_y, node.in_x, node.in_y) = (d.x, d.y, -d.x, -d.y);
                    }
                }
                Grab::Node(i, offset) => {
                    let to = p + offset;
                    s.nodes[i].x = to.x;
                    s.nodes[i].y = to.y;
                }
                Grab::In(i) | Grab::Out(i) => {
                    let outgoing = matches!(grab, Grab::Out(_));
                    let d = p - pos(&s.nodes[i]);
                    let node = &mut s.nodes[i];
                    // a smooth point keeps its handles opposite each other (Alt breaks them apart)
                    let mirrored = !alt && node.in_x == -node.out_x && node.in_y == -node.out_y;
                    if outgoing {
                        (node.out_x, node.out_y) = (d.x, d.y);
                        if mirrored {
                            (node.in_x, node.in_y) = (-d.x, -d.y);
                        }
                    } else {
                        (node.in_x, node.in_y) = (d.x, d.y);
                        if mirrored {
                            (node.out_x, node.out_y) = (-d.x, -d.y);
                        }
                    }
                }
            }
            commit(ctx, &s);
        }
        if released {
            if s.grab.take().is_some() {
                commit(ctx, &s);
            }
            ctx.doc.seal_history();
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let p = canvas_pt(p);
            let node = s.nodes.iter().position(|n| pos(n).distance(p) <= tolerance);
            match (node, s.clip) {
                // the second click of a double click lands on the point the first one added
                (Some(i), None) if i + 1 == s.nodes.len() => {
                    finish_draft(ctx, s);
                    ctx.state.pen = None;
                    return;
                }
                (Some(i), _) => {
                    toggle_smooth(&mut s.nodes, s.closed, i);
                    commit(ctx, &s);
                    ctx.doc.seal_history();
                }
                (None, None) => {
                    finish_draft(ctx, s);
                    ctx.state.pen = None;
                    return;
                }
                (None, Some(_)) => {}
            }
        }
        if resp.secondary_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let p = canvas_pt(p);
            if let Some(i) = s.nodes.iter().position(|n| pos(n).distance(p) <= tolerance)
                && (s.clip.is_none() || s.nodes.len() > 2)
            {
                s.nodes.remove(i);
                s.selected = None;
                commit(ctx, &s);
                ctx.doc.seal_history();
            }
        }
    }

    // closing the draft by clicking its first point may have ended the session
    if ctx.state.pen.is_none() && s.clip.is_none() && s.nodes.is_empty() {
        return;
    }
    draw(painter, view, &s, hover.map(canvas_pt), tolerance);
    hint(painter, view.rect, &s);
    ctx.state.pen = Some(s);
}

/// The mouse went down at `p` (canvas pixels).
fn press(ctx: &mut Ctx<'_>, s: &mut PenSession, p: Pos2, tolerance: f32) {
    // handles of the selected point come first (they stick out over other things)
    if let Some(i) = s.selected.filter(|i| *i < s.nodes.len()) {
        let n = &s.nodes[i];
        if n.has_handles() {
            if out_point(n).distance(p) <= tolerance {
                s.grab = Some(Grab::Out(i));
                return;
            }
            if in_point(n).distance(p) <= tolerance {
                s.grab = Some(Grab::In(i));
                return;
            }
        }
    }
    if let Some(i) = s.nodes.iter().position(|n| pos(n).distance(p) <= tolerance) {
        // clicking the first point of a drawing closes it
        if s.clip.is_none() && i == 0 && s.nodes.len() >= 3 {
            s.closed = true;
            let done = std::mem::replace(s, PenSession::draw());
            finish_draft(ctx, done);
            ctx.state.pen = None;
            return;
        }
        s.selected = Some(i);
        s.grab = Some(Grab::Node(i, pos(&s.nodes[i]) - p));
        return;
    }
    if s.clip.is_some() {
        // editing: a click on the line adds a point there
        if let Some((i, t)) = segment_at(&s.nodes, s.closed, p, tolerance) {
            let j = (i + 1) % s.nodes.len();
            let (a, mid, b) = split(&s.nodes[i], &s.nodes[j], t);
            s.nodes[i] = a;
            s.nodes[j] = b;
            s.nodes.insert(i + 1, mid);
            s.selected = Some(i + 1);
            s.grab = Some(Grab::Node(i + 1, pos(&mid) - p));
        } else {
            s.selected = None;
        }
        return;
    }
    // drawing: a new point at the end; dragging pulls its handles out
    s.nodes.push(PenNode::corner(p.x, p.y));
    let i = s.nodes.len() - 1;
    s.selected = Some(i);
    s.grab = Some(Grab::NewHandles(i));
}

fn draw(
    painter: &egui::Painter,
    view: &View,
    s: &PenSession,
    cursor: Option<Pos2>,
    tolerance: f32,
) {
    let line = Stroke::new(1.5, theme::ACCENT);
    let screen = |p: Pos2| view.screen(p);
    let curve: Vec<Pos2> = sample(&s.nodes, s.closed).into_iter().map(screen).collect();
    if curve.len() >= 2 {
        painter.add(Shape::line(curve, line));
    }
    // while drawing, a thin line shows where the next piece would go
    if s.clip.is_none()
        && let (Some(last), Some(c)) = (s.nodes.last(), cursor)
    {
        painter.extend(Shape::dashed_line(
            &[screen(pos(last)), screen(c)],
            Stroke::new(1.0, Color32::from_white_alpha(120)),
            4.0,
            4.0,
        ));
    }
    for (i, n) in s.nodes.iter().enumerate() {
        let selected = s.selected == Some(i);
        if selected && n.has_handles() {
            for h in [in_point(n), out_point(n)] {
                painter.line_segment(
                    [screen(pos(n)), screen(h)],
                    Stroke::new(1.0, Color32::from_white_alpha(160)),
                );
                painter.circle_filled(screen(h), 4.0, theme::ACCENT);
            }
        }
        let size = if selected { 9.0 } else { 7.0 };
        let r = Rect::from_center_size(screen(pos(n)), vec2(size, size));
        let fill = if selected {
            theme::ACCENT
        } else {
            Color32::WHITE
        };
        // the first point of an open drawing shows it can close the path
        let closable = s.clip.is_none() && i == 0 && s.nodes.len() >= 3;
        if closable && cursor.is_some_and(|c| pos(n).distance(c) <= tolerance) {
            painter.circle_stroke(screen(pos(n)), 9.0, Stroke::new(2.0, theme::ACCENT));
        }
        painter.rect_filled(r, 1.0, fill);
        painter.rect_stroke(
            r,
            1.0,
            Stroke::new(1.0, Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    }
}

fn hint(painter: &egui::Painter, rect: Rect, s: &PenSession) {
    let text = if s.clip.is_some() {
        "Pen: drag points and handles. Click the line to add a point, right-click a point to delete it, double-click a point to smooth it. Esc or Enter to leave."
    } else {
        "Pen: click to add points, drag for a curve, click the first point (or double-click) to finish. Enter finishes an open line, Esc cancels."
    };
    let galley = painter.layout(
        text.to_owned(),
        egui::FontId::proportional(12.0),
        theme::TEXT,
        (rect.width() - 24.0).max(100.0),
    );
    let at = rect.left_top() + vec2(10.0, 8.0);
    let bg = Rect::from_min_size(at - vec2(6.0, 4.0), galley.size() + vec2(12.0, 8.0));
    painter.rect_filled(bg, 6.0, Color32::from_black_alpha(190));
    painter.galley(at, galley, theme::TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri() -> Vec<PenNode> {
        vec![
            PenNode::corner(100.0, 300.0),
            PenNode::corner(200.0, 100.0),
            PenNode::corner(300.0, 300.0),
        ]
    }

    fn placed(fit: &Fitted, unit: f32) -> Placed {
        Placed {
            center: fit.center,
            size: vec2(fit.width * unit, fit.height * unit),
            scale: vec2(1.0, 1.0),
            rotation: 0.0,
        }
    }

    #[test]
    fn fitting_a_path_and_placing_it_again_gives_the_same_points() {
        let mut nodes = tri();
        nodes[1].out_x = 60.0;
        nodes[1].out_y = -40.0;
        nodes[1].in_x = -60.0;
        nodes[1].in_y = 40.0;
        let f = fit(&nodes, false, 0.5).unwrap();
        let back = to_canvas(&f.path, &placed(&f, 0.5));
        for (a, b) in nodes.iter().zip(&back) {
            assert!(
                (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01,
                "{a:?} {b:?}"
            );
            assert!((a.out_x - b.out_x).abs() < 0.01 && (a.in_y - b.in_y).abs() < 0.01);
        }
        // stored points stay in the box except for handles
        assert!(f.path.nodes.iter().all(|n| (-0.01..=1.01).contains(&n.x)));
    }

    #[test]
    fn the_box_follows_the_curve_not_just_the_points() {
        let mut nodes = vec![PenNode::corner(0.0, 100.0), PenNode::corner(200.0, 100.0)];
        nodes[0].out_y = -150.0;
        nodes[1].in_y = -150.0;
        let f = fit(&nodes, false, 1.0).unwrap();
        // the curve rises to about 100 - 0.75 * 150 = -12.5, so the box is more than the 4 px minimum tall
        assert!(f.height > 100.0, "{}", f.height);
        assert!(f.center.y < 100.0);
    }

    #[test]
    fn a_flat_line_still_gets_a_box_to_select() {
        let nodes = vec![PenNode::corner(0.0, 50.0), PenNode::corner(100.0, 50.0)];
        let f = fit(&nodes, false, 1.0).unwrap();
        assert_eq!(f.height, MIN_BOX);
        assert_eq!(f.width, 100.0);
    }

    #[test]
    fn layer_scale_and_turn_are_applied_when_the_points_are_opened_for_editing() {
        let path = PenPath {
            nodes: vec![PenNode::corner(0.0, 0.5), PenNode::corner(1.0, 0.5)],
            closed: false,
        };
        let p = Placed {
            center: Pos2::new(500.0, 500.0),
            size: vec2(100.0, 20.0),
            scale: vec2(2.0, 1.0),
            rotation: 90.0,
        };
        let n = to_canvas(&path, &p);
        // 200 wide after scaling, turned a quarter turn clockwise: the line now runs top to bottom
        assert!(
            (n[0].x - 500.0).abs() < 0.01 && (n[0].y - 400.0).abs() < 0.01,
            "{:?}",
            n[0]
        );
        assert!(
            (n[1].x - 500.0).abs() < 0.01 && (n[1].y - 600.0).abs() < 0.01,
            "{:?}",
            n[1]
        );
    }

    #[test]
    fn splitting_a_straight_piece_adds_a_corner_on_the_line() {
        let nodes = tri();
        let (a, mid, b) = split(&nodes[0], &nodes[1], 0.5);
        assert_eq!((mid.x, mid.y), (150.0, 200.0));
        assert!(!mid.has_handles() && !a.has_handles() && !b.has_handles());
    }

    #[test]
    fn splitting_a_curve_keeps_its_shape() {
        let mut nodes = tri();
        nodes[0].out_x = 80.0;
        nodes[0].out_y = -20.0;
        let before = sample(&nodes[..2], false);
        let (a, mid, b) = split(&nodes[0], &nodes[1], 0.4);
        let after = sample(&[a, mid, b], false);
        // every point of the old curve is close to the new one
        for p in before.iter().step_by(4) {
            let d = after
                .iter()
                .map(|q| q.distance(*p))
                .fold(f32::MAX, f32::min);
            assert!(d < 2.5, "{p:?} is {d} from the new curve");
        }
    }

    #[test]
    fn a_click_near_the_line_finds_the_piece_and_a_click_far_away_finds_nothing() {
        let nodes = tri();
        // the middle of the first piece, a little to the side
        let hit = segment_at(&nodes, false, Pos2::new(152.0, 202.0), 6.0).unwrap();
        assert_eq!(hit.0, 0);
        assert!((hit.1 - 0.5).abs() < 0.1);
        assert!(segment_at(&nodes, false, Pos2::new(10.0, 10.0), 6.0).is_none());
        // the closing piece only exists when the path is closed
        assert!(segment_at(&nodes, false, Pos2::new(200.0, 300.0), 4.0).is_none());
        assert_eq!(
            segment_at(&nodes, true, Pos2::new(200.0, 300.0), 4.0)
                .unwrap()
                .0,
            2
        );
    }

    #[test]
    fn toggling_smooth_adds_and_removes_handles() {
        let mut nodes = tri();
        toggle_smooth(&mut nodes, false, 1);
        assert!(nodes[1].has_handles());
        // the handles lie along the line between the neighbours, opposite each other
        assert_eq!(
            (nodes[1].in_x, nodes[1].in_y),
            (-nodes[1].out_x, -nodes[1].out_y)
        );
        assert!(nodes[1].out_y.abs() < 0.01 && nodes[1].out_x > 0.0);
        toggle_smooth(&mut nodes, false, 1);
        assert!(!nodes[1].has_handles());
    }

    #[test]
    fn a_second_click_of_a_double_click_does_not_leave_a_twin_point() {
        let mut nodes = tri();
        nodes.push(PenNode::corner(300.5, 300.5));
        dedupe_tail(&mut nodes);
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn closed_paths_have_a_closing_piece_and_a_single_point_has_no_pieces() {
        assert_eq!(pairs(3, false).count(), 2);
        assert_eq!(pairs(3, true).count(), 3);
        assert_eq!(
            pairs(2, true).count(),
            1,
            "two points close into the same line"
        );
        assert_eq!(pairs(1, true).count(), 0);
        assert_eq!(sample(&tri()[..1], false).len(), 1);
    }
}
