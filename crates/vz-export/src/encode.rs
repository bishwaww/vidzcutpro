//! The export pipeline: render frames, mix sound, encode, write an MP4.

use crate::ExportSettings;
use ff::format::Pixel;
use ff::format::sample::{Sample, Type as SampleType};
use ff::software::scaling::{Context as Scaler, Flags};
use ff::{ChannelLayout, Dictionary, Packet, Rational, codec, encoder, format, frame};
use ffmpeg_next as ff;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vz_audio::{Mixer, micros_to_samples};
use vz_core::Project;
use vz_render::Renderer;

const AUDIO_RATE: u32 = 48_000;
const AUDIO_FRAME: usize = 1024;
const AUDIO_BIT_RATE: usize = 192_000;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the export was cancelled")]
    Cancelled,
    #[error("there is nothing to export: the timeline is empty")]
    Empty,
    #[error("no video encoder worked on this computer ({0})")]
    NoEncoder(String),
    #[error("{0}")]
    Message(String),
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ff::Error),
    #[error("could not write the file: {0}")]
    Io(#[from] std::io::Error),
}

/// Kept for callers that only care whether an error was a cancel.
pub type Cancelled = ExportError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub frames_done: u64,
    pub frames_total: u64,
}

#[derive(Clone, Debug)]
pub struct ExportReport {
    /// Which encoder wrote the video (e.g. `h264_qsv`).
    pub encoder: String,
    pub frames: u64,
    pub seconds: f64,
    pub bytes: u64,
    pub path: PathBuf,
}

/// Encoders to try, best first.
fn encoder_candidates() -> Vec<&'static str> {
    let all: &[&str] = if cfg!(target_os = "macos") {
        &["h264_videotoolbox", "libopenh264"]
    } else {
        &["h264_nvenc", "h264_qsv", "h264_amf", "libopenh264"]
    };
    all.iter()
        .copied()
        .filter(|n| encoder::find_by_name(n).is_some())
        .collect()
}

/// Write `project` to `settings.path`. `progress` is called regularly; set `cancel` to stop early
/// (the partly written file is deleted).
pub fn export(
    project: &Project,
    settings: &ExportSettings,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<ExportReport, ExportError> {
    vz_media::init()?;
    let duration = project.duration();
    let (start, end) = settings.range.unwrap_or((0, duration));
    let (start, end) = (start.clamp(0, duration), end.clamp(0, duration));
    if end <= start {
        return Err(ExportError::Empty);
    }
    let rate = settings.frame_rate;
    let first_frame = rate.frame_at(start);
    let frames_total = (rate.frame_at(end - 1) - first_frame + 1).max(1) as u64;

    let partial = partial_path(&settings.path);
    let candidates = encoder_candidates();
    if candidates.is_empty() {
        return Err(ExportError::NoEncoder("none are installed".into()));
    }
    let mut problems = Vec::new();
    for name in candidates {
        let began = Instant::now();
        let job = Job {
            project,
            settings,
            first_frame,
            frames_total,
            start,
            partial: &partial,
        };
        match job.run(name, progress, cancel) {
            Ok(frames) => {
                std::fs::rename(&partial, &settings.path)?;
                let bytes = std::fs::metadata(&settings.path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                return Ok(ExportReport {
                    encoder: name.to_string(),
                    frames,
                    seconds: began.elapsed().as_secs_f64(),
                    bytes,
                    path: settings.path.clone(),
                });
            }
            Err((ExportError::Cancelled, _)) => {
                let _ = std::fs::remove_file(&partial);
                return Err(ExportError::Cancelled);
            }
            Err((e, frames_written)) => {
                let _ = std::fs::remove_file(&partial);
                tracing::warn!("encoder {name} failed after {frames_written} frames: {e}");
                // an encoder that fails right away (driver too old, no such hardware) is skipped;
                // one that fails late is a real problem, so report it instead of hiding it
                if frames_written > 8 {
                    return Err(e);
                }
                problems.push(format!("{name}: {e}"));
            }
        }
    }
    Err(ExportError::NoEncoder(problems.join("; ")))
}

fn partial_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".partial");
    PathBuf::from(p)
}

struct Job<'a> {
    project: &'a Project,
    settings: &'a ExportSettings,
    first_frame: i64,
    frames_total: u64,
    start: vz_core::Micros,
    partial: &'a Path,
}

type Attempt<T> = Result<T, (ExportError, u64)>;

