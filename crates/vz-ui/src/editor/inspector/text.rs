//! The Text tab of the inspector: the words, the font, and how the text looks.

use super::{Ctx, change, key, row, section_title, slider};
use crate::theme;
use eframe::egui::{self, Color32, RichText, vec2};
use vz_core::{
    BLACK, Clip, ClipId, Color, Content, TextAlign, TextBackground, TextShadow, TextStroke,
    TextStyle,
};

/// A style button: its letter, whether it is on, and what clicking it does.
type Toggle = (&'static str, bool, fn(&mut TextStyle));

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn from_color32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [r, g, b, a]
}

fn with_style(c: &mut Clip, f: impl FnOnce(&mut TextStyle)) {
    if let Content::Text(t) = &mut c.content {
        f(&mut t.style);
    }
}

/// A colour button. Changes while the picker is being dragged form one undo step.
fn color_button(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    id: ClipId,
    property: &'static str,
    color: Color,
    apply: impl FnOnce(&mut TextStyle, Color),
) {
    let mut c = to_color32(color);
    let resp = ui.color_edit_button_srgba(&mut c);
    if resp.changed() {
        let new = from_color32(c);
        let _ = ctx
            .doc
            .update_clip(id, property, Some(key(id, property)), |clip| {
                with_style(clip, |s| apply(s, new));
            });
    }
}

pub fn text_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let Content::Text(content) = &clip.content else {
        return;
    };
    let id = clip.id;
    let style = &content.style;

    section_title(ui, "Text");
    let mut words = content.text.clone();
    let edit = ui.add(
        egui::TextEdit::multiline(&mut words)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text("Type your text"),
    );
    if std::mem::take(&mut ctx.state.focus_text) {
        edit.request_focus();
    }
    if edit.changed() {
        let _ = ctx
            .doc
            .update_clip(id, "Edit text", Some(key(id, "words")), |c| {
                if let Content::Text(t) = &mut c.content {
                    t.text = words;
                }
            });
    }
    if edit.lost_focus() {
        ctx.doc.seal_history();
    }

    ui.add_space(6.0);
    section_title(ui, "Look");
    egui::Grid::new("text_look")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Font", |ui| font_picker(ui, ctx, id, &style.font));
            slider(
                ui,
                ctx,
                id,
                "text.size",
                "Size",
                style.size,
                8.0..=400.0,
                "",
                0,
                |c, v| {
                    with_style(c, |s| s.size = v);
                },
            );
            row(ui, "Style", |ui| {
                ui.horizontal(|ui| {
                    let toggles: [Toggle; 3] = [
                        ("B", style.bold, |s| s.bold = !s.bold),
                        ("I", style.italic, |s| s.italic = !s.italic),
                        ("U", style.underline, |s| s.underline = !s.underline),
                    ];
                    for (label, on, flip) in toggles {
                        let text = match label {
                            "B" => RichText::new(label).strong(),
                            "I" => RichText::new(label).italics(),
                            _ => RichText::new(label).underline(),
                        };
                        if ui.selectable_label(on, text).clicked() {
                            change(ctx, id, "Text style", |c| with_style(c, flip));
                        }
                    }
                    ui.add_space(8.0);
                    for (label, align) in [
                        ("Left", TextAlign::Left),
                        ("Centre", TextAlign::Center),
                        ("Right", TextAlign::Right),
                    ] {
                        if ui.selectable_label(style.align == align, label).clicked() {
                            change(ctx, id, "Text align", |c| {
                                with_style(c, |s| s.align = align)
                            });
                        }
                    }
                });
            });
            row(ui, "Colour", |ui| {
                color_button(ui, ctx, id, "text.color", style.color, |s, c| s.color = c);
            });
            slider(
                ui,
                ctx,
                id,
                "text.letters",
                "Letter space",
                style.letter_spacing * 100.0,
                -10.0..=100.0,
                " %",
                0,
                |c, v| with_style(c, |s| s.letter_spacing = v / 100.0),
            );
            slider(
                ui,
                ctx,
                id,
                "text.lines",
                "Line space",
                style.line_spacing * 100.0,
                50.0..=300.0,
                " %",
                0,
                |c, v| with_style(c, |s| s.line_spacing = v / 100.0),
            );
            row(ui, "Wrap", |ui| {
                let mut on = style.box_width.is_some();
                if ui.checkbox(&mut on, "Wrap at a width").changed() {
                    change(ctx, id, "Text wrap", |c| {
                        with_style(c, |s| s.box_width = on.then_some(900.0));
                    });
                }
            });
            if let Some(width) = style.box_width {
                slider(
                    ui,
                    ctx,
                    id,
                    "text.box",
                    "Width",
                    width,
                    100.0..=3000.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| s.box_width = Some(v));
                    },
                );
            }
        });

    ui.add_space(8.0);
    outline_section(ui, ctx, id, style);
    shadow_section(ui, ctx, id, style);
    background_section(ui, ctx, id, style);

    // a colour dragged in a picker is one undo step, however long the drag was
    if ui.input(|i| i.pointer.any_released()) {
        ctx.doc.seal_history();
    }
}

