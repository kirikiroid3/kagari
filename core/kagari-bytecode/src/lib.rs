#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

pub mod assets;
pub mod narrative;
pub mod save;
pub mod view;
pub mod window;
