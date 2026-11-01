//! A movie's soundtrack as interleaved stereo `f32` frames at the stream's own rate.

use std::sync::Arc;

use ffmpeg_next::ffi::AV_NOPTS_VALUE;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::format::{Sample, sample};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::resampling;
use ffmpeg_next::util::channel_layout::ChannelLayout;
use ffmpeg_next::{Error, Packet, Rational, codec, decoder, frame};

use crate::{VideoError, io};

/// Frames of silence returned per `decode` call after the end of the stream.
const SILENCE_FRAMES: usize = 1024;

/// Resampled frames with the sample index of the first one.
type Chunk = (usize, Vec<[f32; 2]>);

/// Decodes the best audio stream of a movie, sample-exactly seekable.
pub struct AudioDecoder {
    input: Input,
    stream: usize,
    time_base: Rational,
    start: Option<i64>,
    decoder: decoder::Audio,
    resampler: Option<resampling::Context>,
    rate: u32,
    num_frames: usize,
    /// Decoded frames not handed out yet.
    pending: Vec<[f32; 2]>,
    /// The sample index of the next decoded frame; `None` right after a seek, until a frame
    /// with a timestamp arrives.
    cursor: Option<usize>,
    /// The demuxer reached the end and the decoder is draining.
    eof: bool,
    /// The decoder is drained: only silence follows.
    done: bool,
}

impl AudioDecoder {
    /// Opens the best audio stream; `Ok(None)` when the movie has none.
    pub fn open(bytes: Arc<[u8]>) -> Result<Option<AudioDecoder>, VideoError> {
        crate::init();
        let input = io::open_input(bytes)?;
        let Some(stream) = input.streams().best(Type::Audio) else {
            return Ok(None);
        };
        let index = stream.index();
        let time_base = stream.time_base();
        let start = Some(stream.start_time()).filter(|s| *s != AV_NOPTS_VALUE);
        let stream_duration = stream.duration();
        let mut ctx = codec::Context::from_parameters(stream.parameters())?;
        // SAFETY: setting a field of an unopened context; decoders use it for frame timestamps.
        unsafe { (*ctx.as_mut_ptr()).pkt_timebase = time_base.into() };
        let decoder = ctx.decoder().audio()?;
        let rate = decoder.rate();
        if rate == 0 {
            return Err(VideoError::new("the audio stream has no sample rate"));
        }
        let from_stream = (stream_duration != AV_NOPTS_VALUE && stream_duration > 0)
            .then(|| scale(stream_duration, time_base, rate))
            .flatten();
        let from_container = || {
            let us = input.duration();
            (us > 0)
                .then(|| scale(us, Rational::new(1, 1_000_000), rate))
                .flatten()
        };
        let num_frames = from_stream.or_else(from_container).unwrap_or(0);
        Ok(Some(AudioDecoder {
            input,
            stream: index,
            time_base,
            start,
            decoder,
            resampler: None,
            rate,
            num_frames,
            pending: Vec::new(),
            cursor: Some(0),
            eof: false,
            done: false,
        }))
    }

    /// Frames per second.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// The length in frames, from the stream (else the container) duration; 0 when unknown.
    pub fn num_frames(&self) -> usize {
        self.num_frames
    }

    /// The next frames; never empty. After the end of the stream, silence.
    pub fn decode(&mut self) -> Result<Vec<[f32; 2]>, VideoError> {
        if !self.pending.is_empty() {
            return Ok(std::mem::take(&mut self.pending));
        }
        while let Some((_, samples)) = self.next_chunk()? {
            if !samples.is_empty() {
                return Ok(samples);
            }
        }
        Ok(vec![[0.0; 2]; SILENCE_FRAMES])
    }

