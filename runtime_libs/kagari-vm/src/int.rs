//! Integer instructions over an `IntType { lo, hi, bits, signed }`: i128 arithmetic, checked,
//! then range-checked against `lo..=hi`.

use kagari_bytecode::{IntBinOp, IntType, IntUnOp};

use crate::trap::TrapKind;
use crate::trap::{Trap, mismatch};
use crate::value::Value;

fn overflow() -> Trap {
    Trap::new(TrapKind::Overflow)
}

/// The operand as an `Int`.
pub(crate) fn int(v: &Value) -> Result<i128, Trap> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(mismatch("Int", other.tag())),
    }
}

/// `v` wrapped to `bits`: masked, then sign-extended when signed. At 128 bits an unsigned
/// value must be non-negative.
fn wrap(v: i128, bits: u32, signed: bool) -> Option<i128> {
    if bits >= 128 {
        return (signed || v >= 0).then_some(v);
    }
    let mask = 1u128.checked_shl(bits)?.wrapping_sub(1);
    let raw = (v as u128) & mask;
    let sign = 1u128.checked_shl(bits.checked_sub(1)?)?;
    if signed && raw & sign != 0 {
        Some((raw | !mask) as i128)
    } else {
        Some(raw as i128)
    }
}

/// The shift amount `b`, which must be below `bits`.
fn amount(b: i128, t: &IntType) -> Result<u32, Trap> {
    u32::try_from(b)
        .ok()
        .filter(|n| *n < t.bits)
        .ok_or_else(overflow)
}

fn in_range(r: i128, t: &IntType) -> Result<i128, Trap> {
    if (t.lo..=t.hi).contains(&r) {
        Ok(r)
    } else {
        Err(overflow())
    }
}

pub(crate) fn binary(op: IntBinOp, a: i128, b: i128, t: &IntType) -> Result<i128, Trap> {
    let zero = || Trap::new(TrapKind::DivisionByZero);
    let r = match op {
        IntBinOp::Add => a.checked_add(b),
        IntBinOp::Sub => a.checked_sub(b),
        IntBinOp::Mul => a.checked_mul(b),
        IntBinOp::Div => {
            if b == 0 {
                return Err(zero());
            }
            a.checked_div(b)
        }
        IntBinOp::Rem => {
            if b == 0 {
                return Err(zero());
            }
            a.checked_rem(b)
        }
        IntBinOp::And => Some(a & b),
        IntBinOp::Or => Some(a | b),
        IntBinOp::Xor => Some(a ^ b),
        IntBinOp::Shl => {
            let n = amount(b, t)?;
            wrap((a as u128).wrapping_shl(n) as i128, t.bits, t.signed)
        }
        IntBinOp::Shr => {
            let n = amount(b, t)?;
            a.checked_shr(n)
        }
    };
    in_range(r.ok_or_else(overflow)?, t)
}

pub(crate) fn unary(op: IntUnOp, a: i128, t: &IntType) -> Result<i128, Trap> {
    let r = match op {
        IntUnOp::Neg => a.checked_neg(),
        IntUnOp::Not if t.signed => Some(!a),
        IntUnOp::Not => wrap(!a, t.bits, false),
        IntUnOp::Clamp => return Ok(a.max(t.lo).min(t.hi)),
    };
    in_range(r.ok_or_else(overflow)?, t)
}

/// The operand as an `Int`, applied to both operands of a binary instruction.
pub(crate) fn operands(a: &Value, b: &Value) -> Result<(i128, i128), Trap> {
    Ok((int(a)?, int(b)?))
}
