//! Painting the timeline: headers, ruler, tracks, clips, drag feedback and the playhead.

use super::layout::{Frame, Geo, Row, Zone};
use super::{Ctx, TimelineDrag};
use crate::editor::assets::short_duration;
use crate::editor::keyframes::{self, Prop};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
    vec2,
};
use vz_core::{Clip, Content, MediaKind, Micros, TrackKind, US_PER_SEC};

const GREY: Color32 = Color32::from_rgb(110, 110, 110);

pub fn all(ui: &mut egui::Ui, ctx: &Ctx<'_>, frame: &Frame) {
    let painter = ui.painter_at(frame.area);
    let lane_clip = Rect::from_min_max(
        Pos2::new(frame.geo.lane_x0, frame.ruler.bottom()),
        frame.area.max,
    );
    let lanes = painter.with_clip_rect(lane_clip);

    // backgrounds
    painter.rect_filled(
        frame.area,
        CornerRadius::ZERO,
        Color32::from_rgb(17, 17, 17),
    );
    for row in &frame.rows {
        let rect = frame.row_rect(row);
        lanes.rect_filled(rect, CornerRadius::ZERO, lane_color(row.kind));
    }

    // headers (clipped so they never draw over the ruler)
    let header_clip = Rect::from_min_max(
        Pos2::new(frame.area.min.x, frame.ruler.bottom()),
        Pos2::new(frame.geo.lane_x0, frame.area.max.y),
    );
    let headers = painter.with_clip_rect(header_clip);
    headers.rect_filled(
        header_clip,
        CornerRadius::ZERO,
        Color32::from_rgb(17, 17, 17),
    );
    for row in &frame.rows {
        header(&headers, frame, row);
    }

    // clips
    for row in &frame.rows {
        let rect = frame.row_rect(row);
        if rect.bottom() < frame.ruler.bottom() || rect.top() > frame.area.max.y {
            continue;
        }
        let Some(track) = ctx.doc.project.timeline().track(row.track) else {
            continue;
        };
        for clip in &track.clips {
            let r = clip_rect(clip, frame.geo, rect);
            if r.right() < lane_clip.left() || r.left() > lane_clip.right() {
                continue;
            }
            let selected = ctx.state.selection.contains(&clip.id);
            let moving = matches!(&ctx.state.drag, Some(TimelineDrag::Move { clip: id, .. }) if *id == clip.id);
            clip_body(&lanes, ctx, clip, r, selected, moving, row);
        }
    }
    let no_clips = ctx
        .doc
        .project
        .timeline()
        .tracks
        .iter()
        .all(|t| t.clips.is_empty());
    if no_clips
        && ctx.state.drag.is_none()
        && let Some(main) = frame.rows.iter().find(|r| r.kind == TrackKind::Main)
    {
        let rect = frame.row_rect(main);
        lanes.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Drag material here and start to create",
            FontId::proportional(14.0),
            GREY,
        );
    }

    // transition badges on the cuts
    for (after, badge) in frame.transition_badges(ctx.doc.project.timeline()) {
        if badge.right() < lane_clip.left() || badge.left() > lane_clip.right() {
            continue;
        }
        let selected = ctx.state.selected_transition == Some(after);
        let (fill, fg) = if selected {
            (theme::ACCENT, theme::ON_ACCENT)
        } else {
            (Color32::from_rgb(40, 40, 40), theme::ACCENT)
        };
        lanes.circle_filled(badge.center(), 11.0, fill);
        lanes.circle_stroke(badge.center(), 11.0, Stroke::new(1.0, theme::ACCENT));
        lanes.text(
            badge.center(),
            Align2::CENTER_CENTER,
            theme::icons::BLEND,
            theme::icons::font(13.0),
            fg,
        );
    }

    drag_feedback(&lanes, ctx, frame);

    // ruler on top of the scrolled rows
    painter.rect_filled(
        frame.ruler,
        CornerRadius::ZERO,
        Color32::from_rgb(14, 14, 14),
    );
    painter.rect_filled(
        Rect::from_min_max(
            frame.area.min,
            Pos2::new(frame.geo.lane_x0, frame.ruler.bottom()),
        ),
        CornerRadius::ZERO,
        Color32::from_rgb(17, 17, 17),
    );
    painter.hline(
        frame.area.x_range(),
        frame.ruler.bottom(),
        Stroke::new(1.0, theme::BORDER),
    );
    painter.vline(
        frame.geo.lane_x0,
        frame.area.y_range(),
        Stroke::new(1.0, theme::BORDER),
    );
    ruler(&painter, frame, ctx);

    // playhead through everything
    let px = frame.geo.x(ctx.state.playhead);
    if px >= frame.geo.lane_x0 && px <= frame.area.max.x {
        painter.vline(
            px,
            frame.area.min.y..=frame.area.max.y,
            Stroke::new(1.5, theme::TEXT),
        );
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(px - 6.0, frame.area.min.y),
                Pos2::new(px + 6.0, frame.area.min.y),
                Pos2::new(px, frame.area.min.y + 10.0),
            ],
            theme::TEXT,
            Stroke::NONE,
        ));
    }
}

