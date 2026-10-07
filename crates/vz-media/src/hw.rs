//! Decoding with the graphics card when it can (Direct3D 11 on Windows, VideoToolbox on a Mac).
//! Anything that goes wrong falls back to the normal software decoder, so a picture always appears.

use ff::ffi;
use ffmpeg_next as ff;
use std::sync::atomic::{AtomicU8, Ordering};

/// When the graphics card is asked to decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Only for pictures bigger than full HD, where it is clearly faster (the default).
    Automatic,
    Always,
    Never,
}

static MODE: AtomicU8 = AtomicU8::new(0);

/// Pictures with more pixels than this are decoded by the graphics card in automatic mode (full HD is 2.07 million).
pub const AUTOMATIC_ABOVE_PIXELS: u32 = 2_600_000;

/// Choose when the graphics card decodes, for files opened from now on.
pub fn set_mode(mode: Mode) {
    MODE.store(
        match mode {
            Mode::Automatic => 0,
            Mode::Always => 1,
            Mode::Never => 2,
        },
        Ordering::Relaxed,
    );
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Always,
        2 => Mode::Never,
        _ => Mode::Automatic,
    }
}

/// Should a picture of this size be decoded by the graphics card?
pub fn wanted_for(mode: Mode, width: u32, height: u32) -> bool {
    match mode {
        Mode::Always => true,
        Mode::Never => false,
        Mode::Automatic => width.saturating_mul(height) > AUTOMATIC_ABOVE_PIXELS,
    }
}

#[cfg(target_os = "windows")]
const DEVICE: Option<(ffi::AVHWDeviceType, ffi::AVPixelFormat)> = Some((
    ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
    ffi::AVPixelFormat::AV_PIX_FMT_D3D11,
));
#[cfg(target_os = "macos")]
const DEVICE: Option<(ffi::AVHWDeviceType, ffi::AVPixelFormat)> = Some((
    ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
    ffi::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX,
));
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const DEVICE: Option<(ffi::AVHWDeviceType, ffi::AVPixelFormat)> = None;

/// A reference to the graphics card's decoding device, or `None` when there is none (or it is switched off).
pub(crate) fn create_device() -> Option<*mut ffi::AVBufferRef> {
    let (kind, _) = DEVICE?;
    let mut device: *mut ffi::AVBufferRef = std::ptr::null_mut();
    // SAFETY: `device` is a valid out-pointer; FFmpeg fills it or leaves it null.
    let result = unsafe {
        ffi::av_hwdevice_ctx_create(&mut device, kind, std::ptr::null(), std::ptr::null_mut(), 0)
    };
    (result >= 0 && !device.is_null()).then_some(device)
}

/// Give the decoder (not opened yet) the device and tell it which picture format to ask for.
///
/// # Safety
/// `context` must point to a live, not yet opened codec context, and `device` to a buffer reference from
/// [`create_device`]. The reference is consumed (released here once the context has its own).
pub(crate) unsafe fn attach(context: *mut ffi::AVCodecContext, mut device: *mut ffi::AVBufferRef) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        (*context).hw_device_ctx = ffi::av_buffer_ref(device);
        (*context).get_format = Some(pick_format);
        ffi::av_buffer_unref(&mut device);
    }
}

fn is_hardware_format(format: ffi::AVPixelFormat) -> bool {
    // SAFETY: the descriptor table is static; null means an unknown format.
    unsafe {
        let desc = ffi::av_pix_fmt_desc_get(format);
        !desc.is_null() && ((*desc).flags & ffi::AV_PIX_FMT_FLAG_HWACCEL as u64) != 0
    }
}

/// FFmpeg asks which of the formats the codec can produce we want: the graphics card's if offered, else the first
/// ordinary one (which makes the codec decode on the processor).
unsafe extern "C" fn pick_format(
    _context: *mut ffi::AVCodecContext,
    formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let wanted = DEVICE.map(|(_, f)| f);
    let mut software = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    let mut at = formats;
    // SAFETY: FFmpeg gives a list ended by AV_PIX_FMT_NONE.
    unsafe {
        while *at != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            if Some(*at) == wanted {
                return *at;
            }
            if software == ffi::AVPixelFormat::AV_PIX_FMT_NONE && !is_hardware_format(*at) {
                software = *at;
            }
            at = at.add(1);
        }
    }
    software
}

/// Is this decoded picture still on the graphics card?
pub(crate) fn is_on_card(frame: &ff::frame::Video) -> bool {
    // SAFETY: reading the format field of a live frame.
    let format: i32 = unsafe { (*frame.as_ptr()).format };
    // SAFETY: AVPixelFormat is a plain C enum and `format` came from FFmpeg itself.
    is_hardware_format(unsafe { std::mem::transmute::<i32, ffi::AVPixelFormat>(format) })
}

/// Copy a picture from the graphics card into ordinary memory (keeping its time stamps).
pub(crate) fn download(on_card: &ff::frame::Video) -> Result<ff::frame::Video, ff::Error> {
    let mut ordinary = ff::frame::Video::empty();
    // SAFETY: both frames are live; FFmpeg allocates the destination's buffers.
    unsafe {
        let result = ffi::av_hwframe_transfer_data(ordinary.as_mut_ptr(), on_card.as_ptr(), 0);
        if result < 0 {
            return Err(ff::Error::from(result));
        }
        ffi::av_frame_copy_props(ordinary.as_mut_ptr(), on_card.as_ptr());
    }
    Ok(ordinary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_mode_leaves_full_hd_to_the_processor() {
        assert!(!wanted_for(Mode::Automatic, 1920, 1080));
        assert!(!wanted_for(Mode::Automatic, 1080, 1920));
        assert!(wanted_for(Mode::Automatic, 3840, 2160));
        assert!(wanted_for(Mode::Automatic, 2560, 1440));
        assert!(wanted_for(Mode::Always, 640, 360));
        assert!(!wanted_for(Mode::Never, 7680, 4320));
    }
}
