//! Exact numbers of const evaluation: `Decimal`, `Rat`, and the integer helpers shared by
//! the interpreter. Every operation is checked; `None` means the result is not representable
//! (an overflow of the `i128` parts).

use std::cmp::Ordering;
use std::fmt;

use crate::ty::IntTy;

/// An exact decimal `mantissa / 10^scale`, normalized: no trailing zero digits in the
/// mantissa when `scale > 0`, so equal values have equal representations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    pub const ZERO: Decimal = Decimal {
        mantissa: 0,
        scale: 0,
    };
    pub const ONE: Decimal = Decimal {
        mantissa: 1,
        scale: 0,
    };

    pub fn new(mantissa: i128, scale: u32) -> Decimal {
        let mut out = Decimal { mantissa, scale };
        while out.scale > 0 && out.mantissa.checked_rem(10) == Some(0) {
            out.mantissa = out.mantissa.checked_div(10).unwrap_or(0);
            out.scale = out.scale.saturating_sub(1);
        }
        if out.mantissa == 0 {
            out.scale = 0;
        }
        out
    }

    pub fn from_int(value: i128) -> Decimal {
        Decimal::new(value, 0)
    }

    pub fn mantissa(self) -> i128 {
        self.mantissa
    }

    pub fn scale(self) -> u32 {
        self.scale
    }

    /// Parses a literal such as `0.5`, `12.25` or `1_000.0`.
    pub fn parse(text: &str) -> Option<Decimal> {
        let text: String = text.chars().filter(|ch| *ch != '_').collect();
        let (int_part, frac_part) = text.split_once('.').unwrap_or((&text, ""));
        let mut mantissa: i128 = 0;
        for ch in int_part.chars().chain(frac_part.chars()) {
            let digit = i128::from(ch.to_digit(10)?);
            mantissa = mantissa.checked_mul(10)?.checked_add(digit)?;
        }
        let scale = u32::try_from(frac_part.len()).ok()?;
        Some(Decimal::new(mantissa, scale))
    }

    /// Both mantissas at the larger scale.
    fn aligned(self, other: Decimal) -> Option<(i128, i128, u32)> {
        let scale = self.scale.max(other.scale);
        let a = self
            .mantissa
            .checked_mul(10i128.checked_pow(scale.checked_sub(self.scale)?)?)?;
        let b = other
            .mantissa
            .checked_mul(10i128.checked_pow(scale.checked_sub(other.scale)?)?)?;
        Some((a, b, scale))
    }

    pub fn checked_add(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other)?;
        Some(Decimal::new(a.checked_add(b)?, scale))
    }

    pub fn checked_sub(self, other: Decimal) -> Option<Decimal> {
        let (a, b, scale) = self.aligned(other)?;
        Some(Decimal::new(a.checked_sub(b)?, scale))
    }

    pub fn checked_mul(self, other: Decimal) -> Option<Decimal> {
        Some(Decimal::new(
            self.mantissa.checked_mul(other.mantissa)?,
            self.scale.checked_add(other.scale)?,
        ))
    }

    pub fn checked_neg(self) -> Option<Decimal> {
        Some(Decimal::new(self.mantissa.checked_neg()?, self.scale))
    }

    /// The exact quotient. `Err(true)` when `other` is zero, `Err(false)` when the quotient
    /// has no finite decimal expansion or does not fit.
    pub fn checked_div(self, other: Decimal) -> Result<Decimal, bool> {
        if other.mantissa == 0 {
            return Err(true);
        }
        let quotient = Rat::from_decimal(self)
            .ok_or(false)?
            .checked_div(Rat::from_decimal(other).ok_or(false)?)
            .ok_or(false)?;
        quotient.to_decimal().ok_or(false)
    }

    /// `None` when aligning the scales overflows.
    pub fn checked_cmp(self, other: Decimal) -> Option<Ordering> {
        if self.mantissa.signum() != other.mantissa.signum() {
            return Some(self.mantissa.signum().cmp(&other.mantissa.signum()));
        }
        let (a, b, _) = self.aligned(other)?;
        Some(a.cmp(&b))
    }

    /// Whether `0 <= self <= 1`.
    pub fn is_fraction(self) -> bool {
        self.mantissa >= 0 && self.checked_cmp(Decimal::ONE) != Some(Ordering::Greater)
    }
}

