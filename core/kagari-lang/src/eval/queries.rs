//! The per-item const-evaluation queries: the value of a `const` item, the initializer of
//! a `static` and the value of a promoted constant, each computed by running its MIR.

use crate::db::Db;
use crate::diagnostic::DiagnosticCode;
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::mir::{Body, item_mir};
use crate::signatures::program_signatures;
use crate::thir::PromotedId;
use crate::ty::GenericArgs;

use super::machine::value::{GenericEnv, Value};
use super::machine::{EvalError, Machine, fault};
use super::std_items::std_table;
use super::value::{ConstValue, HandleKind, item_name};

/// The evaluated initializer of a `static`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticValue {
    /// `Some` for a `Setting`/`Meta` handle; `value` is then its default value.
    pub handle: Option<HandleKind>,
    pub value: ConstValue,
}

/// The value of a `const` item with its parent impl's generic arguments.
#[salsa::tracked(returns(ref), cycle_result = const_cycle)]
pub fn const_value(
    db: &dyn Db,
    program: Program,
    item: ItemId,
    args: GenericArgs,
) -> Result<ConstValue, EvalError> {
    let (mut machine, value) = run_initializer(db, program, item, args, |mir| Some(&mir.main))?;
    machine.freeze_result(&value)
}

fn const_cycle(
    db: &dyn Db,
    _id: salsa::Id,
    _program: Program,
    item: ItemId,
    _args: GenericArgs,
) -> Result<ConstValue, EvalError> {
    Err(cycle_error(db, item))
}

/// The evaluated initializer of a `static`.
#[salsa::tracked(returns(ref), cycle_result = static_cycle)]
pub(crate) fn static_value(
    db: &dyn Db,
    program: Program,
    item: ItemId,
) -> Result<StaticValue, EvalError> {
    let (mut machine, value) = run_initializer(db, program, item, GenericArgs::empty(), |mir| {
        Some(&mir.main)
    })?;
    let (handle, value) = match value {
        Value::NewHandle { kind, init } => (Some(kind), *init),
        value => (None, value),
    };
    let value = machine.freeze_result(&value)?;
    Ok(StaticValue { handle, value })
}

fn static_cycle(
    db: &dyn Db,
    _id: salsa::Id,
    _program: Program,
    item: ItemId,
) -> Result<StaticValue, EvalError> {
    Err(cycle_error(db, item))
}

/// The value of promoted constant `index` of `item`'s body.
#[salsa::tracked(returns(ref))]
pub fn promoted_value(
    db: &dyn Db,
    program: Program,
    item: ItemId,
    index: PromotedId,
) -> Result<ConstValue, EvalError> {
    let (mut machine, value) = run_initializer(db, program, item, GenericArgs::empty(), |mir| {
        mir.promoted(index)
    })?;
    machine.freeze_result(&value)
}

/// Runs a body of `item`'s MIR with a fresh machine.
fn run_initializer<'db>(
    db: &'db dyn Db,
    program: Program,
    item: ItemId,
    args: GenericArgs,
    body: impl FnOnce(&'db crate::mir::ItemMir) -> Option<&'db Body>,
) -> Result<(Machine<'db>, Value), EvalError> {
    let Some(std) = std_table(db, program).as_ref() else {
        return Err(EvalError::abort());
    };
    let Some(body) = item_mir(db, program, item).as_ref().and_then(body) else {
        return Err(EvalError::abort());
    };
    let env = GenericEnv::for_item(program_signatures(db, program), item, args);
    let mut machine = Machine::new(db, program, std);
    let value = machine.run_body(body, env, Vec::new())?;
    Ok((machine, value))
}

/// The error of a constant or static whose value depends on itself: it cannot be computed
/// at compile time.
pub(crate) fn cycle_error(db: &dyn Db, item: ItemId) -> EvalError {
    EvalError {
        kind: fault(
            DiagnosticCode::EvalConstCycle,
            format!(
                "the value of `{}` depends on itself, so it cannot be computed at compile time",
                item_name(db, item)
            ),
        ),
        at: None,
    }
}
