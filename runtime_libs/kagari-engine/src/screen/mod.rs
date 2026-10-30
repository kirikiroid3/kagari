//! `std::screen`: the screen model, its host parts and imports, the step runtime with its
//! timeline, and sampling.

pub(crate) mod draw;
pub(crate) mod model;
pub(crate) mod parts;
mod remap;
pub(crate) mod runtime;
pub(crate) mod sample;
#[cfg(test)]
mod tests;

use std::rc::Rc;

use kagari_vm::{HostObj, Ref, Text, Trap, TypeTable, Value, Vm};

use crate::HostCtx;
use crate::values::{
    self, StdTy, StdTypes, angle_value, check_anchor, color_value, decode_anchor, decode_angle,
    decode_asset, decode_color, decode_decimal, decode_duration_ms, decode_effect,
    decode_entity_id, decode_flip, decode_handle, decode_int, decode_local_motion, decode_mark,
    decode_motion, decode_movie, decode_offset, decode_paint, decode_part, decode_plane,
    decode_point, decode_pose, decode_size, decode_sound, decode_transition,
    decode_transitional_motion, decode_tween, decode_valign, decode_visibility, effect_value,
    entity_id_value, host_trap, offset_value, option_asset_value, point_value, visibility_value,
};
use model::{
    Animation, ChannelId, EntityProp, LayerContent, LayerTarget, PartTarget, PendingChange,
    PropTarget, PropValue, ScreenObj, Target,
};
use parts::{
    PendingObj, Receiver, ScreenPart, bad_receiver, part_ref, pending_ref, receiver, with_screen,
};

import_table!(
    /// The `#[builtin]` methods of `std::screen` (the audio channel's are `AudioImport`).
    ScreenImport {
        ScreenEmpty = "std::screen::Screen::empty",
        ScreenEntity = "std::screen::Screen::entity",
        ScreenContains = "std::screen::Screen::contains",
        ScreenEntities = "std::screen::Screen::entities",
        ScreenDespawn = "std::screen::Screen::despawn",
        ScreenHud = "std::screen::Screen::hud",
        ScreenChoiceMenu = "std::screen::Screen::choice_menu",
        ScreenCamera = "std::screen::Screen::camera",
        ScreenFilter = "std::screen::Screen::filter",
        ScreenMusic = "std::screen::Screen::music",
        ScreenAmbience = "std::screen::Screen::ambience",
        ScreenPlay = "std::screen::Screen::play",
        ScreenStopSounds = "std::screen::Screen::stop_sounds",
        ScreenReplayBgm = "std::screen::Screen::replay_bgm",
        ScreenWait = "std::screen::Screen::wait",
        ScreenBarrier = "std::screen::Screen::barrier",
        ScreenAtMark = "std::screen::Screen::at_mark",
        ScreenAtEntry = "std::screen::Screen::at_entry",
        ScreenBeginText = "std::screen::Screen::begin_text",
        EntityPosition = "std::screen::Entity::position",
        EntityScale = "std::screen::Entity::scale",
        EntityRotation = "std::screen::Entity::rotation",
        EntityOpacity = "std::screen::Entity::opacity",
        EntityFilter = "std::screen::Entity::filter",
        EntityMix = "std::screen::Entity::mix",
        EntityMask = "std::screen::Entity::mask",
        EntityLocalMotion = "std::screen::Entity::local_motion",
        EntityTravel = "std::screen::Entity::travel",
        EntityFill = "std::screen::Entity::fill",
        EntityPart = "std::screen::Entity::part",
        EntityLabel = "std::screen::Entity::label",
        EntityMovie = "std::screen::Entity::movie",
        EntitySetPlane = "std::screen::Entity::set_plane",
        EntitySetZ = "std::screen::Entity::set_z",
        EntitySetAnchor = "std::screen::Entity::set_anchor",
        EntitySetFlip = "std::screen::Entity::set_flip",
        EntitySetFillSize = "std::screen::Entity::set_fill_size",
        EntityGate = "std::screen::Entity::gate",
        EntityApplyAnimation = "std::screen::Entity::apply_animation",
        PropGet = "std::screen::Prop::get",
        PropChange = "std::screen::Prop::change",
        ChangeApplyAnimation = "std::screen::Change::apply_animation",
        ImageLayerGet = "std::screen::ImageLayer::get",
        ImageLayerChange = "std::screen::ImageLayer::change",
        ImageLayerClear = "std::screen::ImageLayer::clear",
        ImageLayerPaint = "std::screen::ImageLayer::paint",
        LabelLayerChange = "std::screen::LabelLayer::change",
        LabelLayerClear = "std::screen::LabelLayer::clear",
        LabelLayerSetStyle = "std::screen::LabelLayer::set_style",
        LabelLayerSetWidth = "std::screen::LabelLayer::set_width",
        MovieLayerChange = "std::screen::MovieLayer::change",
        MovieLayerClear = "std::screen::MovieLayer::clear",
        SwapApplyAnimation = "std::screen::Swap::apply_animation",
        HudTextWindow = "std::screen::Hud::text_window",
        HudQuickMenu = "std::screen::Hud::quick_menu",
        HudSideImage = "std::screen::Hud::side_image",
        ChoiceMenuDisabledArm = "std::screen::ChoiceMenu::disabled_arm",
        ChoiceMenuSetAlign = "std::screen::ChoiceMenu::set_align",
        CameraOffset = "std::screen::Camera::offset",
        CameraZoom = "std::screen::Camera::zoom",
        CameraRotation = "std::screen::Camera::rotation",
        CameraApplyAnimation = "std::screen::Camera::apply_animation",
    }
);

