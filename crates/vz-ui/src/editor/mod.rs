//! The editing screen: top bar, asset panel, player, inspector and timeline.

mod assets;
mod canvas;
mod caption_panel;
mod geometry;
mod inspector;
mod keyframes;
mod look_panel;
mod pen;
mod player;
mod scopes_window;
mod sticker_panel;
mod template_panel;
mod text_panel;
mod timeline;
mod topbar;
mod transition_panel;
mod widgets;

use crate::settings::{LayoutPreset, UiSettings};
use crate::theme;
use eframe::egui::{self, Key, Modifiers, Panel};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use vz_core::{ClipId, Document, MediaId, Micros, TimelineId};

pub use assets::{Category, short_duration};
pub use caption_panel::import_file as import_caption_file;
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
    /// How far the tracks are scrolled down, in screen points.
    pub scroll_y: f32,
    /// A time the dragged clip is sticking to (a guide line is drawn there).
    pub snap_guide: Option<Micros>,
    /// Width of the timeline lanes on screen, set while drawing (used by "Fit").
    pub lane_width: f32,
    pub playhead: Micros,
    pub playing: bool,
    pub selection: Vec<ClipId>,
    /// Which tab of the inspector is open for a selected clip.
    pub inspector_tab: inspector::Tab,
    /// Which kind of animation the Animation tab is showing.
    pub anim_kind: vz_presets::animation::AnimKind,
    /// What the mouse is doing on the picture in the player.
    pub canvas_drag: Option<canvas::CanvasDrag>,
    /// Guide lines to draw while a layer is being moved.
    pub guides: Vec<canvas::Guide>,
    pub show_thirds: bool,
    pub show_safe: bool,
    /// Font family names for the font list (filled the first time it is opened).
    pub fonts: Vec<String>,
    /// Put the cursor in the inspector's text box on the next frame.
    pub focus_text: bool,
    /// The words selected in the text box: (clip, first character, end).
    pub text_selection: Option<(ClipId, usize, usize)>,
    /// A timeline tab being renamed: which one and the name typed so far.
    pub renaming: Option<(TimelineId, String)>,
    /// The pen tool, while a path is being drawn or its points are being edited.
    pub pen: Option<pen::PenSession>,
    /// Waiting for a click on the picture to take a colour from it, and the colours used lately.
    pub eyedropper: Option<widgets::Eyedropper>,
    pub recent_colors: Vec<vz_core::Color>,
    /// Captions: the style chosen, find / replace boxes, the time shift, and words per caption for imports.
    pub caption_style: String,
    pub captions_apply_all: bool,
    pub find_text: String,
    pub replace_text: String,
    pub shift_seconds: f32,
    pub import_words: usize,
    /// The scopes window: open or not, which scope, the latest picture and the scope's texture.
    pub show_scopes: bool,
    pub scope: scopes_window::Scope,
    pub shown_frame: Option<std::sync::Arc<vz_media::VideoFrame>>,
    pub scope_texture: Option<egui::TextureHandle>,
    /// Which part of the Adjust tab is open, which colour band and which curve.
    pub adjust_part: inspector::AdjustPart,
    pub hsl_band: usize,
    pub curve_channel: usize,
    /// The Speed tab shows the curve editor (also when the speed has no curve yet).
    pub speed_curve_mode: bool,
    /// The transition being edited, named by the clip it follows.
    pub selected_transition: Option<ClipId>,
    /// The transition tile the mouse was on (its preview is playing).
    pub tile_hover: Option<String>,
    /// Clips copied with Ctrl+C or Ctrl+X.
    pub clipboard: Vec<vz_core::ClipboardItem>,
    /// Pictures of the text styles on the Text tab.
    pub preset_tiles: HashMap<String, egui::TextureHandle>,
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
            scroll_y: 0.0,
            snap_guide: None,
            lane_width: 800.0,
            playhead: 0,
            playing: false,
            selection: Vec::new(),
            inspector_tab: inspector::Tab::default(),
            anim_kind: vz_presets::animation::AnimKind::In,
            canvas_drag: None,
            guides: Vec::new(),
            show_thirds: false,
            show_safe: false,
            fonts: Vec::new(),
            focus_text: false,
            text_selection: None,
            renaming: None,
            pen: None,
            eyedropper: None,
            recent_colors: Vec::new(),
            caption_style: String::new(),
            captions_apply_all: true,
            find_text: String::new(),
            replace_text: String::new(),
            shift_seconds: 0.0,
            import_words: 0,
            show_scopes: false,
            scope: scopes_window::Scope::default(),
            shown_frame: None,
            scope_texture: None,
            adjust_part: inspector::AdjustPart::default(),
            hsl_band: 0,
            curve_channel: 0,
            speed_curve_mode: false,
            selected_transition: None,
            tile_hover: None,
            clipboard: Vec::new(),
            preset_tiles: HashMap::new(),
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
    pub waveforms: &'a HashMap<MediaId, std::sync::Arc<vz_audio::Waveform>>,
    pub missing: &'a HashSet<MediaId>,
    pub player_texture: Option<&'a egui::TextureHandle>,
    pub actions: Vec<Action>,
    pub saved_at: Option<String>,
}

