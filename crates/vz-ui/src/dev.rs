//! Developer helpers, switched on by environment variables. They exist so the screen can be
//! checked automatically (take a picture, then quit) without touching the user's real settings.
//!
//! `VIDZCUT_SHOT=path.png`   save a screenshot after a few frames, then close
//! `VIDZCUT_START=editor`    skip the home screen
//! `VIDZCUT_LAYOUT=key`      `default`, `player_right` or `big_timeline`
//! `VIDZCUT_CATEGORY=n`      open asset tab number n (0 = Media)
//! `VIDZCUT_CONFIG_DIR=dir`  keep settings somewhere else
//! `VIDZCUT_IMPORT=a.mp4;b.mp4`  import these files at start and put them on the timeline
//! `VIDZCUT_SHOT_FRAME=n`    take the screenshot at frame n (default 10)
//! `VIDZCUT_PLAYHEAD=secs`   start with the playhead here
//! `VIDZCUT_AUTOEXPORT=dir`   export the imported files into this folder as soon as they are on the timeline
//! `VIDZCUT_PLAY=1`          start playing once the imported files are on the timeline

use crate::settings::LayoutPreset;
use eframe::egui;
use std::path::PathBuf;

#[derive(Default)]
pub struct DevHooks {
    pub shot: Option<PathBuf>,
    pub start_editor: bool,
    pub layout: Option<LayoutPreset>,
    pub category: Option<usize>,
    pub import: Vec<PathBuf>,
    pub playhead_secs: Option<f64>,
    pub play: bool,
    pub auto_export: Option<PathBuf>,
    pub imported: usize,
    shot_frame: u32,
    frames: u32,
}

impl DevHooks {
    pub fn from_env() -> Self {
        Self {
            shot: std::env::var_os("VIDZCUT_SHOT").map(PathBuf::from),
            start_editor: std::env::var("VIDZCUT_START").is_ok_and(|v| v == "editor"),
            layout: std::env::var("VIDZCUT_LAYOUT")
                .ok()
                .and_then(|k| LayoutPreset::from_key(&k)),
            category: std::env::var("VIDZCUT_CATEGORY")
                .ok()
                .and_then(|v| v.parse().ok()),
            import: std::env::var("VIDZCUT_IMPORT")
                .map(|v| {
                    v.split(';')
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default(),
            auto_export: std::env::var_os("VIDZCUT_AUTOEXPORT").map(PathBuf::from),
            play: std::env::var("VIDZCUT_PLAY").is_ok_and(|v| v == "1"),
            imported: 0,
            playhead_secs: std::env::var("VIDZCUT_PLAYHEAD")
                .ok()
                .and_then(|v| v.parse().ok()),
            shot_frame: std::env::var("VIDZCUT_SHOT_FRAME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            frames: 0,
        }
    }

    /// Call once per frame. Takes the screenshot and closes the window when asked to.
    pub fn after_frame(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else {
            return;
        };
        self.frames += 1;
        if self.frames == self.shot_frame {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(buf) =
                image::RgbaImage::from_raw(img.width() as u32, img.height() as u32, rgba)
            {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match buf.save(&path) {
                    Ok(()) => println!("screenshot saved: {}", path.display()),
                    Err(e) => eprintln!("could not save screenshot: {e}"),
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}
