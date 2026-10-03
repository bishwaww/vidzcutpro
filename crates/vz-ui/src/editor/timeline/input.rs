//! Mouse and keyboard handling for the timeline: clicking, dragging, trimming, dropping, scrolling.

use super::draw::{Switch, clip_rect, switch_positions};
use super::layout::{EDGE_GRAB, Frame, Geo, Zone};
use super::{Ctx, MoveTarget, TimelineDrag, pixels_per_second};
use eframe::egui::{self, Pos2, Rect, Sense};
use vz_core::{Clip, ClipId, Micros, TrackId, TrackKind, US_PER_SEC, track_accepts};

/// How close (in points) an edge must be to a clip edge, the playhead or the start to stick to it.
const SNAP_DISTANCE: f32 = 8.0;

pub fn wheel(ui: &egui::Ui, ctx: &mut Ctx<'_>, resp: &egui::Response, lane_x0: f32, area: Rect) {
    if !resp.hovered() {
        return;
    }
    let (delta, command, alt, pointer) = ui.input(|i| {
        (
            i.smooth_scroll_delta,
            i.modifiers.command,
            i.modifiers.alt,
            i.pointer.hover_pos(),
        )
    });
    if delta == egui::Vec2::ZERO {
        return;
    }
    let over_headers = pointer.is_some_and(|p| p.x < lane_x0 && p.x >= area.min.x);
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
    } else if alt || over_headers {
        // up and down through tracks (the scroll is limited to the real height when the timeline is drawn)
        ctx.state.scroll_y = (ctx.state.scroll_y - delta.y - delta.x).max(0.0);
    } else {
        // the mouse wheel (or shift + wheel, or a sideways swipe) scrolls the timeline sideways
        ctx.state.scroll_x = (ctx.state.scroll_x - (delta.x + delta.y)).max(0.0);
    }
}

/// While playing, keep the playhead on screen.
pub fn follow_playhead(ctx: &mut Ctx<'_>, lane_x0: f32, right: f32) {
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

/// The clip under a screen position, with the track it is on and where it is drawn.
pub(super) fn clip_at(ctx: &Ctx<'_>, frame: &Frame, pos: Pos2) -> Option<(TrackId, Clip, Rect)> {
    if pos.x < frame.geo.lane_x0 || pos.y < frame.ruler.bottom() {
        return None;
    }
    let row = frame.row_at(pos.y)?;
    let rect = frame.row_rect(row);
    let track = ctx.doc.project.timeline().track(row.track)?;
    track
        .clips
        .iter()
        .map(|c| (c.clone(), clip_rect(c, frame.geo, rect)))
        .find(|(_, r)| pos.x >= r.left() && pos.x <= r.right())
        .map(|(c, r)| (row.track, c, r))
}

fn track_locked(ctx: &Ctx<'_>, track: TrackId) -> bool {
    ctx.doc
        .project
        .timeline()
        .track(track)
        .is_some_and(|t| t.locked)
}

pub fn pointer(ui: &egui::Ui, ctx: &mut Ctx<'_>, resp: &egui::Response, frame: &Frame) {
    let hover = ui.input(|i| i.pointer.hover_pos());

    // cursor hints
    if ctx.state.drag.is_none()
        && let Some(p) = hover
        && let Some((_, _, r)) = clip_at(ctx, frame, p)
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
        let toggle = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        if pos.y <= frame.ruler.bottom() && pos.x >= frame.geo.lane_x0 {
            seek(ctx, frame.geo.time(pos.x));
        } else if let Some((_, clip, _)) = clip_at(ctx, frame, pos) {
            if toggle {
                if let Some(i) = ctx.state.selection.iter().position(|id| *id == clip.id) {
                    ctx.state.selection.remove(i);
                } else {
                    ctx.state.selection.push(clip.id);
                }
            } else {
                ctx.state.selection = vec![clip.id];
            }
        } else if pos.x >= frame.geo.lane_x0 && !toggle {
            ctx.state.selection.clear();
        }
    }

    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        start_drag(ui, ctx, frame, origin);
    }

    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some(drag) = ctx.state.drag.clone()
    {
        drag_to(ctx, drag, frame, pos);
    }

    if resp.drag_stopped() {
        let release = ui
            .input(|i| i.pointer.hover_pos())
            .or(resp.interact_pointer_pos());
        finish_drag(ctx, frame, release);
    }
}

