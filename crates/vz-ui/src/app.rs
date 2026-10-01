//! The application: decides which screen is showing, owns the open document, and connects the
//! editor to the background workers (thumbnails, preview, import).

use crate::dev::DevHooks;
use crate::editor::{self, Action, EditorState};
use crate::export_dialog::ExportDialog;
use crate::home::{self, RecentItem};
use crate::preview::Preview;
use crate::settings::UiSettings;
use crate::theme;
use directories::ProjectDirs;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vz_audio::AudioEngine;
use vz_core::{Document, EXTENSION, MediaId, Micros, Project, RecoveryItem};
use vz_media::{Event, MaxSize, MediaWorker, VideoFrame};

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/brand/logo.png");
const MARK_PNG: &[u8] = include_bytes!("../../../assets/brand/mark.png");
const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/icon.png");

/// How often unsaved work is copied to the recovery folder.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(30);

/// File types the import dialog offers.
const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mts", "m2ts", "ts", "hevc", "mp3", "wav", "aac",
    "m4a", "flac", "ogg", "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif",
];

enum Screen {
    Home,
    Editor,
}

/// A question shown on top of the window.
enum Dialog {
    /// Offer to bring back work from a crash.
    Recover(Vec<RecoveryItem>),
    /// The window was asked to close while there are unsaved changes.
    Quit,
}

pub struct VidzApp {
    screen: Screen,
    doc: Document,
    /// Goes up whenever `doc` is replaced, so the preview knows to start fresh.
    doc_serial: u64,
    settings: UiSettings,
    editor: EditorState,
    textures: Option<(egui::TextureHandle, egui::TextureHandle)>,
    /// Bumped by "Reset layout" so egui forgets the panel sizes it had stored.
    layout_gen: u32,
    settings_dirty: bool,
    dev: DevHooks,

    media: MediaWorker,
    preview: Preview,
    thumbs: HashMap<MediaId, egui::TextureHandle>,
    missing: HashSet<MediaId>,
    player_texture: Option<egui::TextureHandle>,
    /// What the preview was last asked to draw: (document, revision, time, size).
    last_request: Option<(u64, u64, Micros, (u32, u32))>,
    last_sent_project: Option<(u64, u64)>,

    /// The sound device, if one could be opened. Without it, playback uses the computer's clock.
    audio: Option<AudioEngine>,
    /// Which project state the sound engine has been told about: (document, revision).
    audio_project: Option<(u64, u64)>,
    play_origin: Option<(Instant, Micros)>,
    last_clock_playhead: Micros,

    last_autosave: Instant,
    last_autosave_revision: u64,
    saved_at: Option<String>,
    dialog: Option<Dialog>,
    export: Option<ExportDialog>,
    allow_close: bool,
}

fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("VIDZCUT_CONFIG_DIR") {
        Some(d) => Some(PathBuf::from(d)),
        None => ProjectDirs::from("", "", "vidzcut").map(|d| d.config_dir().to_path_buf()),
    }
}

fn recovery_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recovery"))
}

