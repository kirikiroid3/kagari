//! Gamepads through gilrs.

use std::collections::{HashMap, HashSet};

use gilrs::{Button, Event, EventType, GamepadId, Gilrs};
use kagari_engine::{InputEvent, PadButton};

/// The engine button of a gilrs button; `C`, `Z`, `Mode` and `Unknown` have none.
pub fn map_button(button: Button) -> Option<PadButton> {
    Some(match button {
        Button::South => PadButton::South,
        Button::East => PadButton::East,
        Button::West => PadButton::West,
        Button::North => PadButton::North,
        Button::LeftTrigger => PadButton::LeftShoulder,
        Button::RightTrigger => PadButton::RightShoulder,
        Button::LeftTrigger2 => PadButton::LeftTrigger,
        Button::RightTrigger2 => PadButton::RightTrigger,
        Button::Start => PadButton::Start,
        Button::Select => PadButton::Select,
        Button::LeftThumb => PadButton::LeftStick,
        Button::RightThumb => PadButton::RightStick,
        Button::DPadUp => PadButton::DpadUp,
        Button::DPadDown => PadButton::DpadDown,
        Button::DPadLeft => PadButton::DpadLeft,
        Button::DPadRight => PadButton::DpadRight,
        Button::C | Button::Z | Button::Mode | Button::Unknown => return None,
    })
}

/// Every connected gamepad; an engine button is pressed while any pad holds it.
pub struct Pads {
    gilrs: Option<Gilrs>,
    held: HashMap<GamepadId, HashSet<PadButton>>,
    counts: HashMap<PadButton, u32>,
}

impl Pads {
    pub fn new() -> Pads {
        let gilrs = match Gilrs::new() {
            Ok(g) => Some(g),
            Err(gilrs::Error::NotImplemented(dummy)) => Some(dummy),
            Err(e) => {
                log::warn!("gamepad support unavailable ({e})");
                None
            }
        };
        Pads {
            gilrs,
            held: HashMap::new(),
            counts: HashMap::new(),
        }
    }

    /// The engine events of every gamepad event since the last call.
    pub fn poll(&mut self) -> Vec<InputEvent> {
        let mut out = Vec::new();
        let Some(gilrs) = self.gilrs.as_mut() else {
            return out;
        };
        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            match event {
                EventType::ButtonPressed(b, _) => {
                    if let Some(button) = map_button(b)
                        && self.held.entry(id).or_default().insert(button)
                    {
                        let count = self.counts.entry(button).or_insert(0);
                        *count = count.saturating_add(1);
                        if *count == 1 {
                            out.push(InputEvent::Pad {
                                button,
                                pressed: true,
                            });
                        }
                    }
                }
                EventType::ButtonReleased(b, _) => {
                    if let Some(button) = map_button(b)
                        && self.held.get_mut(&id).is_some_and(|h| h.remove(&button))
                    {
                        release(&mut self.counts, button, &mut out);
                    }
                }
                EventType::Disconnected => {
                    for button in self.held.remove(&id).unwrap_or_default() {
                        release(&mut self.counts, button, &mut out);
                    }
                }
                _ => {}
            }
        }
        out
    }
}

fn release(counts: &mut HashMap<PadButton, u32>, button: PadButton, out: &mut Vec<InputEvent>) {
    let count = counts.entry(button).or_insert(0);
    *count = count.saturating_sub(1);
    if *count == 0 {
        out.push(InputEvent::Pad {
            button,
            pressed: false,
        });
    }
}
