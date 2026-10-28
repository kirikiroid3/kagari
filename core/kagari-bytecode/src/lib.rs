#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The `.kagari` bytecode: the in-memory module, its binary encoding, the load-time verifier
//! and a disassembler. The format is specified in `docs/dev/src/formats/kagari.md`.

mod decode;
mod disasm;
mod encode;
mod instr;
mod module;
mod verify;

#[cfg(test)]
mod tests;

pub use decode::{DecodeError, decode};
pub use disasm::disassemble;
pub use encode::encode;
pub use instr::{
    ArithOp, BoolBinOp, CmpOp, Instr, InstrError, IntBinOp, IntUnOp, Label, RandDomain, Reg,
    word_offsets,
};
pub use module::{
    AdtType, Asset, AssetId, AssetKind, ConstId, Constant, Debug, Feature, FnDebug, FnFlags,
    FnKind, FuncId, Function, Import, ImportId, IntType, IntTypeId, MAGIC, Module, NONE, Options,
    Static, StaticId, StaticKind, StrId, TypeId, Variant,
};
pub use verify::{VerifyError, VerifyErrorKind, verify, verify_code};
