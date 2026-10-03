//! The Captions tab of the asset panel: the caption list, importing and exporting caption files, and caption styles.

use super::geometry::with_text;
use super::{Ctx, canvas::merge_key};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, RichText, Sense, Stroke, StrokeKind,
    TextureOptions, vec2,
};
use std::path::Path;
use vz_core::captions::{self, CaptionLine, Format};
use vz_core::{CaptionLook, ClipId, Content, TextCase, TextContent, US_PER_SEC};
use vz_presets::captions::{CaptionPreset, caption_presets, find};

/// How long a caption added by hand lasts.
const NEW_CAPTION_LENGTH: i64 = 2 * US_PER_SEC;

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    match ctx.state.sub_menu {
        0 => list(ui, ctx),
        1 => import(ui, ctx),
        2 => export(ui, ctx),
        _ => styles(ui, ctx),
    }
}

/// The look chosen on the Styles page (or the plain default), with how to cut captions and whether to use capitals.
fn current_look(ctx: &Ctx<'_>) -> (CaptionLook, Option<usize>) {
    match find(&ctx.state.caption_style) {
        Some(p) => {
            let mut look = p.look.clone();
            if p.uppercase {
                look.style.case = TextCase::Upper;
            }
            (look, p.words_per_caption)
        }
        None => (CaptionLook::default(), None),
    }
}

// ---- the list ---------------------------------------------------------------------------------------------------------

fn list(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.horizontal(|ui| {
        if ui.add(theme::primary_button("＋ Caption")).on_hover_text("Add a caption at the playhead").clicked() {
            let (look, _) = current_look(ctx);
            match ctx.doc.add_caption(ctx.state.playhead, NEW_CAPTION_LENGTH, "New caption", &look) {
                Ok(id) => {
                    ctx.state.selection = vec![id];
                    ctx.state.inspector_tab = super::inspector::Tab::Text;
                    ctx.state.focus_text = true;
                }
                Err(e) => ctx.state.say(format!("Could not add it: {e}")),
            }
        }
    });
    let Some(track) = ctx.doc.caption_track() else {
        ui.add_space(10.0);
        ui.label(RichText::new("No captions yet. Add one, import a file (Import page), or pick a style on the Styles page first.").color(theme::MUTED));
        return;
    };
    let lines = ctx.doc.caption_lines(track);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut ctx.state.find_text).hint_text("Find").desired_width(80.0));
        ui.add(egui::TextEdit::singleline(&mut ctx.state.replace_text).hint_text("Replace with").desired_width(80.0));
        if ui.small_button("Replace all").clicked() {
            let (find, replace) = (ctx.state.find_text.clone(), ctx.state.replace_text.clone());
            match ctx.doc.replace_in_captions(track, &find, &replace, false) {
                Ok(n) => ctx.state.say(format!("Changed {n} captions")),
                Err(e) => ctx.state.say(format!("Could not do that: {e}")),
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Move all by").color(theme::MUTED).small());
        ui.add(egui::DragValue::new(&mut ctx.state.shift_seconds).speed(0.05).suffix(" s").max_decimals(2));
        if ui.small_button("Move").clicked() {
            let delta = (f64::from(ctx.state.shift_seconds) * US_PER_SEC as f64) as i64;
            let _ = ctx.doc.shift_captions(track, delta);
        }
    });
    ui.add_space(6.0);
    let rate = ctx.doc.project.frame_rate;
    let playhead = ctx.state.playhead;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, (id, line)) in lines.iter().enumerate() {
            let now = playhead >= line.start && playhead < line.end;
            egui::Frame::new()
                .fill(if now { Color32::from_rgb(40, 34, 14) } else { Color32::from_rgb(24, 24, 24) })
                .corner_radius(6)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    let head = ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{}   {} → {}",
                                i + 1,
                                rate.timecode(line.start),
                                rate.timecode(line.end)
                            ))
                            .monospace()
                            .small()
                            .color(theme::MUTED),
                        )
                        .sense(Sense::click()),
                    );
                    if head.clicked() {
                        ctx.seek(line.start);
                        ctx.state.selection = vec![*id];
                    }
                    let mut text = line.text.clone();
                    let resp = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(1).desired_width(f32::INFINITY));
                    if resp.changed() {
                        let _ = ctx.doc.update_clip(*id, "Edit caption", Some(merge_key(*id, "caption-text")), |c| {
                            if let Content::Text(t) = &mut c.content {
                                t.text = text;
                                t.spans.clear();
                            }
                        });
                    }
                    if resp.lost_focus() {
                        ctx.doc.seal_history();
                    }
                    if resp.gained_focus() {
                        ctx.state.selection = vec![*id];
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(i + 1 < lines.len(), egui::Button::new("Join").small())
                            .on_hover_text("Join with the next caption")
                            .clicked()
                        {
                            let _ = ctx.doc.merge_caption_with_next(*id);
                        }
                        if ui
                            .small_button("Cut")
                            .on_hover_text("Cut in two at the playhead (if it is inside), otherwise in the middle")
                            .clicked()
                        {
                            cut(ctx, *id, line);
                        }
                        if ui.small_button("Delete").clicked() {
                            let _ = ctx.doc.delete_clips(&[*id]);
                        }
                    });
                });
            ui.add_space(4.0);
        }
    });
}

