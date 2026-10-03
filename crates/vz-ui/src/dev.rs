//! Developer helpers, switched on by environment variables. They exist so the screen can be
//! checked automatically (take a picture, click things, then quit) without touching the user's real settings.
//!
//! `VIDZCUT_SHOT=path.png`    save a screenshot after a few frames, then close
//! `VIDZCUT_START=editor`     skip the home screen
//! `VIDZCUT_LAYOUT=key`       `default`, `player_right` or `big_timeline`
//! `VIDZCUT_CATEGORY=n`       open asset tab number n (0 = Media)
//! `VIDZCUT_CONFIG_DIR=dir`   keep settings somewhere else
//! `VIDZCUT_IMPORT=a.mp4;b.mp4`  import these files at start and put them on the timeline
//! `VIDZCUT_AUTOEXPORT=dir`   export the imported files into this folder as soon as they are on the timeline
//! `VIDZCUT_SHOT_FRAME=n`     take the screenshot at frame n (default 10)
//! `VIDZCUT_PLAYHEAD=secs`    start with the playhead here
//! `VIDZCUT_PLAY=1`           start playing once the imported files are on the timeline
//! `VIDZCUT_CAPTIONS=file.srt;style_id`  import this caption file at start, with a caption style (ids in vz-presets)
//! `VIDZCUT_SCRIPT=steps`     drive the window like a person: steps are separated by `;`
//!     `wait:N` (frames) · `move:x,y` · `click:x,y` · `rclick:x,y` · `drag:x1,y1,x2,y2` · `key:Ctrl+B` ·
//!     `shot:path.png` · `quit`. Positions are in egui points (screen pixels / display scale).

use crate::settings::LayoutPreset;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
enum Step {
    Wait(u32),
    Move(Pos2),
    Button {
        pos: Pos2,
        pressed: bool,
        button: PointerButton,
    },
    Key {
        key: Key,
        modifiers: Modifiers,
    },
    Shot(PathBuf),
    Quit,
}

#[derive(Default)]
struct Script {
    steps: VecDeque<Step>,
    waiting: u32,
    /// Where the next screenshot should be saved.
    pending_shot: Option<PathBuf>,
    quit: bool,
}

fn parse_pos(text: &str) -> Option<Pos2> {
    let (x, y) = text.split_once(',')?;
    Some(Pos2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn parse_key(text: &str) -> Option<(Key, Modifiers)> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            other => key = Key::from_name(&capitalise(other)),
        }
    }
    if modifiers.ctrl {
        modifiers.command = true;
    }
    key.map(|k| (k, modifiers))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

impl Script {
    fn parse(text: &str) -> Self {
        let mut script = Self::default();
        for raw in text.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (name, arg) = raw.split_once(':').unwrap_or((raw, ""));
            let left = |pos, pressed| Step::Button {
                pos,
                pressed,
                button: PointerButton::Primary,
            };
            match name {
                "wait" => script.steps.push_back(Step::Wait(arg.parse().unwrap_or(1))),
                "move" => {
                    if let Some(p) = parse_pos(arg) {
                        script.steps.push_back(Step::Move(p));
                    }
                }
                "click" => {
                    if let Some(p) = parse_pos(arg) {
                        script.steps.extend([
                            Step::Move(p),
                            Step::Wait(2),
                            left(p, true),
                            Step::Wait(2),
                            left(p, false),
                            Step::Wait(3),
                        ]);
                    }
                }
                "rclick" => {
                    if let Some(p) = parse_pos(arg) {
                        let right = |pressed| Step::Button {
                            pos: p,
                            pressed,
                            button: PointerButton::Secondary,
                        };
                        script.steps.extend([
                            Step::Move(p),
                            Step::Wait(2),
                            right(true),
                            Step::Wait(2),
                            right(false),
                            Step::Wait(3),
                        ]);
                    }
                }
                "drag" => {
                    let v: Vec<f32> = arg
                        .split(',')
                        .filter_map(|n| n.trim().parse().ok())
                        .collect();
                    if let [x1, y1, x2, y2] = v[..] {
                        let (a, b) = (Pos2::new(x1, y1), Pos2::new(x2, y2));
                        script.steps.extend([
                            Step::Move(a),
                            Step::Wait(2),
                            left(a, true),
                            Step::Wait(2),
                        ]);
                        for i in 1..=12 {
                            script
                                .steps
                                .push_back(Step::Move(a + (b - a) * (i as f32 / 12.0)));
                        }
                        script
                            .steps
                            .extend([Step::Wait(2), left(b, false), Step::Wait(3)]);
                    }
                }
                "key" => {
                    if let Some((key, modifiers)) = parse_key(arg) {
                        script
                            .steps
                            .extend([Step::Key { key, modifiers }, Step::Wait(3)]);
                    }
                }
                "shot" => script.steps.push_back(Step::Shot(PathBuf::from(arg))),
                "quit" => script.steps.push_back(Step::Quit),
                other => eprintln!("VIDZCUT_SCRIPT: unknown step '{other}'"),
            }
        }
        script
    }
}

