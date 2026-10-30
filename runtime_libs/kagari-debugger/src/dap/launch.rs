//! `launch` and `attach`: the editor's configuration is read here, the front end's
//! [`Launcher`] opens the project, and the result is a [`Prepared`] runtime that the adapter
//! connects ([`Adapter::start`]).

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use super::answers::Waiting;
use super::{Adapter, Session};
use crate::client::{Backend, Locator};
use crate::headless::{BYTECODE_FILE, StartOptions};
use crate::protocol::command;
use crate::transport::Connection;

/// Where the project is, as a `launch` or `attach` configuration names it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Place {
    /// The project's `kagari.toml` (relative to `cwd`), or else the nearest one.
    pub manifest_path: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    /// The build target; by default the project's first.
    pub target: Option<String>,
}

/// What a [`Launcher`] found for a [`Place`].
pub struct Opened {
    /// The target directory: `game.kagari`, the archives, the debug artifact.
    pub game_dir: PathBuf,
    /// The directory the bytecode's source file names are relative to.
    pub source_root: PathBuf,
}

/// What the adapter needs of the front end that runs it: projects.
pub trait Launcher {
    /// Finds the project and target directory of `place`; with `build`, builds its debug
    /// profile first, each output line to `out`.
    fn open(
        &mut self,
        place: &Place,
        build: bool,
        out: &mut dyn FnMut(&str),
    ) -> Result<Opened, String>;

    /// The runtime a window launch starts when the configuration names none.
    fn default_runtime(&self) -> PathBuf;
}

/// `launch` arguments.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct LaunchArgs {
    #[serde(flatten)]
    place: Place,
    mode: Option<String>,
    runtime: Option<PathBuf>,
    runtime_args: Vec<String>,
    entry: Option<String>,
    seed: Option<u64>,
    build_first: Option<bool>,
    stop_on_entry: bool,
}

/// `attach` arguments.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct AttachArgs {
    #[serde(flatten)]
    place: Place,
    host: Option<String>,
    port: Option<u16>,
}

/// A runtime to connect to, with the sources of its game.
pub(super) struct Prepared {
    backend: Backend,
    locator: Locator,
    /// Send `pause` once connected (`stopOnEntry`).
    pause: bool,
}

fn locator(opened: &Opened) -> Locator {
    Locator::new(
        &opened.source_root,
        Some(&opened.game_dir.join(BYTECODE_FILE)),
    )
}

impl Adapter<'_> {
    pub(super) fn prepare_launch(&mut self, a: LaunchArgs) -> Result<Prepared, String> {
        let window = match a.mode.as_deref() {
            None | Some("window") => true,
            Some("headless") => false,
            Some(other) => return Err(format!("`mode` is `window` or `headless`, not `{other}`")),
        };
        let editor = &self.editor;
        let opened = self
            .launcher
            .open(&a.place, a.build_first.unwrap_or(true), &mut |line| {
                super::output(editor, format!("{line}\n"))
            })?;
        let dir = opened.game_dir.clone();
        let backend = if window {
            let runtime = match (a.runtime, a.place.cwd.as_deref()) {
                (Some(runtime), Some(cwd))
                    if runtime.is_relative() && cwd.join(&runtime).exists() =>
                {
                    cwd.join(runtime)
                }
                (Some(runtime), _) => runtime,
                (None, _) => self.launcher.default_runtime(),
            };
            Backend::Window {
                runtime,
                dir,
                args: a.runtime_args,
            }
        } else {
            Backend::Headless {
                dir,
                start: StartOptions {
                    entry: a.entry,
                    seed: a.seed.unwrap_or(1),
                },
            }
        };
        Ok(Prepared {
            backend,
            locator: locator(&opened),
            pause: window && a.stop_on_entry,
        })
    }

    pub(super) fn prepare_attach(&mut self, a: AttachArgs) -> Result<Prepared, String> {
        let port = a.port.ok_or("`attach` needs the game's `port`")?;
        let host = a.host.unwrap_or_else(|| "127.0.0.1".to_owned());
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let opened = self.launcher.open(&a.place, false, &mut |_| {})?;
        Ok(Prepared {
            backend: Backend::Attach { address },
            locator: locator(&opened),
            pause: false,
        })
    }

    /// Connects to a prepared runtime and asks it to `initialize`; the editor's `launch` or
    /// `attach` (`seq`, `name`) is answered with the runtime's answer.
    pub(super) fn start(&mut self, seq: i64, name: &str, prepared: Prepared) -> Result<(), String> {
        let mut attached = prepared.backend.connect()?;
        let link = attached
            .link
            .take()
            .ok_or("the runtime has no connection")?;
        self.session = Some(Session {
            runtime: Connection::new(link),
            attached,
            locator: prepared.locator,
        });
        self.held = Some(Vec::new());
        let started = Waiting::Started {
            seq,
            command: name.to_owned(),
        };
        self.forward(command::INITIALIZE, Value::Null, started)?;
        if prepared.pause {
            self.forward(command::PAUSE, Value::Null, Waiting::Silent)?;
        }
        Ok(())
    }
}
