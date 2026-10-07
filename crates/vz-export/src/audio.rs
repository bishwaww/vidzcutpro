//! Exporting only the sound of the timeline: WAV, FLAC, AAC (M4A) and, when this FFmpeg has the LAME encoder (the LGPL build here does), MP3.

use crate::encode::{ExportError, ExportReport, Progress};
use ff::format::sample::{Sample, Type as SampleType};
use ff::{ChannelLayout, Packet, Rational, codec, encoder, format, frame};
use ffmpeg_next as ff;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vz_audio::{Mixer, micros_to_samples};
use vz_core::{Micros, Project};

const RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Flac,
    Aac,
    Mp3,
}

impl AudioFormat {
    pub const ALL: [Self; 4] = [Self::Wav, Self::Flac, Self::Aac, Self::Mp3];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wav => "WAV (lossless, big)",
            Self::Flac => "FLAC (lossless, smaller)",
            Self::Aac => "AAC (M4A)",
            Self::Mp3 => "MP3",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Aac => "m4a",
            Self::Mp3 => "mp3",
        }
    }

    fn container(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Aac => "ipod",
            Self::Mp3 => "mp3",
        }
    }

    fn encoder_name(self) -> &'static str {
        match self {
            Self::Wav => "pcm_s16le",
            Self::Flac => "flac",
            Self::Aac => "aac",
            Self::Mp3 => "libmp3lame",
        }
    }

    /// Can this computer write the format? (The LGPL FFmpeg builds often have no MP3 encoder.)
    pub fn is_available(self) -> bool {
        let _ = vz_media::init();
        encoder::find_by_name(self.encoder_name()).is_some()
    }

    /// Does the quality setting (bit rate) matter for this format?
    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Aac | Self::Mp3)
    }
}

#[derive(Clone, Debug)]
pub struct AudioExportSettings {
    pub path: PathBuf,
    pub format: AudioFormat,
    /// Bits per second for AAC and MP3.
    pub bit_rate: usize,
    /// Export only this part of the timeline. `None` exports everything.
    pub range: Option<(Micros, Micros)>,
}

impl AudioExportSettings {
    pub fn new(path: PathBuf, format: AudioFormat) -> Self {
        Self {
            path,
            format,
            bit_rate: 192_000,
            range: None,
        }
    }
}

