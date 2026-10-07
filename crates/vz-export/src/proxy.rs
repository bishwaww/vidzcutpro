//! Proxies: small, quick-to-decode copies of big videos that the preview uses while you edit.
//! The export always goes back to the original file.

use crate::VideoCodec;
use crate::encode::{
    ExportError, Progress, drain_video, encoder_candidates, fill_rgba, pick_pixel_format, set_bt709,
};
use ff::format::Pixel;
use ff::software::scaling::{Context as Scaler, Flags};
use ff::{Dictionary, Packet, Rational, codec, encoder, format, frame};
use ffmpeg_next as ff;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use vz_core::{FrameRate, Micros};
use vz_media::{FrameSource, MaxSize};

/// The longest side of a proxy picture. A preview is at most 960 pixels wide, so nothing visible is lost.
pub const PROXY_LONG_SIDE: u32 = 960;
/// Videos bigger than this (in either direction) get a proxy automatically.
pub const AUTO_PROXY_ABOVE: u32 = 1920;

/// Where the proxy of a file lives, and what makes it out of date: a different path, size or length means a new file.
pub fn proxy_path(folder: &Path, source: &Path, file_size: u64, duration: Micros) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (source, file_size, duration).hash(&mut h);
    let name = source
        .file_stem()
        .map_or_else(|| "video".to_string(), |s| s.to_string_lossy().into_owned());
    let clean: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect();
    folder.join(format!("{clean}-{:016x}.mp4", h.finish()))
}

/// Make a proxy of `source` at `dest` (written to a temporary name first, so a half-made file is never used).
pub fn make_proxy(
    source: &Path,
    dest: &Path,
    duration: Micros,
    frame_rate: FrameRate,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<(), ExportError> {
    vz_media::init()?;
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = {
        let mut p = dest.as_os_str().to_owned();
        p.push(".partial");
        PathBuf::from(p)
    };
    let mut problems = Vec::new();
    for name in encoder_candidates(VideoCodec::H264, true) {
        match try_encoder(
            name, source, &partial, duration, frame_rate, progress, cancel,
        ) {
            Ok(()) => {
                std::fs::rename(&partial, dest)?;
                return Ok(());
            }
            Err(ExportError::Cancelled) => {
                let _ = std::fs::remove_file(&partial);
                return Err(ExportError::Cancelled);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                tracing::warn!("proxy encoder {name} failed: {e}");
                problems.push(format!("{name}: {e}"));
            }
        }
    }
    Err(ExportError::NoEncoder(problems.join("; ")))
}

fn try_encoder(
    encoder_name: &str,
    source: &Path,
    partial: &Path,
    duration: Micros,
    frame_rate: FrameRate,
    progress: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<(), ExportError> {
    let mut reader = FrameSource::open(source, MaxSize::new(PROXY_LONG_SIDE, PROXY_LONG_SIDE))
        .map_err(|e| ExportError::Message(e.to_string()))?;
    let Some(first) = reader
        .frame_at(0)
        .map_err(|e| ExportError::Message(e.to_string()))?
    else {
        return Err(ExportError::Message("the video has no pictures".into()));
    };
    let (w, h) = (first.width, first.height);
    let vcodec = encoder::find_by_name(encoder_name)
        .ok_or_else(|| ExportError::Message(format!("encoder {encoder_name} is not available")))?;

    let mut octx = format::output_as(&partial, "mp4")?;
    let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let pix = pick_pixel_format(vcodec);
    let tb = Rational(frame_rate.den as i32, frame_rate.num as i32);
    let mut venc = codec::context::Context::new_with_codec(vcodec)
        .encoder()
        .video()?;
    venc.set_width(w);
    venc.set_height(h);
    venc.set_format(pix);
    venc.set_time_base(tb);
    venc.set_frame_rate(Some(Rational(frame_rate.num as i32, frame_rate.den as i32)));
    // about 6 Mbit/s: generous for 960 pixels, so the proxy looks like the original
    venc.set_bit_rate(6_000_000);
    // a picture to start from every half second, so jumping around the timeline is quick
    venc.set_gop((frame_rate.nominal() / 2).max(1) as u32);
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
            options.set("preset", "p1");
            options.set("tune", "ll");
        }
        "h264_qsv" => options.set("preset", "veryfast"),
        "h264_amf" => options.set("quality", "speed"),
        _ => {}
    }
    let mut venc = venc.open_as_with(vcodec, options)?;
    let mut vstream = octx.add_stream(vcodec)?;
    vstream.set_parameters(&venc);
    vstream.set_time_base(tb);
    let index = vstream.index();
    octx.write_header()?;
    let stream_tb = octx.stream(index).expect("stream exists").time_base();

    let mut scaler = Scaler::get(Pixel::RGBA, w, h, pix, w, h, Flags::BILINEAR)?;
    set_bt709(&mut scaler);
    let mut rgba = frame::Video::new(Pixel::RGBA, w, h);
    let mut yuv = frame::Video::new(pix, w, h);
    let mut packet = Packet::empty();
    let mut written = 0u64;

    let total = ((duration as f64 / 1e6 * frame_rate.as_f64()).ceil() as i64).max(1);
    for i in 0..total {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        let t = frame_rate.frame_start(i);
        let picture = match reader.frame_at(t) {
            Ok(Some(p)) => p,
            Ok(None) => first.clone(),
            Err(e) => return Err(ExportError::Message(e.to_string())),
        };
        // a picture of another size cannot happen with one reader, but never write a wrong-sized one
        if (picture.width, picture.height) != (w, h) {
            continue;
        }
        fill_rgba(&mut rgba, &picture.rgba, w, h);
        scaler.run(&rgba, &mut yuv)?;
        yuv.set_pts(Some(i));
        venc.send_frame(&yuv)?;
        drain_video(
            &mut venc,
            &mut packet,
            &mut octx,
            index,
            tb,
            stream_tb,
            &mut written,
        )?;
        if i % 8 == 0 || i + 1 == total {
            progress(Progress {
                frames_done: (i + 1) as u64,
                frames_total: total as u64,
            });
        }
    }
    venc.send_eof()?;
    drain_video(
        &mut venc,
        &mut packet,
        &mut octx,
        index,
        tb,
        stream_tb,
        &mut written,
    )?;
    octx.write_trailer()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxy_has_its_own_name_that_changes_with_the_file() {
        let folder = Path::new("proxies");
        let a = proxy_path(
            folder,
            Path::new("D:/Videos/My holiday.mp4"),
            1000,
            5_000_000,
        );
        let b = proxy_path(
            folder,
            Path::new("D:/Videos/My holiday.mp4"),
            1000,
            5_000_000,
        );
        let c = proxy_path(
            folder,
            Path::new("D:/Videos/My holiday.mp4"),
            2000,
            5_000_000,
        );
        let d = proxy_path(
            folder,
            Path::new("E:/Other/My holiday.mp4"),
            1000,
            5_000_000,
        );
        assert_eq!(a, b);
        assert_ne!(a, c, "a changed file needs a new proxy");
        assert_ne!(a, d, "same name in another folder is another file");
        assert!(a.to_string_lossy().ends_with(".mp4"));
        assert!(a.starts_with(folder));
    }
}
