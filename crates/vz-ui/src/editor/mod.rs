//! The editing screen: top bar, asset panel, player, inspector and timeline.

mod assets;
mod inspector;
mod player;
mod timeline;
mod topbar;

use crate::settings::{LayoutPreset, UiSettings};
use crate::theme;
use eframe::egui::{self, Key, Modifiers, Panel};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use vz_core::{ClipId, Document, MediaId, Micros};

pub use assets::{Category, short_duration};
pub use timeline::TimelineDrag;

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
    /// How far the timeline is scrolled to the right, in screen points.
    pub scroll_x: f32,
    /// Width of the timeline lanes on screen, set while drawing (used by "Fit").
    pub lane_width: f32,
    pub playhead: Micros,
    pub playing: bool,
    pub selection: Vec<ClipId>,
    pub(crate) drag: Option<TimelineDrag>,
    next_merge_key: u64,
    pub toast: Option<(String, Instant)>,
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
            scroll_x: 0.0,
            lane_width: 800.0,
            playhead: 0,
            playing: false,
            selection: Vec::new(),
            drag: None,
            next_merge_key: 1,
            toast: None,
        }
    }
}

impl EditorState {
    /// A fresh key for one drag or typing session (see `History::execute`).
    pub fn new_merge_key(&mut self) -> u64 {
        self.next_merge_key += 1;
        self.next_merge_key
    }

    /// Show a short message at the bottom of the window.
    pub fn say(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }
}

/// What the editor asks the app to do (things the editor cannot do by itself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    GoHome,
    SetLayout(LayoutPreset),
    ResetLayout,
    ImportDialog,
    Save,
    SaveAs,
    OpenDialog,
    TogglePlay,
    Export,
    /// Ask for a new thumbnail (used after relinking).
    Relink(MediaId),
}

/// Images the editor needs.
pub struct Images<'a> {
    pub mark: &'a egui::TextureHandle,
}

/// Everything the editor panels get to look at and change.
pub struct Ctx<'a> {
    pub doc: &'a mut Document,
    pub state: &'a mut EditorState,
    pub settings: &'a mut UiSettings,
    pub layout_gen: u32,
    pub images: Images<'a>,
    pub thumbs: &'a HashMap<MediaId, egui::TextureHandle>,
    pub missing: &'a HashSet<MediaId>,
    pub player_texture: Option<&'a egui::TextureHandle>,
    pub actions: Vec<Action>,
    pub saved_at: Option<String>,
}

impl Ctx<'_> {
    /// Move the playhead (snapped to a frame, kept inside the timeline).
    pub fn seek(&mut self, t: Micros) {
        let rate = self.doc.project.frame_rate;
        let end = self.doc.project.duration();
        self.state.playhead = rate.snap(t).clamp(0, end);
    }
}

