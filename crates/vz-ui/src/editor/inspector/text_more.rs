//! More of the Text tab: gradient fill, extra outlines, glow, curve, letter case, caption word effects, text animators,
//! and the look of the words selected in the text box.

use super::super::widgets::pick_color;
use super::{Ctx, change, key, row, section_title};
use crate::theme;
use eframe::egui::{self, RichText, vec2};
use std::sync::Arc;
use vz_anim::Easing;
use vz_core::{
    Clip, ClipId, Color, Content, Gradient, GradientKind, StyleSpan, TextAnimator, TextCase, TextGlow, TextPhase,
    TextStroke, TextUnit, WHITE, WordMode,
};

fn with_text(c: &mut Clip, f: impl FnOnce(&mut vz_core::TextContent)) {
    if let Content::Text(t) = &mut c.content {
        f(t);
    }
}

/// A slider for a number in the text of a clip.
#[allow(clippy::too_many_arguments)]
fn num(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    id: ClipId,
    merge: &str,
    label: &str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    apply: impl FnOnce(&mut vz_core::TextContent, f32),
) {
    row(ui, label, |ui| {
        let mut v = value;
        let resp = ui.add(
            egui::Slider::new(&mut v, range)
                .suffix(suffix)
                .max_decimals(2)
                .clamping(egui::SliderClamping::Never),
        );
        if resp.changed() {
            let _ = ctx.doc.update_clip(id, "Text", Some(key(id, merge)), |c| with_text(c, |t| apply(t, v)));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            ctx.doc.seal_history();
        }
    });
}

pub fn more_sections(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let Content::Text(content) = &clip.content else {
        return;
    };
    let id = clip.id;
    selection_section(ui, ctx, clip);
    ui.add_space(6.0);
    shape_section(ui, ctx, id, content);
    ui.add_space(6.0);
    gradient_section(ui, ctx, id, content);
    ui.add_space(6.0);
    strokes_section(ui, ctx, id, content);
    glow_section(ui, ctx, id, content);
    ui.add_space(6.0);
    words_section(ui, ctx, id, content);
    ui.add_space(6.0);
    animators_section(ui, ctx, id, content);
}

// ---- the selected words -----------------------------------------------------------------------------------------------

fn selection_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let Content::Text(content) = &clip.content else {
        return;
    };
    let id = clip.id;
    let Some((from, to)) = ctx.state.text_selection.filter(|(c, ..)| *c == id).map(|(_, a, b)| (a, b)) else {
        ui.label(RichText::new("Select some words in the text box to give them their own colour, size or font.").color(theme::MUTED).small());
        return;
    };
    let chars = content.text.chars().count();
    let (from, to) = (from.min(chars), to.min(chars));
    if from >= to {
        return;
    }
    let shown: String = content.text.chars().skip(from).take(to - from).take(24).collect();
    section_title(ui, &format!("Selected: \"{shown}\""));
    // the look the selection has now (from the last span that covers all of it)
    let current = content.spans.iter().rev().find(|s| s.start <= from && s.end >= to);
    ui.horizontal(|ui| {
        ui.label("Colour");
        let now = current.and_then(|s| s.color).unwrap_or(content.style.color);
        let apply = Arc::new(move |c: &mut Clip, col: Color| {
            with_text(c, |t| {
                t.spans.push(StyleSpan { start: from, end: to, color: Some(col), ..StyleSpan::default() });
            });
        });
        pick_color(ui, ctx, id, "span.color", now, apply);
        let bold = current.and_then(|s| s.bold).unwrap_or(content.style.bold);
        if ui.selectable_label(bold, RichText::new("B").strong()).clicked() {
            change(ctx, id, "Bold", move |c| with_text(c, |t| t.spans.push(StyleSpan { start: from, end: to, bold: Some(!bold), ..StyleSpan::default() })));
        }
        let italic = current.and_then(|s| s.italic).unwrap_or(content.style.italic);
        if ui.selectable_label(italic, RichText::new("I").italics()).clicked() {
            change(ctx, id, "Italic", move |c| with_text(c, |t| t.spans.push(StyleSpan { start: from, end: to, italic: Some(!italic), ..StyleSpan::default() })));
        }
    });
    egui::Grid::new("span_grid").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
        let size = current.and_then(|s| s.size).unwrap_or(1.0) * 100.0;
        num(ui, ctx, id, "span.size", "Size", size, 30.0..=300.0, " %", move |t, v| {
            t.spans.push(StyleSpan { start: from, end: to, size: Some(v / 100.0), ..StyleSpan::default() });
        });
    });
    ui.horizontal(|ui| {
        if ui.small_button("Clear the look of the selection").clicked() {
            change(ctx, id, "Clear look", move |c| {
                with_text(c, |t| {
                    t.spans.retain(|s| s.end <= from || s.start >= to);
                });
            });
        }
        if ui.small_button("Clear all word looks").clicked() {
            change(ctx, id, "Clear looks", |c| with_text(c, |t| t.spans.clear()));
        }
    });
}

