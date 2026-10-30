#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! `kagari-desktop-runtime`: runs the output of `kagari build` (`game.kagari` plus the
//! `.kiarchive` files) in a window (winit + wgpu, kira audio, gilrs gamepads), or headless from
//! an input script.

mod app;
mod audio;
mod cli;
mod exit;
mod game;
mod gpu;
mod headless;
mod input;
mod pads;
mod platform;
mod sounds;
mod store;
#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;
use crate::exit::Failure;

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("kagari=info,wgpu=warn"),
    )
    .init();
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(failure) => {
            eprintln!("kagari-desktop-runtime: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

fn run(cli: &Cli) -> Result<u8, Failure> {
    let dir = game::game_dir(cli.game_dir.clone())?;
    let bytes = game::read_program(&dir)?;
    if cli.headless {
        headless::run(cli, &dir, &bytes)?;
        Ok(0)
    } else {
        app::run(cli, &dir, &bytes)
    }
}
