//! The asset panel (top left): category tabs, a sub-menu and a searchable grid.

use super::{Action, Ctx};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, vec2,
};
use std::collections::HashSet;
use vz_core::{MediaAsset, MediaId, MediaKind, Micros, US_PER_SEC};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Media,
    Audio,
    Text,
    Stickers,
    Effects,
    Transitions,
    Captions,
    Filters,
    Adjustment,
    Templates,
}

impl Category {
    pub const ALL: [Self; 10] = [
        Self::Media,
        Self::Audio,
        Self::Text,
        Self::Stickers,
        Self::Effects,
        Self::Transitions,
        Self::Captions,
        Self::Filters,
        Self::Adjustment,
        Self::Templates,
    ];

    /// The icon drawn on the tab.
    pub fn icon(self) -> char {
        use crate::theme::icons;
        match self {
            Self::Media => icons::CLAPPERBOARD,
            Self::Audio => icons::MUSIC,
            Self::Text => icons::TYPE,
            Self::Stickers => icons::STICKER,
            Self::Effects => icons::SPARKLES,
            Self::Transitions => icons::BLEND,
            Self::Captions => icons::CAPTIONS,
            Self::Filters => icons::WAND,
            Self::Adjustment => icons::SLIDERS,
            Self::Templates => icons::TEMPLATE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Media => "Media",
            Self::Audio => "Audio",
            Self::Text => "Text",
            Self::Stickers => "Stickers",
            Self::Effects => "Effects",
            Self::Transitions => "Transitions",
            Self::Captions => "Captions",
            Self::Filters => "Filters",
            Self::Adjustment => "Adjustment",
            Self::Templates => "Templates",
        }
    }

    /// The left-hand list inside each tab (spec §6).
    pub fn sub_menu(self) -> &'static [&'static str] {
        match self {
            Self::Media => &["Local", "Library", "Subprojects"],
            Self::Audio => &["Music", "Sound effects", "Extract audio", "Voiceover"],
            Self::Text => &["Add text", "Text styles", "Text templates", "Fonts"],
            Self::Stickers => &["Stickers", "Shapes"],
            Self::Effects => &[
                "Basic",
                "Light & lens",
                "Retro",
                "Distortion",
                "Party",
                "Frames",
            ],
            Self::Transitions => &["Basic", "Movement", "Wipes", "Effects", "3D"],
            Self::Captions => &["Caption list", "Import", "Export", "Styles"],
            Self::Filters => &[
                "Featured",
                "Life",
                "Landscape",
                "Portrait",
                "Movies",
                "Retro",
                "Food",
                "B&W",
                "Night",
            ],
            Self::Adjustment => &["Adjustment layer", "My presets"],
            Self::Templates => &["My templates", "Starter templates"],
        }
    }

    /// Which roadmap phase builds a sub-menu entry of this tab.
    pub fn phase_of(self, _sub_menu: usize) -> u8 {
        self.phase()
    }

    /// Which roadmap phase builds this tab.
    pub fn phase(self) -> u8 {
        match self {
            Self::Media => 1,
            Self::Text | Self::Stickers => 2,
            Self::Transitions => 3,
            Self::Effects | Self::Filters | Self::Adjustment => 4,
            Self::Captions => 5,
            Self::Audio => 6,
            Self::Templates => 7,
        }
    }
}

