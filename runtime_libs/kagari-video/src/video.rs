//! Frames at a playback position.

use std::sync::Arc;

use ffmpeg_next::codec::{self, Id, threading};
use ffmpeg_next::format::context::Input;
use ffmpeg_next::media::Type;
use ffmpeg_next::{Error, Packet, Rational, decoder, frame};

use crate::convert::Converter;
use crate::hw::{self, Hw};
use crate::{Accel, VideoError, io, ts_to_ms};

/// A decoded frame as sRGB RGBA, alpha 255, rows packed (`width * 4` bytes each).
pub struct RgbaFrame {
    /// The frame's presentation time from the start of the movie.
    pub pts_ms: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A frame waiting to be shown, with its time in milliseconds.
type Timed = (frame::Video, i64);

/// Gives the frame on screen at a playback position, seeking when the position jumps.
pub struct VideoDecoder {
    input: Input,
    stream: usize,
    time_base: Rational,
    start: Option<i64>,
    decoder: decoder::Video,
    /// The hardware device while decoding on it; cleared by the fallback to software.
    hw: Option<Hw>,
    converter: Converter,
    /// The newest frame at or before the last target.
    current: Option<Timed>,
    /// `current` was taken although it is after the target (nothing earlier exists).
    early: bool,
    /// The decoded frame after `current`.
    next: Option<Timed>,
    /// `current` as RGBA.
    rgba: Option<Arc<RgbaFrame>>,
    /// The demuxer reached the end and the decoder is draining.
    eof: bool,
    /// The last frame time seen, for frames without a timestamp.
    last_ms: i64,
}

/// Seek forward instead of decoding when the target is this far past the next frame.
const SEEK_AHEAD_MS: i64 = 2000;

impl VideoDecoder {
    /// Opens the best video stream of a movie. `Accel::Hardware` falls back to software when the
    /// platform's hardware decoder cannot be set up.
    pub fn open(bytes: Arc<[u8]>, accel: Accel) -> Result<VideoDecoder, VideoError> {
        crate::init();
        let input = io::open_input(bytes)?;
        let stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| VideoError::new("no video stream"))?;
        let index = stream.index();
        let time_base = stream.time_base();
        let start = Some(stream.start_time()).filter(|s| *s != ffmpeg_next::ffi::AV_NOPTS_VALUE);
        let params = stream.parameters();
        let (decoder, hw) = match accel {
            Accel::Software => (open_software(params, time_base)?, None),
            Accel::Hardware => match open_hardware(params.clone(), time_base) {
                Ok((decoder, hw)) => (decoder, Some(hw)),
                Err(reason) => {
                    log_fallback(&reason);
                    (open_software(params, time_base)?, None)
                }
            },
        };
        Ok(VideoDecoder {
            input,
            stream: index,
            time_base,
            start,
            decoder,
            hw,
            converter: Converter::new(),
            current: None,
            early: false,
            next: None,
            rgba: None,
            eof: false,
            last_ms: 0,
        })
    }