fn lane_color(kind: TrackKind) -> Color32 {
    match kind {
        TrackKind::Main => Color32::from_rgb(19, 19, 19),
        TrackKind::Overlay => Color32::from_rgb(22, 22, 25),
        TrackKind::Text => Color32::from_rgb(25, 22, 17),
        TrackKind::Audio => Color32::from_rgb(17, 22, 23),
    }
}

pub fn clip_rect(clip: &Clip, geo: Geo, row: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(geo.x(clip.start), row.top() + 2.0),
        Pos2::new(geo.x(clip.end()), row.bottom() - 2.0),
    )
}

// ---- headers ---------------------------------------------------------------------------------------------------

/// Which of the three switches a track header shows, and where (used by the painting and the clicking).
pub fn switch_positions(row_rect: Rect, kind: TrackKind, header_right: f32) -> Vec<(Switch, Pos2)> {
    let switches: &[Switch] = match kind {
        TrackKind::Main | TrackKind::Overlay => &[Switch::Lock, Switch::Hide, Switch::Mute],
        TrackKind::Text => &[Switch::Lock, Switch::Hide],
        TrackKind::Audio => &[Switch::Lock, Switch::Mute],
    };
    let tall = row_rect.height() >= 44.0;
    switches
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let pos = if tall {
                Pos2::new(
                    row_rect.left() + 20.0 + i as f32 * 24.0,
                    row_rect.bottom() - 14.0,
                )
            } else {
                Pos2::new(
                    header_right - 14.0 - (switches.len() - 1 - i) as f32 * 24.0,
                    row_rect.center().y,
                )
            };
            (*s, pos)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Lock,
    Hide,
    Mute,
}

fn header(painter: &Painter, frame: &Frame, row: &Row) {
    let rect = frame.header_rect(row);
    painter.text(
        Pos2::new(
            rect.left() + 12.0,
            if rect.height() >= 44.0 {
                rect.top() + 14.0
            } else {
                rect.center().y
            },
        ),
        Align2::LEFT_CENTER,
        &row.name,
        FontId::proportional(12.0),
        theme::TEXT,
    );
    for (switch, pos) in switch_positions(rect, row.kind, rect.right()) {
        let on = match switch {
            Switch::Lock => row.flags.locked,
            Switch::Hide => row.flags.hidden,
            Switch::Mute => row.flags.muted,
        };
        let color = if on {
            theme::ACCENT
        } else {
            Color32::from_rgb(120, 120, 120)
        };
        match switch {
            Switch::Lock => lock_icon(painter, pos, color),
            Switch::Hide => eye_icon(painter, pos, color, on),
            Switch::Mute => speaker_icon(painter, pos, color, on),
        }
    }
    if row.kind == TrackKind::Main {
        let cover = Rect::from_min_size(
            Pos2::new(rect.right() - 62.0, rect.top() + 10.0),
            vec2(52.0, rect.height() - 20.0),
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

fn lock_icon(p: &Painter, c: Pos2, color: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, 2.5), vec2(10.0, 7.0));
    p.rect_filled(body, CornerRadius::same(1), color);
    let shackle: Vec<Pos2> = (0..=8)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 - i as f32 / 8.0);
            c + vec2(a.cos() * 3.2, -1.0 - a.sin() * 3.6)
        })
        .collect();
    p.add(Shape::line(shackle, Stroke::new(1.4, color)));
}

