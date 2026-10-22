//! Parse results and the `parse` entry point.

use rowan::{GreenNode, TextRange};

use crate::cst::{SyntaxNode, parser};
use crate::{ast, validate};

/// A syntax error found while lexing, parsing or validating.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>, range: TextRange) -> SyntaxError {
        SyntaxError {
            message: message.into(),
            range,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn range(&self) -> TextRange {
        self.range
    }
}

/// Which syntax a source file uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKind {
    /// `.kis`: the Rust-like language.
    Kis,
    /// `.kix`: `.kis` plus forms.
    Kix,
}

impl SourceKind {
    /// `kis` → `Kis`, `kix` → `Kix`.
    pub fn from_extension(extension: &str) -> Option<SourceKind> {
        match extension {
            "kis" => Some(SourceKind::Kis),
            "kix" => Some(SourceKind::Kix),
            _ => None,
        }
    }
}

/// A lossless green tree plus the syntax errors found while building and validating it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax_node(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn tree(&self) -> ast::SourceFile {
        ast::SourceFile::from_root(self.syntax_node())
    }

    /// Errors sorted by start offset.
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}

/// Lexes, parses and validates `text`. The root text of the result always equals `text`.
pub fn parse(text: &str, kind: SourceKind) -> Parse {
    let (green, mut errors) = parser::parse_source_file(text);
    let root = SyntaxNode::new_root(green.clone());
    validate::validate(&root, kind, &mut errors);
    errors.sort_by(|a, b| {
        a.range
            .start()
            .cmp(&b.range.start())
            .then_with(|| a.range.end().cmp(&b.range.end()))
            .then_with(|| a.message.cmp(&b.message))
    });
    errors.dedup();
    Parse { green, errors }
}