/// Draw the whole editor screen. Returns true when a panel size changed (so it can be saved).
pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) -> bool {
    let layout = ctx.settings.layout;
    let mut sizes_changed = false;
    let layout_gen = ctx.layout_gen;
    let id = |name: &str| format!("{}-{name}-{layout_gen}", layout.key());
    let total = ui.available_size();

    handle_keys(ui.ctx(), ctx);

    Panel::top(id("topbar"))
        .exact_size(46.0)
        .frame(theme::panel_frame(theme::BG))
        .show(ui, |ui| topbar::show(ui, ctx));

    let timeline_default = match layout {
        LayoutPreset::BigTimeline => (total.y * 0.46).max(300.0),
        _ => (total.y * 0.36).max(240.0),
    };
    let tl = Panel::bottom(id("timeline"))
        .resizable(true)
        .size_range(150.0..=(total.y * 0.75).max(200.0))
        .default_size(ctx.settings.size(layout, "timeline", timeline_default))
        .frame(theme::panel_frame(theme::PANEL))
        .show(ui, |ui| timeline::show(ui, ctx));
    sizes_changed |= ctx
        .settings
        .set_size(layout, "timeline", tl.response.rect.height());

    match layout {
        LayoutPreset::Default | LayoutPreset::BigTimeline => {
            let left = Panel::left(id("assets"))
                .resizable(true)
                .size_range(300.0..=(total.x * 0.6).max(320.0))
                .default_size(ctx.settings.size(layout, "assets", 400.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| assets::show(ui, ctx));
            sizes_changed |= ctx
                .settings
                .set_size(layout, "assets", left.response.rect.width());

            let right = Panel::right(id("inspector"))
                .resizable(true)
                .size_range(260.0..=(total.x * 0.5).max(300.0))
                .default_size(ctx.settings.size(layout, "inspector", 340.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| inspector::show(ui, ctx));
            sizes_changed |=
                ctx.settings
                    .set_size(layout, "inspector", right.response.rect.width());

            egui::CentralPanel::default()
                .frame(theme::panel_frame(theme::BG))
                .show(ui, |ui| player::show(ui, ctx));
        }
        LayoutPreset::PlayerRight => {
            let left = Panel::left(id("assets"))
                .resizable(true)
                .size_range(300.0..=(total.x * 0.5).max(320.0))
                .default_size(ctx.settings.size(layout, "assets", 400.0))
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| assets::show(ui, ctx));
            sizes_changed |= ctx
                .settings
                .set_size(layout, "assets", left.response.rect.width());

            let right = Panel::right(id("player"))
                .resizable(true)
                .size_range(320.0..=(total.x * 0.7).max(400.0))
                .default_size(
                    ctx.settings
                        .size(layout, "player", (total.x * 0.4).max(480.0)),
                )
                .frame(theme::panel_frame(theme::BG))
                .show(ui, |ui| player::show(ui, ctx));
            sizes_changed |= ctx
                .settings
                .set_size(layout, "player", right.response.rect.width());

            egui::CentralPanel::default()
                .frame(theme::panel_frame(theme::PANEL))
                .show(ui, |ui| inspector::show(ui, ctx));
        }
    }

    draw_overlays(ui.ctx(), ctx);
    sizes_changed
}

/// Keyboard shortcuts (the CapCut-style defaults from the spec). They are ignored while typing in a text box.
fn handle_keys(egui_ctx: &egui::Context, ctx: &mut Ctx<'_>) {
    if egui_ctx.egui_wants_keyboard_input() {
        return;
    }
    let (cmd, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
    let pressed = |mods: Modifiers, key: Key| egui_ctx.input_mut(|i| i.consume_key(mods, key));

    if pressed(Modifiers::NONE, Key::Space) {
        ctx.actions.push(Action::TogglePlay);
    }
    if pressed(cmd | shift, Key::Z) || pressed(cmd, Key::Y) {
        ctx.doc.redo();
    } else if pressed(cmd, Key::Z) {
        ctx.doc.undo();
    }
    if pressed(cmd | shift, Key::S) {
        ctx.actions.push(Action::SaveAs);
    } else if pressed(cmd, Key::S) {
        ctx.actions.push(Action::Save);
    }
    if pressed(cmd, Key::O) {
        ctx.actions.push(Action::OpenDialog);
    }
    if pressed(cmd, Key::I) {
        ctx.actions.push(Action::ImportDialog);
    }
    if pressed(cmd, Key::E) {
        ctx.actions.push(Action::Export);
    }
    if pressed(cmd, Key::B) {
        timeline::split_at_playhead(ctx);
    }
    if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
        timeline::delete_selection(ctx);
    }
    // frame step and jumps
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    if pressed(Modifiers::NONE, Key::ArrowLeft) {
        ctx.seek(ctx.state.playhead - frame);
    }
    if pressed(Modifiers::NONE, Key::ArrowRight) {
        ctx.seek(ctx.state.playhead + frame);
    }
    if pressed(Modifiers::NONE, Key::Home) {
        ctx.seek(0);
    }
    if pressed(Modifiers::NONE, Key::End) {
        ctx.seek(ctx.doc.project.duration());
    }
    if pressed(Modifiers::NONE, Key::Escape) {
        ctx.state.selection.clear();
    }
}

/// The "drop files to import" cover while files are dragged over the window, and the toast message.
fn draw_overlays(egui_ctx: &egui::Context, ctx: &mut Ctx<'_>) {
    use egui::{Align2, Color32, FontId, Order, Stroke, StrokeKind};
    let screen = egui_ctx.content_rect();
    if egui_ctx.input(|i| !i.raw.hovered_files.is_empty()) {
        let painter = egui_ctx.layer_painter(egui::LayerId::new(
            Order::Foreground,
            egui::Id::new("drop_overlay"),
        ));
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
        painter.rect_stroke(
            screen.shrink(14.0),
            14.0,
            Stroke::new(2.0, theme::ACCENT),
            StrokeKind::Inside,
        );
        painter.text(
            screen.center(),
            Align2::CENTER_CENTER,
            "Drop files to import them",
            FontId::proportional(24.0),
            theme::ACCENT,
        );
    }
    draw_toast(egui_ctx, ctx.state);
}

/// The short message at the bottom of the window (shown for 5 seconds).
pub fn draw_toast(egui_ctx: &egui::Context, state: &mut EditorState) {
    use egui::{Color32, FontId, Order, Stroke, StrokeKind};
    let screen = egui_ctx.content_rect();
    if let Some((message, when)) = &state.toast {
        if when.elapsed() > Duration::from_secs(5) {
            state.toast = None;
        } else {
            let painter =
                egui_ctx.layer_painter(egui::LayerId::new(Order::Tooltip, egui::Id::new("toast")));
            let pos = egui::pos2(screen.center().x, screen.bottom() - 40.0);
            let galley =
                painter.layout_no_wrap(message.clone(), FontId::proportional(14.0), theme::TEXT);
            let rect = egui::Rect::from_center_size(pos, galley.size() + egui::vec2(32.0, 18.0));
            painter.rect_filled(rect, 10.0, Color32::from_rgb(40, 40, 40));
            painter.rect_stroke(
                rect,
                10.0,
                Stroke::new(1.0, theme::ACCENT),
                StrokeKind::Inside,
            );
            painter.galley(rect.center() - galley.size() / 2.0, galley, theme::TEXT);
            egui_ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
