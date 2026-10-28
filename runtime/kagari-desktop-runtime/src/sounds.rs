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
        match StaticSoundData::from_cursor(Cursor::new(bytes.to_vec())) {
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
