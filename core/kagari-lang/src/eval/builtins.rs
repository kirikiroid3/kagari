//! The `#[builtin]` const functions of std: `asset`, `Vec`, `Integer`, the screen API,
//! text building, `setting` and `meta`.

use rustc_hash::FxHashSet;

use crate::diagnostic::DiagnosticCode;
use crate::hir::ExprId;
use crate::ty::{FnRef, GenericArg, Ty};

use super::Fault;
use super::anim;
use super::interp::{ErrorKind, Frame, Machine, Pending, Place, Value, int_range};
use super::screen::{
    ChannelId, EntityProp, LayerTarget, PendingChange, PropTarget, Screen, ScreenObj, Target,
};
use super::std_items::{Builtin, StdTable};
use super::value::{ConstValue, HandleKind, Text};

fn fault(code: DiagnosticCode, message: impl Into<String>) -> ErrorKind {
    ErrorKind::Fault(Fault::new(code, message))
}

/// The marks of a text, in reading order.
pub(crate) fn text_marks(text: &Text, std: &StdTable) -> Vec<i128> {
    let mut marks = Vec::new();
    let Some(index) = std.text_effect.field("mark") else {
        return marks;
    };
    text.walk_effects(&mut |effect| {
        if let Some(Some(mark)) = effect.field(index).and_then(|mark| std.as_option(mark))
            && let Some(n) = std.newtype_int(mark)
        {
            marks.push(n);
        }
    });
    marks
}

/// A duplicate mark in `text` is an error.
pub(crate) fn check_marks(text: &Text, std: &StdTable) -> Result<(), Fault> {
    let mut seen = FxHashSet::default();
    for mark in text_marks(text, std) {
        if !seen.insert(mark) {
            return Err(Fault::new(
                DiagnosticCode::EvalDuplicateMark,
                format!("duplicate text mark `Mark({mark})`"),
            ));
        }
    }
    Ok(())
}

/// `to_text(value)`: integers in decimal, `Rat` and `Decimal` exactly.
pub(crate) fn to_text(value: &ConstValue) -> Option<Text> {
    Some(match value {
        ConstValue::Text(text) => text.clone(),
        ConstValue::String(text) => Text::plain(text.clone()),
        ConstValue::Int(value) => Text::plain(value.to_string()),
        ConstValue::Decimal(value) => Text::plain(value.to_string()),
        ConstValue::Rat(value) => Text::plain(value.to_string()),
        _ => return None,
    })
}

