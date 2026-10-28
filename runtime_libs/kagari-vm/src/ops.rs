//! Value operations: structural equality, ordering, Decimal and Rat arithmetic, TOTEXT.

use std::cmp::Ordering;
use std::rc::Rc;

use kagari_bytecode::{ArithOp, CmpOp};
use kagari_num::{Decimal, Rat};

use crate::refs;
use crate::trap::{Trap, TrapKind, mismatch};
use crate::value::{Text, Value};

/// The EQ instruction's structural equality, with EQ's traps: `Uninit` → `ReadUninit`,
/// different tags → `TagMismatch`, closures and host functions → `ClosureEquality`.
/// References compare by their targets.
pub fn value_eq(a: &Value, b: &Value) -> Result<bool, Trap> {
    let mut work: Vec<(Value, Value)> = vec![(a.clone(), b.clone())];
    while let Some((a, b)) = work.pop() {
        let a = deref(a)?;
        let b = deref(b)?;
        if a.tag() != b.tag() {
            return Err(mismatch(a.tag(), b.tag()));
        }
        let equal = match (&a, &b) {
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Decimal(x), Value::Decimal(y)) => x == y,
            (Value::Rat(x), Value::Rat(y)) => x == y,
            (Value::String(x), Value::String(y)) => x == y,
            (Value::Asset(x), Value::Asset(y)) => x == y,
            (Value::Func(x), Value::Func(y)) => x == y,
            (Value::Handle(x), Value::Handle(y)) => x == y,
            (Value::Text(x), Value::Text(y)) => text_eq(x, y, &mut work),
            (Value::Tuple(x), Value::Tuple(y))
            | (Value::Array(x), Value::Array(y))
            | (Value::Vec(x), Value::Vec(y)) => items_eq(x, y, &mut work),
            (Value::Adt(x), Value::Adt(y)) => {
                x.ty == y.ty && x.variant == y.variant && items_eq(&x.fields, &y.fields, &mut work)
            }
            (Value::Closure(_), _) | (Value::HostFn(_), _) => {
                return Err(Trap::new(TrapKind::ClosureEquality));
            }
            (Value::Host(x), Value::Host(y)) => {
                if Rc::ptr_eq(&x.0, &y.0) {
                    true
                } else if x.0.type_name() != y.0.type_name() {
                    return Err(mismatch(x.0.type_name(), y.0.type_name()));
                } else {
                    x.0.eq_object(&*y.0)
                        .ok_or_else(|| mismatch("comparable host object", x.0.type_name()))?
                }
            }
            _ => return Err(Trap::new(TrapKind::Unreachable)),
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A comparison operand: references are read, `Uninit` panics.
fn deref(v: Value) -> Result<Value, Trap> {
    match v {
        Value::Ref(r) => refs::read(&r),
        Value::Uninit => Err(Trap::new(TrapKind::ReadUninit)),
        v => Ok(v),
    }
}

/// Queues the items pairwise; false when the lengths differ.
fn items_eq(x: &[Value], y: &[Value], work: &mut Vec<(Value, Value)>) -> bool {
    if x.len() != y.len() {
        return false;
    }
    work.extend(x.iter().cloned().zip(y.iter().cloned()));
    true
}

/// Compares the text nodes' shapes and queues their children; false when they differ.
fn text_eq(x: &Text, y: &Text, work: &mut Vec<(Value, Value)>) -> bool {
    let pair = |a: &Text, b: &Text| (Value::Text(a.clone()), Value::Text(b.clone()));
    match (x, y) {
        (Text::Plain(a), Text::Plain(b)) => a == b,
        (Text::Ruby(a), Text::Ruby(b)) => {
            work.push(pair(&a.0, &b.0));
            work.push(pair(&a.1, &b.1));
            true
        }
        (Text::MonoRuby(a), Text::MonoRuby(b)) => {
            if a.1.len() != b.1.len() {
                return false;
            }
            work.push(pair(&a.0, &b.0));
            work.extend(a.1.iter().zip(&b.1).map(|(r, s)| pair(r, s)));
            true
        }
        (Text::Effect(a), Text::Effect(b)) => {
            work.push(pair(&a.0, &b.0));
            work.push((a.1.clone(), b.1.clone()));
            true
        }
        (Text::Concat(a), Text::Concat(b)) => {
            if a.len() != b.len() {
                return false;
            }
            work.extend(a.iter().zip(b.iter()).map(|(r, s)| pair(r, s)));
            true
        }
        _ => false,
    }
}

fn overflow() -> Trap {
    Trap::new(TrapKind::Overflow)
}

/// EQ through GE on two operands (already dereferenced for the orderings).
pub(crate) fn compare(op: CmpOp, a: &Value, b: &Value) -> Result<bool, Trap> {
    let ordering = match op {
        CmpOp::Eq => return value_eq(a, b),
        CmpOp::Ne => return value_eq(a, b).map(|eq| !eq),
        _ => order(a, b)?,
    };
    Ok(match op {
        CmpOp::Lt => ordering == Ordering::Less,
        CmpOp::Le => ordering != Ordering::Greater,
        CmpOp::Gt => ordering == Ordering::Greater,
        _ => ordering != Ordering::Less,
    })
}

fn order(a: &Value, b: &Value) -> Result<Ordering, Trap> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Decimal(x), Value::Decimal(y)) => x.checked_cmp(*y).ok_or_else(overflow),
        (Value::Rat(x), Value::Rat(y)) => x.checked_cmp(*y).ok_or_else(overflow),
        (Value::Int(_) | Value::Decimal(_) | Value::Rat(_), other) => {
            Err(mismatch(a.tag(), other.tag()))
        }
        (other, _) => Err(mismatch("Int, Decimal or Rat", other.tag())),
    }
}

