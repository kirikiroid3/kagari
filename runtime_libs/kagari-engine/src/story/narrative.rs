//! `std::narrative`: the `Narrative` handle of the running game, its arms, and `Meta` statics.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{HostObj, HostObject, StaticId, Trap, Value, Vm};

use super::driver::{self, effective_mode};
use super::{Current, Entry, GameFrame, PlayMode, StoryInput};
use crate::HostCtx;
use crate::saves;
use crate::store::value::check_storable;
use crate::system::NavOp;
use crate::ui::SourceKey;
use crate::ui::imports::deref;
use crate::values::{StdTy, StdTypes, Visibility};

/// The `Narrative` handle: the game of one generation.
#[derive(Clone, Debug)]
pub(crate) struct NarrativeObj {
    pub generation: u64,
}

impl HostObject for NarrativeObj {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "Narrative"
    }
    fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
        let other = other.as_any().downcast_ref::<NarrativeObj>()?;
        Some(self.generation == other.generation)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// An arm of the choice at step `serial` of game `generation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArmObj {
    pub generation: u64,
    pub serial: u64,
    pub index: u32,
}

impl HostObject for ArmObj {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "Arm"
    }
    fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
        let other = other.as_any().downcast_ref::<ArmObj>()?;
        Some(self == other)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

import_table!(
    /// The `std::narrative` imports.
    NarrativeImport {
        Line = "std::narrative::Narrative::line",
        Menu = "std::narrative::Narrative::menu",
        Hud = "std::narrative::Narrative::hud",
        History = "std::narrative::Narrative::history",
        Mode = "std::narrative::Narrative::mode",
        SetMode = "std::narrative::Narrative::set_mode",
        Advance = "std::narrative::Narrative::advance",
        Select = "std::narrative::Narrative::select",
        Save = "std::narrative::Narrative::save",
        Rewind = "std::narrative::Narrative::rewind",
        Call = "std::narrative::Narrative::call",
        ReplayVoice = "std::narrative::Narrative::replay_voice",
        MetaGet = "std::narrative::Meta::get",
        MetaSet = "std::narrative::Meta::set",
        MetaWrite = "std::narrative::Meta::write",
        MetaInit = "std::narrative::meta",
    }
);

/// The game a handle names; traps once that game ended.
pub(crate) fn game_of<'a>(ctx: &'a HostCtx, handle: &NarrativeObj) -> Result<&'a GameFrame, Trap> {
    ctx.state
        .game
        .as_ref()
        .filter(|g| g.generation == handle.generation)
        .ok_or_else(|| Trap::host("the `Narrative` handle was used after its game ended"))
}

fn game_of_mut<'a>(ctx: &'a mut HostCtx, handle: &NarrativeObj) -> Result<&'a mut GameFrame, Trap> {
    ctx.state
        .game
        .as_mut()
        .filter(|g| g.generation == handle.generation)
        .ok_or_else(|| Trap::host("the `Narrative` handle was used after its game ended"))
}

fn narrative(vm: &Vm, path: &str, v: Option<&Value>) -> Result<NarrativeObj, Trap> {
    let v = deref(vm, v.cloned().unwrap_or(Value::Uninit))?;
    match &v {
        Value::Host(obj) => obj.downcast::<NarrativeObj>().cloned(),
        _ => None,
    }
    .ok_or_else(|| {
        Trap::host(format!(
            "`{path}` expected a `Narrative`, found {}",
            v.tag()
        ))
    })
}

fn handle(vm: &Vm, path: &str, v: Option<&Value>) -> Result<StaticId, Trap> {
    let v = deref(vm, v.cloned().unwrap_or(Value::Uninit))?;
    match v {
        Value::Handle(id) => Ok(id),
        other => Err(Trap::host(format!(
            "`{path}` expected a handle, found {}",
            other.tag()
        ))),
    }
}

fn opt_text(std: &StdTypes, t: &Option<kagari_vm::Text>) -> Value {
    match t {
        Some(t) => std.some(Value::Text(t.clone())),
        None => std.none(),
    }
}

fn opt_asset(std: &StdTypes, a: Option<kagari_vm::AssetId>) -> Value {
    match a {
        Some(a) => std.some(Value::Asset(a)),
        None => std.none(),
    }
}

