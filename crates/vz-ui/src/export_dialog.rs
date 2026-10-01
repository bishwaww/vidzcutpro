//! The Export window: choose size and quality, watch progress, open the result.

use crate::settings::UiSettings;
use crate::theme;
use directories::UserDirs;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use vz_export::{ExportError, ExportReport, ExportSettings, Progress, Quality};

/// Sizes offered (the short side of the picture).
const SIZES: [(&str, u32); 5] = [
    ("480p", 480),
    ("720p", 720),
    ("1080p", 1080),
    ("2K", 1440),
    ("4K", 2160),
];

enum Stage {
    Choosing,
    Running {
        shared: Arc<Mutex<Progress>>,
        cancel: Arc<AtomicBool>,
        started: Instant,
        done: crossbeam_channel::Receiver<Result<ExportReport, ExportError>>,
    },
    Finished(ExportReport),
    Failed(String),
}

pub struct ExportDialog {
    stage: Stage,
    file_name: String,
    folder: PathBuf,
    size_index: usize,
    quality: Quality,
}

fn default_folder(settings: &UiSettings) -> PathBuf {
    settings
        .export_dir
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| UserDirs::new().and_then(|u| u.video_dir().map(Path::to_path_buf)))
        .or_else(|| UserDirs::new().map(|u| u.home_dir().to_path_buf()))
        .unwrap_or_default()
}

