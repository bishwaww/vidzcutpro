//! The timeline: tabs, toolbar, track headers, ruler and the main track with its clips.

use super::Ctx;
use super::assets::short_duration;
use crate::theme;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, vec2,
};
use vz_core::{Clip, ClipId, MediaId, MediaKind, Micros, US_PER_SEC};

const HEADER_W: f32 = 132.0;
const RULER_H: f32 = 26.0;
const MAIN_ROW_H: f32 = 74.0;
/// How close to a clip's edge (in points) a click counts as grabbing the edge.
const EDGE_GRAB: f32 = 7.0;
const COMING_SOON: &str = "Coming soon";

/// What the mouse is doing to the timeline right now.
#[derive(Clone, Debug)]
pub enum TimelineDrag {
    /// Dragging the playhead along the ruler.
    Playhead,
    /// Dragging a whole clip to a new place in the order.
    Move {
        clip: ClipId,
        orig: Clip,
        grab_dx: f32,
        to_index: usize,
    },
    /// Dragging a clip's left edge.
    TrimLeft {
        clip: ClipId,
        orig: Clip,
        press_x: f32,
        key: u64,
    },
    /// Dragging a clip's right edge.
    TrimRight {
        clip: ClipId,
        orig: Clip,
        press_x: f32,
        key: u64,
    },
}

/// Pixels per second at a zoom level 0..1 (about 20 px/s zoomed out, 400 px/s zoomed in).
pub fn pixels_per_second(zoom: f32) -> f32 {
    20.0 * 20f32.powf(zoom.clamp(0.0, 1.0))
}

/// The zoom level that gives `pps` pixels per second.
fn zoom_for(pps: f32) -> f32 {
    ((pps / 20.0).max(1.0).ln() / 20f32.ln()).clamp(0.0, 1.0)
}

/// Converts between time and screen position for the current zoom and scroll.
#[derive(Clone, Copy)]
struct Geo {
    lane_x0: f32,
    pps: f32,
    scroll_x: f32,
}

impl Geo {
    fn x(self, t: Micros) -> f32 {
        self.lane_x0 + (t as f64 / US_PER_SEC as f64) as f32 * self.pps - self.scroll_x
    }

    fn time(self, x: f32) -> Micros {
        (f64::from(x - self.lane_x0 + self.scroll_x) / f64::from(self.pps) * US_PER_SEC as f64)
            as Micros
    }

    /// A length in points as a length of time.
    fn span(self, dx: f32) -> Micros {
        (f64::from(dx) / f64::from(self.pps) * US_PER_SEC as f64) as Micros
    }
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    // forget selected clips that no longer exist (after undo, delete, ...)
    let clips = &ctx.doc.project.timeline().main_track().clips;
    ctx.state
        .selection
        .retain(|id| clips.iter().any(|c| c.id == *id));

    ui.add_space(6.0);
    tabs(ui);
    toolbar(ui, ctx);
    ui.add_space(2.0);
    tracks(ui, ctx);
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

fn tool(ui: &mut egui::Ui, label: &str, enabled: bool, hint: &str) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).small()).small(),
    )
    .on_hover_text(hint)
    .on_disabled_hover_text(hint)
    .clicked()
}