impl Ctx<'_> {
    /// Open another timeline for editing (a tab, or the inside of a compound clip).
    pub fn open_timeline(&mut self, id: TimelineId) {
        if self.doc.project.timeline().id == id {
            return;
        }
        if self.doc.set_active_timeline(id) {
            self.state.selection.clear();
            self.state.selected_transition = None;
            self.state.canvas_drag = None;
            self.state.drag = None;
            self.state.pen = None;
            self.state.playing = false;
            self.state.playhead = 0;
            self.state.scroll_x = 0.0;
        }
    }

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
    scopes_window::show(ui.ctx(), ctx);
    sizes_changed
}

/// Keyboard shortcuts (the CapCut-style defaults from the spec). They are ignored while typing in a text box.
fn handle_keys(egui_ctx: &egui::Context, ctx: &mut Ctx<'_>) {
    if egui_ctx.egui_wants_keyboard_input() {
        return;
    }
    let (cmd, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
    let pressed = |mods: Modifiers, key: Key| egui_ctx.input_mut(|i| i.consume_key(mods, key));

    pen::keys(egui_ctx, ctx);
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
    // copy, cut and paste arrive as their own events (not as key presses) when typed on a real keyboard
    let (copy, cut, paste) = egui_ctx.input(|i| {
        let has = |f: fn(&egui::Event) -> bool| i.events.iter().any(f);
        (
            has(|e| matches!(e, egui::Event::Copy)),
            has(|e| matches!(e, egui::Event::Cut)),
            has(|e| matches!(e, egui::Event::Paste(_))),
        )
    });
    if copy || pressed(cmd, Key::C) {
        timeline::copy_selection(ctx);
    }
    if cut || pressed(cmd, Key::X) {
        timeline::cut_selection(ctx);
    }
    if paste || pressed(cmd, Key::V) {
        timeline::paste_at_playhead(ctx);
    }
    if pressed(Modifiers::ALT, Key::G) {
        timeline::make_compound(ctx);
    }
    if pressed(cmd | shift, Key::G) {
        let ids = ctx.state.selection.clone();
        let _ = ctx.doc.ungroup_clips(&ids);
    } else if pressed(cmd, Key::G) {
        let ids = ctx.state.selection.clone();
        let _ = ctx.doc.group_clips(&ids);
    }
    if pressed(cmd, Key::D) {
        timeline::duplicate_selection(ctx);
    }
    if pressed(Modifiers::NONE, Key::M) {
        timeline::toggle_marker(ctx);
    }
    // nudge the selected layer on the picture: Alt + arrows move 1 pixel, Alt + Shift + arrows 10 pixels
    for (key, dx, dy) in [
        (Key::ArrowLeft, -1.0, 0.0),
        (Key::ArrowRight, 1.0, 0.0),
        (Key::ArrowUp, 0.0, -1.0),
        (Key::ArrowDown, 0.0, 1.0),
    ] {
        for (mods, step) in [(Modifiers::ALT | shift, 10.0), (Modifiers::ALT, 1.0)] {
            if pressed(mods, key) {
                canvas::nudge(ctx, dx * step, dy * step);
            }
        }
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
    if pressed(Modifiers::NONE, Key::Escape) && ctx.state.eyedropper.take().is_none() {
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