// ---- case, curve ------------------------------------------------------------------------------------------------------

fn shape_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    section_title(ui, "Shape of the text");
    ui.horizontal(|ui| {
        for (label, case) in [("Aa", TextCase::Normal), ("AA", TextCase::Upper), ("aa", TextCase::Lower), ("Title", TextCase::Title)] {
            if ui.selectable_label(content.style.case == case, label).clicked() {
                change(ctx, id, "Letter case", move |c| with_text(c, |t| t.style.case = case));
            }
        }
        if ui
            .selectable_label(content.style.align == vz_core::TextAlign::Justify, "Justify")
            .on_hover_text("Stretch lines to the full width (needs Wrap at a width)")
            .clicked()
        {
            change(ctx, id, "Justify", |c| with_text(c, |t| t.style.align = vz_core::TextAlign::Justify));
        }
    });
    egui::Grid::new("text_curve").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
        num(ui, ctx, id, "text.curve", "Curve", content.style.curve, -100.0..=100.0, "", |t, v| t.style.curve = v);
    });
}

// ---- fill --------------------------------------------------------------------------------------------------------------

fn gradient_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    let style = &content.style;
    let mut on = style.gradient.is_some();
    if ui.checkbox(&mut on, RichText::new("Gradient fill").strong()).changed() {
        let colour = style.color;
        change(ctx, id, "Gradient", move |c| {
            with_text(c, |t| {
                t.style.gradient = on.then(|| Gradient::two(colour, [colour[0] / 2, colour[1] / 2, 255, 255], 90.0));
            });
        });
    }
    let Some(g) = &style.gradient else {
        return;
    };
    ui.horizontal(|ui| {
        for (label, kind) in [("Linear", GradientKind::Linear), ("Radial", GradientKind::Radial)] {
            if ui.selectable_label(g.kind == kind, label).clicked() {
                change(ctx, id, "Gradient kind", move |c| with_text(c, |t| {
                    if let Some(g) = &mut t.style.gradient {
                        g.kind = kind;
                    }
                }));
            }
        }
        let first = g.stops.first().map_or(WHITE, |s| s.1);
        let last = g.stops.last().map_or(WHITE, |s| s.1);
        let set_first = Arc::new(|c: &mut Clip, col: Color| with_text(c, |t| {
            if let Some(g) = &mut t.style.gradient {
                if let Some(s) = g.stops.first_mut() {
                    s.1 = col;
                }
            }
        }));
        let set_last = Arc::new(|c: &mut Clip, col: Color| with_text(c, |t| {
            if let Some(g) = &mut t.style.gradient {
                if let Some(s) = g.stops.last_mut() {
                    s.1 = col;
                }
            }
        }));
        pick_color(ui, ctx, id, "grad.first", first, set_first);
        pick_color(ui, ctx, id, "grad.last", last, set_last);
    });
    if g.kind == GradientKind::Linear {
        egui::Grid::new("text_grad").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
            num(ui, ctx, id, "grad.angle", "Angle", g.angle, 0.0..=360.0, "°", |t, v| {
                if let Some(g) = &mut t.style.gradient {
                    g.angle = v;
                }
            });
        });
    }
}

// ---- outlines and glow -------------------------------------------------------------------------------------------------