/// `3.5 s` becomes `0:03`, 3725 s becomes `1:02:05`.
pub fn short_duration(t: Micros) -> String {
    let secs = (t.max(0) + US_PER_SEC / 2) / US_PER_SEC;
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn show(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.add_space(6.0);
    category_tabs(ui, ctx);
    ui.add_space(4.0);
    ui.separator();

    ui.horizontal_top(|ui| {
        // left sub-menu
        ui.allocate_ui_with_layout(
            vec2(118.0, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(6.0);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 0))
                    .show(ui, |ui| {
                        for (i, name) in ctx.state.category.sub_menu().iter().enumerate() {
                            let selected = ctx.state.sub_menu == i;
                            let text = if selected {
                                RichText::new(*name).color(theme::ACCENT)
                            } else {
                                RichText::new(*name)
                            };
                            if ui.selectable_label(selected, text).clicked() {
                                ctx.state.sub_menu = i;
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(RichText::new("Favourites").color(theme::MUTED));
                    });
            },
        );
        ui.separator();

        // right: search + grid
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut ctx.state.search)
                    .hint_text("Search")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);
            if ctx.state.category == Category::Media && ctx.state.sub_menu == 0 {
                media_grid(ui, ctx);
            } else if ctx.state.category == Category::Text
                && super::text_panel::handles(ctx.state.sub_menu)
            {
                super::text_panel::show(ui, ctx);
            } else if matches!(
                ctx.state.category,
                Category::Filters | Category::Effects | Category::Adjustment
            ) {
                super::look_panel::show(ui, ctx);
            } else if ctx.state.category == Category::Captions {
                super::caption_panel::show(ui, ctx);
            } else if ctx.state.category == Category::Transitions {
                super::transition_panel::show(ui, ctx);
            } else if ctx.state.category == Category::Stickers {
                super::sticker_panel::show(ui, ctx);
            } else {
                placeholder(ui, ctx.state.category, ctx.state.sub_menu);
            }
        });
    });
}

fn category_tabs(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    egui::ScrollArea::horizontal()
        .id_salt("category_tabs")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                for cat in Category::ALL {
                    let selected = ctx.state.category == cat;
                    let (rect, resp) = ui.allocate_exact_size(vec2(66.0, 52.0), Sense::click());
                    let painter = ui.painter();
                    if resp.hovered() && !selected {
                        painter.rect_filled(rect, CornerRadius::same(8), theme::PANEL_2);
                    }
                    // the icon on a rounded tile
                    let tile = Rect::from_center_size(
                        Pos2::new(rect.center().x, rect.top() + 18.0),
                        vec2(26.0, 26.0),
                    );
                    let (fill, fg) = if selected {
                        (theme::ACCENT, theme::ON_ACCENT)
                    } else {
                        (theme::PANEL_2, theme::MUTED)
                    };
                    painter.rect_filled(tile, CornerRadius::same(7), fill);
                    painter.text(
                        tile.center(),
                        Align2::CENTER_CENTER,
                        cat.icon(),
                        theme::icons::font(15.0),
                        fg,
                    );
                    painter.text(
                        Pos2::new(rect.center().x, rect.bottom() - 9.0),
                        Align2::CENTER_CENTER,
                        cat.label(),
                        FontId::proportional(11.0),
                        if selected { theme::ACCENT } else { theme::TEXT },
                    );
                    if resp.clicked() {
                        ctx.state.category = cat;
                        ctx.state.sub_menu = 0;
                    }
                }
            });
        });
}

// ---- the media grid ---------------------------------------------------------------------------------

const TILE_W: f32 = 108.0;
const THUMB_H: f32 = 66.0;
const TILE_H: f32 = THUMB_H + 24.0;

fn media_grid(ui: &mut egui::Ui, ctx: &mut Ctx<'_>) {
    ui.horizontal(|ui| {
        if ui
            .add(theme::primary_button("＋ Import"))
            .on_hover_text("Choose videos, photos or audio (Ctrl+I)")
            .clicked()
        {
            ctx.actions.push(Action::ImportDialog);
        }
        ui.label(
            RichText::new(format!("{} files", ctx.doc.project.media.len()))
                .color(theme::MUTED)
                .small(),
        );
    });
    ui.add_space(8.0);

    let search = ctx.state.search.to_lowercase();
    let used: HashSet<MediaId> = ctx
        .doc
        .project
        .timeline()
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().filter_map(|c| c.media()))
        .collect();
    let assets: Vec<MediaAsset> = ctx
        .doc
        .project
        .media
        .iter()
        .filter(|m| search.is_empty() || m.name.to_lowercase().contains(&search))
        .cloned()
        .collect();

    if assets.is_empty() {
        empty_state(ui, ctx.doc.project.media.is_empty());
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for asset in &assets {
                    tile(ui, ctx, asset, used.contains(&asset.id));
                }
            });
        });
}