    /// Moves to frame `index`, so that the next `decode` starts there. Returns `index`.
    pub fn seek(&mut self, index: usize) -> Result<usize, VideoError> {
        let rate = u64::from(self.rate);
        let us = u64::try_from(index)
            .ok()
            .and_then(|i| i.checked_mul(1_000_000))
            .and_then(|i| i.checked_div(rate))
            .and_then(|i| i64::try_from(i).ok())
            .unwrap_or(i64::MAX);
        self.input.seek(us, ..us)?;
        self.decoder.flush();
        self.resampler = None;
        self.pending.clear();
        self.cursor = None;
        self.eof = false;
        self.done = false;
        while let Some((start, samples)) = self.next_chunk()? {
            let end = start.saturating_add(samples.len());
            if end > index {
                let skip = index.saturating_sub(start);
                self.pending = samples.get(skip..).map(<[_]>::to_vec).unwrap_or_default();
                break;
            }
        }
        Ok(index)
    }

    /// The next decoded and resampled frame with the sample index of its first sample;
    /// `None` once the stream is drained.
    fn next_chunk(&mut self) -> Result<Option<Chunk>, VideoError> {
        let Some(frame) = self.next_frame()? else {
            return Ok(None);
        };
        let start = match self.cursor {
            Some(cursor) => cursor,
            None => frame
                .timestamp()
                .or_else(|| frame.pts())
                .and_then(|ts| self.sample_index(ts))
                .unwrap_or(0),
        };
        let samples = self.resample(&frame)?;
        self.cursor = Some(start.saturating_add(samples.len()));
        Ok(Some((start, samples)))
    }

    fn next_frame(&mut self) -> Result<Option<frame::Audio>, VideoError> {
        if self.done {
            return Ok(None);
        }
        loop {
            let mut frame = frame::Audio::empty();
            match self.decoder.receive_frame(&mut frame) {
                Ok(()) => return Ok(Some(frame)),
                Err(Error::Eof) => {
                    self.done = true;
                    return Ok(None);
                }
                Err(Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                    if self.eof {
                        self.done = true;
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e.into()),
            }
            self.feed()?;
        }
    }

    /// Sends the next packet of the audio stream, or the end of stream.
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

    /// A timestamp in stream units as a sample index from the stream start.
    fn sample_index(&self, ts: i64) -> Option<usize> {
        let ts = match self.start {
            Some(start) => ts.checked_sub(start)?,
            None => ts,
        };
        scale(ts.max(0), self.time_base, self.rate)
    }

    /// `frame` converted to interleaved stereo `f32` at the same rate.
    fn resample(&mut self, frame: &frame::Audio) -> Result<Vec<[f32; 2]>, VideoError> {
        if frame.samples() == 0 {
            return Ok(Vec::new());
        }
        let mut layout = frame.channel_layout();
        if layout.is_empty() {
            layout = ChannelLayout::default(i32::from(frame.channels()));
        }
        let (format, rate) = (frame.format(), frame.rate());
        let resampler = match &mut self.resampler {
            Some(resampler)
                if resampler.input().format == format
                    && resampler.input().channel_layout == layout
                    && resampler.input().rate == rate =>
            {
                resampler
            }
            slot => slot.insert(resampling::Context::get(
                format,
                layout,
                rate,
                Sample::F32(sample::Type::Packed),
                ChannelLayout::STEREO,
                rate,
            )?),
        };
        let mut out = frame::Audio::empty();
        resampler.run(frame, &mut out)?;
        let (floats, _) = out.data(0).as_chunks::<4>();
        let (pairs, _) = floats.as_chunks::<2>();
        Ok(pairs
            .iter()
            .take(out.samples())
            .map(|[l, r]| [f32::from_ne_bytes(*l), f32::from_ne_bytes(*r)])
            .collect())
    }
}

/// `ts` in `time_base` units as a number of samples at `rate`; `None` on overflow or when
/// negative.
fn scale(ts: i64, time_base: Rational, rate: u32) -> Option<usize> {
    let samples = i128::from(ts)
        .checked_mul(i128::from(time_base.numerator()))?
        .checked_mul(i128::from(rate))?
        .checked_div(i128::from(time_base.denominator()))?;
    usize::try_from(samples).ok()
}
