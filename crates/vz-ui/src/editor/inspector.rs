//! The inspector (right panel). With nothing selected it shows the project details; with a clip
//! selected it shows that clip's tabs (Video, Audio, Text, Details).

use super::assets::short_duration;
use super::geometry::{base_size, placed};
use super::{Ctx, topbar::NAME_TYPING_KEY};
use crate::theme;
use eframe::egui::{self, RichText, vec2};
use vz_core::{
    AspectPreset, Background, BlendMode, Clip, ClipId, Content, FrameRate, MediaKind, Micros,
};

/// The tabs a selected clip can show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Video,
    Audio,
    Text,
    Shape,
    Speed,
    Animation,
    Details,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Text => "Text",
            Self::Shape => "Shape",
            Self::Speed => "Speed",
            Self::Animation => "Animation",
            Self::Details => "Details",
        }
    }
}

/// Which tabs make sense for a clip.
fn tabs_for(ctx: &Ctx<'_>, clip: &Clip) -> Vec<Tab> {
    match &clip.content {
        Content::Text(_) => vec![Tab::Text, Tab::Video, Tab::Animation, Tab::Details],
        Content::Shape(_) => vec![Tab::Shape, Tab::Video, Tab::Animation, Tab::Details],
        Content::Media(m) => match ctx.doc.project.asset(*m).map(|a| (a.kind, a.has_audio)) {
            Some((MediaKind::Audio, _)) => vec![Tab::Audio, Tab::Speed, Tab::Details],
            Some((MediaKind::Video, true)) => {
                vec![
                    Tab::Video,
                    Tab::Audio,
                    Tab::Speed,
                    Tab::Animation,
                    Tab::Details,
                ]
            }
            Some((MediaKind::Video, false)) => {
                vec![Tab::Video, Tab::Speed, Tab::Animation, Tab::Details]
            }
            _ => vec![Tab::Video, Tab::Animation, Tab::Details],
        },
    }
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.add_space(8.0);
    let selected = (ctx.state.selection.len() == 1)
        .then(|| {
            ctx.doc
                .project
                .timeline()
                .clip(ctx.state.selection[0])
                .cloned()
        })
        .flatten();

    // the tab strip
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        match &selected {
            Some(clip) => {
                let tabs = tabs_for(ctx, clip);
                if !tabs.contains(&ctx.state.inspector_tab) {
                    ctx.state.inspector_tab = tabs[0];
                }
                for tab in tabs {
                    let active = ctx.state.inspector_tab == tab;
                    let text = RichText::new(tab.label()).strong().color(if active {
                        theme::ACCENT
                    } else {
                        theme::MUTED
                    });
                    let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                    if active {
                        ui.painter().hline(
                            resp.rect.x_range(),
                            resp.rect.bottom() + 3.0,
                            egui::Stroke::new(2.0, theme::ACCENT),
                        );
                    }
                    if resp.clicked() {
                        ctx.state.inspector_tab = tab;
                    }
                    ui.add_space(10.0);
                }
            }
            None => {
                let title =
                    if ctx.state.selected_transition.is_some() && ctx.state.selection.is_empty() {
                        "Transition"
                    } else {
                        "Details"
                    };
                let resp = ui.label(RichText::new(title).strong().color(theme::ACCENT));
                ui.painter().hline(
                    resp.rect.x_range(),
                    resp.rect.bottom() + 3.0,
                    egui::Stroke::new(2.0, theme::ACCENT),
                );
            }
        }
    });
    ui.add_space(4.0);
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 0)).show(ui, |ui| {
            ui.spacing_mut().slider_width = 150.0;
            match (&selected, ctx.state.selection.len()) {
                (Some(clip), _) => clip_tabs(ui, ctx, clip),
                (None, 0) => match ctx.state.selected_transition {
                    Some(after) => transition::transition_section(ui, ctx, after),
                    None => project_details(ui, ctx),
                },
                (None, n) => {
                    ui.label(RichText::new(format!("{n} clips selected")).strong());
                    ui.add_space(4.0);
                    ui.label(RichText::new("Select one clip to change its settings.\nPress Delete to remove them all.").color(theme::MUTED));
                }
            }
        });
    });
}

// ---- helpers --------------------------------------------------------------------------------------------------------

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(label).color(theme::MUTED));
    add(ui);
    ui.end_row();
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    ui.add_space(2.0);
}

mod animation;
mod shape;
mod speed;
mod text;
mod transition;

