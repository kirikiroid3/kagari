//! Data instructions: constants, arithmetic, aggregates, references, Vec and text. Each one
//! writes its destination register and lets the loop advance `pc`.

use std::rc::Rc;

use kagari_bytecode::Instr;

use super::{Frame, int_operands, raw_range, reg, set, val};
use crate::int;
use crate::ops;
use crate::program::{Program, new_cell};
use crate::refs;
use crate::trap::{Trap, TrapKind, fault, mismatch};
use crate::value::{Adt, AdtTy, Closure, Proj, Ref, RefRoot, Text, Value};

/// An index operand as `usize`; negative or too large → `IndexOutOfRange`.
fn index(v: &Value) -> Result<usize, Trap> {
    let i = int::int(v)?;
    u64::try_from(i)
        .ok()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or(Trap::new(TrapKind::IndexOutOfRange))
}

/// The raw register `r`, which must hold a reference.
fn raw_ref(regs: &[Value], r: u16) -> Result<&Ref, Trap> {
    match reg(regs, r)? {
        Value::Ref(r) => Ok(r),
        other => Err(mismatch("Ref", other.tag())),
    }
}

/// The items of an Array (or Vec) of Text.
fn texts(v: &Value) -> Result<Vec<Text>, Trap> {
    match v {
        Value::Array(items) | Value::Vec(items) => items.iter().map(ops::text).collect(),
        other => Err(mismatch("Array", other.tag())),
    }
}

/// A reference extended by one projection, after checking that the projection exists.
fn project_ref(r: &Ref, proj: Proj) -> Result<Value, Trap> {
    let (cell, path) = refs::resolve(r)?;
    refs::with_target(&cell, &path, |target| {
        refs::project(target, proj).map(|_| ())
    })?;
    let mut extended = Vec::with_capacity(path.len().saturating_add(1));
    extended.extend(path.iter().copied());
    extended.push(proj);
    Ok(Value::Ref(Ref {
        root: RefRoot::Cell(cell),
        path: Rc::new(extended),
    }))
}