/// The value a render returned, which must be a `Screen`.
pub(crate) fn expect_screen(v: &Value) -> Result<HostObj, Trap> {
    match v {
        Value::Host(obj) if obj.downcast::<ScreenObj>().is_some() => Ok(obj.clone()),
        _ => Err(host_trap("a render must return a `Screen`")),
    }
}

fn unit() -> Value {
    Value::unit()
}

/// The screen reference of a `&mut self` receiver.
pub(crate) fn screen_of(rcv: Receiver, path: &str) -> Result<Ref, Trap> {
    match rcv {
        Receiver::Screen(r) => Ok(r),
        _ => Err(bad_receiver(path)),
    }
}

/// The part a receiver denotes.
pub(crate) fn part_of(rcv: Receiver, path: &str) -> Result<ScreenPart, Trap> {
    match rcv {
        Receiver::Part(part) => Ok(part),
        _ => Err(bad_receiver(path)),
    }
}

/// The pending change a receiver denotes.
pub(crate) fn pending_of(rcv: Receiver, path: &str) -> Result<PendingObj, Trap> {
    match rcv {
        Receiver::Pending(pending) => Ok(pending),
        _ => Err(bad_receiver(path)),
    }
}

/// Runs `f` on the screen a receiver reads; a screen passed by value is read from a copy.
pub(crate) fn read_screen<R>(
    vm: &Vm,
    rcv: &Receiver,
    f: impl FnOnce(&mut ScreenObj) -> Result<R, Trap>,
) -> Result<R, Trap> {
    match rcv {
        Receiver::Screen(r) => with_screen(vm, r, f),
        Receiver::Part(ScreenPart { screen, .. })
        | Receiver::Pending(PendingObj { screen, .. }) => with_screen(vm, screen, f),
        Receiver::ScreenValue(obj) => match obj.downcast::<ScreenObj>() {
            Some(screen) => f(&mut screen.clone()),
            None => Err(host_trap("the receiver is not a `Screen`")),
        },
    }
}

/// The std type of an effect property's value.
fn effect_ty(target: PropTarget) -> StdTy {
    match target {
        PropTarget::Entity(_, EntityProp::Mix) => StdTy::Mix,
        PropTarget::Entity(_, EntityProp::Mask) => StdTy::Mask,
        PropTarget::Entity(_, EntityProp::LocalMotion) => StdTy::LocalMotion,
        _ => StdTy::Filter,
    }
}