fn strokes_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    let more = &content.style.more_strokes;
    if content.style.stroke.is_none() {
        return; // extra outlines hang on the first one
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Extra outlines").strong());
        if ui.add_enabled(more.len() < 2, egui::Button::new("+ Add").small()).clicked() {
            change(ctx, id, "Add outline", |c| with_text(c, |t| t.style.more_strokes.push(TextStroke { color: [255, 255, 255, 255], width: 6.0 })));
        }
    });
    for (i, s) in more.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("Outline {}", i + 2));
            let apply = Arc::new(move |c: &mut Clip, col: Color| with_text(c, |t| {
                if let Some(st) = t.style.more_strokes.get_mut(i) {
                    st.color = col;
                }
            }));
            pick_color(ui, ctx, id, &format!("stroke{i}.color"), s.color, apply);
            let mut w = s.width;
            let resp = ui.add(egui::Slider::new(&mut w, 0.0..=40.0).suffix(" px").max_decimals(1));
            if resp.changed() {
                let _ = ctx.doc.update_clip(id, "Outline", Some(key(id, &format!("stroke{i}.width"))), |c| with_text(c, |t| {
                    if let Some(st) = t.style.more_strokes.get_mut(i) {
                        st.width = w;
                    }
                }));
            }
            if resp.drag_stopped() || resp.lost_focus() {
                ctx.doc.seal_history();
            }
            if ui.small_button("Remove").clicked() {
                change(ctx, id, "Remove outline", move |c| with_text(c, |t| {
                    t.style.more_strokes.remove(i);
                }));
            }
        });
    }
}

fn glow_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    let mut on = content.style.glow.is_some();
    if ui.checkbox(&mut on, RichText::new("Glow").strong()).changed() {
        change(ctx, id, "Glow", move |c| with_text(c, |t| {
            t.style.glow = on.then_some(TextGlow { color: [255, 200, 0, 255], radius: 20.0, strength: 0.9 });
        }));
    }
    let Some(g) = content.style.glow else {
        return;
    };
    egui::Grid::new("text_glow").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
        row(ui, "Colour", |ui| {
            let apply = Arc::new(|c: &mut Clip, col: Color| with_text(c, |t| {
                if let Some(g) = &mut t.style.glow {
                    g.color = col;
                }
            }));
            pick_color(ui, ctx, id, "glow.color", g.color, apply);
        });
        num(ui, ctx, id, "glow.radius", "Size", g.radius, 0.0..=80.0, " px", |t, v| {
            if let Some(g) = &mut t.style.glow {
                g.radius = v;
            }
        });
        num(ui, ctx, id, "glow.strength", "Strength", g.strength * 100.0, 0.0..=100.0, " %", |t, v| {
            if let Some(g) = &mut t.style.glow {
                g.strength = v / 100.0;
            }
        });
    });
}

// ---- caption word effects -----------------------------------------------------------------------------------------------

fn mode_name(m: &WordMode) -> &'static str {
    match m {
        WordMode::Off => "Off",
        WordMode::Karaoke { .. } => "Karaoke (words light up)",
        WordMode::Reveal { .. } => "Reveal (words appear)",
        WordMode::Single { .. } => "One word at a time",
        WordMode::Keywords { .. } => "Keywords stand out",
    }
}

