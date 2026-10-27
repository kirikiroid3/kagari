#![deny(clippy::unwrap_used, clippy::panic, clippy::expect_used, clippy::arithmetic_side_effects)]

//! The `kagari` command: create, check, build and format Kagari projects, disassemble
//! bytecode, and run the language server.

mod cli;
mod command;
mod failure;
mod workspace;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::failure::Failure;

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Init { path, name } => command::init::run(&path, name.as_deref()),
        Command::Check {
            manifest_path,
            resolutions,
        } => command::check::run(manifest_path.as_deref(), resolutions),
        Command::Build {
            manifest_path,
            targets,
        } => command::build::run(manifest_path.as_deref(), &targets),
        Command::Fmt {
            check,
            manifest_path,
            files,
        } => command::fmt::run(check, manifest_path.as_deref(), &files),
        Command::Disasm { file, all } => command::disasm::run(&file, all),
        Command::Lsp => command::lsp::run(),
    };
    match result {
        Ok(code) => code,
        Err(Failure(message)) => {
            eprintln!("kagari: {message}");
            ExitCode::from(2)
        }
    }
}
