#![deny(clippy::unwrap_used, clippy::panic, clippy::expect_used, clippy::arithmetic_side_effects)]

//! Kagari projects: the `kagari.toml` manifest and the program it describes.

mod error;
mod fonts;
mod manifest;
mod project;
mod target;

#[cfg(test)]
mod tests;

pub use error::ProjectError;
pub use fonts::system_font_dirs;
pub use manifest::{Manifest, parse_manifest};
pub use project::Project;
pub use target::Target;

/// The manifest's file name.
pub const MANIFEST_FILE: &str = "kagari.toml";
/// The version of this toolchain, checked against `toolchain.kagari`.
pub const TOOLCHAIN_VERSION: &str = env!("CARGO_PKG_VERSION");
