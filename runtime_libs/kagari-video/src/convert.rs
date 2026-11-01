//! Decoded frames to packed sRGB RGBA.

use std::ffi::c_int;

use ffmpeg_next::ffi::{
    SWS_CS_BT2020, SWS_CS_DEFAULT, SWS_CS_ITU601, SWS_CS_ITU709, sws_getCoefficients,
    sws_setColorspaceDetails,
};
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;
use ffmpeg_next::software::scaling::{Context, Flags};
use ffmpeg_next::util::color;

use crate::VideoError;

/// What a scaler was built for.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Source {
    format: Pixel,
    width: u32,
    height: u32,
    space: color::Space,
    range: color::Range,
}

/// A scaler, rebuilt when the source format, size or colour description changes.
pub(crate) struct Converter {
    scaler: Option<(Source, Context)>,
}

// SAFETY: the `SwsContext` is owned by one decoder, which one thread uses at a time; swscale
// contexts are not bound to the thread that created them.
unsafe impl Send for Converter {}

impl Converter {
    pub(crate) fn new() -> Converter {
        Converter { scaler: None }
    }

    /// `frame` as RGBA rows without padding (`width * 4` bytes each).
    pub(crate) fn rgba(&mut self, frame: &frame::Video) -> Result<Vec<u8>, VideoError> {
        let source = Source {
            format: frame.format(),
            width: frame.width(),
            height: frame.height(),
            space: frame.color_space(),
            range: frame.color_range(),
        };
        let scaler = match &mut self.scaler {
            Some((built, scaler)) if *built == source => scaler,
            slot => &mut slot.insert((source, scaler(source)?)).1,
        };
        let mut out = frame::Video::new(Pixel::RGBA, source.width, source.height);
        scaler.run(frame, &mut out)?;
        pack(&out, source.width, source.height)
    }
}

fn scaler(source: Source) -> Result<Context, VideoError> {
    let mut scaler = Context::get(
        source.format,
        source.width,
        source.height,
        Pixel::RGBA,
        source.width,
        source.height,
        Flags::BILINEAR | Flags::ACCURATE_RND,
    )?;
    let space = match source.space {
        color::Space::BT709 => SWS_CS_ITU709,
        color::Space::BT2020NCL | color::Space::BT2020CL => SWS_CS_BT2020,
        color::Space::BT470BG | color::Space::SMPTE170M => SWS_CS_ITU601,
        _ if source.height >= 720 => SWS_CS_ITU709,
        _ => SWS_CS_ITU601,
    };
    let full_range = c_int::from(source.range == color::Range::JPEG);
    // SAFETY: `scaler` is a valid context; the coefficient tables are static.
    unsafe {
        sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            sws_getCoefficients(space),
            full_range,
            sws_getCoefficients(SWS_CS_DEFAULT),
            1,
            0,
            1 << 16,
            1 << 16,
        );
    }
    Ok(scaler)
}

/// Copies the RGBA plane of `out`, dropping the per-row padding.
fn pack(out: &frame::Video, width: u32, height: u32) -> Result<Vec<u8>, VideoError> {
    let row = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(4))
        .ok_or_else(|| VideoError::new("frame too wide"))?;
    let rows = usize::try_from(height).map_err(|_| VideoError::new("frame too tall"))?;
    let stride = out.stride(0);
    let data = out.data(0);
    let mut rgba = Vec::with_capacity(row.checked_mul(rows).unwrap_or(0));
    for y in 0..rows {
        let line = y
            .checked_mul(stride)
            .and_then(|start| data.get(start..start.checked_add(row)?))
            .ok_or_else(|| VideoError::new("short RGBA plane"))?;
        rgba.extend_from_slice(line);
    }
    Ok(rgba)
}
