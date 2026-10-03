//! The inspector panel for a transition selected on the timeline.

use super::{Ctx, key, row, section_title};
use crate::theme;
use eframe::egui::{self, RichText, vec2};
use vz_core::{ClipId, US_PER_SEC};
use vz_presets::transition::{Group, find, transitions};

pub fn transition_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, after: ClipId) {
    let Some((tr, longest)) = ctx.doc.project.timeline().track_of(after).and_then(|t| {
        t.transition_after(after)
            .map(|(tr, a, b)| (tr.clone(), a.duration.min(b.duration)))
    }) else {
        ctx.state.selected_transition = None;
        return;
    };
    let name = find(&tr.preset).map_or("?", |p| p.name);
    section_title(ui, "Transition");
    egui::Grid::new("transition_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Kind", |ui| {
                egui::ComboBox::from_id_salt("transition_kind")
                    .selected_text(name)
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for group in Group::ALL {
                            ui.label(RichText::new(group.label()).color(theme::MUTED).small());
                            for p in transitions().iter().filter(|p| p.group == group) {
                                if ui.selectable_label(tr.preset == p.id, p.name).clicked() {
                                    let mut next = tr.clone();
                                    next.preset = p.id.to_owned();
                                    let _ = ctx.doc.set_transition(after, Some(next), None);
                                }
                            }
                        }
                    });
            });
            row(ui, "Length", |ui| {
                let mut seconds = tr.duration as f32 / US_PER_SEC as f32;
                let max = (longest as f32 / US_PER_SEC as f32).max(0.2);
                let resp = ui.add(
                    egui::Slider::new(&mut seconds, 0.1..=max)
                        .suffix(" s")
                        .max_decimals(1)
                        .clamping(egui::SliderClamping::Always),
                );
                if resp.changed() {
                    let mut next = tr.clone();
                    next.duration = (f64::from(seconds) * US_PER_SEC as f64) as i64;
                    let _ = ctx.doc.set_transition(
                        after,
                        Some(next),
                        Some(key(after, "transition-length")),
                    );
                }
                if resp.drag_stopped() || resp.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
            row(ui, "Easing", |ui| {
                egui::ComboBox::from_id_salt("transition_easing")
                    .selected_text(tr.easing.label())
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for easing in vz_anim::Easing::presets() {
                            if ui
                                .selectable_label(tr.easing == easing, easing.label())
                                .clicked()
                            {
                                let mut next = tr.clone();
                                next.easing = easing;
                                let _ = ctx.doc.set_transition(after, Some(next), None);
                            }
                        }
                    });
            });
        });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .button("Apply to all cuts")
            .on_hover_text("Use this transition, length and easing on every cut of the main track")
            .clicked()
        {
            match ctx
                .doc
                .apply_transition_to_all(&tr.preset, tr.duration, tr.easing)
            {
                Ok(n) => ctx.state.say(format!("Transition added to {n} cuts")),
                Err(e) => ctx.state.say(format!("Could not do that: {e}")),
            }
        }
        if ui.button("Remove").clicked() {
            let _ = ctx.doc.set_transition(after, None, None);
            ctx.state.selected_transition = None;
        }
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new("The transition is centred on the cut. Where a clip has no more picture to give, its first or last picture is held.")
            .color(theme::MUTED)
            .small(),
    );
}
