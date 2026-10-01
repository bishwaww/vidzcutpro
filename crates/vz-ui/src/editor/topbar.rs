//! The bar across the top: logo, Menu, save status, project name, layout choice, Export.

use super::{Action, Ctx};
use crate::settings::LayoutPreset;
use crate::theme;
use eframe::egui::{self, Align, Layout, Rect, RichText};

const COMING_SOON: &str = "Coming soon";

/// Typing in the name box is one undo step until the box loses focus (merge keys from drags start at 2).
pub const NAME_TYPING_KEY: u64 = 1;

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let full = ui.max_rect();

    // The project name sits in the exact middle of the bar.
    let name_rect = Rect::from_center_size(full.center(), egui::vec2(300.0, 28.0));
    // (a child ui, so placing it does not push the rest of the bar out of view)
    let mut name_ui = ui.new_child(egui::UiBuilder::new().max_rect(name_rect));
    let mut name = ctx.doc.project.name.clone();
    let response = name_ui.add_sized(
        name_rect.size(),
        egui::TextEdit::singleline(&mut name)
            .horizontal_align(Align::Center)
            .font(egui::FontId::proportional(15.0))
            .frame(egui::Frame::NONE),
    );
    if response.changed() {
        let _ = ctx.doc.rename(name, Some(NAME_TYPING_KEY));
    }
    if response.lost_focus() {
        ctx.doc.seal_history();
    }

    ui.horizontal_centered(|ui| {
        ui.add_space(12.0);
        ui.add(egui::Image::new(ctx.images.mark).fit_to_exact_size(egui::vec2(26.0, 26.0)));
        ui.add_space(4.0);

        ui.menu_button("Menu", |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Back to home").clicked() {
                    ctx.actions.push(Action::GoHome);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Open…").shortcut_text("Ctrl+O"))
                    .clicked()
                {
                    ctx.actions.push(Action::OpenDialog);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                    .clicked()
                {
                    ctx.actions.push(Action::Save);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Save as…").shortcut_text("Ctrl+Shift+S"))
                    .clicked()
                {
                    ctx.actions.push(Action::SaveAs);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Import media…").shortcut_text("Ctrl+I"))
                    .clicked()
                {
                    ctx.actions.push(Action::ImportDialog);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new("Export…").shortcut_text("Ctrl+E"))
                    .clicked()
                {
                    ctx.actions.push(Action::Export);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                let undo_text = ctx
                    .doc
                    .undo_label()
                    .map_or("Undo".to_string(), |l| format!("Undo {l}"));
                if ui
                    .add_enabled(
                        ctx.doc.can_undo(),
                        egui::Button::new(undo_text).shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    ctx.doc.undo();
                    ui.close();
                }
                let redo_text = ctx
                    .doc
                    .redo_label()
                    .map_or("Redo".to_string(), |l| format!("Redo {l}"));
                if ui
                    .add_enabled(
                        ctx.doc.can_redo(),
                        egui::Button::new(redo_text).shortcut_text("Ctrl+Shift+Z"),
                    )
                    .clicked()
                {
                    ctx.doc.redo();
                    ui.close();
                }
                ui.separator();
                for name in ["Cut", "Copy", "Paste", "Select all"] {
                    ui.add_enabled(false, egui::Button::new(name))
                        .on_disabled_hover_text(COMING_SOON);
                }
            });
            ui.menu_button("View", |ui| {
                for preset in LayoutPreset::ALL {
                    if ui
                        .radio(ctx.settings.layout == preset, preset.label())
                        .clicked()
                    {
                        ctx.actions.push(Action::SetLayout(preset));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Reset layout").clicked() {
                    ctx.actions.push(Action::ResetLayout);
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
        let (status, hover) = match (&ctx.saved_at, ctx.doc.is_dirty(), &ctx.doc.path) {
            (Some(t), _, _) if ctx.doc.is_dirty() => (
                format!("● Auto saved: {t}"),
                "A recovery copy was saved. Press Ctrl+S to save the project.",
            ),
            (_, false, Some(_)) => ("● Saved".to_string(), "All changes are saved."),
            (_, true, Some(_)) => ("● Unsaved changes".to_string(), "Press Ctrl+S to save."),
            (_, _, None) => (
                "● Not saved yet".to_string(),
                "Press Ctrl+S to choose where to save this project.",
            ),
        };
        ui.label(RichText::new(status).color(theme::MUTED).small())
            .on_hover_text(hover);

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            let can_export = ctx.doc.project.duration() > 0;
            if ui
                .add_enabled(can_export, theme::primary_button("Export"))
                .on_disabled_hover_text("Add something to the timeline first")
                .clicked()
            {
                ctx.actions.push(Action::Export);
            }
            ui.add_space(6.0);
            ui.menu_button(format!("Layout: {}", ctx.settings.layout.label()), |ui| {
                for preset in LayoutPreset::ALL {
                    if ui
                        .radio(ctx.settings.layout == preset, preset.label())
                        .clicked()
                    {
                        ctx.actions.push(Action::SetLayout(preset));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Reset layout").clicked() {
                    ctx.actions.push(Action::ResetLayout);
                    ui.close();
                }
            });
        });
    });
}
