//! Integer helpers of const evaluation, by Rust integer type.

use crate::ty::IntTy;

/// The number of bits of a Rust integer type (`usize`/`isize` are 64-bit).
pub fn int_bits(ty: IntTy) -> u32 {
    match ty {
        IntTy::U8 | IntTy::I8 => 8,
        IntTy::U16 | IntTy::I16 => 16,
        IntTy::U32 | IntTy::I32 => 32,
        IntTy::U64 | IntTy::I64 | IntTy::Usize | IntTy::Isize => 64,
        IntTy::U128 | IntTy::I128 => 128,
    }
}

pub fn is_signed(ty: IntTy) -> bool {
    matches!(
        ty,
        IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize
    )
}

/// `value` truncated to the bits of `ty` and reinterpreted, as Rust's `as` does. `None` for
/// a `u128` result above `i128::MAX`, which the compiler cannot represent.
pub fn wrap_to(value: i128, ty: IntTy) -> Option<i128> {
    let bits = int_bits(ty);
    if bits == 128 {
        return if is_signed(ty) || value >= 0 {
            Some(value)
        } else {
            None
        };
    }
    let modulus: u128 = 1u128.checked_shl(bits)?;
    let mask = modulus.wrapping_sub(1);
    let raw = (value as u128) & mask;
    let half = 1u128.checked_shl(bits.checked_sub(1)?)?;
    if is_signed(ty) && raw >= half {
        Some((raw as i128).wrapping_sub(modulus as i128))
    } else {
        Some(raw as i128)
    }
}

/// `!value` in the type `ty`.
pub fn bit_not(value: i128, ty: IntTy) -> Option<i128> {
    if is_signed(ty) {
        Some(!value)
    } else {
        wrap_to(!value, ty)
    }
}

/// `value << amount` in `ty`; `None` when `amount` is not below the bit width.
pub fn shl(value: i128, amount: i128, ty: IntTy) -> Option<i128> {
    let amount = u32::try_from(amount).ok().filter(|a| *a < int_bits(ty))?;
    wrap_to(((value as u128).checked_shl(amount)?) as i128, ty)
}

/// `value >> amount` in `ty` (arithmetic for signed types); `None` when `amount` is not
/// below the bit width.
pub fn shr(value: i128, amount: i128, ty: IntTy) -> Option<i128> {
    let amount = u32::try_from(amount).ok().filter(|a| *a < int_bits(ty))?;
    value.checked_shr(amount)
}
