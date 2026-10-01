//! The inspector (right panel). With nothing selected it shows the project details.

use super::assets::short_duration;
use super::{Ctx, topbar::NAME_TYPING_KEY};
use crate::theme;
use eframe::egui::{self, RichText, vec2};
use vz_core::{AspectPreset, FrameRate, Micros};

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.add_space(8.0);
    // tab strip (only "Details" exists while nothing is selected)
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        let resp = ui.label(RichText::new("Details").strong().color(theme::ACCENT));
        let y = resp.rect.bottom() + 3.0;
        ui.painter().hline(
            resp.rect.x_range(),
            y,
            egui::Stroke::new(2.0, theme::ACCENT),
        );
    });
    ui.add_space(4.0);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(6.0);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(14, 0))
                .show(ui, |ui| {
                    if ctx.state.selection.len() == 1 {
                        clip_details(ui, ctx);
                        ui.add_space(14.0);
                    }
                    details(ui, ctx);
                    ui.add_space(14.0);
                    ui.label(RichText::new("Global edits").strong());
                    ui.add_space(4.0);
                    for name in [
                        "Normalize loudness of all clips",
                        "Apply a filter to all clips",
                        "Apply a transition to all cuts",
                    ] {
                        ui.add_enabled(false, egui::Button::new(name))
                            .on_disabled_hover_text("Coming soon");
                    }
                });
        });
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(label).color(theme::MUTED));
    add(ui);
    ui.end_row();
}

/// Read-only facts about the selected clip.
fn clip_details(ui: &mut egui::Ui, ctx: &Ctx<'_>) {
    let Some(clip) = ctx
        .doc
        .project
        .timeline()
        .main_track()
        .clip(ctx.state.selection[0])
    else {
        return;
    };
    let rate = ctx.doc.project.frame_rate;
    let name = ctx
        .doc
        .project
        .asset(clip.media)
        .map_or("?", |a| a.name.as_str());
    ui.label(RichText::new("Selected clip").strong());
    ui.add_space(4.0);
    let tc = |t: Micros| rate.timecode(t);
    egui::Grid::new("clip_details")
        .num_columns(2)
        .spacing(vec2(16.0, 8.0))
        .min_col_width(96.0)
        .show(ui, |ui| {
            row(ui, "File", |ui| {
                ui.label(name);
            });
            row(ui, "Starts at", |ui| {
                ui.label(RichText::new(tc(clip.start)).monospace());
            });
            row(ui, "Length", |ui| {
                ui.label(format!(
                    "{}  ({})",
                    tc(clip.duration),
                    short_duration(clip.duration)
                ));
            });
            row(ui, "From file time", |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} → {}",
                        tc(clip.source_in),
                        tc(clip.source_in + clip.duration)
                    ))
                    .monospace(),
                );
            });
        });
}

fn details(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    egui::Grid::new("project_details")
        .num_columns(2)
        .spacing(vec2(16.0, 10.0))
        .min_col_width(96.0)
        .show(ui, |ui| {
            row(ui, "Name", |ui| {
                let mut name = ctx.doc.project.name.clone();
                let response =
                    ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                if response.changed() {
                    let _ = ctx.doc.rename(name, Some(NAME_TYPING_KEY));
                }
                if response.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
            row(ui, "Aspect ratio", |ui| {
                let current = ctx.doc.project.aspect_preset();
                egui::ComboBox::from_id_salt("details_ratio")
                    .selected_text(current.map_or("Custom", AspectPreset::label))
                    .show_ui(ui, |ui| {
                        for preset in AspectPreset::ALL {
                            if ui
                                .selectable_label(current == Some(preset), preset.label())
                                .clicked()
                            {
                                let _ = ctx.doc.set_aspect(preset);
                            }
                        }
                    });
            });
            row(ui, "Resolution", |ui| {
                ui.label(format!(
                    "{} × {}",
                    ctx.doc.project.canvas.width, ctx.doc.project.canvas.height
                ));
            });
            row(ui, "Frame rate", |ui| {
                let rate = ctx.doc.project.frame_rate;
                egui::ComboBox::from_id_salt("details_fps")
                    .selected_text(format!("{} fps", rate.label()))
                    .show_ui(ui, |ui| {
                        for candidate in FrameRate::PRESETS {
                            if ui
                                .selectable_label(
                                    rate == candidate,
                                    format!("{} fps", candidate.label()),
                                )
                                .clicked()
                            {
                                let _ = ctx.doc.set_frame_rate(candidate);
                            }
                        }
                    });
            });
            row(ui, "Colour space", |ui| {
                ui.label("Rec. 709 SDR");
            });
            row(ui, "Proxy", |ui| {
                let mut off = false;
                ui.add_enabled(false, egui::Checkbox::new(&mut off, "Use proxies"))
                    .on_disabled_hover_text("Coming in Phase 7");
            });
            row(ui, "Saved to", |ui| {
                match &ctx.doc.path {
                    Some(p) => ui
                        .label(p.display().to_string())
                        .on_hover_text(p.display().to_string()),
                    None => ui.label(RichText::new("Not saved yet").color(theme::MUTED)),
                };
            });
            row(ui, "Duration", |ui| {
                let d = ctx.doc.project.duration();
                ui.label(RichText::new(ctx.doc.project.frame_rate.timecode(d)).monospace());
            });
        });
}
