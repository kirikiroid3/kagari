#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari front end: a salsa database running compilation stages 1-3
//! (collect, structure check, represent check) and lowering the story to a
//! narrative IR.

mod db;
mod diagnostic;
mod input;
mod pipeline;

pub mod collect;
pub mod exhaustive;
pub mod ir;
pub mod kis;
pub mod parse;
pub mod represent;

pub use db::{Db, FileSystem, OsFileSystem, RootDatabase};
pub use diagnostic::{Diagnostic, DiagnosticCode, Related, Severity, Span, render_diagnostics};
pub use input::{FilePath, Program, ResolveError, SourceFile, resolve_src};
pub use pipeline::check;