fn toolbar(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let t = ctx.state.playhead;
    let on_clip = ctx.doc.project.timeline().main_track().clip_at(t).is_some();
    let can_delete = !ctx.state.selection.is_empty();
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        tool(ui, "Select", false, COMING_SOON);
        if tool(ui, "Undo", ctx.doc.can_undo(), "Undo (Ctrl+Z)") {
            ctx.doc.undo();
        }
        if tool(ui, "Redo", ctx.doc.can_redo(), "Redo (Ctrl+Shift+Z)") {
            ctx.doc.redo();
        }
        if tool(ui, "Split", on_clip, "Split at the playhead (Ctrl+B)") {
            split_at_playhead(ctx);
        }
        if tool(
            ui,
            "Delete left",
            on_clip,
            "Split and delete everything left of the playhead (Q)",
        ) {
            trim_to_playhead(ctx, Side::Left);
        }
        if tool(
            ui,
            "Delete right",
            on_clip,
            "Split and delete everything right of the playhead (W)",
        ) {
            trim_to_playhead(ctx, Side::Right);
        }
        if tool(
            ui,
            "Delete",
            can_delete,
            "Delete the selected clips and close the gap (Delete)",
        ) {
            delete_selection(ctx);
        }
        tool(ui, "Marker", false, COMING_SOON);
        tool(ui, "Freeze", false, COMING_SOON);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.add(
                egui::Slider::new(&mut ctx.state.zoom, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .on_hover_text("Zoom the timeline (Ctrl + mouse wheel)");
            if ui
                .small_button("Fit")
                .on_hover_text("Show the whole timeline")
                .clicked()
            {
                let secs = (ctx.doc.project.duration() as f32 / US_PER_SEC as f32).max(1.0);
                ctx.state.zoom = zoom_for(ctx.state.lane_width / (secs * 1.08));
                ctx.state.scroll_x = 0.0;
            }
            ui.label(RichText::new("Zoom").color(theme::MUTED).small());
            ui.add_space(8.0);
            ui.toggle_value(
                &mut ctx.state.preview_axis,
                RichText::new("Preview axis").small(),
            );
            ui.toggle_value(&mut ctx.state.linkage, RichText::new("Linkage").small());
            ui.toggle_value(&mut ctx.state.snapping, RichText::new("Snap").small());
            ui.toggle_value(&mut ctx.state.magnet, RichText::new("Magnet").small());
        });
    });
}

// ---- editing actions (also used by the keyboard shortcuts) --------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

pub fn split_at_playhead(ctx: &mut Ctx<'_>) {
    match ctx.doc.split_at(ctx.state.playhead) {
        Ok(new_clip) => ctx.state.selection = vec![new_clip],
        Err(_) => ctx
            .state
            .say("To split, put the playhead inside a clip, not at its very edge."),
    }
}

/// Q / W: remove everything left or right of the playhead in the clip under it (one undo step).
pub fn trim_to_playhead(ctx: &mut Ctx<'_>, side: Side) {
    let t = ctx.state.playhead;
    let Some(clip) = ctx.doc.project.timeline().main_track().clip_at(t).cloned() else {
        ctx.state.say("Put the playhead inside a clip first.");
        return;
    };
    let offset = t - clip.start;
    let result = match side {
        Side::Left => ctx.doc.trim_left(clip.id, offset, None),
        Side::Right => ctx.doc.trim_right(clip.id, offset, None),
    };
    match result {
        Ok(()) if side == Side::Left => ctx.seek(clip.start), // the clip now begins where it began; follow the cut
        Ok(()) => {}
        Err(_) => ctx
            .state
            .say("The playhead is at the very edge of the clip, so there is nothing to remove."),
    }
}

pub fn delete_selection(ctx: &mut Ctx<'_>) {
    if ctx.state.selection.is_empty() {
        return;
    }
    let ids = ctx.state.selection.clone();
    match ctx.doc.delete_clips(&ids) {
        Ok(()) => {
            ctx.state.selection.clear();
            let end = ctx.doc.project.duration();
            ctx.state.playhead = ctx.state.playhead.min(end);
        }
        Err(e) => ctx.state.say(format!("Could not delete: {e}")),
    }
}

// ---- the tracks -----------------------------------------------------------------------------------------------

