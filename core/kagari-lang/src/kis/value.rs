//! Names, values and patterns written in `.kis` attributes.

use la_arena::{Arena, Idx};

use crate::diagnostic::DiagnosticCode;
use crate::kis::hir::EnumDecl;

/// The type of a game variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarType {
    Bool,
    Enum(Idx<EnumDecl>),
    /// `0..=255`.
    Char,
    Int {
        min: i16,
        max: i16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    /// Index into `EnumDecl::variants`.
    Variant(u32),
    Int(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Wildcard,
    Value(Value),
    /// Inclusive; only for Char and Int.
    Range(i32, i32),
}

/// Why an attribute value does not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueError {
    /// Syntactically wrong (`kis/bad-value`).
    Bad(String),
    /// Well-formed but outside the type's domain (`kis/value-out-of-range`).
    OutOfRange(String),
    /// A range whose low end exceeds its high end (`kis/bad-range`).
    BadRange(String),
}

impl ValueError {
    pub fn code(&self) -> DiagnosticCode {
        match self {
            ValueError::Bad(_) => DiagnosticCode::KisBadValue,
            ValueError::OutOfRange(_) => DiagnosticCode::KisValueOutOfRange,
            ValueError::BadRange(_) => DiagnosticCode::KisBadRange,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            ValueError::Bad(msg) | ValueError::OutOfRange(msg) | ValueError::BadRange(msg) => msg,
        }
    }
}

/// Whether `s` is a valid name: a Unicode identifier other than `_` alone.
pub fn check_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || unicode_ident::is_xid_start(first))
        && chars.all(unicode_ident::is_xid_continue)
        && s != "_"
}

/// The inclusive value domain of a counter type, or `None` for bool and enums.
pub fn int_domain(ty: VarType) -> Option<(i32, i32)> {
    match ty {
        VarType::Char => Some((0, 255)),
        VarType::Int { min, max } => Some((i32::from(min), i32::from(max))),
        VarType::Bool | VarType::Enum(_) => None,
    }
}

/// Parse a value of type `ty`: `true`/`false`, a variant name, or a decimal number.
pub fn parse_value(ty: VarType, enums: &Arena<EnumDecl>, text: &str) -> Result<Value, ValueError> {
    match ty {
        VarType::Bool => match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(ValueError::Bad(format!(
                "`{text}` is not a bool; expected `true` or `false`"
            ))),
        },
        VarType::Enum(idx) => {
            let decl = &enums[idx];
            decl.variants
                .iter()
                .position(|variant| variant.name == text)
                .and_then(|index| u32::try_from(index).ok())
                .map(Value::Variant)
                .ok_or_else(|| {
                    ValueError::Bad(format!("`{text}` is not a variant of `{}`", decl.name))
                })
        }
        VarType::Char | VarType::Int { .. } => {
            let (min, max) = int_domain(ty).unwrap_or((0, 0));
            parse_int(ty, text, min, max).map(Value::Int)
        }
    }
}

/// Parse the amount of an `<add>`/`<sub>`: digits only, at most `i16::MAX` for int and 255
/// for char.
pub fn parse_amount(ty: VarType, text: &str) -> Result<u16, ValueError> {
    let limit: u16 = match ty {
        VarType::Char => 255,
        VarType::Int { .. } => i16::MAX.unsigned_abs(),
        VarType::Bool | VarType::Enum(_) => {
            return Err(ValueError::Bad(
                "only `char` and `int` variables can be changed by an amount".to_owned(),
            ));
        }
    };
    if !is_digits(text) {
        return Err(ValueError::Bad(format!(
            "`{text}` is not an amount; expected a non-negative decimal number"
        )));
    }
    match text.parse::<u64>() {
        Ok(value) if value <= u64::from(limit) => u16::try_from(value)
            .map_err(|_| ValueError::OutOfRange(format!("amount `{text}` exceeds {limit}"))),
        _ => Err(ValueError::OutOfRange(format!(
            "amount `{text}` exceeds {limit}"
        ))),
    }
}

/// Parse one `<case val>` pattern of type `ty`: `_`, a value, or `lo..=hi` for char and int.
pub fn parse_pattern(
    ty: VarType,
    enums: &Arena<EnumDecl>,
    text: &str,
) -> Result<Pattern, ValueError> {
    if text == "_" {
        return Ok(Pattern::Wildcard);
    }
    let Some((lo, hi)) = text.split_once("..=") else {
        return parse_value(ty, enums, text).map(Pattern::Value);
    };
    let Some((min, max)) = int_domain(ty) else {
        return Err(ValueError::Bad(format!(
            "`{text}`: ranges are only allowed for `char` and `int` variables"
        )));
    };
    let lo = parse_int(ty, lo, min, max)?;
    let hi = parse_int(ty, hi, min, max)?;
    if lo > hi {
        return Err(ValueError::BadRange(format!(
            "range `{text}` is empty: {lo} is greater than {hi}"
        )));
    }
    Ok(Pattern::Range(lo, hi))
}

