//! `.kirp`/`.kix`: the Rust-like representing-code and extension language.

pub mod ast;
pub mod cst;
pub mod lex;

pub use cst::{
    RepresentKind, RepresentLanguage, SyntaxElement, SyntaxNode, SyntaxToken, parse_file,
    parse_inline,
};
