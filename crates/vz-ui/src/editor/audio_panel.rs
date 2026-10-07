//! The Audio tab of the asset panel: music and sound effects from folders, our own sound effects,
//! extracting sound from a video, and recording a voice-over.

use super::{Action, Ctx};
use crate::theme;
use directories::ProjectDirs;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, vec2};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vz_audio::preview::Previewer;
use vz_audio::recorder::Recorder;
use vz_audio::sfx::{self, Sfx};
use vz_core::{MediaKind, Micros};

const EXTENSIONS: [&str; 9] = [
    "mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "wma", "aif",
];
const MAX_FILES: usize = 3000;

/// What the Audio tab remembers while the editor is open.
#[derive(Default)]
pub struct PanelState {
    previewer: Previewer,
    music: Vec<PathBuf>,
    effects: Vec<PathBuf>,
    scanned: bool,
    recorder: Option<Recorder>,
    /// Where on the timeline the recording will be placed.
    record_at: Micros,
    countdown: Option<Instant>,
    mics: Vec<String>,
    level: f32,
    play_along: bool,
    pub error: Option<String>,
}

/// Where we keep recordings, extracted sound and our own sound-effect files.
pub fn audio_dir() -> PathBuf {
    let base = match std::env::var_os("VIDZCUT_CONFIG_DIR") {
        Some(d) => PathBuf::from(d),
        None => ProjectDirs::from("", "", "vidzcut")
            .map_or_else(std::env::temp_dir, |d| d.data_local_dir().to_path_buf()),
    };
    base.join("audio")
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    if !ctx.state.audio_panel.scanned {
        rescan(ctx);
    }
    match ctx.state.sub_menu {
        0 => library(ui, ctx, false),
        1 => {
            builtin_effects(ui, ctx);
            ui.add_space(10.0);
            library(ui, ctx, true);
        }
        2 => extract(ui, ctx),
        _ => voiceover(ui, ctx),
    }
}

// ---- folders ----------------------------------------------------------------------------------------------------------

fn rescan(ctx: &mut Ctx<'_>) {
    let music = scan(&ctx.settings.music_dirs);
    let effects = scan(&ctx.settings.effect_dirs);
    let p = &mut ctx.state.audio_panel;
    p.music = music;
    p.effects = effects;
    p.scanned = true;
}

fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in dirs {
        walk(dir, 0, &mut found);
    }
    found.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if found.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth < 3 {
                walk(&path, depth + 1, found);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        {
            found.push(path);
        }
    }
}

/// The list of files from the user's folders (music, or sound effects).
fn library(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, effects: bool) {
    let title = if effects {
        "Your sound effects"
    } else {
        "Music"
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if ui.button("Add a folder…").clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_title(if effects {
                    "Choose a folder with sound effects"
                } else {
                    "Choose a folder with music"
                })
                .pick_folder()
        {
            let dirs = if effects {
                &mut ctx.settings.effect_dirs
            } else {
                &mut ctx.settings.music_dirs
            };
            if !dirs.contains(&dir) {
                dirs.push(dir);
                ctx.settings.save();
            }
            rescan(ctx);
        }
        if ui.small_button("↻ Refresh").clicked() {
            rescan(ctx);
        }
    });
    let dirs = if effects {
        ctx.settings.effect_dirs.clone()
    } else {
        ctx.settings.music_dirs.clone()
    };
    if dirs.is_empty() {
        ui.label(
            RichText::new("Choose a folder on your computer and its sound files show here.")
                .color(theme::MUTED)
                .small(),
        );
        return;
    }
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (i, d) in dirs.iter().enumerate() {
            let name = d.file_name().map_or_else(
                || d.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            if ui
                .small_button(format!("{name}  ×"))
                .on_hover_text(format!("{} (click to forget this folder)", d.display()))
                .clicked()
            {
                remove = Some(i);
            }
        }
    });
    if let Some(i) = remove {
        let list = if effects {
            &mut ctx.settings.effect_dirs
        } else {
            &mut ctx.settings.music_dirs
        };
        list.remove(i);
        ctx.settings.save();
        rescan(ctx);
        return;
    }
    ui.add_space(4.0);
    let files = if effects {
        ctx.state.audio_panel.effects.clone()
    } else {
        ctx.state.audio_panel.music.clone()
    };
    let needle = ctx.state.search.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt(("audio_files", effects))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut shown = 0;
            for path in files.iter().filter(|p| {
                needle.is_empty()
                    || p.file_name()
                        .is_some_and(|n| n.to_string_lossy().to_lowercase().contains(&needle))
            }) {
                shown += 1;
                let name = path
                    .file_stem()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                match file_row(ui, ctx, &name, path) {
                    RowAction::Add => add_file(ctx, path),
                    RowAction::Preview => preview(ctx, path, None),
                    RowAction::None => {}
                }
            }
            if shown == 0 {
                ui.label(RichText::new("No sound files found.").color(theme::MUTED));
            }
        });
}

