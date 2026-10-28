//! `System::bindings`: inputs bound to actions.

use kagari_vm::{Trap, Value, Vm};

use super::{Key, MouseButton, PadButton};
use crate::HostCtx;
use crate::frame::ThumbKey;
use crate::saves;
use crate::story::driver::set_mode;
use crate::story::{PlayMode, StoryInput};
use crate::system::NavOp;
use crate::system::modes::ModeKind;
use crate::ui::SourceKey;
use crate::values::{StdTy, StdTypes};

/// Runs one action. `Back` and `Open` work everywhere; the rest need the game on top.
pub(crate) fn fire(ctx: &mut HostCtx, vm: &mut Vm, action: Action) -> Result<(), Trap> {
    match action {
        Action::Back => {
            ctx.state.pending_nav.push(NavOp::Back);
            return Ok(());
        }
        Action::Open(o) => {
            ctx.state.pending_nav.push(NavOp::Call(o));
            return Ok(());
        }
        _ => {}
    }
    if ctx.state.system.top() != Some(ModeKind::Game) {
        return Ok(());
    }
    let Some(game) = ctx.state.game.as_mut() else {
        return Ok(());
    };
    match action {
        Action::Advance => {
            if game.hide_window {
                game.hide_window = false;
                ctx.state.ui.invalidate(SourceKey::NarrativeHud);
            } else {
                game.pending_input.get_or_insert(StoryInput::Advance);
            }
        }
        Action::HoldSkip => {
            if !ctx.state.system.hold_skip {
                ctx.state.system.hold_skip = true;
                ctx.state.ui.invalidate(SourceKey::NarrativeMode);
            }
        }
        Action::ToggleSkip | Action::ToggleAuto => {
            let target = if matches!(action, Action::ToggleSkip) {
                PlayMode::Skip
            } else {
                PlayMode::Auto
            };
            let mode = if game.mode == target {
                PlayMode::Normal
            } else {
                target
            };
            set_mode(ctx, mode);
        }
        Action::HideWindow => {
            game.hide_window = !game.hide_window;
            ctx.state.ui.invalidate(SourceKey::NarrativeHud);
        }
        Action::QuickSave => saves::save_slot(ctx, vm, ThumbKey::Quick),
        Action::QuickLoad => {
            if let Err(d) = saves::load_slot(ctx, vm, Some(ThumbKey::Quick)) {
                log::warn!("quick load failed: {}", d.describe());
            }
        }
        Action::OpenInGame(o) => {
            let generation = game.generation;
            ctx.state.pending_nav.push(NavOp::CallGame {
                component: o,
                generation,
            });
        }
        Action::Back | Action::Open(_) => {}
    }
    Ok(())
}

/// An input a binding can name: `std::system::Input`, with the wheel directions split out of
/// `MouseButton`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum BindInput {
    Key(Key),
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
    Pad(PadButton),
}

/// What a binding does: `std::system::Action`.
#[derive(Clone, Debug)]
pub(crate) enum Action {
    Advance,
    HoldSkip,
    ToggleSkip,
    ToggleAuto,
    HideWindow,
    QuickSave,
    QuickLoad,
    Back,
    /// A `fn() -> Overlay`.
    Open(Value),
    /// A `fn(Narrative) -> GameOverlay`.
    OpenInGame(Value),
}

/// One entry of `System::bindings`.
#[derive(Clone, Debug)]
pub(crate) struct Binding {
    pub input: BindInput,
    pub action: Action,
}

/// The message of a binding list that does not decode.
const UNKNOWN_INPUT: &str = "`System::bindings` holds an unknown input";

/// Decodes a `Vec<Binding>`; the error is the boot failure message.
pub(crate) fn decode_bindings(std: &StdTypes, v: &Value) -> Result<Vec<Binding>, String> {
    let (Value::Vec(items) | Value::Array(items)) = v else {
        return Err("`System::bindings` is not a `Vec<Binding>`".to_owned());
    };
    items.iter().map(|item| decode_binding(std, item)).collect()
}

fn decode_binding(std: &StdTypes, v: &Value) -> Result<Binding, String> {
    let malformed = |_| "`System::bindings` holds a malformed binding".to_owned();
    let fields = std.fields(v, StdTy::Binding).map_err(malformed)?;
    let [input, action] = fields else {
        return Err("`System::bindings` holds a malformed binding".to_owned());
    };
    Ok(Binding {
        input: decode_input(std, input)?,
        action: decode_action(std, action)?,
    })
}

fn decode_input(std: &StdTypes, v: &Value) -> Result<BindInput, String> {
    let unknown = || UNKNOWN_INPUT.to_owned();
    let (variant, fields) = std.variant(v, StdTy::Input).map_err(|_| unknown())?;
    let [inner] = fields else {
        return Err(unknown());
    };
    let (index, t) = match variant {
        0 => (StdTy::Key, 0),
        1 => (StdTy::MouseButton, 1),
        2 => (StdTy::PadButton, 2),
        _ => return Err(unknown()),
    };
    let (n, inner_fields) = std.variant(inner, index).map_err(|_| unknown())?;
    if !inner_fields.is_empty() {
        return Err(unknown());
    }
    let input = match t {
        0 => Key::from_index(n).map(BindInput::Key),
        1 => match n {
            5 => Some(BindInput::WheelUp),
            6 => Some(BindInput::WheelDown),
            _ => MouseButton::from_index(n).map(BindInput::Mouse),
        },
        _ => PadButton::from_index(n).map(BindInput::Pad),
    };
    input.ok_or_else(unknown)
}

fn decode_action(std: &StdTypes, v: &Value) -> Result<Action, String> {
    let malformed = || "`System::bindings` holds an unknown action".to_owned();
    let (variant, fields) = std.variant(v, StdTy::Action).map_err(|_| malformed())?;
    Ok(match (variant, fields) {
        (0, []) => Action::Advance,
        (1, []) => Action::HoldSkip,
        (2, []) => Action::ToggleSkip,
        (3, []) => Action::ToggleAuto,
        (4, []) => Action::HideWindow,
        (5, []) => Action::QuickSave,
        (6, []) => Action::QuickLoad,
        (7, []) => Action::Back,
        (8, [f]) => Action::Open(f.clone()),
        (9, [f]) => Action::OpenInGame(f.clone()),
        _ => return Err(malformed()),
    })
}