#[derive(Default)]
pub struct DevHooks {
    pub shot: Option<PathBuf>,
    pub start_editor: bool,
    pub layout: Option<LayoutPreset>,
    pub category: Option<usize>,
    pub import: Vec<PathBuf>,
    pub playhead_secs: Option<f64>,
    pub auto_export: Option<PathBuf>,
    pub play: bool,
    pub captions: Option<(PathBuf, String)>,
    pub imported: usize,
    shot_frame: u32,
    frames: u32,
    script: Option<Script>,
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
            captions: std::env::var("VIDZCUT_CAPTIONS").ok().map(|v| {
                let (file, style) = v.split_once(';').unwrap_or((v.as_str(), ""));
                (PathBuf::from(file), style.to_owned())
            }),
            imported: 0,
            playhead_secs: std::env::var("VIDZCUT_PLAYHEAD")
                .ok()
                .and_then(|v| v.parse().ok()),
            shot_frame: std::env::var("VIDZCUT_SHOT_FRAME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            frames: 0,
            script: std::env::var("VIDZCUT_SCRIPT")
                .ok()
                .map(|s| Script::parse(&s)),
        }
    }

    /// True while a script or screenshot run is driving the window (so quitting does not ask "save your changes?").
    pub fn scripted(&self) -> bool {
        self.script.is_some() || self.shot.is_some()
    }

    /// Called before every frame: push the next scripted mouse or keyboard event into egui's input.
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        let Some(script) = &mut self.script else {
            return;
        };
        if script.pending_shot.is_some() {
            return; // wait until the screenshot has been taken before doing anything else
        }
        if script.waiting > 0 {
            script.waiting -= 1;
            return;
        }
        let Some(step) = script.steps.pop_front() else {
            return;
        };
        match step {
            Step::Wait(n) => script.waiting = n,
            Step::Move(pos) => raw.events.push(Event::PointerMoved(pos)),
            Step::Button {
                pos,
                pressed,
                button,
            } => {
                raw.events.push(Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            Step::Key { key, modifiers } => {
                raw.events.push(Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
                raw.events.push(Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: false,
                    repeat: false,
                    modifiers,
                });
            }
            Step::Shot(path) => script.pending_shot = Some(path),
            Step::Quit => {
                script.steps.clear();
                script.quit = true;
            }
        }
    }

    /// Call once per frame. Takes screenshots and closes the window when asked to.
    pub fn after_frame(&mut self, ctx: &egui::Context) {
        if let Some(script) = &mut self.script {
            if let Some(path) = script.pending_shot.clone() {
                match screenshot_in(ctx) {
                    Some(img) => {
                        save_png(&img, &path);
                        script.pending_shot = None;
                    }
                    None => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    )),
                }
            }
            let finished =
                script.steps.is_empty() && script.waiting == 0 && script.pending_shot.is_none();
            if finished && (script.quit || self.shot.is_none()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }

        let Some(path) = self.shot.clone() else {
            return;
        };
        self.frames += 1;
        if self.frames == self.shot_frame {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self
            .script
            .as_ref()
            .is_none_or(|s| s.pending_shot.is_none())
            && let Some(img) = screenshot_in(ctx)
        {
            save_png(&img, &path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn screenshot_in(ctx: &egui::Context) -> Option<Arc<egui::ColorImage>> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    })
}

fn save_png(img: &egui::ColorImage, path: &Path) {
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
    if let Some(buf) = image::RgbaImage::from_raw(img.width() as u32, img.height() as u32, rgba) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match buf.save(path) {
            Ok(()) => println!("screenshot saved: {}", path.display()),
            Err(e) => eprintln!("could not save screenshot: {e}"),
        }
    }
}
