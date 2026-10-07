//! The Settings window: general, storage, performance, appearance and keyboard shortcuts.

use crate::keymap::{self, Cmd, Combo, Keymap, Preset};
use crate::settings::UiSettings;
use crate::theme;
use eframe::egui::{self, Event, RichText};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Appearance,
    Performance,
    Storage,
    Shortcuts,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Appearance,
        Self::Performance,
        Self::Storage,
        Self::Shortcuts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Performance => "Performance",
            Self::Storage => "Storage",
            Self::Shortcuts => "Shortcuts",
        }
    }
}

/// What the window remembers while it is open.
#[derive(Default)]
pub struct SettingsState {
    pub open: bool,
    page: Page,
    /// The command whose new shortcut is being waited for.
    pub waiting_for_key: Option<Cmd>,
    message: Option<String>,
}

impl SettingsState {
    /// Open the window on the shortcuts page.
    pub fn show_shortcuts(&mut self) {
        self.open = true;
        self.page = Page::Shortcuts;
    }

    pub fn is_typing_a_shortcut(&self) -> bool {
        self.open && self.waiting_for_key.is_some()
    }
}

/// Draw the window if it is open. Returns true when a setting changed (so it should be saved).
pub fn show(ctx: &egui::Context, state: &mut SettingsState, settings: &mut UiSettings) -> bool {
    if !state.open {
        return false;
    }
    let mut changed = false;
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([560.0, 520.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for page in Page::ALL {
                    if ui
                        .selectable_label(state.page == page, page.label())
                        .clicked()
                    {
                        state.page = page;
                        state.waiting_for_key = None;
                        state.message = None;
                    }
                }
            });
            ui.separator();
            ui.add_space(4.0);
            match state.page {
                Page::General => changed |= general(ui, settings),
                Page::Appearance => changed |= appearance(ui, settings),
                Page::Performance => changed |= performance(ui, settings),
                Page::Storage => storage(ui),
                Page::Shortcuts => changed |= shortcuts(ui, ctx, state, settings),
            }
        });
    if !open {
        state.open = false;
        state.waiting_for_key = None;
    }
    changed
}

fn general(ui: &mut egui::Ui, settings: &mut UiSettings) -> bool {
    let mut changed = false;
    egui::Grid::new("settings_general")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Save a copy for crash recovery every");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut settings.prefs.autosave_secs)
                        .range(10..=600)
                        .speed(1.0)
                        .suffix(" seconds"),
                )
                .changed();
            ui.end_row();
            ui.label("Interface language");
            ui.label("English");
            ui.end_row();
        });
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "If vidzcut closes by accident, it offers to bring your project back from the last copy.",
        )
        .color(theme::MUTED)
        .small(),
    );
    changed
}

fn appearance(ui: &mut egui::Ui, settings: &mut UiSettings) -> bool {
    let mut changed = false;
    let mut percent = (settings.prefs.ui_scale * 100.0).round();
    egui::Grid::new("settings_appearance")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Size of text and buttons");
            if ui
                .add(
                    egui::Slider::new(&mut percent, 75.0..=150.0)
                        .suffix(" %")
                        .step_by(5.0),
                )
                .changed()
            {
                settings.prefs.ui_scale = percent / 100.0;
                changed = true;
            }
            ui.end_row();
            ui.label("Theme");
            ui.label("Dark");
            ui.end_row();
        });
    if ui.small_button("Back to 100 %").clicked() {
        settings.prefs.ui_scale = 1.0;
        changed = true;
    }
    changed
}

fn performance(ui: &mut egui::Ui, settings: &mut UiSettings) -> bool {
    let mut changed = false;
    changed |= ui
        .checkbox(
            &mut settings.prefs.hardware_encoder,
            "Use the graphics card to make videos when it can",
        )
        .changed();
    ui.label(
        RichText::new(
            "Faster exports. Turn it off if a video comes out wrong; vidzcut then uses the processor only.",
        )
        .color(theme::MUTED)
        .small(),
    );
    changed
}

// ---- storage ------------------------------------------------------------------------------------------------------------

