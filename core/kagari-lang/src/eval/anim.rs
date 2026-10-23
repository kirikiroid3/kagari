//! The const-evaluation rules on animation values and fractions
//! (`docs/std/src/checks.md#const-evaluation-errors`): the Motion identity rule, easings
//! used by applied animations, and `Decimal` fractions of opacity, volume, anchor and
//! filters.

use std::cmp::Ordering;

use crate::diagnostic::DiagnosticCode;

use super::Fault;
use super::num::Decimal;
use super::std_items::{AdtInfo, StdTable};
use super::value::ConstValue;

fn fraction_fault(what: &str, value: Decimal, range: &str) -> Fault {
    Fault::new(
        DiagnosticCode::EvalFractionOutOfRange,
        format!("{what} must be {range}, found {value}"),
    )
}

/// `value` is a `Decimal` in `0.0..=1.0`.
pub fn check_fraction(value: &ConstValue, what: &str) -> Result<(), Fault> {
    match value.as_decimal() {
        Some(decimal) if !decimal.is_fraction() => {
            Err(fraction_fault(what, decimal, "in 0.0..=1.0"))
        }
        _ => Ok(()),
    }
}

fn check_non_negative(value: &ConstValue, what: &str) -> Result<(), Fault> {
    match value.as_decimal() {
        Some(decimal) if decimal.mantissa() < 0 => {
            Err(fraction_fault(what, decimal, "at least 0.0"))
        }
        _ => Ok(()),
    }
}

fn field<'v>(value: &'v ConstValue, info: &AdtInfo, name: &str) -> Option<&'v ConstValue> {
    value.field(info.field(name)?)
}

/// A `Filter`: grayscale and sepia are fractions, brightness, contrast and saturation are
/// factors `>= 0`.
pub fn check_filter(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    for name in ["grayscale", "sepia"] {
        if let Some(amount) = field(value, &std.filter, name) {
            check_fraction(amount, &format!("the filter's {name}"))?;
        }
    }
    for name in ["brightness", "contrast", "saturation"] {
        if let Some(amount) = field(value, &std.filter, name) {
            check_non_negative(amount, &format!("the filter's {name}"))?;
        }
    }
    Ok(())
}

/// An `Anchor`: both coordinates are fractions.
pub fn check_anchor(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    for name in ["x", "y"] {
        if let Some(coordinate) = field(value, &std.anchor, name) {
            check_fraction(coordinate, &format!("the anchor's {name}"))?;
        }
    }
    Ok(())
}

fn invalid_easing(message: String) -> Fault {
    Fault::new(DiagnosticCode::EvalInvalidEasing, message)
}

/// An `Easing` used by an applied animation: `CubicBezier` x1 and x2 in `0.0..=1.0`,
/// `Steps(n)` with `n > 0`.
pub fn check_easing(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    let ConstValue::Adt {
        variant, fields, ..
    } = value
    else {
        return Ok(());
    };
    if Some(*variant) == std.easing.variant("CubicBezier") {
        for (index, name) in [(0, "x1"), (2, "x2")] {
            if let Some(x) = fields.get(index).and_then(ConstValue::as_decimal)
                && !x.is_fraction()
            {
                return Err(invalid_easing(format!(
                    "`CubicBezier` {name} must be in 0.0..=1.0, found {x}"
                )));
            }
        }
    } else if Some(*variant) == std.easing.variant("Steps")
        && fields.first().and_then(ConstValue::as_int) == Some(0)
    {
        return Err(invalid_easing(
            "`Steps(0)`: the number of steps must not be 0".to_owned(),
        ));
    }
    Ok(())
}

/// A `Tween`'s easing.
pub fn check_tween(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    match field(value, &std.tween, "easing") {
        Some(easing) => check_easing(easing, std),
        None => Ok(()),
    }
}

/// A `Transition`'s easing.
pub fn check_transition(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    match field(value, &std.transition, "easing") {
        Some(easing) => check_easing(easing, std),
        None => Ok(()),
    }
}

fn invalid_motion(message: impl Into<String>) -> Fault {
    Fault::new(DiagnosticCode::EvalInvalidMotion, message)
}

/// The identity rule of a `Motion`: at least one key, every `at` above 0 and strictly
/// increasing, a last key at identity (offset (0, 0), scale 1.0, rotation 0°), `repeat > 0`;
/// plus the easing of every key.
pub fn check_motion(value: &ConstValue, std: &StdTable) -> Result<(), Fault> {
    let keys: &[ConstValue] = match field(value, &std.motion, "keys") {
        Some(ConstValue::Vec(keys) | ConstValue::Array(keys)) => keys,
        _ => &[],
    };
    if keys.is_empty() {
        return Err(invalid_motion("a motion needs at least one key"));
    }
    let mut previous: Option<u32> = None;
    for key in keys {
        let at = field(key, &std.motion_key, "at").and_then(|at| std.duration_ms(at));
        if let Some(at) = at {
            match previous {
                None if at == 0 => {
                    return Err(invalid_motion("the first motion key must be after 0 ms"));
                }
                Some(previous) if at <= previous => {
                    return Err(invalid_motion(format!(
                        "motion key at {at} ms is not after the previous key at {previous} ms"
                    )));
                }
                _ => {}
            }
            previous = Some(at);
        }
        if let Some(easing) = field(key, &std.motion_key, "easing") {
            check_easing(easing, std)?;
        }
    }
    if let Some(last) = keys.last()
        && !is_identity_key(last, std)
    {
        return Err(invalid_motion(
            "the last motion key must be the identity: offset (0, 0), scale 1.0, rotation 0°",
        ));
    }
    if field(value, &std.motion, "repeat").and_then(ConstValue::as_int) == Some(0) {
        return Err(invalid_motion(
            "a motion must repeat at least once, found `repeat(0)`",
        ));
    }
    Ok(())
}

fn is_identity_key(key: &ConstValue, std: &StdTable) -> bool {
    let offset_zero = field(key, &std.motion_key, "offset").is_none_or(|offset| {
        ["dx", "dy"]
            .iter()
            .all(|name| field(offset, &std.offset, name).and_then(ConstValue::as_int) == Some(0))
    });
    let scale_one = field(key, &std.motion_key, "scale")
        .and_then(ConstValue::as_decimal)
        .is_none_or(|scale| scale.checked_cmp(Decimal::ONE) == Some(Ordering::Equal));
    let rotation_zero = field(key, &std.motion_key, "rotation")
        .and_then(|angle| field(angle, &std.angle, "degrees"))
        .and_then(ConstValue::as_decimal)
        .is_none_or(|degrees| degrees == Decimal::ZERO);
    offset_zero && scale_one && rotation_zero
}

/// The `delay` of a `Tween`, `Transition`, `Motion` or `Sound` (`info`), in ms.
pub fn delay_ms(value: &ConstValue, info: &AdtInfo, std: &StdTable) -> u32 {
    field(value, info, "delay")
        .and_then(|delay| std.duration_ms(delay))
        .unwrap_or(0)
}
