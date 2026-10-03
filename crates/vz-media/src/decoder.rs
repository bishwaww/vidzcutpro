//! Reading pictures out of media files, frame-accurately.

use crate::error::{MediaError, Result};
use crate::probe::{is_image_path, stream_rotation, to_micros};
use ff::format::Pixel;
use ff::media::Type;
use ff::software::scaling::{Context as Scaler, Flags};
use ffmpeg_next as ff;
use std::path::Path;
use std::sync::Arc;
use vz_core::Micros;

/// One picture, 8 bits per channel, RGBA, rows packed without padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// When this frame is shown, in the media file's own time.
    pub pts: Micros,
}

impl VideoFrame {
    pub fn black(width: u32, height: u32) -> Self {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
        Self {
            width,
            height,
            rgba,
            pts: 0,
        }
    }
}

/// The biggest size a decoded picture may have. The picture keeps its shape and is scaled down to fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxSize {
    pub width: u32,
    pub height: u32,
    /// Also make small pictures bigger so they fill the box (used for the player and for export).
    pub enlarge: bool,
}

impl MaxSize {
    pub const FULL: Self = Self {
        width: u32::MAX,
        height: u32::MAX,
        enlarge: false,
    };

    /// A box that pictures are only ever shrunk to fit (thumbnails).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            enlarge: false,
        }
    }

    /// A box that pictures are scaled to fit, bigger or smaller (the player and export).
    pub fn fill_box(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            enlarge: true,
        }
    }

    /// Size after fitting `w` x `h` inside this box, at least 2x2 and even.
    pub fn fit(self, w: u32, h: u32) -> (u32, u32) {
        let mut scale =
            (f64::from(self.width) / f64::from(w)).min(f64::from(self.height) / f64::from(h));
        if !self.enlarge {
            scale = scale.min(1.0);
        }
        let even = |v: f64| ((v.round() as u32).max(2) + 1) & !1;
        (even(f64::from(w) * scale), even(f64::from(h) * scale))
    }
}

/// How far ahead of the current frame we keep reading instead of seeking.
const SEEK_THRESHOLD: Micros = 2_000_000;
/// Going backwards (reverse playback) decodes a whole run of pictures at once and keeps them: seeking back for every
/// single picture would be far too slow. The run stays under this many bytes, and is only made for short steps back.
const BACK_CACHE_BYTES: usize = 160 * 1024 * 1024;
const BACK_RANGE: Micros = 3_000_000;

/// A colour converter and what it was built for, so it is rebuilt only when the picture format or size changes.
struct CachedScaler {
    scaler: Scaler,
    input: (Pixel, u32, u32, ff::color::Space, ff::color::Range),
    output: (u32, u32),
}

struct Held {
    pts: Micros,
    frame: ff::frame::Video,
}

pub struct VideoDecoder {
    input: ff::format::context::Input,
    stream_index: usize,
    decoder: ff::decoder::Video,
    time_base: (i32, i32),
    /// Time of the first frame in the file; subtracted so media time starts at 0.
    origin: Micros,
    rotation: u32,
    sar: (u32, u32),
    max: MaxSize,
    scaler: Option<CachedScaler>,
    current: Option<Held>,
    lookahead: Option<Held>,
    reached_end: bool,
    cache: Option<Arc<VideoFrame>>,
    /// A run of converted pictures in time order, filled when playing backwards.
    back: Vec<(Micros, Arc<VideoFrame>)>,
}