fn font_picker(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, current: &str) {
    if ctx.state.fonts.is_empty() {
        ctx.state.fonts = super::super::geometry::with_text(|e| e.families());
    }
    let shown = if current.is_empty() {
        vz_text::DEFAULT_FONT
    } else {
        current
    };
    egui::ComboBox::from_id_salt("text_font")
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, |ui| {
            let fonts = ctx.state.fonts.clone();
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (i, name) in fonts.iter().enumerate() {
                        if i == vz_text::BUNDLED_FAMILIES.len() {
                            ui.separator();
                            ui.label(
                                RichText::new("On this computer")
                                    .color(theme::MUTED)
                                    .small(),
                            );
                        }
                        if ui.selectable_label(shown == name, name).clicked() {
                            let name = name.clone();
                            change(ctx, id, "Font", |c| with_style(c, |s| s.font = name));
                        }
                    }
                });
        });
}

fn outline_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, style: &TextStyle) {
    let mut on = style.stroke.is_some();
    if ui
        .checkbox(&mut on, RichText::new("Outline").strong())
        .changed()
    {
        change(ctx, id, "Outline", |c| {
            with_style(c, |s| {
                s.stroke = on.then_some(TextStroke {
                    color: BLACK,
                    width: 6.0,
                });
            });
        });
    }
    if let Some(stroke) = style.stroke {
        egui::Grid::new("text_outline")
            .num_columns(2)
            .spacing(vec2(14.0, 8.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                row(ui, "Colour", |ui| {
                    color_button(ui, ctx, id, "text.outline.color", stroke.color, |s, c| {
                        if let Some(st) = &mut s.stroke {
                            st.color = c;
                        }
                    });
                });
                slider(
                    ui,
                    ctx,
                    id,
                    "text.outline.width",
                    "Width",
                    stroke.width,
                    0.0..=40.0,
                    " px",
                    1,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(st) = &mut s.stroke {
                                st.width = v;
                            }
                        });
                    },
                );
            });
    }
    ui.add_space(6.0);
}

fn shadow_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, style: &TextStyle) {
    let mut on = style.shadow.is_some();
    if ui
        .checkbox(&mut on, RichText::new("Shadow / glow").strong())
        .changed()
    {
        change(ctx, id, "Shadow", |c| {
            with_style(c, |s| {
                s.shadow = on.then_some(TextShadow {
                    color: [0, 0, 0, 200],
                    blur: 8.0,
                    dx: 4.0,
                    dy: 4.0,
                });
            });
        });
    }
    if let Some(shadow) = style.shadow {
        egui::Grid::new("text_shadow")
            .num_columns(2)
            .spacing(vec2(14.0, 8.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                row(ui, "Colour", |ui| {
                    color_button(ui, ctx, id, "text.shadow.color", shadow.color, |s, c| {
                        if let Some(sh) = &mut s.shadow {
                            sh.color = c;
                        }
                    });
                });
                slider(
                    ui,
                    ctx,
                    id,
                    "text.shadow.blur",
                    "Softness",
                    shadow.blur,
                    0.0..=80.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(sh) = &mut s.shadow {
                                sh.blur = v;
                            }
                        });
                    },
                );
                slider(
                    ui,
                    ctx,
                    id,
                    "text.shadow.dx",
                    "Across",
                    shadow.dx,
                    -60.0..=60.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(sh) = &mut s.shadow {
                                sh.dx = v;
                            }
                        });
                    },
                );
                slider(
                    ui,
                    ctx,
                    id,
                    "text.shadow.dy",
                    "Down",
                    shadow.dy,
                    -60.0..=60.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(sh) = &mut s.shadow {
                                sh.dy = v;
                            }
                        });
                    },
                );
            });
    }
    ui.add_space(6.0);
}

fn background_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, id: ClipId, style: &TextStyle) {
    let mut on = style.background.is_some();
    if ui
        .checkbox(&mut on, RichText::new("Background box").strong())
        .changed()
    {
        change(ctx, id, "Background", |c| {
            with_style(c, |s| {
                s.background = on.then_some(TextBackground {
                    color: [0, 0, 0, 170],
                    padding: 20.0,
                    radius: 10.0,
                });
            });
        });
    }
    if let Some(bg) = style.background {
        egui::Grid::new("text_background")
            .num_columns(2)
            .spacing(vec2(14.0, 8.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                row(ui, "Colour", |ui| {
                    color_button(ui, ctx, id, "text.bg.color", bg.color, |s, c| {
                        if let Some(b) = &mut s.background {
                            b.color = c;
                        }
                    });
                });
                slider(
                    ui,
                    ctx,
                    id,
                    "text.bg.padding",
                    "Padding",
                    bg.padding,
                    0.0..=120.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(b) = &mut s.background {
                                b.padding = v;
                            }
                        });
                    },
                );
                slider(
                    ui,
                    ctx,
                    id,
                    "text.bg.radius",
                    "Corners",
                    bg.radius,
                    0.0..=80.0,
                    " px",
                    0,
                    |c, v| {
                        with_style(c, |s| {
                            if let Some(b) = &mut s.background {
                                b.radius = v;
                            }
                        });
                    },
                );
            });
    }
}