impl VidzApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let dev = DevHooks::from_env();
        let mut settings = UiSettings::load();
        if let Some(layout) = dev.layout {
            settings.layout = layout;
        }
        let mut editor = EditorState::default();
        if let Some(i) = dev.category {
            editor.category = editor::Category::ALL[i.min(editor::Category::ALL.len() - 1)];
        }
        let screen = if dev.start_editor {
            Screen::Editor
        } else {
            Screen::Home
        };

        let wake = {
            let ctx = cc.egui_ctx.clone();
            move || ctx.request_repaint()
        };
        let recovered = recovery_dir()
            .map(|d| Document::recovery_items(&d))
            .unwrap_or_default();
        let mut app = Self {
            screen,
            doc: Document::new(Project::new("Untitled project")),
            doc_serial: 1,
            settings,
            editor,
            textures: None,
            layout_gen: 0,
            settings_dirty: false,
            dev,
            media: MediaWorker::start(wake.clone()),
            preview: Preview::start(wake),
            thumbs: HashMap::new(),
            missing: HashSet::new(),
            player_texture: None,
            last_request: None,
            last_sent_project: None,
            audio: AudioEngine::start()
                .inspect_err(|e| tracing::warn!("playing without sound: {e}"))
                .ok(),
            audio_project: None,
            play_origin: None,
            last_clock_playhead: 0,
            last_autosave: Instant::now(),
            last_autosave_revision: 0,
            saved_at: None,
            dialog: (!recovered.is_empty()).then_some(Dialog::Recover(recovered)),
            export: None,
            allow_close: false,
        };
        // developer switch: import files at start-up
        for path in app.dev.import.clone() {
            app.media.import(path);
        }
        // developer switch: open a project at start-up (used to check the screen automatically)
        if let Some(path) = std::env::var_os("VIDZCUT_OPEN") {
            app.open_path(Path::new(&path));
        }
        app
    }

    // ---- documents ---------------------------------------------------------------------------------------

    fn set_document(&mut self, doc: Document) {
        self.doc = doc;
        self.doc_serial += 1;
        self.editor = EditorState::default();
        self.thumbs.clear();
        self.player_texture = None;
        self.last_request = None;
        self.play_origin = None;
        self.last_autosave_revision = self.doc.revision();
        self.saved_at = None;
        self.refresh_media_state();
        self.screen = Screen::Editor;
    }

    /// After a project is loaded: find missing files and ask for thumbnails.
    fn refresh_media_state(&mut self) {
        self.missing = self.doc.project.missing_media().into_iter().collect();
        for asset in &self.doc.project.media {
            if !self.missing.contains(&asset.id) {
                self.media
                    .poster(asset.id, asset.path.clone(), asset.kind, asset.duration);
            }
        }
    }

    fn new_project(&mut self) {
        self.set_document(Document::new(Project::new("Untitled project")));
    }

    fn open_path(&mut self, path: &Path) {
        match Document::open(path) {
            Ok(doc) => {
                self.settings.remember_recent(path);
                self.settings_dirty = true;
                self.set_document(doc);
                if !self.missing.is_empty() {
                    self.editor.say(format!("{} media file(s) could not be found. Right-click them in the Media tab to relink.", self.missing.len()));
                }
            }
            Err(e) => self.editor.say(format!("Could not open the project: {e}")),
        }
    }

    fn save(&mut self, save_as: bool) {
        let target = if save_as || self.doc.path.is_none() {
            let name = format!("{}.{EXTENSION}", sanitize(&self.doc.project.name));
            let mut dialog = rfd::FileDialog::new()
                .add_filter("vidzcut project", &[EXTENSION])
                .set_file_name(name);
            if let Some(dir) = self.doc.path.as_ref().and_then(|p| p.parent()) {
                dialog = dialog.set_directory(dir);
            }
            let Some(mut path) = dialog.save_file() else {
                return;
            };
            if path.extension().is_none() {
                path.set_extension(EXTENSION);
            }
            Some(path)
        } else {
            None
        };
        match self.doc.save(target.as_deref()) {
            Ok(()) => {
                if let Some(path) = self.doc.path.clone() {
                    self.settings.remember_recent(&path);
                    self.settings_dirty = true;
                }
                if let Some(dir) = recovery_dir() {
                    self.doc.discard_autosave(&dir);
                }
                self.saved_at = None;
                self.editor.say("Project saved");
            }
            Err(e) => self.editor.say(format!("Could not save: {e}")),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("vidzcut project", &[EXTENSION])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    fn import_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Media", MEDIA_EXTENSIONS)
            .pick_files()
        {
            for path in paths {
                self.media.import(path);
            }
        }
    }

    fn autosave_tick(&mut self) {
        if !matches!(self.screen, Screen::Editor) || self.last_autosave.elapsed() < AUTOSAVE_EVERY {
            return;
        }
        self.last_autosave = Instant::now();
        if !self.doc.is_dirty() || self.doc.revision() == self.last_autosave_revision {
            return;
        }
        if let Some(dir) = recovery_dir() {
            match self.doc.autosave(&dir) {
                Ok(()) => {
                    self.last_autosave_revision = self.doc.revision();
                    self.saved_at = Some(chrono::Local::now().format("%H:%M:%S").to_string());
                }
                Err(e) => tracing::warn!("autosave failed: {e}"),
            }
        }
    }

    // ---- background results ------------------------------------------------------------------------------------

    fn process_media_events(&mut self, ctx: &egui::Context) {
        for event in self.media.poll() {
            match event {
                Event::Imported { asset, poster } => {
                    let name = asset.name.clone();
                    match self.doc.import_media(asset) {
                        Ok(id) => {
                            if !self.dev.import.is_empty() {
                                // developer switch: lay imported files out on the timeline
                                let _ = self.doc.add_clip(id, usize::MAX);
                                if let Some(secs) = self.dev.playhead_secs {
                                    self.editor.playhead = (secs * 1_000_000.0) as Micros;
                                }
                                self.dev.imported += 1;
                                if self.dev.play && self.dev.imported == self.dev.import.len() {
                                    self.toggle_play();
                                }
                                if let Some(dir) = self.dev.auto_export.clone()
                                    && self.dev.imported == self.dev.import.len()
                                {
                                    let mut dialog =
                                        ExportDialog::new(&self.doc.project.name, &self.settings);
                                    dialog.start_now_into(
                                        dir,
                                        &self.doc.project,
                                        &mut self.settings,
                                    );
                                    self.export = Some(dialog);
                                }
                            }
                            if let Some(poster) = poster {
                                self.thumbs.insert(
                                    id,
                                    frame_texture(ctx, &format!("thumb-{id:?}"), &poster),
                                );
                            }
                            self.editor.say(format!("Imported {name}"));
                        }
                        Err(e) => self.editor.say(format!("Could not import {name}: {e}")),
                    }
                }
                Event::ImportFailed { path, error } => {
                    let name = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    self.editor.say(format!("Could not import {name}: {error}"));
                }
                Event::Poster { id, poster } => {
                    self.thumbs
                        .insert(id, frame_texture(ctx, &format!("thumb-{id:?}"), &poster));
                }
            }
        }
        if let Some(frame) = self.preview.take_new_frame() {
            match &mut self.player_texture {
                Some(tex) => tex.set(color_image(&frame), egui::TextureOptions::LINEAR),
                None => self.player_texture = Some(frame_texture(ctx, "player", &frame)),
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
            {
                self.open_path(&path);
            } else if matches!(self.screen, Screen::Editor) {
                self.media.import(path);
            }
        }
    }

    // ---- playback ---------------------------------------------------------------------------------------------------

    /// Tell the sound engine about the current project if it changed.
    fn sync_audio_project(&mut self) -> bool {
        let key = (self.doc_serial, self.doc.revision());
        let Some(audio) = &self.audio else {
            return false;
        };
        if self.audio_project != Some(key) {
            audio.set_project(Arc::new(self.doc.project.clone()));
            self.audio_project = Some(key);
            return true;
        }
        false
    }

    fn pause(&mut self) {
        self.editor.playing = false;
        self.play_origin = None;
        if let Some(audio) = &self.audio {
            audio.pause();
        }
    }

    fn toggle_play(&mut self) {
        if self.editor.playing {
            self.pause();
            return;
        }
        let end = self.doc.project.duration();
        if end == 0 {
            return;
        }
        if self.editor.playhead >= end {
            self.editor.playhead = 0;
        }
        self.editor.playing = true;
        self.play_origin = Some((Instant::now(), self.editor.playhead));
        self.last_clock_playhead = self.editor.playhead;
        self.sync_audio_project();
        if let Some(audio) = &self.audio {
            audio.play(self.editor.playhead);
        }
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.editor.playing {
            return;
        }
        let end = self.doc.project.duration();
        let now = Instant::now();
        // an edit while playing, or the user moved the playhead: restart the sound from the playhead
        let edited = self.sync_audio_project();
        let moved = self.editor.playhead != self.last_clock_playhead || self.play_origin.is_none();
        if let Some(audio) = &self.audio
            && (edited || moved)
        {
            audio.play(self.editor.playhead);
        }
        if moved || edited {
            self.play_origin = Some((now, self.editor.playhead));
        }
        // the sound card's clock is the master; without a sound device the computer's clock is used
        let t = match &self.audio {
            Some(audio) => audio.position(),
            None => {
                let (start_time, start_t) = self.play_origin.expect("set just above");
                start_t + now.duration_since(start_time).as_micros() as Micros
            }
        };
        let rate = self.doc.project.frame_rate;
        if t >= end {
            self.editor.playhead = end;
            self.pause();
        } else {
            self.editor.playhead = rate.frame_start(rate.frame_at(t.max(0)));
        }
        self.last_clock_playhead = self.editor.playhead;
        ctx.request_repaint();
    }

    /// Ask the preview thread for the picture at the playhead whenever something it shows has changed.
    fn sync_preview(&mut self) {
        let project_key = (self.doc_serial, self.doc.revision());
        let canvas = self.doc.project.canvas;
        let size = MaxSize::new(960, 960).fit(canvas.width, canvas.height);
        let key = (project_key.0, project_key.1, self.editor.playhead, size);
        if self.last_request == Some(key) {
            return;
        }
        if self.last_sent_project != Some(project_key) {
            self.preview.set_project(Arc::new(self.doc.project.clone()));
            self.last_sent_project = Some(project_key);
        }
        self.preview.request(self.editor.playhead, size);
        self.last_request = Some(key);
    }

    // ---- actions from the screens ------------------------------------------------------------------------------------

    fn run_action(&mut self, action: Action) {
        match action {
            Action::GoHome => {
                self.pause();
                self.screen = Screen::Home;
            }
            Action::SetLayout(preset) => {
                self.settings.layout = preset;
                self.settings_dirty = true;
            }
            Action::ResetLayout => {
                self.settings.reset_layout(self.settings.layout);
                self.layout_gen += 1;
                self.settings_dirty = true;
            }
            Action::ImportDialog => self.import_dialog(),
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
            Action::OpenDialog => self.open_dialog(),
            Action::TogglePlay => self.toggle_play(),
            Action::Export => {
                if self.doc.project.duration() == 0 {
                    self.editor.say("Add something to the timeline first.");
                } else {
                    self.pause();
                    self.export = Some(ExportDialog::new(&self.doc.project.name, &self.settings));
                }
            }
            Action::Relink(id) => self.relink(id),
        }
    }

    fn relink(&mut self, id: MediaId) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Media", MEDIA_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        match self.doc.relink_media(id, path.clone()) {
            Ok(()) => {
                self.missing.remove(&id);
                if let Some(asset) = self.doc.project.asset(id) {
                    self.media.poster(id, path, asset.kind, asset.duration);
                }
                self.last_request = None; // draw again with the file in its new place
            }
            Err(e) => self.editor.say(format!("Could not relink: {e}")),
        }
    }

    // ---- dialogs ----------------------------------------------------------------------------------------------------

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &self.dialog else { return };
        let mut choice = None;
        match dialog {
            Dialog::Recover(items) => {
                let item = items[0].clone();
                egui::Window::new("vidzcut closed unexpectedly")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Your project “{}” has changes that were not saved.",
                            item.name
                        ));
                        ui.label("Do you want to restore it?");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.add(theme::primary_button("Restore")).clicked() {
                                choice = Some(true);
                            }
                            if ui.button("Discard").clicked() {
                                choice = Some(false);
                            }
                        });
                    });
                if let Some(restore) = choice {
                    if restore {
                        match Document::restore(&item) {
                            Ok(doc) => self.set_document(doc),
                            Err(e) => self.editor.say(format!("Could not restore: {e}")),
                        }
                    }
                    let _ = std::fs::remove_file(&item.file);
                    // more than one leftover: ask about the next one
                    let rest: Vec<_> = recovery_dir()
                        .map(|d| Document::recovery_items(&d))
                        .unwrap_or_default();
                    self.dialog = (!rest.is_empty()).then_some(Dialog::Recover(rest));
                }
            }
            Dialog::Quit => {
                let mut action = None;
                egui::Window::new("Save your changes?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "“{}” has changes that are not saved.",
                            self.doc.project.name
                        ));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.add(theme::primary_button("Save")).clicked() {
                                action = Some(1);
                            }
                            if ui.button("Don't save").clicked() {
                                action = Some(2);
                            }
                            if ui.button("Cancel").clicked() {
                                action = Some(0);
                            }
                        });
                    });
                match action {
                    Some(1) => {
                        self.save(false);
                        if !self.doc.is_dirty() {
                            self.finish_closing(ctx);
                        } else {
                            self.dialog = None;
                        }
                    }
                    Some(2) => self.finish_closing(ctx),
                    Some(_) => self.dialog = None,
                    None => {}
                }
            }
        }
    }

    /// Clean exit: remove the recovery copy (there is nothing to recover) and close.
    fn finish_closing(&mut self, ctx: &egui::Context) {
        if let Some(dir) = recovery_dir() {
            self.doc.discard_autosave(&dir);
        }
        self.allow_close = true;
        self.dialog = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        if matches!(self.screen, Screen::Editor) && self.doc.is_dirty() && self.dev.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Quit);
        } else if let Some(dir) = recovery_dir() {
            self.doc.discard_autosave(&dir);
        }
    }

    fn recent_items(&self) -> Vec<RecentItem> {
        self.settings
            .recent
            .iter()
            .map(|path| {
                let exists = path.exists();
                let (mut name, mut duration) = (
                    path.file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                    String::new(),
                );
                if exists
                    && let Ok(text) = std::fs::read_to_string(path)
                    && let Ok(project) = Project::from_json(&text)
                {
                    name = project.name.clone();
                    duration = editor::short_duration(project.duration());
                }
                let modified = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .map(|t| {
                        chrono::DateTime::<chrono::Local>::from(t)
                            .format("%Y-%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                RecentItem {
                    path: path.clone(),
                    name,
                    duration,
                    modified,
                    exists,
                }
            })
            .collect()
    }
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "Untitled project".into()
    } else {
        cleaned
    }
}

