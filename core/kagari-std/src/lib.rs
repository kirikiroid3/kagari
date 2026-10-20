#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The bundled Kagari standard library: `.kix` files addressed as `std/<name>`.

/// Name of the prelude, visible in the game scope without an import.
pub const PRELUDE: &str = "prelude.kix";

/// The program scope a std file may be imported into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StdScope {
    Game,
    System,
}

/// The embedded text of the std file `std/<name>`, if it is bundled.
pub fn file(name: &str) -> Option<&'static str> {
    match name {
        "prelude.kix" => Some(include_str!("../kix/std/prelude.kix")),
        "system-ctx.kix" => Some(include_str!("../kix/std/system-ctx.kix")),
        "ui.kix" => Some(include_str!("../kix/std/ui.kix")),
        "narrative.kix" => Some(include_str!("../kix/std/narrative.kix")),
        _ => None,
    }
}

/// The scope the std file `std/<name>` belongs to.
///
/// System files are only scope-checked, never loaded, so a file can have a
/// scope without being bundled.
pub fn scope(name: &str) -> Option<StdScope> {
    match name {
        "prelude.kix" | "system-ctx.kix" => Some(StdScope::Game),
        "ui.kix" | "system-setting.kix" | "narrative.kix" => Some(StdScope::System),
        _ => None,
    }
}
