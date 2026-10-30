//! The headless query engine: a game directory booted without a window, audio or stores on
//! disk, served over a link until the client disconnects.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use kagari_archive::archive::ArchiveDir;
use kagari_engine::{AssetStore, Engine, Frame, Platform, Timestamp};

use crate::frames::Artifact;
use crate::server::Server;
use crate::session::Target;
use crate::transport::Link;

/// The bytecode file of a game directory.
pub const BYTECODE_FILE: &str = "game.kagari";

/// A platform for the query engine: assets from the game directory's archives, stores in
/// memory (empty at boot, never written to disk), no audio lengths, no thumbnails, a fixed
/// time and seed.
pub struct HeadlessPlatform {
    archives: ArchiveDir,
    stores: HashMap<String, Vec<u8>>,
    seed: u64,
}

impl HeadlessPlatform {
    pub fn new(dir: &Path, seed: u64) -> HeadlessPlatform {
        HeadlessPlatform {
            archives: ArchiveDir::new(dir),
            stores: HashMap::new(),
            seed,
        }
    }
}

impl Platform for HeadlessPlatform {
    fn read_store(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.stores.get(name).cloned())
    }

    fn write_store(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.stores.insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn delete_store(&mut self, name: &str) -> io::Result<()> {
        self.stores.remove(name);
        Ok(())
    }

    fn read_asset(&mut self, path: &str) -> io::Result<Option<Vec<u8>>> {
        self.archives.read(path, |file| {
            log::warn!(target: "kagari::debug", "missing archive {}", file.display());
        })
    }

    fn audio_duration_ms(&mut self, _path: &str, _bytes: &[u8]) -> Option<u32> {
        None
    }

    fn render_thumbnail(
        &mut self,
        _frame: &Frame,
        _assets: &AssetStore,
        _width: u32,
        _height: u32,
    ) -> Option<Vec<u8>> {
        None
    }

    fn local_time(&self) -> Timestamp {
        Timestamp {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    fn random_seed(&mut self) -> u64 {
        self.seed
    }

    fn set_title(&mut self, _title: &str) {}
}

/// How the query engine starts the story.
#[derive(Clone, Debug, Default)]
pub struct StartOptions {
    /// The entry scene's path; default: the story graph's first root, or the program's first
    /// entry scene.
    pub entry: Option<String>,
    pub seed: u64,
}

/// A booted game directory.
pub struct Headless {
    pub engine: Engine,
    pub platform: HeadlessPlatform,
    pub artifact: Option<Artifact>,
    pub dir: PathBuf,
}

impl Headless {
    /// Boots `dir/game.kagari` and reads its debug artifact when one matches.
    pub fn open(dir: &Path, seed: u64) -> Result<Headless, String> {
        let path = dir.join(BYTECODE_FILE);
        let bytes =
            std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut platform = HeadlessPlatform::new(dir, seed);
        let engine = Engine::boot(&bytes, &mut platform, 0).map_err(|e| e.to_string())?;
        if let Some(crash) = engine.crash() {
            return Err(format!("the game stopped while booting: {crash}"));
        }
        let artifact = Artifact::load(dir, engine.script_hash());
        Ok(Headless {
            engine,
            platform,
            artifact,
            dir: dir.to_path_buf(),
        })
    }

    pub fn target(&mut self) -> Target<'_> {
        Target {
            engine: &mut self.engine,
            platform: &mut self.platform,
            now_ms: 0,
        }
    }
}

/// The query engine's thread body: boots `dir`, starts the story, attaches a headless server
/// to `link` and serves it until the client disconnects. A failure to boot or start is sent
/// as an `output` event, then the link closes.
pub fn serve(dir: &Path, start: &StartOptions, link: Link) {
    let mut game = match Headless::open(dir, start.seed) {
        Ok(game) => game,
        Err(error) => {
            fail(link, &error);
            return;
        }
    };
    let artifact = game.artifact.take();
    let mut t = game.target();
    let mut server = Server::attach(link, artifact, false, &mut t);
    match server
        .debugger()
        .start(&mut t, start.entry.as_deref(), start.seed)
    {
        Ok(()) => {
            let body = server.debugger().stopped(&t, "entry", None);
            server.send_stopped(body);
        }
        Err(error) => {
            let body = server.debugger().stopped(&t, "error", Some(error));
            server.send_stopped(body);
        }
    }
    server.serve(&mut t);
}

fn fail(link: Link, error: &str) {
    let conn = crate::transport::Connection::new(link);
    conn.output(format!("kagari debug: {error}\n"));
    conn.event(
        crate::protocol::event::STOPPED,
        crate::protocol::StoppedBody {
            reason: "error".to_owned(),
            description: Some(error.to_owned()),
            ..Default::default()
        },
    );
}
