#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
//! Lossless syntax trees for Kagari `.kis` and `.kix` files.

pub mod ast;
pub mod cst;
pub mod lex;
mod parse;
mod validate;

pub use cst::{KagariLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};
pub use parse::{Parse, SourceKind, SyntaxError, parse};
pub use rowan::{TextRange, TextSize};