enum RowAction {
    None,
    Preview,
    Add,
}

/// One line of a list: a play button, the name and a "+" button.
fn file_row(ui: &mut egui::Ui, ctx: &Ctx<'_>, name: &str, path: &Path) -> RowAction {
    let mut action = RowAction::None;
    let playing = ctx.state.audio_panel.previewer.playing() == Some(path);
    ui.horizontal(|ui| {
        let icon = if playing { "■" } else { "▶" };
        if ui
            .add_sized([26.0, 22.0], egui::Button::new(icon))
            .on_hover_text(if playing { "Stop" } else { "Listen" })
            .clicked()
        {
            action = RowAction::Preview;
        }
        let label = ui.add(
            egui::Label::new(RichText::new(name))
                .truncate()
                .sense(Sense::click()),
        );
        if label.double_clicked() {
            action = RowAction::Add;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_sized([26.0, 22.0], egui::Button::new("+"))
                .on_hover_text("Add to the timeline at the playhead")
                .clicked()
            {
                action = RowAction::Add;
            }
        });
    });
    action
}

fn preview(ctx: &mut Ctx<'_>, path: &Path, known_length: Option<Micros>) {
    let p = &mut ctx.state.audio_panel;
    if p.previewer.playing() == Some(path) {
        p.previewer.stop();
        return;
    }
    let length = known_length
        .or_else(|| vz_media::probe(path).ok().map(|a| a.duration))
        .unwrap_or(30_000_000);
    p.previewer.play(path, length);
}

/// Import a file and put it on an audio track at the playhead.
fn add_file(ctx: &mut Ctx<'_>, path: &Path) {
    match vz_media::probe(path) {
        Ok(asset) => {
            let result = ctx
                .doc
                .import_media(asset)
                .and_then(|id| ctx.doc.add_media_auto(id, ctx.state.playhead));
            match result {
                Ok(clip) => ctx.state.selection = vec![clip],
                Err(e) => ctx.state.say(format!("Could not add the sound: {e}")),
            }
        }
        Err(e) => ctx
            .state
            .say(format!("Could not read {}: {e}", path.display())),
    }
}

// ---- our own sound effects ----------------------------------------------------------------------------------------

