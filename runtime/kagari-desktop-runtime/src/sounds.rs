//! Decoded sounds, cached by asset path.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use kira::sound::static_sound::StaticSoundData;

/// The decoded-audio budget, counted as 8 bytes per frame (two `f32` channels).
const CAPACITY: usize = 256 << 20;

/// Decoded sounds, least recently used evicted past [`CAPACITY`]. Playing sounds keep their
/// own `Arc` of the frames, so eviction never cuts one off.
#[derive(Default)]
pub struct SoundCache {
    sounds: HashMap<String, (StaticSoundData, u64)>,
    /// Paths that failed to decode, so the warning is logged once.
    failed: HashSet<String>,
    clock: u64,
    bytes: usize,
}

impl SoundCache {
    /// The cached sound at `path`, if decoded before.
    pub fn get(&mut self, path: &str) -> Option<StaticSoundData> {
        self.clock = self.clock.saturating_add(1);
        let clock = self.clock;
        self.sounds.get_mut(path).map(|(data, used)| {
            *used = clock;
            data.clone()
        })
    }

    /// Whether `path` failed to decode before.
    pub fn failed(&self, path: &str) -> bool {
        self.failed.contains(path)
    }

    /// The sound at `path`: cached, or decoded from `bytes` and cached. A decode error is
    /// logged once and gives `None`.
    pub fn decode(&mut self, path: &str, bytes: &[u8]) -> Option<StaticSoundData> {
        if let Some(data) = self.get(path) {
            return Some(data);
        }
        if self.failed.contains(path) {
            return None;
        }
        let mut bytes = bytes.to_vec();
        fit_wav_sizes(&mut bytes);
        match StaticSoundData::from_cursor(Cursor::new(bytes)) {
            Ok(data) => {
                self.insert(path, data.clone());
                Some(data)
            }
            Err(e) => {
                log::warn!("cannot decode audio `{path}`: {e}");
                self.failed.insert(path.to_owned());
                None
            }
        }
    }

    fn insert(&mut self, path: &str, data: StaticSoundData) {
        let size = data.frames.len().saturating_mul(8);
        self.bytes = self.bytes.saturating_add(size);
        while self.bytes > CAPACITY {
            let Some(oldest) = self
                .sounds
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(p, _)| p.clone())
            else {
                break;
            };
            if let Some((old, _)) = self.sounds.remove(&oldest) {
                self.bytes = self
                    .bytes
                    .saturating_sub(old.frames.len().saturating_mul(8));
            }
        }
        self.sounds.insert(path.to_owned(), (data, self.clock));
    }
}

/// Clamps the RIFF and `data` chunk sizes of a WAV to the bytes present, so a size past the end
/// of the file means "until the end": the `0xFFFFFFFF` placeholder that streaming encoders write
/// (ffmpeg to a pipe, TTS services), or a truncated file. Symphonia reads such a `data` chunk
/// until it runs out of bytes and then fails the whole decode. Other files are left alone.
fn fit_wav_sizes(bytes: &mut [u8]) -> Option<()> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    clamp_size(bytes, 4)?;
    let mut at: usize = 12;
    loop {
        let size_at = at.checked_add(4)?;
        if bytes.get(at..size_at)? == b"data" {
            return clamp_size(bytes, size_at).map(drop);
        }
        let size = usize::try_from(size_field(bytes, size_at)?).ok()?;
        // Chunks are padded to an even size.
        at = size_at
            .checked_add(4)?
            .checked_add(size)?
            .checked_add(size & 1)?;
    }
}

/// The little-endian chunk size at `at`.
fn size_field(bytes: &[u8], at: usize) -> Option<u32> {
    let field = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(field.try_into().ok()?))
}

/// Lowers the chunk size at `at` to the bytes after it, if it claims more; gives the new size.
fn clamp_size(bytes: &mut [u8], at: usize) -> Option<u32> {
    let body = at.checked_add(4)?;
    let left = u32::try_from(bytes.len().checked_sub(body)?).unwrap_or(u32::MAX);
    let size = size_field(bytes, at)?.min(left);
    bytes
        .get_mut(at..body)?
        .copy_from_slice(&size.to_le_bytes());
    Some(size)
}