fn color_image(frame: &VideoFrame) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    )
}

fn frame_texture(ctx: &egui::Context, name: &str, frame: &VideoFrame) -> egui::TextureHandle {
    ctx.load_texture(name, color_image(frame), egui::TextureOptions::LINEAR)
}

fn load_texture(ctx: &egui::Context, name: &str, png: &[u8]) -> egui::TextureHandle {
    let rgba = image::load_from_memory(png)
        .expect("built-in image is valid")
        .to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    )
}

/// The window icon, decoded from the vidzcut icon.
pub fn window_icon() -> egui::IconData {
    let rgba = image::load_from_memory(ICON_PNG)
        .expect("built-in icon is valid")
        .to_rgba8();
    egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}

impl eframe::App for VidzApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.textures.is_none() {
            self.textures = Some((
                load_texture(&ctx, "logo", LOGO_PNG),
                load_texture(&ctx, "mark", MARK_PNG),
            ));
        }
        let (logo, mark) = self.textures.clone().expect("textures were just created");

        self.handle_close_request(&ctx);
        self.process_media_events(&ctx);
        self.handle_dropped_files(&ctx);

        match self.screen {
            Screen::Home => {
                let recent = self.recent_items();
                match home::show(ui, &logo, &recent) {
                    home::Action::None => {}
                    home::Action::CreateProject => self.new_project(),
                    home::Action::OpenDialog => self.open_dialog(),
                    home::Action::Open(path) => self.open_path(&path),
                    home::Action::RemoveRecent(path) => {
                        self.settings.recent.retain(|p| *p != path);
                        self.settings_dirty = true;
                    }
                }
                editor::draw_toast(&ctx, &mut self.editor);
            }
            Screen::Editor => {
                self.advance_playback(&ctx);
                let mut ed = editor::Ctx {
                    doc: &mut self.doc,
                    state: &mut self.editor,
                    settings: &mut self.settings,
                    layout_gen: self.layout_gen,
                    images: editor::Images { mark: &mark },
                    thumbs: &self.thumbs,
                    missing: &self.missing,
                    player_texture: self.player_texture.as_ref(),
                    actions: Vec::new(),
                    saved_at: self.saved_at.clone(),
                };
                let sizes_changed = editor::show(ui, &mut ed);
                let actions = std::mem::take(&mut ed.actions);
                self.settings_dirty |= sizes_changed;
                for action in actions {
                    self.run_action(action);
                }
                // seeking past the end after edits keeps the playhead valid
                let end = self.doc.project.duration();
                if self.editor.playhead > end {
                    self.editor.playhead = end;
                }
                self.sync_preview();
                self.autosave_tick();
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }

        self.show_dialog(&ctx);
        if let Some(dialog) = &mut self.export
            && dialog.show(&ctx, &self.doc.project, &mut self.settings)
        {
            self.export = None;
        }

        // Save the remembered layout once the user lets go of the mouse (not on every pixel of a drag).
        if self.settings_dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.settings.save();
            self.settings_dirty = false;
        }
        self.dev.after_frame(&ctx);
    }
}