/// Parse an `i16` bound (`min`/`max` of `<var-int>`/`<meta-int>`).
pub fn parse_i16(text: &str) -> Result<i16, ValueError> {
    let bad = || ValueError::Bad(format!("`{text}` is not an integer from -32768 to 32767"));
    if !is_signed_digits(text) {
        return Err(bad());
    }
    text.parse::<i16>().map_err(|_| bad())
}

/// Parse an enum variant's `repr` (`u8`).
pub fn parse_repr(text: &str) -> Result<u8, ValueError> {
    let bad = || ValueError::Bad(format!("`{text}` is not an integer from 0 to 255"));
    if !is_digits(text) {
        return Err(bad());
    }
    text.parse::<u8>().map_err(|_| bad())
}

/// Split a `vars` or `val` list on ASCII whitespace.
pub fn split_list(text: &str) -> Vec<&str> {
    text.split_ascii_whitespace().collect()
}

/// Parse a decimal number in `[min, max]`. `char` accepts digits only; `int` also a leading `-`.
fn parse_int(ty: VarType, text: &str, min: i32, max: i32) -> Result<i32, ValueError> {
    let well_formed = match ty {
        VarType::Char => is_digits(text),
        _ => is_signed_digits(text),
    };
    if !well_formed {
        let what = match ty {
            VarType::Char => "a `char` (a decimal number from 0 to 255)",
            _ => "an `int` (a decimal number)",
        };
        return Err(ValueError::Bad(format!("`{text}` is not {what}")));
    }
    let out_of_range = || ValueError::OutOfRange(format!("`{text}` is outside {min}..={max}"));
    let value = text.parse::<i64>().map_err(|_| out_of_range())?;
    if value < i64::from(min) || value > i64::from(max) {
        return Err(out_of_range());
    }
    i32::try_from(value).map_err(|_| out_of_range())
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn is_signed_digits(text: &str) -> bool {
    is_digits(text.strip_prefix('-').unwrap_or(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(check_name("visited_a"));
        assert!(check_name("_x"));
        assert!(check_name("はなこ"));
        assert!(!check_name("_"));
        assert!(!check_name("1abc"));
        assert!(!check_name(""));
        assert!(!check_name("a-b"));
    }

    #[test]
    fn values() {
        let enums = Arena::new();
        let int = VarType::Int { min: -2, max: 5 };
        assert_eq!(
            parse_value(VarType::Bool, &enums, "true"),
            Ok(Value::Bool(true))
        );
        assert!(matches!(
            parse_value(VarType::Bool, &enums, "maybe"),
            Err(ValueError::Bad(_))
        ));
        assert_eq!(parse_value(int, &enums, "-2"), Ok(Value::Int(-2)));
        assert!(matches!(
            parse_value(int, &enums, "6"),
            Err(ValueError::OutOfRange(_))
        ));
        assert!(matches!(
            parse_value(VarType::Char, &enums, "-1"),
            Err(ValueError::Bad(_))
        ));
        assert!(matches!(
            parse_value(VarType::Char, &enums, "256"),
            Err(ValueError::OutOfRange(_))
        ));
        assert_eq!(parse_amount(VarType::Char, "255"), Ok(255));
        assert!(parse_amount(VarType::Char, "256").is_err());
        assert_eq!(parse_amount(int, "32767"), Ok(32767));
        assert!(parse_amount(int, "-1").is_err());
        assert_eq!(
            parse_pattern(int, &enums, "0..=3"),
            Ok(Pattern::Range(0, 3))
        );
        assert!(matches!(
            parse_pattern(int, &enums, "3..=0"),
            Err(ValueError::BadRange(_))
        ));
        assert!(matches!(
            parse_pattern(VarType::Bool, &enums, "0..=1"),
            Err(ValueError::Bad(_))
        ));
        assert_eq!(
            parse_pattern(VarType::Bool, &enums, "_"),
            Ok(Pattern::Wildcard)
        );
        assert_eq!(parse_i16("-32768"), Ok(i16::MIN));
        assert!(parse_i16("40000").is_err());
        assert_eq!(parse_repr("255"), Ok(255));
        assert!(parse_repr("256").is_err());
    }
}
