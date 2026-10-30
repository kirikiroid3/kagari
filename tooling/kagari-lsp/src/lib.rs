#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari language server: diagnostics, formatting, go to definition, hover and document
//! symbols over the Language Server Protocol. `kagari lsp` runs it on stdio.

mod convert;
mod diagnostics;
mod error;
pub mod line_index;
mod projects;
mod server;
pub mod std_files;

use lsp_server::Connection;

pub use error::LspError;

/// Runs the server on `connection` until the client sends `shutdown` and `exit`.
pub fn run(connection: Connection) -> Result<(), LspError> {
    server::run(connection)
}

/// Runs the server on stdin/stdout.
pub fn run_stdio() -> Result<(), LspError> {
    let (connection, io_threads) = Connection::stdio();
    run(connection)?;
    io_threads.join()?;
    Ok(())
}
