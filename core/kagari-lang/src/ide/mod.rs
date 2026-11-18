//! Editor queries over a program: go to definition, hover, the symbols of a file, rename and
//! completion.
//!
//! A position is mapped back to what is written there (a path or path segment, a binding, a
//! method call, a field, a `use` path or a definition's name) through the HIR source maps, the
//! resolution and inference results and the syntax tree (see [`lookup`]).

mod completion;
mod definition;
mod header;
mod hover;
mod lookup;
mod rename;
mod symbols;

use kagari_syntax::TextRange;

use crate::input::FilePath;

pub use completion::{CompletionItem, CompletionKind, Completions, completions};
pub use definition::definition;
pub use hover::{Hover, hover};
pub use rename::{RenameError, prepare_rename, rename};
pub use symbols::{FileSymbol, SymbolKind, file_symbols};

/// Where a definition is: its file and the range of its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavTarget {
    pub file: FilePath,
    /// The name to select; `0..0` for a module file.
    pub range: TextRange,
}

/// A replacement of `range` by `new_text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub range: TextRange,
    pub new_text: String,
}

/// The edits of one file, in text order, not overlapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEdits {
    pub file: FilePath,
    pub edits: Vec<TextEdit>,
}

#[cfg(test)]
mod tests;
