//! The application: decides which screen is showing and keeps the open project.

use crate::dev::DevHooks;
use crate::editor::{self, EditorState};
use crate::home;
use crate::settings::UiSettings;
use crate::theme;
use eframe::egui;
use vz_core::Project;

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/brand/logo.png");
const MARK_PNG: &[u8] = include_bytes!("../../../assets/brand/mark.png");
const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/icon.png");

enum Screen {
    Home,
    Editor,
}

pub struct VidzApp {
    screen: Screen,
    project: Project,
    settings: UiSettings,
    editor: EditorState,
    textures: Option<(egui::TextureHandle, egui::TextureHandle)>,
    /// Bumped by "Reset layout" so egui forgets the panel sizes it had stored.
    layout_gen: u32,
    settings_dirty: bool,
    dev: DevHooks,
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
        Self {
            screen,
            project: Project::new("Untitled project"),
            settings,
            editor,
            textures: None,
            layout_gen: 0,
            settings_dirty: false,
            dev,
        }
    }
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

        match self.screen {
            Screen::Home => {
                if home::show(ui, &logo) == home::Action::CreateProject {
                    self.project = Project::new("Untitled project");
                    self.editor = EditorState::default();
                    self.screen = Screen::Editor;
                }
            }
            Screen::Editor => {
                let images = editor::Images { mark: &mark };
                let (action, sizes_changed) = editor::show(
                    ui,
                    &mut self.editor,
                    &mut self.project,
                    &mut self.settings,
                    self.layout_gen,
                    &images,
                );
                self.settings_dirty |= sizes_changed;
                match action {
                    editor::Action::None => {}
                    editor::Action::GoHome => self.screen = Screen::Home,
                    editor::Action::SetLayout(preset) => {
                        self.settings.layout = preset;
                        self.settings_dirty = true;
                    }
                    editor::Action::ResetLayout => {
                        self.settings.reset_layout(self.settings.layout);
                        self.layout_gen += 1;
                        self.settings_dirty = true;
                    }
                }
            }
        }

        // Save the remembered layout once the user lets go of the mouse (not on every pixel of a drag).
        if self.settings_dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.settings.save();
            self.settings_dirty = false;
        }
        self.dev.after_frame(&ctx);
    }
}
