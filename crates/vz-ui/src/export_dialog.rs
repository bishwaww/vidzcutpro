//! The Export window: choose what to make (video, sound only, GIF, one picture, captions), set it up,
//! watch the progress (it can be tucked away while you keep editing) and open the result.

use crate::settings::UiSettings;
use crate::theme;
use directories::UserDirs;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use vz_core::captions::{self, CaptionLine};
use vz_core::{FrameRate, Micros};
use vz_export::{
    AudioExportSettings, AudioFormat, Container, Dither, ExportError, ExportReport, ExportSettings,
    GifSettings, PRESETS, PictureFormat, Progress, Quality, VideoCodec,
};

/// Sizes offered (the short side of the picture).
const SIZES: [(&str, u32); 5] = [
    ("480p", 480),
    ("720p", 720),
    ("1080p", 1080),
    ("2K", 1440),
    ("4K", 2160),
];
/// Index of "Custom" after the sizes.
const CUSTOM_SIZE: usize = SIZES.len();

/// Frame rates offered; the first entry is "same as the project".
const RATES: [Option<FrameRate>; 6] = [
    None,
    Some(FrameRate::FPS_24),
    Some(FrameRate::FPS_25),
    Some(FrameRate::FPS_30),
    Some(FrameRate::FPS_50),
    Some(FrameRate::FPS_60),
];

const GIF_SIZES: [(&str, u32); 4] = [("240p", 240), ("360p", 360), ("480p", 480), ("720p", 720)];
const GIF_RATES: [u32; 5] = [8, 10, 12, 15, 20];

/// What the editor tells the window about the project when it opens.
#[derive(Clone, Default)]
pub struct ExportContext {
    /// Where the playhead is (the picture export uses it).
    pub playhead: Micros,
    /// From the start of the first selected clip to the end of the last, if clips are selected.
    pub selection: Option<(Micros, Micros)>,
    /// The captions on the timeline, for the captions export.
    pub captions: Vec<CaptionLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Video,
    Audio,
    Gif,
    Picture,
    Captions,
}

impl Mode {
    const ALL: [Self; 5] = [
        Self::Video,
        Self::Audio,
        Self::Gif,
        Self::Picture,
        Self::Captions,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Sound only",
            Self::Gif => "GIF",
            Self::Picture => "Picture",
            Self::Captions => "Captions",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptionFormat {
    Srt,
    Vtt,
    Txt,
}

impl CaptionFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Srt => "SRT subtitles",
            Self::Vtt => "WebVTT subtitles",
            Self::Txt => "Plain text",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Txt => "txt",
        }
    }
}

enum Stage {
    Choosing,
    Running {
        shared: Arc<Mutex<Progress>>,
        cancel: Arc<AtomicBool>,
        started: Instant,
        done: crossbeam_channel::Receiver<Result<ExportReport, ExportError>>,
    },
    Finished(ExportReport),
    Failed(String),
}

pub struct ExportDialog {
    stage: Stage,
    mode: Mode,
    context: ExportContext,
    file_name: String,
    folder: PathBuf,
    // video
    preset: Option<usize>,
    size_index: usize,
    custom_short: u32,
    rate_index: usize,
    quality: Quality,
    custom_bitrate: bool,
    custom_kbps: u32,
    codec: VideoCodec,
    container: Container,
    selection_only: bool,
    // sound only
    audio_format: AudioFormat,
    audio_kbps: usize,
    // gif
    gif_size: usize,
    gif_rate: usize,
    gif_dither: Dither,
    gif_loop: bool,
    // picture and captions
    picture_format: PictureFormat,
    caption_format: CaptionFormat,
    /// The window is shrunk to a small progress bar while the export runs.
    tucked_away: bool,
}

fn default_folder(settings: &UiSettings) -> PathBuf {
    settings
        .export_dir
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| UserDirs::new().and_then(|u| u.video_dir().map(Path::to_path_buf)))
        .or_else(|| UserDirs::new().map(|u| u.home_dir().to_path_buf()))
        .unwrap_or_default()
}

fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "Untitled project".into()
    } else {
        cleaned
    }
}