/// The file of one of our sound effects (made the first time it is needed).
fn builtin_file(effect: Sfx) -> Option<PathBuf> {
    let dir = audio_dir().join("effects");
    let path = dir.join(format!("{}.wav", effect.slug()));
    if path.exists() {
        return Some(path);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let rate = 48_000;
    let mono = sfx::render(effect, rate);
    let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
    vz_audio::wav::write_wav(&path, rate, 2, &stereo).ok()?;
    Some(path)
}

fn builtin_effects(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(RichText::new("Built-in sound effects").strong());
    ui.label(
        RichText::new("Made by vidzcut itself, free to use anywhere.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(4.0);
    let needle = ctx.state.search.to_lowercase();
    for effect in Sfx::ALL {
        if !needle.is_empty() && !effect.label().to_lowercase().contains(&needle) {
            continue;
        }
        let path = audio_dir()
            .join("effects")
            .join(format!("{}.wav", effect.slug()));
        match file_row(ui, ctx, effect.label(), &path) {
            RowAction::Add => match builtin_file(effect) {
                Some(p) => add_file(ctx, &p),
                None => ctx.state.say("Could not make the sound file."),
            },
            RowAction::Preview => match builtin_file(effect) {
                Some(p) => preview(ctx, &p, Some((effect.seconds() * 1e6) as Micros)),
                None => ctx.state.say("Could not make the sound file."),
            },
            RowAction::None => {}
        }
    }
}

// ---- extract audio ------------------------------------------------------------------------------------------------------

fn extract(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(RichText::new("Extract audio from a video").strong());
    ui.label(
        RichText::new("Makes a separate sound file from a video in your project, so you can use it on its own.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(6.0);
    let videos: Vec<_> = ctx
        .doc
        .project
        .media
        .iter()
        .filter(|a| a.kind == MediaKind::Video && a.has_audio)
        .map(|a| (a.id, a.name.clone(), a.path.clone(), a.duration))
        .collect();
    if videos.is_empty() {
        ui.label(RichText::new("No videos with sound in this project yet.").color(theme::MUTED));
        return;
    }
    for (id, name, path, duration) in videos {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let busy = ctx.state.audio_jobs.extracting(id);
                if busy {
                    ui.spinner();
                } else if ui.button("Extract").clicked() {
                    let stem = Path::new(&name)
                        .file_stem()
                        .map_or_else(|| "audio".to_string(), |s| s.to_string_lossy().into_owned());
                    let out = audio_dir().join("extracted").join(format!(
                        "{stem}-audio-{}.wav",
                        chrono::Local::now().format("%H%M%S")
                    ));
                    ctx.state
                        .audio_jobs
                        .extract(ui.ctx(), id, path.clone(), duration, out);
                }
            });
        });
    }
}

// ---- voice-over ------------------------------------------------------------------------------------------------------------

fn voiceover(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(RichText::new("Record a voice-over").strong());
    ui.label(
        RichText::new("The recording goes on a new audio track where the playhead is.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(6.0);

    if ctx.state.audio_panel.mics.is_empty() {
        ctx.state.audio_panel.mics = vz_audio::recorder::input_devices();
    }
    let recording = ctx.state.audio_panel.recorder.is_some();
    let counting = ctx.state.audio_panel.countdown.is_some();

    ui.add_enabled_ui(!recording && !counting, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Microphone").color(theme::MUTED));
            let current = ctx
                .settings
                .mic
                .clone()
                .or_else(|| ctx.state.audio_panel.mics.first().cloned())
                .unwrap_or_else(|| "None found".to_string());
            let shown: String = if current.chars().count() > 26 {
                format!("{}…", current.chars().take(25).collect::<String>())
            } else {
                current.clone()
            };
            egui::ComboBox::from_id_salt("mic")
                .selected_text(shown)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for name in ctx.state.audio_panel.mics.clone() {
                        if ui.selectable_label(current == name, &name).clicked() {
                            ctx.settings.mic = Some(name);
                            ctx.settings.save();
                        }
                    }
                });
            if ui
                .small_button("↻")
                .on_hover_text("Look for microphones again")
                .clicked()
            {
                ctx.state.audio_panel.mics = vz_audio::recorder::input_devices();
            }
        });
        ui.checkbox(
            &mut ctx.state.audio_panel.play_along,
            "Play the project while recording (use headphones)",
        );
    });
    ui.add_space(8.0);

    // 3, 2, 1
    if let Some(started) = ctx.state.audio_panel.countdown {
        let left = 3.0 - started.elapsed().as_secs_f32();
        if left <= 0.0 {
            ctx.state.audio_panel.countdown = None;
            start_recording(ctx);
        } else {
            ui.label(
                RichText::new(format!("{}", left.ceil() as i32))
                    .size(54.0)
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }

    if recording {
        let (length, level) = {
            let p = &mut ctx.state.audio_panel;
            let l = p.recorder.as_ref().map_or(0.0, Recorder::level);
            p.level = p.level.max(l);
            (p.recorder.as_ref().map_or(0, Recorder::length), p.level)
        };
        meter(ui, level);
        ui.label(
            RichText::new(format!(
                "● Recording  {}",
                super::assets::short_duration(length)
            ))
            .strong()
            .color(Color32::from_rgb(230, 60, 60)),
        );
        ui.add_space(4.0);
        if ui
            .button(RichText::new("■  Stop and add").strong())
            .clicked()
        {
            stop_recording(ctx);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        ctx.state.audio_panel.level *= 0.85;
    } else if !counting {
        let can = !ctx.state.audio_panel.mics.is_empty();
        if ui
            .add_enabled(
                can,
                egui::Button::new(
                    RichText::new("●  Record")
                        .strong()
                        .color(Color32::from_rgb(230, 60, 60)),
                ),
            )
            .on_hover_text("Starts after a 3 second countdown")
            .clicked()
        {
            ctx.state.audio_panel.countdown = Some(Instant::now());
            ctx.state.audio_panel.error = None;
        }
        if !can {
            ui.label(RichText::new("No microphone found.").color(theme::MUTED));
        }
    } else if ui.button("Cancel").clicked() {
        ctx.state.audio_panel.countdown = None;
    }
    if let Some(e) = &ctx.state.audio_panel.error {
        ui.add_space(4.0);
        ui.label(RichText::new(e).color(Color32::from_rgb(230, 90, 90)));
    }
}

fn meter(ui: &mut egui::Ui, level: f32) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(260.0), 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, theme::PANEL_2);
    // a log-ish scale so quiet voices still move the bar
    let shown = (level.max(1e-4).log10() / 2.5 + 1.0).clamp(0.0, 1.0);
    let mut fill = rect;
    fill.set_right(rect.left() + rect.width() * shown);
    let colour = if level > 0.9 {
        Color32::from_rgb(230, 60, 60)
    } else {
        theme::ACCENT
    };
    ui.painter().rect_filled(fill, 3.0, colour);
    ui.painter().rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
}

