//! The Audio tab: basic sound (volume, fades, balance, loudness, beats), clean-up (noise, equaliser,
//! compressor, de-esser, ducking) and voice effects.

use super::looks::num;
use super::{Ctx, anim_slider, change, from_db, row, section_title, slider, to_db};
use crate::audio_jobs::JobKind;
use crate::theme;
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, vec2};
use std::path::PathBuf;
use vz_core::{
    ChannelMode, Clip, Compressor, Denoise, Duck, EQ_FREQS, EQ_PRESETS, Eq, Micros, Normalize,
    Voice, VoiceEffect, VoiceEffect as V,
};

use super::super::keyframes::Prop;

/// The three pages of the Audio tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Basic,
    Clean,
    Voice,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Clean => "Clean-up",
            Self::Voice => "Voice effects",
        }
    }
}

/// The file, start and length of the media a clip plays (what the measurements look at).
fn source_of(ctx: &Ctx<'_>, clip: &Clip) -> Option<(PathBuf, Micros, Micros)> {
    let asset = clip.media().and_then(|m| ctx.doc.project.asset(m))?;
    asset
        .has_audio
        .then(|| (asset.path.clone(), clip.source_in, clip.source_span()))
}

pub fn audio_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    ui.horizontal(|ui| {
        for page in [Page::Basic, Page::Clean, Page::Voice] {
            if ui
                .selectable_label(ctx.state.audio_page == page, page.label())
                .clicked()
            {
                ctx.state.audio_page = page;
            }
        }
    });
    ui.add_space(4.0);
    match ctx.state.audio_page {
        Page::Basic => basic_page(ui, ctx, clip),
        Page::Clean => clean_page(ui, ctx, clip),
        Page::Voice => voice_page(ui, ctx, clip),
    }
}

// ---- basic ----------------------------------------------------------------------------------------------------------

fn basic_page(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.horizontal(|ui| {
        section_title(ui, "Basic");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("↺ Reset").clicked() {
                change(ctx, id, "Reset sound", |c| {
                    c.volume = 1.0.into();
                    c.fade_in = 0;
                    c.fade_out = 0;
                    c.audio.pan = 0.0;
                    c.audio.channel = ChannelMode::Stereo;
                    c.audio.normalize = None;
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
            num(
                ui,
                ctx,
                id,
                "pan",
                "Balance",
                clip.audio.pan * 100.0,
                -100.0..=100.0,
                " %",
                |c, v| c.audio.pan = (v / 100.0).clamp(-1.0, 1.0),
            );
            row(ui, "Channels", |ui| {
                egui::ComboBox::from_id_salt(("channel", id))
                    .selected_text(clip.audio.channel.label())
                    .show_ui(ui, |ui| {
                        for m in ChannelMode::ALL {
                            if ui
                                .selectable_label(clip.audio.channel == m, m.label())
                                .clicked()
                            {
                                change(ctx, id, "Sound channels", move |c| c.audio.channel = m);
                            }
                        }
                    });
            });
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new("−60 dB is silent, 0 dB is the original level.")
            .color(theme::MUTED)
            .small(),
    );
    ui.add_space(8.0);
    loudness_section(ui, ctx, clip);
    ui.add_space(8.0);
    beats_section(ui, ctx, clip);
}

fn loudness_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let source = source_of(ctx, clip);
    let mut on = clip.audio.normalize.is_some();
    ui.add_enabled_ui(source.is_some(), |ui| {
        if ui
            .checkbox(&mut on, RichText::new("Normalize loudness").strong())
            .on_hover_text("Make this clip as loud as the target, whatever it was recorded at")
            .changed()
        {
            change(ctx, id, "Normalize loudness", move |c| {
                c.audio.normalize = on.then_some(Normalize {
                    target_lufs: Normalize::SOCIAL,
                    // until it is measured nothing changes
                    measured_lufs: Normalize::SOCIAL,
                });
            });
            if on && let Some((path, from, length)) = source.clone() {
                ctx.state
                    .audio_jobs
                    .start(ui.ctx(), id, JobKind::Loudness, path, from, length);
            }
        }
    });
    let Some(n) = clip.audio.normalize else {
        if source.is_none() {
            ui.label(
                RichText::new("Not available for this clip.")
                    .color(theme::MUTED)
                    .small(),
            );
        }
        return;
    };
    ui.horizontal(|ui| {
        for (name, target) in [("Social −14", Normalize::SOCIAL), ("TV −23", Normalize::TV)] {
            if ui
                .selectable_label((n.target_lufs - target).abs() < 0.05, name)
                .clicked()
            {
                change(ctx, id, "Loudness target", move |c| {
                    if let Some(n) = &mut c.audio.normalize {
                        n.target_lufs = target;
                    }
                });
            }
        }
    });
    egui::Grid::new("loudness_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            num(
                ui,
                ctx,
                id,
                "target",
                "Target",
                n.target_lufs,
                -30.0..=-6.0,
                " LUFS",
                |c, v| {
                    if let Some(n) = &mut c.audio.normalize {
                        n.target_lufs = v;
                    }
                },
            );
        });
    let busy = ctx.state.audio_jobs.busy(id, JobKind::Loudness);
    ui.horizontal(|ui| {
        if busy {
            ui.spinner();
            ui.label(RichText::new("Measuring…").color(theme::MUTED));
        } else {
            ui.label(
                RichText::new(format!(
                    "Measured {:.1} LUFS, so {:+.1} dB",
                    n.measured_lufs,
                    n.gain_db()
                ))
                .color(theme::MUTED),
            );
            if ui.small_button("Measure again").clicked()
                && let Some((path, from, length)) = source
            {
                ctx.state
                    .audio_jobs
                    .start(ui.ctx(), id, JobKind::Loudness, path, from, length);
            }
        }
    });
}

fn beats_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let Some(asset) = clip.media().and_then(|m| ctx.doc.project.asset(m)) else {
        return;
    };
    if !asset.has_audio {
        return;
    }
    let (path, length) = (asset.path.clone(), asset.duration);
    ui.label(RichText::new("Beats").strong());
    let busy = ctx.state.audio_jobs.busy(id, JobKind::Beats);
    let beats = &clip.audio.beats;
    ui.horizontal(|ui| {
        if busy {
            ui.spinner();
            ui.label(RichText::new("Listening for the beat…").color(theme::MUTED));
            return;
        }
        let label = if beats.is_empty() {
            "Find beats"
        } else {
            "Find beats again"
        };
        if ui
            .button(label)
            .on_hover_text("Marks the beats of the music. Clips and the playhead snap to them.")
            .clicked()
        {
            ctx.state
                .audio_jobs
                .start(ui.ctx(), id, JobKind::Beats, path, 0, length);
        }
        if !beats.is_empty() && ui.button("Clear").clicked() {
            change(ctx, id, "Clear beats", |c| c.audio.beats.clear());
        }
    });
    if !beats.is_empty() {
        ui.label(
            RichText::new(format!(
                "{} beats, about {:.0} per minute. They show as ticks on the clip.",
                beats.len(),
                beats_per_minute(beats)
            ))
            .color(theme::MUTED)
            .small(),
        );
    }
}