impl VideoDecoder {
    pub fn open(path: &Path, max: MaxSize) -> Result<Self> {
        crate::init()?;
        let input = ff::format::input(&path).map_err(|e| MediaError::Open {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| MediaError::NothingToRead(path.display().to_string()))?;
        let stream_index = stream.index();
        let tb = stream.time_base();
        let rotation = stream_rotation(&stream);
        let origin = if stream.start_time() == ff::ffi::AV_NOPTS_VALUE {
            0
        } else {
            to_micros(stream.start_time(), tb.numerator(), tb.denominator())
        };
        let context = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let decoder = context.decoder().video()?;
        let sar = decoder.aspect_ratio();
        let sar = if sar.numerator() > 0 && sar.denominator() > 0 {
            (sar.numerator() as u32, sar.denominator() as u32)
        } else {
            (1, 1)
        };
        Ok(Self {
            input,
            stream_index,
            decoder,
            time_base: (tb.numerator(), tb.denominator()),
            origin,
            rotation,
            sar,
            max,
            scaler: None,
            current: None,
            lookahead: None,
            reached_end: false,
            cache: None,
            back: Vec::new(),
        })
    }

    /// Picture size after rotation and pixel-shape correction, before scaling to `max`.
    pub fn display_size(&self) -> (u32, u32) {
        let w = (f64::from(self.decoder.width()) * f64::from(self.sar.0) / f64::from(self.sar.1))
            .round() as u32;
        let h = self.decoder.height();
        if self.rotation % 180 == 90 {
            (h, w)
        } else {
            (w, h)
        }
    }

    fn to_media_time(&self, pts: i64) -> Micros {
        to_micros(pts, self.time_base.0, self.time_base.1) - self.origin
    }

    /// Decode the next picture in file order. `None` at the end of the file.
    fn decode_next(&mut self) -> Result<Option<Held>> {
        let mut frame = ff::frame::Video::empty();
        loop {
            if self.decoder.receive_frame(&mut frame).is_ok() {
                let pts = frame.timestamp().or(frame.pts()).unwrap_or(0);
                return Ok(Some(Held {
                    pts: self.to_media_time(pts),
                    frame,
                }));
            }
            if self.reached_end {
                return Ok(None);
            }
            let mut sent = false;
            for (stream, packet) in self.input.packets() {
                if stream.index() == self.stream_index {
                    // a damaged packet should not stop the whole clip
                    if let Err(e) = self.decoder.send_packet(&packet) {
                        tracing::debug!("skipping bad packet: {e}");
                        continue;
                    }
                    sent = true;
                    break;
                }
            }
            if !sent {
                self.reached_end = true;
                let _ = self.decoder.send_eof();
            }
        }
    }

    /// Jump to the keyframe at or before `t`.
    fn seek(&mut self, t: Micros) -> Result<()> {
        let target = (t + self.origin).max(0);
        self.input.seek(target, ..target + 1)?;
        self.decoder.flush();
        self.current = None;
        self.lookahead = None;
        self.reached_end = false;
        Ok(())
    }

    /// The picture at `t` from the run kept for backward playback, if `t` is inside it (not at its very end).
    fn cached_back(&self, t: Micros) -> Option<Arc<VideoFrame>> {
        let (first, last) = (self.back.first()?, self.back.last()?);
        if t < first.0 || t >= last.0 {
            return None;
        }
        let i = self.back.partition_point(|(pts, _)| *pts <= t);
        self.back.get(i.checked_sub(1)?).map(|(_, f)| f.clone())
    }

    /// Decode and keep the pictures from the keyframe before `t` up to `stop` (or until the memory limit).
    fn fill_back(&mut self, t: Micros, stop: Micros) -> Result<()> {
        self.seek(t)?;
        self.back.clear();
        let mut bytes = 0usize;
        while let Some(f) = self.decode_next()? {
            let pts = f.pts;
            self.current = Some(f);
            let converted = Arc::new(self.convert(pts)?);
            bytes += converted.rgba.len();
            self.back.push((pts, converted));
            if pts >= stop || bytes >= BACK_CACHE_BYTES {
                break;
            }
        }
        Ok(())
    }

    /// The picture showing at media time `t`: the last frame that started at or before `t`.
    /// Returns `None` only when the file has no pictures at all.
    pub fn frame_at(&mut self, t: Micros) -> Result<Option<Arc<VideoFrame>>> {
        let t = t.max(0);
        if let Some(f) = self.cached_back(t) {
            return Ok(Some(f));
        }
        // a small step backwards (reverse playback): decode the whole run once instead of seeking for every picture
        let reference = match self.back.first() {
            Some((first, _)) => Some(*first),
            None => self.current.as_ref().map(|c| c.pts),
        };
        if let Some(reference) = reference
            && t < reference
            && reference - t <= BACK_RANGE
        {
            self.fill_back(t, reference)?;
            if let Some(f) = self.cached_back(t) {
                return Ok(Some(f));
            }
            // the run ended before it could cover `t` (the file's first pictures): fall through to the normal way
        }
        let cached_ok = match (&self.current, &self.lookahead) {
            (Some(cur), next) => cur.pts <= t && next.as_ref().is_some_and(|n| n.pts > t),
            _ => false,
        };
        if !cached_ok {
            let must_seek = match &self.current {
                None => true,
                Some(cur) => t < cur.pts || t > cur.pts + SEEK_THRESHOLD,
            };
            if must_seek {
                self.seek(t)?;
            }
            // read forward until the next frame is later than `t`
            loop {
                let next = match self.lookahead.take() {
                    Some(f) => Some(f),
                    None => self.decode_next()?,
                };
                match next {
                    None => break,
                    Some(f) if f.pts <= t => self.current = Some(f),
                    Some(f) => {
                        if self.current.is_none() {
                            self.current = Some(f); // asked for a time before the first frame
                        } else {
                            self.lookahead = Some(f);
                        }
                        break;
                    }
                }
            }
        }
        let Some(cur) = &self.current else {
            return Ok(None);
        };
        let pts = cur.pts;
        if let Some(hit) = self.cache.as_ref().filter(|c| c.pts == pts) {
            return Ok(Some(hit.clone()));
        }
        let converted = Arc::new(self.convert(pts)?);
        self.cache = Some(converted.clone());
        Ok(Some(converted))
    }

    /// Turn the current decoded frame into an upright, scaled RGBA picture.
    fn convert(&mut self, pts: Micros) -> Result<VideoFrame> {
        let cur = self
            .current
            .as_ref()
            .expect("convert is only called with a frame");
        let src = &cur.frame;
        let (dw, dh) = self.display_size();
        let (out_w, out_h) = self.max.fit(dw, dh);
        // scale before rotating, so the (smaller) picture is what gets rotated
        let sideways = self.rotation % 180 == 90;
        let (sw, sh) = if sideways {
            (out_h, out_w)
        } else {
            (out_w, out_h)
        };
        let key = (
            src.format(),
            src.width(),
            src.height(),
            src.color_space(),
            src.color_range(),
        );
        let rebuild = !matches!(&self.scaler, Some(c) if c.input == key && c.output == (sw, sh));
        if rebuild {
            let mut scaler =
                Scaler::get(key.0, key.1, key.2, Pixel::RGBA, sw, sh, Flags::BILINEAR)?;
            set_yuv_matrix(&mut scaler, key.3, key.4, key.2);
            self.scaler = Some(CachedScaler {
                scaler,
                input: key,
                output: (sw, sh),
            });
        }
        let scaler = &mut self.scaler.as_mut().expect("scaler was just built").scaler;
        let mut rgba = ff::frame::Video::empty();
        scaler.run(src, &mut rgba)?;
        let packed = pack_rows(&rgba, sw, sh);
        let (width, height, data) = rotate_rgba(packed, sw, sh, self.rotation);
        Ok(VideoFrame {
            width,
            height,
            rgba: data,
            pts,
        })
    }
}

/// Tell the colour converter which YUV→RGB matrix the footage uses. FFmpeg's default is the old
/// standard-definition one (BT.601), which would shift the colours of HD and phone video (BT.709).
/// When the file does not say, HD-sized pictures are assumed to be BT.709.
fn set_yuv_matrix(
    scaler: &mut Scaler,
    space: ff::color::Space,
    range: ff::color::Range,
    height: u32,
) {
    use ff::color::Space;
    let table = match space {
        Space::BT709 => ff::ffi::SWS_CS_ITU709,
        Space::BT470BG | Space::SMPTE170M => ff::ffi::SWS_CS_ITU601,
        Space::SMPTE240M => ff::ffi::SWS_CS_SMPTE240M,
        Space::BT2020NCL | Space::BT2020CL => ff::ffi::SWS_CS_BT2020,
        _ if height >= 720 => ff::ffi::SWS_CS_ITU709,
        _ => ff::ffi::SWS_CS_ITU601,
    };
    let source_is_full_range = i32::from(range == ff::color::Range::JPEG);
    // SAFETY: the pointer is the scaler's own live SwsContext; the tables come from FFmpeg itself.
    // (Picture formats that are already RGB make this call return an error, which is fine to ignore.)
    unsafe {
        let coefficients = ff::ffi::sws_getCoefficients(table);
        ff::ffi::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            coefficients,
            source_is_full_range,
            coefficients,
            1,
            0,
            1 << 16,
            1 << 16,
        );
    }
}

