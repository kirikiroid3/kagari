//! Editor queries over a program: go to definition, hover and the symbols of a file.
//!
//! A position is mapped back to the path, binding or method call under it through the
//! HIR source maps (see [`lookup`]). Field accesses and `use` paths are not navigable.

mod definition;
mod header;
mod hover;
mod lookup;
mod symbols;

use kagari_syntax::TextRange;

use crate::input::FilePath;

pub use definition::definition;
pub use hover::{Hover, hover};
pub use symbols::{FileSymbol, SymbolKind, file_symbols};

/// Where a definition is: its file and the range of its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavTarget {
    pub file: FilePath,
    /// The name to select; `0..0` for a module file.
    pub range: TextRange,
}

#[cfg(test)]
mod tests;