impl<'db> Machine<'_, 'db> {
    fn arg(args: &[Value], index: usize) -> Result<&Value, ErrorKind> {
        args.get(index).ok_or(ErrorKind::Abort)
    }

    /// Argument `index` as plain data, references followed.
    fn const_arg(&mut self, args: &[Value], index: usize) -> Result<ConstValue, ErrorKind> {
        let value = self.deref_value(Self::arg(args, index)?.clone())?;
        self.freeze(&value)
    }

    fn int_arg(&mut self, args: &[Value], index: usize) -> Result<i128, ErrorKind> {
        match self.deref_value(Self::arg(args, index)?.clone())? {
            Value::Int(value) => Ok(value),
            _ => Err(ErrorKind::Abort),
        }
    }

    /// The `u32`/`u8` inside a newtype argument (`EntityId`, `Part`, `Mark`).
    fn newtype_arg(&mut self, args: &[Value], index: usize) -> Result<i128, ErrorKind> {
        let value = self.const_arg(args, index)?;
        self.std().newtype_int(&value).ok_or(ErrorKind::Abort)
    }

    fn duration_arg(&mut self, args: &[Value], index: usize) -> Result<u32, ErrorKind> {
        let value = self.const_arg(args, index)?;
        self.std().duration_ms(&value).ok_or(ErrorKind::Abort)
    }

    fn text_arg(&mut self, args: &[Value], index: usize) -> Result<Text, ErrorKind> {
        match self.const_arg(args, index)? {
            ConstValue::Text(text) => Ok(text),
            _ => Err(ErrorKind::Abort),
        }
    }

    /// The receiver place and the screen part it denotes (`None`: the screen itself).
    fn receiver_obj(&mut self, args: &[Value]) -> Result<(Place, Option<ScreenObj>), ErrorKind> {
        let place = self.ref_place(Self::arg(args, 0)?)?;
        let obj = place.obj;
        Ok((place, obj))
    }

    fn screen_receiver(&mut self, args: &[Value]) -> Result<(Place, &mut Screen), ErrorKind> {
        let (place, obj) = self.receiver_obj(args)?;
        if obj.is_some() {
            return Err(ErrorKind::Abort);
        }
        let screen = self.screen_at(&place)?;
        Ok((place, screen))
    }

    fn entity_receiver(&mut self, args: &[Value]) -> Result<(Place, u32), ErrorKind> {
        match self.receiver_obj(args)? {
            (place, Some(ScreenObj::Entity(id))) => Ok((place, id)),
            _ => Err(ErrorKind::Abort),
        }
    }

    fn obj_ref(place: &Place, obj: ScreenObj) -> Value {
        Value::Ref(Box::new(place.with_obj(obj)))
    }

    fn pending(&mut self, place: &Place, change: PendingChange) -> Result<Value, ErrorKind> {
        Screen::check_change(&change, self.std())?;
        // Touch the screen so a probe is detected where the change is made.
        self.screen_at(place)?;
        let cell = self.alloc(Value::Pending(Box::new(Pending {
            screen: place.without_obj(),
            change,
        })));
        Ok(Value::Ref(Box::new(Place {
            cell,
            path: Vec::new(),
            obj: None,
        })))
    }

    fn commit(&mut self, args: &[Value]) -> Result<Value, ErrorKind> {
        let place = self.ref_place(Self::arg(args, 0)?)?;
        let Value::Pending(pending) = self.place_mut(&place)?.clone() else {
            return Err(ErrorKind::Abort);
        };
        let animation = self.const_arg(args, 1)?;
        let std = self.std();
        self.screen_at(&pending.screen)?
            .commit(pending.change, animation, std)?;
        Ok(Value::unit())
    }

    fn option_value(&self, value: Option<Value>) -> Value {
        let std = self.std();
        match value {
            Some(value) => Value::Adt {
                item: std.option.item,
                variant: std.option.variant("Some").unwrap_or(1),
                fields: vec![value],
            },
            None => Value::Adt {
                item: std.option.item,
                variant: std.option.variant("None").unwrap_or(0),
                fields: Vec::new(),
            },
        }
    }

    /// Runs `builtin`; `expr` is the call.
    pub(crate) fn builtin(
        &mut self,
        frame: &Frame<'db>,
        expr: ExprId,
        builtin: Builtin,
        fn_ref: &FnRef,
        args: Vec<Value>,
    ) -> Result<Value, ErrorKind> {
        let std = self.std();
        for arg in &args {
            if matches!(arg, Value::Probe) {
                return Err(ErrorKind::Inspected);
            }
        }
        match builtin {
            Builtin::Asset => {
                let Text::Plain(path) = self.text_arg(&args, 0)? else {
                    return Err(ErrorKind::Abort);
                };
                let asset_item = match fn_ref.args.last() {
                    Some(GenericArg::Type(Ty::Adt { item, .. })) => Some(*item),
                    _ => match frame.ty(expr) {
                        Some(Ty::Adt { item, .. }) => Some(item),
                        _ => None,
                    },
                };
                let kind = asset_item
                    .and_then(|item| std.asset_kind(item))
                    .ok_or(ErrorKind::Abort)?;
                let file = frame.b.item.file.path(self.ctx.db).parent_dir_join(&path);
                Ok(Value::Asset { kind, path: file })
            }
            Builtin::VecNew => Ok(Value::Vec(Vec::new())),
            Builtin::VecLen | Builtin::VecIsEmpty | Builtin::VecGet => {
                let place = self.ref_place(Self::arg(&args, 0)?)?;
                let Value::Vec(items) = self.place_mut(&place)?.clone() else {
                    return Err(ErrorKind::Abort);
                };
                match builtin {
                    Builtin::VecLen => Ok(Value::Int(
                        i128::try_from(items.len()).map_err(|_| ErrorKind::Abort)?,
                    )),
                    Builtin::VecIsEmpty => Ok(Value::Bool(items.is_empty())),
                    _ => {
                        let index = self.int_arg(&args, 1)?;
                        let item = usize::try_from(index)
                            .ok()
                            .and_then(|index| items.get(index))
                            .cloned();
                        Ok(self.option_value(item))
                    }
                }
            }
            Builtin::VecSet => {
                let index = self.int_arg(&args, 1)?;
                let value = Self::arg(&args, 2)?.clone();
                let place = self.ref_place(Self::arg(&args, 0)?)?;
                let Value::Vec(items) = self.place_mut(&place)? else {
                    return Err(ErrorKind::Abort);
                };
                let len = items.len();
                match usize::try_from(index)
                    .ok()
                    .and_then(|index| items.get_mut(index))
                {
                    Some(slot) => {
                        *slot = value;
                        Ok(Value::unit())
                    }
                    None => Err(fault(
                        DiagnosticCode::EvalIndexOutOfRange,
                        format!("`Vec::set` index {index} is out of range for length {len}"),
                    )),
                }
            }
            Builtin::VecPush => {
                let value = Self::arg(&args, 1)?.clone();
                let place = self.ref_place(Self::arg(&args, 0)?)?;
                let Value::Vec(items) = self.place_mut(&place)? else {
                    return Err(ErrorKind::Abort);
                };
                items.push(value);
                Ok(Value::unit())
            }
            Builtin::IntegerInto => Ok(Value::Int(self.int_arg(&args, 0)?)),
            Builtin::IntegerSaturate => {
                let value = self.int_arg(&args, 0)?;
                Ok(Value::Int(
                    match frame.ty(expr).as_ref().and_then(int_range) {
                        Some((lo, hi)) => value.clamp(lo, hi),
                        None => value,
                    },
                ))
            }
            Builtin::ScreenEmpty => Ok(Value::Screen(Box::new(Screen::empty(std)))),
            Builtin::ScreenEntity => {
                let id =
                    u32::try_from(self.newtype_arg(&args, 1)?).map_err(|_| ErrorKind::Abort)?;
                let (place, screen) = self.screen_receiver(&args)?;
                screen.entity(id, std);
                Ok(Self::obj_ref(&place, ScreenObj::Entity(id)))
            }
            Builtin::ScreenContains => {
                let id =
                    u32::try_from(self.newtype_arg(&args, 1)?).map_err(|_| ErrorKind::Abort)?;
                let (_, screen) = self.screen_receiver(&args)?;
                Ok(Value::Bool(screen.contains(id)))
            }
            Builtin::ScreenEntities => {
                let plane = self.const_arg(&args, 1)?;
                let (_, screen) = self.screen_receiver(&args)?;
                let ids = screen.entities_on(&plane);
                Ok(Value::Vec(
                    ids.into_iter()
                        .map(|id| self.thaw(&std.entity_id_value(id)))
                        .collect(),
                ))
            }
            Builtin::ScreenDespawn => {
                let id =
                    u32::try_from(self.newtype_arg(&args, 1)?).map_err(|_| ErrorKind::Abort)?;
                let (_, screen) = self.screen_receiver(&args)?;
                screen.despawn(id);
                Ok(Value::unit())
            }
            Builtin::ScreenHud
            | Builtin::ScreenChoiceMenu
            | Builtin::ScreenCamera
            | Builtin::ScreenFilter
            | Builtin::ScreenMusic => {
                let (place, _) = self.screen_receiver(&args)?;
                let obj = match builtin {
                    Builtin::ScreenHud => ScreenObj::Hud,
                    Builtin::ScreenChoiceMenu => ScreenObj::ChoiceMenu,
                    Builtin::ScreenCamera => ScreenObj::Camera,
                    Builtin::ScreenFilter => ScreenObj::Prop(PropTarget::ScreenFilter),
                    _ => ScreenObj::Channel(ChannelId::Music),
                };
                Ok(Self::obj_ref(&place, obj))
            }
            Builtin::ScreenAmbience => {
                let channel = u8::try_from(self.int_arg(&args, 1)?)
                    .ok()
                    .filter(|channel| *channel < 4)
                    .ok_or(ErrorKind::Abort)?;
                let (place, _) = self.screen_receiver(&args)?;
                Ok(Self::obj_ref(
                    &place,
                    ScreenObj::Channel(ChannelId::Ambience(channel)),
                ))
            }
            Builtin::ScreenPlay => {
                let sound = self.const_arg(&args, 1)?;
                let (_, screen) = self.screen_receiver(&args)?;
                screen.play(sound, std)?;
                Ok(Value::unit())
            }
            Builtin::ScreenStopSounds => {
                let fade_out = self.duration_arg(&args, 1)?;
                let (_, screen) = self.screen_receiver(&args)?;
                screen.stop_sounds(fade_out)?;
                Ok(Value::unit())
            }
            Builtin::ScreenWait => {
                let duration = self.duration_arg(&args, 1)?;
                let (_, screen) = self.screen_receiver(&args)?;
                screen.wait(duration)?;
                Ok(Value::unit())
            }
            Builtin::ScreenBarrier => {
                self.screen_receiver(&args)?.1.barrier();
                Ok(Value::unit())
            }
            Builtin::ScreenAtMark => {
                let mark =
                    u8::try_from(self.newtype_arg(&args, 1)?).map_err(|_| ErrorKind::Abort)?;
                self.screen_receiver(&args)?.1.at_mark(mark);
                Ok(Value::unit())
            }
            Builtin::ScreenAtEntry => {
                self.screen_receiver(&args)?.1.at_entry();
                Ok(Value::unit())
            }
            Builtin::ScreenBeginText => {
                self.screen_receiver(&args)?.1.begin_text()?;
                Ok(Value::unit())
            }
            Builtin::EntityPosition
            | Builtin::EntityScale
            | Builtin::EntityRotation
            | Builtin::EntityOpacity
            | Builtin::EntityFilter
            | Builtin::EntityFill => {
                let (place, id) = self.entity_receiver(&args)?;
                let prop = match builtin {
                    Builtin::EntityPosition => EntityProp::Position,
                    Builtin::EntityScale => EntityProp::Scale,
                    Builtin::EntityRotation => EntityProp::Rotation,
                    Builtin::EntityOpacity => EntityProp::Opacity,
                    Builtin::EntityFilter => EntityProp::Filter,
                    _ => EntityProp::Fill,
                };
                Ok(Self::obj_ref(
                    &place,
                    ScreenObj::Prop(PropTarget::Entity(id, prop)),
                ))
            }
            Builtin::EntityPart => {
                let part =
                    u8::try_from(self.newtype_arg(&args, 1)?).map_err(|_| ErrorKind::Abort)?;
                let (place, id) = self.entity_receiver(&args)?;
                Ok(Self::obj_ref(
                    &place,
                    ScreenObj::Layer(LayerTarget::Part(id, part)),
                ))
            }
            Builtin::EntityLabel | Builtin::EntityMovie => {
                let (place, id) = self.entity_receiver(&args)?;
                let layer = if builtin == Builtin::EntityLabel {
                    LayerTarget::Label(id)
                } else {
                    LayerTarget::Movie(id)
                };
                Ok(Self::obj_ref(&place, ScreenObj::Layer(layer)))
            }
            Builtin::EntitySetPlane
            | Builtin::EntitySetZ
            | Builtin::EntitySetAnchor
            | Builtin::EntitySetFlip
            | Builtin::EntitySetBlend
            | Builtin::EntitySetFillSize
            | Builtin::EntityGate => {
                let value = self.const_arg(&args, 1)?;
                if builtin == Builtin::EntitySetAnchor {
                    anim::check_anchor(&value, std)?;
                }
                let (place, id) = self.entity_receiver(&args)?;
                let entity = self.screen_at(&place)?.entity(id, std);
                match builtin {
                    Builtin::EntitySetPlane => entity.plane = value,
                    Builtin::EntitySetZ => entity.z = value.as_int().ok_or(ErrorKind::Abort)?,
                    Builtin::EntitySetAnchor => entity.anchor = value,
                    Builtin::EntitySetFlip => entity.flip = value,
                    Builtin::EntitySetBlend => entity.blend = value,
                    Builtin::EntitySetFillSize => entity.fill_size = value,
                    _ => entity.gate = value,
                }
                Ok(Value::unit())
            }
            Builtin::EntityApplyAnimation | Builtin::CameraApplyAnimation => {
                let motion = self.const_arg(&args, 1)?;
                let (place, obj) = self.receiver_obj(&args)?;
                let target = match obj {
                    Some(ScreenObj::Entity(id)) => Target::Entity(id),
                    Some(ScreenObj::Camera) => Target::Camera,
                    _ => return Err(ErrorKind::Abort),
                };
                self.screen_at(&place)?.apply_motion(target, motion, std)?;
                Ok(Value::unit())
            }
            Builtin::PropGet => {
                let (place, Some(ScreenObj::Prop(target))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                let value = self.screen_at(&place)?.prop(target, std);
                Ok(self.thaw(&value))
            }
            Builtin::PropChange => {
                let value = self.const_arg(&args, 1)?;
                let (place, Some(ScreenObj::Prop(target))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.pending(&place, PendingChange::Prop(target, value))
            }
            Builtin::ChangeApplyAnimation
            | Builtin::SwapApplyAnimation
            | Builtin::ChannelChangeApplyAnimation => self.commit(&args),
            Builtin::ImageLayerGet => {
                let (place, Some(ScreenObj::Layer(target))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                let content = self.screen_at(&place)?.layer(target, std);
                let content = content.map(|content| self.thaw(&content));
                Ok(self.option_value(content))
            }
            Builtin::ImageLayerChange | Builtin::LabelLayerChange | Builtin::MovieLayerChange => {
                let content = self.const_arg(&args, 1)?;
                if let ConstValue::Text(text) = &content {
                    check_marks(text, std)?;
                }
                let (place, Some(ScreenObj::Layer(target))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.pending(&place, PendingChange::Layer(target, Some(content)))
            }
            Builtin::ImageLayerClear | Builtin::LabelLayerClear | Builtin::MovieLayerClear => {
                let (place, Some(ScreenObj::Layer(target))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.pending(&place, PendingChange::Layer(target, None))
            }
            Builtin::LabelLayerSetStyle | Builtin::LabelLayerSetWidth => {
                let value = self.const_arg(&args, 1)?;
                let (place, Some(ScreenObj::Layer(LayerTarget::Label(id)))) =
                    self.receiver_obj(&args)?
                else {
                    return Err(ErrorKind::Abort);
                };
                let entity = self.screen_at(&place)?.entity(id, std);
                if builtin == Builtin::LabelLayerSetStyle {
                    entity.label_style = Some(value);
                } else {
                    entity.label_width = value;
                }
                Ok(Value::unit())
            }
            Builtin::HudTextWindow | Builtin::HudQuickMenu | Builtin::HudSideImage => {
                let (place, Some(ScreenObj::Hud)) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.screen_at(&place)?;
                let obj = match builtin {
                    Builtin::HudTextWindow => ScreenObj::Prop(PropTarget::HudTextWindow),
                    Builtin::HudQuickMenu => ScreenObj::Prop(PropTarget::HudQuickMenu),
                    _ => ScreenObj::Layer(LayerTarget::SideImage),
                };
                Ok(Self::obj_ref(&place, obj))
            }
            Builtin::ChoiceMenuDisabledArm => {
                let at = self.int_arg(&args, 1)?;
                let text = self.const_arg(&args, 2)?;
                let (place, Some(ScreenObj::ChoiceMenu)) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.screen_at(&place)?.step.disabled_arms.push((at, text));
                Ok(Value::unit())
            }
            Builtin::ChoiceMenuSetAlign => {
                let align = self.const_arg(&args, 1)?;
                let (place, Some(ScreenObj::ChoiceMenu)) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.screen_at(&place)?.step.menu_align = align;
                Ok(Value::unit())
            }
            Builtin::CameraOffset | Builtin::CameraZoom | Builtin::CameraRotation => {
                let (place, Some(ScreenObj::Camera)) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.screen_at(&place)?;
                let prop = match builtin {
                    Builtin::CameraOffset => PropTarget::CameraOffset,
                    Builtin::CameraZoom => PropTarget::CameraZoom,
                    _ => PropTarget::CameraRotation,
                };
                Ok(Self::obj_ref(&place, ScreenObj::Prop(prop)))
            }
            Builtin::ChannelGet => {
                let (place, Some(ScreenObj::Channel(channel))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                let track = self.screen_at(&place)?.channel(channel).track.clone();
                let track = track.map(|track| self.thaw(&track));
                Ok(self.option_value(track))
            }
            Builtin::ChannelChange | Builtin::ChannelStop => {
                let track = if builtin == Builtin::ChannelChange {
                    Some(self.const_arg(&args, 1)?)
                } else {
                    None
                };
                let (place, Some(ScreenObj::Channel(channel))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.pending(&place, PendingChange::Channel(channel, track))
            }
            Builtin::ChannelVolume => {
                let (place, Some(ScreenObj::Channel(channel))) = self.receiver_obj(&args)? else {
                    return Err(ErrorKind::Abort);
                };
                self.screen_at(&place)?;
                Ok(Self::obj_ref(
                    &place,
                    ScreenObj::Prop(PropTarget::ChannelVolume(channel)),
                ))
            }
            Builtin::Ruby => {
                let base = self.text_arg(&args, 0)?;
                let reading = self.text_arg(&args, 1)?;
                Ok(Value::Text(Text::Ruby {
                    base: Box::new(base),
                    reading: Box::new(reading),
                }))
            }
            Builtin::MonoRuby => {
                let base = self.text_arg(&args, 0)?;
                let ConstValue::Array(readings) = self.const_arg(&args, 1)? else {
                    return Err(ErrorKind::Abort);
                };
                let readings = readings
                    .into_iter()
                    .map(|reading| match reading {
                        ConstValue::Text(text) => Ok(text),
                        _ => Err(ErrorKind::Abort),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Text(Text::MonoRuby {
                    base: Box::new(base),
                    readings,
                }))
            }
            Builtin::Effect => {
                let base = self.text_arg(&args, 0)?;
                let effect = self.const_arg(&args, 1)?;
                let text = Text::Effect {
                    base: Box::new(base),
                    effect: Box::new(effect),
                };
                check_marks(&text, std)?;
                Ok(Value::Text(text))
            }
            Builtin::Concat => {
                let ConstValue::Array(items) = self.const_arg(&args, 0)? else {
                    return Err(ErrorKind::Abort);
                };
                let items = items
                    .into_iter()
                    .map(|item| match item {
                        ConstValue::Text(text) => Ok(text),
                        _ => Err(ErrorKind::Abort),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let text = Text::Concat(items);
                check_marks(&text, std)?;
                Ok(Value::Text(text))
            }
            Builtin::ToText => {
                let value = self.const_arg(&args, 0)?;
                to_text(&value).map(Value::Text).ok_or(ErrorKind::Abort)
            }
            Builtin::Setting | Builtin::Meta => {
                let init = self.deref_value(Self::arg(&args, 0)?.clone())?;
                let kind = if builtin == Builtin::Setting {
                    HandleKind::Setting
                } else {
                    HandleKind::Meta
                };
                Ok(Value::NewHandle {
                    kind,
                    init: Box::new(init),
                })
            }
            // A `fn` builtin in const code is a call-rule error of stage 3.
            Builtin::Runtime => Err(ErrorKind::Abort),
        }
    }
}