impl ExportDialog {
    pub fn new(project_name: &str, settings: &UiSettings, context: ExportContext) -> Self {
        Self {
            stage: Stage::Choosing,
            mode: Mode::Video,
            context,
            file_name: clean_name(project_name),
            folder: default_folder(settings),
            preset: None,
            size_index: 2, // 1080p
            custom_short: 1080,
            rate_index: 0,
            quality: Quality::Recommended,
            custom_bitrate: false,
            custom_kbps: 8000,
            codec: VideoCodec::H264,
            container: Container::Mp4,
            selection_only: false,
            audio_format: AudioFormat::Wav,
            audio_kbps: 192,
            gif_size: 1,
            gif_rate: 3,
            gif_dither: Dither::Ordered,
            gif_loop: true,
            picture_format: PictureFormat::Png,
            caption_format: CaptionFormat::Srt,
            tucked_away: false,
        }
    }

    /// Developer helper: export straight away into `folder` (used by the automatic screen checks).
    pub fn start_now_into(
        &mut self,
        folder: PathBuf,
        project: &vz_core::Project,
        settings: &mut UiSettings,
    ) {
        self.folder = folder;
        self.start(project, settings);
    }

    /// Open on the "Picture" page.
    pub fn show_picture_page(&mut self) {
        self.mode = Mode::Picture;
    }

