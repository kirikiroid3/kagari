//! The debug channel (`--debug-port`, `--debug-stdio`): a debugger (`kagari dap`, `kagari debug
//! --attach`) talks to a live `kagari_debugger::Server`, polled once per frame.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use kagari_debugger::{Artifact, Link, Server, Target};
use kagari_engine::{Engine, Platform};

use crate::cli::Cli;
use crate::exit::{Failure, USAGE};

/// Where debuggers come from, and the one attached.
pub struct DebugPort {
    clients: Receiver<Link>,
    dir: PathBuf,
    server: Option<Server>,
    /// The debug artifact read at open, for the first debugger.
    artifact: Option<Artifact>,
}

impl DebugPort {
    /// The channel `cli` asks for, if any. With a debug artifact for the running
    /// `game.kagari` (a debug build), `engine` records call snapshots from now on, so a
    /// debugger can hot swap scenes.
    pub fn open(cli: &Cli, dir: &Path, engine: &mut Engine) -> Result<Option<DebugPort>, Failure> {
        let clients = if let Some(port) = cli.debug_port {
            let (address, clients) = kagari_debugger::listen(port)
                .map_err(|e| Failure::new(USAGE, format!("cannot listen on port {port}: {e}")))?;
            log::info!(target: "kagari::debug", "debugger: listening on {address}");
            clients
        } else if cli.debug_stdio {
            let (tx, rx) = std::sync::mpsc::channel();
            // The receiver keeps the one link; nothing else is ever sent.
            let _ = tx.send(Link::over(std::io::stdin(), std::io::stdout()));
            rx
        } else {
            return Ok(None);
        };
        let artifact = Artifact::load(dir, engine.script_hash());
        if artifact.is_some() {
            engine.debug_record_calls(true);
        }
        Ok(Some(DebugPort {
            clients,
            dir: dir.to_path_buf(),
            server: None,
            artifact,
        }))
    }

    /// Accepts a waiting debugger when none is attached, then serves the attached one.
    pub fn poll(&mut self, engine: &mut Engine, platform: &mut dyn Platform, now_ms: u64) {
        let mut t = Target {
            engine,
            platform,
            now_ms,
        };
        if self.server.as_ref().is_none_or(|s| !s.attached())
            && let Ok(link) = self.clients.try_recv()
        {
            let artifact = self
                .artifact
                .take()
                .or_else(|| Artifact::load(&self.dir, t.engine.script_hash()));
            if artifact.is_none() {
                log::warn!(
                    target: "kagari::debug",
                    "no debug artifact for this game.kagari (build with `--profile debug`): \
                     no story graph, no local names, no hot swap"
                );
            }
            log::info!(target: "kagari::debug", "debugger attached");
            self.server =
                Some(Server::attach(link, artifact, true, &mut t).with_game_dir(&self.dir));
        }
        if let Some(server) = self.server.as_mut() {
            server.poll(&mut t);
            if !server.attached() {
                log::info!(target: "kagari::debug", "debugger detached");
                self.server = None;
            }
        }
    }
}
