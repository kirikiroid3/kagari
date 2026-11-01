//! Demuxing straight from memory.

use std::io::Cursor;
use std::sync::Arc;

use ffmpeg_next::format::{self, context::Input, context::StreamIo};

use crate::VideoError;

/// Opens a container over in-memory bytes through a custom, seekable `AVIOContext`.
pub(crate) fn open_input(bytes: Arc<[u8]>) -> Result<Input, VideoError> {
    let io = StreamIo::from_read_seek(Cursor::new(bytes))?;
    Ok(format::input_from_stream(io, None, None)?)
}