impl Job<'_> {
    /// One try with one encoder. On failure also says how many frames had been written.
    fn run(
        &self,
        encoder_name: &str,
        progress: &mut dyn FnMut(Progress),
        cancel: &AtomicBool,
    ) -> Attempt<u64> {
        let mut written = 0u64;
        self.run_inner(encoder_name, progress, cancel, &mut written)
            .map_err(|e| (e, written))
    }

    fn run_inner(
        &self,
        encoder_name: &str,
        progress: &mut dyn FnMut(Progress),
        cancel: &AtomicBool,
        written: &mut u64,
    ) -> Result<u64, ExportError> {
        let s = self.settings;
        let rate = s.frame_rate;
        let (w, h) = (s.width, s.height);
        let vcodec = encoder::find_by_name(encoder_name).ok_or_else(|| {
            ExportError::Message(format!("encoder {encoder_name} is not available"))
        })?;
        let acodec = encoder::find(codec::Id::AAC)
            .ok_or_else(|| ExportError::Message("no AAC encoder".into()))?;

        let mut octx = format::output_as(&self.partial, "mp4")?;
        let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);

        // ---- video encoder ----
        let pix = pick_pixel_format(vcodec);
        let mut venc = codec::context::Context::new_with_codec(vcodec)
            .encoder()
            .video()?;
        venc.set_width(w);
        venc.set_height(h);
        venc.set_format(pix);
        venc.set_time_base(Rational(rate.den as i32, rate.num as i32));
        venc.set_frame_rate(Some(Rational(rate.num as i32, rate.den as i32)));
        venc.set_bit_rate(s.video_bit_rate().max(200_000) as usize);
        venc.set_gop((rate.nominal() * 2) as u32);
        venc.set_max_b_frames(0);
        venc.set_colorspace(ff::color::Space::BT709);
        venc.set_color_range(ff::color::Range::MPEG);
        venc.set_color_primaries(ff::color::Primaries::BT709);
        venc.set_color_transfer_characteristic(ff::color::TransferCharacteristic::BT709);
        if global_header {
            venc.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let mut options = Dictionary::new();
        match encoder_name {
            "h264_nvenc" => {
                options.set("preset", "p4");
                options.set("tune", "hq");
            }
            "h264_qsv" => options.set("preset", "medium"),
            "h264_amf" => options.set("quality", "balanced"),
            _ => {}
        }
        let mut venc = venc.open_as_with(vcodec, options)?;
        let mut vstream = octx.add_stream(vcodec)?;
        vstream.set_parameters(&venc);
        vstream.set_time_base(Rational(rate.den as i32, rate.num as i32));
        let video_index = vstream.index();

        // ---- audio encoder ----
        let mut aenc = codec::context::Context::new_with_codec(acodec)
            .encoder()
            .audio()?;
        aenc.set_rate(AUDIO_RATE as i32);
        aenc.set_channel_layout(ChannelLayout::STEREO);
        aenc.set_format(Sample::F32(SampleType::Planar));
        aenc.set_bit_rate(AUDIO_BIT_RATE);
        aenc.set_time_base(Rational(1, AUDIO_RATE as i32));
        if global_header {
            aenc.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let mut aenc = aenc.open_as(acodec)?;
        let mut astream = octx.add_stream(acodec)?;
        astream.set_parameters(&aenc);
        astream.set_time_base(Rational(1, AUDIO_RATE as i32));
        let audio_index = astream.index();

        let mut header_options = Dictionary::new();
        header_options.set("movflags", "+faststart");
        octx.write_header_with(header_options)?;
        let video_tb = octx
            .stream(video_index)
            .expect("video stream exists")
            .time_base();
        let audio_tb = octx
            .stream(audio_index)
            .expect("audio stream exists")
            .time_base();
        let venc_tb = Rational(rate.den as i32, rate.num as i32);
        let aenc_tb = Rational(1, AUDIO_RATE as i32);

        // ---- colour conversion: RGBA picture to the encoder's pixel format, with the HD (BT.709) matrix ----
        let mut scaler = Scaler::get(Pixel::RGBA, w, h, pix, w, h, Flags::BILINEAR)?;
        set_bt709(&mut scaler);
        let mut rgba_frame = frame::Video::new(Pixel::RGBA, w, h);
        let mut yuv_frame = frame::Video::new(pix, w, h);

        let mut renderer = Renderer::new();
        let mut mixer = Mixer::new(AUDIO_RATE);
        let mut audio_buf = Vec::new();
        let audio_start = micros_to_samples(self.start, AUDIO_RATE);
        let total_audio_samples = {
            let secs = self.frames_total as f64 / rate.as_f64();
            ((secs * f64::from(AUDIO_RATE)) / AUDIO_FRAME as f64).ceil() as i64 * AUDIO_FRAME as i64
        };
        let mut audio_written: i64 = 0;
        let mut packet = Packet::empty();

        for i in 0..self.frames_total {
            if cancel.load(Ordering::Relaxed) {
                return Err(ExportError::Cancelled);
            }
            let t = rate.frame_start(self.first_frame + i as i64);
            let picture = renderer.render(self.project, t, (w, h));
            fill_rgba(&mut rgba_frame, &picture.rgba, w, h);
            scaler.run(&rgba_frame, &mut yuv_frame)?;
            yuv_frame.set_pts(Some(i as i64));
            venc.send_frame(&yuv_frame)?;
            drain_video(
                &mut venc,
                &mut packet,
                &mut octx,
                video_index,
                venc_tb,
                video_tb,
                written,
            )?;

            // sound up to the end of this picture
            let target = ((i + 1) as f64 / rate.as_f64() * f64::from(AUDIO_RATE)) as i64;
            while audio_written < target.min(total_audio_samples) {
                write_audio_chunk(
                    &mut mixer,
                    self.project,
                    audio_start + audio_written,
                    audio_written,
                    &mut audio_buf,
                    &mut aenc,
                )?;
                audio_written += AUDIO_FRAME as i64;
                drain_audio(
                    &mut aenc,
                    &mut packet,
                    &mut octx,
                    audio_index,
                    aenc_tb,
                    audio_tb,
                )?;
            }
            if i % 8 == 0 || i + 1 == self.frames_total {
                progress(Progress {
                    frames_done: i + 1,
                    frames_total: self.frames_total,
                });
            }
        }
        // the rest of the sound, then flush both encoders
        while audio_written < total_audio_samples {
            write_audio_chunk(
                &mut mixer,
                self.project,
                audio_start + audio_written,
                audio_written,
                &mut audio_buf,
                &mut aenc,
            )?;
            audio_written += AUDIO_FRAME as i64;
            drain_audio(
                &mut aenc,
                &mut packet,
                &mut octx,
                audio_index,
                aenc_tb,
                audio_tb,
            )?;
        }
        venc.send_eof()?;
        drain_video(
            &mut venc,
            &mut packet,
            &mut octx,
            video_index,
            venc_tb,
            video_tb,
            written,
        )?;
        aenc.send_eof()?;
        drain_audio(
            &mut aenc,
            &mut packet,
            &mut octx,
            audio_index,
            aenc_tb,
            audio_tb,
        )?;
        octx.write_trailer()?;
        Ok(*written)
    }
}