/// Cut a caption in two: at the playhead if it is inside the caption, otherwise where the text is half way.
fn cut(ctx: &mut Ctx<'_>, id: ClipId, line: &CaptionLine) {
    let inside = ctx.state.playhead > line.start && ctx.state.playhead < line.end;
    let chars = line.text.chars().count();
    // with the playhead, the text is cut at the matching share of its length
    let at_chars = if inside {
        let share = (ctx.state.playhead - line.start) as f64 / (line.end - line.start) as f64;
        // move to the next space so a word is not cut in two
        let target = (chars as f64 * share) as usize;
        let text: Vec<char> = line.text.chars().collect();
        (target..chars).find(|i| text[*i] == ' ').unwrap_or(target).max(1)
    } else {
        let text: Vec<char> = line.text.chars().collect();
        (chars / 2..chars).find(|i| text[*i] == ' ').unwrap_or(chars / 2).max(1)
    };
    let at = inside.then_some(ctx.state.playhead);
    if let Err(e) = ctx.doc.split_caption(id, at_chars, at) {
        ctx.state.say(format!("Could not cut it: {e}"));
    }
}

// ---- import -----------------------------------------------------------------------------------------------------------

fn import(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(RichText::new("Import captions from a file: SRT, VTT, LRC, ASS or a text file (one caption per line).").color(theme::MUTED).small());
    ui.add_space(8.0);
    let (look, from_style) = current_look(ctx);
    ui.horizontal(|ui| {
        ui.label("Style");
        ui.label(RichText::new(find(&ctx.state.caption_style).map_or("Plain", |p| p.name)).strong());
        ui.label(RichText::new("(change it on the Styles page)").color(theme::MUTED).small());
    });
    ui.horizontal(|ui| {
        ui.label("Words per caption");
        ui.add(egui::Slider::new(&mut ctx.state.import_words, 0..=10).custom_formatter(|v, _| if v == 0.0 { "as in the file".into() } else { format!("{v}") }));
    });
    ui.add_space(8.0);
    if ui.add(theme::primary_button("Choose a caption file…")).clicked()
        && let Some(path) = rfd::FileDialog::new().add_filter("Captions", &["srt", "vtt", "lrc", "ass", "ssa", "txt"]).pick_file()
    {
        let words = if ctx.state.import_words > 0 { Some(ctx.state.import_words) } else { from_style };
        match read_file(&path, ctx.doc.project.duration()) {
            Ok(lines) => {
                let lines = match words {
                    Some(n) => captions::split_by_words(&lines, n),
                    None => lines,
                };
                match ctx.doc.set_captions(&lines, &look) {
                    Ok((_, ids)) => {
                        ctx.state.say(format!("Imported {} captions", ids.len()));
                        ctx.state.sub_menu = 0;
                    }
                    Err(e) => ctx.state.say(format!("Could not add them: {e}")),
                }
            }
            Err(e) => ctx.state.say(format!("Could not read {}: {e}", path.display())),
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Importing replaces the captions that are on the timeline now (Ctrl+Z brings them back).").color(theme::MUTED).small());
}

fn read_file(path: &Path, total: i64) -> Result<Vec<CaptionLine>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    // most files are UTF-8; a UTF-16 file starts with a byte order mark
    let text = if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|b| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension)
        .ok_or("this kind of file is not supported")?;
    captions::parse(&text, format, total).map_err(|e| e.to_string())
}

// ---- export -----------------------------------------------------------------------------------------------------------

fn export(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let Some(track) = ctx.doc.caption_track() else {
        ui.label(RichText::new("There are no captions to export yet.").color(theme::MUTED));
        return;
    };
    let lines: Vec<CaptionLine> = ctx.doc.caption_lines(track).into_iter().map(|(_, l)| l).collect();
    ui.label(RichText::new(format!("{} captions on the timeline.", lines.len())).color(theme::MUTED));
    ui.add_space(8.0);
    for (label, ext) in [("Save as SRT…", "srt"), ("Save as WebVTT…", "vtt"), ("Save as text…", "txt")] {
        if ui.button(label).clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("{}.{ext}", ctx.doc.project.name))
                .add_filter(ext, &[ext])
                .save_file()
        {
            let text = match ext {
                "srt" => captions::write_srt(&lines),
                "vtt" => captions::write_vtt(&lines),
                _ => captions::write_txt(&lines),
            };
            match std::fs::write(&path, text) {
                Ok(()) => ctx.state.say(format!("Saved {}", path.display())),
                Err(e) => ctx.state.say(format!("Could not save: {e}")),
            }
        }
    }
}

