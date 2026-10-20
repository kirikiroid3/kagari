//! `.kis` game scripts: a strict XML subset parsed into a lossless CST plus a typed AST.

pub mod ast;
pub mod cst;
pub mod lex;

pub use cst::{ScriptKind, ScriptLanguage, SyntaxElement, SyntaxNode, SyntaxToken, parse};
