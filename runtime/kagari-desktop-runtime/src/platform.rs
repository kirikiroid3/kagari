//! [`DesktopPlatform`]: the engine's view of this machine.

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use kagari_engine::{AssetStore, Frame, Platform, Timestamp};
use kira::sound::static_sound::StaticSoundData;

use crate::archives::Archives;
use crate::gpu::Gpu;
use crate::sounds::SoundCache;
use crate::store::Store;

/// The time `local_time` reports.
pub enum TimeSource {
    Local,
    Fixed(Timestamp),
}

/// The seed `random_seed` reports.
pub enum SeedSource {
    System,
    Fixed(u64),
}

/// The fixed save time of headless runs.
pub const HEADLESS_TIME: Timestamp = Timestamp {
    year: 2026,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
};

pub struct DesktopPlatform {
    archives: Archives,
    store: Store,
    pub sounds: SoundCache,
    /// Set once the GPU exists (after the window opens, or at headless start).
    pub gpu: Option<Gpu>,
    time: TimeSource,
    seed: SeedSource,
}

impl DesktopPlatform {
    pub fn new(
        game_dir: &Path,
        store: Store,
        time: TimeSource,
        seed: SeedSource,
    ) -> DesktopPlatform {
        DesktopPlatform {
            archives: Archives::new(game_dir),
            store,
            sounds: SoundCache::default(),
            gpu: None,
            time,
            seed,
        }
    }

    /// The decoded sound at an asset path, read from the archives on first use.
    pub fn sound(&mut self, path: &str) -> Option<StaticSoundData> {
        if let Some(data) = self.sounds.get(path) {
            return Some(data);
        }
        if self.sounds.failed(path) {
            return None;
        }
        match self.archives.read(path) {
            Ok(Some(bytes)) => self.sounds.decode(path, &bytes),
            Ok(None) => {
                log::warn!("audio `{path}` not found");
                None
            }
            Err(e) => {
                log::warn!("cannot read audio `{path}`: {e}");
                None
            }
        }
    }
}

impl Platform for DesktopPlatform {
    fn read_store(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        self.store.read_store(name)
    }

    fn write_store(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.store.write_store(name, bytes)
    }

    fn delete_store(&mut self, name: &str) -> io::Result<()> {
        self.store.delete_store(name)
    }

    fn read_asset(&mut self, path: &str) -> io::Result<Option<Vec<u8>>> {
        self.archives.read(path)
    }

    fn audio_duration_ms(&mut self, path: &str, bytes: &[u8]) -> Option<u32> {
        let data = self.sounds.decode(path, bytes)?;
        u32::try_from(data.duration().as_millis()).ok()
    }

    fn render_thumbnail(
        &mut self,
        frame: &Frame,
        assets: &AssetStore,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let Some(gpu) = self.gpu.as_mut() else {
            log::warn!("thumbnail requested before the GPU is ready");
            return None;
        };
        let rgba = gpu.renderer.render_to_rgba(frame, assets, width, height);
        (!rgba.is_empty()).then_some(rgba)
    }

    fn local_time(&self) -> Timestamp {
        match &self.time {
            TimeSource::Fixed(t) => *t,
            TimeSource::Local => {
                let now = jiff::Zoned::now();
                Timestamp {
                    year: u16::try_from(now.year()).unwrap_or(0),
                    month: u8::try_from(now.month()).unwrap_or(1),
                    day: u8::try_from(now.day()).unwrap_or(1),
                    hour: u8::try_from(now.hour()).unwrap_or(0),
                    minute: u8::try_from(now.minute()).unwrap_or(0),
                    second: u8::try_from(now.second()).unwrap_or(0),
                }
            }
        }
    }

    fn random_seed(&mut self) -> u64 {
        match self.seed {
            SeedSource::Fixed(seed) => seed,
            SeedSource::System => {
                // Truncating the nanoseconds keeps their fast-changing low bits.
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                nanos ^ u64::from(std::process::id())
            }
        }
    }

    fn set_title(&mut self, title: &str) {
        self.store.set_title(title);
    }
}