fn seek(ctx: &mut Ctx<'_>, t: Micros) {
    ctx.seek(t);
}

fn start_drag(ui: &egui::Ui, ctx: &mut Ctx<'_>, frame: &Frame, origin: Pos2) {
    if origin.y <= frame.ruler.bottom() && origin.x >= frame.geo.lane_x0 {
        ctx.state.drag = Some(TimelineDrag::Playhead);
        return;
    }
    if let Some((track, clip, rect)) = clip_at(ctx, frame, origin) {
        if !ctx.state.selection.contains(&clip.id) {
            ctx.state.selection = vec![clip.id];
        }
        if track_locked(ctx, track) {
            return; // the clip is selected, but a locked track cannot be edited
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
            TimelineDrag::Move {
                clip: clip.id,
                grab_dx: origin.x - rect.left(),
                orig: clip,
                from: track,
                target: None,
            }
        });
        return;
    }
    if origin.x >= frame.geo.lane_x0 && origin.y > frame.ruler.bottom() {
        let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        let base = if additive {
            ctx.state.selection.clone()
        } else {
            Vec::new()
        };
        if !additive {
            ctx.state.selection.clear();
        }
        ctx.state.drag = Some(TimelineDrag::BoxSelect {
            origin,
            additive,
            base,
        });
    }
}

/// The nearest time worth sticking to (the start, the playhead, another clip's edge), if one is close enough.
fn snap_candidates(ctx: &Ctx<'_>, exclude: &[ClipId]) -> Vec<Micros> {
    let mut points = vec![0, ctx.state.playhead];
    points.extend(ctx.doc.project.timeline().markers.iter().map(|m| m.time));
    for track in &ctx.doc.project.timeline().tracks {
        for c in track.clips.iter().filter(|c| !exclude.contains(&c.id)) {
            points.push(c.start);
            points.push(c.end());
        }
    }
    points
}

/// Stick `t` to something nearby (within a few points on screen). Returns the time to use and what it stuck to.
fn snap(ctx: &Ctx<'_>, geo: Geo, t: Micros, exclude: &[ClipId]) -> (Micros, Option<Micros>) {
    if !ctx.state.snapping {
        return (t, None);
    }
    let limit = geo.span(SNAP_DISTANCE);
    snap_candidates(ctx, exclude)
        .into_iter()
        .map(|p| (p, (p - t).abs()))
        .filter(|(_, d)| *d <= limit)
        .min_by_key(|(_, d)| *d)
        .map_or((t, None), |(p, _)| (p, Some(p)))
}