use super::keyframes::{self, Prop};

use super::canvas::merge_key as key;

/// A labelled slider (with a typed number) for one property of a clip.
#[allow(clippy::too_many_arguments)]
fn slider(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    clip: ClipId,
    property: &'static str,
    label: &str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    decimals: usize,
    apply: impl FnOnce(&mut Clip, f32),
) {
    row(ui, label, |ui| {
        let mut v = value;
        let resp = ui.add(
            egui::Slider::new(&mut v, range)
                .suffix(suffix)
                .max_decimals(decimals)
                .clamping(egui::SliderClamping::Never),
        );
        if resp.changed() {
            let _ = ctx
                .doc
                .update_clip(clip, property, Some(key(clip, property)), |c| apply(c, v));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            ctx.doc.seal_history();
        }
    });
}

/// Run a one-off change (a button) on a clip.
fn change(ctx: &mut Ctx<'_>, clip: ClipId, label: &'static str, f: impl FnOnce(&mut Clip)) {
    let _ = ctx.doc.update_clip(clip, label, None, f);
}

// ---- a selected clip ---------------------------------------------------------------------------------------------------

fn clip_tabs(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    match ctx.state.inspector_tab {
        Tab::Video => {
            transform_section(ui, ctx, clip);
            blend_section(ui, ctx, clip);
            crop_section(ui, ctx, clip);
        }
        Tab::Audio => audio_section(ui, ctx, clip),
        Tab::Text => text::text_section(ui, ctx, clip),
        Tab::Shape => shape::shape_section(ui, ctx, clip),
        Tab::Speed => speed::speed_section(ui, ctx, clip),
        Tab::Animation => animation::animation_section(ui, ctx, clip),
        Tab::Details => clip_details(ui, ctx, clip),
    }
}

fn transform_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let canvas = ctx.doc.project.canvas;
    ui.horizontal(|ui| {
        section_title(ui, "Transform");
        keyframe_nav(ui, ctx, clip, &Prop::TRANSFORM);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("↺ Reset")
                .on_hover_text("Put position, size and rotation back (removes their keyframes)")
                .clicked()
            {
                change(ctx, id, "Reset transform", |c| {
                    c.transform = vz_core::Transform::default()
                });
            }
        });
    });
    egui::Grid::new("transform_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            row(ui, "Scale", |ui| {
                let mut uniform = clip.transform.uniform_scale;
                if ui.checkbox(&mut uniform, "Keep shape").changed() {
                    change(ctx, id, "Scale", |c| {
                        c.transform.uniform_scale = uniform;
                        if uniform {
                            c.transform.scale_y = c.transform.scale_x.clone();
                        }
                    });
                }
            });
            let percent: Units = (|v| v * 100.0, |v| v / 100.0);
            if clip.transform.uniform_scale {
                anim_slider(
                    ui,
                    ctx,
                    clip,
                    &[Prop::ScaleX, Prop::ScaleY],
                    "",
                    1.0..=400.0,
                    " %",
                    0,
                    percent,
                );
            } else {
                anim_slider(
                    ui,
                    ctx,
                    clip,
                    &[Prop::ScaleX],
                    "Width",
                    1.0..=400.0,
                    " %",
                    0,
                    percent,
                );
                anim_slider(
                    ui,
                    ctx,
                    clip,
                    &[Prop::ScaleY],
                    "Height",
                    1.0..=400.0,
                    " %",
                    0,
                    percent,
                );
            }
            let (half_w, half_h) = (canvas.width as f32, canvas.height as f32);
            let same: Units = (|v| v, |v| v);
            anim_slider(
                ui,
                ctx,
                clip,
                &[Prop::X],
                "Position X",
                -half_w..=half_w,
                " px",
                0,
                same,
            );
            anim_slider(
                ui,
                ctx,
                clip,
                &[Prop::Y],
                "Position Y",
                -half_h..=half_h,
                " px",
                0,
                same,
            );
            anim_slider(
                ui,
                ctx,
                clip,
                &[Prop::Rotation],
                "Rotation",
                -180.0..=180.0,
                "°",
                0,
                same,
            );
            row(ui, "Mirror", |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(clip.transform.flip_h, "Flip left–right")
                        .clicked()
                    {
                        change(ctx, id, "Flip", |c| {
                            c.transform.flip_h = !c.transform.flip_h
                        });
                    }
                    if ui
                        .selectable_label(clip.transform.flip_v, "Flip up–down")
                        .clicked()
                    {
                        change(ctx, id, "Flip", |c| {
                            c.transform.flip_v = !c.transform.flip_v
                        });
                    }
                });
            });
            row(ui, "Align", |ui| {
                ui.horizontal_wrapped(|ui| alignment_buttons(ui, ctx, clip));
            });
        });
}

