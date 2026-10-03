//! The Transitions tab of the asset panel: pick a transition and put it on a cut. Tiles show a live preview when hovered.

use super::Ctx;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind,
    TextureOptions, vec2,
};
use vz_core::{ClipId, TrackKind, Transition};
use vz_media::VideoFrame;
use vz_presets::transition::{Group, TransitionPreset, find, transitions};
use vz_render::transition::blend;

const TILE_W: f32 = 108.0;
const TILE_H: f32 = 64.0;
/// Size of the little pictures the previews are made from.
const PREVIEW: (u32, u32) = (96, 54);

/// A warm picture and a cool picture, so a blend of them shows what a transition does.
fn sample_pictures() -> (VideoFrame, VideoFrame) {
    let make = |from: [f32; 3], to: [f32; 3], dot: [u8; 3]| {
        let mut f = VideoFrame::black(PREVIEW.0, PREVIEW.1);
        for y in 0..PREVIEW.1 {
            for x in 0..PREVIEW.0 {
                let t = (x as f32 / PREVIEW.0 as f32 + y as f32 / PREVIEW.1 as f32) / 2.0;
                let i = ((y * PREVIEW.0 + x) * 4) as usize;
                for c in 0..3 {
                    f.rgba[i + c] = (from[c] + (to[c] - from[c]) * t) as u8;
                }
                // a bright dot in the middle, so movement and zoom can be seen
                let (dx, dy) = (x as f32 - 48.0, y as f32 - 27.0);
                if dx * dx + dy * dy < 100.0 {
                    f.rgba[i..i + 3].copy_from_slice(&dot);
                }
            }
        }
        f
    };
    (
        make([255.0, 150.0, 40.0], [200.0, 40.0, 60.0], [255, 240, 200]),
        make([30.0, 160.0, 200.0], [40.0, 60.0, 190.0], [200, 255, 240]),
    )
}

thread_local! {
    static PICTURES: (VideoFrame, VideoFrame) = sample_pictures();
}

fn preview_image(preset: &TransitionPreset, progress: f32) -> ColorImage {
    PICTURES.with(|(a, b)| {
        let f = blend(a, b, progress, preset.recipe);
        ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.rgba)
    })
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let group = Group::ALL[ctx.state.sub_menu.min(Group::ALL.len() - 1)];
    ui.label(
        RichText::new(
            "Click one to put it on the cut at the playhead (or after the selected clip).",
        )
        .color(theme::MUTED)
        .small(),
    );
    ui.add_space(8.0);
    let search = ctx.state.search.to_lowercase();
    let list: Vec<&'static TransitionPreset> = transitions()
        .iter()
        .filter(|t| t.group == group)
        .filter(|t| search.is_empty() || t.name.to_lowercase().contains(&search))
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for preset in list {
                    if tile(ui, ctx, preset).clicked() {
                        apply(ctx, preset.id);
                    }
                }
            });
        });
}

fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, preset: &TransitionPreset) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE_W, TILE_H));
    let key = format!("tr:{}", preset.id);
    // hovering plays the transition in a loop; otherwise it shows the middle of it
    let progress = if resp.hovered() {
        ui.ctx().request_repaint();
        let t = ui.input(|i| i.time) as f32;
        (t * 0.9).fract()
    } else {
        0.5
    };
    // the picture is only remade while the mouse is on the tile (and once more when it leaves)
    let was_hovered = ctx.state.tile_hover.as_deref() == Some(key.as_str());
    let redraw = resp.hovered() || was_hovered;
    if resp.hovered() {
        ctx.state.tile_hover = Some(key.clone());
    } else if was_hovered {
        ctx.state.tile_hover = None;
    }
    let texture = match ctx.state.preset_tiles.get_mut(&key) {
        Some(tex) => {
            if redraw {
                tex.set(preview_image(preset, progress), TextureOptions::LINEAR);
            }
            tex.clone()
        }
        None => {
            let tex = ui.ctx().load_texture(
                key.clone(),
                preview_image(preset, progress),
                TextureOptions::LINEAR,
            );
            ctx.state.preset_tiles.insert(key.clone(), tex.clone());
            tex
        }
    };
    let painter = ui.painter();
    painter.image(
        texture.id(),
        card,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if resp.hovered() {
        painter.rect_stroke(
            card,
            CornerRadius::same(4),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        preset.name,
        FontId::proportional(11.0),
        theme::TEXT,
    );
    resp.on_hover_text(preset.name)
}

/// The clip whose end a new transition should go on: the selected clip if another clip follows it, otherwise the cut
/// nearest to the playhead.
pub fn target_cut(ctx: &Ctx<'_>) -> Option<ClipId> {
    let tl = ctx.doc.project.timeline();
    let joinable = |track: &vz_core::Track, id: ClipId| {
        matches!(track.kind, TrackKind::Main | TrackKind::Overlay)
            && track
                .clips
                .windows(2)
                .any(|w| w[0].id == id && w[1].start == w[0].end())
    };
    if let [id] = ctx.state.selection[..]
        && let Some(track) = tl.track_of(id)
        && joinable(track, id)
    {
        return Some(id);
    }
    let main = tl.main_track();
    main.clips
        .windows(2)
        .filter(|w| w[1].start == w[0].end())
        .min_by_key(|w| (w[0].end() - ctx.state.playhead).abs())
        .map(|w| w[0].id)
}

/// Put the transition on its target cut and select it.
pub fn apply(ctx: &mut Ctx<'_>, preset_id: &str) {
    let Some(preset) = find(preset_id) else {
        return;
    };
    let Some(after) = target_cut(ctx) else {
        ctx.state
            .say("Put another clip after this one first: a transition joins two clips.");
        return;
    };
    // keep the duration and easing of a transition that is already there
    let existing = ctx
        .doc
        .project
        .timeline()
        .track_of(after)
        .and_then(|t| t.transitions.iter().find(|x| x.after == after))
        .cloned();
    let transition = Transition {
        after,
        preset: preset.id.to_owned(),
        duration: existing
            .as_ref()
            .map_or(preset.default_duration, |t| t.duration),
        easing: existing.map_or(vz_anim::Easing::EaseInOut, |t| t.easing),
    };
    match ctx.doc.set_transition(after, Some(transition), None) {
        Ok(()) => {
            ctx.state.selected_transition = Some(after);
            ctx.state.selection.clear();
            // show the middle of the transition
            if let Some(clip) = ctx.doc.project.timeline().clip(after) {
                let end = clip.end();
                ctx.seek(end - 200_000);
            }
        }
        Err(e) => ctx.state.say(format!("Could not add the transition: {e}")),
    }
}