fn line_value(
    std: &StdTypes,
    who: &Option<kagari_vm::Text>,
    text: &Option<kagari_vm::Text>,
    voice: Option<kagari_vm::AssetId>,
    read: bool,
) -> Value {
    std.make(
        StdTy::Line,
        0,
        vec![
            opt_text(std, who),
            opt_text(std, text),
            opt_asset(std, voice),
            Value::Bool(read),
        ],
    )
}

fn visibility(std: &StdTypes, shown: bool) -> Value {
    let v = if shown {
        Visibility::Shown
    } else {
        Visibility::Hidden
    };
    std.unit(StdTy::Visibility, v as u32)
}

impl NarrativeImport {
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use NarrativeImport as I;
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, &args, i);
        match self {
            I::MetaInit => Err(Trap::host(
                "`meta(…)` and `setting(…)` only initialize statics",
            )),
            I::MetaGet => {
                let id = handle(vm, path, args.first())?;
                ctx.state.ui.track(SourceKey::Meta(id));
                ctx.state
                    .system
                    .stores
                    .meta
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| Trap::host(format!("`{path}`: not a `Meta` static")))
            }
            I::MetaSet | I::MetaWrite => {
                let id = handle(vm, path, args.first())?;
                let value = arg(1)?;
                if self == I::MetaWrite && ctx.state.system.replaying {
                    return Ok(Value::unit());
                }
                check_storable(&value, "Meta")?;
                let stores = &mut ctx.state.system.stores;
                if !stores.meta.contains_key(&id) {
                    return Err(Trap::host(format!("`{path}`: not a `Meta` static")));
                }
                stores.meta.insert(id, value);
                stores.dirty_meta = true;
                ctx.state.ui.invalidate(SourceKey::Meta(id));
                Ok(Value::unit())
            }
            _ => {
                let obj = narrative(vm, path, args.first())?;
                self.call_narrative(ctx, vm, &obj, &args)
            }
        }
    }

    fn call_narrative(
        self,
        ctx: &mut HostCtx,
        vm: &mut Vm,
        obj: &NarrativeObj,
        args: &[Value],
    ) -> Result<Value, Trap> {
        use NarrativeImport as I;
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, args, i);
        let std = &ctx.state.std;
        match self {
            I::Line => {
                ctx.state.ui.track(SourceKey::NarrativeLine);
                let std = &ctx.state.std;
                Ok(match &game_of(ctx, obj)?.current {
                    Current::Say(say) => {
                        std.some(line_value(std, &say.who, &say.text, say.voice, say.read))
                    }
                    _ => std.none(),
                })
            }
            I::Menu => {
                ctx.state.ui.track(SourceKey::NarrativeMenu);
                let std = &ctx.state.std;
                let game = game_of(ctx, obj)?;
                let Current::Choice(choice) = &game.current else {
                    return Ok(std.none());
                };
                let mut arms: Vec<Value> = Vec::new();
                for (index, arm) in choice.arms.iter().enumerate() {
                    if !arm.shown {
                        continue;
                    }
                    let handle = Value::Host(HostObj::new(ArmObj {
                        generation: game.generation,
                        serial: game.step_serial,
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                    }));
                    arms.push(std.make(
                        StdTy::MenuArm,
                        0,
                        vec![Value::Text(arm.text.clone()), std.some(handle)],
                    ));
                }
                for (at, text) in &choice.disabled {
                    let at = (*at as usize).min(arms.len());
                    arms.insert(
                        at,
                        std.make(
                            StdTy::MenuArm,
                            0,
                            vec![Value::Text(text.clone()), std.none()],
                        ),
                    );
                }
                Ok(std.some(std.make(
                    StdTy::Menu,
                    0,
                    vec![
                        Value::Vec(Rc::new(arms)),
                        std.unit(StdTy::VAlign, choice.align as u32),
                    ],
                )))
            }
            I::Hud => {
                ctx.state.ui.track(SourceKey::NarrativeHud);
                let std = &ctx.state.std;
                let game = game_of(ctx, obj)?;
                let hidden = game.hide_window;
                let (window, quick) = game.hud_shown;
                // A paint is not an image: `HudState::side_image` is `None` then.
                let side = match &game.screen.logical().hud.side_image {
                    Some(crate::screen::model::PartContent::Image(image)) => Some(*image),
                    _ => None,
                };
                Ok(std.make(
                    StdTy::HudState,
                    0,
                    vec![
                        visibility(std, window && !hidden),
                        visibility(std, quick && !hidden),
                        opt_asset(std, side),
                    ],
                ))
            }
            I::History => {
                ctx.state.ui.track(SourceKey::NarrativeHistory);
                let std = &ctx.state.std;
                let game = game_of(ctx, obj)?;
                let entries = game
                    .backlog
                    .iter()
                    .map(|e| match &e.entry {
                        Entry::Line {
                            who,
                            text,
                            voice,
                            read,
                        } => std.make(
                            StdTy::Entry,
                            0,
                            vec![line_value(std, who, text, *voice, *read)],
                        ),
                        Entry::Chosen(t) => std.make(StdTy::Entry, 1, vec![Value::Text(t.clone())]),
                    })
                    .collect();
                Ok(Value::Vec(Rc::new(entries)))
            }
            I::Mode => {
                ctx.state.ui.track(SourceKey::NarrativeMode);
                game_of(ctx, obj)?;
                let mode = effective_mode(ctx);
                Ok(ctx.state.std.unit(StdTy::PlayMode, mode.index()))
            }
            I::SetMode => {
                let (variant, _) = std.variant(&arg(1)?, StdTy::PlayMode)?;
                let mode = PlayMode::from_index(variant)
                    .ok_or_else(|| Trap::host(format!("`{path}`: unknown play mode")))?;
                game_of(ctx, obj)?;
                driver::set_mode(ctx, mode);
                Ok(Value::unit())
            }
            I::Advance => {
                let game = game_of_mut(ctx, obj)?;
                game.pending_input.get_or_insert(StoryInput::Advance);
                Ok(Value::unit())
            }
            I::Select => {
                let arm = match deref(vm, arg(1)?)? {
                    Value::Host(h) => h.downcast::<ArmObj>().cloned(),
                    _ => None,
                }
                .ok_or_else(|| Trap::host(format!("`{path}` expected an `Arm`")))?;
                let game = game_of_mut(ctx, obj)?;
                if arm.generation != game.generation || arm.serial != game.step_serial {
                    log::warn!("ignored a stale `Arm`");
                } else {
                    game.pending_input
                        .get_or_insert(StoryInput::Select(arm.index));
                }
                Ok(Value::unit())
            }
            I::Save => {
                let slot = saves::decode_slot(ctx, &arg(1)?)?;
                game_of(ctx, obj)?;
                if let Some(key) = slot {
                    saves::save_slot(ctx, vm, key);
                }
                Ok(Value::unit())
            }
            I::Rewind => {
                let index = match arg(1)? {
                    Value::Int(i) => usize::try_from(i).ok(),
                    _ => None,
                };
                let game = game_of(ctx, obj)?;
                let target = index.and_then(|i| game.backlog.get(i)).map(|e| e.at);
                let Some(at) = target else {
                    log::warn!(
                        "rewind to entry {} failed: no such entry",
                        index.map_or(-1, |i| i64::try_from(i).unwrap_or(-1))
                    );
                    return Ok(Value::unit());
                };
                let mut trace = game.trace.clone();
                trace.choices.truncate(at.choices as usize);
                trace.steps_after_last_choice = at.steps;
                let on_exit = game.on_exit.clone();
                match saves::replay::replay(ctx, vm, &trace, on_exit) {
                    Ok(frame) => ctx.state.pending_nav.push(NavOp::Restore(frame)),
                    Err(d) => log::warn!(
                        "rewind to entry {} failed: {}",
                        index.unwrap_or(0),
                        d.describe()
                    ),
                }
                Ok(Value::unit())
            }
            I::Call => {
                let component = arg(1)?;
                game_of(ctx, obj)?;
                ctx.state.pending_nav.push(NavOp::CallGame {
                    component,
                    generation: obj.generation,
                });
                Ok(Value::unit())
            }
            I::ReplayVoice => {
                let Value::Asset(voice) = arg(1)? else {
                    return Err(Trap::host(format!("`{path}` expected a voice asset")));
                };
                let t = {
                    let game = game_of(ctx, obj)?;
                    game.clock.story_ms(ctx.now_ms)
                };
                let state = &mut *ctx.state;
                let length = state.assets.audio_duration_ms(voice, &mut *ctx.platform)?;
                state
                    .audio
                    .play_voice(state.assets.path(voice), 0, length, t);
                Ok(Value::unit())
            }
            I::MetaGet | I::MetaSet | I::MetaWrite | I::MetaInit => Ok(Value::unit()),
        }
    }
}
