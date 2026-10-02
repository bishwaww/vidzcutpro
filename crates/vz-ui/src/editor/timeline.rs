//! The timeline: tabs, toolbar, track headers, ruler and every track with its clips.

mod draw;
mod input;
pub(crate) mod layout;
#[cfg(test)]
mod tests;

use super::Ctx;
use crate::theme;
use eframe::egui::{self, Align, Layout, Pos2, Rect, RichText, Sense, Stroke};
use layout::{Frame, Geo, HEADER_W, RULER_H, build_rows};
use vz_core::{Clip, ClipId, Micros, TrackId, TrackKind, US_PER_SEC};

const COMING_SOON: &str = "Coming soon";

/// Where a dragged clip would land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveTarget {
    pub track: TrackId,
    pub start: Micros,
}

/// What the mouse is doing to the timeline right now.
#[derive(Clone, Debug)]
pub enum TimelineDrag {
    /// Dragging the playhead along the ruler.
    Playhead,
    /// Dragging a clip to a new place (and maybe another track). Nothing changes until the mouse is released.
    Move {
        clip: ClipId,
        orig: Clip,
        from: TrackId,
        grab_dx: f32,
        target: Option<MoveTarget>,
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
    /// Dragging a rubber band over empty space to select clips.
    BoxSelect {
        origin: Pos2,
        additive: bool,
        base: Vec<ClipId>,
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

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    // forget selected clips that no longer exist (after undo, delete, ...)
    let tl = ctx.doc.project.timeline();
    ctx.state.selection.retain(|id| tl.clip(*id).is_some());

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
    let can_cut = !targets_at_playhead(ctx).is_empty();
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
        if tool(
            ui,
            "Split",
            can_cut,
            "Split the selected clips (or the main clip) at the playhead (Ctrl+B)",
        ) {
            split_at_playhead(ctx);
        }
        if tool(
            ui,
            "Delete left",
            can_cut,
            "Delete everything left of the playhead in the selected clips (Q)",
        ) {
            trim_to_playhead(ctx, Side::Left);
        }
        if tool(
            ui,
            "Delete right",
            can_cut,
            "Delete everything right of the playhead in the selected clips (W)",
        ) {
            trim_to_playhead(ctx, Side::Right);
        }
        if tool(
            ui,
            "Delete",
            can_delete,
            "Delete the selected clips (Delete). The main track closes the gap.",
        ) {
            delete_selection(ctx);
        }
        tool(ui, "Marker", false, COMING_SOON);
        tool(ui, "Freeze", false, COMING_SOON);
        ui.menu_button(RichText::new("+ Track").small(), |ui| {
            for (kind, label) in [
                (TrackKind::Overlay, "Overlay track"),
                (TrackKind::Text, "Text track"),
                (TrackKind::Audio, "Audio track"),
            ] {
                if ui.button(label).clicked() {
                    if let Err(e) = ctx.doc.add_track(kind) {
                        ctx.state.say(format!("Could not add the track: {e}"));
                    }
                    ui.close();
                }
            }
        });
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

/// The clips that Split, Q and W act on: the selected clips the playhead cuts through, or, when
/// nothing is selected, the main-track clip under the playhead.
pub fn targets_at_playhead(ctx: &Ctx<'_>) -> Vec<ClipId> {
    let t = ctx.state.playhead;
    let tl = ctx.doc.project.timeline();
    let selected: Vec<ClipId> = ctx
        .state
        .selection
        .iter()
        .copied()
        .filter(|id| tl.clip(*id).is_some_and(|c| c.start < t && t < c.end()))
        .collect();
    if !selected.is_empty() || !ctx.state.selection.is_empty() {
        return selected;
    }
    tl.main_track()
        .clip_at(t)
        .filter(|c| c.start < t)
        .map(|c| c.id)
        .into_iter()
        .collect()
}

pub fn split_at_playhead(ctx: &mut Ctx<'_>) {
    let targets = targets_at_playhead(ctx);
    match ctx.doc.split_clips(&targets, ctx.state.playhead) {
        Ok(created) => ctx.state.selection = created,
        Err(_) => ctx
            .state
            .say("To split, put the playhead inside a clip (not at its very edge) and select it."),
    }
}

/// Q / W: remove everything left or right of the playhead in those clips (one undo step).
pub fn trim_to_playhead(ctx: &mut Ctx<'_>, side: Side) {
    let targets = targets_at_playhead(ctx);
    let t = ctx.state.playhead;
    match ctx.doc.cut_clips_at(&targets, t, side == Side::Left) {
        Ok(()) => {}
        Err(_) => ctx
            .state
            .say("Put the playhead inside a clip (not at its very edge) and select it."),
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
    input::wheel(ui, ctx, &resp, lane_x0, area);
    input::follow_playhead(ctx, lane_x0, area.max.x);

    let (rows, content_height) = build_rows(ctx.doc.project.timeline());
    let view_height = (area.height() - RULER_H).max(1.0);
    ctx.state.scroll_y = ctx
        .state
        .scroll_y
        .clamp(0.0, (content_height - view_height).max(0.0));
    let frame = Frame {
        area,
        ruler: Rect::from_min_max(
            Pos2::new(lane_x0, area.min.y),
            Pos2::new(area.max.x, area.min.y + RULER_H),
        ),
        geo: Geo {
            lane_x0,
            pps: pixels_per_second(ctx.state.zoom),
            scroll_x: ctx.state.scroll_x,
        },
        origin_y: area.min.y + RULER_H - ctx.state.scroll_y,
        rows,
    };

    input::pointer(ui, ctx, &resp, &frame);
    input::media_drop(ui, ctx, &resp, &frame);
    draw::all(ui, ctx, &frame);
    input::header_buttons(ui, ctx, &frame);
}