fn empty_state(ui: &mut egui::Ui, nothing_imported: bool) {
    let avail = ui.available_size_before_wrap();
    let (rect, _) =
        ui.allocate_exact_size(vec2(avail.x, (avail.y - 8.0).max(120.0)), Sense::hover());
    let painter = ui.painter();
    let r = rect.shrink(2.0);
    painter.rect_filled(r, CornerRadius::same(10), Color32::from_rgb(16, 16, 16));
    let outline = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &outline,
        Stroke::new(1.0, theme::BORDER),
        6.0,
        5.0,
    ));
    let (title, hint) = if nothing_imported {
        ("Drag and drop media here", "or press Import")
    } else {
        ("Nothing matches your search", "")
    };
    painter.text(
        r.center() - vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(14.0),
        theme::TEXT,
    );
    painter.text(
        r.center() + vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        hint,
        FontId::proportional(12.0),
        theme::MUTED,
    );
}

fn tile(ui: &mut egui::Ui, ctx: &mut Ctx<'_>, asset: &MediaAsset, used: bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let hovered = resp.hovered();
    let missing = ctx.missing.contains(&asset.id);

    painter.rect_filled(rect, CornerRadius::same(8), theme::PANEL_2);
    let thumb = Rect::from_min_size(rect.min, vec2(TILE_W, THUMB_H));
    painter.rect_filled(
        thumb,
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        Color32::BLACK,
    );
    match ctx.thumbs.get(&asset.id) {
        Some(texture) => {
            // fit the whole picture inside the thumbnail area
            let size = texture.size_vec2();
            let scale = (thumb.width() / size.x).min(thumb.height() / size.y);
            let image_rect = Rect::from_center_size(thumb.center(), size * scale);
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            let label = if asset.kind == MediaKind::Audio {
                "Audio"
            } else {
                "…"
            };
            painter.text(
                thumb.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.0),
                theme::MUTED,
            );
        }
    }

    // name (one line, cut off with …)
    let mut job = egui::text::LayoutJob::simple_singleline(
        asset.name.clone(),
        FontId::proportional(11.5),
        theme::TEXT,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: TILE_W - 12.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        Pos2::new(rect.left() + 6.0, thumb.bottom() + 4.0),
        galley,
        theme::TEXT,
    );

    // duration badge
    if asset.kind != MediaKind::Image {
        badge(
            &painter,
            Pos2::new(thumb.right() - 4.0, thumb.bottom() - 4.0),
            &short_duration(asset.duration),
            Align2::RIGHT_BOTTOM,
            Color32::from_black_alpha(190),
            theme::TEXT,
        );
    }
    if used {
        badge(
            &painter,
            thumb.left_top() + vec2(4.0, 4.0),
            "Added",
            Align2::LEFT_TOP,
            theme::ACCENT,
            theme::ON_ACCENT,
        );
    }
    if missing {
        painter.rect_filled(
            thumb,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(160, 20, 20, 150),
        );
        painter.text(
            thumb.center(),
            Align2::CENTER_CENTER,
            "Missing",
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
    if hovered {
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
    }

    // the + button (own click area, on top of the tile)
    let plus_rect = Rect::from_center_size(thumb.right_top() + vec2(-15.0, 15.0), vec2(22.0, 22.0));
    let plus = ui.interact(plus_rect, resp.id.with("plus"), Sense::click());
    if hovered || plus.hovered() {
        let fill = if plus.hovered() {
            theme::ACCENT_HOVER
        } else {
            theme::ACCENT
        };
        painter.circle_filled(plus_rect.center(), 11.0, fill);
        painter.text(
            plus_rect.center(),
            Align2::CENTER_CENTER,
            "+",
            FontId::proportional(17.0),
            theme::ON_ACCENT,
        );
    }

    if plus.clicked() || resp.double_clicked() {
        add_to_timeline(ctx, asset.id);
    }
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), asset.id);
    }
    if resp.dragged() {
        ghost(ui.ctx(), &asset.name);
    }
    resp.on_hover_text(hover_text(asset)).context_menu(|ui| {
        if ui.button("Add to timeline").clicked() {
            add_to_timeline(ctx, asset.id);
            ui.close();
        }
        if missing && ui.button("Relink…").clicked() {
            ctx.actions.push(Action::Relink(asset.id));
            ui.close();
        }
    });
}

