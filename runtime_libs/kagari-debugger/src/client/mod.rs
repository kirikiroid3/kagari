//! What the debugger front ends share (feature `client`): reaching a runtime ([`backend`]),
//! source locations and line breakpoints ([`locate`]), and the query engine's commands as text
//! ([`text`]). The front ends are the `kagari debug` REPL and the DAP adapter ([`crate::dap`]).

pub mod backend;
pub mod locate;
pub mod text;

pub use backend::{Attached, Backend};
pub use locate::{LineBreakpoints, Location, Locator};
