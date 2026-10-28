//! `std::screen::audio`: the channel imports, the commands the engine sends to the platform's
//! audio backend, and the audio model that produces them.

pub(crate) mod model;
#[cfg(test)]
mod tests;

use kagari_vm::{Trap, Value, Vm};

use crate::HostCtx;
use crate::screen::model::{Animation, PartTarget, PendingChange, PropTarget};
use crate::screen::parts::{Receiver, part_ref, receiver};
use crate::screen::{commit, part_of, pending, pending_of, read_screen};
use crate::values::{self, decode_audio_fade, decode_loop_spec, host_trap, loop_value};

pub(crate) use model::AudioModel;

/// Records what started playing as heard; a new entry invalidates `Heard`.
pub(crate) fn flush_heard(
    audio: &mut AudioModel,
    system: &mut crate::system::SystemState,
    ui: &mut crate::ui::UiState,
) {
    for id in audio.drain_heard() {
        if system.mark_heard(id) {
            ui.invalidate(crate::ui::SourceKey::Heard(id));
        }
    }
}

/// A mixer bus.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Bus {
    Music,
    Sound,
    Voice,
}

/// What the platform's audio backend does. Gains are linear amplitude.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioCommand {
    /// Master × bus volume, already multiplied.
    SetBusGain {
        bus: Bus,
        gain: f32,
    },
    Play {
        id: u64,
        path: String,
        bus: Bus,
        start_ms: u32,
        end_ms: Option<u32>,
        loop_region: Option<(u32, Option<u32>)>,
        gain: f32,
        fade_in_ms: u32,
        delay_ms: u32,
    },
    SetGain {
        id: u64,
        gain: f32,
        tween_ms: u32,
    },
    Stop {
        id: u64,
        fade_out_ms: u32,
    },
    Restart {
        id: u64,
    },
}

import_table!(
    /// The `#[builtin]` methods of `std::screen::audio`.
    AudioImport {
        Get = "std::screen::audio::Channel::get",
        Change = "std::screen::audio::Channel::change",
        Stop = "std::screen::audio::Channel::stop",
        Volume = "std::screen::audio::Channel::volume",
        FadeApplyAnimation = "std::screen::audio::ChannelChange::apply_animation",
    }
);

impl AudioImport {
    /// Runs the import on `args` (the receiver first).
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use AudioImport as I;
        let path = self.path();
        let std = &ctx.state.std;
        let rcv = receiver(vm, path, args.first())?;
        let part = match self {
            I::FadeApplyAnimation => {
                let pending_change = pending_of(rcv, path)?;
                if !matches!(pending_change.change, PendingChange::Channel(..)) {
                    return Err(host_trap(format!(
                        "`{path}`: receiver is not a `ChannelChange`"
                    )));
                }
                let fade = decode_audio_fade(std, &values::arg(vm, path, &args, 1)?)?;
                return commit(vm, pending_change, Animation::Fade(fade));
            }
            I::Get | I::Change | I::Stop | I::Volume => part_of(rcv, path)?,
        };
        let PartTarget::Channel(channel) = part.target else {
            return Err(host_trap(format!("`{path}`: receiver is not a `Channel`")));
        };
        if self == I::Get {
            let rcv = Receiver::Part(part);
            let track = read_screen(vm, &rcv, |s| Ok(s.logical.channel(channel).track.clone()))?;
            return Ok(match track {
                Some(track) => std.some(loop_value(std, &track)),
                None => std.none(),
            });
        }
        if self == I::Change {
            let value = values::arg(vm, path, &args, 1)?;
            let track = decode_loop_spec(std, &ctx.state.program, &value)?;
            return pending(&part.screen, PendingChange::Channel(channel, Some(track)));
        }
        if self == I::Stop {
            return pending(&part.screen, PendingChange::Channel(channel, None));
        }
        Ok(part_ref(
            &part.screen,
            PartTarget::Prop(PropTarget::ChannelVolume(channel)),
        ))
    }
}
