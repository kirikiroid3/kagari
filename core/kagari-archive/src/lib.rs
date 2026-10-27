#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! `.kiarchive` asset archives and `.kisave` save databases.

pub mod archive;
mod le;
pub mod save;
