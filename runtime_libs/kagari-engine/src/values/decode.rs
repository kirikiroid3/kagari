//! Typed decoders and constructors of std values, and the validity checks of animation values
//! and fractions (`docs/dev/src/std/contract.md`).

use std::cmp::Ordering;
use std::rc::Rc;

use kagari_shader::ArgValue;
use kagari_vm::{AssetId, Decimal, Program, StaticId, Trap, TrapKind, TypeTable, Value, value_eq};

use super::shader_args::arg_value;
use super::{StdTy, StdTypes, host_trap, tag_name};

/// `kagari_shader::MAX_ARGS_BYTES` as a length.
pub(crate) const MAX_ARGS_BYTES: usize = kagari_shader::MAX_ARGS_BYTES as usize;

/// Declares a field-less std enum mirrored as a Rust enum, in declaration order.
macro_rules! unit_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
        pub(crate) enum $name { $($variant),* }

        impl $name {
            const ALL: &'static [$name] = &[$($name::$variant),*];

            pub fn from_index(index: u32) -> Option<$name> {
                Self::ALL.get(usize::try_from(index).ok()?).copied()
            }
        }
    };
}

unit_enum!(Direction {
    Left,
    Right,
    Up,
    Down
});
unit_enum!(Plane {
    Background,
    Stage,
    Front,
    OverWindow
});
unit_enum!(Visibility { Shown, Hidden });
unit_enum!(Flip {
    None,
    Horizontal,
    Vertical,
    Both
});
unit_enum!(VAlign {
    Top,
    Middle,
    Bottom
});
unit_enum!(
    /// The 31 named easings of `std::screen::anim::Easing`, in declaration order.
    NamedEasing {
        Linear, InQuad, OutQuad, InOutQuad, InCubic, OutCubic, InOutCubic, InQuart, OutQuart,
        InOutQuart, InQuint, OutQuint, InOutQuint, InSine, OutSine, InOutSine, InExpo, OutExpo,
        InOutExpo, InCirc, OutCirc, InOutCirc, InBack, OutBack, InOutBack, InElastic,
        OutElastic, InOutElastic, InBounce, OutBounce, InOutBounce,
    }
);

/// `std::screen::anim::Easing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Easing {
    Named(NamedEasing),
    CubicBezier([Decimal; 4]),
    Steps(u32),
}

