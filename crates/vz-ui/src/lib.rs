//! Every panel the user sees (egui). See docs/ARCHITECTURE.md.

mod app;
mod dev;
mod editor;
mod export_dialog;
mod home;
mod preview;
mod settings;
mod theme;
mod waveforms;

/// Open the vidzcut window and run until it is closed.
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("vidzcut")
            .with_inner_size([1440.0, 820.0])
            .with_min_inner_size([1000.0, 600.0])
            .with_icon(app::window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "vidzcut",
        options,
        Box::new(|cc| Ok(Box::new(app::VidzApp::new(cc)))),
    )
}

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-ui";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-ui");
    }
}