/// The std value of a property.
pub(crate) fn prop_value(std: &StdTypes, target: PropTarget, value: PropValue) -> Value {
    match value {
        PropValue::Point(p) => point_value(std, p),
        PropValue::Offset(o) => offset_value(std, o),
        PropValue::Decimal(d) => Value::Decimal(d),
        PropValue::Angle(d) => angle_value(std, d),
        PropValue::Color(c) => color_value(std, c),
        PropValue::Effect(Some(e)) => std.some(effect_value(std, effect_ty(target), &e)),
        PropValue::Effect(None) => std.none(),
        PropValue::Visibility(v) => visibility_value(std, v),
    }
}

/// A trap while converting an import's argument, prefixed with the import's path.
fn in_import(path: &str, trap: Trap) -> Trap {
    match trap.kind {
        kagari_vm::TrapKind::Host(message) => host_trap(format!("`{path}`: {message}")),
        _ => trap,
    }
}

/// A `Prop::change` argument, decoded by the property's type; effect arguments are converted
/// for the shader now.
fn decode_prop(
    std: &StdTypes,
    types: &TypeTable,
    target: PropTarget,
    v: &Value,
) -> Result<PropValue, Trap> {
    Ok(match target {
        PropTarget::Entity(_, EntityProp::Position) => PropValue::Point(decode_point(std, v)?),
        PropTarget::Entity(_, EntityProp::Scale | EntityProp::Opacity)
        | PropTarget::CameraZoom
        | PropTarget::ChannelVolume(_) => PropValue::Decimal(decode_decimal(v)?),
        PropTarget::Entity(_, EntityProp::Rotation) | PropTarget::CameraRotation => {
            PropValue::Angle(decode_angle(std, v)?)
        }
        PropTarget::Entity(_, EntityProp::LocalMotion) => PropValue::Effect(
            std.option(v)?
                .map(|m| decode_local_motion(std, types, m).map(Rc::new))
                .transpose()?,
        ),
        PropTarget::Entity(_, EntityProp::Filter | EntityProp::Mix | EntityProp::Mask)
        | PropTarget::ScreenFilter => PropValue::Effect(
            std.option(v)?
                .map(|e| decode_effect(std, types, e, effect_ty(target)).map(Rc::new))
                .transpose()?,
        ),
        PropTarget::Entity(_, EntityProp::Fill) => PropValue::Color(decode_color(std, v)?),
        PropTarget::HudTextWindow | PropTarget::HudQuickMenu => {
            PropValue::Visibility(decode_visibility(std, v)?)
        }
        PropTarget::CameraOffset => PropValue::Offset(decode_offset(std, v)?),
    })
}

/// A text value argument.
pub(crate) fn decode_text(v: &Value) -> Result<Text, Trap> {
    match v {
        Value::Text(text) => Ok(text.clone()),
        other => Err(kagari_vm::Trap::new(kagari_vm::TrapKind::TagMismatch {
            expected: "Text",
            found: other.tag(),
        })),
    }
}

/// A duplicate `Mark(n)` among the effects of `text` is an error.
pub(crate) fn check_marks(std: &StdTypes, text: &Text) -> Result<(), Trap> {
    fn walk(std: &StdTypes, text: &Text, seen: &mut Vec<u8>) -> Result<(), Trap> {
        match text {
            Text::Plain(_) => Ok(()),
            Text::Ruby(pair) => {
                walk(std, &pair.0, seen)?;
                walk(std, &pair.1, seen)
            }
            Text::MonoRuby(pair) => {
                walk(std, &pair.0, seen)?;
                pair.1
                    .iter()
                    .try_for_each(|reading| walk(std, reading, seen))
            }
            Text::Effect(pair) => {
                let fields = std.fields(&pair.1, StdTy::TextEffect)?;
                if let Some(mark) = fields.get(10)
                    && let Some(mark) = std.option(mark)?
                {
                    let n = decode_mark(std, mark)?;
                    if seen.contains(&n) {
                        return Err(host_trap(format!("duplicate text mark `Mark({n})`")));
                    }
                    seen.push(n);
                }
                walk(std, &pair.0, seen)
            }
            Text::Concat(items) => items.iter().try_for_each(|item| walk(std, item, seen)),
        }
    }
    walk(std, text, &mut Vec::new())
}