    /// The decoding backend: `"software"`, `"vaapi"`, `"d3d11va"` or `"videotoolbox"`.
    pub fn backend(&self) -> &'static str {
        self.hw.as_ref().map_or("software", Hw::name)
    }

    /// The frame shown at `target_ms`: the last frame at or before it, or the first frame when
    /// the target is before it. `None` when the stream has no frame at all.
    pub fn frame_at(&mut self, target_ms: u64) -> Result<Option<Arc<RgbaFrame>>, VideoError> {
        let target = i64::try_from(target_ms).unwrap_or(i64::MAX);
        match self.advance(target) {
            Err(e) if self.hw.is_some() => {
                log_fallback(&e.to_string());
                self.reopen_software()?;
                self.seek(target)?;
                self.advance(target)
            }
            result => result,
        }
    }

    fn advance(&mut self, target: i64) -> Result<Option<Arc<RgbaFrame>>, VideoError> {
        let behind = self
            .current
            .as_ref()
            .is_some_and(|(_, pts)| target < *pts && !self.early);
        let ahead = match (&self.current, &self.next) {
            (_, Some((_, pts))) => target > pts.saturating_add(SEEK_AHEAD_MS),
            (None, None) => target > SEEK_AHEAD_MS,
            (Some(_), None) => false,
        };
        if behind || ahead {
            self.seek(target)?;
        }
        loop {
            if self.next.is_none() {
                self.next = self.decode_next()?;
            }
            match &self.next {
                Some((_, pts)) if *pts <= target => {
                    self.current = self.next.take();
                    self.early = false;
                }
                _ => break,
            }
        }
        if self.current.is_none() {
            self.current = self.next.take();
            self.early = true;
        }
        let Some((frame, pts)) = &self.current else {
            return Ok(None);
        };
        let pts_ms = u64::try_from(*pts).unwrap_or(0);
        if let Some(rgba) = self.rgba.as_ref().filter(|r| r.pts_ms == pts_ms) {
            return Ok(Some(Arc::clone(rgba)));
        }
        let rgba = match &self.hw {
            Some(hw) if hw.owns(frame) => self.converter.rgba(&hw::download(frame)?)?,
            _ => self.converter.rgba(frame)?,
        };
        let rgba = Arc::new(RgbaFrame {
            pts_ms,
            width: frame.width(),
            height: frame.height(),
            rgba,
        });
        self.rgba = Some(Arc::clone(&rgba));
        Ok(Some(rgba))
    }

    /// Moves the demuxer to the keyframe at or before `target` and resets the decoder.
    fn seek(&mut self, target: i64) -> Result<(), VideoError> {
        let ts = target.saturating_mul(1000);
        self.input.seek(ts, ..ts)?;
        self.decoder.flush();
        self.current = None;
        self.next = None;
        self.early = false;
        self.eof = false;
        Ok(())
    }

    /// The next decoded frame in presentation order; `None` once the stream is drained.
    fn decode_next(&mut self) -> Result<Option<Timed>, VideoError> {
        loop {
            let mut frame = frame::Video::empty();
            match self.decoder.receive_frame(&mut frame) {
                Ok(()) => {
                    if self.hw.as_ref().is_some_and(|hw| !hw.owns(&frame)) {
                        // FFmpeg chose its software path inside the hardware decoder.
                        return Err(VideoError::new("the hardware declined the stream"));
                    }
                    let pts = frame
                        .timestamp()
                        .or_else(|| frame.pts())
                        .and_then(|ts| self.frame_ms(ts))
                        .unwrap_or(self.last_ms);
                    self.last_ms = pts;
                    return Ok(Some((frame, pts)));
                }
                Err(Error::Eof) => return Ok(None),
                Err(Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                    if self.eof {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e.into()),
            }
            self.feed()?;
        }
    }

    /// Sends the next packet of the video stream, or the end of stream.
    fn feed(&mut self) -> Result<(), VideoError> {
        let mut packet = Packet::empty();
        loop {
            match packet.read(&mut self.input) {
                Ok(()) if packet.stream() == self.stream => {
                    return Ok(self.decoder.send_packet(&packet)?);
                }
                Ok(()) | Err(Error::InvalidData) => {}
                Err(Error::Eof) => {
                    self.eof = true;
                    return Ok(self.decoder.send_eof()?);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// A frame timestamp in stream units as milliseconds from the stream start, at least 0.
    fn frame_ms(&self, ts: i64) -> Option<i64> {
        let ts = match self.start {
            Some(start) => ts.checked_sub(start)?,
            None => ts,
        };
        Some(ts_to_ms(ts, self.time_base)?.max(0))
    }

    /// Drops the hardware decoder for a software one on the same stream.
    fn reopen_software(&mut self) -> Result<(), VideoError> {
        let params = self
            .input
            .stream(self.stream)
            .ok_or_else(|| VideoError::new("the video stream disappeared"))?
            .parameters();
        self.decoder = open_software(params, self.time_base)?;
        self.hw = None;
        self.current = None;
        self.next = None;
        self.early = false;
        self.eof = false;
        Ok(())
    }
}

fn log_fallback(reason: &str) {
    log::info!(
        target: "kagari::video",
        "video: hardware decoding unavailable ({reason}); using software"
    );
}

fn context(params: codec::Parameters, time_base: Rational) -> Result<codec::Context, VideoError> {
    let mut ctx = codec::Context::from_parameters(params)?;
    // SAFETY: setting a field of an unopened context; decoders use it for frame timestamps.
    unsafe { (*ctx.as_mut_ptr()).pkt_timebase = time_base.into() };
    Ok(ctx)
}

fn open_software(
    params: codec::Parameters,
    time_base: Rational,
) -> Result<decoder::Video, VideoError> {
    let mut ctx = context(params, time_base)?;
    let id = ctx.id();
    let codec = match id {
        Id::AV1 => decoder::find_by_name("libdav1d").or_else(|| decoder::find(id)),
        _ => decoder::find(id),
    }
    .ok_or_else(|| VideoError::new(format!("no decoder for {id:?}")))?;
    ctx.set_threading(threading::Config {
        kind: threading::Type::Frame,
        count: 0,
    });
    Ok(ctx.decoder().open_as(codec)?.video()?)
}

/// The native decoder on the hardware device; the error is the fallback reason.
fn open_hardware(
    params: codec::Parameters,
    time_base: Rational,
) -> Result<(decoder::Video, Hw), String> {
    let mut ctx = context(params, time_base).map_err(|e| e.to_string())?;
    let id = ctx.id();
    let codec = match id {
        Id::AV1 => decoder::find_by_name("av1"),
        _ => decoder::find(id),
    }
    .ok_or_else(|| format!("no native decoder for {id:?}"))?;
    let hw = hw::attach(codec, &mut ctx)?;
    let decoder = ctx
        .decoder()
        .open_as(codec)
        .and_then(|o| o.video())
        .map_err(|e| format!("cannot open the {} decoder: {e}", hw.name()))?;
    Ok((decoder, hw))
}
