//! The inspector (right panel). With nothing selected it shows the project details.

use crate::theme;
use eframe::egui::{self, RichText, vec2};
use vz_core::{AspectPreset, FrameRate, Project};

pub fn show(ui: &mut egui::Ui, project: &mut Project) {
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
                    details(ui, project);
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

fn details(ui: &mut egui::Ui, project: &mut Project) {
    egui::Grid::new("project_details")
        .num_columns(2)
        .spacing(vec2(16.0, 10.0))
        .min_col_width(96.0)
        .show(ui, |ui| {
            row(ui, "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut project.name).desired_width(f32::INFINITY));
            });
            row(ui, "Aspect ratio", |ui| {
                let current = project.aspect_preset();
                egui::ComboBox::from_id_salt("details_ratio")
                    .selected_text(current.map_or("Custom", AspectPreset::label))
                    .show_ui(ui, |ui| {
                        for preset in AspectPreset::ALL {
                            if ui
                                .selectable_label(current == Some(preset), preset.label())
                                .clicked()
                            {
                                project.set_aspect(preset);
                            }
                        }
                    });
            });
            row(ui, "Resolution", |ui| {
                ui.label(format!(
                    "{} × {}",
                    project.canvas.width, project.canvas.height
                ));
            });
            row(ui, "Frame rate", |ui| {
                egui::ComboBox::from_id_salt("details_fps")
                    .selected_text(format!("{} fps", project.frame_rate.label()))
                    .show_ui(ui, |ui| {
                        for rate in FrameRate::PRESETS {
                            if ui
                                .selectable_label(
                                    project.frame_rate == rate,
                                    format!("{} fps", rate.label()),
                                )
                                .clicked()
                            {
                                project.frame_rate = rate;
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
                ui.label(RichText::new("Not saved yet").color(theme::MUTED));
            });
            row(ui, "Duration", |ui| {
                ui.label(RichText::new(project.frame_rate.timecode(project.duration)).monospace());
            });
        });
}