fn hover_text(asset: &MediaAsset) -> String {
    let mut s = asset.name.clone();
    if asset.kind != MediaKind::Audio {
        s += &format!("\n{} × {}", asset.width, asset.height);
    }
    if let Some(r) = asset.frame_rate {
        s += &format!("  {} fps", r.label());
    }
    if asset.kind != MediaKind::Image {
        s += &format!("\nLength {}", short_duration(asset.duration));
    }
    s + &format!("\n{}", asset.path.display())
}

fn badge(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    anchor: Align2,
    fill: Color32,
    fg: Color32,
) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(10.5), fg);
    let size = galley.size() + vec2(8.0, 3.0);
    let rect = anchor.anchor_size(pos, size);
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
}

/// A label that follows the mouse while a tile is dragged.
fn ghost(egui_ctx: &egui::Context, name: &str) {
    if let Some(pos) = egui_ctx.pointer_interact_pos() {
        let painter = egui_ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("drag_ghost"),
        ));
        let galley = painter.layout_no_wrap(
            name.to_string(),
            FontId::proportional(12.0),
            theme::ON_ACCENT,
        );
        let rect = Rect::from_min_size(pos + vec2(12.0, 12.0), galley.size() + vec2(14.0, 8.0));
        painter.rect_filled(rect, CornerRadius::same(6), theme::ACCENT);
        painter.galley(
            rect.center() - galley.size() / 2.0,
            galley,
            theme::ON_ACCENT,
        );
    }
}

/// Put a media file on the timeline at the playhead and select the new clip.
fn add_to_timeline(ctx: &mut Ctx<'_>, media: MediaId) {
    match ctx.doc.add_media_auto(media, ctx.state.playhead) {
        Ok(id) => ctx.state.selection = vec![id],
        Err(e) => ctx.state.say(format!("Could not add the clip: {e}")),
    }
}

/// What the grid shows until the real content exists.
fn placeholder(ui: &mut egui::Ui, category: Category, sub_menu: usize) {
    let avail = ui.available_size_before_wrap();
    let (rect, _) =
        ui.allocate_exact_size(vec2(avail.x, (avail.y - 8.0).max(120.0)), Sense::hover());
    let painter = ui.painter();
    let r = rect.shrink(2.0);
    painter.rect_filled(r, CornerRadius::same(10), Color32::from_rgb(16, 16, 16));
    let outline = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &outline,
        Stroke::new(1.0, theme::BORDER),
        6.0,
        5.0,
    ));
    painter.text(
        r.center() - vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        category
            .sub_menu()
            .get(sub_menu)
            .copied()
            .unwrap_or(category.label()),
        FontId::proportional(14.0),
        theme::TEXT,
    );
    painter.text(
        r.center() + vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        format!("Coming in Phase {}", category.phase_of(sub_menu)),
        FontId::proportional(12.0),
        theme::MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::short_duration;
    use vz_core::US_PER_SEC;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(short_duration(0), "0:00");
        assert_eq!(short_duration(3_400_000), "0:03");
        assert_eq!(short_duration(3_600_000), "0:04");
        assert_eq!(short_duration(75 * US_PER_SEC), "1:15");
        assert_eq!(short_duration(3725 * US_PER_SEC), "1:02:05");
    }
}