/// The diamond that adds or removes a keyframe at the playhead for the given properties.
fn diamond(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip, props: &[Prop]) {
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    let tol = keyframes::tolerance(frame);
    let local = keyframes::local_time(clip, ctx.state.playhead);
    let on_clip = keyframes::playhead_on(clip, ctx.state.playhead);
    let animated = props.iter().any(|p| keyframes::animated(clip, *p));
    let keyed = props
        .iter()
        .all(|p| keyframes::has_key(clip, *p, local, tol))
        && on_clip;
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
    let c = rect.center();
    let r = 6.0;
    let shape = vec![
        egui::pos2(c.x, c.y - r),
        egui::pos2(c.x + r, c.y),
        egui::pos2(c.x, c.y + r),
        egui::pos2(c.x - r, c.y),
    ];
    let color = if !on_clip {
        egui::Color32::from_rgb(70, 70, 70)
    } else if animated || resp.hovered() {
        theme::ACCENT
    } else {
        egui::Color32::from_rgb(120, 120, 120)
    };
    if keyed {
        ui.painter().add(egui::Shape::convex_polygon(
            shape,
            color,
            egui::Stroke::NONE,
        ));
    } else {
        ui.painter().add(egui::Shape::closed_line(
            shape,
            egui::Stroke::new(1.5, color),
        ));
    }
    let hint = if !on_clip {
        "Move the playhead onto the clip to add keyframes"
    } else if keyed {
        "Remove the keyframe here"
    } else {
        "Add a keyframe here. Once a property has keyframes, changing it at another time adds more."
    };
    let resp = resp.on_hover_text(hint);
    if resp.clicked() && on_clip {
        let props = props.to_vec();
        change(ctx, clip.id, "Keyframe", |c| {
            keyframes::toggle_key(c, &props, local, tol)
        });
    }
    resp.context_menu(|ui| {
        if animated && ui.button("Stop animating (keep this value)").clicked() {
            let props = props.to_vec();
            change(ctx, clip.id, "Stop animating", |c| {
                keyframes::clear_animation(c, &props, local)
            });
            ui.close();
        }
        let key_index = props[0].get(clip).key_at(local, tol);
        if let (Some(i), true) = (key_index, on_clip) {
            ui.menu_button("How it moves to the next keyframe", |ui| {
                for easing in vz_anim::Easing::presets() {
                    if ui.button(easing.label()).clicked() {
                        let props = props.to_vec();
                        change(ctx, clip.id, "Easing", |c| {
                            for p in &props {
                                p.get_mut(c).set_easing(i, easing);
                            }
                        });
                        ui.close();
                    }
                }
            });
        }
    });
}

/// "‹ ›" buttons that jump the playhead to the previous / next keyframe of the properties.
fn keyframe_nav(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip, props: &[Prop]) {
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    let tol = keyframes::tolerance(frame);
    let local = keyframes::local_time(clip, ctx.state.playhead);
    let prev = keyframes::neighbour(clip, props, local, tol, false);
    let next = keyframes::neighbour(clip, props, local, tol, true);
    if prev.is_none() && next.is_none() {
        return;
    }
    let mut jump = None;
    if ui
        .add_enabled(prev.is_some(), egui::Button::new("‹").small())
        .on_hover_text("Previous keyframe")
        .clicked()
    {
        jump = prev;
    }
    if ui
        .add_enabled(next.is_some(), egui::Button::new("›").small())
        .on_hover_text("Next keyframe")
        .clicked()
    {
        jump = next;
    }
    if let Some(t) = jump {
        ctx.seek(clip.start + t);
    }
}

/// How a property number is shown on its slider, and the way back.
type Units = (fn(f32) -> f32, fn(f32) -> f32);

