//! "Open a CapCut project": a list of the CapCut projects on this computer. Clicking one reads it (in the background)
//! and gives back a vidzcut project. CapCut's files are only read, never changed.

use crate::theme;
use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::{self, RichText};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use vz_import::{CapCutProject, ImportError, Imported};

type Reply = Result<Imported, ImportError>;

struct Working {
    name: String,
    reply: Receiver<Reply>,
}

pub enum Outcome {
    None,
    Close,
    Imported(Box<Imported>),
    /// What went wrong in plain words, and the details.
    Failed(String, String),
}

pub struct CapCutDialog {
    projects: Vec<CapCutProject>,
    filter: String,
    working: Option<Working>,
}

impl CapCutDialog {
    pub fn new() -> Self {
        Self {
            projects: vz_import::find_capcut_projects(),
            filter: String::new(),
            working: None,
        }
    }

    /// Start reading the project in `folder` (a project's folder or its `draft_content.json`).
    fn start(&mut self, folder: PathBuf, name: String) {
        let (send, reply) = crossbeam_channel::bounded(1);
        let (path, label) = (folder, name.clone());
        let spawned = std::thread::Builder::new()
            .name("capcut-import".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let result = vz_import::import_capcut(&path);
                tracing::info!("CapCut project {label} read in {:?}", started.elapsed());
                let _ = send.send(result);
            });
        if spawned.is_ok() {
            self.working = Some(Working { name, reply });
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        if let Some(work) = &self.working {
            match work.reply.try_recv() {
                Ok(Ok(imported)) => return Outcome::Imported(Box::new(imported)),
                Ok(Err(e)) => {
                    return Outcome::Failed(e.to_string(), format!("{} / {e:?}", work.name));
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(80)),
                Err(TryRecvError::Disconnected) => {
                    return Outcome::Failed(
                        "The project could not be read.".into(),
                        format!("{}: the reading stopped unexpectedly", work.name),
                    );
                }
            }
        }
        let busy = self.working.is_some();
        if busy {
            // nothing behind the window can be clicked while the project is being read
            egui::Area::new(egui::Id::new("capcut_block"))
                .order(egui::Order::Middle)
                .fixed_pos(egui::Pos2::ZERO)
                .show(ctx, |ui| {
                    ui.allocate_response(ctx.content_rect().size(), egui::Sense::click_and_drag());
                });
        }
        let mut outcome = Outcome::None;
        let mut chosen: Option<(PathBuf, String)> = None;
        egui::Window::new("Open a CapCut project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "vidzcut reads the project and opens it as a new vidzcut project. Your CapCut project is not changed.",
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(8.0);
                if let Some(work) = &self.working {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Reading “{}”…", work.name));
                    });
                    ui.add_space(8.0);
                    return;
                }
                ui.add_enabled_ui(!busy, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Search")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    let word = self.filter.to_lowercase();
                    let shown: Vec<&CapCutProject> = self
                        .projects
                        .iter()
                        .filter(|p| word.is_empty() || p.name.to_lowercase().contains(&word))
                        .collect();
                    if self.projects.is_empty() {
                        ui.label("vidzcut did not find CapCut projects on this computer. Use “Choose a folder…”.");
                    } else if shown.is_empty() {
                        ui.label("No project has this in its name.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for p in shown {
                                let line = format!("{}    {}", p.name, age(p.modified));
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::Button::selectable(false, line),
                                    )
                                    .clicked()
                                {
                                    chosen = Some((p.folder.clone(), p.name.clone()));
                                }
                            }
                        });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Choose a folder…").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if let Some(dir) = vz_import::capcut_projects_dir() {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(folder) = dialog.pick_folder() {
                            let name = folder
                                .file_name()
                                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                            chosen = Some((folder, name));
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Close;
                    }
                });
            });
        if let Some((folder, name)) = chosen {
            self.start(folder, name);
        }
        outcome
    }
}

/// "today", "yesterday", "5 days ago", "3 months ago".
fn age(when: SystemTime) -> String {
    let days = SystemTime::now()
        .duration_since(when)
        .map_or(0, |d| d.as_secs() / 86_400);
    match days {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..=59 => format!("{days} days ago"),
        _ => format!("{} months ago", days / 30),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_are_said_in_plain_words() {
        let ago = |days: u64| SystemTime::now() - Duration::from_secs(days * 86_400 + 60);
        assert_eq!(age(SystemTime::now()), "today");
        assert_eq!(age(ago(1)), "yesterday");
        assert_eq!(age(ago(9)), "9 days ago");
        assert_eq!(age(ago(95)), "3 months ago");
    }
}