/// Write the sound of `project` to `settings.path`.
pub fn export_audio(
    project: &Project,
    settings: &AudioExportSettings,
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
    let codec = encoder::find_by_name(settings.format.encoder_name()).ok_or_else(|| {
        ExportError::Message(format!(
            "this computer cannot write {} files",
            settings.format.extension().to_uppercase()
        ))
    })?;
    let began = Instant::now();
    let partial = {
        let mut p = settings.path.as_os_str().to_owned();
        p.push(".partial");
        PathBuf::from(p)
    };
    let result = write(
        project, settings, codec, &partial, start, end, progress, cancel,
    );
    match result {
        Ok(frames) => {
            std::fs::rename(&partial, &settings.path)?;
            Ok(ExportReport {
                encoder: settings.format.encoder_name().to_string(),
                frames,
                seconds: began.elapsed().as_secs_f64(),
                bytes: std::fs::metadata(&settings.path).map_or(0, |m| m.len()),
                path: settings.path.clone(),
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write(
    project: &Project,
    settings: &AudioExportSettings,
    acodec: ff::Codec,
    partial: &Path,
    start: Micros,
    end: Micros,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<u64, ExportError> {
    let mut octx = format::output_as(&partial, settings.format.container())?;
    let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let sample_format = pick_format(acodec);

    let mut aenc = codec::context::Context::new_with_codec(acodec)
        .encoder()
        .audio()?;
    aenc.set_rate(RATE as i32);
    aenc.set_channel_layout(ChannelLayout::STEREO);
    aenc.set_format(sample_format);
    if settings.format.is_lossy() {
        aenc.set_bit_rate(settings.bit_rate);
    }
    aenc.set_time_base(Rational(1, RATE as i32));
    if global_header {
        aenc.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let mut aenc = aenc.open_as(acodec)?;
    let mut astream = octx.add_stream(acodec)?;
    astream.set_parameters(&aenc);
    astream.set_time_base(Rational(1, RATE as i32));
    let index = astream.index();
    octx.write_header()?;
    let stream_tb = octx.stream(index).expect("stream exists").time_base();
    let enc_tb = Rational(1, RATE as i32);

    let chunk = match aenc.frame_size() {
        0 => 1024,
        n => n as usize,
    };
    let first = micros_to_samples(start, RATE);
    let total = micros_to_samples(end, RATE) - first;
    let mut mixer = Mixer::new(RATE);
    let mut buf = Vec::new();
    let mut packet = Packet::empty();
    let mut done = 0i64;
    let mut frames = 0u64;
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        let n = ((total - done) as usize).min(chunk);
        mixer.render(project, first + done, n, &mut buf);
        let mut f = frame::Audio::new(sample_format, n, ChannelLayout::STEREO);
        f.set_rate(RATE);
        f.set_pts(Some(done));
        fill(&mut f, sample_format, &buf, n);
        aenc.send_frame(&f)?;
        done += n as i64;
        frames += 1;
        drain(&mut aenc, &mut packet, &mut octx, index, enc_tb, stream_tb)?;
        if frames.is_multiple_of(16) || done >= total {
            progress(Progress {
                frames_done: done as u64,
                frames_total: total as u64,
            });
        }
    }
    aenc.send_eof()?;
    drain(&mut aenc, &mut packet, &mut octx, index, enc_tb, stream_tb)?;
    octx.write_trailer()?;
    Ok(frames)
}

/// The first sample layout the encoder can take, preferring ones `fill` handles.
fn pick_format(codec: ff::Codec) -> Sample {
    let formats: Vec<Sample> = codec
        .audio()
        .ok()
        .and_then(|a| a.formats())
        .map(|f| f.collect())
        .unwrap_or_default();
    let wanted = [
        Sample::F32(SampleType::Planar),
        Sample::I16(SampleType::Packed),
        Sample::I16(SampleType::Planar),
        Sample::I32(SampleType::Packed),
        Sample::F32(SampleType::Packed),
    ];
    wanted
        .into_iter()
        .find(|s| formats.contains(s))
        .or_else(|| formats.first().copied())
        .unwrap_or(Sample::I16(SampleType::Packed))
}

/// Copy `n` stereo frames from `buf` (interleaved f32) into the frame in the encoder's format.
fn fill(f: &mut frame::Audio, format: Sample, buf: &[f32], n: usize) {
    let to_i16 = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    let to_i32 = |v: f32| (f64::from(v.clamp(-1.0, 1.0)) * 2_147_483_647.0).round() as i32;
    match format {
        Sample::F32(SampleType::Planar) => {
            for ch in 0..2 {
                for (i, slot) in f.plane_mut::<f32>(ch).iter_mut().take(n).enumerate() {
                    *slot = buf[i * 2 + ch];
                }
            }
        }
        Sample::F32(SampleType::Packed) => {
            for (slot, v) in f.plane_mut::<f32>(0).iter_mut().zip(&buf[..n * 2]) {
                *slot = *v;
            }
        }
        Sample::I16(SampleType::Planar) => {
            for ch in 0..2 {
                for (i, slot) in f.plane_mut::<i16>(ch).iter_mut().take(n).enumerate() {
                    *slot = to_i16(buf[i * 2 + ch]);
                }
            }
        }
        Sample::I16(SampleType::Packed) => {
            for (slot, v) in f.plane_mut::<i16>(0).iter_mut().zip(&buf[..n * 2]) {
                *slot = to_i16(*v);
            }
        }
        Sample::I32(SampleType::Packed) => {
            for (slot, v) in f.plane_mut::<i32>(0).iter_mut().zip(&buf[..n * 2]) {
                *slot = to_i32(*v);
            }
        }
        Sample::I32(SampleType::Planar) => {
            for ch in 0..2 {
                for (i, slot) in f.plane_mut::<i32>(ch).iter_mut().take(n).enumerate() {
                    *slot = to_i32(buf[i * 2 + ch]);
                }
            }
        }
        _ => {}
    }
}

fn drain(
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
