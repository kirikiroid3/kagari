#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! Runtime profiles and the asset formats each can decode: `kagari build` sniffs every
//! referenced asset by content and rejects formats a listed profile does not support.

mod format;
mod profile;
mod sniff;

#[cfg(test)]
mod tests;

pub use format::{AssetFormat, describe, supports, supports_kind};
pub use profile::RuntimeProfile;
pub use sniff::sniff;
