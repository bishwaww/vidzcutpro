//! "My templates" in the Templates tab: save the selected text, shapes and effect layers as a template,
//! and put saved templates on the timeline later (in any project).

use super::{Ctx, audio_dir, inspector};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind, vec2,
};
use std::path::{Path, PathBuf};
use vz_core::{Background, Clip, Document, Project, TrackKind, UserTemplate};
use vz_render::Renderer;

const TILE_W: f32 = 200.0;
const TILE_H: f32 = TILE_W * 9.0 / 16.0;

thread_local! {
    static RENDERER: std::cell::RefCell<Renderer> = std::cell::RefCell::new(Renderer::new());
}

/// What the panel remembers while the editor is open.
#[derive(Default)]
pub struct PanelState {
    name: String,
    /// The templates on disk (loaded the first time the panel is shown, again after a change).
    loaded: Option<Vec<(PathBuf, UserTemplate)>>,
}

fn folder() -> PathBuf {
    audio_dir()
        .parent()
        .map_or_else(|| PathBuf::from("templates"), |p| p.join("templates"))
}

fn load_all() -> Vec<(PathBuf, UserTemplate)> {
    let Ok(entries) = std::fs::read_dir(folder()) else {
        return Vec::new();
    };
    let mut all: Vec<(PathBuf, UserTemplate)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let t = UserTemplate::from_json(&std::fs::read_to_string(&p).ok()?)?;
            Some((p, t))
        })
        .collect();
    all.sort_by_key(|(_, t)| t.name.to_lowercase());
    all
}

/// A file name for a template (no characters Windows does not allow, and not one that is taken).
fn free_path(name: &str) -> PathBuf {
    let clean: String = name
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let dir = folder();
    let mut path = dir.join(format!("{clean}.json"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{clean} ({n}).json"));
        n += 1;
    }
    path
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    if ctx.state.user_templates.loaded.is_none() {
        ctx.state.user_templates.loaded = Some(load_all());
    }

    ui.label(RichText::new("Save your own titles and designs").strong());
    ui.label(
        RichText::new("Select text, shapes or effect layers on the timeline, give them a name and save. Pictures and videos cannot be saved in a template.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ctx.state.user_templates.name)
                .hint_text("Name of the template")
                .desired_width(180.0),
        );
        let has_selection = !ctx.state.selection.is_empty();
        let named = !ctx.state.user_templates.name.trim().is_empty();
        if ui
            .add_enabled(
                has_selection && named,
                egui::Button::new(RichText::new("Save template").strong()),
            )
            .on_disabled_hover_text("Select clips and type a name first")
            .clicked()
        {
            save_selected(ctx);
        }
    });
    ui.add_space(8.0);

    let templates = ctx.state.user_templates.loaded.clone().unwrap_or_default();
    if templates.is_empty() {
        ui.label(RichText::new("You have no saved templates yet.").color(theme::MUTED));
        return;
    }
    ui.label(
        RichText::new("Click one to add it at the playhead. Right-click to delete it.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(4.0);
    let search = ctx.state.search.to_lowercase();
    let mut add_this = None;
    let mut delete_this = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for (path, t) in templates
                    .iter()
                    .filter(|(_, t)| search.is_empty() || t.name.to_lowercase().contains(&search))
                {
                    let resp = tile(ui, ctx, path, t);
                    if resp.clicked() {
                        add_this = Some(t.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Delete this template").clicked() {
                            delete_this = Some(path.clone());
                            ui.close();
                        }
                    });
                }
            });
        });
    if let Some(t) = add_this {
        add(ctx, &t);
    }
    if let Some(path) = delete_this {
        let _ = std::fs::remove_file(&path);
        ctx.state
            .preset_tiles
            .remove(&format!("usertemplate:{}", path.display()));
        ctx.state.user_templates.loaded = None;
        ctx.state.say("Template deleted.");
    }
}

