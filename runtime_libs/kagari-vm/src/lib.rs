#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari bytecode interpreter: decoded and verified programs, values, host objects, the
//! instruction loop, story fibers with their PRNG, and traps. The format and the semantics of
//! every instruction are specified in `docs/dev/src/formats/kagari.md`.

mod exec;
mod hook;
mod host;
mod int;
mod ops;
mod program;
mod refs;
mod remap;
mod rng;
mod story;
mod trap;
mod types;
mod value;
mod vm;

#[cfg(test)]
mod tests;

pub use hook::{FrameView, HookStop, InstrHook};
pub use host::{Host, HostFn, HostObj, HostObject};
pub use kagari_bytecode::{AssetId, FnKind, FuncId, ImportId, IntTypeId, StaticId, TypeId};
pub use kagari_num::{Decimal, Rat};
pub use ops::value_eq;
pub use program::{LoadError, Program};
pub use remap::{Remap, RemapError};
pub use story::{StoryFiber, StoryStatics, Suspend};
pub use trap::{StepId, Trap, TrapKind};
pub use types::TypeTable;
pub use value::{Adt, AdtTy, Closure, Proj, Ref, RefRoot, Text, Value};
pub use vm::Vm;

/// The instruction budget of each top-level run, unless [`Vm::set_fuel_per_run`] changes it.
pub const FUEL_PER_RUN: u64 = 100_000_000;