pub(crate) fn decimal(v: &Value) -> Result<Decimal, Trap> {
    match v {
        Value::Decimal(d) => Ok(*d),
        other => Err(mismatch("Decimal", other.tag())),
    }
}

pub(crate) fn rat(v: &Value) -> Result<Rat, Trap> {
    match v {
        Value::Rat(r) => Ok(*r),
        other => Err(mismatch("Rat", other.tag())),
    }
}

pub(crate) fn boolean(v: &Value) -> Result<bool, Trap> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(mismatch("Bool", other.tag())),
    }
}

pub(crate) fn text(v: &Value) -> Result<Text, Trap> {
    match v {
        Value::Text(t) => Ok(t.clone()),
        other => Err(mismatch("Text", other.tag())),
    }
}

pub(crate) fn decimal_op(op: ArithOp, a: Decimal, b: Decimal) -> Result<Decimal, Trap> {
    match op {
        ArithOp::Add => a.checked_add(b).ok_or_else(overflow),
        ArithOp::Sub => a.checked_sub(b).ok_or_else(overflow),
        ArithOp::Mul => a.checked_mul(b).ok_or_else(overflow),
        ArithOp::Div => a.checked_div(b).map_err(|zero| {
            if zero {
                Trap::new(TrapKind::DivisionByZero)
            } else {
                overflow()
            }
        }),
    }
}

pub(crate) fn rat_op(op: ArithOp, a: Rat, b: Rat) -> Result<Rat, Trap> {
    match op {
        ArithOp::Add => a.checked_add(b).ok_or_else(overflow),
        ArithOp::Sub => a.checked_sub(b).ok_or_else(overflow),
        ArithOp::Mul => a.checked_mul(b).ok_or_else(overflow),
        ArithOp::Div => {
            if b.is_zero() {
                return Err(Trap::new(TrapKind::DivisionByZero));
            }
            a.checked_div(b).ok_or_else(overflow)
        }
    }
}

/// RATNEW: `num / den` in lowest terms.
pub(crate) fn rat_new(num: i128, den: i128) -> Result<Rat, Trap> {
    if den == 0 {
        return Err(Trap::new(TrapKind::DivisionByZero));
    }
    Rat::new(num, den).ok_or_else(overflow)
}

/// TOTEXT: a value as plain text.
pub(crate) fn to_text(v: &Value) -> Result<Text, Trap> {
    let plain = |s: String| Text::Plain(Rc::from(s));
    match v {
        Value::Int(i) => Ok(plain(i.to_string())),
        Value::Decimal(d) => Ok(plain(d.to_string())),
        Value::Rat(r) => Ok(plain(r.to_string())),
        Value::Text(t) => Ok(t.clone()),
        Value::String(s) => Ok(Text::Plain(s.clone())),
        other => Err(mismatch("Int, Decimal, Rat, Text or String", other.tag())),
    }
}
