//! The evaluation machine: a heap of [`Value`]s, places into it, the operations on values
//! (arithmetic, comparison, conversion to and from [`ConstValue`]) and the builtins. The
//! MIR executor (`exec`) drives it.
//!
//! Values follow `docs/reference/en/src/syntax/values.md`: every value is copied on assignment and
//! argument passing; a reference is a pointer to a place (a heap cell plus a projection
//! path), so writes through `&mut` are visible through every alias. The screen parts
//! builtins hand out (`&mut Entity`, `&mut Prop<T>`, …) are places with a [`ScreenObj`].

pub(crate) mod anim;
pub(crate) mod builtins;
pub(crate) mod exec;
pub(crate) mod screen;
pub(crate) mod story;
pub(crate) mod value;

use std::cmp::Ordering;

use crate::db::Db;
use crate::diagnostic::{Anchor, DiagnosticCode};
use crate::hir::BinaryOp;
use crate::input::Program;
use crate::mir::MirSource;
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{IntTy, Ty, TyKind};

use super::num;
use super::std_items::StdTable;
use super::value::ConstValue;
use super::{EVAL_STEP_BUDGET, Fault};
use screen::{Screen, ScreenObj};
use story::StoryVars;
use value::{ClosureValue, Value, int_range, int_ty_name};

/// Interpreted calls nested deeper than this, counted across the constants an evaluation
/// needs, are `E4003` ("recursion too deep").
pub const MAX_CALL_DEPTH: u32 = 10_000;

/// Why an evaluation did not produce a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Something already reported or outside const evaluation (`Expr::Missing`, a
    /// `Ty::Error`, a missing inference result): stop without a diagnostic.
    Abort,
    /// A render inspected its input screen (see [`Value::Probe`]).
    Inspected,
    /// A const-evaluation error.
    Fault(Fault),
}

impl From<Fault> for ErrorKind {
    fn from(fault: Fault) -> ErrorKind {
        ErrorKind::Fault(fault)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalError {
    pub kind: ErrorKind,
    /// The expression that failed, in whatever body it is (std included).
    pub at: Option<Anchor>,
}

impl EvalError {
    pub fn abort() -> EvalError {
        EvalError {
            kind: ErrorKind::Abort,
            at: None,
        }
    }
}

/// A place: a heap cell, a projection path into it, and optionally the part of the screen
/// at that path a builtin returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub cell: usize,
    pub path: Vec<Proj>,
    pub obj: Option<ScreenObj>,
}

impl Place {
    pub(crate) fn cell(cell: usize) -> Place {
        Place {
            cell,
            path: Vec::new(),
            obj: None,
        }
    }

    pub(crate) fn project(&self, proj: Proj) -> Place {
        let mut place = self.clone();
        place.path.push(proj);
        place
    }

    /// The place of the screen itself.
    pub fn without_obj(&self) -> Place {
        Place {
            cell: self.cell,
            path: self.path.clone(),
            obj: None,
        }
    }

    pub fn with_obj(&self, obj: ScreenObj) -> Place {
        Place {
            cell: self.cell,
            path: self.path.clone(),
            obj: Some(obj),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Proj {
    Field(u32),
    Index(usize),
}
pub(crate) fn overflow(value: Option<i128>, ty: &Ty) -> Fault {
    let message = match value {
        Some(value) => format!(
            "arithmetic overflow: {value} does not fit in `{}`",
            int_ty_name(ty)
        ),
        None => "arithmetic overflow".to_owned(),
    };
    Fault::new(DiagnosticCode::EvalOverflow, message)
}

pub(crate) fn fault(code: DiagnosticCode, message: impl Into<String>) -> ErrorKind {
    ErrorKind::Fault(Fault::new(code, message))
}

/// Where a builtin is called: the call's location and its result type (with the caller's
/// generic arguments substituted).
pub(crate) struct CallSite {
    pub at: Option<Anchor>,
    pub ty: Option<Ty>,
}

/// One evaluation with its own heap, step budget and call depth.
pub(crate) struct Machine<'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub std: &'db StdTable,
    pub sigs: &'db ProgramSignatures,
    pub heap: Vec<Value>,
    /// The evaluation steps taken so far.
    pub steps: u64,
    /// The nesting of interpreted calls, counted across the constants an evaluation needs.
    pub depth: u32,
    /// Story mode (stage 5): the story variables, which can then be read and written, and
    /// instant procedures called.
    pub story: Option<StoryVars<'db>>,
}

impl<'db> Machine<'db> {
    pub fn new(db: &'db dyn Db, program: Program, std: &'db StdTable) -> Machine<'db> {
        Machine {
            db,
            program,
            std,
            sigs: program_signatures(db, program),
            heap: Vec::new(),
            steps: 0,
            depth: 0,
            story: None,
        }
    }

    pub fn std(&self) -> &'db StdTable {
        self.std
    }

