//! "Check for updates": asks GitHub for the newest vidzcut release, downloads its installer and starts it.
//! Only runs when the user clicks the button. Uses `curl.exe`, which comes with Windows 10/11.

use crate::theme;
use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::{self, RichText};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const REPO: &str = "bishwaww/vidzcutpro";
/// Installers are only taken from this address.
const DOWNLOAD_PREFIX: &str = "https://github.com/bishwaww/vidzcutpro/releases/download/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub name: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    UpToDate,
    Newer(Release),
}

/// The version of this program (`VIDZCUT_FAKE_VERSION` pretends to be another one, for testing).
pub fn current_version() -> String {
    std::env::var("VIDZCUT_FAKE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned())
}

/// "v0.1.2" or "0.1.2" → (0, 1, 2).
fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches(['v', 'V']).split('.');
    let mut next = || {
        parts
            .next()?
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    };
    Some((next()?, next()?, next()?))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Reads GitHub's answer for "latest release". `Ok(None)` = nothing published yet.
fn parse_release(json: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad answer: {e}"))?;
    let Some(tag) = v["tag_name"].as_str() else {
        let message = v["message"].as_str().unwrap_or("unknown answer");
        return if message == "Not Found" {
            Ok(None)
        } else {
            Err(message.to_owned())
        };
    };
    let asset = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| n.ends_with("-setup.exe"))
        })
        .ok_or("the newest version has no installer file")?;
    let url = asset["browser_download_url"].as_str().unwrap_or_default();
    if !url.starts_with(DOWNLOAD_PREFIX) {
        return Err("the installer is not at the expected address".into());
    }
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_owned(),
        notes: v["body"].as_str().unwrap_or_default().trim().to_owned(),
        url: url.to_owned(),
        name: asset["name"]
            .as_str()
            .unwrap_or("vidzcut-setup.exe")
            .to_owned(),
        size: asset["size"].as_u64().unwrap_or(0),
    }))
}

fn curl() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new("curl.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no black window
    }
    cmd
}

