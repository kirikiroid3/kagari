//! The Kagari standard library, written in Kagari and embedded as source text.

#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

/// Path of the std root module, relative to the std directory.
pub const ROOT: &str = "mod.kis";

/// Source text of the std file at `path` (relative to the std directory, `/`-separated).
pub fn file(path: &str) -> Option<&'static str> {
    Some(match path {
        "mod.kis" => include_str!("../kis/std/mod.kis"),
        "prelude.kis" => include_str!("../kis/std/prelude.kis"),
        "screen.kis" => include_str!("../kis/std/screen.kis"),
        "narrative.kis" => include_str!("../kis/std/narrative.kis"),
        "narrative/text.kis" => include_str!("../kis/std/narrative/text.kis"),
        "ui.kis" => include_str!("../kis/std/ui.kis"),
        "system.kis" => include_str!("../kis/std/system.kis"),
        "system/nav.kis" => include_str!("../kis/std/system/nav.kis"),
        _ => return None,
    })
}
