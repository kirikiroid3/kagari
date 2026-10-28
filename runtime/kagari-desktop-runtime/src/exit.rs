//! Exit codes, and the failure that carries one to `main`.

/// Usage errors (clap), a bad game directory, a boot error.
pub const USAGE: u8 = 2;
/// The game stopped with a trap, during boot or later.
pub const STOPPED: u8 = 3;
/// A windowing or GPU failure.
pub const WINDOW: u8 = 4;
/// A headless script error.
pub const SCRIPT: u8 = 5;

/// Why the runtime stops with a nonzero status. `main` prints
/// `kagari-desktop-runtime: {message}` to stderr and exits with `code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: u8,
    pub message: String,
}

impl Failure {
    pub fn new(code: u8, message: impl Into<String>) -> Failure {
        Failure {
            code,
            message: message.into(),
        }
    }

    /// Exit 3: `game stopped: {message}`.
    pub fn stopped(message: &str) -> Failure {
        Failure::new(STOPPED, format!("game stopped: {message}"))
    }
}