/// Typical beats per minute of a list of beat times (median distance).
pub fn beats_per_minute(beats: &[Micros]) -> f32 {
    let mut gaps: Vec<Micros> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return 0.0;
    }
    gaps.sort_unstable();
    60.0 * 1e6 / gaps[gaps.len() / 2].max(1) as f32
}

// ---- clean-up -------------------------------------------------------------------------------------------------------

fn clean_page(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    noise_section(ui, ctx, clip);
    ui.add_space(10.0);
    eq_section(ui, ctx, clip);
    ui.add_space(10.0);
    compressor_section(ui, ctx, clip);
    ui.add_space(10.0);
    deess_section(ui, ctx, clip);
    ui.add_space(10.0);
    duck_section(ui, ctx, clip);
}

fn noise_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let source = source_of(ctx, clip);
    let mut on = clip.audio.denoise.is_some();
    ui.add_enabled_ui(source.is_some(), |ui| {
        if ui
            .checkbox(&mut on, RichText::new("Noise reduction").strong())
            .on_hover_text("Takes away steady background noise such as hiss, fans and hum")
            .changed()
        {
            change(ctx, id, "Noise reduction", move |c| {
                c.audio.denoise = on.then(|| Denoise {
                    amount: 0.6,
                    profile: Vec::new(),
                });
            });
            if on && let Some((path, from, length)) = source.clone() {
                ctx.state
                    .audio_jobs
                    .start(ui.ctx(), id, JobKind::Noise, path, from, length);
            }
        }
    });
    let Some(d) = &clip.audio.denoise else {
        if source.is_none() {
            ui.label(
                RichText::new("Not available for this clip.")
                    .color(theme::MUTED)
                    .small(),
            );
        }
        return;
    };
    egui::Grid::new("denoise_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 8.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            num(
                ui,
                ctx,
                id,
                "denoise",
                "Strength",
                d.amount * 100.0,
                0.0..=100.0,
                " %",
                |c, v| {
                    if let Some(d) = &mut c.audio.denoise {
                        d.amount = (v / 100.0).clamp(0.0, 1.0);
                    }
                },
            );
        });
    let busy = ctx.state.audio_jobs.busy(id, JobKind::Noise);
    ui.horizontal(|ui| {
        if busy || d.profile.is_empty() {
            ui.spinner();
            ui.label(RichText::new("Measuring the noise…").color(theme::MUTED));
        } else {
            ui.label(
                RichText::new("Noise measured from the quietest parts.")
                    .color(theme::MUTED)
                    .small(),
            );
            if ui.small_button("Measure again").clicked()
                && let Some((path, from, length)) = source
            {
                ctx.state
                    .audio_jobs
                    .start(ui.ctx(), id, JobKind::Noise, path, from, length);
            }
        }
    });
}

