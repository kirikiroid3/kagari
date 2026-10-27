//! `kagari lsp`: the language server over stdio.

use std::process::ExitCode;

use crate::failure::Failure;

pub fn run() -> Result<ExitCode, Failure> {
    kagari_lsp::run_stdio().map_err(|error| Failure(error.to_string()))?;
    Ok(ExitCode::SUCCESS)
}