fn start_recording(ctx: &mut Ctx<'_>) {
    let dir = audio_dir().join("recordings");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        ctx.state.audio_panel.error = Some(format!("Could not make the recordings folder: {e}"));
        return;
    }
    let path = dir.join(format!(
        "voiceover-{}.wav",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    let mic = ctx.settings.mic.clone();
    match Recorder::start(mic.as_deref(), &path) {
        Ok(r) => {
            let p = &mut ctx.state.audio_panel;
            p.recorder = Some(r);
            p.record_at = ctx.state.playhead;
            p.level = 0.0;
            if p.play_along && !ctx.state.playing && ctx.doc.project.duration() > 0 {
                ctx.actions.push(Action::TogglePlay);
            }
        }
        Err(e) => ctx.state.audio_panel.error = Some(format!("Could not start recording: {e}")),
    }
}

fn stop_recording(ctx: &mut Ctx<'_>) {
    let Some(recorder) = ctx.state.audio_panel.recorder.take() else {
        return;
    };
    if ctx.state.playing {
        ctx.actions.push(Action::TogglePlay);
    }
    match recorder.stop() {
        Ok((path, length)) if length > 100_000 => match vz_media::probe(&path) {
            Ok(asset) => {
                let at = ctx.state.audio_panel.record_at;
                let result = ctx
                    .doc
                    .import_media(asset)
                    .and_then(|id| ctx.doc.place_media_new_track(id, at));
                match result {
                    Ok((_, clip)) => {
                        ctx.state.selection = vec![clip];
                        ctx.state.say("Voice-over added on a new audio track.");
                    }
                    Err(e) => {
                        ctx.state.audio_panel.error =
                            Some(format!("Could not add the recording: {e}"))
                    }
                }
            }
            Err(e) => {
                ctx.state.audio_panel.error = Some(format!("Could not read the recording: {e}"))
            }
        },
        Ok(_) => ctx.state.say("That recording was too short to keep."),
        Err(e) => ctx.state.audio_panel.error = Some(format!("The recording failed: {e}")),
    }
}