/// Size of everything in a folder, in bytes.
fn folder_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                folder_size(&p)
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn storage(ui: &mut egui::Ui) {
    let base = crate::editor::audio_dir();
    let rows: [(&str, PathBuf, &str); 3] = [
        (
            "Voice-overs",
            base.join("recordings"),
            "Your recordings. They are your files: vidzcut never deletes them.",
        ),
        (
            "Extracted sound",
            base.join("extracted"),
            "Sound files made with \"Extract audio\".",
        ),
        (
            "Sound-effect cache",
            base.join("effects"),
            "Our built-in sound effects, made when first used. Safe to clear.",
        ),
    ];
    egui::Grid::new("settings_storage")
        .num_columns(3)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            for (name, path, _) in &rows {
                ui.label(*name);
                ui.label(megabytes(folder_size(path)));
                if ui.small_button("Open folder").clicked() {
                    let _ = std::fs::create_dir_all(path);
                    crate::export_dialog::open_folder(path);
                }
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    for (name, _, note) in &rows {
        ui.label(
            RichText::new(format!("{name}: {note}"))
                .color(theme::MUTED)
                .small(),
        );
    }
    ui.add_space(8.0);
    if ui.button("Clear the sound-effect cache").clicked() {
        let _ = std::fs::remove_dir_all(base.join("effects"));
    }
}

// ---- shortcuts ----------------------------------------------------------------------------------------------------------

fn shortcuts(
    ui: &mut egui::Ui,
    egui_ctx: &egui::Context,
    state: &mut SettingsState,
    settings: &mut UiSettings,
) -> bool {
    let mut changed = false;
    let preset = Preset::from_id(&settings.shortcut_preset);
    ui.horizontal(|ui| {
        ui.label("Shortcut set");
        egui::ComboBox::from_id_salt("shortcut_preset")
            .selected_text(preset.label())
            .show_ui(ui, |ui| {
                for p in Preset::ALL {
                    if ui.selectable_label(p == preset, p.label()).clicked() && p != preset {
                        settings.shortcut_preset = p.id().to_string();
                        settings.shortcuts.clear();
                        state.message =
                            Some("Your own changes were cleared for the new set.".into());
                        changed = true;
                    }
                }
            });
        if ui
            .small_button("Reset all")
            .on_hover_text("Forget your own changes to this set")
            .clicked()
        {
            settings.shortcuts.clear();
            changed = true;
        }
    });
    ui.add_space(6.0);

    // a key press while waiting becomes the new shortcut
    if let Some(cmd) = state.waiting_for_key {
        let pressed = egui_ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, mods)) = pressed {
            state.waiting_for_key = None;
            if key == egui::Key::Escape && !mods.any() {
                state.message = Some("Cancelled.".into());
            } else {
                let combo = Combo {
                    command: mods.command,
                    shift: mods.shift,
                    alt: mods.alt,
                    key,
                };
                let keymap = Keymap::from_settings(settings);
                match keymap.used_by(combo, cmd) {
                    Some(other) => {
                        state.message = Some(format!(
                            "{} is already used for \"{}\". Change that one first.",
                            combo.text(),
                            other.label()
                        ));
                    }
                    None => {
                        keymap::set_override(settings, cmd, combo);
                        state.message = Some(format!("{} → {}", cmd.label(), combo.text()));
                        changed = true;
                    }
                }
            }
        }
    }

    let keymap = Keymap::from_settings(settings);
    egui::ScrollArea::vertical()
        .max_height(340.0)
        .show(ui, |ui| {
            egui::Grid::new("shortcut_grid")
                .num_columns(3)
                .spacing([16.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for cmd in Cmd::ALL {
                        ui.label(cmd.label());
                        let waiting = state.waiting_for_key == Some(cmd);
                        let text = if waiting {
                            "Press the keys…".to_string()
                        } else {
                            keymap.get(cmd).text()
                        };
                        if ui
                            .add_sized([150.0, 22.0], egui::Button::new(text).selected(waiting))
                            .on_hover_text("Click, then press the new keys (Esc cancels)")
                            .clicked()
                        {
                            state.waiting_for_key = if waiting { None } else { Some(cmd) };
                            state.message = None;
                        }
                        let own = settings.shortcuts.contains_key(cmd.id());
                        if ui
                            .add_enabled(own, egui::Button::new("Reset").small())
                            .clicked()
                        {
                            settings.shortcuts.remove(cmd.id());
                            changed = true;
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some(m) = &state.message {
        ui.add_space(6.0);
        ui.label(RichText::new(m).color(theme::ACCENT));
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Copy, cut and paste (Ctrl+C, X, V) and nudging with Alt + arrows stay as they are.",
        )
        .color(theme::MUTED)
        .small(),
    );
    changed
}