fn tracks(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let area = ui.available_rect_before_wrap();
    let resp = ui.interact(area, ui.id().with("timeline_area"), Sense::click_and_drag());
    ui.allocate_rect(area, Sense::hover());

    let lane_x0 = area.min.x + HEADER_W;
    ctx.state.lane_width = (area.max.x - lane_x0).max(100.0);
    handle_wheel(ui, ctx, &resp, lane_x0);
    follow_playhead(ctx, lane_x0, area.max.x);

    let geo = Geo {
        lane_x0,
        pps: pixels_per_second(ctx.state.zoom),
        scroll_x: ctx.state.scroll_x,
    };
    let ruler = Rect::from_min_max(
        Pos2::new(lane_x0, area.min.y),
        Pos2::new(area.max.x, area.min.y + RULER_H),
    );
    let rows_top = ruler.bottom() + 6.0;
    let text_row = Rect::from_min_size(
        Pos2::new(lane_x0, rows_top),
        vec2(area.max.x - lane_x0, 34.0),
    );
    let main_row = Rect::from_min_size(
        Pos2::new(lane_x0, text_row.bottom() + 6.0),
        vec2(area.max.x - lane_x0, MAIN_ROW_H),
    );
    let audio_row = Rect::from_min_size(
        Pos2::new(lane_x0, main_row.bottom() + 6.0),
        vec2(area.max.x - lane_x0, 40.0),
    );

    handle_pointer(ui, ctx, &resp, geo, ruler, main_row);
    handle_media_drop(ui, ctx, &resp, geo, main_row);

    // ---- drawing ----
    let painter = ui.painter_at(area);
    let header = Rect::from_min_max(area.min, Pos2::new(lane_x0, area.max.y));
    painter.rect_filled(header, CornerRadius::ZERO, Color32::from_rgb(17, 17, 17));
    painter.rect_filled(ruler, CornerRadius::ZERO, Color32::from_rgb(14, 14, 14));
    painter.hline(
        area.x_range(),
        ruler.bottom(),
        Stroke::new(1.0, theme::BORDER),
    );
    draw_ruler(&painter, ruler, geo, ctx);

    for (name, row) in [("Text", text_row), ("Main", main_row), ("Audio", audio_row)] {
        let lane_painter = painter.with_clip_rect(Rect::from_min_max(
            Pos2::new(lane_x0, row.top()),
            Pos2::new(area.max.x, row.bottom() + 6.0),
        ));
        lane_painter.rect_filled(row, CornerRadius::ZERO, Color32::from_rgb(19, 19, 19));
        draw_header(&painter, name, header, row);
    }
    painter.vline(lane_x0, area.y_range(), Stroke::new(1.0, theme::BORDER));

    let lane_painter = painter.with_clip_rect(Rect::from_min_max(
        Pos2::new(lane_x0, ruler.bottom()),
        area.max,
    ));
    draw_clips(&lane_painter, ctx, geo, main_row);
    if ctx.doc.project.timeline().main_track().clips.is_empty() && ctx.state.drag.is_none() {
        lane_painter.text(
            Pos2::new((lane_x0 + area.max.x) / 2.0, main_row.center().y),
            Align2::CENTER_CENTER,
            "Drag material here and start to create",
            FontId::proportional(14.0),
            Color32::from_rgb(110, 110, 110),
        );
    }
    draw_drag_feedback(&lane_painter, ctx, geo, main_row);

    // playhead on top of everything
    let px = geo.x(ctx.state.playhead);
    if px >= lane_x0 && px <= area.max.x {
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

fn draw_header(painter: &egui::Painter, name: &str, header: Rect, row: Rect) {
    let head = Rect::from_min_max(
        Pos2::new(header.left(), row.top()),
        Pos2::new(header.right(), row.bottom()),
    );
    painter.text(
        Pos2::new(head.left() + 12.0, head.center().y - 8.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(12.0),
        theme::TEXT,
    );
    // lock / eye / speaker placeholders (they become real in Phase 2)
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
            StrokeKind::Inside,
        );
        painter.text(
            cover.center(),
            Align2::CENTER_CENTER,
            "Cover",
            FontId::proportional(11.0),
            theme::MUTED,
        );
    }
}

fn draw_ruler(painter: &egui::Painter, ruler: Rect, geo: Geo, ctx: &Ctx<'_>) {
    let rate = ctx.doc.project.frame_rate;
    let painter = painter.with_clip_rect(ruler);
    // the first "nice" step that leaves at least 90 px between labels
    let steps = [
        0.5f32, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
    ];
    let step = steps
        .into_iter()
        .find(|s| s * geo.pps >= 90.0)
        .unwrap_or(3600.0);
    let first = (geo.scroll_x / (step * geo.pps)).floor().max(0.0) as i64;
    let mut n = first;
    loop {
        let t = n as f32 * step;
        let x = geo.lane_x0 + t * geo.pps - geo.scroll_x;
        if x > ruler.right() {
            break;
        }
        painter.vline(
            x,
            (ruler.bottom() - 9.0)..=ruler.bottom(),
            Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
        );
        painter.text(
            Pos2::new(x + 4.0, ruler.top() + 8.0),
            Align2::LEFT_CENTER,
            rate.timecode((f64::from(t) * US_PER_SEC as f64) as Micros),
            FontId::monospace(10.0),
            theme::MUTED,
        );
        for k in 1..5 {
            let mx = x + step * geo.pps * k as f32 / 5.0;
            painter.vline(
                mx,
                (ruler.bottom() - 4.0)..=ruler.bottom(),
                Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
            );
        }
        n += 1;
    }
}

// ---- clips -------------------------------------------------------------------------------------------------------

fn clip_rect(clip: &Clip, geo: Geo, row: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(geo.x(clip.start), row.top() + 2.0),
        Pos2::new(geo.x(clip.end()), row.bottom() - 2.0),
    )
}

