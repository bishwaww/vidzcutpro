//! The home screen: create a project, see recent projects.

use crate::theme;
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Panel, RichText, Sense, Stroke, StrokeKind, vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    CreateProject,
}

pub fn show(ui: &mut egui::Ui, logo: &egui::TextureHandle) -> Action {
    let mut action = Action::None;

    Panel::left("home_side")
        .exact_size(210.0)
        .resizable(false)
        .frame(theme::panel_frame(theme::PANEL))
        .show(ui, |ui| {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new(logo).fit_to_exact_size(vec2(120.0, 120.0)));
            });
            ui.add_space(20.0);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 0))
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), 34.0],
                        egui::Button::new(RichText::new("Home").color(theme::ACCENT).strong())
                            .fill(theme::ACCENT_DARK),
                    );
                    ui.add_enabled_ui(false, |ui| {
                        ui.add_sized([ui.available_width(), 34.0], egui::Button::new("Settings"))
                            .on_disabled_hover_text("Coming soon");
                    });
                });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(format!("vidzcut {}", env!("CARGO_PKG_VERSION")))
                            .color(theme::MUTED)
                            .small(),
                    );
                });
            });
        });

    egui::CentralPanel::default()
        .frame(theme::panel_frame(theme::BG))
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(40, 32))
                .show(ui, |ui| {
                    ui.label(RichText::new("Welcome to vidzcut").size(26.0).strong());
                    ui.add_space(20.0);

                    // the big yellow "Create project" card
                    let (rect, resp) = ui.allocate_exact_size(vec2(380.0, 120.0), Sense::click());
                    let fill = if resp.hovered() {
                        theme::ACCENT_HOVER
                    } else {
                        theme::ACCENT
                    };
                    ui.painter().rect_filled(rect, CornerRadius::same(16), fill);
                    ui.painter().text(
                        rect.center() - vec2(0.0, 12.0),
                        Align2::CENTER_CENTER,
                        "+",
                        FontId::proportional(40.0),
                        theme::ON_ACCENT,
                    );
                    ui.painter().text(
                        rect.center() + vec2(0.0, 26.0),
                        Align2::CENTER_CENTER,
                        "Create project",
                        FontId::proportional(18.0),
                        theme::ON_ACCENT,
                    );
                    if resp.clicked() {
                        action = Action::CreateProject;
                    }

                    ui.add_space(32.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Recent projects").size(18.0).strong());
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut String::new()).hint_text("Search"),
                        );
                    });
                    ui.add_space(10.0);

                    // empty state
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width().min(760.0), 150.0),
                        Sense::hover(),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(12),
                        Stroke::new(1.0, theme::BORDER),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center() - vec2(0.0, 8.0),
                        Align2::CENTER_CENTER,
                        "No projects yet",
                        FontId::proportional(15.0),
                        theme::TEXT,
                    );
                    ui.painter().text(
                        rect.center() + vec2(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        "Saving and opening projects arrives in Phase 1",
                        FontId::proportional(12.0),
                        theme::MUTED,
                    );
                });
        });

    action
}