    fn extension(&self) -> &'static str {
        match self.mode {
            Mode::Video => self.container.extension(),
            Mode::Audio => self.audio_format.extension(),
            Mode::Gif => "gif",
            Mode::Picture => self.picture_format.extension(),
            Mode::Captions => self.caption_format.extension(),
        }
    }

    fn output_path(&self) -> PathBuf {
        let ext = self.extension();
        let name = self.file_name.trim();
        let name = name.strip_suffix(&format!(".{ext}")).unwrap_or(name);
        self.folder.join(format!("{}.{ext}", clean_name(name)))
    }

    fn range(&self) -> Option<(Micros, Micros)> {
        if self.selection_only {
            self.context.selection
        } else {
            None
        }
    }

    fn short_side(&self) -> u32 {
        if self.size_index == CUSTOM_SIZE {
            self.custom_short.clamp(72, 4320)
        } else {
            SIZES[self.size_index].1
        }
    }

    fn apply_preset(&mut self, index: usize) {
        let p = PRESETS[index];
        self.preset = Some(index);
        match SIZES.iter().position(|(_, s)| *s == p.short_side) {
            Some(i) => self.size_index = i,
            None => {
                self.size_index = CUSTOM_SIZE;
                self.custom_short = p.short_side;
            }
        }
        self.rate_index = RATES.iter().position(|r| *r == p.frame_rate).unwrap_or(0);
        self.quality = p.quality;
        self.custom_bitrate = false;
        self.codec = p.codec;
        self.container = Container::Mp4;
    }

    /// Draw the window. Returns true when it should be closed.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        project: &vz_core::Project,
        settings: &mut UiSettings,
    ) -> bool {
        let mut close = false;
        let mut start = false;
        let mut cancel_requested = false;

        // collect a finished export
        if let Stage::Running { done, .. } = &self.stage
            && let Ok(result) = done.try_recv()
        {
            self.stage = match result {
                Ok(report) => Stage::Finished(report),
                Err(ExportError::Cancelled) => Stage::Choosing,
                Err(e) => Stage::Failed(e.to_string()),
            };
            // a finished (or failed) export always pops the window back up
            self.tucked_away = false;
        }

        // the small progress bar shown while the export runs in the background
        if self.tucked_away && matches!(self.stage, Stage::Running { .. }) {
            if let Stage::Running {
                shared, started, ..
            } = &self.stage
            {
                let p = shared.lock().map(|g| *g).unwrap_or(Progress {
                    frames_done: 0,
                    frames_total: 1,
                });
                let fraction = p.frames_done as f32 / p.frames_total.max(1) as f32;
                let started = *started;
                let mut show_again = false;
                let mut stop = false;
                egui::Window::new("Exporting")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Exporting… {:.0} %  ({} s)",
                                fraction * 100.0,
                                started.elapsed().as_secs()
                            ))
                            .strong(),
                        );
                        ui.add(egui::ProgressBar::new(fraction).desired_width(220.0));
                        ui.horizontal(|ui| {
                            if ui.small_button("Show").clicked() {
                                show_again = true;
                            }
                            if ui.small_button("Cancel").clicked() {
                                stop = true;
                            }
                        });
                    });
                if show_again {
                    self.tucked_away = false;
                }
                cancel_requested |= stop;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            if cancel_requested && let Stage::Running { cancel, .. } = &self.stage {
                cancel.store(true, Ordering::Relaxed);
            }
            return false;
        }

        egui::Window::new("Export")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .min_width(460.0)
            .show(ctx, |ui| match &self.stage {
                Stage::Choosing => {
                    ui.horizontal(|ui| {
                        for m in Mode::ALL {
                            if ui.selectable_label(self.mode == m, m.label()).clicked() {
                                self.mode = m;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    self.choosing(ui, project);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let can_go = self.can_start(project);
                        if ui
                            .add_enabled(can_go, theme::primary_button("Export"))
                            .clicked()
                        {
                            start = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }
                Stage::Running {
                    shared, started, ..
                } => {
                    let p = shared.lock().map(|g| *g).unwrap_or(Progress {
                        frames_done: 0,
                        frames_total: 1,
                    });
                    let fraction = p.frames_done as f32 / p.frames_total.max(1) as f32;
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .desired_width(420.0),
                    );
                    let elapsed = started.elapsed().as_secs_f64();
                    let eta = if p.frames_done > 8 {
                        elapsed / p.frames_done as f64 * (p.frames_total - p.frames_done) as f64
                    } else {
                        f64::NAN
                    };
                    let eta_text = if eta.is_finite() {
                        format!("about {} left", human_seconds(eta))
                    } else {
                        "working…".to_string()
                    };
                    let progress_text = match self.mode {
                        Mode::Audio => format!("Writing the sound  ·  {eta_text}"),
                        Mode::Gif => format!(
                            "Picture {} of {}  ·  {eta_text}",
                            p.frames_done, p.frames_total
                        ),
                        _ => format!(
                            "Frame {} of {}  ·  {eta_text}",
                            p.frames_done, p.frames_total
                        ),
                    };
                    ui.label(
                        egui::RichText::new(progress_text)
                            .color(theme::MUTED)
                            .small(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button("Keep editing")
                            .on_hover_text("Tuck this window away; the export carries on")
                            .clicked()
                        {
                            self.tucked_away = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_requested = true;
                        }
                    });
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Stage::Finished(report) => {
                    ui.label(
                        egui::RichText::new("Export finished")
                            .strong()
                            .color(theme::ACCENT),
                    );
                    let detail = match self.mode {
                        Mode::Video => format!(
                            "{:.1} MB, {} frames, {:.0} s using {}",
                            report.bytes as f64 / 1e6,
                            report.frames,
                            report.seconds,
                            report.encoder
                        ),
                        Mode::Gif => format!(
                            "{:.1} MB, {} pictures, {:.0} s",
                            report.bytes as f64 / 1e6,
                            report.frames,
                            report.seconds
                        ),
                        _ => format!("{:.1} MB", report.bytes as f64 / 1e6),
                    };
                    ui.label(detail);
                    ui.label(
                        egui::RichText::new(report.path.display().to_string())
                            .small()
                            .color(theme::MUTED),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary_button("Open folder")).clicked() {
                            reveal_in_folder(&report.path);
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }
                Stage::Failed(message) => {
                    ui.label(
                        egui::RichText::new("The export did not work")
                            .strong()
                            .color(egui::Color32::LIGHT_RED),
                    );
                    ui.label(message.as_str());
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });

        if cancel_requested && let Stage::Running { cancel, .. } = &self.stage {
            cancel.store(true, Ordering::Relaxed);
        }
        if start {
            self.start(project, settings);
        }
        // closing while running means cancelling
        if close && let Stage::Running { cancel, .. } = &self.stage {
            cancel.store(true, Ordering::Relaxed);
        }
        close
    }

    fn can_start(&self, project: &vz_core::Project) -> bool {
        match self.mode {
            Mode::Captions => !self.context.captions.is_empty(),
            Mode::Picture => true,
            _ => project.duration() > 0,
        }
    }

    // ---- the choices of each mode ------------------------------------------------------------------------------------

    fn choosing(&mut self, ui: &mut egui::Ui, project: &vz_core::Project) {
        egui::Grid::new("export_grid")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.file_name).desired_width(280.0));
                ui.end_row();

                ui.label("Save in");
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.folder.display().to_string()).small());
                    if ui.small_button("Change…").clicked()
                        && let Some(dir) = rfd::FileDialog::new()
                            .set_directory(&self.folder)
                            .pick_folder()
                    {
                        self.folder = dir;
                    }
                });
                ui.end_row();

                match self.mode {
                    Mode::Video => self.video_rows(ui, project),
                    Mode::Audio => self.audio_rows(ui, project),
                    Mode::Gif => self.gif_rows(ui, project),
                    Mode::Picture => self.picture_rows(ui, project),
                    Mode::Captions => self.caption_rows(ui),
                }
                if matches!(self.mode, Mode::Video | Mode::Audio | Mode::Gif) {
                    self.range_row(ui);
                }
            });
        if self.mode == Mode::Video
            && let Some(i) = self.preset
        {
            let p = PRESETS[i];
            ui.add_space(4.0);
            ui.label(egui::RichText::new(p.note).color(theme::MUTED).small());
            if !p.fits_canvas(project.canvas.width, project.canvas.height)
                && let Some((w, h)) = p.aspect
            {
                ui.label(
                    egui::RichText::new(format!(
                        "This preset is made for {w}:{h} pictures but your project is {} × {}. \
                         Change the Aspect ratio in Details first, or export anyway.",
                        project.canvas.width, project.canvas.height
                    ))
                    .color(egui::Color32::from_rgb(230, 170, 60))
                    .small(),
                );
            }
        }
    }

    fn range_row(&mut self, ui: &mut egui::Ui) {
        ui.label("What to export");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.selection_only, false, "Whole timeline");
            ui.add_enabled_ui(self.context.selection.is_some(), |ui| {
                ui.selectable_value(&mut self.selection_only, true, "Selected clips")
                    .on_disabled_hover_text("Select some clips on the timeline first");
            });
        });
        ui.end_row();
    }

    fn video_rows(&mut self, ui: &mut egui::Ui, project: &vz_core::Project) {
        ui.label("Preset");
        let shown = self.preset.map_or("Custom", |i| PRESETS[i].name);
        egui::ComboBox::from_id_salt("export_preset")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.preset.is_none(), "Custom")
                    .clicked()
                {
                    self.preset = None;
                }
                for (i, p) in PRESETS.iter().enumerate() {
                    if ui
                        .selectable_label(self.preset == Some(i), p.name)
                        .clicked()
                    {
                        self.apply_preset(i);
                    }
                }
            });
        ui.end_row();

        ui.label("Resolution");
        ui.horizontal(|ui| {
            let shown = if self.size_index == CUSTOM_SIZE {
                "Custom"
            } else {
                SIZES[self.size_index].0
            };
            egui::ComboBox::from_id_salt("export_size")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (i, (label, _)) in SIZES.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.size_index, i, *label)
                            .clicked()
                        {
                            self.preset = None;
                        }
                    }
                    if ui
                        .selectable_value(&mut self.size_index, CUSTOM_SIZE, "Custom")
                        .clicked()
                    {
                        self.preset = None;
                    }
                });
            if self.size_index == CUSTOM_SIZE {
                ui.add(
                    egui::DragValue::new(&mut self.custom_short)
                        .range(72..=4320)
                        .speed(4.0)
                        .suffix(" px short side"),
                );
            }
        });
        ui.end_row();

        ui.label("Frame rate");
        let shown = RATES[self.rate_index].map_or_else(
            || format!("Same as project ({} fps)", project.frame_rate.label()),
            |r| format!("{} fps", r.label()),
        );
        egui::ComboBox::from_id_salt("export_rate")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (i, r) in RATES.iter().enumerate() {
                    let label = r.map_or_else(
                        || format!("Same as project ({} fps)", project.frame_rate.label()),
                        |r| format!("{} fps", r.label()),
                    );
                    if ui
                        .selectable_value(&mut self.rate_index, i, label)
                        .clicked()
                    {
                        self.preset = None;
                    }
                }
            });
        ui.end_row();

        ui.label("Quality");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("export_quality")
                .selected_text(if self.custom_bitrate {
                    "Custom"
                } else {
                    self.quality.label()
                })
                .show_ui(ui, |ui| {
                    for q in Quality::ALL {
                        if ui
                            .selectable_label(!self.custom_bitrate && self.quality == q, q.label())
                            .clicked()
                        {
                            self.quality = q;
                            self.custom_bitrate = false;
                            self.preset = None;
                        }
                    }
                    if ui
                        .selectable_label(self.custom_bitrate, "Custom bit rate")
                        .clicked()
                    {
                        self.custom_bitrate = true;
                        self.preset = None;
                    }
                });
            if self.custom_bitrate {
                ui.add(
                    egui::DragValue::new(&mut self.custom_kbps)
                        .range(200..=200_000)
                        .speed(50.0)
                        .suffix(" kbps"),
                );
            }
        });
        ui.end_row();

        ui.label("Video format");
        egui::ComboBox::from_id_salt("export_codec")
            .selected_text(self.codec.label())
            .show_ui(ui, |ui| {
                for c in VideoCodec::ALL {
                    if c.is_available() {
                        ui.selectable_value(&mut self.codec, c, c.label());
                    }
                }
            });
        ui.end_row();

        ui.label("File type");
        ui.horizontal(|ui| {
            for c in Container::ALL {
                ui.selectable_value(&mut self.container, c, c.label());
            }
        });
        ui.end_row();

        ui.label("Estimated size");
        let es = self.video_settings(project);
        let seconds = self.length(project) as f64 / 1e6;
        let bytes = (es.video_bit_rate() + 192_000) as f64 / 8.0 * seconds;
        ui.label(format!(
            "{:.1} MB  ({} × {}, {} fps)",
            bytes / 1e6,
            es.width,
            es.height,
            es.frame_rate.label()
        ));
        ui.end_row();
    }

    fn audio_rows(&mut self, ui: &mut egui::Ui, project: &vz_core::Project) {
        ui.label("Format");
        egui::ComboBox::from_id_salt("export_audio_format")
            .selected_text(self.audio_format.label())
            .show_ui(ui, |ui| {
                for f in AudioFormat::ALL {
                    if f.is_available() {
                        ui.selectable_value(&mut self.audio_format, f, f.label());
                    }
                }
            });
        ui.end_row();
        if self.audio_format.is_lossy() {
            ui.label("Quality");
            egui::ComboBox::from_id_salt("export_audio_rate")
                .selected_text(format!("{} kbps", self.audio_kbps))
                .show_ui(ui, |ui| {
                    for k in [96, 128, 192, 256, 320] {
                        ui.selectable_value(&mut self.audio_kbps, k, format!("{k} kbps"));
                    }
                });
            ui.end_row();
        }
        ui.label("Estimated size");
        let seconds = self.length(project) as f64 / 1e6;
        let bytes = match self.audio_format {
            AudioFormat::Wav => 48_000.0 * 4.0 * seconds,
            AudioFormat::Flac => 48_000.0 * 4.0 * seconds * 0.6,
            _ => self.audio_kbps as f64 * 1000.0 / 8.0 * seconds,
        };
        ui.label(format!("{:.1} MB", bytes / 1e6));
        ui.end_row();
    }

    fn gif_rows(&mut self, ui: &mut egui::Ui, project: &vz_core::Project) {
        ui.label("Size");
        egui::ComboBox::from_id_salt("export_gif_size")
            .selected_text(GIF_SIZES[self.gif_size].0)
            .show_ui(ui, |ui| {
                for (i, (label, _)) in GIF_SIZES.iter().enumerate() {
                    ui.selectable_value(&mut self.gif_size, i, *label);
                }
            });
        ui.end_row();
        ui.label("Pictures per second");
        egui::ComboBox::from_id_salt("export_gif_rate")
            .selected_text(GIF_RATES[self.gif_rate].to_string())
            .show_ui(ui, |ui| {
                for (i, r) in GIF_RATES.iter().enumerate() {
                    ui.selectable_value(&mut self.gif_rate, i, r.to_string());
                }
            });
        ui.end_row();
        ui.label("Dithering");
        egui::ComboBox::from_id_salt("export_gif_dither")
            .selected_text(self.gif_dither.label())
            .show_ui(ui, |ui| {
                for d in Dither::ALL {
                    ui.selectable_value(&mut self.gif_dither, d, d.label());
                }
            });
        ui.end_row();
        ui.label("Repeat");
        ui.checkbox(&mut self.gif_loop, "Play again and again");
        ui.end_row();
        ui.label("Estimated size");
        let g = self.gif_settings(project);
        let seconds = self.length(project) as f64 / 1e6;
        // a rough rule: about 0.4 byte per pixel per picture
        let bytes = f64::from(g.width) * f64::from(g.height) * f64::from(g.fps) * seconds * 0.4;
        ui.label(format!(
            "about {:.1} MB  ({} × {})",
            bytes / 1e6,
            g.width,
            g.height
        ));
        ui.end_row();
        if seconds > 20.0 {
            ui.label("");
            ui.label(
                egui::RichText::new("GIFs get very big when they are long. Try a shorter part.")
                    .color(egui::Color32::from_rgb(230, 170, 60))
                    .small(),
            );
            ui.end_row();
        }
    }

    fn picture_rows(&mut self, ui: &mut egui::Ui, project: &vz_core::Project) {
        ui.label("Picture at");
        ui.label(project.frame_rate.timecode(self.context.playhead));
        ui.end_row();
        ui.label("Format");
        ui.horizontal(|ui| {
            for f in PictureFormat::ALL {
                ui.selectable_value(&mut self.picture_format, f, f.label());
            }
        });
        ui.end_row();
        ui.label("Size");
        egui::ComboBox::from_id_salt("export_picture_size")
            .selected_text(SIZES[self.size_index.min(SIZES.len() - 1)].0)
            .show_ui(ui, |ui| {
                for (i, (label, _)) in SIZES.iter().enumerate() {
                    ui.selectable_value(&mut self.size_index, i, *label);
                }
            });
        ui.end_row();
        ui.label("");
        let (w, h) = self.picture_size(project);
        ui.label(
            egui::RichText::new(format!("{w} × {h} pixels"))
                .color(theme::MUTED)
                .small(),
        );
        ui.end_row();
    }

    fn caption_rows(&mut self, ui: &mut egui::Ui) {
        ui.label("Format");
        egui::ComboBox::from_id_salt("export_caption_format")
            .selected_text(self.caption_format.label())
            .show_ui(ui, |ui| {
                for f in [CaptionFormat::Srt, CaptionFormat::Vtt, CaptionFormat::Txt] {
                    ui.selectable_value(&mut self.caption_format, f, f.label());
                }
            });
        ui.end_row();
        ui.label("Captions");
        if self.context.captions.is_empty() {
            ui.label(
                egui::RichText::new("There are no captions on the timeline.").color(theme::MUTED),
            );
        } else {
            ui.label(format!("{} captions", self.context.captions.len()));
        }
        ui.end_row();
    }

    // ---- turning the choices into settings -----------------------------------------------------------------------------

    /// How long the part to export is.
    fn length(&self, project: &vz_core::Project) -> Micros {
        match self.range() {
            Some((a, b)) => (b - a).max(0),
            None => project.duration(),
        }
    }

    fn picture_size(&self, project: &vz_core::Project) -> (u32, u32) {
        vz_export::size_for_short_side(
            project.canvas.width,
            project.canvas.height,
            SIZES[self.size_index.min(SIZES.len() - 1)].1,
        )
    }

    fn video_settings(&self, project: &vz_core::Project) -> ExportSettings {
        let mut s = ExportSettings::for_project(project, self.output_path(), self.short_side());
        s.quality = self.quality;
        s.custom_kbps = self.custom_bitrate.then_some(self.custom_kbps);
        s.codec = self.codec;
        s.container = self.container;
        if let Some(rate) = RATES[self.rate_index] {
            s.frame_rate = rate;
        }
        s.range = self.range();
        s
    }

    fn gif_settings(&self, project: &vz_core::Project) -> GifSettings {
        let mut g =
            GifSettings::for_project(project, self.output_path(), GIF_SIZES[self.gif_size].1);
        g.fps = GIF_RATES[self.gif_rate];
        g.dither = self.gif_dither;
        g.looping = self.gif_loop;
        g.range = self.range();
        g
    }

    fn start(&mut self, project: &vz_core::Project, settings: &mut UiSettings) {
        if let Err(e) = std::fs::create_dir_all(&self.folder) {
            self.stage = Stage::Failed(format!("Could not use that folder: {e}"));
            return;
        }
        settings.export_dir = Some(self.folder.clone());
        settings.save();
        let path = self.output_path();

        // things that are done in a moment
        match self.mode {
            Mode::Picture => {
                let size = self.picture_size(project);
                self.stage = match vz_export::save_picture(
                    project,
                    self.context.playhead,
                    size,
                    self.picture_format,
                    &path,
                ) {
                    Ok(()) => Stage::Finished(report_for(&path, "picture", 1)),
                    Err(e) => Stage::Failed(e.to_string()),
                };
                return;
            }
            Mode::Captions => {
                let lines = &self.context.captions;
                let text = match self.caption_format {
                    CaptionFormat::Srt => captions::write_srt(lines),
                    CaptionFormat::Vtt => captions::write_vtt(lines),
                    CaptionFormat::Txt => captions::write_txt(lines),
                };
                self.stage = match std::fs::write(&path, text) {
                    Ok(()) => Stage::Finished(report_for(&path, "captions", lines.len() as u64)),
                    Err(e) => Stage::Failed(format!("Could not write the file: {e}")),
                };
                return;
            }
            _ => {}
        }

        let job = match self.mode {
            Mode::Video => Job::Video(self.video_settings(project)),
            Mode::Audio => {
                let mut a = AudioExportSettings::new(path, self.audio_format);
                a.bit_rate = self.audio_kbps * 1000;
                a.range = self.range();
                Job::Audio(a)
            }
            _ => Job::Gif(self.gif_settings(project)),
        };
        let project = project.clone();
        let shared = Arc::new(Mutex::new(Progress {
            frames_done: 0,
            frames_total: 1,
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded(1);
        {
            let (shared, cancel) = (shared.clone(), cancel.clone());
            std::thread::Builder::new()
                .name("vidzcut-export".into())
                .spawn(move || {
                    let mut report = |p| {
                        if let Ok(mut g) = shared.lock() {
                            *g = p;
                        }
                    };
                    let result = match job {
                        Job::Video(s) => vz_export::export(&project, &s, &mut report, &cancel),
                        Job::Audio(a) => {
                            vz_export::export_audio(&project, &a, &mut report, &cancel)
                        }
                        Job::Gif(g) => vz_export::export_gif(&project, &g, &mut report, &cancel),
                    };
                    let _ = tx.send(result);
                })
                .expect("could not start the export thread");
        }
        self.stage = Stage::Running {
            shared,
            cancel,
            started: Instant::now(),
            done: rx,
        };
    }
}

/// What the export thread has to do.
enum Job {
    Video(ExportSettings),
    Audio(AudioExportSettings),
    Gif(GifSettings),
}

fn report_for(path: &Path, what: &str, frames: u64) -> ExportReport {
    ExportReport {
        encoder: what.to_string(),
        frames,
        seconds: 0.0,
        bytes: std::fs::metadata(path).map_or(0, |m| m.len()),
        path: path.to_path_buf(),
    }
}

fn human_seconds(s: f64) -> String {
    let s = s.round() as u64;
    if s >= 60 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

/// Show a file in the system's file manager.
pub fn reveal_in_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    if let Err(e) = result {
        tracing::warn!("could not open the folder: {e}");
    }
}
