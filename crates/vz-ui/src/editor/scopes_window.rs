//! The scopes window: histogram, waveform and vectorscope of the picture in the player.

use super::Ctx;
use crate::theme;
use eframe::egui::{self, ColorImage, RichText, TextureOptions, vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Waveform,
    Vectorscope,
    Histogram,
}

impl Scope {
    const ALL: [Self; 3] = [Self::Waveform, Self::Vectorscope, Self::Histogram];

    fn label(self) -> &'static str {
        match self {
            Self::Waveform => "Waveform",
            Self::Vectorscope => "Vectorscope",
            Self::Histogram => "Histogram",
        }
    }
}

pub fn show(egui_ctx: &egui::Context, ctx: &mut Ctx<'_>) {
    if !ctx.state.show_scopes {
        return;
    }
    let mut open = true;
    egui::Window::new("Scopes")
        .open(&mut open)
        .default_size(vec2(340.0, 300.0))
        .resizable(true)
        .show(egui_ctx, |ui| {
            ui.horizontal(|ui| {
                for s in Scope::ALL {
                    if ui.selectable_label(ctx.state.scope == s, s.label()).clicked() {
                        ctx.state.scope = s;
                    }
                }
            });
            ui.add_space(6.0);
            let Some(frame) = ctx.state.shown_frame.clone() else {
                ui.label(RichText::new("Add a clip to see its colours here.").color(theme::MUTED));
                return;
            };
            let width = (ui.available_width().min(560.0) as usize).max(120);
            let (img, size) = match ctx.state.scope {
                Scope::Waveform => (vz_render::scopes::waveform(&frame, width, width * 5 / 8), (width, width * 5 / 8)),
                Scope::Vectorscope => {
                    let s = width.min(320);
                    (vz_render::scopes::vectorscope(&frame, s), (s, s))
                }
                Scope::Histogram => (vz_render::scopes::histogram(&frame, width, width / 2), (width, width / 2)),
            };
            let color = ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
            match &mut ctx.state.scope_texture {
                Some(t) => t.set(color, TextureOptions::LINEAR),
                None => ctx.state.scope_texture = Some(egui_ctx.load_texture("scope", color, TextureOptions::LINEAR)),
            }
            if let Some(t) = &ctx.state.scope_texture {
                ui.image((t.id(), vec2(size.0 as f32, size.1 as f32)));
            }
            ui.label(
                RichText::new(match ctx.state.scope {
                    Scope::Waveform => "Brightness up the side, the picture left to right. Bright dots near the top may be too bright.",
                    Scope::Vectorscope => "Colours around the circle; the further from the middle, the stronger. Skin tones sit between red and yellow.",
                    Scope::Histogram => "How many pixels there are at each brightness (dark on the left). Piling up at an end means lost detail.",
                })
                .color(theme::MUTED)
                .small(),
            );
        });
    if !open {
        ctx.state.show_scopes = false;
    }
}
