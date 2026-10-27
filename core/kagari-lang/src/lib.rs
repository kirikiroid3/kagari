#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari front end: a salsa database running compilation stages 1 (parse, item trees,
//! HIR lowering with forms desugared), 2 (module trees and name resolution), 3 (type and
//! kind checking), 4 (const evaluation) and 5 (whole-program analysis), then emitting
//! bytecode.
//!
//! Bodies go HIR + inference → [`thir`] (a typed tree) → [`mir`] (a control-flow graph).
//! Const evaluation and the story exploration run a MIR interpreter, the analyses read MIR,
//! and codegen emits bytecode from MIR, so the language's semantics are defined once.
//!
//! Queries are per item and their results hold no text ranges: diagnostics carry
//! [`Anchor`]s, resolved to ranges through the source maps only in [`check`]. Editing text
//! inside one item re-runs the queries of that item and of the code that uses it.

mod callee;
mod db;
mod diagnostic;
mod input;
mod intern;
mod module_files;
mod parse_query;
mod pipeline;

pub mod analysis;
pub mod debug;
pub mod def_map;
pub mod emit;
pub mod eval;
pub mod hir;
pub mod infer;
pub mod item_tree;
pub mod mir;
pub mod paths;
pub mod resolve;
pub mod signatures;
pub mod thir;
pub mod ty;
pub mod typeck;

/// When less native stack than this is left, a recursive tree walker continues on a new
/// segment (`stacker::maybe_grow`).
pub(crate) const STACK_RED_ZONE: usize = 100 * 1024;
/// The size of each new stack segment.
pub(crate) const STACK_GROWTH: usize = 1024 * 1024;

pub use db::{Db, FileSystem, MemoryFileSystem, OsFileSystem, RootDatabase};
pub use diagnostic::{
    Anchor, Diag, Diagnostic, DiagnosticCode, NodeId, Owner, Related, Severity, render_diagnostics,
};
pub use emit::{Build, build};
pub use input::{
    AssetFile, CompileOptions, DEFAULT_RECURSION_DEPTH, DEFAULT_STATE_CAP, FilePath, Program,
    SourceFile,
};
pub use parse_query::parse;
pub use pipeline::check;