    pub fn alloc(&mut self, value: Value) -> usize {
        self.heap.push(value);
        self.heap.len().saturating_sub(1)
    }

    /// The value at `place` (ignoring `obj`).
    pub fn place_mut(&mut self, place: &Place) -> Result<&mut Value, ErrorKind> {
        let mut value = self.heap.get_mut(place.cell).ok_or(ErrorKind::Abort)?;
        for proj in &place.path {
            value = match (proj, value) {
                (_, Value::Probe) => return Err(ErrorKind::Inspected),
                (Proj::Field(index), Value::Tuple(fields) | Value::Adt { fields, .. }) => {
                    fields.get_mut(*index as usize).ok_or(ErrorKind::Abort)?
                }
                (Proj::Index(index), Value::Array(items) | Value::Vec(items)) => {
                    items.get_mut(*index).ok_or(ErrorKind::Abort)?
                }
                _ => return Err(ErrorKind::Abort),
            };
        }
        Ok(value)
    }

    pub fn read_place(&mut self, place: &Place) -> Result<Value, ErrorKind> {
        if place.obj.is_some() {
            return Err(ErrorKind::Abort);
        }
        let value = self.place_mut(place)?;
        if matches!(value, Value::Uninit) {
            return Err(ErrorKind::Abort);
        }
        Ok(value.clone())
    }

    pub(crate) fn write_place(&mut self, place: &Place, value: Value) -> Result<(), ErrorKind> {
        if place.obj.is_some() {
            return Err(ErrorKind::Abort);
        }
        *self.place_mut(place)? = value;
        Ok(())
    }

    /// The place a reference points to, following references stored at that place.
    pub fn ref_place(&mut self, value: &Value) -> Result<Place, ErrorKind> {
        let Value::Ref(place) = value else {
            return match value {
                Value::Probe => Err(ErrorKind::Inspected),
                _ => Err(ErrorKind::Abort),
            };
        };
        let mut place = (**place).clone();
        while place.obj.is_none() {
            match self.place_mut(&place)? {
                Value::Ref(next) => place = (**next).clone(),
                _ => break,
            }
        }
        Ok(place)
    }

    /// `value` with references followed.
    pub fn deref_value(&mut self, value: Value) -> Result<Value, ErrorKind> {
        match value {
            Value::Ref(_) => {
                let place = self.ref_place(&value)?;
                self.read_place(&place)
            }
            Value::Probe => Err(ErrorKind::Inspected),
            value => Ok(value),
        }
    }

    /// The screen at `place`.
    pub fn screen_at(&mut self, place: &Place) -> Result<&mut Screen, ErrorKind> {
        match self.place_mut(&place.without_obj())? {
            Value::Screen(screen) => Ok(std::sync::Arc::make_mut(screen)),
            Value::Probe => Err(ErrorKind::Inspected),
            _ => Err(ErrorKind::Abort),
        }
    }

    pub(crate) fn check_int(&self, value: i128, ty: &Ty) -> Result<i128, ErrorKind> {
        match int_range(ty) {
            Some((lo, hi)) if value < lo || value > hi => Err(overflow(Some(value), ty).into()),
            _ => Ok(value),
        }
    }

