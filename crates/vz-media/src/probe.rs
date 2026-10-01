//! Looking inside a media file: how long it is, how big, whether it has sound.

use crate::error::{MediaError, Result};
use ff::media::Type;
use ffmpeg_next as ff;
use std::path::Path;
use vz_core::{FrameRate, MediaAsset, MediaId, MediaKind, Micros};

/// File extensions that are still pictures.
const IMAGE_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "heic"];

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Turn an FFmpeg rational (a/b) into microseconds per unit helper: `value * num/den` seconds → µs.
pub(crate) fn to_micros(value: i64, num: i32, den: i32) -> Micros {
    if den == 0 {
        return 0;
    }
    ((i128::from(value) * 1_000_000 * i128::from(num)) / i128::from(den)) as Micros
}

/// How a video must be turned to be upright (phones record sideways and add a "rotate" note).
pub(crate) fn stream_rotation(stream: &ff::format::stream::Stream<'_>) -> u32 {
    for side in stream.side_data() {
        if side.kind() == ff::packet::side_data::Type::DisplayMatrix {
            let d = side.data();
            if d.len() >= 36 {
                let m = |i: usize| {
                    i32::from_le_bytes([d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]]) as f64
                        / 65536.0
                };
                let (a, b) = (m(0), m(1));
                let angle = -(b.atan2(a)).to_degrees();
                let quarter = (angle / 90.0).round() as i32;
                return quarter.rem_euclid(4) as u32 * 90;
            }
        }
    }
    0
}

/// Read a media file's properties and describe it as a project asset.
pub fn probe(path: &Path) -> Result<MediaAsset> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    if is_image_path(path) {
        let (width, height) = image::image_dimensions(path)?;
        return Ok(MediaAsset {
            id: MediaId::new(),
            name,
            path: path.to_path_buf(),
            relative_path: None,
            kind: MediaKind::Image,
            duration: 0,
            width,
            height,
            frame_rate: None,
            has_audio: false,
            file_size,
        });
    }

    crate::init()?;
    let input = ff::format::input(&path).map_err(|e| MediaError::Open {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let video = input.streams().best(Type::Video);
    let has_audio = input.streams().best(Type::Audio).is_some();
    if video.is_none() && !has_audio {
        return Err(MediaError::NothingToRead(name));
    }

    // container length is in AV_TIME_BASE units, which are microseconds
    let mut duration = input.duration().max(0);
    let (mut width, mut height, mut frame_rate) = (0, 0, None);
    if let Some(stream) = &video {
        let params = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let dec = params.decoder().video()?;
        let rotation = stream_rotation(stream);
        let sar = dec.aspect_ratio();
        let mut w = dec.width();
        if sar.numerator() > 0 && sar.denominator() > 0 {
            w = (f64::from(w) * f64::from(sar.numerator()) / f64::from(sar.denominator())).round()
                as u32;
        }
        (width, height) = if rotation == 90 || rotation == 270 {
            (dec.height(), w)
        } else {
            (w, dec.height())
        };
        let avg = stream.avg_frame_rate();
        if avg.numerator() > 0 && avg.denominator() > 0 {
            frame_rate = FrameRate::new(avg.numerator() as u32, avg.denominator() as u32);
        }
        if duration == 0 && stream.duration() > 0 {
            let tb = stream.time_base();
            duration = to_micros(stream.duration(), tb.numerator(), tb.denominator());
        }
    }
    let kind = if video.is_some() {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };
    Ok(MediaAsset {
        id: MediaId::new(),
        name,
        path: path.to_path_buf(),
        relative_path: None,
        kind,
        duration,
        width,
        height,
        frame_rate,
        has_audio,
        file_size,
    })
}
