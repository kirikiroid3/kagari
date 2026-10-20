#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

mod parse;
pub mod represent;
pub mod script;

pub use parse::{Parse, SyntaxError};
pub use rowan::{TextRange, TextSize};
