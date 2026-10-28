//! Shader arguments: a `#[shader_arg]` value (or `()`) converted to the `ArgValue` that
//! `kagari_shader::pack` lays out as the WGSL `Args` struct (`docs/dev/src/runtime/engine.md`,
//! "Shader effects").

use kagari_shader::ArgValue;
use kagari_vm::{Adt, Trap, TypeTable, Value};

use super::{StdTy, StdTypes, decode_anchor, decode_angle, decode_color, decode_duration_ms};
use super::{decode_offset, decode_point, decode_size, host_trap};
use crate::anim::{color_premul_linear, decimal_f32};

/// `v` as shader arguments: `bool` → `U32`, integers → `I32` (else `U32`), `Decimal`/`Rat` →
/// `F32`, `Color` → premultiplied linear `Vec4`, `Point`/`Offset`/`Size`/`Anchor` → `Vec2`,
/// `Angle` → radians, `Duration` → seconds, fieldless enums → the variant index, `()` → `U32(0)`,
/// arrays element-wise and other structs member-wise.
pub(crate) fn arg_value(std: &StdTypes, types: &TypeTable, v: &Value) -> Result<ArgValue, Trap> {
    Ok(match v {
        Value::Bool(b) => ArgValue::U32(u32::from(*b)),
        Value::Int(n) => match (i32::try_from(*n), u32::try_from(*n)) {
            (Ok(n), _) => ArgValue::I32(n),
            (_, Ok(n)) => ArgValue::U32(n),
            _ => {
                return Err(host_trap(format!(
                    "shader argument {n} does not fit in 32 bits"
                )));
            }
        },
        Value::Decimal(d) => ArgValue::F32(decimal_f32(*d)),
        Value::Rat(r) => ArgValue::F32((r.numer() as f64 / r.denom() as f64) as f32),
        Value::Tuple(items) if items.is_empty() => ArgValue::U32(0),
        Value::Array(items) => ArgValue::Array(
            items
                .iter()
                .map(|item| arg_value(std, types, item))
                .collect::<Result<_, _>>()?,
        ),
        Value::Adt(adt) => adt_value(std, types, v, adt)?,
        other => {
            return Err(host_trap(format!(
                "shader arguments contain a {} value",
                other.tag()
            )));
        }
    })
}

fn adt_value(std: &StdTypes, types: &TypeTable, v: &Value, adt: &Adt) -> Result<ArgValue, Trap> {
    let is = |t: StdTy| adt.ty == std.ty(t);
    let vec2 = |[x, y]: [f32; 2]| ArgValue::Vec2([x, y]);
    let ints = |[x, y]: [i32; 2]| vec2([x as f32, y as f32]);
    Ok(if is(StdTy::Color) {
        ArgValue::Vec4(color_premul_linear(decode_color(std, v)?))
    } else if is(StdTy::Point) {
        ints(decode_point(std, v)?)
    } else if is(StdTy::Offset) {
        ints(decode_offset(std, v)?)
    } else if is(StdTy::Size) {
        let [w, h] = decode_size(std, v)?;
        vec2([w as f32, h as f32])
    } else if is(StdTy::Anchor) {
        vec2(decode_anchor(std, v)?.map(decimal_f32))
    } else if is(StdTy::Angle) {
        ArgValue::F32(decimal_f32(decode_angle(std, v)?).to_radians())
    } else if is(StdTy::Duration) {
        ArgValue::F32(decode_duration_ms(std, v)? as f32 / 1000.0)
    } else if types.is_enum(adt.ty) && adt.fields.is_empty() {
        ArgValue::U32(adt.variant)
    } else {
        ArgValue::Struct(
            adt.fields
                .iter()
                .map(|field| arg_value(std, types, field))
                .collect::<Result<_, _>>()?,
        )
    })
}
