//! winit input → engine [`InputEvent`]s.

use std::collections::{HashMap, HashSet};

use kagari_engine::{InputEvent, Key, MouseButton};
use winit::event::{ElementState, KeyEvent, MouseScrollDelta};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The engine key of a physical key code.
pub fn map_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Space => Key::Space,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,
        KeyCode::Digit0 => Key::Digit0,
        KeyCode::Digit1 => Key::Digit1,
        KeyCode::Digit2 => Key::Digit2,
        KeyCode::Digit3 => Key::Digit3,
        KeyCode::Digit4 => Key::Digit4,
        KeyCode::Digit5 => Key::Digit5,
        KeyCode::Digit6 => Key::Digit6,
        KeyCode::Digit7 => Key::Digit7,
        KeyCode::Digit8 => Key::Digit8,
        KeyCode::Digit9 => Key::Digit9,
        _ => return None,
    })
}

/// The primary physical key code of an engine key (the inverse of [`map_key`]); exhaustive, so
/// a new `Key` variant fails to compile until it is mapped. The round-trip test uses it.
#[cfg(test)]
pub fn key_code(key: Key) -> KeyCode {
    match key {
        Key::Enter => KeyCode::Enter,
        Key::Escape => KeyCode::Escape,
        Key::Space => KeyCode::Space,
        Key::Backspace => KeyCode::Backspace,
        Key::Tab => KeyCode::Tab,
        Key::Up => KeyCode::ArrowUp,
        Key::Down => KeyCode::ArrowDown,
        Key::Left => KeyCode::ArrowLeft,
        Key::Right => KeyCode::ArrowRight,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::Shift => KeyCode::ShiftLeft,
        Key::Control => KeyCode::ControlLeft,
        Key::Alt => KeyCode::AltLeft,
        Key::F1 => KeyCode::F1,
        Key::F2 => KeyCode::F2,
        Key::F3 => KeyCode::F3,
        Key::F4 => KeyCode::F4,
        Key::F5 => KeyCode::F5,
        Key::F6 => KeyCode::F6,
        Key::F7 => KeyCode::F7,
        Key::F8 => KeyCode::F8,
        Key::F9 => KeyCode::F9,
        Key::F10 => KeyCode::F10,
        Key::F11 => KeyCode::F11,
        Key::F12 => KeyCode::F12,
        Key::A => KeyCode::KeyA,
        Key::B => KeyCode::KeyB,
        Key::C => KeyCode::KeyC,
        Key::D => KeyCode::KeyD,
        Key::E => KeyCode::KeyE,
        Key::F => KeyCode::KeyF,
        Key::G => KeyCode::KeyG,
        Key::H => KeyCode::KeyH,
        Key::I => KeyCode::KeyI,
        Key::J => KeyCode::KeyJ,
        Key::K => KeyCode::KeyK,
        Key::L => KeyCode::KeyL,
        Key::M => KeyCode::KeyM,
        Key::N => KeyCode::KeyN,
        Key::O => KeyCode::KeyO,
        Key::P => KeyCode::KeyP,
        Key::Q => KeyCode::KeyQ,
        Key::R => KeyCode::KeyR,
        Key::S => KeyCode::KeyS,
        Key::T => KeyCode::KeyT,
        Key::U => KeyCode::KeyU,
        Key::V => KeyCode::KeyV,
        Key::W => KeyCode::KeyW,
        Key::X => KeyCode::KeyX,
        Key::Y => KeyCode::KeyY,
        Key::Z => KeyCode::KeyZ,
        Key::Digit0 => KeyCode::Digit0,
        Key::Digit1 => KeyCode::Digit1,
        Key::Digit2 => KeyCode::Digit2,
        Key::Digit3 => KeyCode::Digit3,
        Key::Digit4 => KeyCode::Digit4,
        Key::Digit5 => KeyCode::Digit5,
        Key::Digit6 => KeyCode::Digit6,
        Key::Digit7 => KeyCode::Digit7,
        Key::Digit8 => KeyCode::Digit8,
        Key::Digit9 => KeyCode::Digit9,
    }
}

/// The engine button of a winit mouse button; `Other(_)` has none.
pub fn map_mouse(button: winit::event::MouseButton) -> Option<MouseButton> {
    use winit::event::MouseButton as W;
    Some(match button {
        W::Left => MouseButton::Left,
        W::Right => MouseButton::Right,
        W::Middle => MouseButton::Middle,
        W::Back => MouseButton::Back,
        W::Forward => MouseButton::Forward,
        W::Other(_) => return None,
    })
}

/// A wheel event in notches; `None` for no movement.
pub fn wheel(delta: MouseScrollDelta) -> Option<InputEvent> {
    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
    };
    (delta_y != 0.0).then_some(InputEvent::Wheel { delta_y })
}

/// The physical keys held down, so an engine key is pressed when its first code goes down and
/// released when its last code comes up (both Shift keys are one `Key::Shift`).
#[derive(Default)]
pub struct HeldInputs {
    codes: HashSet<KeyCode>,
    counts: HashMap<Key, u32>,
}

impl HeldInputs {
    /// The engine event of a winit key event, if it changes an engine key's state. Repeats and
    /// keys without an engine key give `None`.
    pub fn key(&mut self, event: &KeyEvent) -> Option<InputEvent> {
        if event.repeat {
            return None;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return None;
        };
        let key = map_key(code)?;
        let count = self.counts.entry(key).or_insert(0);
        match event.state {
            ElementState::Pressed => {
                if !self.codes.insert(code) {
                    return None;
                }
                *count = count.saturating_add(1);
                (*count == 1).then_some(InputEvent::Key { key, pressed: true })
            }
            ElementState::Released => {
                if !self.codes.remove(&code) {
                    return None;
                }
                *count = count.saturating_sub(1);
                (*count == 0).then_some(InputEvent::Key {
                    key,
                    pressed: false,
                })
            }
        }
    }

    /// Window focus lost: every held key is forgotten (the engine releases them on
    /// `Focus(false)`).
    pub fn clear(&mut self) {
        self.codes.clear();
        self.counts.clear();
    }
}