/// One step of a drag, with the mouse now at `pos`.
pub(super) fn drag_to(ctx: &mut Ctx<'_>, drag: TimelineDrag, frame: &Frame, pos: Pos2) {
    let geo = frame.geo;
    let rate = ctx.doc.project.frame_rate;
    let frame_len = rate.frame_start(1);
    ctx.state.snap_guide = None;
    match drag {
        TimelineDrag::Playhead => ctx.seek(geo.time(pos.x)),
        TimelineDrag::Move {
            clip,
            orig,
            from,
            grab_dx,
            ..
        } => {
            let mut target = None;
            if pos.x >= geo.lane_x0
                && let Some(row) = frame.row_at(pos.y)
                && track_accepts(&ctx.doc.project, row.kind, &orig)
            {
                let wanted = geo.time(pos.x - grab_dx);
                let selected = ctx.state.selection.clone();
                // stick the clip's start or its end to something nearby
                let (by_start, guide_start) = snap(ctx, geo, wanted, &selected);
                let (by_end, guide_end) = snap(ctx, geo, wanted + orig.duration, &selected);
                let (start, guide) = match (guide_start, guide_end) {
                    (Some(_), Some(_))
                        if (by_start - wanted).abs() <= (by_end - orig.duration - wanted).abs() =>
                    {
                        (by_start, guide_start)
                    }
                    (Some(_), None) => (by_start, guide_start),
                    (_, Some(_)) => (by_end - orig.duration, guide_end),
                    _ => (wanted, None),
                };
                ctx.state.snap_guide = guide;
                target = Some(MoveTarget {
                    track: row.track,
                    start: rate.snap(start.max(0)),
                });
            }
            ctx.state.drag = Some(TimelineDrag::Move {
                clip,
                orig,
                from,
                grab_dx,
                target,
            });
        }
        TimelineDrag::TrimLeft {
            clip,
            orig,
            press_x,
            key,
        } => {
            let main = ctx
                .doc
                .project
                .timeline()
                .track_of(clip)
                .is_some_and(|t| t.kind == TrackKind::Main);
            let mut delta = geo.span(pos.x - press_x);
            if !main {
                // the left edge moves: stick it to something nearby
                let (edge, guide) = snap(ctx, geo, orig.start + delta, &[clip]);
                delta = edge - orig.start;
                ctx.state.snap_guide = guide;
            }
            // how far the edge may go left: not before the start of the file, and not before the start of the timeline
            let lowest = if main {
                -orig.source_in
            } else if orig.is_text() {
                -orig.start
            } else {
                -orig.source_in.min(orig.start)
            };
            let delta = delta.clamp(lowest, orig.duration - frame_len);
            let Some(now) = ctx.doc.project.timeline().clip(clip).cloned() else {
                return;
            };
            // how much of the drag is already applied (the main track keeps its start; other tracks move it)
            let applied = if main {
                now.source_in - orig.source_in
            } else {
                now.start - orig.start
            };
            let step = delta - applied;
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
            let limit = orig
                .media()
                .and_then(|m| ctx.doc.project.asset(m))
                .and_then(vz_core::MediaAsset::max_clip_duration)
                .map_or(Micros::MAX / 4, |len| len - orig.source_in);
            let mut target = orig.duration + geo.span(pos.x - press_x);
            let main = ctx
                .doc
                .project
                .timeline()
                .track_of(clip)
                .is_some_and(|t| t.kind == TrackKind::Main);
            if !main {
                let (end, guide) = snap(ctx, geo, orig.start + target, &[clip]);
                target = end - orig.start;
                ctx.state.snap_guide = guide;
            }
            let target = target.clamp(frame_len, limit.max(frame_len));
            let _ = ctx.doc.trim_right(clip, target, Some(key));
        }
        TimelineDrag::BoxSelect {
            origin,
            additive,
            base,
        } => {
            let band = Rect::from_two_pos(origin, pos);
            let mut selected = if additive { base.clone() } else { Vec::new() };
            for row in &frame.rows {
                let rect = frame.row_rect(row);
                if !rect.intersects(band) {
                    continue;
                }
                if let Some(track) = ctx.doc.project.timeline().track(row.track) {
                    for c in &track.clips {
                        if clip_rect(c, geo, rect).intersects(band) && !selected.contains(&c.id) {
                            selected.push(c.id);
                        }
                    }
                }
            }
            ctx.state.selection = selected;
        }
    }
}

/// The mouse was released: do what the drag was for.
pub(super) fn finish_drag(ctx: &mut Ctx<'_>, frame: &Frame, release: Option<Pos2>) {
    if let Some(TimelineDrag::Move {
        clip,
        orig,
        from,
        target,
        ..
    }) = ctx.state.drag.clone()
    {
        let zone = release.and_then(|p| frame.zone_at(p.y).filter(|_| p.x >= frame.geo.lane_x0));
        let result = match (target, zone) {
            (Some(target), _) => {
                let group: Vec<ClipId> = ctx
                    .state
                    .selection
                    .iter()
                    .copied()
                    .filter(|id| *id != clip)
                    .collect();
                if target.track == from && !group.is_empty() && !is_on_main(ctx, from) {
                    // several clips selected and dragged along their own track: they all move by the same amount
                    let mut ids = group;
                    ids.push(clip);
                    ctx.doc.shift_clips(&ids, target.start - orig.start, None)
                } else if target.track == from
                    && target.start == orig.start
                    && !is_on_main(ctx, from)
                {
                    Ok(())
                } else {
                    ctx.doc.move_clip_to(clip, target.track, target.start, None)
                }
            }
            (None, Some(zone)) => {
                let kind = if zone == Zone::Top {
                    TrackKind::Overlay
                } else {
                    TrackKind::Audio
                };
                let start = release.map_or(orig.start, |p| frame.geo.time(p.x));
                ctx.doc
                    .move_clip_to_new_track(clip, kind, start, None)
                    .map(|_| ())
            }
            (None, None) => Ok(()),
        };
        if let Err(e) = result {
            ctx.state.say(format!("Could not move the clip: {e}"));
        }
    }
    ctx.doc.seal_history();
    ctx.state.drag = None;
    ctx.state.snap_guide = None;
}

