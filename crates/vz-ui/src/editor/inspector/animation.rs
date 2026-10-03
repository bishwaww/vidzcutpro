//! The Animation tab of the inspector: ready-made In, Out and Combo (looping) animations for a clip.

use super::{Ctx, change, section_title, slider};
use crate::theme;
use eframe::egui::{self, RichText};
use vz_core::{AnimRef, Clip, US_PER_SEC};
use vz_presets::animation::{AnimKind, presets};

pub fn animation_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    section_title(ui, "Animation");
    ui.horizontal(|ui| {
        for (kind, label) in [
            (AnimKind::In, "In"),
            (AnimKind::Out, "Out"),
            (AnimKind::Combo, "Combo"),
        ] {
            if ui
                .selectable_label(ctx.state.anim_kind == kind, label)
                .clicked()
            {
                ctx.state.anim_kind = kind;
            }
        }
    });
    let kind = ctx.state.anim_kind;
    let current: Option<&AnimRef> = match kind {
        AnimKind::In => clip.anim_in.as_ref(),
        AnimKind::Out => clip.anim_out.as_ref(),
        AnimKind::Combo => clip.anim_combo.as_ref(),
    };
    ui.add_space(4.0);
    ui.label(
        RichText::new(match kind {
            AnimKind::In => "Plays when the clip appears.",
            AnimKind::Out => "Plays when the clip goes away.",
            AnimKind::Combo => "Repeats while the clip is shown. Choosing one removes In and Out.",
        })
        .color(theme::MUTED)
        .small(),
    );
    ui.add_space(6.0);

    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(current.is_none(), "None").clicked() {
            change(ctx, id, "Remove animation", |c| set(c, kind, None));
        }
        for p in presets(kind) {
            let on = current.is_some_and(|a| a.preset == p.id);
            if ui.selectable_label(on, p.name).clicked() {
                let keep = current.map_or(p.default_duration, |a| a.duration);
                let duration = if on { keep } else { p.default_duration };
                let anim = AnimRef {
                    preset: p.id.to_owned(),
                    duration: duration.min(clip.duration.max(1)),
                };
                change(ctx, id, "Animation", |c| set(c, kind, Some(anim)));
                // show the first moment of the animation
                let at = match kind {
                    AnimKind::Out => clip.end() - duration.min(clip.duration),
                    _ => clip.start,
                };
                ctx.seek(at);
            }
        }
    });

    if let Some(a) = current {
        ui.add_space(8.0);
        let (min, max) = match kind {
            AnimKind::Combo => (0.2, 6.0),
            _ => (0.1, (clip.duration as f32 / US_PER_SEC as f32).max(0.2)),
        };
        egui::Grid::new("anim_duration")
            .num_columns(2)
            .show(ui, |ui| {
                slider(
                    ui,
                    ctx,
                    id,
                    "anim.duration",
                    if kind == AnimKind::Combo {
                        "One cycle"
                    } else {
                        "Length"
                    },
                    a.duration as f32 / US_PER_SEC as f32,
                    min..=max,
                    " s",
                    1,
                    move |c, v| {
                        let micros = (f64::from(v) * US_PER_SEC as f64) as i64;
                        let target = match kind {
                            AnimKind::In => &mut c.anim_in,
                            AnimKind::Out => &mut c.anim_out,
                            AnimKind::Combo => &mut c.anim_combo,
                        };
                        if let Some(a) = target {
                            a.duration = micros;
                        }
                    },
                );
            });
    }
}

/// Choose (or remove) an animation. A combo replaces In and Out, and choosing In or Out removes a combo.
fn set(c: &mut Clip, kind: AnimKind, anim: Option<AnimRef>) {
    match kind {
        AnimKind::In => {
            if anim.is_some() {
                c.anim_combo = None;
            }
            c.anim_in = anim;
        }
        AnimKind::Out => {
            if anim.is_some() {
                c.anim_combo = None;
            }
            c.anim_out = anim;
        }
        AnimKind::Combo => {
            if anim.is_some() {
                c.anim_in = None;
                c.anim_out = None;
            }
            c.anim_combo = anim;
        }
    }
}