/// A slider with a keyframe diamond for properties that can be animated. The first property gives the number
/// shown; a change is written to all of them.
#[allow(clippy::too_many_arguments)]
fn anim_slider(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    clip: &Clip,
    props: &[Prop],
    label: &str,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    decimals: usize,
    (to_shown, from_shown): Units,
) {
    let id = clip.id;
    let frame = ctx.doc.project.frame_rate.frame_start(1);
    let tol = keyframes::tolerance(frame);
    let local = keyframes::local_time(clip, ctx.state.playhead);
    let on_clip = keyframes::playhead_on(clip, ctx.state.playhead);
    let animated = props.iter().any(|p| keyframes::animated(clip, *p));
    let shown = to_shown(props[0].get(clip).value_at(local));
    row(ui, label, |ui| {
        ui.horizontal(|ui| {
            let mut v = shown;
            let resp = ui.add_enabled(
                on_clip || !animated,
                egui::Slider::new(&mut v, range)
                    .suffix(suffix)
                    .max_decimals(decimals)
                    .clamping(egui::SliderClamping::Never),
            );
            if resp.changed() {
                let props = props.to_vec();
                let value = from_shown(v);
                let merge = key(id, props[0].name());
                let _ = ctx.doc.update_clip(id, "Change", Some(merge), |c| {
                    for p in &props {
                        keyframes::write(c, *p, local, value, tol);
                    }
                });
            }
            if resp.drag_stopped() || resp.lost_focus() {
                ctx.doc.seal_history();
            }
            diamond(ui, ctx, clip, props);
        });
    });
}

/// Left / centre / right and top / middle / bottom, plus fit and fill, in canvas terms.
fn alignment_buttons(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let canvas = ctx.doc.project.canvas;
    let Some(p) = placed(&ctx.doc.project, clip, ctx.state.playhead.max(clip.start)) else {
        return;
    };
    let (cw, ch) = (canvas.width as f32, canvas.height as f32);
    let local = keyframes::local_time(clip, ctx.state.playhead);
    let tol = keyframes::tolerance(ctx.doc.project.frame_rate.frame_start(1));
    let size = p.scaled_size();
    let small = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Button::new(RichText::new(text).small()).small())
            .clicked()
    };
    if small(ui, "Left") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::X, local, -(cw - size.x) / 2.0, tol)
        });
    }
    if small(ui, "Center") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::X, local, 0.0, tol)
        });
    }
    if small(ui, "Right") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::X, local, (cw - size.x) / 2.0, tol)
        });
    }
    if small(ui, "Top") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::Y, local, -(ch - size.y) / 2.0, tol)
        });
    }
    if small(ui, "Middle") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::Y, local, 0.0, tol)
        });
    }
    if small(ui, "Bottom") {
        change(ctx, id, "Align", |c| {
            keyframes::write(c, Prop::Y, local, (ch - size.y) / 2.0, tol)
        });
    }
    if let Some(base) = base_size(&ctx.doc.project, clip) {
        if small(ui, "Fit") {
            change(ctx, id, "Fit to canvas", |c| {
                c.transform = vz_core::Transform {
                    flip_h: c.transform.flip_h,
                    flip_v: c.transform.flip_v,
                    ..Default::default()
                };
            });
        }
        if small(ui, "Fill") {
            let k = (cw / base.x).max(ch / base.y);
            change(ctx, id, "Fill canvas", |c| {
                c.transform = vz_core::Transform {
                    flip_h: c.transform.flip_h,
                    flip_v: c.transform.flip_v,
                    ..Default::default()
                };
                c.transform.scale_x = k.into();
                c.transform.scale_y = k.into();
            });
        }
    }
}

fn blend_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Blend");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("↺ Reset").clicked() {
                change(ctx, id, "Reset blend", |c| {
                    c.opacity = 1.0.into();
                    c.blend = BlendMode::Normal;
                });
            }
        });
    });
    egui::Grid::new("blend_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            anim_slider(
                ui,
                ctx,
                clip,
                &[Prop::Opacity],
                "Opacity",
                0.0..=100.0,
                " %",
                0,
                (|v| v * 100.0, |v| v / 100.0),
            );
            row(ui, "Mode", |ui| {
                egui::ComboBox::from_id_salt("blend_mode")
                    .selected_text(clip.blend.label())
                    .show_ui(ui, |ui| {
                        for mode in BlendMode::ALL {
                            if ui
                                .selectable_label(clip.blend == mode, mode.label())
                                .clicked()
                            {
                                change(ctx, id, "Blend mode", |c| c.blend = mode);
                            }
                        }
                    });
            });
        });
}