    /// A binary operator on evaluated operands; `ty` is the result type (for overflow
    /// checks), `lhs_ty` the left operand's (for shifts).
    pub(crate) fn binop(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
        ty: &Ty,
        lhs_ty: &Ty,
    ) -> Result<Value, ErrorKind> {
        match op {
            BinaryOp::Eq => return self.value_eq(&left, &right).map(Value::Bool),
            BinaryOp::Ne => return self.value_eq(&left, &right).map(|eq| Value::Bool(!eq)),
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let ordering = self.value_cmp(left, right)?;
                return Ok(Value::Bool(match op {
                    BinaryOp::Lt => ordering == Ordering::Less,
                    BinaryOp::Le => ordering != Ordering::Greater,
                    BinaryOp::Gt => ordering == Ordering::Greater,
                    _ => ordering != Ordering::Less,
                }));
            }
            _ => {}
        }
        let left = self.deref_value(left)?;
        let right = self.deref_value(right)?;
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                let int_of = |ty: &Ty| match ty.kind() {
                    TyKind::Int(int) => *int,
                    _ => IntTy::I128,
                };
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div | BinaryOp::Rem => {
                        if b == 0 {
                            return Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            ));
                        }
                        if op == BinaryOp::Div {
                            a.checked_div(b)
                        } else {
                            a.checked_rem(b)
                        }
                    }
                    BinaryOp::BitAnd => Some(a & b),
                    BinaryOp::BitOr => Some(a | b),
                    BinaryOp::BitXor => Some(a ^ b),
                    BinaryOp::Shl => num::shl(a, b, int_of(lhs_ty)),
                    BinaryOp::Shr => num::shr(a, b, int_of(lhs_ty)),
                    _ => return Err(ErrorKind::Abort),
                };
                match result {
                    Some(value) => self.check_int(value, ty).map(Value::Int),
                    None => Err(overflow(None, ty).into()),
                }
            }
            (Value::Bool(a), Value::Bool(b)) => match op {
                BinaryOp::BitAnd => Ok(Value::Bool(a & b)),
                BinaryOp::BitOr => Ok(Value::Bool(a | b)),
                BinaryOp::BitXor => Ok(Value::Bool(a ^ b)),
                _ => Err(ErrorKind::Abort),
            },
            (Value::Decimal(a), Value::Decimal(b)) => {
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div => {
                        return match a.checked_div(b) {
                            Ok(value) => Ok(Value::Decimal(value)),
                            Err(true) => Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            )),
                            Err(false) => Err(fault(
                                DiagnosticCode::EvalOverflow,
                                format!("{a} / {b} overflows `Decimal`"),
                            )),
                        };
                    }
                    _ => return Err(ErrorKind::Abort),
                };
                result
                    .map(Value::Decimal)
                    .ok_or_else(|| overflow(None, ty).into())
            }
            (Value::Rat(a), Value::Rat(b)) => {
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div => {
                        if b.is_zero() {
                            return Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            ));
                        }
                        a.checked_div(b)
                    }
                    _ => return Err(ErrorKind::Abort),
                };
                result
                    .map(Value::Rat)
                    .ok_or_else(|| overflow(None, ty).into())
            }
            (Value::Probe, _) | (_, Value::Probe) => Err(ErrorKind::Inspected),
            _ => Err(ErrorKind::Abort),
        }
    }

    /// Structural equality; references compare the values they point to.
    pub(crate) fn value_eq(&mut self, left: &Value, right: &Value) -> Result<bool, ErrorKind> {
        let left = self.deref_value(left.clone())?;
        let right = self.deref_value(right.clone())?;
        Ok(match (&left, &right) {
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Decimal(a), Value::Decimal(b)) => a == b,
            (Value::Rat(a), Value::Rat(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Asset { kind: k1, path: p1 }, Value::Asset { kind: k2, path: p2 }) => {
                k1 == k2 && p1 == p2
            }
            (Value::Fn(a), Value::Fn(b)) => a == b,
            (Value::Handle { item: a, .. }, Value::Handle { item: b, .. }) => a == b,
            (Value::Screen(a), Value::Screen(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b))
            | (Value::Array(a), Value::Array(b))
            | (Value::Vec(a), Value::Vec(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !self.value_eq(a, b)? {
                        return Ok(false);
                    }
                }
                true
            }
            (
                Value::Adt {
                    item: i1,
                    variant: v1,
                    fields: f1,
                },
                Value::Adt {
                    item: i2,
                    variant: v2,
                    fields: f2,
                },
            ) => {
                if i1 != i2 || v1 != v2 || f1.len() != f2.len() {
                    return Ok(false);
                }
                for (a, b) in f1.iter().zip(f2) {
                    if !self.value_eq(a, b)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Closure(_) | Value::Pending(_) | Value::Uninit, _)
            | (_, Value::Closure(_) | Value::Pending(_) | Value::Uninit) => {
                return Err(ErrorKind::Abort);
            }
            _ => false,
        })
    }

    pub(crate) fn value_cmp(&mut self, left: Value, right: Value) -> Result<Ordering, ErrorKind> {
        let left = self.deref_value(left)?;
        let right = self.deref_value(right)?;
        let ordering = match (left, right) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(&b)),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(&b)),
            (Value::Decimal(a), Value::Decimal(b)) => a.checked_cmp(b),
            (Value::Rat(a), Value::Rat(b)) => a.checked_cmp(b),
            (Value::Probe, _) | (_, Value::Probe) => return Err(ErrorKind::Inspected),
            _ => return Err(ErrorKind::Abort),
        };
        ordering.ok_or_else(|| overflow(None, &Ty::error()).into())
    }

    /// `value` as plain data. References and pending changes cannot be constants;
    /// the probe screen reports that the render inspected it.
    pub(crate) fn freeze(&mut self, value: &Value) -> Result<ConstValue, ErrorKind> {
        let list = |machine: &mut Self, items: &imbl::Vector<Value>| {
            items
                .iter()
                .map(|item| machine.freeze(item))
                .collect::<Result<imbl::Vector<_>, _>>()
        };
        Ok(match value {
            Value::Bool(value) => ConstValue::Bool(*value),
            Value::Int(value) => ConstValue::Int(*value),
            Value::Decimal(value) => ConstValue::Decimal(*value),
            Value::Rat(value) => ConstValue::Rat(*value),
            Value::Text(text) => ConstValue::Text(text.clone()),
            Value::String(text) => ConstValue::String(text.clone()),
            Value::Tuple(items) => ConstValue::Tuple(list(self, items)?),
            Value::Array(items) => ConstValue::Array(list(self, items)?),
            Value::Vec(items) => ConstValue::Vec(list(self, items)?),
            Value::Adt {
                item,
                variant,
                fields,
            } => ConstValue::Adt {
                item: *item,
                variant: *variant,
                fields: list(self, fields)?,
            },
            Value::Asset { kind, path } => ConstValue::Asset {
                kind: *kind,
                path: path.clone(),
            },
            Value::Fn(fn_ref) => ConstValue::Fn(fn_ref.clone()),
            Value::Closure(closure) if closure.captures.is_empty() => match closure.source {
                MirSource::Closure { owner, expr } => ConstValue::Closure { owner, expr },
                MirSource::Item(_) | MirSource::Promoted { .. } => return Err(ErrorKind::Abort),
            },
            Value::Handle { kind, item } => ConstValue::Handle {
                kind: *kind,
                item: *item,
            },
            Value::NewHandle { .. } => {
                return Err(fault(
                    DiagnosticCode::EvalHandleOutsideStatic,
                    "a `Setting` or `Meta` handle can only be the initializer of a `static`",
                ));
            }
            Value::Screen(screen) => ConstValue::Screen(screen.clone()),
            Value::Probe => return Err(ErrorKind::Inspected),
            Value::Closure(_) | Value::Ref(_) | Value::Pending(_) | Value::Uninit => {
                return Err(ErrorKind::Abort);
            }
        })
    }

    pub(crate) fn thaw(&self, value: &ConstValue) -> Value {
        let list =
            |items: &imbl::Vector<ConstValue>| items.iter().map(|item| self.thaw(item)).collect();
        match value {
            ConstValue::Bool(value) => Value::Bool(*value),
            ConstValue::Int(value) => Value::Int(*value),
            ConstValue::Decimal(value) => Value::Decimal(*value),
            ConstValue::Rat(value) => Value::Rat(*value),
            ConstValue::Text(text) => Value::Text(text.clone()),
            ConstValue::String(text) => Value::String(text.clone()),
            ConstValue::Tuple(items) => Value::Tuple(list(items)),
            ConstValue::Array(items) => Value::Array(list(items)),
            ConstValue::Vec(items) => Value::Vec(list(items)),
            ConstValue::Adt {
                item,
                variant,
                fields,
            } => Value::Adt {
                item: *item,
                variant: *variant,
                fields: list(fields),
            },
            ConstValue::Asset { kind, path } => Value::Asset {
                kind: *kind,
                path: path.clone(),
            },
            ConstValue::Fn(fn_ref) => Value::Fn(fn_ref.clone()),
            ConstValue::Closure { owner, expr } => {
                Value::Closure(Box::new(ClosureValue::plain(*owner, *expr)))
            }
            ConstValue::Handle { kind, item } => Value::Handle {
                kind: *kind,
                item: *item,
            },
            ConstValue::Screen(screen) => Value::Screen(screen.clone()),
        }
    }
}

pub(crate) fn budget_exceeded() -> ErrorKind {
    fault(
        DiagnosticCode::EvalBudgetExceeded,
        format!("constant evaluation exceeded its budget of {EVAL_STEP_BUDGET} steps"),
    )
}

pub(crate) fn recursion_too_deep() -> ErrorKind {
    fault(
        DiagnosticCode::EvalBudgetExceeded,
        format!(
            "recursion too deep: more than {MAX_CALL_DEPTH} nested calls during constant evaluation"
        ),
    )
}