// ---- styles -----------------------------------------------------------------------------------------------------------

fn styles(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.label(RichText::new("Click a style. It is used for new captions, and given to all captions on the timeline if the box below is ticked.").color(theme::MUTED).small());
    ui.checkbox(&mut ctx.state.captions_apply_all, "Apply to all captions");
    ui.add_space(6.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            for preset in caption_presets() {
                if tile(ui, ctx, preset).clicked() {
                    choose(ctx, preset);
                }
            }
        });
    });
}

fn choose(ctx: &mut Ctx<'_>, preset: &CaptionPreset) {
    ctx.state.caption_style = preset.id.to_owned();
    if !ctx.state.captions_apply_all {
        return;
    }
    let Some(track) = ctx.doc.caption_track() else {
        return;
    };
    let (look, words) = current_look(ctx);
    // quick looks show a few words at a time: cut the captions up to match
    let result = match words {
        Some(n) => {
            let lines: Vec<CaptionLine> = ctx.doc.caption_lines(track).into_iter().map(|(_, l)| l).collect();
            ctx.doc.set_captions(&captions::split_by_words(&lines, n), &look).map(|(_, ids)| ids.len())
        }
        None => ctx.doc.restyle_captions(track, &look),
    };
    match result {
        Ok(n) => ctx.state.say(format!("{} given to {n} captions", preset.name)),
        Err(e) => ctx.state.say(format!("Could not apply it: {e}")),
    }
}

fn sample_image(preset: &CaptionPreset, local: i64) -> egui::ColorImage {
    let mut look = preset.look.clone();
    if preset.uppercase {
        look.style.case = TextCase::Upper;
    }
    let text = TextContent {
        text: "Sample words".to_owned(),
        style: look.style.clone(),
        animators: look.animators.clone(),
        words: look.words.clone(),
        ..TextContent::default()
    };
    let factor = (26.0 / look.style.size).clamp(0.1, 1.0);
    let image = with_text(|e| e.render_at(&text, factor, local, 2 * US_PER_SEC));
    egui::ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba)
}

fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, preset: &CaptionPreset) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(108.0, 64.0 + 20.0), Sense::click());
    let card = egui::Rect::from_min_size(rect.min, vec2(108.0, 64.0));
    let key = format!("caption:{}", preset.id);
    let texture = match ctx.state.preset_tiles.get(&key) {
        Some(t) => t.clone(),
        None => {
            // show the middle of the caption so word effects are visible
            let tex = ui.ctx().load_texture(key.clone(), sample_image(preset, 1_100_000), TextureOptions::LINEAR);
            ctx.state.preset_tiles.insert(key, tex.clone());
            tex
        }
    };
    let painter = ui.painter();
    painter.rect_filled(card, CornerRadius::same(8), Color32::from_rgb(52, 62, 72));
    let size = texture.size_vec2();
    let k = ((108.0 - 10.0) / size.x).min((64.0 - 10.0) / size.y).min(1.0);
    painter.image(
        texture.id(),
        egui::Rect::from_center_size(card.center(), size * k),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    let chosen = ctx.state.caption_style == preset.id;
    if resp.hovered() || chosen {
        painter.rect_stroke(card, CornerRadius::same(8), Stroke::new(if chosen { 2.0 } else { 1.5 }, theme::ACCENT), StrokeKind::Inside);
    }
    painter.text(egui::pos2(rect.center().x, rect.bottom() - 8.0), Align2::CENTER_CENTER, preset.name, FontId::proportional(11.0), theme::TEXT);
    resp.on_hover_text(preset.name)
}

/// Read a caption file and put its captions on the timeline in the given style (an id from `vz-presets`; empty for plain).
pub fn import_file(doc: &mut vz_core::Document, path: &Path, style: &str) -> Result<usize, String> {
    let (mut look, words) = match find(style) {
        Some(p) => (p.look.clone(), p.words_per_caption),
        None => (CaptionLook::default(), None),
    };
    if find(style).is_some_and(|p| p.uppercase) {
        look.style.case = TextCase::Upper;
    }
    let mut lines = read_file(path, doc.project.duration())?;
    if let Some(n) = words {
        lines = captions::split_by_words(&lines, n);
    }
    doc.set_captions(&lines, &look).map(|(_, ids)| ids.len()).map_err(|e| e.to_string())
}
