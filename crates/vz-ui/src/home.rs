//! The home screen: create a project, see recent projects.

use crate::theme;
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Panel, RichText, Sense, Stroke, StrokeKind, vec2,
};
use std::path::PathBuf;

/// One entry of the "Recent projects" list.
pub struct RecentItem {
    pub path: PathBuf,
    pub name: String,
    pub duration: String,
    pub modified: String,
    pub exists: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    CreateProject,
    OpenDialog,
    Open(PathBuf),
    RemoveRecent(PathBuf),
}

pub fn show(ui: &mut egui::Ui, logo: &egui::TextureHandle, recent: &[RecentItem]) -> Action {
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

                    ui.horizontal(|ui| {
                        // the big yellow "Create project" card
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(380.0, 120.0), Sense::click());
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

                        ui.add_space(12.0);
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(180.0, 120.0), Sense::click());
                        let stroke = if resp.hovered() {
                            Stroke::new(1.5, theme::ACCENT)
                        } else {
                            Stroke::new(1.0, theme::BORDER)
                        };
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(16), theme::PANEL);
                        ui.painter().rect_stroke(
                            rect,
                            CornerRadius::same(16),
                            stroke,
                            StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Open project…",
                            FontId::proportional(15.0),
                            theme::TEXT,
                        );
                        if resp.clicked() {
                            action = Action::OpenDialog;
                        }
                    });

                    ui.add_space(32.0);
                    ui.label(RichText::new("Recent projects").size(18.0).strong());
                    ui.add_space(10.0);

                    if recent.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width().min(760.0), 130.0),
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
                            "Projects you save will appear here",
                            FontId::proportional(12.0),
                            theme::MUTED,
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for item in recent {
                                    if let Some(a) = recent_row(ui, item) {
                                        action = a;
                                    }
                                }
                            });
                    }
                });
        });

    action
}

fn recent_row(ui: &mut egui::Ui, item: &RecentItem) -> Option<Action> {
    let width = ui.available_width().min(760.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 58.0), Sense::click());
    let fill = if resp.hovered() {
        theme::PANEL_2
    } else {
        theme::PANEL
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    let name_color = if item.exists {
        theme::TEXT
    } else {
        theme::MUTED
    };
    ui.painter().text(
        rect.left_top() + vec2(16.0, 18.0),
        Align2::LEFT_CENTER,
        &item.name,
        FontId::proportional(15.0),
        name_color,
    );
    let detail = if item.exists {
        format!(
            "{}   ·   {}   ·   {}",
            item.duration,
            item.modified,
            item.path.display()
        )
    } else {
        format!("File not found   ·   {}", item.path.display())
    };
    ui.painter().text(
        rect.left_top() + vec2(16.0, 40.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(11.0),
        theme::MUTED,
    );

    let mut action = None;
    if resp.clicked() && item.exists {
        action = Some(Action::Open(item.path.clone()));
    }
    resp.context_menu(|ui| {
        if item.exists && ui.button("Open").clicked() {
            action = Some(Action::Open(item.path.clone()));
            ui.close();
        }
        if ui.button("Remove from this list").clicked() {
            action = Some(Action::RemoveRecent(item.path.clone()));
            ui.close();
        }
    });
    ui.add_space(4.0);
    action
}