fn eye_icon(p: &Painter, c: Pos2, color: Color32, closed: bool) {
    let outline: Vec<Pos2> = (0..24)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 24.0;
            c + vec2(a.cos() * 7.0, a.sin() * 4.0)
        })
        .collect();
    p.add(Shape::closed_line(outline, Stroke::new(1.3, color)));
    p.circle_filled(c, 2.0, color);
    if closed {
        p.line_segment(
            [c + vec2(-7.0, 5.0), c + vec2(7.0, -5.0)],
            Stroke::new(1.6, color),
        );
    }
}

fn speaker_icon(p: &Painter, c: Pos2, color: Color32, muted: bool) {
    p.add(Shape::convex_polygon(
        vec![
            c + vec2(-6.0, -2.5),
            c + vec2(-2.5, -2.5),
            c + vec2(1.0, -6.0),
            c + vec2(1.0, 6.0),
            c + vec2(-2.5, 2.5),
            c + vec2(-6.0, 2.5),
        ],
        color,
        Stroke::NONE,
    ));
    if muted {
        p.line_segment(
            [c + vec2(3.5, -4.0), c + vec2(8.0, 4.0)],
            Stroke::new(1.5, color),
        );
        p.line_segment(
            [c + vec2(8.0, -4.0), c + vec2(3.5, 4.0)],
            Stroke::new(1.5, color),
        );
    } else {
        for r in [3.0f32, 6.0] {
            let arc: Vec<Pos2> = (0..=6)
                .map(|i| {
                    let a = -0.9 + 1.8 * i as f32 / 6.0;
                    c + vec2(2.0 + a.cos() * r, a.sin() * r)
                })
                .collect();
            p.add(Shape::line(arc, Stroke::new(1.1, color)));
        }
    }
}

// ---- ruler ------------------------------------------------------------------------------------------------------

fn ruler(painter: &Painter, frame: &Frame, ctx: &Ctx<'_>) {
    let rate = ctx.doc.project.frame_rate;
    let geo = frame.geo;
    let painter = painter.with_clip_rect(frame.ruler);
    // the first "nice" step that leaves at least 90 px between labels
    let steps = [
        0.5f32, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
    ];
    let step = steps
        .into_iter()
        .find(|s| s * geo.pps >= 90.0)
        .unwrap_or(3600.0);
    let mut n = (geo.scroll_x / (step * geo.pps)).floor().max(0.0) as i64;
    loop {
        let t = n as f32 * step;
        let x = geo.lane_x0 + t * geo.pps - geo.scroll_x;
        if x > frame.ruler.right() {
            break;
        }
        painter.vline(
            x,
            (frame.ruler.bottom() - 9.0)..=frame.ruler.bottom(),
            Stroke::new(1.0, Color32::from_rgb(90, 90, 90)),
        );
        painter.text(
            Pos2::new(x + 4.0, frame.ruler.top() + 8.0),
            Align2::LEFT_CENTER,
            rate.timecode((f64::from(t) * US_PER_SEC as f64) as Micros),
            FontId::monospace(10.0),
            theme::MUTED,
        );
        for k in 1..5 {
            let mx = x + step * geo.pps * k as f32 / 5.0;
            painter.vline(
                mx,
                (frame.ruler.bottom() - 4.0)..=frame.ruler.bottom(),
                Stroke::new(1.0, Color32::from_rgb(60, 60, 60)),
            );
        }
        n += 1;
    }
    for m in &ctx.doc.project.timeline().markers {
        let x = geo.x(m.time);
        if x < frame.ruler.left() - 8.0 || x > frame.ruler.right() + 8.0 {
            continue;
        }
        let color = Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.color[3]);
        let top = frame.ruler.bottom() - 12.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x - 5.0, top),
                Pos2::new(x + 5.0, top),
                Pos2::new(x + 5.0, top + 6.0),
                Pos2::new(x, top + 11.0),
                Pos2::new(x - 5.0, top + 6.0),
            ],
            color,
            Stroke::NONE,
        ));
    }
}

