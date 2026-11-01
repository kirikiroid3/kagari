//! Hardware decoding: the platform's device type, the codec's matching pixel format, the device
//! and the frame download.

use std::ffi::{c_int, c_void};
use std::ptr;

use ffmpeg_next::ffi::{
    AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX, AV_PIX_FMT_FLAG_HWACCEL, AVBufferRef, AVCodecContext,
    AVHWDeviceType, AVPixelFormat, av_buffer_ref, av_buffer_unref, av_frame_copy_props,
    av_hwdevice_ctx_create, av_hwframe_transfer_data, av_pix_fmt_desc_get, avcodec_get_hw_config,
};
use ffmpeg_next::frame;
use ffmpeg_next::{Codec, codec};

use crate::VideoError;

/// The hardware device API used on this platform; `None` decodes in software only.
fn device_type() -> Option<(AVHWDeviceType, &'static str)> {
    if cfg!(target_os = "windows") {
        Some((AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA, "d3d11va"))
    } else if cfg!(target_os = "linux") {
        Some((AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI, "vaapi"))
    } else if cfg!(target_os = "macos") {
        Some((
            AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            "videotoolbox",
        ))
    } else {
        None
    }
}

/// An owned reference to an `AVHWDeviceContext`.
struct HwDevice(*mut AVBufferRef);

// SAFETY: the reference is owned by one decoder, which one thread uses at a time; FFmpeg device
// contexts are reference counted atomically and not bound to the thread that created them.
unsafe impl Send for HwDevice {}

impl Drop for HwDevice {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a reference created by `av_hwdevice_ctx_create` and released once.
        unsafe { av_buffer_unref(&mut self.0) };
    }
}

/// The hardware side of an opened decoder.
pub(crate) struct Hw {
    _device: HwDevice,
    format: AVPixelFormat,
    name: &'static str,
}

impl Hw {
    /// The backend name reported by `VideoDecoder::backend`.
    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    /// Whether `frame` lives in GPU memory and needs [`download`].
    pub(crate) fn owns(&self, frame: &frame::Video) -> bool {
        // SAFETY: reading a field of a valid frame.
        unsafe { (*frame.as_ptr()).format == self.format as c_int }
    }
}

/// Prepares `ctx` (not yet opened) to decode `codec` on the platform's hardware device.
/// The error is the reason logged before falling back to software.
pub(crate) fn attach(codec: Codec, ctx: &mut codec::Context) -> Result<Hw, String> {
    let (kind, name) = device_type().ok_or_else(|| "no hardware API on this OS".to_owned())?;
    let format = hw_format(codec, kind)
        .ok_or_else(|| format!("the {} decoder has no {name} support", codec.name()))?;
    let mut raw = ptr::null_mut();
    // SAFETY: `raw` receives a new reference on success; no device name, options or flags.
    let ret = unsafe { av_hwdevice_ctx_create(&mut raw, kind, ptr::null(), ptr::null_mut(), 0) };
    if ret < 0 || raw.is_null() {
        return Err(format!(
            "cannot create the {name} device: {}",
            ffmpeg_next::Error::from(ret)
        ));
    }
    let device = HwDevice(raw);
    // SAFETY: `ctx` is a valid, unopened codec context. It takes its own reference to the
    // device; `opaque` carries the wanted pixel format to `hw_get_format`.
    unsafe {
        let c = ctx.as_mut_ptr();
        (*c).hw_device_ctx = av_buffer_ref(device.0);
        if (*c).hw_device_ctx.is_null() {
            return Err("cannot reference the hardware device".to_owned());
        }
        (*c).opaque = format as i32 as isize as *mut c_void;
        (*c).get_format = Some(hw_get_format);
    }
    Ok(Hw {
        _device: device,
        format,
        name,
    })
}

/// The pixel format `codec` decodes to on a `kind` device through `hw_device_ctx`.
fn hw_format(codec: Codec, kind: AVHWDeviceType) -> Option<AVPixelFormat> {
    let mut i: c_int = 0;
    loop {
        // SAFETY: `codec` is a valid codec; FFmpeg returns null past the last config.
        let config = unsafe { avcodec_get_hw_config(codec.as_ptr(), i) };
        if config.is_null() {
            return None;
        }
        // SAFETY: non-null configs point to static descriptors.
        let config = unsafe { &*config };
        if config.methods & (AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as c_int) != 0
            && config.device_type == kind
        {
            return Some(config.pix_fmt);
        }
        i = i.checked_add(1)?;
    }
}

/// `get_format` callback: the hardware format stored in `opaque` when offered, otherwise the
/// first software format (FFmpeg lists hardware formats first and calls again without the
/// format whose setup failed, e.g. a picture too small for the hardware).
unsafe extern "C" fn hw_get_format(
    ctx: *mut AVCodecContext,
    fmts: *const AVPixelFormat,
) -> AVPixelFormat {
    // SAFETY: FFmpeg passes its context and a list terminated by `AV_PIX_FMT_NONE`; pixel
    // format descriptors are static.
    unsafe {
        let wanted = (*ctx).opaque as isize as i32;
        let mut software = None;
        let mut p = fmts;
        while *p != AVPixelFormat::AV_PIX_FMT_NONE {
            if *p as i32 == wanted {
                return *p;
            }
            let desc = av_pix_fmt_desc_get(*p);
            let hwaccel = desc.is_null() || (*desc).flags & (AV_PIX_FMT_FLAG_HWACCEL as u64) != 0;
            if software.is_none() && !hwaccel {
                software = Some(*p);
            }
            p = p.add(1);
        }
        software.unwrap_or(AVPixelFormat::AV_PIX_FMT_NONE)
    }
}

/// Copies a GPU frame into a new frame in system memory.
pub(crate) fn download(hw: &frame::Video) -> Result<frame::Video, VideoError> {
    let mut sw = frame::Video::empty();
    // SAFETY: both frames are valid; `sw` is empty, so FFmpeg allocates it.
    let ret = unsafe { av_hwframe_transfer_data(sw.as_mut_ptr(), hw.as_ptr(), 0) };
    if ret < 0 {
        return Err(VideoError::from(ffmpeg_next::Error::from(ret)));
    }
    // SAFETY: as above; copies colour properties and timestamps.
    let ret = unsafe { av_frame_copy_props(sw.as_mut_ptr(), hw.as_ptr()) };
    if ret < 0 {
        return Err(VideoError::from(ffmpeg_next::Error::from(ret)));
    }
    Ok(sw)
}
