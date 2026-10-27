#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
//! A rustfmt-style formatter for Kagari `.kis` and `.kix` files: width-based reflow at
//! `MAX_WIDTH` columns with `INDENT`-space indentation, not configurable.

mod doc;
mod rules;
mod trivia;
mod verify;

use std::fmt;

use kagari_syntax::{SourceKind, SyntaxError};

/// Lines are reflowed to fit in this many columns where possible.
pub const MAX_WIDTH: usize = 100;
/// Spaces per indentation level.
pub const INDENT: usize = 4;

/// Why a file could not be formatted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The file has syntax errors; it is left alone.
    Syntax(Vec<SyntaxError>),
    /// The formatter produced a different program. Always a formatter bug.
    Internal(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Syntax(errors) => {
                write!(f, "cannot format: {} syntax error(s)", errors.len())
            }
            FormatError::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for FormatError {}

/// Formats a whole file. Refuses files with syntax errors. The output ends with exactly one `\n`,
/// or is empty for an empty file.
pub fn format(text: &str, kind: SourceKind) -> Result<String, FormatError> {
    let parsed = kagari_syntax::parse(text, kind);
    if !parsed.errors().is_empty() {
        return Err(FormatError::Syntax(parsed.errors().to_vec()));
    }
    let mut doc = rules::format_file(&parsed.syntax_node());
    let output = doc::print::print(&mut doc);
    verify::check(&parsed, &output, kind)?;
    Ok(output)
}