impl ScreenImport {
    /// Runs the import on `args` (the receiver first).
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use ScreenImport as I;
        let path = self.path();
        if self == I::ScreenEmpty {
            return Ok(Value::Host(HostObj::new(ScreenObj::empty())));
        }
        let program = ctx.state.program.clone();
        let std = &ctx.state.std;
        let a = |i: usize| values::arg(vm, path, &args, i);
        let types = vm.types();
        let rcv = receiver(vm, path, args.first())?;
        let part = |target: PartTarget| -> Result<Value, Trap> {
            let screen = screen_of_part(&rcv, path)?;
            Ok(part_ref(&screen, target))
        };
        match self {
            I::ScreenEmpty => Ok(Value::Host(HostObj::new(ScreenObj::empty()))),
            I::ScreenEntity => {
                let id = decode_entity_id(std, &a(1)?)?;
                let screen = screen_of(rcv, path)?;
                with_screen(vm, &screen, |s| {
                    s.entity_mut(id);
                    Ok(())
                })?;
                Ok(part_ref(&screen, PartTarget::Entity(id)))
            }
            I::ScreenContains => {
                let id = decode_entity_id(std, &a(1)?)?;
                Ok(Value::Bool(read_screen(vm, &rcv, |s| Ok(s.contains(id)))?))
            }
            I::ScreenEntities => {
                let plane = decode_plane(std, &a(1)?)?;
                let ids = read_screen(vm, &rcv, |s| Ok(s.entities_on(plane)))?;
                Ok(Value::Vec(Rc::new(
                    ids.into_iter().map(|id| entity_id_value(std, id)).collect(),
                )))
            }
            I::ScreenDespawn => {
                let id = decode_entity_id(std, &a(1)?)?;
                with_screen(vm, &screen_of(rcv, path)?, |s| {
                    s.despawn(id);
                    Ok(unit())
                })
            }
            I::ScreenHud => part(PartTarget::Hud),
            I::ScreenChoiceMenu => part(PartTarget::ChoiceMenu),
            I::ScreenCamera => part(PartTarget::Camera),
            I::ScreenFilter => part(PartTarget::Prop(PropTarget::ScreenFilter)),
            I::ScreenMusic => part(PartTarget::Channel(ChannelId::Music)),
            I::ScreenAmbience => {
                let n = decode_int::<i128>(&a(1)?, "Int", "channel")?;
                let channel = u8::try_from(n).ok().filter(|n| *n <= 3).ok_or_else(|| {
                    host_trap(format!("ambience channel {n} is out of range 0..=3"))
                })?;
                part(PartTarget::Channel(ChannelId::Ambience(channel)))
            }
            I::ScreenPlay => {
                let sound = decode_sound(std, &program, &a(1)?)?;
                let screen = screen_of(rcv, path)?;
                let length_ms = match sound.end_ms {
                    Some(end) => end.saturating_sub(sound.start_ms),
                    None => match ctx
                        .state
                        .assets
                        .audio_duration_ms(sound.audio, ctx.platform)?
                    {
                        Some(duration) => duration.saturating_sub(sound.start_ms),
                        None => {
                            log::warn!(
                                "audio `{}`: unknown duration; barrier ignores its length",
                                sound.path
                            );
                            0
                        }
                    },
                };
                with_screen(vm, &screen, |s| s.play(sound, length_ms))?;
                Ok(unit())
            }
            I::ScreenStopSounds => {
                let ms = decode_duration_ms(std, &a(1)?)?;
                with_screen(vm, &screen_of(rcv, path)?, |s| s.stop_sounds(ms))?;
                Ok(unit())
            }
            I::ScreenReplayBgm => {
                with_screen(vm, &screen_of(rcv, path)?, |s| s.replay_music())?;
                Ok(unit())
            }
            I::ScreenWait => {
                let ms = decode_duration_ms(std, &a(1)?)?;
                with_screen(vm, &screen_of(rcv, path)?, |s| s.wait(ms))?;
                Ok(unit())
            }
            I::ScreenBarrier => with_screen(vm, &screen_of(rcv, path)?, |s| {
                s.barrier();
                Ok(unit())
            }),
            I::ScreenAtMark => {
                let mark = decode_mark(std, &a(1)?)?;
                with_screen(vm, &screen_of(rcv, path)?, |s| {
                    s.at_mark(mark);
                    Ok(unit())
                })
            }
            I::ScreenAtEntry => with_screen(vm, &screen_of(rcv, path)?, |s| {
                s.at_entry();
                Ok(unit())
            }),
            I::ScreenBeginText => {
                with_screen(vm, &screen_of(rcv, path)?, |s| s.begin_text())?;
                Ok(unit())
            }
            I::EntityPosition
            | I::EntityScale
            | I::EntityRotation
            | I::EntityOpacity
            | I::EntityFilter
            | I::EntityMix
            | I::EntityMask
            | I::EntityLocalMotion
            | I::EntityFill
            | I::EntityPart
            | I::EntityLabel
            | I::EntityMovie => {
                let (screen, id) = entity_of(vm, rcv, path)?;
                let target = match self {
                    I::EntityPosition => {
                        PartTarget::Prop(PropTarget::Entity(id, EntityProp::Position))
                    }
                    I::EntityScale => PartTarget::Prop(PropTarget::Entity(id, EntityProp::Scale)),
                    I::EntityRotation => {
                        PartTarget::Prop(PropTarget::Entity(id, EntityProp::Rotation))
                    }
                    I::EntityOpacity => {
                        PartTarget::Prop(PropTarget::Entity(id, EntityProp::Opacity))
                    }
                    I::EntityFilter => PartTarget::Prop(PropTarget::Entity(id, EntityProp::Filter)),
                    I::EntityMix => PartTarget::Prop(PropTarget::Entity(id, EntityProp::Mix)),
                    I::EntityMask => PartTarget::Prop(PropTarget::Entity(id, EntityProp::Mask)),
                    I::EntityLocalMotion => {
                        PartTarget::Prop(PropTarget::Entity(id, EntityProp::LocalMotion))
                    }
                    I::EntityFill => PartTarget::Prop(PropTarget::Entity(id, EntityProp::Fill)),
                    I::EntityPart => {
                        PartTarget::Layer(LayerTarget::Part(id, decode_part(std, &a(1)?)?))
                    }
                    I::EntityLabel => PartTarget::Layer(LayerTarget::Label(id)),
                    _ => PartTarget::Layer(LayerTarget::Movie(id)),
                };
                Ok(part_ref(&screen, target))
            }
            I::EntitySetPlane
            | I::EntitySetZ
            | I::EntitySetAnchor
            | I::EntitySetFlip
            | I::EntitySetFillSize
            | I::EntityGate => {
                let value = a(1)?;
                let (screen, id) = entity_of(vm, rcv, path)?;
                enum Set {
                    Plane(values::Plane),
                    Z(i32),
                    Anchor([kagari_vm::Decimal; 2]),
                    Flip(values::Flip),
                    FillSize(Option<[u32; 2]>),
                    Gate(Option<kagari_vm::StaticId>),
                }
                let set = match self {
                    I::EntitySetPlane => Set::Plane(decode_plane(std, &value)?),
                    I::EntitySetZ => Set::Z(decode_int(&value, "Entity", "z")?),
                    I::EntitySetAnchor => {
                        let anchor = decode_anchor(std, &value)?;
                        check_anchor(anchor)?;
                        Set::Anchor(anchor)
                    }
                    I::EntitySetFlip => Set::Flip(decode_flip(std, &value)?),
                    I::EntitySetFillSize => Set::FillSize(
                        std.option(&value)?
                            .map(|size| decode_size(std, size))
                            .transpose()?,
                    ),
                    _ => Set::Gate(std.option(&value)?.map(decode_handle).transpose()?),
                };
                with_screen(vm, &screen, |s| {
                    let entity = s.entity_mut(id);
                    match set {
                        Set::Plane(plane) => entity.plane = plane,
                        Set::Z(z) => entity.z = z,
                        Set::Anchor(anchor) => entity.anchor = anchor,
                        Set::Flip(flip) => entity.flip = flip,
                        Set::FillSize(size) => entity.fill_size = size,
                        Set::Gate(gate) => entity.gate = gate,
                    }
                    Ok(unit())
                })
            }
            I::EntityTravel => {
                let to = decode_pose(std, &a(1)?)?;
                let motion = decode_transitional_motion(std, types, &a(2)?)
                    .map_err(|t| in_import(path, t))?;
                let (screen, id) = entity_of(vm, rcv, path)?;
                with_screen(vm, &screen, |s| s.travel(id, to, motion))?;
                Ok(unit())
            }
            I::EntityApplyAnimation | I::CameraApplyAnimation => {
                let motion = decode_motion(std, &a(1)?)?;
                let part = part_of(rcv, path)?;
                let target = match part.target {
                    PartTarget::Entity(id) => Target::Entity(id),
                    PartTarget::Camera => Target::Camera,
                    _ => return Err(bad_receiver(path)),
                };
                with_screen(vm, &part.screen, |s| s.apply_motion(target, motion))?;
                Ok(unit())
            }
            I::PropGet => {
                let part = part_of(rcv, path)?;
                let PartTarget::Prop(target) = part.target else {
                    return Err(bad_receiver(path));
                };
                let value = with_screen(vm, &part.screen, |s| Ok(s.prop(target)))?;
                Ok(prop_value(std, target, value))
            }
            I::PropChange => {
                let part = part_of(rcv, path)?;
                let PartTarget::Prop(target) = part.target else {
                    return Err(bad_receiver(path));
                };
                let value =
                    decode_prop(std, types, target, &a(1)?).map_err(|t| in_import(path, t))?;
                pending(&part.screen, PendingChange::Prop(target, value))
            }
            I::ChangeApplyAnimation => {
                let pending = pending_of(rcv, path)?;
                if !matches!(pending.change, PendingChange::Prop(..)) {
                    return Err(bad_receiver(path));
                }
                let tween = decode_tween(std, &a(1)?)?;
                commit(vm, pending, Animation::Tween(tween))
            }
            I::ImageLayerGet => {
                let part = part_of(rcv, path)?;
                let PartTarget::Layer(target) = part.target else {
                    return Err(bad_receiver(path));
                };
                let content = with_screen(vm, &part.screen, |s| Ok(s.layer(target)))?;
                let image = match content {
                    Some(LayerContent::Image(image)) => Some(image),
                    _ => None,
                };
                Ok(option_asset_value(std, image))
            }
            I::ImageLayerChange
            | I::ImageLayerClear
            | I::ImageLayerPaint
            | I::LabelLayerChange
            | I::LabelLayerClear
            | I::MovieLayerChange
            | I::MovieLayerClear => {
                let part = part_of(rcv, path)?;
                let PartTarget::Layer(target) = part.target else {
                    return Err(bad_receiver(path));
                };
                let content = match self {
                    I::ImageLayerChange => Some(LayerContent::Image(decode_asset(&a(1)?)?)),
                    I::ImageLayerPaint => Some(LayerContent::Paint(Rc::new(
                        decode_paint(std, types, &a(1)?).map_err(|t| in_import(path, t))?,
                    ))),
                    I::LabelLayerChange => {
                        let text = decode_text(&a(1)?)?;
                        check_marks(std, &text)?;
                        Some(LayerContent::Label(text))
                    }
                    I::MovieLayerChange => Some(LayerContent::Movie(decode_movie(std, &a(1)?)?)),
                    _ => None,
                };
                pending(&part.screen, PendingChange::Layer(target, content))
            }
            I::LabelLayerSetStyle | I::LabelLayerSetWidth => {
                let value = a(1)?;
                let part = part_of(rcv, path)?;
                let PartTarget::Layer(LayerTarget::Label(id)) = part.target else {
                    return Err(bad_receiver(path));
                };
                let (style, width) = if self == I::LabelLayerSetStyle {
                    std.fields(&value, StdTy::TextStyle)?;
                    (Some(value), None)
                } else {
                    let width = std
                        .option(&value)?
                        .map(|w| decode_int::<u32>(w, "Option<u32>", "width"))
                        .transpose()?;
                    (None, Some(width))
                };
                with_screen(vm, &part.screen, |s| {
                    let entity = s.entity_mut(id);
                    if let Some(style) = style {
                        entity.label_style = Some(style);
                    }
                    if let Some(width) = width {
                        entity.label_width = width;
                    }
                    Ok(unit())
                })
            }
            I::SwapApplyAnimation => {
                let pending = pending_of(rcv, path)?;
                if !matches!(pending.change, PendingChange::Layer(..)) {
                    return Err(bad_receiver(path));
                }
                let transition =
                    decode_transition(std, types, &a(1)?).map_err(|t| in_import(path, t))?;
                commit(vm, pending, Animation::Transition(transition))
            }
            I::HudTextWindow | I::HudQuickMenu | I::HudSideImage => {
                let part = part_of(rcv, path)?;
                if part.target != PartTarget::Hud {
                    return Err(bad_receiver(path));
                }
                let target = match self {
                    I::HudTextWindow => PartTarget::Prop(PropTarget::HudTextWindow),
                    I::HudQuickMenu => PartTarget::Prop(PropTarget::HudQuickMenu),
                    _ => PartTarget::Layer(LayerTarget::SideImage),
                };
                Ok(part_ref(&part.screen, target))
            }
            I::ChoiceMenuDisabledArm => {
                let at = decode_int::<u32>(&a(1)?, "ChoiceMenu", "at")?;
                let text = decode_text(&a(2)?)?;
                let part = part_of(rcv, path)?;
                if part.target != PartTarget::ChoiceMenu {
                    return Err(bad_receiver(path));
                }
                with_screen(vm, &part.screen, |s| {
                    s.step.disabled_arms.push((at, text));
                    Ok(unit())
                })
            }
            I::ChoiceMenuSetAlign => {
                let align = decode_valign(std, &a(1)?)?;
                let part = part_of(rcv, path)?;
                if part.target != PartTarget::ChoiceMenu {
                    return Err(bad_receiver(path));
                }
                with_screen(vm, &part.screen, |s| {
                    s.step.menu_align = align;
                    Ok(unit())
                })
            }
            I::CameraOffset | I::CameraZoom | I::CameraRotation => {
                let part = part_of(rcv, path)?;
                if part.target != PartTarget::Camera {
                    return Err(bad_receiver(path));
                }
                let prop = match self {
                    I::CameraOffset => PropTarget::CameraOffset,
                    I::CameraZoom => PropTarget::CameraZoom,
                    _ => PropTarget::CameraRotation,
                };
                Ok(part_ref(&part.screen, PartTarget::Prop(prop)))
            }
        }
    }
}