impl fmt::Display for Decimal {
    /// Exact decimal notation, as a literal is written: `0.5`, `-1.25`, `3.0`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.mantissa.unsigned_abs().to_string();
        let sign = if self.mantissa < 0 { "-" } else { "" };
        let scale = self.scale as usize;
        if scale == 0 {
            return write!(f, "{sign}{digits}.0");
        }
        let padded = if digits.len() <= scale {
            format!(
                "{}{digits}",
                "0".repeat(scale.saturating_sub(digits.len()).saturating_add(1))
            )
        } else {
            digits
        };
        let split = padded.len().saturating_sub(scale);
        write!(f, "{sign}{}.{}", &padded[..split], &padded[split..])
    }
}

/// An exact rational `num / den` in lowest terms with `den > 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rat {
    num: i128,
    den: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.checked_abs().unwrap_or(i128::MAX);
    b = b.checked_abs().unwrap_or(i128::MAX);
    while b != 0 {
        let rem = a.checked_rem(b).unwrap_or(0);
        a = b;
        b = rem;
    }
    a
}

impl Rat {
    /// `None` when `den` is zero or normalizing overflows.
    pub fn new(num: i128, den: i128) -> Option<Rat> {
        if den == 0 {
            return None;
        }
        let divisor = gcd(num, den).max(1);
        let mut num = num.checked_div(divisor)?;
        let mut den = den.checked_div(divisor)?;
        if den < 0 {
            num = num.checked_neg()?;
            den = den.checked_neg()?;
        }
        Some(Rat { num, den })
    }

    pub fn from_int(value: i128) -> Rat {
        Rat { num: value, den: 1 }
    }

    pub fn from_decimal(value: Decimal) -> Option<Rat> {
        Rat::new(value.mantissa, 10i128.checked_pow(value.scale)?)
    }

    pub fn numer(self) -> i128 {
        self.num
    }

    pub fn denom(self) -> i128 {
        self.den
    }

    pub fn checked_add(self, other: Rat) -> Option<Rat> {
        let num = self
            .num
            .checked_mul(other.den)?
            .checked_add(other.num.checked_mul(self.den)?)?;
        Rat::new(num, self.den.checked_mul(other.den)?)
    }

    pub fn checked_sub(self, other: Rat) -> Option<Rat> {
        self.checked_add(other.checked_neg()?)
    }

    pub fn checked_mul(self, other: Rat) -> Option<Rat> {
        Rat::new(
            self.num.checked_mul(other.num)?,
            self.den.checked_mul(other.den)?,
        )
    }

    /// `None` on division by zero or overflow; callers check for zero first.
    pub fn checked_div(self, other: Rat) -> Option<Rat> {
        Rat::new(
            self.num.checked_mul(other.den)?,
            self.den.checked_mul(other.num)?,
        )
    }

    pub fn checked_neg(self) -> Option<Rat> {
        Some(Rat {
            num: self.num.checked_neg()?,
            den: self.den,
        })
    }

    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    pub fn checked_cmp(self, other: Rat) -> Option<Ordering> {
        let a = self.num.checked_mul(other.den)?;
        let b = other.num.checked_mul(self.den)?;
        Some(a.cmp(&b))
    }

    /// The decimal with the same value, when the denominator has only the prime factors 2
    /// and 5.
    pub fn to_decimal(self) -> Option<Decimal> {
        let mut den = self.den;
        let mut twos: u32 = 0;
        let mut fives: u32 = 0;
        while den.checked_rem(2) == Some(0) {
            den = den.checked_div(2)?;
            twos = twos.checked_add(1)?;
        }
        while den.checked_rem(5) == Some(0) {
            den = den.checked_div(5)?;
            fives = fives.checked_add(1)?;
        }
        if den != 1 {
            return None;
        }
        let scale = twos.max(fives);
        // num / (2^twos 5^fives) = num * 2^(scale-twos) * 5^(scale-fives) / 10^scale
        let mantissa = self
            .num
            .checked_mul(2i128.checked_pow(scale.checked_sub(twos)?)?)?
            .checked_mul(5i128.checked_pow(scale.checked_sub(fives)?)?)?;
        Some(Decimal::new(mantissa, scale))
    }
}

impl fmt::Display for Rat {
    /// `3/4`, `-1/2`, `5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

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