fn clean_name(name: &str) -> String {
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

impl ExportDialog {
    pub fn new(project_name: &str, settings: &UiSettings) -> Self {
        Self {
            stage: Stage::Choosing,
            file_name: clean_name(project_name),
            folder: default_folder(settings),
            size_index: 2, // 1080p
            quality: Quality::Recommended,
        }
    }

    /// Developer helper: export straight away into `folder` (used by the automatic screen checks).
    pub fn start_now_into(
        &mut self,
        folder: PathBuf,
        project: &vz_core::Project,
        settings: &mut UiSettings,
    ) {
        self.folder = folder;
        self.start(project, settings);
    }

    fn output_path(&self) -> PathBuf {
        let name = self.file_name.trim().trim_end_matches(".mp4");
        self.folder.join(format!("{}.mp4", clean_name(name)))
    }

    /// Draw the window. Returns true when it should be closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &vz_core::Project,
        settings: &mut UiSettings,
    ) -> bool {
        let mut close = false;
        let mut start = false;
        let mut cancel_requested = false;

        // collect a finished export
        if let Stage::Running { done, .. } = &self.stage
            && let Ok(result) = done.try_recv()
        {
            self.stage = match result {
                Ok(report) => Stage::Finished(report),
                Err(ExportError::Cancelled) => Stage::Choosing,
                Err(e) => Stage::Failed(e.to_string()),
            };
        }

        egui::Window::new("Export")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .min_width(420.0)
            .show(ctx, |ui| match &self.stage {
                Stage::Choosing => {
                    egui::Grid::new("export_grid")
                        .num_columns(2)
                        .spacing([16.0, 10.0])
                        .show(ui, |ui| {
                            ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.file_name)
                                    .desired_width(260.0),
                            );
                            ui.end_row();

                            ui.label("Save in");
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(self.folder.display().to_string()).small(),
                                );
                                if ui.small_button("Change…").clicked()
                                    && let Some(dir) = rfd::FileDialog::new()
                                        .set_directory(&self.folder)
                                        .pick_folder()
                                {
                                    self.folder = dir;
                                }
                            });
                            ui.end_row();

                            ui.label("Resolution");
                            egui::ComboBox::from_id_salt("export_size")
                                .selected_text(SIZES[self.size_index].0)
                                .show_ui(ui, |ui| {
                                    for (i, (label, _)) in SIZES.iter().enumerate() {
                                        ui.selectable_value(&mut self.size_index, i, *label);
                                    }
                                });
                            ui.end_row();

                            ui.label("Quality");
                            egui::ComboBox::from_id_salt("export_quality")
                                .selected_text(self.quality.label())
                                .show_ui(ui, |ui| {
                                    for q in Quality::ALL {
                                        ui.selectable_value(&mut self.quality, q, q.label());
                                    }
                                });
                            ui.end_row();

                            ui.label("Format");
                            ui.label(format!(
                                "MP4 (H.264 + AAC), {} fps",
                                project.frame_rate.label()
                            ));
                            ui.end_row();

                            ui.label("Estimated size");
                            let es = self.settings_for(project);
                            let bytes = (es.video_bit_rate() + 192_000) as f64 / 8.0
                                * (project.duration() as f64 / 1e6);
                            ui.label(format!(
                                "{:.1} MB  ({} × {})",
                                bytes / 1e6,
                                es.width,
                                es.height
                            ));
                            ui.end_row();
                        });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary_button("Export")).clicked() {
                            start = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }
                Stage::Running {
                    shared, started, ..
                } => {
                    let p = shared.lock().map(|g| *g).unwrap_or(Progress {
                        frames_done: 0,
                        frames_total: 1,
                    });
                    let fraction = p.frames_done as f32 / p.frames_total.max(1) as f32;
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .desired_width(380.0),
                    );
                    let elapsed = started.elapsed().as_secs_f64();
                    let eta = if p.frames_done > 8 {
                        elapsed / p.frames_done as f64 * (p.frames_total - p.frames_done) as f64
                    } else {
                        f64::NAN
                    };
                    let eta_text = if eta.is_finite() {
                        format!("about {} left", human_seconds(eta))
                    } else {
                        "working…".to_string()
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "Frame {} of {}  ·  {eta_text}",
                            p.frames_done, p.frames_total
                        ))
                        .color(theme::MUTED)
                        .small(),
                    );
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        cancel_requested = true;
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Stage::Finished(report) => {
                    ui.label(
                        egui::RichText::new("Export finished")
                            .strong()
                            .color(theme::ACCENT),
                    );
                    ui.label(format!(
                        "{:.1} MB, {} frames, {:.0} s using {}",
                        report.bytes as f64 / 1e6,
                        report.frames,
                        report.seconds,
                        report.encoder
                    ));
                    ui.label(
                        egui::RichText::new(report.path.display().to_string())
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary_button("Open folder")).clicked() {
                            reveal_in_folder(&report.path);
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }
                Stage::Failed(message) => {
                    ui.label(
                        egui::RichText::new("The export did not work")
                            .strong()
                            .color(egui::Color32::LIGHT_RED),
                    );
                    ui.label(message.as_str());
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });

        if cancel_requested && let Stage::Running { cancel, .. } = &self.stage {
            cancel.store(true, Ordering::Relaxed);
        }
        if start {
            self.start(project, settings);
        }
        // closing while running means cancelling
        if close && let Stage::Running { cancel, .. } = &self.stage {
            cancel.store(true, Ordering::Relaxed);
        }
        close
    }

    fn settings_for(&self, project: &vz_core::Project) -> ExportSettings {
        let mut s =
            ExportSettings::for_project(project, self.output_path(), SIZES[self.size_index].1);
        s.quality = self.quality;
        s
    }

    fn start(&mut self, project: &vz_core::Project, settings: &mut UiSettings) {
        if let Err(e) = std::fs::create_dir_all(&self.folder) {
            self.stage = Stage::Failed(format!("Could not use that folder: {e}"));
            return;
        }
        settings.export_dir = Some(self.folder.clone());
        settings.save();
        let export_settings = self.settings_for(project);
        let project = project.clone();
        let shared = Arc::new(Mutex::new(Progress {
            frames_done: 0,
            frames_total: 1,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded(1);
        {
            let (shared, cancel) = (shared.clone(), cancel.clone());
            std::thread::Builder::new()
                .name("vidzcut-export".into())
                .spawn(move || {
                    let result = vz_export::export(
                        &project,
                        &export_settings,
                        &mut |p| {
                            if let Ok(mut g) = shared.lock() {
                                *g = p;
                            }
                        },
                        &cancel,
                    );
                    let _ = tx.send(result);
                })
                .expect("could not start the export thread");
        }
        self.stage = Stage::Running {
            shared,
            cancel,
            started: Instant::now(),
            done: rx,
        };
    }
}

fn human_seconds(s: f64) -> String {
    let s = s.round() as u64;
    if s >= 60 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

/// Show a file in the system's file manager.
pub fn reveal_in_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    if let Err(e) = result {
        tracing::warn!("could not open the folder: {e}");
    }
}