pub(super) fn exec(
    instr: Instr,
    frame: &mut Frame,
    program: &Program,
    fuel: &mut u64,
) -> Result<(), Trap> {
    let regs = &mut frame.regs;
    match instr {
        Instr::Move { dst, src } => {
            let v = reg(regs, src)?.clone();
            set(regs, dst, v)
        }
        Instr::Const { dst, value } => {
            let v = program.constant(value).ok_or_else(fault)?.clone();
            set(regs, dst, v)
        }
        Instr::Bool { dst, value } => set(regs, dst, Value::Bool(value)),
        Instr::Int { dst, value } => set(regs, dst, Value::Int(i128::from(value))),
        Instr::Func { dst, func } => set(regs, dst, Value::Func(func)),
        Instr::Clear { reg: r } => set(regs, r, Value::Uninit),
        Instr::IntBin {
            op,
            dst,
            lhs,
            rhs,
            ty,
        } => {
            let t = program
                .module()
                .int_types
                .get(ty.0 as usize)
                .ok_or_else(fault)?;
            let (a, b) = int_operands(regs, lhs, rhs)?;
            set(regs, dst, Value::Int(int::binary(op, a, b, t)?))
        }
        Instr::IntUn { op, dst, src, ty } => {
            let t = program
                .module()
                .int_types
                .get(ty.0 as usize)
                .ok_or_else(fault)?;
            let a = int::int(&*val(regs, src)?)?;
            set(regs, dst, Value::Int(int::unary(op, a, t)?))
        }
        Instr::Not { dst, src } => {
            let b = ops::boolean(&*val(regs, src)?)?;
            set(regs, dst, Value::Bool(!b))
        }
        Instr::BoolBin { op, dst, lhs, rhs } => {
            let a = ops::boolean(&*val(regs, lhs)?)?;
            let b = ops::boolean(&*val(regs, rhs)?)?;
            let r = match op {
                kagari_bytecode::BoolBinOp::And => a & b,
                kagari_bytecode::BoolBinOp::Or => a | b,
                kagari_bytecode::BoolBinOp::Xor => a ^ b,
            };
            set(regs, dst, Value::Bool(r))
        }
        Instr::DecBin { op, dst, lhs, rhs } => {
            let a = ops::decimal(&*val(regs, lhs)?)?;
            let b = ops::decimal(&*val(regs, rhs)?)?;
            set(regs, dst, Value::Decimal(ops::decimal_op(op, a, b)?))
        }
        Instr::DecNeg { dst, src } => {
            let a = ops::decimal(&*val(regs, src)?)?;
            let r = a.checked_neg().ok_or(Trap::new(TrapKind::Overflow))?;
            set(regs, dst, Value::Decimal(r))
        }
        Instr::RatBin { op, dst, lhs, rhs } => {
            let a = ops::rat(&*val(regs, lhs)?)?;
            let b = ops::rat(&*val(regs, rhs)?)?;
            set(regs, dst, Value::Rat(ops::rat_op(op, a, b)?))
        }
        Instr::RatNeg { dst, src } => {
            let a = ops::rat(&*val(regs, src)?)?;
            let r = a.checked_neg().ok_or(Trap::new(TrapKind::Overflow))?;
            set(regs, dst, Value::Rat(r))
        }
        Instr::RatNew { dst, num, den } => {
            let (n, d) = int_operands(regs, num, den)?;
            set(regs, dst, Value::Rat(ops::rat_new(n, d)?))
        }
        Instr::RatDec { dst, src } => {
            let a = ops::rat(&*val(regs, src)?)?;
            let r = a.to_decimal().ok_or(Trap::new(TrapKind::Overflow))?;
            set(regs, dst, Value::Decimal(r))
        }
        Instr::Cmp { op, dst, lhs, rhs } => {
            let r = ops::compare(op, &*val(regs, lhs)?, &*val(regs, rhs)?)?;
            set(regs, dst, Value::Bool(r))
        }
        Instr::Closure {
            dst,
            captures,
            count,
            func,
        } => {
            let captures = raw_range(regs, captures, count)?;
            set(
                regs,
                dst,
                Value::Closure(Rc::new(Closure { func, captures })),
            )
        }
        Instr::Tuple { dst, first, count } => {
            let v = if count == 0 {
                program.unit.clone()
            } else {
                Value::Tuple(Rc::new(raw_range(regs, first, count)?))
            };
            set(regs, dst, v)
        }
        Instr::Array { dst, first, count } => {
            let v = if count == 0 {
                program.empty_array.clone()
            } else {
                Value::Array(Rc::new(raw_range(regs, first, count)?))
            };
            set(regs, dst, v)
        }
        Instr::Repeat { dst, value, count } => {
            *fuel = fuel
                .checked_sub(count)
                .ok_or(Trap::new(TrapKind::FuelExhausted))?;
            let n = usize::try_from(count).map_err(|_| Trap::new(TrapKind::FuelExhausted))?;
            let v = reg(regs, value)?.clone();
            set(regs, dst, Value::Array(Rc::new(vec![v; n])))
        }
        Instr::Adt {
            dst,
            first,
            count,
            ty,
            variant,
        } => {
            let fields = raw_range(regs, first, count)?;
            let v = Value::Adt(Rc::new(Adt {
                ty: AdtTy::Module(ty),
                variant,
                fields,
            }));
            set(regs, dst, v)
        }
        Instr::Field { dst, src, index } => {
            let source = val(regs, src)?;
            let field = refs::project(&source, Proj::Field(index))?.clone();
            drop(source);
            set(regs, dst, field)
        }
        Instr::IsVariant {
            dst,
            src,
            ty,
            variant,
        } => {
            let source = val(regs, src)?;
            let r = match &*source {
                Value::Adt(adt) if adt.ty == AdtTy::Module(ty) => adt.variant == variant,
                Value::Adt(_) => {
                    return Err(mismatch(
                        "variant of the tested type",
                        "Adt of another type",
                    ));
                }
                other => return Err(mismatch("Adt", other.tag())),
            };
            drop(source);
            set(regs, dst, Value::Bool(r))
        }
        Instr::Index {
            dst,
            base,
            index: i,
        } => {
            let i = index(&*val(regs, i)?)?;
            let source = val(regs, base)?;
            let item = match &*source {
                Value::Array(items) | Value::Vec(items) => items
                    .get(i)
                    .cloned()
                    .ok_or(Trap::new(TrapKind::IndexOutOfRange))?,
                other => return Err(mismatch("Array or Vec", other.tag())),
            };
            drop(source);
            set(regs, dst, item)
        }
        Instr::Len { dst, src } => {
            let len = match &*val(regs, src)? {
                Value::Array(items) | Value::Vec(items) => items.len(),
                other => return Err(mismatch("Array or Vec", other.tag())),
            };
            let len = i128::try_from(len).map_err(|_| fault())?;
            set(regs, dst, Value::Int(len))
        }
        Instr::Cell { dst, init } => {
            let init = reg(regs, init)?.clone();
            let r = Ref {
                root: RefRoot::Cell(new_cell(init)),
                path: program.empty_path.clone(),
            };
            set(regs, dst, Value::Ref(r))
        }
        Instr::RefField { dst, base, index } => {
            let r = project_ref(raw_ref(regs, base)?, Proj::Field(index))?;
            set(regs, dst, r)
        }
        Instr::RefIndex {
            dst,
            base,
            index: i,
        } => {
            let i = index(&*val(regs, i)?)?;
            let i = u64::try_from(i).map_err(|_| Trap::new(TrapKind::IndexOutOfRange))?;
            let r = project_ref(raw_ref(regs, base)?, Proj::Index(i))?;
            set(regs, dst, r)
        }
        Instr::Load { dst, src } => {
            let v = match reg(regs, src)? {
                Value::Ref(r) => refs::read(r)?,
                v => v.clone(),
            };
            set(regs, dst, v)
        }
        Instr::Store { dst, value } => {
            let v = reg(regs, value)?.clone();
            refs::write(raw_ref(regs, dst)?, v)
        }
        Instr::VecNew { dst } => set(regs, dst, program.empty_vec.clone()),
        Instr::VecGet {
            dst,
            vec,
            index: i,
            option,
        } => {
            let opt = program
                .option
                .as_ref()
                .filter(|o| o.ty == option)
                .ok_or(mismatch("std::Option", "another type"))?;
            let i = int::int(&*val(regs, i)?)?;
            let source = val(regs, vec)?;
            let items = match &*source {
                Value::Vec(items) => items,
                other => return Err(mismatch("Vec", other.tag())),
            };
            let item = usize::try_from(i).ok().and_then(|i| items.get(i)).cloned();
            drop(source);
            let v = match item {
                Some(item) => Value::Adt(Rc::new(Adt {
                    ty: AdtTy::Module(opt.ty),
                    variant: opt.some,
                    fields: vec![item],
                })),
                None => opt.none.clone(),
            };
            set(regs, dst, v)
        }
        Instr::VecSet {
            vec,
            index: i,
            value,
        } => {
            let i = index(&*val(regs, i)?)?;
            let v = reg(regs, value)?.clone();
            refs::with_mut(raw_ref(regs, vec)?, |slot| match slot {
                Value::Vec(items) => match Rc::make_mut(items).get_mut(i) {
                    Some(item) => {
                        *item = v;
                        Ok(())
                    }
                    None => Err(Trap::new(TrapKind::IndexOutOfRange)),
                },
                other => Err(mismatch("Vec", other.tag())),
            })?
        }
        Instr::VecPush { vec, value } => {
            let v = reg(regs, value)?.clone();
            refs::with_mut(raw_ref(regs, vec)?, |slot| match slot {
                Value::Vec(items) => {
                    Rc::make_mut(items).push(v);
                    Ok(())
                }
                other => Err(mismatch("Vec", other.tag())),
            })?
        }
        Instr::Ruby { dst, base, reading } => {
            let base = ops::text(&*val(regs, base)?)?;
            let reading = ops::text(&*val(regs, reading)?)?;
            set(regs, dst, Value::Text(Text::Ruby(Rc::new((base, reading)))))
        }
        Instr::MonoRuby {
            dst,
            base,
            readings,
        } => {
            let base = ops::text(&*val(regs, base)?)?;
            let readings = texts(&*val(regs, readings)?)?;
            set(
                regs,
                dst,
                Value::Text(Text::MonoRuby(Rc::new((base, readings)))),
            )
        }
        Instr::Effect { dst, base, effect } => {
            let base = ops::text(&*val(regs, base)?)?;
            let effect = val(regs, effect)?.into_owned();
            set(
                regs,
                dst,
                Value::Text(Text::Effect(Rc::new((base, effect)))),
            )
        }
        Instr::Concat { dst, parts } => {
            let parts = texts(&*val(regs, parts)?)?;
            set(regs, dst, Value::Text(Text::Concat(Rc::new(parts))))
        }
        Instr::ToText { dst, src } => {
            let t = ops::to_text(&*val(regs, src)?)?;
            set(regs, dst, Value::Text(t))
        }
        Instr::Unreachable => Err(Trap::new(TrapKind::Unreachable)),
        Instr::Jmp { .. }
        | Instr::JmpIf { .. }
        | Instr::JmpNot { .. }
        | Instr::Ret { .. }
        | Instr::Call { .. }
        | Instr::CallImp { .. }
        | Instr::CallVal { .. }
        | Instr::Step { .. }
        | Instr::Rand { .. }
        | Instr::RefStatic { .. } => Err(fault()),
    }
}
