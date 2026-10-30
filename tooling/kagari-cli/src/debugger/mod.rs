//! What the debugger front ends (`kagari debug`, `kagari dap`) share: reaching a runtime
//! ([`backend`]), source locations and line breakpoints ([`locate`]), and the query engine's
//! commands as text ([`text`]). The runtime side is `kagari-debugger`.

pub mod backend;
pub mod locate;
pub mod text;