/// Copy a frame's rows without the padding at the end of each row.
fn pack_rows(frame: &ff::frame::Video, w: u32, h: u32) -> Vec<u8> {
    let stride = frame.stride(0);
    let data = frame.data(0);
    let row = (w * 4) as usize;
    let mut out = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    out
}

/// Turn an RGBA picture clockwise by 0, 90, 180 or 270 degrees.
pub(crate) fn rotate_rgba(src: Vec<u8>, w: u32, h: u32, degrees: u32) -> (u32, u32, Vec<u8>) {
    let (w, h) = (w as usize, h as usize);
    let px = |x: usize, y: usize| &src[(y * w + x) * 4..(y * w + x) * 4 + 4];
    match degrees % 360 {
        90 => {
            let mut out = vec![0u8; src.len()];
            for y in 0..h {
                for x in 0..w {
                    // (x, y) moves to (h - 1 - y, x) in a picture that is h wide and w tall
                    let o = (x * h + (h - 1 - y)) * 4;
                    out[o..o + 4].copy_from_slice(px(x, y));
                }
            }
            (h as u32, w as u32, out)
        }
        180 => {
            let mut out = Vec::with_capacity(src.len());
            for p in src.as_chunks::<4>().0.iter().rev() {
                out.extend_from_slice(p);
            }
            (w as u32, h as u32, out)
        }
        270 => {
            let mut out = vec![0u8; src.len()];
            for y in 0..h {
                for x in 0..w {
                    // (x, y) moves to (y, w - 1 - x)
                    let o = ((w - 1 - x) * h + y) * 4;
                    out[o..o + 4].copy_from_slice(px(x, y));
                }
            }
            (h as u32, w as u32, out)
        }
        _ => (w as u32, h as u32, src),
    }
}

