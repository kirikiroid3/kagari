//! Movie decoding for Kagari runtimes, on top of FFmpeg (`ffmpeg-next`).
//!
//! The crate depends on no other Kagari crate. Inputs are whole files in memory (`Arc<[u8]>`):
//! [`probe`] reads a movie's facts, [`VideoDecoder`] gives RGBA frames at a playback position
//! and [`AudioDecoder`] gives the soundtrack as interleaved stereo `f32` frames.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

mod audio;
mod convert;
mod hw;
mod io;
mod video;

#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::{Arc, Once};

use ffmpeg_next as ffmpeg;
use ffmpeg_next::media::Type;

pub use audio::AudioDecoder;
pub use video::{RgbaFrame, VideoDecoder};

/// Initialises FFmpeg once per process and limits its own logging to errors.
pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Err(e) = ffmpeg::init() {
            log::warn!(target: "kagari::video", "video: cannot initialise FFmpeg: {e}");
        }
        ffmpeg::log::set_level(ffmpeg::log::Level::Error);
    });
}

/// Why a movie cannot be probed or decoded.
#[derive(Debug, Clone)]
pub struct VideoError(String);

impl VideoError {
    pub(crate) fn new(message: impl Into<String>) -> VideoError {
        VideoError(message.into())
    }
}

impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for VideoError {}

impl From<ffmpeg::Error> for VideoError {
    fn from(e: ffmpeg::Error) -> VideoError {
        VideoError(e.to_string())
    }
}

/// The facts the engine needs about a movie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    /// The length in milliseconds; 0 when unknown.
    pub duration_ms: u32,
    pub has_audio: bool,
}

/// Reads a movie's size, length and whether it has sound. Fails when the bytes are not a
/// container FFmpeg can read or hold no video stream.
pub fn probe(bytes: Arc<[u8]>) -> Result<MediaInfo, VideoError> {
    init();
    let input = io::open_input(bytes)?;
    let stream = input
        .streams()
        .best(Type::Video)
        .ok_or_else(|| VideoError::new("no video stream"))?;
    let params = stream.parameters();
    // SAFETY: `params` wraps the stream's live `AVCodecParameters`.
    let (width, height) = unsafe {
        let p = params.as_ptr();
        ((*p).width, (*p).height)
    };
    let width = u32::try_from(width).unwrap_or(0);
    let height = u32::try_from(height).unwrap_or(0);
    if width == 0 || height == 0 {
        return Err(VideoError::new("the video stream has no size"));
    }
    let container_ms = input.duration().checked_div(1000).filter(|ms| *ms > 0);
    let duration_ms = container_ms
        .or_else(|| ts_to_ms(stream.duration(), stream.time_base()).filter(|ms| *ms > 0))
        .and_then(|ms| u32::try_from(ms).ok())
        .unwrap_or(0);
    let has_audio = input.streams().best(Type::Audio).is_some();
    Ok(MediaInfo {
        width,
        height,
        duration_ms,
        has_audio,
    })
}

/// Which decoders [`VideoDecoder::open`] tries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accel {
    /// CPU decoding only: deterministic, used for headless runs.
    Software,
    /// The platform's hardware decoder, falling back to software when it is unavailable.
    Hardware,
}

/// `ts` in `time_base` units as milliseconds; `None` for `AV_NOPTS_VALUE` or on overflow.
pub(crate) fn ts_to_ms(ts: i64, time_base: ffmpeg::Rational) -> Option<i64> {
    if ts == ffmpeg::ffi::AV_NOPTS_VALUE {
        return None;
    }
    let num = i128::from(time_base.numerator());
    let den = i128::from(time_base.denominator());
    let ms = i128::from(ts)
        .checked_mul(num)?
        .checked_mul(1000)?
        .checked_div(den)?;
    i64::try_from(ms).ok()
}