fn is_on_main(ctx: &Ctx<'_>, track: TrackId) -> bool {
    ctx.doc
        .project
        .timeline()
        .track(track)
        .is_some_and(|t| t.kind == TrackKind::Main)
}

/// A media tile dropped onto the timeline.
pub fn media_drop(ui: &egui::Ui, ctx: &mut Ctx<'_>, resp: &egui::Response, frame: &Frame) {
    let egui_ctx = ui.ctx();
    let Some(media) = egui::DragAndDrop::payload::<vz_core::MediaId>(egui_ctx) else {
        return;
    };
    let Some(pos) = egui_ctx.pointer_hover_pos() else {
        return;
    };
    if !resp.rect.contains(pos) || !egui_ctx.input(|i| i.pointer.any_released()) {
        return;
    }
    let start = frame.geo.time(pos.x.max(frame.geo.lane_x0));
    let result = if let Some(zone) = frame.zone_at(pos.y) {
        let wants_audio = ctx
            .doc
            .project
            .asset(*media)
            .is_some_and(|a| a.kind == vz_core::MediaKind::Audio);
        if (zone == Zone::Top && wants_audio) || (zone == Zone::Bottom && !wants_audio) {
            ctx.state
                .say("That kind of file does not go on that kind of track.");
            egui::DragAndDrop::clear_payload(egui_ctx);
            return;
        }
        ctx.doc
            .place_media_new_track(*media, start)
            .map(|(_, clip)| clip)
    } else if pos.x >= frame.geo.lane_x0
        && let Some(row) = frame.row_at(pos.y)
    {
        ctx.doc.place_media(row.track, *media, start)
    } else {
        egui::DragAndDrop::clear_payload(egui_ctx);
        return;
    };
    match result {
        Ok(id) => ctx.state.selection = vec![id],
        Err(vz_core::DocError::Edit(vz_core::EditError::WrongTrack)) => {
            ctx.state
                .say("That kind of file does not go on that kind of track.");
        }
        Err(e) => ctx.state.say(format!("Could not add the clip: {e}")),
    }
    egui::DragAndDrop::clear_payload(egui_ctx);
}

/// The lock / hide / mute switches and the right-click menu of each track header.
pub fn header_buttons(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, frame: &Frame) {
    for row in &frame.rows {
        let rect = frame.header_rect(row);
        if rect.bottom() < frame.ruler.bottom() || rect.top() > frame.area.max.y {
            continue;
        }
        for (switch, pos) in switch_positions(rect, row.kind, rect.right()) {
            let button =
                Rect::from_center_size(pos, egui::vec2(20.0, 20.0)).intersect(Rect::from_min_max(
                    Pos2::new(frame.area.min.x, frame.ruler.bottom()),
                    frame.area.max,
                ));
            let resp = ui.interact(
                button,
                ui.id().with(("switch", row.track, switch as u8)),
                Sense::click(),
            );
            let hint = match switch {
                Switch::Lock => "Lock the track: its clips cannot be changed",
                Switch::Hide => "Hide the track from the picture",
                Switch::Mute => "Mute the track's sound",
            };
            if resp.on_hover_text(hint).clicked() {
                let result = ctx.doc.set_track_flags(row.track, |f| match switch {
                    Switch::Lock => f.locked = !f.locked,
                    Switch::Hide => f.hidden = !f.hidden,
                    Switch::Mute => f.muted = !f.muted,
                });
                if let Err(e) = result {
                    ctx.state.say(format!("Could not change the track: {e}"));
                }
            }
        }
        let header = ui.interact(
            rect.intersect(Rect::from_min_max(
                Pos2::new(frame.area.min.x, frame.ruler.bottom()),
                frame.area.max,
            )),
            ui.id().with(("header", row.track)),
            Sense::click(),
        );
        header.context_menu(|ui| {
            if row.kind != TrackKind::Main && ui.button("Remove track").clicked() {
                if let Err(e) = ctx.doc.remove_track(row.track) {
                    ctx.state.say(format!("Could not remove the track: {e}"));
                }
                ui.close();
            }
            if row.kind == TrackKind::Main {
                ui.label("The main track cannot be removed");
            }
        });
    }
}