fn crop_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Crop");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("↺ Reset").clicked() {
                change(ctx, id, "Reset crop", |c| c.crop = vz_core::Crop::default());
            }
        });
    });
    egui::Grid::new("crop_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            let c = clip.crop;
            slider(
                ui,
                ctx,
                id,
                "Crop left",
                "Left",
                c.left * 100.0,
                0.0..=90.0,
                " %",
                0,
                |c, v| {
                    c.crop.left = v / 100.0;
                    c.crop = c.crop.clamped();
                },
            );
            slider(
                ui,
                ctx,
                id,
                "Crop top",
                "Top",
                c.top * 100.0,
                0.0..=90.0,
                " %",
                0,
                |c, v| {
                    c.crop.top = v / 100.0;
                    c.crop = c.crop.clamped();
                },
            );
            slider(
                ui,
                ctx,
                id,
                "Crop right",
                "Right",
                c.right * 100.0,
                0.0..=90.0,
                " %",
                0,
                |c, v| {
                    c.crop.right = v / 100.0;
                    c.crop = c.crop.clamped();
                },
            );
            slider(
                ui,
                ctx,
                id,
                "Crop bottom",
                "Bottom",
                c.bottom * 100.0,
                0.0..=90.0,
                " %",
                0,
                |c, v| {
                    c.crop.bottom = v / 100.0;
                    c.crop = c.crop.clamped();
                },
            );
        });
}

/// Linear volume factor to decibels (0 becomes the bottom of the slider).
fn to_db(factor: f32) -> f32 {
    if factor <= 0.001 {
        -60.0
    } else {
        20.0 * factor.log10()
    }
}

fn from_db(db: f32) -> f32 {
    if db <= -59.5 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

fn audio_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Basic");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("↺ Reset").clicked() {
                change(ctx, id, "Reset sound", |c| {
                    c.volume = 1.0.into();
                    c.fade_in = 0;
                    c.fade_out = 0;
                });
            }
        });
    });
    let half = (clip.duration as f32 / 1_000_000.0 / 2.0).max(0.1);
    egui::Grid::new("audio_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            anim_slider(
                ui,
                ctx,
                clip,
                &[Prop::Volume],
                "Volume",
                -60.0..=12.0,
                " dB",
                1,
                (to_db, from_db),
            );
            slider(
                ui,
                ctx,
                id,
                "Fade in",
                "Fade in",
                clip.fade_in as f32 / 1e6,
                0.0..=half,
                " s",
                1,
                |c, v| c.fade_in = (f64::from(v) * 1e6) as Micros,
            );
            slider(
                ui,
                ctx,
                id,
                "Fade out",
                "Fade out",
                clip.fade_out as f32 / 1e6,
                0.0..=half,
                " s",
                1,
                |c, v| c.fade_out = (f64::from(v) * 1e6) as Micros,
            );
        });
    ui.add_space(6.0);
    ui.label(
        RichText::new("−60 dB is silent, 0 dB is the original level.")
            .color(theme::MUTED)
            .small(),
    );
}

fn clip_details(ui: &mut egui::Ui, ctx: &Ctx<'_>, clip: &Clip) {
    let rate = ctx.doc.project.frame_rate;
    let name = match &clip.content {
        Content::Media(m) => ctx
            .doc
            .project
            .asset(*m)
            .map_or("?".to_string(), |a| a.name.clone()),
        Content::Text(t) => t.text.lines().next().unwrap_or("").to_string(),
        Content::Shape(s) => s.kind.label().to_string(),
    };
    section_title(ui, "Selected clip");
    let tc = |t: Micros| rate.timecode(t);
    egui::Grid::new("clip_details")
        .num_columns(2)
        .spacing(vec2(16.0, 8.0))
        .min_col_width(96.0)
        .show(ui, |ui| {
            row(ui, "Name", |ui| {
                ui.label(name);
            });
            row(ui, "Starts at", |ui| {
                ui.label(RichText::new(tc(clip.start)).monospace());
            });
            row(ui, "Length", |ui| {
                ui.label(format!(
                    "{}  ({})",
                    tc(clip.duration),
                    short_duration(clip.duration)
                ));
            });
            if !clip.is_text() {
                row(ui, "From file time", |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} → {}",
                            tc(clip.source_in),
                            tc(clip.source_in + clip.source_span())
                        ))
                        .monospace(),
                    );
                });
            }
        });
}

// ---- the project ---------------------------------------------------------------------------------------------------------

