//! Small interface pieces used in several places: the colour picker with an eyedropper and recent colours.

use super::Ctx;
use super::canvas::merge_key;
use crate::theme;
use eframe::egui::{
    self, Color32, CornerRadius, PopupCloseBehavior, Sense, Stroke, StrokeKind, color_picker, vec2,
};
use std::sync::Arc;
use vz_core::{Clip, ClipId, Color};

/// What to do with a colour picked from the picture.
pub type ApplyColor = Arc<dyn Fn(&mut Clip, Color) + Send + Sync>;

/// Waiting for a click on the picture to take a colour from it.
pub struct Eyedropper {
    pub clip: ClipId,
    pub merge: String,
    pub apply: ApplyColor,
}

fn to32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn from32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [r, g, b, a]
}

/// A swatch that opens a full colour picker: saturation square, hue and opacity sliders, numbers, the colours used
/// recently, and an eyedropper that takes a colour from the picture in the player. `apply` writes a colour into the
/// clip; all changes made while the picker is open count as one undo step per drag.
pub fn pick_color(
    ui: &mut egui::Ui,
    ctx: &mut Ctx<'_>,
    clip: ClipId,
    merge: &str,
    current: Color,
    apply: ApplyColor,
) {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 22.0), Sense::click());
    let painter = ui.painter();
    // a checkerboard under see-through colours
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_gray(70));
    painter.rect_filled(
        egui::Rect::from_min_max(rect.center_top(), rect.right_center()),
        CornerRadius::ZERO,
        Color32::from_gray(110),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(rect.left_center(), rect.center_bottom()),
        CornerRadius::ZERO,
        Color32::from_gray(110),
    );
    painter.rect_filled(rect, CornerRadius::same(4), to32(current));
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
    let waiting = ctx
        .state
        .eyedropper
        .as_ref()
        .is_some_and(|e| e.clip == clip && e.merge == merge);
    if waiting {
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(2.0, theme::ACCENT),
            StrokeKind::Outside,
        );
    }
    egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(220.0);
            let mut c32 = to32(current);
            if color_picker::color_picker_color32(ui, &mut c32, color_picker::Alpha::OnlyBlend) {
                let new = from32(c32);
                let a = apply.clone();
                let _ = ctx
                    .doc
                    .update_clip(clip, "Colour", Some(merge_key(clip, merge)), |c| a(c, new));
                remember(ctx, new);
            }
            ui.add_space(4.0);
            if ui
                .button("Pick from the picture")
                .on_hover_text(
                    "Click here, then click a place in the player to copy its colour (Esc cancels)",
                )
                .clicked()
            {
                ctx.state.eyedropper = Some(Eyedropper {
                    clip,
                    merge: merge.to_owned(),
                    apply: apply.clone(),
                });
                ui.close();
            }
            if !ctx.state.recent_colors.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Recent").color(theme::MUTED).small());
                ui.horizontal_wrapped(|ui| {
                    for col in ctx.state.recent_colors.clone() {
                        let (r, rr) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                        ui.painter()
                            .rect_filled(r, CornerRadius::same(3), to32(col));
                        ui.painter().rect_stroke(
                            r,
                            CornerRadius::same(3),
                            Stroke::new(1.0, theme::BORDER),
                            StrokeKind::Inside,
                        );
                        if rr.clicked() {
                            let a = apply.clone();
                            let _ = ctx.doc.update_clip(clip, "Colour", None, |c| a(c, col));
                        }
                    }
                });
            }
        });
    if resp.hovered() {
        resp.on_hover_text("Click to choose a colour");
    }
    if ui.input(|i| i.pointer.any_released()) {
        ctx.doc.seal_history();
    }
}

/// Keep a colour in the list of recent colours (newest first, no repeats).
pub fn remember(ctx: &mut Ctx<'_>, color: Color) {
    let colours = &mut ctx.state.recent_colors;
    colours.retain(|c| *c != color);
    colours.insert(0, color);
    colours.truncate(10);
}

/// The colour of the picture at a point inside the player, given as a share of its width and height (0 to 1).
pub fn colour_under(ctx: &Ctx<'_>, u: f32, v: f32) -> Option<Color> {
    let f = ctx.state.shown_frame.as_ref()?;
    let (x, y) = (
        ((u * f.width as f32) as usize).min(f.width as usize - 1),
        ((v * f.height as f32) as usize).min(f.height as usize - 1),
    );
    let i = (y * f.width as usize + x) * 4;
    Some([f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_through_the_interface_type() {
        for c in [[0, 0, 0, 255], [255, 128, 7, 255], [10, 20, 30, 200]] {
            let back = from32(to32(c));
            // pre-multiplication can move see-through colours by one level
            assert!(
                back.iter()
                    .zip(c)
                    .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= 1),
                "{c:?} {back:?}"
            );
        }
    }
}
