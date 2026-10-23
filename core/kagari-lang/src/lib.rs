#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari front end: a salsa database running compilation stages 1 (parse, item trees,
//! HIR lowering with forms desugared), 2 (module trees and name resolution), 3 (type and
//! kind checking) and 4 (const evaluation).

mod db;
mod diagnostic;
mod input;
mod module_files;
mod parse_query;
mod pipeline;

pub mod debug;
pub mod def_map;
pub mod eval;
pub mod hir;
pub mod infer;
pub mod item_tree;
pub mod resolve;
pub mod signatures;
pub mod ty;
pub mod typeck;

pub use db::{Db, FileSystem, MemoryFileSystem, OsFileSystem, RootDatabase};
pub use diagnostic::{Diagnostic, DiagnosticCode, Related, Severity, Span, render_diagnostics};
pub use input::{AssetFile, FilePath, Program, SourceFile};
pub use parse_query::parse;
pub use pipeline::check;