/// Use the encoder's own preference: YUV420P if it takes it, else NV12 (what Intel's encoder wants).
fn pick_pixel_format(codec: ff::Codec) -> Pixel {
    let formats: Vec<Pixel> = codec
        .video()
        .ok()
        .and_then(|v| v.formats())
        .map(|f| f.collect())
        .unwrap_or_default();
    [Pixel::YUV420P, Pixel::NV12]
        .into_iter()
        .find(|p| formats.contains(p))
        .or_else(|| formats.first().copied())
        .unwrap_or(Pixel::YUV420P)
}

/// Make the RGB→YUV conversion use the HD colour matrix (BT.709) and limited range, matching how we tag the file.
fn set_bt709(scaler: &mut Scaler) {
    // SAFETY: the pointer is the scaler's own live SwsContext and the table pointers come from FFmpeg.
    unsafe {
        let coefficients = ff::ffi::sws_getCoefficients(ff::ffi::SWS_CS_ITU709);
        ff::ffi::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            coefficients,
            1,
            coefficients,
            0,
            0,
            1 << 16,
            1 << 16,
        );
    }
}

/// Copy a packed RGBA picture into an FFmpeg frame (whose rows may be padded).
fn fill_rgba(frame: &mut frame::Video, rgba: &[u8], w: u32, h: u32) {
    let stride = frame.stride(0);
    let row = (w * 4) as usize;
    let data = frame.data_mut(0);
    for y in 0..h as usize {
        data[y * stride..y * stride + row].copy_from_slice(&rgba[y * row..(y + 1) * row]);
    }
}

/// Mix 1024 sound frames starting at timeline sample `from` and give them to the AAC encoder.
fn write_audio_chunk(
    mixer: &mut Mixer,
    project: &Project,
    from: i64,
    pts: i64,
    buf: &mut Vec<f32>,
    aenc: &mut encoder::Audio,
) -> Result<(), ExportError> {
    mixer.render(project, from, AUDIO_FRAME, buf);
    let mut frame = frame::Audio::new(
        Sample::F32(SampleType::Planar),
        AUDIO_FRAME,
        ChannelLayout::STEREO,
    );
    frame.set_rate(AUDIO_RATE);
    frame.set_pts(Some(pts));
    for channel in 0..2 {
        let plane = frame.plane_mut::<f32>(channel);
        for (i, slot) in plane.iter_mut().take(AUDIO_FRAME).enumerate() {
            *slot = buf[i * 2 + channel];
        }
    }
    aenc.send_frame(&frame)?;
    Ok(())
}

fn drain_video(
    enc: &mut encoder::Video,
    packet: &mut Packet,
    octx: &mut format::context::Output,
    index: usize,
    from_tb: Rational,
    to_tb: Rational,
    written: &mut u64,
) -> Result<(), ExportError> {
    while enc.receive_packet(packet).is_ok() {
        packet.set_stream(index);
        packet.rescale_ts(from_tb, to_tb);
        packet.write_interleaved(octx)?;
        *written += 1;
    }
    Ok(())
}

fn drain_audio(
    enc: &mut encoder::Audio,
    packet: &mut Packet,
    octx: &mut format::context::Output,
    index: usize,
    from_tb: Rational,
    to_tb: Rational,
) -> Result<(), ExportError> {
    while enc.receive_packet(packet).is_ok() {
        packet.set_stream(index);
        packet.rescale_ts(from_tb, to_tb);
        packet.write_interleaved(octx)?;
    }
    Ok(())
}
