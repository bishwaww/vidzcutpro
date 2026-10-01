//! vidzcut: the app you double-click.

// On Windows, do not open a black console window next to the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use directories::ProjectDirs;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

/// Where logs go (`VIDZCUT_LOG_DIR` overrides, for tests).
fn log_dir() -> Option<PathBuf> {
    match std::env::var_os("VIDZCUT_LOG_DIR") {
        Some(d) => Some(PathBuf::from(d)),
        None => ProjectDirs::from("", "", "vidzcut").map(|d| d.data_local_dir().join("logs")),
    }
}

fn start_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,wgpu=warn,naga=warn,eframe=warn")
    });
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false);
    let file = log_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("vidzcut.log"))
            .ok()
    });
    match file {
        Some(f) => builder.with_writer(Mutex::new(f)).init(),
        None => builder.init(),
    }
}

fn main() {
    start_logging();
    tracing::info!("vidzcut {} starting", env!("CARGO_PKG_VERSION"));
    if let Err(e) = vz_ui::run() {
        tracing::error!("vidzcut stopped with an error: {e}");
        eprintln!("vidzcut stopped with an error: {e}");
        std::process::exit(1);
    }
    tracing::info!("vidzcut closed normally");
}
