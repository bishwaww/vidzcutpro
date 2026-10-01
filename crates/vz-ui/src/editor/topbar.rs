//! The bar across the top: logo, Menu, save status, project name, layout choice, Export.

use super::{Action, Images};
use crate::settings::{LayoutPreset, UiSettings};
use crate::theme;
use eframe::egui::{self, Align, Layout, Rect, RichText};
use vz_core::Project;

const COMING_SOON: &str = "Coming soon";

pub fn show(
    ui: &mut egui::Ui,
    project: &mut Project,
    settings: &UiSettings,
    images: &Images<'_>,
) -> Action {
    let mut action = Action::None;
    let full = ui.max_rect();

    // The project name sits in the exact middle of the bar.
    let name_rect = Rect::from_center_size(full.center(), egui::vec2(300.0, 28.0));
    // (a child ui, so placing it does not push the rest of the bar out of view)
    let mut name_ui = ui.new_child(egui::UiBuilder::new().max_rect(name_rect));
    name_ui.add_sized(
        name_rect.size(),
        egui::TextEdit::singleline(&mut project.name)
            .horizontal_align(Align::Center)
            .font(egui::FontId::proportional(15.0))
            .frame(egui::Frame::NONE),
    );

    ui.horizontal_centered(|ui| {
        ui.add_space(12.0);
        ui.add(egui::Image::new(images.mark).fit_to_exact_size(egui::vec2(26.0, 26.0)));
        ui.add_space(4.0);

        ui.menu_button("Menu", |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Back to home").clicked() {
                    action = Action::GoHome;
                    ui.close();
                }
                for name in [
                    "New project",
                    "Open…",
                    "Save",
                    "Save as…",
                    "Import media…",
                    "Export…",
                ] {
                    ui.add_enabled(false, egui::Button::new(name))
                        .on_disabled_hover_text(COMING_SOON);
                }
            });
            ui.menu_button("Edit", |ui| {
                for name in [
                    "Undo",
                    "Redo",
                    "Cut",
                    "Copy",
                    "Paste",
                    "Delete",
                    "Select all",
                ] {
                    ui.add_enabled(false, egui::Button::new(name))
                        .on_disabled_hover_text(COMING_SOON);
                }
            });
            ui.menu_button("View", |ui| {
                for preset in LayoutPreset::ALL {
                    if ui
                        .radio(settings.layout == preset, preset.label())
                        .clicked()
                    {
                        action = Action::SetLayout(preset);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Reset layout").clicked() {
                    action = Action::ResetLayout;
                    ui.close();
                }
            });
            ui.add_enabled(false, egui::Button::new("Settings"))
                .on_disabled_hover_text(COMING_SOON);
            ui.menu_button("Help", |ui| {
                ui.add_enabled(false, egui::Button::new("Keyboard shortcuts"))
                    .on_disabled_hover_text(COMING_SOON);
                ui.add_enabled(false, egui::Button::new("Open log folder"))
                    .on_disabled_hover_text(COMING_SOON);
                ui.label(
                    RichText::new(format!("vidzcut {}", env!("CARGO_PKG_VERSION")))
                        .color(theme::MUTED),
                );
            });
        });

        ui.add_space(8.0);
        ui.label(RichText::new("● Not saved yet").color(theme::MUTED).small())
            .on_hover_text("Auto-save arrives in Phase 1");

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            ui.add_enabled(false, theme::primary_button("Export"))
                .on_disabled_hover_text(COMING_SOON);
            ui.add_space(6.0);
            ui.menu_button(format!("Layout: {}", settings.layout.label()), |ui| {
                for preset in LayoutPreset::ALL {
                    if ui
                        .radio(settings.layout == preset, preset.label())
                        .clicked()
                    {
                        action = Action::SetLayout(preset);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Reset layout").clicked() {
                    action = Action::ResetLayout;
                    ui.close();
                }
            });
        });
    });
    action
}
