//! vidzcut's look: near-black panels with the logo's yellow as the accent.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};

pub const BG: Color32 = Color32::from_rgb(10, 10, 10);
pub const PANEL: Color32 = Color32::from_rgb(21, 21, 21);
pub const PANEL_2: Color32 = Color32::from_rgb(32, 32, 32);
pub const BORDER: Color32 = Color32::from_rgb(44, 44, 44);
pub const ACCENT: Color32 = Color32::from_rgb(0xFC, 0xC6, 0x10);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xFF, 0xD8, 0x4D);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x2E, 0x26, 0x06);
pub const ON_ACCENT: Color32 = Color32::from_rgb(10, 10, 10);
pub const TEXT: Color32 = Color32::from_rgb(232, 232, 232);
pub const MUTED: Color32 = Color32::from_rgb(138, 138, 138);

/// Set colours, spacing and fonts for the whole app.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(6, 6, 6);
    v.faint_bg_color = PANEL;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DARK;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.inactive.bg_fill = PANEL_2;
    v.widgets.inactive.weak_bg_fill = PANEL_2;
    v.widgets.hovered.bg_fill = Color32::from_rgb(48, 48, 48);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 48, 48);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = ACCENT_DARK;
    v.widgets.active.weak_bg_fill = ACCENT_DARK;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    let round = CornerRadius::same(6);
    v.widgets.noninteractive.corner_radius = round;
    v.widgets.inactive.corner_radius = round;
    v.widgets.hovered.corner_radius = round;
    v.widgets.active.corner_radius = round;
    v.widgets.open.corner_radius = round;
    ctx.set_visuals(v);

    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 26.0;
        style.spacing.scroll.bar_width = 8.0;
    });
}

/// egui ships Latin letters only. Sinhala and Tamil need a font that has them.
/// TEMPORARY (Phase 0): use the font Windows already has. Phase 2 bundles Noto Sans Sinhala / Tamil (OFL).
fn install_fonts(ctx: &egui::Context) {
    // Sinhala and Tamil letters come with the program, so they look the same on every computer
    let mut fonts = FontDefinitions::default();
    let bundled: [(&str, &'static [u8]); 3] = [
        (
            "noto-sinhala",
            include_bytes!("../../../assets/fonts/NotoSansSinhala-Variable.ttf"),
        ),
        (
            "noto-tamil",
            include_bytes!("../../../assets/fonts/NotoSansTamil-Variable.ttf"),
        ),
        ("lucide", include_bytes!("../../../assets/icons/lucide.ttf")),
    ];
    for (name, bytes) in bundled {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
    }
    // a system font last, for symbols (arrows, bullets, full-width signs) the bundled fonts do not have
    let system = [
        r"C:\Windows\Fonts\Nirmala.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .find_map(|p| std::fs::read(p).ok());
    let has_system = system.is_some();
    if let Some(bytes) = system {
        fonts
            .font_data
            .insert("system-symbols".into(), FontData::from_owned(bytes).into());
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.push("noto-sinhala".into());
        list.push("noto-tamil".into());
        if has_system {
            list.push("system-symbols".into());
        }
    }
    fonts
        .families
        .insert(icons::family(), vec!["lucide".into()]);
    ctx.set_fonts(fonts);
}

/// Icons from the Lucide icon font (ISC licence, see THIRD_PARTY_LICENSES.md).
pub mod icons {
    use eframe::egui::{FontFamily, FontId};

    pub fn family() -> FontFamily {
        FontFamily::Name("lucide".into())
    }

    /// A font to draw icons in, `size` points tall.
    pub fn font(size: f32) -> FontId {
        FontId::new(size, family())
    }

    pub const CLAPPERBOARD: char = '\u{e29b}';
    pub const MUSIC: char = '\u{e122}';
    pub const TYPE: char = '\u{e198}';
    pub const STICKER: char = '\u{e302}';
    pub const SPARKLES: char = '\u{e412}';
    pub const BLEND: char = '\u{e59c}';
    pub const CAPTIONS: char = '\u{e3a4}';
    pub const WAND: char = '\u{e357}';
    pub const SLIDERS: char = '\u{e29a}';
    pub const TEMPLATE: char = '\u{e207}';
}

/// The big yellow call-to-action button.
pub fn primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.into()).color(ON_ACCENT).strong())
        .fill(ACCENT)
        .stroke(Stroke::NONE)
}

/// A panel background frame.
pub fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(0))
}