// ---- clips ------------------------------------------------------------------------------------------------------

fn clip_body(
    painter: &Painter,
    ctx: &Ctx<'_>,
    clip: &Clip,
    rect: Rect,
    selected: bool,
    moving: bool,
    row: &Row,
) {
    let missing = clip.media().is_some_and(|m| ctx.missing.contains(&m));
    let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let round = CornerRadius::same(5);
    let (base, border, label) = match &clip.content {
        Content::Text(t) => (
            Color32::from_rgb(58, 46, 14),
            Color32::from_rgb(180, 140, 30),
            first_line(&t.text),
        ),
        Content::Shape(s) => (
            Color32::from_rgb(48, 30, 66),
            Color32::from_rgb(140, 90, 190),
            s.kind.label().to_string(),
        ),
        Content::Media(m) => {
            let asset = ctx.doc.project.asset(*m);
            let name = asset.map_or_else(|| "?".to_string(), |a| a.name.clone());
            if asset.is_some_and(|a| a.kind == MediaKind::Audio) {
                (
                    Color32::from_rgb(14, 52, 54),
                    Color32::from_rgb(40, 150, 150),
                    name,
                )
            } else {
                (
                    Color32::from_rgb(34, 34, 38),
                    Color32::from_rgb(70, 70, 76),
                    name,
                )
            }
        }
    };
    clipped.rect_filled(rect, round, base);

    // thumbnails: the poster picture repeated along the clip
    if let Some(texture) = clip.media().and_then(|m| ctx.thumbs.get(&m)) {
        let size = texture.size_vec2();
        let tile_w = (rect.height() * size.x / size.y).max(24.0);
        let tint = if moving {
            Color32::from_white_alpha(70)
        } else {
            Color32::from_white_alpha(235)
        };
        let mut x = rect.left();
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
    if let Some(wave) = clip.media().and_then(|m| ctx.waveforms.get(&m)) {
        waveform(&clipped, clip, wave, rect);
    }
    if missing {
        clipped.rect_filled(
            rect,
            round,
            Color32::from_rgba_unmultiplied(170, 20, 20, 140),
        );
    }
    if row.flags.hidden || row.flags.locked {
        clipped.rect_filled(rect, round, Color32::from_black_alpha(110));
    }

    // keyframes: small diamonds along the bottom edge of the selected clip
    if selected && clip.duration > 0 {
        let props = [
            Prop::X,
            Prop::Y,
            Prop::ScaleX,
            Prop::ScaleY,
            Prop::Rotation,
            Prop::Opacity,
            Prop::Volume,
        ];
        let tol = keyframes::tolerance(ctx.doc.project.frame_rate.frame_start(1));
        for t in keyframes::key_times(clip, &props, tol) {
            let x = rect.left() + rect.width() * (t as f32 / clip.duration as f32);
            let c = Pos2::new(x, rect.bottom() - 7.0);
            clipped.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x, c.y - 4.5),
                    Pos2::new(c.x + 4.5, c.y),
                    Pos2::new(c.x, c.y + 4.5),
                    Pos2::new(c.x - 4.5, c.y),
                ],
                theme::ACCENT,
                Stroke::new(1.0, Color32::BLACK),
            ));
        }
    }

    // label
    let duration = short_duration(clip.duration);
    let text = if missing {
        format!("⚠ {label} — file missing")
    } else {
        let mut badges = String::new();
        if clip.speed.is_animated() {
            badges.push_str("  ~curve");
        } else if clip.speed_at(0) == 0.0 && clip.media().is_some() {
            badges.push_str("  freeze");
        } else if (clip.speed_at(0) - 1.0).abs() > 0.001 {
            badges.push_str(&format!("  {:.2}×", clip.speed_at(0)).replace(".00×", "×"));
        }
        if clip.reverse {
            badges.push_str("  ⟲");
        }
        if clip.anim_in.is_some() || clip.anim_out.is_some() || clip.anim_combo.is_some() {
            badges.push_str("  ✦");
        }
        format!("{label}   {duration}{badges}")
    };
    let galley = clipped.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
    let tag = Rect::from_min_size(rect.min + vec2(2.0, 2.0), galley.size() + vec2(10.0, 4.0));
    clipped.rect_filled(tag, round, Color32::from_black_alpha(165));
    clipped.galley(tag.min + vec2(5.0, 2.0), galley, Color32::WHITE);

    let stroke = if selected {
        Stroke::new(2.0, theme::ACCENT)
    } else {
        Stroke::new(1.0, border)
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

/// The loudness shape of an audio clip, mirrored around the middle of the clip.
fn waveform(painter: &Painter, clip: &Clip, wave: &vz_audio::Waveform, rect: Rect) {
    if wave.peaks.is_empty() || rect.width() < 2.0 {
        return;
    }
    let visible = rect.intersect(painter.clip_rect());
    let per_pixel = clip.duration as f64 / f64::from(rect.width());
    let (mid, half) = (rect.center().y, rect.height() / 2.0 - 3.0);
    // drawn relative to the loudest part of the file, so quiet recordings are still readable
    let loudest = wave.loudest.max(0.05);
    let gain = clip.volume.value_at(0).clamp(0.0, 2.0) / loudest;
    let color = Color32::from_rgb(90, 210, 205);
    let mut x = visible.left().floor();
    while x < visible.right() {
        // the part of the file this pixel column stands for (speed and reverse decide where it is)
        let at =
            |x: f32| clip.source_time(clip.start + (f64::from(x - rect.left()) * per_pixel) as i64);
        let (a, b) = (at(x), at(x + 1.0));
        let peak = (wave.peak_between(a.min(b), a.max(b)) * gain).min(1.0);
        let h = (peak * half).max(0.5);
        painter.vline(x + 0.5, (mid - h)..=(mid + h), Stroke::new(1.0, color));
        x += 1.0;
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let shown: String = line.chars().take(40).collect();
    if shown.is_empty() {
        "Text".into()
    } else {
        shown
    }
}

// ---- drag feedback ------------------------------------------------------------------------------------------------

fn drag_feedback(painter: &Painter, ctx: &Ctx<'_>, frame: &Frame) {
    let marker = |rect: Rect, t: Micros| {
        let x = frame.geo.x(t);
        painter.vline(x, rect.y_range(), Stroke::new(3.0, theme::ACCENT));
    };
    let egui_ctx = painter.ctx().clone();
    let pointer = egui_ctx.pointer_interact_pos();

    if let Some(t) = ctx.state.snap_guide {
        let x = frame.geo.x(t);
        painter.vline(
            x,
            frame.ruler.bottom()..=frame.area.max.y,
            Stroke::new(1.5, Color32::from_rgb(255, 120, 200)),
        );
    }

    match &ctx.state.drag {
        Some(TimelineDrag::Move {
            clip,
            orig,
            grab_dx,
            target,
            ..
        }) => {
            let width = (orig.duration as f64 / US_PER_SEC as f64) as f32 * frame.geo.pps;
            if let Some(target) = target {
                if let Some(row) = frame.rows.iter().find(|r| r.track == target.track) {
                    let rect = frame.row_rect(row);
                    painter.rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        Color32::from_rgba_unmultiplied(252, 198, 16, 14),
                    );
                    if row.kind == TrackKind::Main {
                        // where it would land in the order
                        let others: Vec<&Clip> = ctx
                            .doc
                            .project
                            .timeline()
                            .main_track()
                            .clips
                            .iter()
                            .filter(|c| c.id != *clip)
                            .collect();
                        let before: Micros = others
                            .iter()
                            .take_while(|c| {
                                c.start + c.duration / 2 < target.start + orig.duration / 2
                            })
                            .map(|c| c.duration)
                            .sum();
                        marker(rect, before);
                    } else {
                        let ghost = Rect::from_min_size(
                            Pos2::new(frame.geo.x(target.start), rect.top() + 2.0),
                            vec2(width, rect.height() - 4.0),
                        );
                        painter.rect_filled(
                            ghost,
                            CornerRadius::same(5),
                            Color32::from_rgba_unmultiplied(252, 198, 16, 60),
                        );
                        painter.rect_stroke(
                            ghost,
                            CornerRadius::same(5),
                            Stroke::new(2.0, theme::ACCENT),
                            StrokeKind::Inside,
                        );
                    }
                }
            } else if let Some(pos) = pointer {
                // no valid place under the mouse
                let ghost = Rect::from_min_size(
                    Pos2::new(pos.x - grab_dx, pos.y - 14.0),
                    vec2(width.max(20.0), 28.0),
                );
                painter.rect_filled(
                    ghost,
                    CornerRadius::same(5),
                    Color32::from_rgba_unmultiplied(200, 50, 50, 90),
                );
            }
            zones(painter, frame, pointer, true);
        }
        Some(TimelineDrag::BoxSelect { origin, .. }) => {
            if let Some(pos) = pointer {
                let r = Rect::from_two_pos(*origin, pos);
                painter.rect_filled(
                    r,
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(252, 198, 16, 30),
                );
                painter.rect_stroke(
                    r,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }
        }
        _ => {
            // a media tile is being dragged over the timeline
            if egui::DragAndDrop::has_payload_of_type::<vz_core::MediaId>(&egui_ctx)
                && let Some(pos) = egui_ctx.pointer_hover_pos()
            {
                if let Some(row) = frame.row_at(pos.y).filter(|_| pos.x >= frame.geo.lane_x0) {
                    let rect = frame.row_rect(row);
                    painter.rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        Color32::from_rgba_unmultiplied(252, 198, 16, 14),
                    );
                    if row.kind == TrackKind::Main {
                        let clips = &ctx.doc.project.timeline().main_track().clips;
                        let index = vz_core::edit::index_for_time(clips, frame.geo.time(pos.x));
                        let t = clips
                            .get(index)
                            .map_or_else(|| clips.last().map_or(0, Clip::end), |c| c.start);
                        marker(rect, t);
                    } else {
                        painter.vline(pos.x, rect.y_range(), Stroke::new(2.0, theme::ACCENT));
                    }
                }
                zones(painter, frame, Some(pos), true);
            }
        }
    }
}

/// While dragging, show the strips where a drop makes a new track.
fn zones(painter: &Painter, frame: &Frame, pointer: Option<Pos2>, show: bool) {
    if !show {
        return;
    }
    let Some(pos) = pointer else { return };
    let hot = frame.zone_at(pos.y);
    for (zone, label) in [
        (Zone::Top, "Drop here for a new overlay track"),
        (Zone::Bottom, "Drop here for a new audio track"),
    ] {
        let (Some(first), Some(last)) = (frame.rows.first(), frame.rows.last()) else {
            return;
        };
        let y = match zone {
            Zone::Top => frame.origin_y + first.top - super::layout::ZONE_H + 2.0,
            Zone::Bottom => frame.origin_y + last.top + last.height + 2.0,
        };
        let rect = Rect::from_min_size(
            Pos2::new(frame.geo.lane_x0, y),
            vec2(
                frame.area.max.x - frame.geo.lane_x0,
                super::layout::ZONE_H - 4.0,
            ),
        );
        let active = hot == Some(zone);
        let fill = if active {
            Color32::from_rgba_unmultiplied(252, 198, 16, 50)
        } else {
            Color32::from_rgba_unmultiplied(252, 198, 16, 12)
        };
        painter.rect_filled(rect, CornerRadius::same(4), fill);
        if active {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.0),
                theme::ACCENT,
            );
        }
    }
}
