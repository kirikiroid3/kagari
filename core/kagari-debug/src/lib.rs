#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! `.kagari-cache` files: the explored story graph and debugging information, stored in the
//! `.kisave` container so that tools and the runtime read them without the compiler.
//!
//! Two kinds share the format (see `docs/dev/src/formats/cache.md`):
//!
//! - [`AnalysisCache`] (`<out-dir>/analysis.kagari-cache`): the result of the story analysis
//!   keyed by the program's source fingerprint, so that `kagari check` and `kagari build`
//!   reuse it while the sources are unchanged.
//! - [`DebugArtifact`] (`<out-dir>/<target>/game.kagari-cache`, written by a debug build):
//!   the story graph and the debug information of the `game.kagari` whose script hash it
//!   records.
//!
//! The crate depends only on `kagari-archive`; runtime crates may use it.

mod codec;
mod debug_info;
mod file;
mod graph;
mod value;

#[cfg(test)]
mod tests;

pub use debug_info::{DebugInfo, FunctionDebug, LocalDebug, RegisterRange, ScopeDebug};
pub use file::{
    AnalysisCache, DebugArtifact, DiagnosticRecord, FileKind, RelatedRecord, Severity, SourceRef,
};
pub use graph::{
    Caller, Context, Exit, Incomplete, Node, NodeKind, Point, Scene, StoryGraph, Target,
};
pub use value::{Text, Value};

use std::fmt;

/// The extension of every cache file, without the dot.
pub const EXTENSION: &str = "kagari-cache";
/// The analysis cache in the output directory.
pub const ANALYSIS_FILE: &str = "analysis.kagari-cache";
/// The debug artifact next to `game.kagari` in a target directory.
pub const DEBUG_FILE: &str = "game.kagari-cache";
/// The schema version of the content; a reader rejects any other.
pub const SCHEMA: i64 = 2;

/// Why a cache file cannot be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The `.kisave` container is invalid or an edit failed.
    Save(kagari_archive::save::Error),
    /// The file holds another kind of content.
    Kind { expected: FileKind, found: String },
    /// The content has another schema version.
    Schema(i64),
    /// A field is missing or has the wrong type or value; names the field.
    Malformed(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Save(error) => write!(f, "{error}"),
            Error::Kind { expected, found } => write!(
                f,
                "expected a `{}` cache file, found `{found}`",
                expected.as_str()
            ),
            Error::Schema(found) => {
                write!(f, "unsupported cache schema {found} (expected {SCHEMA})")
            }
            Error::Malformed(field) => write!(f, "the field `{field}` is missing or malformed"),
        }
    }
}

impl std::error::Error for Error {}

impl From<kagari_archive::save::Error> for Error {
    fn from(error: kagari_archive::save::Error) -> Error {
        Error::Save(error)
    }
}