fn save_selected(ctx: &mut Ctx<'_>) {
    let timeline = ctx.doc.project.timeline();
    // bottom layer first: the order of the tracks
    let mut chosen: Vec<(TrackKind, &Clip)> = Vec::new();
    for track in &timeline.tracks {
        for clip in &track.clips {
            if ctx.state.selection.contains(&clip.id) {
                chosen.push((track.kind, clip));
            }
        }
    }
    let canvas = (ctx.doc.project.canvas.width, ctx.doc.project.canvas.height);
    let name = ctx.state.user_templates.name.clone();
    match UserTemplate::from_clips(&name, canvas, &chosen) {
        Ok((template, left_out)) => {
            let path = free_path(&template.name);
            let saved = std::fs::create_dir_all(folder())
                .and_then(|()| std::fs::write(&path, template.to_json()));
            match saved {
                Ok(()) => {
                    ctx.state.user_templates.name.clear();
                    ctx.state.user_templates.loaded = None;
                    ctx.state.say(if left_out > 0 {
                        format!(
                            "Template saved. {left_out} picture or video clip(s) were left out."
                        )
                    } else {
                        "Template saved.".to_string()
                    });
                }
                Err(e) => ctx.state.say(format!("Could not save the template: {e}")),
            }
        }
        Err(e) => ctx.state.say(e.to_string()),
    }
}

/// Put a saved template on the timeline at the playhead.
fn add(ctx: &mut Ctx<'_>, template: &UserTemplate) {
    let canvas = ctx.doc.project.canvas;
    let parts = template.build(canvas.width, canvas.height);
    match ctx.doc.add_group(parts, ctx.state.playhead, "Add template") {
        Ok(ids) => {
            let first_text = ids.iter().copied().find(|id| {
                ctx.doc
                    .project
                    .timeline()
                    .clip(*id)
                    .is_some_and(Clip::is_text)
            });
            if let Some(id) = first_text {
                ctx.state.selection = vec![id];
                ctx.state.inspector_tab = inspector::Tab::Text;
                ctx.state.focus_text = true;
            } else {
                ctx.state.selection = ids;
            }
        }
        Err(e) => ctx.state.say(format!("Could not add the template: {e}")),
    }
}

fn tile(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    path: &Path,
    template: &UserTemplate,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(TILE_W, TILE_H));
    let painter = ui.painter();
    painter.rect_filled(card, CornerRadius::same(8), Color32::from_rgb(30, 30, 30));
    if let Some(texture) = picture(ui.ctx(), ctx, path, template) {
        painter.image(
            texture.id(),
            card.shrink(3.0),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if resp.hovered() {
        painter.rect_stroke(
            card,
            CornerRadius::same(8),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        &template.name,
        FontId::proportional(11.0),
        theme::TEXT,
    );
    resp.on_hover_text(&template.name)
}

/// The template as it looks a little after it has come in, on a plain blue backdrop.
fn picture(
    egui_ctx: &egui::Context,
    ctx: &mut Ctx<'_>,
    path: &Path,
    template: &UserTemplate,
) -> Option<egui::TextureHandle> {
    let key = format!("usertemplate:{}", path.display());
    if let Some(t) = ctx.state.preset_tiles.get(&key) {
        return Some(t.clone());
    }
    let mut project = Project::new("tile");
    project.background = Background::Color([52, 66, 96, 255]);
    let mut doc = Document::new(project);
    doc.add_group(template.build(1920, 1080), 0, "tile").ok()?;
    let at = template.duration * 6 / 10;
    let (w, h) = (TILE_W as u32 * 2, TILE_H as u32 * 2);
    let frame = RENDERER.with(|r| r.borrow_mut().render(&doc.project, at, (w, h)));
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [frame.width as usize, frame.height as usize],
        &frame.rgba,
    );
    let texture = egui_ctx.load_texture(key.clone(), image, egui::TextureOptions::LINEAR);
    ctx.state.preset_tiles.insert(key, texture.clone());
    Some(texture)
}