fn eq_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.label(RichText::new("Equaliser").strong());
    let current = EQ_PRESETS
        .iter()
        .find(|(_, g)| {
            g.iter()
                .zip(&clip.audio.eq.gains)
                .all(|(a, b)| (a - b).abs() < 0.05)
        })
        .map_or("Custom", |(name, _)| *name);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Preset").color(theme::MUTED));
        egui::ComboBox::from_id_salt(("eq_preset", id))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (name, gains) in EQ_PRESETS {
                    if ui.selectable_label(current == name, name).clicked() {
                        change(ctx, id, "Equaliser preset", move |c| {
                            c.audio.eq = Eq { gains };
                        });
                    }
                }
            });
    });
    eq_curve(ui, &clip.audio.eq);
    egui::Grid::new("eq_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            for (i, freq) in EQ_FREQS.iter().enumerate() {
                let label = if *freq >= 1000.0 {
                    format!("{} kHz", freq / 1000.0)
                } else {
                    format!("{freq} Hz")
                };
                num(
                    ui,
                    ctx,
                    id,
                    &format!("eq{i}"),
                    &label,
                    clip.audio.eq.gains[i],
                    -12.0..=12.0,
                    " dB",
                    move |c, v| c.audio.eq.gains[i] = v.clamp(-12.0, 12.0),
                );
            }
        });
}

/// A small picture of the equaliser's effect from 20 Hz to 20 kHz.
fn eq_curve(ui: &mut egui::Ui, eq: &Eq) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(300.0), 64.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::PANEL_2);
    let mid = rect.center().y;
    painter.hline(
        rect.x_range(),
        mid,
        Stroke::new(1.0, Color32::from_gray(70)),
    );
    let points: Vec<Pos2> = (0..=120)
        .map(|i| {
            let t = i as f32 / 120.0;
            let freq = 20.0 * 1000f32.powf(t);
            let db = 20.0 * vz_audio::fx::eq::response(48_000, eq, freq).log10() as f32;
            Pos2::new(
                rect.left() + t * rect.width(),
                mid - (db / 14.0).clamp(-1.0, 1.0) * (rect.height() / 2.0 - 3.0),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(2.0, theme::ACCENT)));
}

fn compressor_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let mut on = clip.audio.compressor.is_some();
    if ui
        .checkbox(&mut on, RichText::new("Compressor and limiter").strong())
        .on_hover_text("Makes loud parts quieter so the level is more even")
        .changed()
    {
        change(ctx, id, "Compressor", move |c| {
            c.audio.compressor = on.then(Compressor::default);
        });
    }
    let Some(comp) = clip.audio.compressor else {
        return;
    };
    egui::Grid::new("comp_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            let set = |f: fn(&mut Compressor, f32)| {
                move |c: &mut Clip, v: f32| {
                    if let Some(k) = &mut c.audio.compressor {
                        f(k, v);
                    }
                }
            };
            num(
                ui,
                ctx,
                id,
                "c.th",
                "Threshold",
                comp.threshold_db,
                -60.0..=0.0,
                " dB",
                set(|k, v| k.threshold_db = v),
            );
            num(
                ui,
                ctx,
                id,
                "c.ratio",
                "Ratio",
                comp.ratio,
                1.0..=20.0,
                " : 1",
                set(|k, v| k.ratio = v),
            );
            num(
                ui,
                ctx,
                id,
                "c.att",
                "Attack",
                comp.attack_ms,
                0.5..=200.0,
                " ms",
                set(|k, v| k.attack_ms = v),
            );
            num(
                ui,
                ctx,
                id,
                "c.rel",
                "Release",
                comp.release_ms,
                10.0..=1500.0,
                " ms",
                set(|k, v| k.release_ms = v),
            );
            num(
                ui,
                ctx,
                id,
                "c.mk",
                "Make-up gain",
                comp.makeup_db,
                0.0..=24.0,
                " dB",
                set(|k, v| k.makeup_db = v),
            );
        });
    let mut limit = comp.limit;
    if ui.checkbox(&mut limit, "Never go above −1 dB").changed() {
        change(ctx, id, "Limiter", move |c| {
            if let Some(k) = &mut c.audio.compressor {
                k.limit = limit;
            }
        });
    }
}

