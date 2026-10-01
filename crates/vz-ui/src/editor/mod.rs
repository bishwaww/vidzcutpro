//! The editing screen: top bar, asset panel, player, inspector and timeline.
//!
//! Phase 0: every panel is drawn and resizable but empty. Real content arrives phase by phase.

mod assets;
mod inspector;
mod player;
mod timeline;
mod topbar;

use crate::settings::{LayoutPreset, UiSettings};
use crate::theme;
use eframe::egui::{self, Panel};
use vz_core::{Micros, Project};

pub use assets::Category;

/// Everything the editor screen remembers while it is open (none of it is saved in the project).
pub struct EditorState {
    pub category: Category,
    pub sub_menu: usize,
    pub search: String,
    pub magnet: bool,
    pub snapping: bool,
    pub linkage: bool,
    pub preview_axis: bool,
    /// Timeline zoom, 0.0 (zoomed out) to 1.0 (zoomed in).
    pub zoom: f32,
    pub playhead: Micros,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            category: Category::Media,
            sub_menu: 0,
            search: String::new(),
            magnet: true,
            snapping: true,
            linkage: true,
            preview_axis: false,
            zoom: 0.35,
            playhead: 0,
        }
    }
}

/// What the editor asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    GoHome,
    SetLayout(LayoutPreset),
    ResetLayout,
}

/// Images the editor needs.
pub struct Images<'a> {
    pub mark: &'a egui::TextureHandle,
}

/// Draw the whole editor screen.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    project: &mut Project,
    settings: &mut UiSettings,
    layout_gen: u32,
    images: &Images<'_>,
) -> (Action, bool) {
    let layout = settings.layout;
    let mut action = Action::None;
    let mut sizes_changed = false;
    let id = |name: &str| format!("{}-{name}-{layout_gen}", layout.key());
    let total = ui.available_size();

    let top = Panel::top(id("topbar"))
        .exact_size(46.0)
        .frame(theme::panel_frame(theme::BG))
        .show(ui, |ui| topbar::show(ui, project, settings, images));
    if top.inner != Action::None {
        action = top.inner;
    }

    let timeline_default = match layout {
        LayoutPreset::BigTimeline => (total.y * 0.46).max(300.0),
        _ => (total.y * 0.36).max(240.0),
    };
    let tl = Panel::bottom(id("timeline"))
        .resizable(true)
        .size_range(150.0..=(total.y * 0.75).max(200.0))
        .default_size(settings.size(layout, "timeline", timeline_default))
        .frame(theme::panel_frame(theme::PANEL))
        .show(ui, |ui| timeline::show(ui, state, project));
    sizes_changed |= settings.set_size(layout, "timeline", tl.response.rect.height());

    match layout {
        LayoutPreset::Default | LayoutPreset::BigTimeline => {
            let left = Panel::left(id("assets"))
                .resizable(true)
                .size_range(300.0..=(total.x * 0.6).max(320.0))
                .default_size(settings.size(layout, "assets", 400.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| assets::show(ui, state));
            sizes_changed |= settings.set_size(layout, "assets", left.response.rect.width());

            let right = Panel::right(id("inspector"))
                .resizable(true)
                .size_range(260.0..=(total.x * 0.5).max(300.0))
                .default_size(settings.size(layout, "inspector", 340.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| inspector::show(ui, project));
            sizes_changed |= settings.set_size(layout, "inspector", right.response.rect.width());

            egui::CentralPanel::default()
                .frame(theme::panel_frame(theme::BG))
                .show(ui, |ui| player::show(ui, state, project));
        }
        LayoutPreset::PlayerRight => {
            let left = Panel::left(id("assets"))
                .resizable(true)
                .size_range(300.0..=(total.x * 0.5).max(320.0))
                .default_size(settings.size(layout, "assets", 400.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| assets::show(ui, state));
            sizes_changed |= settings.set_size(layout, "assets", left.response.rect.width());

            let right = Panel::right(id("player"))
                .resizable(true)
                .size_range(320.0..=(total.x * 0.7).max(400.0))
                .default_size(settings.size(layout, "player", (total.x * 0.4).max(480.0)))
                .frame(theme::panel_frame(theme::BG))
                .show(ui, |ui| player::show(ui, state, project));
            sizes_changed |= settings.set_size(layout, "player", right.response.rect.width());

            egui::CentralPanel::default()
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| inspector::show(ui, project));
        }
    }

    (action, sizes_changed)
}
