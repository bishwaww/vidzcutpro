//! The asset panel (top left): category tabs, a sub-menu and a searchable grid.

use super::EditorState;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, vec2,
};

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

pub fn show(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.add_space(6.0);
    category_tabs(ui, state);
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
                        for (i, name) in state.category.sub_menu().iter().enumerate() {
                            let selected = state.sub_menu == i;
                            let text = if selected {
                                RichText::new(*name).color(theme::ACCENT)
                            } else {
                                RichText::new(*name)
                            };
                            if ui.selectable_label(selected, text).clicked() {
                                state.sub_menu = i;
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
                egui::TextEdit::singleline(&mut state.search)
                    .hint_text("Search")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);
            placeholder(ui, state.category);
        });
    });
}

fn category_tabs(ui: &mut egui::Ui, state: &mut EditorState) {
    egui::ScrollArea::horizontal()
        .id_salt("category_tabs")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                for cat in Category::ALL {
                    let selected = state.category == cat;
                    let (rect, resp) = ui.allocate_exact_size(vec2(66.0, 52.0), Sense::click());
                    let painter = ui.painter();
                    if resp.hovered() && !selected {
                        painter.rect_filled(rect, CornerRadius::same(8), theme::PANEL_2);
                    }
                    // placeholder icon: a tile with the first letter (real icons come later)
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
                        &cat.label()[..1],
                        FontId::proportional(14.0),
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
                        state.category = cat;
                        state.sub_menu = 0;
                    }
                }
            });
        });
}

/// What the grid shows until the real content exists.
fn placeholder(ui: &mut egui::Ui, category: Category) {
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
    let (title, hint) = match category {
        Category::Media => (
            "Drag and drop media here",
            "Importing arrives in Phase 1".to_string(),
        ),
        other => (other.label(), format!("Coming in Phase {}", other.phase())),
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