/// The screen behind a `&mut Screen` receiver, for the part accessors.
fn screen_of_part(rcv: &Receiver, path: &str) -> Result<Ref, Trap> {
    match rcv {
        Receiver::Screen(r) => Ok(r.clone()),
        _ => Err(bad_receiver(path)),
    }
}

/// The screen and id of an `&mut Entity` receiver; the entity is revived or spawned.
fn entity_of(vm: &Vm, rcv: Receiver, path: &str) -> Result<(Ref, u32), Trap> {
    let part = part_of(rcv, path)?;
    let PartTarget::Entity(id) = part.target else {
        return Err(bad_receiver(path));
    };
    with_screen(vm, &part.screen, |s| {
        s.entity_mut(id);
        Ok(())
    })?;
    Ok((part.screen, id))
}

/// A pending change after its value check.
pub(crate) fn pending(screen: &Ref, change: PendingChange) -> Result<Value, Trap> {
    ScreenObj::check_change(&change)?;
    Ok(pending_ref(screen, change))
}

/// `apply_animation` on a pending change.
pub(crate) fn commit(vm: &Vm, pending: PendingObj, animation: Animation) -> Result<Value, Trap> {
    with_screen(vm, &pending.screen, |s| s.commit(pending.change, animation))?;
    Ok(Value::unit())
}
