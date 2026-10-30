use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// The Kagari toolchain.
#[derive(Parser)]
#[command(name = "kagari", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new project in PATH.
    Init {
        /// The project directory; created when missing.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// The project name; defaults to the directory name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Check the project and print its diagnostics.
    Check {
        /// The project's `kagari.toml`; by default the nearest one from the current directory.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Also print what every name resolves to.
        #[arg(long)]
        resolutions: bool,
    },
    /// Build the project's bytecode and asset archives.
    Build {
        /// The project's `kagari.toml`; by default the nearest one from the current directory.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Build only these targets (each listed in `build.targets`).
        #[arg(long = "target", value_name = "TARGET")]
        targets: Vec<String>,
        /// The build profile; `debug` also writes the story graph and debug information.
        #[arg(long, value_enum, default_value_t = Profile::Release)]
        profile: Profile,
    },
    /// Delete the build outputs and the analysis cache.
    Clean {
        /// The project's `kagari.toml`; by default the nearest one from the current directory.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
    /// Format the project's source files, or the given files.
    Fmt {
        /// Report files that would change instead of rewriting them.
        #[arg(long)]
        check: bool,
        /// The project's `kagari.toml`; by default the nearest one from the current directory.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Files to format instead of the project's.
        files: Vec<PathBuf>,
    },
    /// Disassemble a `.kagari` bytecode file.
    Disasm {
        file: PathBuf,
        /// Include std functions.
        #[arg(long)]
        all: bool,
    },
    /// Run the language server over stdio.
    Lsp,
    /// Debug the story: the query engine's REPL on the headless game (by default), a game in a
    /// window (`--window`), or a running game (`--attach`).
    Debug {
        /// The project's `kagari.toml`; by default the nearest one from the current directory.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// The build target to debug [default: the first of `build.targets`].
        #[arg(long)]
        target: Option<String>,
        /// Attach to a game started with `--debug-port PORT`: `PORT` or `HOST:PORT`.
        #[arg(long, value_name = "ADDRESS")]
        attach: Option<String>,
        /// Launch the game in a window (`kagari-desktop-runtime`) instead of headless.
        #[arg(long, conflicts_with = "attach")]
        window: bool,
        /// The runtime `--window` launches [default: next to `kagari`, else on PATH].
        #[arg(long, value_name = "PATH", requires = "window")]
        runtime: Option<PathBuf>,
        /// The entry scene the headless story starts at [default: the first].
        #[arg(long, value_name = "PATH", conflicts_with_all = ["attach", "window"])]
        entry: Option<String>,
        /// The PRNG seed of the headless story.
        #[arg(long, default_value_t = 1, conflicts_with_all = ["attach", "window"])]
        seed: u64,
        /// Do not run `kagari build --profile debug` first.
        #[arg(long)]
        no_build: bool,
    },
}

/// What `kagari build` writes besides the bytecode and the archives. The bytecode is the same
/// in both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Profile {
    /// Also `game.kagari-cache`: the story graph and debug information.
    Debug,
    /// Only the bytecode and the archives.
    Release,
}