fn project_details(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    details(ui, ctx);
    ui.add_space(14.0);
    ui.label(RichText::new("Canvas background").strong());
    ui.add_space(4.0);
    background(ui, ctx);
    ui.add_space(14.0);
    ui.label(RichText::new("Global edits").strong());
    ui.add_space(4.0);
    for name in [
        "Normalize loudness of all clips",
        "Apply a filter to all clips",
        "Apply a transition to all cuts",
    ] {
        ui.add_enabled(false, egui::Button::new(name))
            .on_disabled_hover_text("Coming soon");
    }
}

fn background(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    let current = ctx.doc.project.background;
    let is_color = matches!(current, Background::Color(_));
    ui.horizontal(|ui| {
        if ui.selectable_label(is_color, "Colour").clicked() && !is_color {
            let _ = ctx.doc.set_background(Background::Color(vz_core::BLACK));
        }
        if ui.selectable_label(!is_color, "Blur").clicked() && is_color {
            let _ = ctx.doc.set_background(Background::Blur(0.5));
        }
    });
    match current {
        Background::Color(c) => {
            let mut color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut color,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                let _ = ctx.doc.set_background(Background::Color([
                    color.r(),
                    color.g(),
                    color.b(),
                    255,
                ]));
            }
        }
        Background::Blur(amount) => {
            let mut v = amount * 100.0;
            if ui
                .add(egui::Slider::new(&mut v, 0.0..=100.0).suffix(" %"))
                .changed()
            {
                let _ = ctx.doc.set_background(Background::Blur(v / 100.0));
            }
        }
    }
}

fn details(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    egui::Grid::new("project_details")
        .num_columns(2)
        .spacing(vec2(16.0, 10.0))
        .min_col_width(96.0)
        .show(ui, |ui| {
            row(ui, "Name", |ui| {
                let mut name = ctx.doc.project.name.clone();
                let response =
                    ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
                if response.changed() {
                    let _ = ctx.doc.rename(name, Some(NAME_TYPING_KEY));
                }
                if response.lost_focus() {
                    ctx.doc.seal_history();
                }
            });
            row(ui, "Aspect ratio", |ui| {
                let current = ctx.doc.project.aspect_preset();
                egui::ComboBox::from_id_salt("details_ratio")
                    .selected_text(current.map_or("Custom", AspectPreset::label))
                    .show_ui(ui, |ui| {
                        for preset in AspectPreset::ALL {
                            if ui
                                .selectable_label(current == Some(preset), preset.label())
                                .clicked()
                            {
                                let _ = ctx.doc.set_aspect(preset);
                            }
                        }
                    });
            });
            row(ui, "Resolution", |ui| {
                ui.label(format!(
                    "{} × {}",
                    ctx.doc.project.canvas.width, ctx.doc.project.canvas.height
                ));
            });
            row(ui, "Frame rate", |ui| {
                let rate = ctx.doc.project.frame_rate;
                egui::ComboBox::from_id_salt("details_fps")
                    .selected_text(format!("{} fps", rate.label()))
                    .show_ui(ui, |ui| {
                        for candidate in FrameRate::PRESETS {
                            if ui
                                .selectable_label(
                                    rate == candidate,
                                    format!("{} fps", candidate.label()),
                                )
                                .clicked()
                            {
                                let _ = ctx.doc.set_frame_rate(candidate);
                            }
                        }
                    });
            });
            row(ui, "Colour space", |ui| {
                ui.label("Rec. 709 SDR");
            });
            row(ui, "Proxy", |ui| {
                let mut off = false;
                ui.add_enabled(false, egui::Checkbox::new(&mut off, "Use proxies"))
                    .on_disabled_hover_text("Coming in Phase 7");
            });
            row(ui, "Saved to", |ui| {
                match &ctx.doc.path {
                    Some(p) => ui
                        .label(p.display().to_string())
                        .on_hover_text(p.display().to_string()),
                    None => ui.label(RichText::new("Not saved yet").color(theme::MUTED)),
                };
            });
            row(ui, "Duration", |ui| {
                let d = ctx.doc.project.duration();
                ui.label(RichText::new(ctx.doc.project.frame_rate.timecode(d)).monospace());
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibel_conversion_round_trips_and_floors_at_silence() {
        for factor in [0.1f32, 0.5, 1.0, 1.5, 3.0] {
            assert!((from_db(to_db(factor)) - factor).abs() < 1e-3, "{factor}");
        }
        assert_eq!(to_db(0.0), -60.0);
        assert_eq!(from_db(-60.0), 0.0);
        assert!((to_db(1.0)).abs() < 1e-6);
    }
}