fn words_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    section_title(ui, "Word effect");
    row_combo(ui, |ui| {
        egui::ComboBox::from_id_salt("word_mode")
            .selected_text(mode_name(&content.words))
            .width(190.0)
            .show_ui(ui, |ui| {
                let yellow = [255, 214, 10, 255];
                let options = [
                    WordMode::Off,
                    WordMode::Karaoke { highlight: yellow, scale: 1.0 },
                    WordMode::Reveal { highlight: None },
                    WordMode::Single { scale: 1.2 },
                    WordMode::Keywords { highlight: [255, 64, 64, 255], scale: 1.2, words: vec![0] },
                ];
                for m in options {
                    let same = std::mem::discriminant(&m) == std::mem::discriminant(&content.words);
                    if ui.selectable_label(same, mode_name(&m)).clicked() && !same {
                        change(ctx, id, "Word effect", move |c| with_text(c, |t| t.words = m));
                    }
                }
            });
    });
    match &content.words {
        WordMode::Off => {}
        WordMode::Karaoke { highlight, scale } => {
            let (h, s) = (*highlight, *scale);
            highlight_row(ui, ctx, id, h, s * 100.0, true);
        }
        WordMode::Keywords { highlight, scale, words } => {
            let (h, s) = (*highlight, *scale);
            highlight_row(ui, ctx, id, h, s * 100.0, true);
            ui.horizontal(|ui| {
                ui.label("Which words");
                let mut text = words.iter().map(|w| (w + 1).to_string()).collect::<Vec<_>>().join(", ");
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0).hint_text("for example 2, 5")).changed() {
                    let list: Vec<usize> = text.split([',', ' ']).filter_map(|p| p.trim().parse::<usize>().ok()).filter(|n| *n > 0).map(|n| n - 1).collect();
                    let _ = ctx.doc.update_clip(id, "Keywords", Some(key(id, "keywords")), |c| with_text(c, |t| {
                        if let WordMode::Keywords { words, .. } = &mut t.words {
                            *words = list;
                        }
                    }));
                }
            });
        }
        WordMode::Reveal { highlight } => {
            let mut on = highlight.is_some();
            if ui.checkbox(&mut on, "Colour for the words").changed() {
                change(ctx, id, "Word colour", move |c| with_text(c, |t| {
                    t.words = WordMode::Reveal { highlight: on.then_some([255, 214, 10, 255]) };
                }));
            }
            if let Some(h) = highlight {
                let apply = Arc::new(|c: &mut Clip, col: Color| with_text(c, |t| t.words = WordMode::Reveal { highlight: Some(col) }));
                pick_color(ui, ctx, id, "reveal.color", *h, apply);
            }
        }
        WordMode::Single { scale } => {
            let s = *scale;
            egui::Grid::new("single_grid").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
                num(ui, ctx, id, "single.scale", "Size", s * 100.0, 50.0..=250.0, " %", |t, v| t.words = WordMode::Single { scale: v / 100.0 });
            });
        }
    }
    if content.words != WordMode::Off {
        ui.label(RichText::new("Words are timed by their length across the clip. Cut or stretch the clip to change the speed.").color(theme::MUTED).small());
    }
}

fn row_combo(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(f);
}

fn highlight_row(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, colour: Color, scale_percent: f32, _both: bool) {
    ui.horizontal(|ui| {
        ui.label("Highlight");
        let apply = Arc::new(|c: &mut Clip, col: Color| with_text(c, |t| match &mut t.words {
            WordMode::Karaoke { highlight, .. } | WordMode::Keywords { highlight, .. } => *highlight = col,
            _ => {}
        }));
        pick_color(ui, ctx, id, "word.color", colour, apply);
    });
    egui::Grid::new("highlight_grid").num_columns(2).spacing(vec2(14.0, 6.0)).min_col_width(80.0).show(ui, |ui| {
        num(ui, ctx, id, "word.scale", "Bigger", scale_percent, 80.0..=200.0, " %", |t, v| match &mut t.words {
            WordMode::Karaoke { scale, .. } | WordMode::Keywords { scale, .. } => *scale = v / 100.0,
            _ => {}
        });
    });
}

// ---- animators ----------------------------------------------------------------------------------------------------------

fn unit_name(u: TextUnit) -> &'static str {
    match u {
        TextUnit::Character => "Letters",
        TextUnit::Word => "Words",
        TextUnit::Line => "Lines",
    }
}

fn phase_name(p: TextPhase) -> &'static str {
    match p {
        TextPhase::In => "In (at the start)",
        TextPhase::Out => "Out (at the end)",
        TextPhase::Loop => "Loop",
    }
}