fn deess_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    ui.label(RichText::new("De-esser").strong());
    egui::Grid::new("deess_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            num(
                ui,
                ctx,
                id,
                "deess",
                "Strength",
                clip.audio.deess * 100.0,
                0.0..=100.0,
                " %",
                |c, v| c.audio.deess = (v / 100.0).clamp(0.0, 1.0),
            );
        });
    ui.label(
        RichText::new("Softens sharp \"s\" sounds in a voice.")
            .color(theme::MUTED)
            .small(),
    );
}

fn duck_section(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    let mut on = clip.audio.duck.is_some();
    if ui
        .checkbox(&mut on, RichText::new("Auto ducking").strong())
        .on_hover_text("Lowers this clip while other clips (for example a voice) are playing")
        .changed()
    {
        change(ctx, id, "Auto ducking", move |c| {
            c.audio.duck = on.then(Duck::default);
        });
    }
    let Some(duck) = clip.audio.duck else {
        ui.label(
            RichText::new(
                "Turn this on for background music: it gets quieter while someone speaks.",
            )
            .color(theme::MUTED)
            .small(),
        );
        return;
    };
    egui::Grid::new("duck_grid")
        .num_columns(2)
        .spacing(vec2(14.0, 6.0))
        .min_col_width(80.0)
        .show(ui, |ui| {
            let set = |f: fn(&mut Duck, f32)| {
                move |c: &mut Clip, v: f32| {
                    if let Some(k) = &mut c.audio.duck {
                        f(k, v);
                    }
                }
            };
            num(
                ui,
                ctx,
                id,
                "d.amt",
                "Lower by",
                duck.amount_db,
                0.0..=40.0,
                " dB",
                set(|k, v| k.amount_db = v),
            );
            num(
                ui,
                ctx,
                id,
                "d.att",
                "Attack",
                duck.attack_ms,
                5.0..=1000.0,
                " ms",
                set(|k, v| k.attack_ms = v),
            );
            num(
                ui,
                ctx,
                id,
                "d.rel",
                "Release",
                duck.release_ms,
                50.0..=3000.0,
                " ms",
                set(|k, v| k.release_ms = v),
            );
        });
    ui.label(
        RichText::new("This clip gets quieter whenever any clip that is not ducked makes sound.")
            .color(theme::MUTED)
            .small(),
    );
}

// ---- voice effects ----------------------------------------------------------------------------------------------------

fn voice_page(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, clip: &Clip) {
    let id = clip.id;
    section_title(ui, "Voice effects");
    let current = clip.audio.voice.map(|v| v.effect);
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(current.is_none(), "None").clicked() {
            change(ctx, id, "Voice effect", |c| c.audio.voice = None);
        }
        for effect in VoiceEffect::ALL {
            if ui
                .selectable_label(current == Some(effect), effect.label())
                .clicked()
            {
                let amount = clip
                    .audio
                    .voice
                    .map_or(default_amount(effect), |v| v.amount);
                change(ctx, id, "Voice effect", move |c| {
                    c.audio.voice = Some(Voice { effect, amount });
                });
            }
        }
    });
    if let Some(v) = clip.audio.voice {
        ui.add_space(6.0);
        egui::Grid::new("voice_grid")
            .num_columns(2)
            .spacing(vec2(14.0, 6.0))
            .min_col_width(80.0)
            .show(ui, |ui| {
                num(
                    ui,
                    ctx,
                    id,
                    "voice",
                    "Amount",
                    v.amount * 100.0,
                    0.0..=100.0,
                    " %",
                    |c, x| {
                        if let Some(v) = &mut c.audio.voice {
                            v.amount = (x / 100.0).clamp(0.0, 1.0);
                        }
                    },
                );
            });
        ui.label(RichText::new(hint(v.effect)).color(theme::MUTED).small());
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new("Effects are made by sound processing on your computer. For a different speed use the Speed tab.")
            .color(theme::MUTED)
            .small(),
    );
}

fn default_amount(effect: VoiceEffect) -> f32 {
    match effect {
        V::Chipmunk | V::Deep => 0.5,
        V::Echo | V::Room | V::Hall => 0.4,
        _ => 0.8,
    }
}

fn hint(effect: VoiceEffect) -> &'static str {
    match effect {
        V::Chipmunk => "Amount: how high the voice goes (up to an octave).",
        V::Deep => "Amount: how low the voice goes.",
        V::Robot => "Amount: how robotic.",
        V::Echo => "Amount: how loud the echoes are.",
        V::Room => "Amount: how much room sound is mixed in.",
        V::Hall => "Amount: how much hall sound is mixed in.",
        V::Telephone | V::Megaphone | V::Radio => "Amount: how much of the effect is mixed in.",
        V::Underwater => "Amount: how much of the effect is mixed in.",
        V::Vibrato => "Amount: how deep the wobble is.",
    }
}