fn draw_clips(painter: &egui::Painter, ctx: &Ctx<'_>, geo: Geo, row: Rect) {
    let dragged = match &ctx.state.drag {
        Some(TimelineDrag::Move { clip, .. }) => Some(*clip),
        _ => None,
    };
    for clip in &ctx.doc.project.timeline().main_track().clips {
        let rect = clip_rect(clip, geo, row);
        if rect.right() < painter.clip_rect().left() || rect.left() > painter.clip_rect().right() {
            continue;
        }
        let selected = ctx.state.selection.contains(&clip.id);
        draw_clip(painter, ctx, clip, rect, selected, dragged == Some(clip.id));
    }
}

fn draw_clip(
    painter: &egui::Painter,
    ctx: &Ctx<'_>,
    clip: &Clip,
    rect: Rect,
    selected: bool,
    faded: bool,
) {
    let asset = ctx.doc.project.asset(clip.media);
    let missing = ctx.missing.contains(&clip.media);
    let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let round = CornerRadius::same(5);
    clipped.rect_filled(rect, round, Color32::from_rgb(34, 34, 38));

    // thumbnails: the poster picture repeated along the clip
    if let Some(texture) = ctx.thumbs.get(&clip.media) {
        let size = texture.size_vec2();
        let tile_w = (rect.height() * size.x / size.y).max(24.0);
        let mut x = rect.left();
        let tint = if faded {
            Color32::from_white_alpha(70)
        } else {
            Color32::from_white_alpha(235)
        };
        while x < rect.right() {
            let tile = Rect::from_min_size(Pos2::new(x, rect.top()), vec2(tile_w, rect.height()));
            clipped.image(
                texture.id(),
                tile,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
            x += tile_w;
        }
    }
    if missing {
        clipped.rect_filled(
            rect,
            round,
            Color32::from_rgba_unmultiplied(170, 20, 20, 140),
        );
    }

    // label
    let label = asset.map_or_else(|| "?".to_string(), |a| a.name.clone());
    let duration = if asset.is_some_and(|a| a.kind == MediaKind::Audio) {
        String::new()
    } else {
        short_duration(clip.duration)
    };
    let text = if missing {
        format!("⚠ {label} — file missing")
    } else {
        format!("{label}   {duration}")
    };
    let galley = clipped.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
    let tag = Rect::from_min_size(rect.min + vec2(2.0, 2.0), galley.size() + vec2(10.0, 4.0));
    clipped.rect_filled(tag, round, Color32::from_black_alpha(165));
    clipped.galley(tag.min + vec2(5.0, 2.0), galley, Color32::WHITE);

    let stroke = if selected {
        Stroke::new(2.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, Color32::from_rgb(70, 70, 76))
    };
    painter.rect_stroke(rect, round, stroke, StrokeKind::Inside);
    if selected {
        // edge handles show that the clip can be trimmed
        for x in [rect.left() + 2.0, rect.right() - 6.0] {
            let handle = Rect::from_min_size(
                Pos2::new(x, rect.top() + 2.0),
                vec2(4.0, rect.height() - 4.0),
            );
            painter.rect_filled(handle, CornerRadius::same(2), theme::ACCENT);
        }
    }
}

fn draw_drag_feedback(painter: &egui::Painter, ctx: &Ctx<'_>, geo: Geo, row: Rect) {
    let marker = |t: Micros| {
        let x = geo.x(t);
        painter.vline(x, row.y_range(), Stroke::new(3.0, theme::ACCENT));
    };
    let clips = &ctx.doc.project.timeline().main_track().clips;
    match &ctx.state.drag {
        Some(TimelineDrag::Move {
            clip,
            orig,
            grab_dx,
            to_index,
        }) => {
            // where it would land
            let before: Micros = clips
                .iter()
                .filter(|c| c.id != *clip)
                .take(*to_index)
                .map(|c| c.duration)
                .sum();
            marker(before);
            // the clip following the mouse
            if let Some(pos) = painter.ctx().pointer_interact_pos() {
                let width = (orig.duration as f64 / US_PER_SEC as f64) as f32 * geo.pps;
                let ghost = Rect::from_min_size(
                    Pos2::new(pos.x - grab_dx, row.top() + 2.0),
                    vec2(width, row.height() - 4.0),
                );
                painter.rect_filled(
                    ghost,
                    CornerRadius::same(5),
                    Color32::from_rgba_unmultiplied(252, 198, 16, 70),
                );
                painter.rect_stroke(
                    ghost,
                    CornerRadius::same(5),
                    Stroke::new(2.0, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }
        }
        _ => {
            // a media tile is being dragged over the timeline
            if let Some(_media) = egui::DragAndDrop::payload::<MediaId>(painter.ctx())
                && let Some(pos) = painter.ctx().pointer_hover_pos()
                && row.expand2(vec2(2000.0, 14.0)).contains(pos)
            {
                let index = vz_core::edit::index_for_time(clips, geo.time(pos.x));
                let t = clips
                    .get(index)
                    .map_or_else(|| clips.last().map_or(0, Clip::end), |c| c.start);
                marker(t);
            }
        }
    }
}

// ---- mouse and wheel -------------------------------------------------------------------------------------------

fn handle_wheel(ui: &egui::Ui, ctx: &mut Ctx<'_>, resp: &egui::Response, lane_x0: f32) {
    if !resp.hovered() {
        return;
    }
    let (delta, command, pointer) = ui.input(|i| {
        (
            i.smooth_scroll_delta,
            i.modifiers.command,
            i.pointer.hover_pos(),
        )
    });
    if delta == egui::Vec2::ZERO {
        return;
    }
    if command {
        // zoom, keeping the time under the mouse where it is
        let anchor_x = pointer.map_or(lane_x0 + 100.0, |p| p.x);
        let old = Geo {
            lane_x0,
            pps: pixels_per_second(ctx.state.zoom),
            scroll_x: ctx.state.scroll_x,
        };
        let t = old.time(anchor_x);
        ctx.state.zoom = (ctx.state.zoom + delta.y * 0.002).clamp(0.0, 1.0);
        let new_pps = pixels_per_second(ctx.state.zoom);
        let new_x = (t as f64 / US_PER_SEC as f64) as f32 * new_pps;
        ctx.state.scroll_x = (new_x - (anchor_x - lane_x0)).max(0.0);
    } else {
        // the mouse wheel (or shift + wheel, or a sideways swipe) scrolls the timeline sideways
        let amount = delta.x + delta.y;
        ctx.state.scroll_x = (ctx.state.scroll_x - amount).max(0.0);
    }
}

/// While playing, keep the playhead on screen.
fn follow_playhead(ctx: &mut Ctx<'_>, lane_x0: f32, right: f32) {
    if !ctx.state.playing {
        return;
    }
    let pps = pixels_per_second(ctx.state.zoom);
    let x =
        lane_x0 + (ctx.state.playhead as f64 / US_PER_SEC as f64) as f32 * pps - ctx.state.scroll_x;
    if x > right - 20.0 || x < lane_x0 {
        ctx.state.scroll_x = ((ctx.state.playhead as f64 / US_PER_SEC as f64) as f32 * pps
            - (right - lane_x0) * 0.2)
            .max(0.0);
    }
}

fn clip_at_x(ctx: &Ctx<'_>, geo: Geo, row: Rect, pos: Pos2) -> Option<(Clip, Rect)> {
    if pos.y < row.top() || pos.y > row.bottom() || pos.x < geo.lane_x0 {
        return None;
    }
    ctx.doc
        .project
        .timeline()
        .main_track()
        .clips
        .iter()
        .map(|c| (c.clone(), clip_rect(c, geo, row)))
        .find(|(_, r)| pos.x >= r.left() && pos.x <= r.right())
}

fn handle_pointer(
    ui: &egui::Ui,
    ctx: &mut Ctx<'_>,
    resp: &egui::Response,
    geo: Geo,
    ruler: Rect,
    main_row: Rect,
) {
    let hover = ui.input(|i| i.pointer.hover_pos());

    // cursor hints
    if ctx.state.drag.is_none()
        && let Some((_, r)) = hover.and_then(|p| clip_at_x(ctx, geo, main_row, p))
        && let Some(p) = hover
    {
        if (p.x - r.left()).abs() < EDGE_GRAB || (p.x - r.right()).abs() < EDGE_GRAB {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        } else {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    // a plain click
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        if pos.y <= ruler.bottom() && pos.x >= geo.lane_x0 {
            ctx.seek(geo.time(pos.x));
        } else if let Some((clip, _)) = clip_at_x(ctx, geo, main_row, pos) {
            let toggle = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            if toggle {
                if let Some(i) = ctx.state.selection.iter().position(|id| *id == clip.id) {
                    ctx.state.selection.remove(i);
                } else {
                    ctx.state.selection.push(clip.id);
                }
            } else {
                ctx.state.selection = vec![clip.id];
            }
        } else if pos.x >= geo.lane_x0 {
            ctx.state.selection.clear();
        }
    }

    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        if origin.y <= ruler.bottom() && origin.x >= geo.lane_x0 {
            ctx.state.drag = Some(TimelineDrag::Playhead);
        } else if let Some((clip, rect)) = clip_at_x(ctx, geo, main_row, origin) {
            if !ctx.state.selection.contains(&clip.id) {
                ctx.state.selection = vec![clip.id];
            }
            let key = ctx.state.new_merge_key();
            ctx.state.drag = Some(if (origin.x - rect.left()).abs() < EDGE_GRAB {
                TimelineDrag::TrimLeft {
                    clip: clip.id,
                    orig: clip,
                    press_x: origin.x,
                    key,
                }
            } else if (origin.x - rect.right()).abs() < EDGE_GRAB {
                TimelineDrag::TrimRight {
                    clip: clip.id,
                    orig: clip,
                    press_x: origin.x,
                    key,
                }
            } else {
                let index = ctx
                    .doc
                    .project
                    .timeline()
                    .main_track()
                    .clips
                    .iter()
                    .position(|c| c.id == clip.id)
                    .unwrap_or(0);
                TimelineDrag::Move {
                    clip: clip.id,
                    grab_dx: origin.x - rect.left(),
                    orig: clip,
                    to_index: index,
                }
            });
        }
    }

    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some(drag) = ctx.state.drag.clone()
    {
        drag_to(ctx, drag, geo, pos);
    }

    if resp.drag_stopped() {
        if let Some(TimelineDrag::Move { clip, to_index, .. }) = ctx.state.drag.clone() {
            let current = ctx
                .doc
                .project
                .timeline()
                .main_track()
                .clips
                .iter()
                .position(|c| c.id == clip);
            if current != Some(to_index)
                && let Err(e) = ctx.doc.move_clip(clip, to_index)
            {
                ctx.state.say(format!("Could not move the clip: {e}"));
            }
        }
        ctx.doc.seal_history();
        ctx.state.drag = None;
    }
}

/// One step of a drag, with the mouse now at `pos`.
fn drag_to(ctx: &mut Ctx<'_>, drag: TimelineDrag, geo: Geo, pos: Pos2) {
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    match drag {
        TimelineDrag::Playhead => ctx.seek(geo.time(pos.x)),
        TimelineDrag::Move {
            clip,
            orig,
            grab_dx,
            ..
        } => {
            let centre = geo.time(pos.x - grab_dx) + orig.duration / 2;
            let clips = &ctx.doc.project.timeline().main_track().clips;
            let to_index = clips
                .iter()
                .filter(|c| c.id != clip && c.start + c.duration / 2 < centre)
                .count();
            if let Some(TimelineDrag::Move { to_index: t, .. }) = &mut ctx.state.drag {
                *t = to_index;
            }
        }
        TimelineDrag::TrimLeft {
            clip,
            orig,
            press_x,
            key,
        } => {
            let delta = geo
                .span(pos.x - press_x)
                .clamp(-orig.source_in, orig.duration - frame);
            let Some(now) = ctx.doc.project.timeline().main_track().clip(clip).cloned() else {
                return;
            };
            let step = delta - (now.source_in - orig.source_in);
            if step != 0 {
                let _ = ctx.doc.trim_left(clip, step, Some(key));
            }
        }
        TimelineDrag::TrimRight {
            clip,
            orig,
            press_x,
            key,
        } => {
            let limit = ctx
                .doc
                .project
                .asset(orig.media)
                .and_then(vz_core::MediaAsset::max_clip_duration)
                .map_or(Micros::MAX / 4, |len| len - orig.source_in);
            let target = (orig.duration + geo.span(pos.x - press_x)).clamp(frame, limit.max(frame));
            let _ = ctx.doc.trim_right(clip, target, Some(key));
        }
    }
}

/// A media tile dropped onto the timeline.
fn handle_media_drop(
    ui: &egui::Ui,
    ctx: &mut Ctx<'_>,
    resp: &egui::Response,
    geo: Geo,
    main_row: Rect,
) {
    let egui_ctx = ui.ctx();
    let Some(media) = egui::DragAndDrop::payload::<MediaId>(egui_ctx) else {
        return;
    };
    let Some(pos) = egui_ctx.pointer_hover_pos() else {
        return;
    };
    let over = resp.rect.contains(pos) && pos.y >= main_row.top() - 40.0;
    if over && egui_ctx.input(|i| i.pointer.any_released()) {
        let clips = &ctx.doc.project.timeline().main_track().clips;
        let index = vz_core::edit::index_for_time(clips, geo.time(pos.x));
        match ctx.doc.add_clip(*media, index) {
            Ok(id) => ctx.state.selection = vec![id],
            Err(e) => ctx.state.say(format!("Could not add the clip: {e}")),
        }
        egui::DragAndDrop::clear_payload(egui_ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_range_is_sensible() {
        assert!((pixels_per_second(0.0) - 20.0).abs() < 0.01);
        assert!((pixels_per_second(1.0) - 400.0).abs() < 0.01);
        assert!(pixels_per_second(0.5) > pixels_per_second(0.2));
        // out-of-range values are clamped
        assert_eq!(pixels_per_second(-3.0), pixels_per_second(0.0));
        assert_eq!(pixels_per_second(9.0), pixels_per_second(1.0));
    }

    #[test]
    fn zoom_for_is_the_inverse() {
        for z in [0.0, 0.25, 0.5, 0.9] {
            assert!((zoom_for(pixels_per_second(z)) - z).abs() < 0.001);
        }
    }

    #[test]
    fn geometry_converts_both_ways() {
        let geo = Geo {
            lane_x0: 100.0,
            pps: 50.0,
            scroll_x: 25.0,
        };
        assert_eq!(geo.x(0), 75.0);
        assert_eq!(geo.time(geo.x(3 * US_PER_SEC)), 3 * US_PER_SEC);
        assert_eq!(geo.span(50.0), US_PER_SEC);
    }
}
