//! Input: the events a platform feeds the engine, and the bindings of `System::bindings`.

pub(crate) mod bindings;

use kagari_vm::{Trap, Vm};

use crate::HostCtx;
use crate::ui::{self, SourceKey, UiInput};
use bindings::{Action, BindInput, fire};

/// A pressed input, for dropping repeats and matching releases.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum HeldInput {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}

impl HeldInput {
    fn bind(self) -> BindInput {
        match self {
            HeldInput::Key(k) => BindInput::Key(k),
            HeldInput::Mouse(m) => BindInput::Mouse(m),
            HeldInput::Pad(p) => BindInput::Pad(p),
        }
    }
}

/// Routes one input: the top mount's UI first, then the bindings.
pub(crate) fn dispatch(ctx: &mut HostCtx, vm: &mut Vm, event: InputEvent) -> Result<(), Trap> {
    let (held, pressed) = match event {
        InputEvent::Focus(false) => {
            ctx.state.system.held.clear();
            if ctx.state.system.hold_skip {
                ctx.state.system.hold_skip = false;
                ctx.state.ui.invalidate(SourceKey::NarrativeMode);
            }
            ui::handle_input(ctx, vm, &event)?;
            return Ok(());
        }
        InputEvent::Focus(true) | InputEvent::MouseMove { .. } => {
            ui::handle_input(ctx, vm, &event)?;
            return Ok(());
        }
        InputEvent::Wheel { delta_y } => {
            if delta_y == 0.0 {
                return Ok(());
            }
            if ui::handle_input(ctx, vm, &event)? == UiInput::Consumed {
                return Ok(());
            }
            let input = if delta_y > 0.0 {
                BindInput::WheelUp
            } else {
                BindInput::WheelDown
            };
            return fire_all(ctx, vm, input);
        }
        InputEvent::Key { key, pressed } => (HeldInput::Key(key), pressed),
        InputEvent::MouseButton { button, pressed } => (HeldInput::Mouse(button), pressed),
        InputEvent::Pad { button, pressed } => (HeldInput::Pad(button), pressed),
    };
    if pressed {
        if !ctx.state.system.held.insert(held) {
            return Ok(());
        }
        if ui::handle_input(ctx, vm, &event)? == UiInput::Consumed {
            return Ok(());
        }
        fire_all(ctx, vm, held.bind())
    } else {
        if !ctx.state.system.held.remove(&held) {
            return Ok(());
        }
        ui::handle_input(ctx, vm, &event)?;
        let hold_skip = ctx.state.system.config.as_ref().is_some_and(|c| {
            c.bindings
                .iter()
                .any(|b| b.input == held.bind() && matches!(b.action, Action::HoldSkip))
        });
        if hold_skip && ctx.state.system.hold_skip {
            ctx.state.system.hold_skip = false;
            ctx.state.ui.invalidate(SourceKey::NarrativeMode);
        }
        Ok(())
    }
}

/// Fires every binding of `input`, in order.
fn fire_all(ctx: &mut HostCtx, vm: &mut Vm, input: BindInput) -> Result<(), Trap> {
    let actions: Vec<Action> = ctx
        .state
        .system
        .config
        .as_ref()
        .map(|c| {
            c.bindings
                .iter()
                .filter(|b| b.input == input)
                .map(|b| b.action.clone())
                .collect()
        })
        .unwrap_or_default();
    for action in actions {
        fire(ctx, vm, action)?;
    }
    Ok(())
}

/// A keyboard key, as `std::system::Key`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Shift,
    Control,
    Alt,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
}

impl Key {
    /// Every key, in declaration order (the variant order of `std::system::Key`).
    pub const ALL: [Key; 64] = [
        Key::Enter,
        Key::Escape,
        Key::Space,
        Key::Backspace,
        Key::Tab,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::PageUp,
        Key::PageDown,
        Key::Home,
        Key::End,
        Key::Shift,
        Key::Control,
        Key::Alt,
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::Digit0,
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
    ];

    /// The key whose `std::system::Key` variant index is `index`.
    pub fn from_index(index: u32) -> Option<Key> {
        Key::ALL.get(usize::try_from(index).ok()?).copied()
    }
}

/// A mouse button, as `std::system::MouseButton` without the wheel directions (those arrive as
/// [`InputEvent::Wheel`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    /// Every button, in declaration order.
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];

    /// The button whose `std::system::MouseButton` variant index is `index` (the wheel
    /// directions, 5 and 6, are not buttons).
    pub fn from_index(index: u32) -> Option<MouseButton> {
        MouseButton::ALL.get(usize::try_from(index).ok()?).copied()
    }
}

/// A gamepad button by position, as `std::system::PadButton`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    Start,
    Select,
    LeftStick,
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl PadButton {
    /// Every button, in declaration order.
    pub const ALL: [PadButton; 16] = [
        PadButton::South,
        PadButton::East,
        PadButton::West,
        PadButton::North,
        PadButton::LeftShoulder,
        PadButton::RightShoulder,
        PadButton::LeftTrigger,
        PadButton::RightTrigger,
        PadButton::Start,
        PadButton::Select,
        PadButton::LeftStick,
        PadButton::RightStick,
        PadButton::DpadUp,
        PadButton::DpadDown,
        PadButton::DpadLeft,
        PadButton::DpadRight,
    ];

    /// The button whose `std::system::PadButton` variant index is `index`.
    pub fn from_index(index: u32) -> Option<PadButton> {
        PadButton::ALL.get(usize::try_from(index).ok()?).copied()
    }
}

/// One input event from the platform.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum InputEvent {
    Key {
        key: Key,
        pressed: bool,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    /// Design px; a point outside the view means no hover.
    MouseMove {
        x: f32,
        y: f32,
    },
    /// Notches; `> 0` is `WheelUp`.
    Wheel {
        delta_y: f32,
    },
    Pad {
        button: PadButton,
        pressed: bool,
    },
    /// Window focus; `false` releases held inputs.
    Focus(bool),
}