/// Ask GitHub (blocking; run it in a thread).
fn check() -> Result<Check, String> {
    let out = curl()
        .args([
            "-s",
            "-L",
            "-m",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
        ])
        .arg("User-Agent: vidzcut")
        .arg(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .output()
        .map_err(|e| format!("could not start curl: {e}"))?;
    if !out.status.success() {
        return Err("could not reach github.com — is the internet on?".into());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(match parse_release(&text)? {
        Some(r) if is_newer(&r.version, &current_version()) => Check::Newer(r),
        _ => Check::UpToDate,
    })
}

fn update_folder() -> PathBuf {
    std::env::temp_dir().join("vidzcut-update")
}

/// Download the installer (blocking; run it in a thread). Returns where it was saved.
fn download(release: &Release) -> Result<PathBuf, String> {
    let dir = update_folder();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (part, done) = paths_for(&dir, release);
    let _ = std::fs::remove_file(&part);
    let status = curl()
        .args(["-s", "-L", "-f", "-m", "1800", "-o"])
        .arg(&part)
        .arg(&release.url)
        .status()
        .map_err(|e| format!("could not start curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err("the download did not finish".into());
    }
    let got = std::fs::metadata(&part).map_or(0, |m| m.len());
    if release.size > 0 && got != release.size {
        let _ = std::fs::remove_file(&part);
        return Err("the downloaded file is not complete".into());
    }
    let _ = std::fs::remove_file(&done);
    std::fs::rename(&part, &done).map_err(|e| e.to_string())?;
    Ok(done)
}

fn paths_for(dir: &Path, release: &Release) -> (PathBuf, PathBuf) {
    let name = release.name.replace(['/', '\\', ':'], "_");
    (dir.join(format!("{name}.part")), dir.join(name))
}

/// Run the installer (it replaces the installed program). The caller closes vidzcut afterwards.
pub fn start_installer(path: &Path) -> Result<(), String> {
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

type Job<T> = Receiver<Result<T, String>>;

fn run_in_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Job<T> {
    let (send, receive) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("update".into())
        .spawn(move || {
            let _ = send.send(work());
        });
    if spawned.is_err() {
        let (send, receive) = crossbeam_channel::bounded(1);
        let _ = send.send(Err("could not start a background job".to_owned()));
        return receive;
    }
    receive
}

enum State {
    Checking(Job<Check>),
    UpToDate,
    Newer(Release),
    Downloading(Release, Job<PathBuf>),
    Ready(Release, PathBuf),
    Failed(String),
}

pub enum Outcome {
    None,
    Close,
    /// Run this installer, then quit.
    Install(PathBuf),
}

pub struct UpdateDialog {
    state: State,
}

impl UpdateDialog {
    /// Opens and starts asking GitHub at once.
    pub fn new() -> Self {
        Self {
            state: State::Checking(run_in_thread(check)),
        }
    }

    fn download(&mut self, release: Release) {
        let job = {
            let release = release.clone();
            run_in_thread(move || download(&release))
        };
        self.state = State::Downloading(release, job);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        // move on when a background job finished
        let next = match &self.state {
            State::Checking(job) => poll(job).map(|r| match r {
                Ok(Check::UpToDate) => State::UpToDate,
                Ok(Check::Newer(release)) => State::Newer(release),
                Err(e) => State::Failed(e),
            }),
            State::Downloading(release, job) => poll(job).map(|r| match r {
                Ok(path) => State::Ready(release.clone(), path),
                Err(e) => State::Failed(e),
            }),
            _ => None,
        };
        if let Some(state) = next {
            self.state = state;
        }
        if matches!(self.state, State::Checking(_) | State::Downloading(..)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        let mut outcome = Outcome::None;
        let mut start_download = None;
        egui::Window::new("Update vidzcut")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!("You have version {}.", current_version()))
                        .color(theme::MUTED),
                );
                ui.add_space(8.0);
                match &self.state {
                    State::Checking(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Looking for a new version…");
                        });
                    }
                    State::UpToDate => {
                        ui.label("vidzcut is up to date. There is no newer version.");
                    }
                    State::Newer(release) => {
                        ui.label(
                            RichText::new(format!("Version {} is ready.", release.version))
                                .strong(),
                        );
                        if !release.notes.is_empty() {
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .max_height(140.0)
                                .show(ui, |ui| {
                                    ui.label(&release.notes);
                                });
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!(
                                "Download size: {:.0} MB. Your projects are not touched.",
                                release.size as f64 / 1_048_576.0
                            ))
                            .color(theme::MUTED),
                        );
                        ui.add_space(8.0);
                        if ui
                            .add(theme::primary_button("Download and install"))
                            .clicked()
                        {
                            start_download = Some(release.clone());
                        }
                    }
                    State::Downloading(release, _) => {
                        let (part, _) = paths_for(&update_folder(), release);
                        let got = std::fs::metadata(part).map_or(0, |m| m.len());
                        let share = if release.size > 0 {
                            (got as f32 / release.size as f32).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        ui.label(format!("Downloading version {}…", release.version));
                        ui.add(egui::ProgressBar::new(share).show_percentage());
                    }
                    State::Ready(release, path) => {
                        ui.label(format!("Version {} is downloaded.", release.version));
                        ui.add_space(4.0);
                        ui.label(
                            "vidzcut will close and the installer will open. Save your work first.",
                        );
                        ui.add_space(8.0);
                        if ui.add(theme::primary_button("Install now")).clicked() {
                            outcome = Outcome::Install(path.clone());
                        }
                    }
                    State::Failed(why) => {
                        ui.label(RichText::new("The update did not work.").strong());
                        ui.label(why);
                        ui.add_space(8.0);
                        if ui.button("Try again").clicked() {
                            self.state = State::Checking(run_in_thread(check));
                        }
                    }
                }
                ui.add_space(8.0);
                if !matches!(self.state, State::Downloading(..)) && ui.button("Close").clicked() {
                    outcome = Outcome::Close;
                }
            });
        if let Some(release) = start_download {
            self.download(release);
        }
        outcome
    }
}

fn poll<T>(job: &Job<T>) -> Option<Result<T, String>> {
    match job.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err("the job stopped unexpectedly".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_number() {
        assert_eq!(parse_version("v0.1.12"), Some((0, 1, 12)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("nonsense"), None);
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    const SAMPLE: &str = r#"{
        "tag_name": "v0.1.1",
        "body": "CapCut import, close goes home",
        "assets": [
          {"name": "notes.txt", "browser_download_url": "https://github.com/bishwaww/vidzcutpro/releases/download/v0.1.1/notes.txt", "size": 5},
          {"name": "vidzcut_0.1.1_x64-setup.exe", "browser_download_url": "https://github.com/bishwaww/vidzcutpro/releases/download/v0.1.1/vidzcut_0.1.1_x64-setup.exe", "size": 123}
        ]}"#;

    #[test]
    fn the_installer_is_found_in_a_release() {
        let r = parse_release(SAMPLE).unwrap().unwrap();
        assert_eq!(r.version, "0.1.1");
        assert_eq!(r.name, "vidzcut_0.1.1_x64-setup.exe");
        assert_eq!(r.size, 123);
        assert_eq!(r.notes, "CapCut import, close goes home");
    }

    #[test]
    fn no_release_yet_is_not_an_error() {
        assert_eq!(parse_release(r#"{"message":"Not Found"}"#), Ok(None));
        assert!(parse_release(r#"{"message":"API rate limit exceeded"}"#).is_err());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn installers_from_other_addresses_are_refused() {
        let bad = SAMPLE.replace("github.com/bishwaww", "example.com/someone");
        assert!(parse_release(&bad).is_err());
        let none = r#"{"tag_name":"v9.9.9","assets":[]}"#;
        assert!(parse_release(none).is_err());
    }
}
