//! Command-line arguments.

use std::path::PathBuf;

/// The window backend on Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendArg {
    Wayland,
    X11,
}

#[derive(clap::Parser, Debug)]
#[command(
    name = "kagari-desktop-runtime",
    version,
    about = "Runs a game built by `kagari build`"
)]
pub struct Cli {
    /// Directory with `game.kagari` and the `.kiarchive` files [default: the executable's directory]
    pub game_dir: Option<PathBuf>,
    /// Directory for settings and saves [default: the platform data directory, `Kagari/<title>`]
    #[arg(long, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
    /// Window backend (Linux only) [default: wayland when WAYLAND_DISPLAY is set, else x11]
    #[arg(long, value_enum)]
    pub backend: Option<BackendArg>,
    /// Run without a window, driven by `--script`, writing captures to `--out`
    #[arg(long, requires_all = ["script", "out"])]
    pub headless: bool,
    /// The headless input script
    #[arg(long, value_name = "FILE", requires = "headless")]
    pub script: Option<PathBuf>,
    /// The directory headless captures (and, without `--data-dir`, saves) go to
    #[arg(long, value_name = "DIR", requires = "headless")]
    pub out: Option<PathBuf>,
    /// The seed of every new story in headless mode [default: 1]
    #[arg(long, value_name = "N", requires = "headless")]
    pub seed: Option<u64>,
    /// Accept a debugger (`kagari dap`, `kagari debug --attach`) on 127.0.0.1:PORT (0 picks a
    /// free port, logged at start)
    #[arg(long, value_name = "PORT", conflicts_with_all = ["headless", "debug_stdio"])]
    pub debug_port: Option<u16>,
    /// Talk to a debugger over standard input and output (how `kagari dap` launches a game)
    #[arg(long, conflicts_with = "headless")]
    pub debug_stdio: bool,
}