#[cfg(test)]
impl Easing {
    pub const LINEAR: Easing = Easing::Named(NamedEasing::Linear);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Tween {
    pub duration_ms: u32,
    pub easing: Easing,
    pub delay_ms: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum TransitionKind {
    Cut,
    Crossfade,
    Rule {
        mask: AssetId,
        softness: u8,
        reverse: bool,
    },
    Wipe(Direction),
    Blinds {
        direction: Direction,
        count: u32,
    },
    ThroughColor {
        color: [u8; 4],
        hold_ms: u32,
    },
    Masked {
        mask: EffectSpec,
        softness: u8,
        reverse: bool,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Transition {
    pub kind: TransitionKind,
    pub duration_ms: u32,
    pub easing: Easing,
    pub delay_ms: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct MotionKey {
    pub at_ms: u32,
    pub offset: [i32; 2],
    pub scale: Decimal,
    pub rotation: Decimal,
    pub easing: Easing,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Motion {
    pub keys: Vec<MotionKey>,
    pub repeat: u32,
    pub delay_ms: u32,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct LoopSpec {
    pub audio: AssetId,
    pub path: Rc<str>,
    pub loop_start_ms: u32,
    pub loop_end_ms: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct SoundSpec {
    pub audio: AssetId,
    pub path: Rc<str>,
    pub volume: Decimal,
    pub start_ms: u32,
    pub end_ms: Option<u32>,
    pub delay_ms: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct AudioFade {
    pub fade_out_ms: u32,
    pub fade_in_ms: u32,
    pub overlap: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct MovieSpec {
    pub video: AssetId,
    pub looping: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Volume {
    pub level: u8,
    pub muted: bool,
}

/// How a local motion's time runs (`std::screen::shader::LocalMotionBehavior`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LocalMotionBehavior {
    Periodic { after_ms: u32, period_ms: u32 },
    Stall { after_ms: u32 },
}

impl LocalMotionBehavior {
    /// The shader time `elapsed_ms` after the effect's origin: `Stall` holds at `after`;
    /// `Periodic` runs `after` once, then loops `period` (a zero period holds at `after`).
    pub fn map_ms(self, elapsed_ms: u64) -> u64 {
        match self {
            LocalMotionBehavior::Stall { after_ms } => elapsed_ms.min(u64::from(after_ms)),
            LocalMotionBehavior::Periodic {
                after_ms,
                period_ms,
            } => {
                let after = u64::from(after_ms);
                match elapsed_ms.checked_sub(after) {
                    None => elapsed_ms,
                    Some(rest) => {
                        after.saturating_add(rest.checked_rem(u64::from(period_ms)).unwrap_or(0))
                    }
                }
            }
        }
    }
}

/// A shader with its arguments: a `Filter`, `Mix`, `Mask` or `LocalMotion` value, or the shader
/// part of a `Paint` or `TransitionalMotion`. Equality compares the shader, the behaviour and
/// the argument value, as the compiler's screen model does.
#[derive(Clone, Debug)]
pub(crate) struct EffectSpec {
    pub shader: AssetId,
    /// The argument value as the program built it.
    pub args: Value,
    /// `args` converted for the shader.
    pub arg_value: Rc<ArgValue>,
    /// `arg_value` in WGSL storage layout; longer than `MAX_ARGS_BYTES` means not drawn.
    pub packed: Rc<[u8]>,
    /// `Some` exactly for a `LocalMotion`.
    pub behavior: Option<LocalMotionBehavior>,
}

impl PartialEq for EffectSpec {
    fn eq(&self, other: &EffectSpec) -> bool {
        self.shader == other.shader
            && self.behavior == other.behavior
            && value_eq(&self.args, &other.args).unwrap_or(false)
    }
}

/// `std::screen::shader::Paint`: generated content of `size` px.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaintSpec {
    pub texture: EffectSpec,
    pub size: [u32; 2],
    pub mask: Option<EffectSpec>,
}

/// `std::screen::shader::TransitionalMotion`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitionalMotionSpec {
    pub effect: EffectSpec,
    pub duration_ms: u32,
    pub easing: Easing,
    pub delay_ms: u32,
}

/// `std::screen::Pose`; `rotation` in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PoseSpec {
    pub position: [i32; 2],
    pub scale: Decimal,
    pub rotation: Decimal,
}

// ---------------------------------------------------------------------------------------------
// Primitives

fn field<'v>(fields: &'v [Value], index: usize, ty: &str) -> Result<&'v Value, Trap> {
    fields
        .get(index)
        .ok_or_else(|| host_trap(format!("malformed `{ty}`: field {index} is missing")))
}

fn int(v: &Value) -> Result<i128, Trap> {
    match v {
        Value::Int(n) => Ok(*n),
        other => Err(Trap::new(TrapKind::TagMismatch {
            expected: "Int",
            found: tag_name(other),
        })),
    }
}

/// An `Int` field converted to `T`.
fn int_as<T: TryFrom<i128>>(v: &Value, ty: &str, name: &str) -> Result<T, Trap> {
    let n = int(v)?;
    T::try_from(n).map_err(|_| host_trap(format!("`{ty}` field `{name}` out of range: {n}")))
}

/// An `Int` converted to `T`; out of range is a trap naming `ty` and its field `name`.
pub(crate) fn decode_int<T: TryFrom<i128>>(v: &Value, ty: &str, name: &str) -> Result<T, Trap> {
    int_as(v, ty, name)
}

fn int_field<T: TryFrom<i128>>(
    fields: &[Value],
    index: usize,
    ty: &str,
    name: &str,
) -> Result<T, Trap> {
    int_as(field(fields, index, ty)?, ty, name)
}

pub(crate) fn decode_decimal(v: &Value) -> Result<Decimal, Trap> {
    match v {
        Value::Decimal(d) => Ok(*d),
        other => Err(Trap::new(TrapKind::TagMismatch {
            expected: "Decimal",
            found: tag_name(other),
        })),
    }
}

pub(crate) fn decode_bool(v: &Value) -> Result<bool, Trap> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(Trap::new(TrapKind::TagMismatch {
            expected: "Bool",
            found: tag_name(other),
        })),
    }
}

pub(crate) fn decode_asset(v: &Value) -> Result<AssetId, Trap> {
    match v {
        Value::Asset(id) => Ok(*id),
        other => Err(Trap::new(TrapKind::TagMismatch {
            expected: "Asset",
            found: tag_name(other),
        })),
    }
}

pub(crate) fn decode_handle(v: &Value) -> Result<StaticId, Trap> {
    match v {
        Value::Handle(id) => Ok(*id),
        other => Err(Trap::new(TrapKind::TagMismatch {
            expected: "Handle",
            found: tag_name(other),
        })),
    }
}

/// The path of asset `id`; `""` out of range.
pub(crate) fn asset_path(program: &Program, id: AssetId) -> Rc<str> {
    let path = usize::try_from(id.0)
        .ok()
        .and_then(|index| program.module().assets.get(index))
        .map_or("", |asset| program.string(asset.path));
    Rc::from(path)
}

fn unit_variant<E>(
    std: &StdTypes,
    v: &Value,
    t: StdTy,
    from_index: fn(u32) -> Option<E>,
) -> Result<E, Trap> {
    let (variant, fields) = std.variant(v, t)?;
    match from_index(variant) {
        Some(e) if fields.is_empty() => Ok(e),
        _ => Err(host_trap(format!(
            "malformed `{}`: variant {variant}",
            super::STD_ADTS[t as usize].path
        ))),
    }
}

fn option_with<T>(
    std: &StdTypes,
    v: &Value,
    f: impl FnOnce(&Value) -> Result<T, Trap>,
) -> Result<Option<T>, Trap> {
    std.option(v)?.map(f).transpose()
}

// ---------------------------------------------------------------------------------------------
// Decoders

pub(crate) fn decode_duration_ms(std: &StdTypes, v: &Value) -> Result<u32, Trap> {
    int_field(std.fields(v, StdTy::Duration)?, 0, "Duration", "ms")
}

pub(crate) fn decode_point(std: &StdTypes, v: &Value) -> Result<[i32; 2], Trap> {
    let f = std.fields(v, StdTy::Point)?;
    Ok([
        int_field(f, 0, "Point", "x")?,
        int_field(f, 1, "Point", "y")?,
    ])
}

pub(crate) fn decode_offset(std: &StdTypes, v: &Value) -> Result<[i32; 2], Trap> {
    let f = std.fields(v, StdTy::Offset)?;
    Ok([
        int_field(f, 0, "Offset", "dx")?,
        int_field(f, 1, "Offset", "dy")?,
    ])
}

pub(crate) fn decode_size(std: &StdTypes, v: &Value) -> Result<[u32; 2], Trap> {
    let f = std.fields(v, StdTy::Size)?;
    Ok([int_field(f, 0, "Size", "w")?, int_field(f, 1, "Size", "h")?])
}

pub(crate) fn decode_anchor(std: &StdTypes, v: &Value) -> Result<[Decimal; 2], Trap> {
    let f = std.fields(v, StdTy::Anchor)?;
    Ok([
        decode_decimal(field(f, 0, "Anchor")?)?,
        decode_decimal(field(f, 1, "Anchor")?)?,
    ])
}

/// An `Angle`, in degrees.
pub(crate) fn decode_angle(std: &StdTypes, v: &Value) -> Result<Decimal, Trap> {
    decode_decimal(field(std.fields(v, StdTy::Angle)?, 0, "Angle")?)
}

pub(crate) fn decode_color(std: &StdTypes, v: &Value) -> Result<[u8; 4], Trap> {
    let f = std.fields(v, StdTy::Color)?;
    Ok([
        int_field(f, 0, "Color", "r")?,
        int_field(f, 1, "Color", "g")?,
        int_field(f, 2, "Color", "b")?,
        int_field(f, 3, "Color", "a")?,
    ])
}

pub(crate) fn decode_entity_id(std: &StdTypes, v: &Value) -> Result<u32, Trap> {
    int_field(std.fields(v, StdTy::EntityId)?, 0, "EntityId", "0")
}

pub(crate) fn decode_part(std: &StdTypes, v: &Value) -> Result<u8, Trap> {
    int_field(std.fields(v, StdTy::Part)?, 0, "Part", "0")
}

pub(crate) fn decode_mark(std: &StdTypes, v: &Value) -> Result<u8, Trap> {
    int_field(std.fields(v, StdTy::Mark)?, 0, "Mark", "0")
}

pub(crate) fn decode_direction(std: &StdTypes, v: &Value) -> Result<Direction, Trap> {
    unit_variant(std, v, StdTy::Direction, Direction::from_index)
}

pub(crate) fn decode_plane(std: &StdTypes, v: &Value) -> Result<Plane, Trap> {
    unit_variant(std, v, StdTy::Plane, Plane::from_index)
}

pub(crate) fn decode_visibility(std: &StdTypes, v: &Value) -> Result<Visibility, Trap> {
    unit_variant(std, v, StdTy::Visibility, Visibility::from_index)
}

pub(crate) fn decode_flip(std: &StdTypes, v: &Value) -> Result<Flip, Trap> {
    unit_variant(std, v, StdTy::Flip, Flip::from_index)
}

pub(crate) fn decode_valign(std: &StdTypes, v: &Value) -> Result<VAlign, Trap> {
    unit_variant(std, v, StdTy::VAlign, VAlign::from_index)
}

pub(crate) fn decode_easing(std: &StdTypes, v: &Value) -> Result<Easing, Trap> {
    let (variant, fields) = std.variant(v, StdTy::Easing)?;
    match (variant, fields) {
        (31, [x1, y1, x2, y2]) => Ok(Easing::CubicBezier([
            decode_decimal(x1)?,
            decode_decimal(y1)?,
            decode_decimal(x2)?,
            decode_decimal(y2)?,
        ])),
        (32, [n]) => Ok(Easing::Steps(int_as(n, "Easing", "Steps")?)),
        (variant, []) => NamedEasing::from_index(variant)
            .map(Easing::Named)
            .ok_or_else(|| host_trap(format!("malformed `Easing`: variant {variant}"))),
        _ => Err(host_trap(format!("malformed `Easing`: variant {variant}"))),
    }
}

pub(crate) fn decode_tween(std: &StdTypes, v: &Value) -> Result<Tween, Trap> {
    let f = std.fields(v, StdTy::Tween)?;
    Ok(Tween {
        duration_ms: decode_duration_ms(std, field(f, 0, "Tween")?)?,
        easing: decode_easing(std, field(f, 1, "Tween")?)?,
        delay_ms: decode_duration_ms(std, field(f, 2, "Tween")?)?,
    })
}

fn decode_transition_kind(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
) -> Result<TransitionKind, Trap> {
    let (variant, f) = std.variant(v, StdTy::TransitionKind)?;
    let ty = "TransitionKind";
    Ok(match (variant, f) {
        (0, []) => TransitionKind::Cut,
        (1, []) => TransitionKind::Crossfade,
        (2, [mask, softness, reverse]) => TransitionKind::Rule {
            mask: decode_asset(mask)?,
            softness: int_as(softness, ty, "softness")?,
            reverse: decode_bool(reverse)?,
        },
        (3, [direction]) => TransitionKind::Wipe(decode_direction(std, direction)?),
        (4, [direction, count]) => TransitionKind::Blinds {
            direction: decode_direction(std, direction)?,
            count: int_as(count, ty, "count")?,
        },
        (5, [color, hold]) => TransitionKind::ThroughColor {
            color: decode_color(std, color)?,
            hold_ms: decode_duration_ms(std, hold)?,
        },
        (6, [mask, softness, reverse]) => TransitionKind::Masked {
            mask: decode_effect(std, types, mask, StdTy::Mask)?,
            softness: int_as(softness, ty, "softness")?,
            reverse: decode_bool(reverse)?,
        },
        _ => return Err(host_trap(format!("malformed `{ty}`: variant {variant}"))),
    })
}

pub(crate) fn decode_transition(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
) -> Result<Transition, Trap> {
    let f = std.fields(v, StdTy::Transition)?;
    Ok(Transition {
        kind: decode_transition_kind(std, types, field(f, 0, "Transition")?)?,
        duration_ms: decode_duration_ms(std, field(f, 1, "Transition")?)?,
        easing: decode_easing(std, field(f, 2, "Transition")?)?,
        delay_ms: decode_duration_ms(std, field(f, 3, "Transition")?)?,
    })
}

/// A shader and its arguments (`shader`, `args` of an effect value), converted and packed.
fn effect(
    std: &StdTypes,
    types: &TypeTable,
    shader: &Value,
    args: &Value,
    behavior: Option<LocalMotionBehavior>,
) -> Result<EffectSpec, Trap> {
    let arg_value = arg_value(std, types, args)?;
    let packed: Rc<[u8]> = Rc::from(kagari_shader::pack(&arg_value));
    if packed.len() > MAX_ARGS_BYTES {
        log::warn!(
            "shader arguments of {} bytes exceed the limit of {MAX_ARGS_BYTES}; the effect is not drawn",
            packed.len()
        );
    }
    Ok(EffectSpec {
        shader: decode_asset(shader)?,
        args: args.clone(),
        arg_value: Rc::new(arg_value),
        packed,
        behavior,
    })
}

/// A `Filter`, `Mix` or `Mask` (`t`).
pub(crate) fn decode_effect(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
    t: StdTy,
) -> Result<EffectSpec, Trap> {
    let f = std.fields(v, t)?;
    let ty = super::STD_ADTS[t as usize].path;
    effect(std, types, field(f, 0, ty)?, field(f, 1, ty)?, None)
}

fn decode_behavior(std: &StdTypes, v: &Value) -> Result<LocalMotionBehavior, Trap> {
    match std.variant(v, StdTy::LocalMotionBehavior)? {
        (0, [after, period]) => Ok(LocalMotionBehavior::Periodic {
            after_ms: decode_duration_ms(std, after)?,
            period_ms: decode_duration_ms(std, period)?,
        }),
        (1, [after]) => Ok(LocalMotionBehavior::Stall {
            after_ms: decode_duration_ms(std, after)?,
        }),
        (variant, _) => Err(host_trap(format!(
            "malformed `LocalMotionBehavior`: variant {variant}"
        ))),
    }
}

pub(crate) fn decode_local_motion(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
) -> Result<EffectSpec, Trap> {
    let f = std.fields(v, StdTy::LocalMotion)?;
    let ty = "LocalMotion";
    let behavior = decode_behavior(std, field(f, 2, ty)?)?;
    effect(
        std,
        types,
        field(f, 0, ty)?,
        field(f, 1, ty)?,
        Some(behavior),
    )
}

pub(crate) fn decode_paint(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
) -> Result<PaintSpec, Trap> {
    let f = std.fields(v, StdTy::ShaderPaint)?;
    let ty = "Paint";
    Ok(PaintSpec {
        texture: effect(std, types, field(f, 0, ty)?, field(f, 1, ty)?, None)?,
        size: decode_size(std, field(f, 2, ty)?)?,
        mask: option_with(std, field(f, 3, ty)?, |m| {
            decode_effect(std, types, m, StdTy::Mask)
        })?,
    })
}

pub(crate) fn decode_transitional_motion(
    std: &StdTypes,
    types: &TypeTable,
    v: &Value,
) -> Result<TransitionalMotionSpec, Trap> {
    let f = std.fields(v, StdTy::TransitionalMotion)?;
    let ty = "TransitionalMotion";
    Ok(TransitionalMotionSpec {
        effect: effect(std, types, field(f, 0, ty)?, field(f, 1, ty)?, None)?,
        duration_ms: decode_duration_ms(std, field(f, 2, ty)?)?,
        easing: decode_easing(std, field(f, 3, ty)?)?,
        delay_ms: decode_duration_ms(std, field(f, 4, ty)?)?,
    })
}

pub(crate) fn decode_pose(std: &StdTypes, v: &Value) -> Result<PoseSpec, Trap> {
    let f = std.fields(v, StdTy::Pose)?;
    Ok(PoseSpec {
        position: decode_point(std, field(f, 0, "Pose")?)?,
        scale: decode_decimal(field(f, 1, "Pose")?)?,
        rotation: decode_angle(std, field(f, 2, "Pose")?)?,
    })
}

fn decode_motion_key(std: &StdTypes, v: &Value) -> Result<MotionKey, Trap> {
    let f = std.fields(v, StdTy::MotionKey)?;
    Ok(MotionKey {
        at_ms: decode_duration_ms(std, field(f, 0, "MotionKey")?)?,
        offset: decode_offset(std, field(f, 1, "MotionKey")?)?,
        scale: decode_decimal(field(f, 2, "MotionKey")?)?,
        rotation: decode_angle(std, field(f, 3, "MotionKey")?)?,
        easing: decode_easing(std, field(f, 4, "MotionKey")?)?,
    })
}

pub(crate) fn decode_motion(std: &StdTypes, v: &Value) -> Result<Motion, Trap> {
    let f = std.fields(v, StdTy::Motion)?;
    let keys = match field(f, 0, "Motion")? {
        Value::Vec(items) | Value::Array(items) => items
            .iter()
            .map(|key| decode_motion_key(std, key))
            .collect::<Result<Vec<_>, _>>()?,
        other => {
            return Err(Trap::new(TrapKind::TagMismatch {
                expected: "Vec",
                found: tag_name(other),
            }));
        }
    };
    Ok(Motion {
        keys,
        repeat: int_field(f, 1, "Motion", "repeat")?,
        delay_ms: decode_duration_ms(std, field(f, 2, "Motion")?)?,
    })
}

pub(crate) fn decode_loop_spec(
    std: &StdTypes,
    program: &Program,
    v: &Value,
) -> Result<LoopSpec, Trap> {
    let f = std.fields(v, StdTy::Loop)?;
    let audio = decode_asset(field(f, 0, "Loop")?)?;
    Ok(LoopSpec {
        audio,
        path: asset_path(program, audio),
        loop_start_ms: decode_duration_ms(std, field(f, 1, "Loop")?)?,
        loop_end_ms: option_with(std, field(f, 2, "Loop")?, |d| decode_duration_ms(std, d))?,
    })
}

pub(crate) fn decode_sound(
    std: &StdTypes,
    program: &Program,
    v: &Value,
) -> Result<SoundSpec, Trap> {
    let f = std.fields(v, StdTy::Sound)?;
    let audio = decode_asset(field(f, 0, "Sound")?)?;
    Ok(SoundSpec {
        audio,
        path: asset_path(program, audio),
        volume: decode_decimal(field(f, 1, "Sound")?)?,
        start_ms: decode_duration_ms(std, field(f, 2, "Sound")?)?,
        end_ms: option_with(std, field(f, 3, "Sound")?, |d| decode_duration_ms(std, d))?,
        delay_ms: decode_duration_ms(std, field(f, 4, "Sound")?)?,
    })
}

pub(crate) fn decode_audio_fade(std: &StdTypes, v: &Value) -> Result<AudioFade, Trap> {
    let f = std.fields(v, StdTy::AudioFade)?;
    Ok(AudioFade {
        fade_out_ms: decode_duration_ms(std, field(f, 0, "AudioFade")?)?,
        fade_in_ms: decode_duration_ms(std, field(f, 1, "AudioFade")?)?,
        overlap: decode_bool(field(f, 2, "AudioFade")?)?,
    })
}

pub(crate) fn decode_movie(std: &StdTypes, v: &Value) -> Result<MovieSpec, Trap> {
    let f = std.fields(v, StdTy::Movie)?;
    Ok(MovieSpec {
        video: decode_asset(field(f, 0, "Movie")?)?,
        looping: decode_bool(field(f, 1, "Movie")?)?,
    })
}

pub(crate) fn decode_volume(std: &StdTypes, v: &Value) -> Result<Volume, Trap> {
    let f = std.fields(v, StdTy::Volume)?;
    Ok(Volume {
        level: int_field(f, 0, "Volume", "level")?,
        muted: decode_bool(field(f, 1, "Volume")?)?,
    })
}

// ---------------------------------------------------------------------------------------------
// Constructors

fn ints<const N: usize, T: Into<i128>>(values: [T; N]) -> Vec<Value> {
    values.into_iter().map(|v| Value::Int(v.into())).collect()
}

pub(crate) fn point_value(std: &StdTypes, p: [i32; 2]) -> Value {
    std.make(StdTy::Point, 0, ints(p))
}

pub(crate) fn offset_value(std: &StdTypes, p: [i32; 2]) -> Value {
    std.make(StdTy::Offset, 0, ints(p))
}

pub(crate) fn angle_value(std: &StdTypes, degrees: Decimal) -> Value {
    std.make(StdTy::Angle, 0, vec![Value::Decimal(degrees)])
}

pub(crate) fn color_value(std: &StdTypes, c: [u8; 4]) -> Value {
    std.make(StdTy::Color, 0, ints(c))
}

pub(crate) fn visibility_value(std: &StdTypes, v: Visibility) -> Value {
    std.unit(StdTy::Visibility, v as u32)
}

pub(crate) fn entity_id_value(std: &StdTypes, id: u32) -> Value {
    std.make(StdTy::EntityId, 0, ints([id]))
}

pub(crate) fn duration_value(std: &StdTypes, ms: u32) -> Value {
    std.make(StdTy::Duration, 0, ints([ms]))
}

pub(crate) fn loop_value(std: &StdTypes, l: &LoopSpec) -> Value {
    let end = match l.loop_end_ms {
        Some(ms) => std.some(duration_value(std, ms)),
        None => std.none(),
    };
    std.make(
        StdTy::Loop,
        0,
        vec![
            Value::Asset(l.audio),
            duration_value(std, l.loop_start_ms),
            end,
        ],
    )
}

/// The std value of an effect: `Filter`, `Mix` or `Mask` (`t`), or a `LocalMotion` when the
/// effect has a behaviour.
pub(crate) fn effect_value(std: &StdTypes, t: StdTy, e: &EffectSpec) -> Value {
    let mut fields = vec![Value::Asset(e.shader), e.args.clone()];
    match e.behavior {
        Some(behavior) => {
            fields.push(behavior_value(std, behavior));
            std.make(StdTy::LocalMotion, 0, fields)
        }
        None => std.make(t, 0, fields),
    }
}

fn behavior_value(std: &StdTypes, b: LocalMotionBehavior) -> Value {
    match b {
        LocalMotionBehavior::Periodic {
            after_ms,
            period_ms,
        } => std.make(
            StdTy::LocalMotionBehavior,
            0,
            vec![
                duration_value(std, after_ms),
                duration_value(std, period_ms),
            ],
        ),
        LocalMotionBehavior::Stall { after_ms } => std.make(
            StdTy::LocalMotionBehavior,
            1,
            vec![duration_value(std, after_ms)],
        ),
    }
}

pub(crate) fn option_asset_value(std: &StdTypes, a: Option<AssetId>) -> Value {
    match a {
        Some(id) => std.some(Value::Asset(id)),
        None => std.none(),
    }
}

// ---------------------------------------------------------------------------------------------
// Validity checks

/// `value` is in `0.0..=1.0`.
pub(crate) fn check_fraction(value: Decimal, what: &str) -> Result<(), Trap> {
    if value.is_fraction() {
        Ok(())
    } else {
        Err(host_trap(format!(
            "{what} must be in 0.0..=1.0, found {value}"
        )))
    }
}

/// Both coordinates of an `Anchor` are fractions.
pub(crate) fn check_anchor(anchor: [Decimal; 2]) -> Result<(), Trap> {
    check_fraction(anchor[0], "the anchor's x")?;
    check_fraction(anchor[1], "the anchor's y")
}

/// `CubicBezier` x1 and x2 in `0.0..=1.0`, `Steps(n)` with `n > 0`.
pub(crate) fn check_easing(easing: &Easing) -> Result<(), Trap> {
    match easing {
        Easing::CubicBezier([x1, _, x2, _]) => {
            for (name, x) in [("x1", x1), ("x2", x2)] {
                if !x.is_fraction() {
                    return Err(host_trap(format!(
                        "`CubicBezier` {name} must be in 0.0..=1.0, found {x}"
                    )));
                }
            }
            Ok(())
        }
        Easing::Steps(0) => Err(host_trap("`Steps(0)`: the number of steps must not be 0")),
        _ => Ok(()),
    }
}

/// The identity rule of a `Motion`, plus the easing of every key.
pub(crate) fn check_motion(motion: &Motion) -> Result<(), Trap> {
    let Some(last) = motion.keys.last() else {
        return Err(host_trap("a motion needs at least one key"));
    };
    let mut previous: Option<u32> = None;
    for key in &motion.keys {
        match previous {
            None if key.at_ms == 0 => {
                return Err(host_trap("the first motion key must be after 0 ms"));
            }
            Some(previous) if key.at_ms <= previous => {
                return Err(host_trap(format!(
                    "motion key at {} ms is not after the previous key at {previous} ms",
                    key.at_ms
                )));
            }
            _ => {}
        }
        previous = Some(key.at_ms);
        check_easing(&key.easing)?;
    }
    let identity = last.offset == [0, 0]
        && last.scale.checked_cmp(Decimal::ONE) == Some(Ordering::Equal)
        && last.rotation.checked_cmp(Decimal::ZERO) == Some(Ordering::Equal);
    if !identity {
        return Err(host_trap(
            "the last motion key must be the identity: offset (0, 0), scale 1.0, rotation 0°",
        ));
    }
    if motion.repeat == 0 {
        return Err(host_trap(
            "a motion must repeat at least once, found `repeat(0)`",
        ));
    }
    Ok(())
}
