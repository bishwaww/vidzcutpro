//! Developer helpers, switched on by environment variables. They exist so the screen can be
//! checked automatically (take a picture, then quit) without touching the user's real settings.
//!
//! `VIDZCUT_SHOT=path.png`   save a screenshot after a few frames, then close
//! `VIDZCUT_START=editor`    skip the home screen
//! `VIDZCUT_LAYOUT=key`      `default`, `player_right` or `big_timeline`
//! `VIDZCUT_CATEGORY=n`      open asset tab number n (0 = Media)
//! `VIDZCUT_CONFIG_DIR=dir`  keep settings somewhere else

use crate::settings::LayoutPreset;
use eframe::egui;
use std::path::PathBuf;

#[derive(Default)]
pub struct DevHooks {
    pub shot: Option<PathBuf>,
    pub start_editor: bool,
    pub layout: Option<LayoutPreset>,
    pub category: Option<usize>,
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
            frames: 0,
        }
    }

    /// Call once per frame. Takes the screenshot and closes the window when asked to.
    pub fn after_frame(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else {
            return;
        };
        self.frames += 1;
        if self.frames == 10 {
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
