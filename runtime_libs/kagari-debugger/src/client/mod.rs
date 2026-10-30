//! What the debugger front ends share (feature `client`): reaching a runtime ([`backend`]),
//! source locations and line breakpoints ([`locate`]), the query engine's commands as text
//! ([`text`]) and watching the sources ([`watch`]). The front ends are the `kagari debug` REPL
//! and the DAP adapter ([`crate::dap`]).

pub mod backend;
pub mod locate;
pub mod text;
pub mod watch;

pub use backend::{Attached, Backend};
pub use locate::{LineBreakpoints, Location, Locator};
pub use watch::Watcher;
