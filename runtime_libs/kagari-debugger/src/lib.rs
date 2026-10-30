#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari story debugger (`docs/dev/src/runtime/debugger.md`):
//!
//! - the **query engine** ([`Debugger`]): `stack`, `ls`, `goto`, `push`, `pop`, `detail`,
//!   `nav`, `seed`, step breakpoints and step-level execution over an engine, every move a
//!   drive of the engine's replay loop;
//! - the **Kagari debug protocol** ([`protocol`]): DAP's framing and message shapes with
//!   Kagari's payloads (step IDs, byte offsets), and its [`Server`];
//! - instruction breakpoints and stepping inside code through the VM's hook ([`hook`]);
//! - a **headless** game ([`headless`]) for the query engine without a window;
//! - with the feature `client`, what front ends share ([`client`]: reaching a runtime, source
//!   locations, the commands as text); with the feature `dap`, a debug adapter ([`dap`]).
//!
//! The crate never depends on the compiler: the story graph and the local names come from
//! the debug artifact (`game.kagari-cache`).

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "dap")]
pub mod dap;
pub mod frames;
pub mod headless;
pub mod hook;
pub mod nav;
pub mod protocol;
pub mod server;
pub mod session;
pub mod tracker;
pub mod transport;
pub mod values;

#[cfg(test)]
mod tests;

pub use frames::Artifact;
pub use server::Server;
pub use session::{Debugger, Run, Target};
pub use transport::{Connection, Link, listen};