/// Where pictures come from: a video file or a still image. Both answer "what is showing at time t?".
pub enum FrameSource {
    Video(Box<VideoDecoder>),
    Image(Arc<VideoFrame>),
}

impl FrameSource {
    pub fn open(path: &Path, max: MaxSize) -> Result<Self> {
        if is_image_path(path) {
            let img = image::open(path)?.to_rgba8();
            let (w, h) = img.dimensions();
            let (ow, oh) = max.fit(w, h);
            let rgba = if (ow, oh) == (w, h) {
                img.into_raw()
            } else {
                image::imageops::resize(&img, ow, oh, image::imageops::FilterType::Triangle)
                    .into_raw()
            };
            return Ok(Self::Image(Arc::new(VideoFrame {
                width: ow,
                height: oh,
                rgba,
                pts: 0,
            })));
        }
        Ok(Self::Video(Box::new(VideoDecoder::open(path, max)?)))
    }

    pub fn frame_at(&mut self, t: Micros) -> Result<Option<Arc<VideoFrame>>> {
        match self {
            Self::Video(d) => d.frame_at(t),
            Self::Image(f) => Ok(Some(f.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_never_enlarges_and_keeps_the_shape() {
        let max = MaxSize::new(960, 540);
        assert_eq!(max.fit(1920, 1080), (960, 540));
        assert_eq!(max.fit(640, 360), (640, 360));
        assert_eq!(max.fit(1080, 1920), (304, 540));
        assert_eq!(MaxSize::FULL.fit(1920, 1080), (1920, 1080));
        // the player/export box also enlarges
        assert_eq!(MaxSize::fill_box(1920, 1080).fit(640, 360), (1920, 1080));
        assert_eq!(MaxSize::fill_box(1080, 1920).fit(1920, 1080), (1080, 608));
    }

    #[test]
    fn rotation_moves_pixels_to_the_right_place() {
        // 2 wide, 1 tall: A B
        let src = vec![1, 1, 1, 1, 2, 2, 2, 2];
        let (w, h, out) = rotate_rgba(src.clone(), 2, 1, 90);
        assert_eq!((w, h), (1, 2));
        assert_eq!(out, vec![1, 1, 1, 1, 2, 2, 2, 2]); // clockwise: A on top, B below
        let (w, h, out) = rotate_rgba(src.clone(), 2, 1, 270);
        assert_eq!((w, h), (1, 2));
        assert_eq!(out, vec![2, 2, 2, 2, 1, 1, 1, 1]);
        let (_, _, out) = rotate_rgba(src.clone(), 2, 1, 180);
        assert_eq!(out, vec![2, 2, 2, 2, 1, 1, 1, 1]);
        assert_eq!(rotate_rgba(src.clone(), 2, 1, 0).2, src);
    }

    #[test]
    fn black_frame_is_opaque() {
        let f = VideoFrame::black(4, 2);
        assert_eq!(f.rgba.len(), 32);
        assert!(
            f.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
    }
}