fn animators_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, content: &vz_core::TextContent) {
    ui.horizontal(|ui| {
        section_title(ui, "Text animators");
        ui.menu_button(RichText::new("+ Add").small(), |ui| {
            let presets: [(&str, TextAnimator); 6] = [
                ("Letters fly up", TextAnimator::default()),
                ("Typewriter", TextAnimator { dy: 0.0, delay: 0.05, duration: 0.01, easing: Easing::Linear, ..TextAnimator::default() }),
                ("Words pop in", TextAnimator { unit: TextUnit::Word, dy: 0.0, scale: 0.0, delay: 0.12, duration: 0.3, easing: Easing::Named(vz_anim::Family::Back, vz_anim::Dir::Out), ..TextAnimator::default() }),
                ("Lines slide in", TextAnimator { unit: TextUnit::Line, dx: -120.0, dy: 0.0, delay: 0.2, duration: 0.5, ..TextAnimator::default() }),
                ("Wave", TextAnimator { phase: TextPhase::Loop, dy: -30.0, opacity: 1.0, delay: 0.08, duration: 1.0, easing: Easing::Linear, ..TextAnimator::default() }),
                ("Letters drop out", TextAnimator { phase: TextPhase::Out, dy: 60.0, delay: 0.05, duration: 0.4, ..TextAnimator::default() }),
            ];
            for (name, an) in presets {
                if ui.button(name).clicked() {
                    change(ctx, id, "Add animator", move |c| with_text(c, |t| t.animators.push(an)));
                    ui.close();
                }
            }
        });
    });
    if content.animators.is_empty() {
        ui.label(RichText::new("Move letters, words or lines one after another.").color(theme::MUTED).small());
    }
    for (i, an) in content.animators.iter().enumerate() {
        egui::Frame::new().fill(egui::Color32::from_rgb(24, 24, 24)).corner_radius(6).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("an_unit", id, i)).selected_text(unit_name(an.unit)).width(80.0).show_ui(ui, |ui| {
                    for u in [TextUnit::Character, TextUnit::Word, TextUnit::Line] {
                        if ui.selectable_label(an.unit == u, unit_name(u)).clicked() {
                            change(ctx, id, "Animator", move |c| with_text(c, |t| t.animators[i].unit = u));
                        }
                    }
                });
                egui::ComboBox::from_id_salt(("an_phase", id, i)).selected_text(phase_name(an.phase)).width(120.0).show_ui(ui, |ui| {
                    for p in [TextPhase::In, TextPhase::Out, TextPhase::Loop] {
                        if ui.selectable_label(an.phase == p, phase_name(p)).clicked() {
                            change(ctx, id, "Animator", move |c| with_text(c, |t| t.animators[i].phase = p));
                        }
                    }
                });
                if ui.small_button("Remove").clicked() {
                    change(ctx, id, "Remove animator", move |c| with_text(c, |t| {
                        t.animators.remove(i);
                    }));
                }
            });
            egui::Grid::new(("an_grid", id, i)).num_columns(2).spacing(vec2(14.0, 4.0)).min_col_width(80.0).show(ui, |ui| {
                let m = |n: &str| format!("an{i}.{n}");
                num(ui, ctx, id, &m("dx"), "Across", an.dx, -500.0..=500.0, " px", move |t, v| t.animators[i].dx = v);
                num(ui, ctx, id, &m("dy"), "Up / down", an.dy, -500.0..=500.0, " px", move |t, v| t.animators[i].dy = v);
                num(ui, ctx, id, &m("scale"), "Size", an.scale * 100.0, 0.0..=300.0, " %", move |t, v| t.animators[i].scale = v / 100.0);
                num(ui, ctx, id, &m("rot"), "Turn", an.rotation, -360.0..=360.0, "°", move |t, v| t.animators[i].rotation = v);
                num(ui, ctx, id, &m("op"), "Opacity", an.opacity * 100.0, 0.0..=100.0, " %", move |t, v| t.animators[i].opacity = v / 100.0);
                num(ui, ctx, id, &m("delay"), "Delay", an.delay, 0.0..=1.0, " s", move |t, v| t.animators[i].delay = v);
                num(ui, ctx, id, &m("dur"), "Takes", an.duration, 0.01..=3.0, " s", move |t, v| t.animators[i].duration = v);
                row(ui, "Easing", |ui| {
                    egui::ComboBox::from_id_salt(("an_ease", id, i)).selected_text(an.easing.label()).width(150.0).show_ui(ui, |ui| {
                        for e in Easing::presets() {
                            if ui.selectable_label(an.easing == e, e.label()).clicked() {
                                change(ctx, id, "Animator", move |c| with_text(c, |t| t.animators[i].easing = e));
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(4.0);
    }
    if !content.animators.is_empty() {
        ui.label(RichText::new("The first value is how far away each part starts (In), ends (Out) or swings (Loop).").color(theme::MUTED).small());
    }
}
