//! The audio model: what plays on each game channel, one-shots, the voice and the system
//! music, turned into [`AudioCommand`]s for the platform. Every gain and fade is linear in
//! amplitude.

use kagari_vm::AssetId;

use super::{AudioCommand, Bus};
use crate::anim::decimal_f32;
use crate::screen::model::LogicalScreen;
use crate::values::{AudioFade, LoopSpec, SoundSpec, Volume};

/// A game channel: the screen's `ChannelId`.
pub(crate) use crate::screen::model::ChannelId as ChannelSlot;

#[derive(Default)]
struct ChannelPlay {
    track: Option<LoopSpec>,
    id: Option<u64>,
}

#[derive(Default)]
struct GameAudio {
    music: ChannelPlay,
    ambience: [ChannelPlay; 4],
    /// `(id, absolute end)`; `None`: unknown length.
    one_shots: Vec<(u64, Option<u64>)>,
    voice: Option<(u64, Option<u64>)>,
}

impl GameAudio {
    fn channel(&mut self, ch: ChannelSlot) -> &mut ChannelPlay {
        match ch {
            ChannelSlot::Music => &mut self.music,
            ChannelSlot::Ambience(index) => &mut self.ambience[usize::from(index).min(3)],
        }
    }

    fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.ambience
            .iter()
            .chain(std::iter::once(&self.music))
            .filter_map(|play| play.id)
            .chain(self.one_shots.iter().map(|(id, _)| *id))
            .chain(self.voice.map(|(id, _)| id))
    }
}

struct SystemMusic {
    requested: Option<LoopSpec>,
    fade: AudioFade,
    playing: ChannelPlay,
}

/// The ids and commands every operation appends to.
struct Out {
    next_id: u64,
    commands: Vec<AudioCommand>,
    heard: Vec<AssetId>,
}

impl Out {
    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn stop(&mut self, id: u64, fade_out_ms: u32) {
        self.commands.push(AudioCommand::Stop { id, fade_out_ms });
    }

    /// Starts `track` looping on `bus`; its id.
    fn play_loop(
        &mut self,
        track: &LoopSpec,
        bus: Bus,
        gain: f32,
        fade_in_ms: u32,
        delay_ms: u32,
    ) -> u64 {
        let id = self.id();
        self.commands.push(AudioCommand::Play {
            id,
            path: track.path.to_string(),
            bus,
            start_ms: 0,
            end_ms: None,
            loop_region: Some((track.loop_start_ms, track.loop_end_ms)),
            gain,
            fade_in_ms,
            delay_ms,
        });
        self.heard.push(track.audio);
        id
    }

    /// Switches `playing` to `track` with `fade`; the same track keeps playing.
    fn switch(
        &mut self,
        playing: &mut ChannelPlay,
        track: Option<&LoopSpec>,
        fade: AudioFade,
        gain: f32,
        bus: Bus,
    ) {
        if track == playing.track.as_ref() {
            return;
        }
        let old = playing.id.take();
        if let Some(old) = old {
            self.stop(old, fade.fade_out_ms);
        }
        playing.track = track.cloned();
        if let Some(track) = track {
            let delay = if fade.overlap || old.is_none() {
                0
            } else {
                fade.fade_out_ms
            };
            playing.id = Some(self.play_loop(track, bus, gain, fade.fade_in_ms, delay));
        }
    }

    fn play_sound(&mut self, s: &SoundSpec) -> u64 {
        let id = self.id();
        self.commands.push(AudioCommand::Play {
            id,
            path: s.path.to_string(),
            bus: Bus::Sound,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            loop_region: None,
            gain: decimal_f32(s.volume),
            fade_in_ms: 0,
            delay_ms: 0,
        });
        self.heard.push(s.audio);
        id
    }
}

pub(crate) struct AudioModel {
    out: Out,
    bus_gain: [Option<f32>; 3],
    game: Option<GameAudio>,
    system: SystemMusic,
}

const NO_FADE: AudioFade = AudioFade {
    fade_out_ms: 0,
    fade_in_ms: 0,
    overlap: false,
};

fn bus_gain(master: Volume, bus: Volume) -> f32 {
    if master.muted || bus.muted {
        0.0
    } else {
        (f32::from(master.level) / 100.0) * (f32::from(bus.level) / 100.0)
    }
}

impl AudioModel {
    pub fn new() -> AudioModel {
        AudioModel {
            out: Out {
                next_id: 1,
                commands: Vec::new(),
                heard: Vec::new(),
            },
            bus_gain: [None; 3],
            game: None,
            system: SystemMusic {
                requested: None,
                fade: NO_FADE,
                playing: ChannelPlay::default(),
            },
        }
    }

    /// The commands since the last call.
    pub fn take_commands(&mut self) -> Vec<AudioCommand> {
        std::mem::take(&mut self.out.commands)
    }

    pub fn set_volumes(&mut self, master: Volume, music: Volume, sound: Volume, voice: Volume) {
        let buses = [
            (Bus::Music, music),
            (Bus::Sound, sound),
            (Bus::Voice, voice),
        ];
        for (slot, (bus, volume)) in self.bus_gain.iter_mut().zip(buses) {
            let gain = bus_gain(master, volume);
            if *slot != Some(gain) {
                *slot = Some(gain);
                self.out
                    .commands
                    .push(AudioCommand::SetBusGain { bus, gain });
            }
        }
    }

    fn stop_game(&mut self) {
        if let Some(game) = self.game.take() {
            let ids: Vec<u64> = game.ids().collect();
            for id in ids {
                self.out.stop(id, 0);
            }
        }
    }

    /// A game starts: the system music fades out, and is kept to come back after the game.
    pub fn begin_game(&mut self) {
        self.stop_game();
        if let Some(id) = self.system.playing.id.take() {
            self.out.stop(id, self.system.fade.fade_out_ms);
        }
        self.system.playing.track = None;
        self.game = Some(GameAudio::default());
    }

    /// The game ends: everything it plays stops, and the system music comes back.
    pub fn end_game(&mut self) {
        self.stop_game();
        if let Some(track) = self.system.requested.clone()
            && self.system.playing.track.is_none()
        {
            let id = self
                .out
                .play_loop(&track, Bus::Music, 1.0, self.system.fade.fade_in_ms, 0);
            self.system.playing = ChannelPlay {
                track: Some(track),
                id: Some(id),
            };
        }
    }

    /// Makes every game channel play its logical track, without fades.
    pub fn sync_game(&mut self, logical: &LogicalScreen) {
        let Some(game) = self.game.as_mut() else {
            log::debug!("audio: no game to sync");
            return;
        };
        let channels = std::iter::once((ChannelSlot::Music, &logical.music)).chain(
            logical
                .ambience
                .iter()
                .zip(0u8..)
                .map(|(channel, i)| (ChannelSlot::Ambience(i), channel)),
        );
        for (slot, channel) in channels {
            let bus = if slot == ChannelSlot::Music {
                Bus::Music
            } else {
                Bus::Sound
            };
            self.out.switch(
                game.channel(slot),
                channel.track.as_ref(),
                NO_FADE,
                decimal_f32(channel.volume),
                bus,
            );
        }
    }

    /// A channel change of the game screen.
    pub fn change_channel(
        &mut self,
        ch: ChannelSlot,
        track: Option<&LoopSpec>,
        fade: AudioFade,
        gain: f32,
    ) {
        let Some(game) = self.game.as_mut() else {
            log::debug!("audio: channel change without a game");
            return;
        };
        let bus = if ch == ChannelSlot::Music {
            Bus::Music
        } else {
            Bus::Sound
        };
        self.out.switch(game.channel(ch), track, fade, gain, bus);
    }

    /// A channel volume tween; the ramp is linear, easing is ignored.
    pub fn set_channel_gain(&mut self, ch: ChannelSlot, gain: f32, tween_ms: u32) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        if let Some(id) = game.channel(ch).id {
            self.out
                .commands
                .push(AudioCommand::SetGain { id, gain, tween_ms });
        }
    }

    /// `replay_bgm()`: the music channel restarts.
    pub fn replay_music(&mut self) {
        if let Some(id) = self.game.as_ref().and_then(|game| game.music.id) {
            self.out.commands.push(AudioCommand::Restart { id });
        }
    }

    pub fn play_one_shot(&mut self, s: &SoundSpec, length_ms: Option<u32>, now_ms: u64) {
        let Some(game) = self.game.as_mut() else {
            log::debug!("audio: one-shot without a game");
            return;
        };
        // Forget the one-shots that have ended, so the list holds only those still playing.
        game.one_shots
            .retain(|(_, end)| end.is_none_or(|end| end > now_ms));
        let id = self.out.play_sound(s);
        game.one_shots
            .push((id, length_ms.map(|l| now_ms.saturating_add(u64::from(l)))));
    }

    /// Fades out every one-shot still playing; all are forgotten.
    pub fn stop_sounds(&mut self, fade_out_ms: u32, now_ms: u64) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        for (id, end) in std::mem::take(&mut game.one_shots) {
            if end.is_none_or(|end| end > now_ms) {
                self.out.stop(id, fade_out_ms);
            }
        }
    }

    /// A UI sound: played untracked, with or without a game.
    pub fn play_ui_sound(&mut self, s: &SoundSpec) {
        self.out.play_sound(s);
    }

    /// Plays a voice line after `delay_ms`, replacing the current one.
    pub fn play_voice(&mut self, path: &str, delay_ms: u32, length_ms: Option<u32>, now_ms: u64) {
        self.stop_voice();
        let Some(game) = self.game.as_mut() else {
            log::debug!("audio: voice without a game");
            return;
        };
        let id = self.out.id();
        self.out.commands.push(AudioCommand::Play {
            id,
            path: path.to_owned(),
            bus: Bus::Voice,
            start_ms: 0,
            end_ms: None,
            loop_region: None,
            gain: 1.0,
            fade_in_ms: 0,
            delay_ms,
        });
        let end = length_ms.map(|l| {
            now_ms
                .saturating_add(u64::from(delay_ms))
                .saturating_add(u64::from(l))
        });
        game.voice = Some((id, end));
    }

    pub fn stop_voice(&mut self) {
        if let Some((id, _)) = self.game.as_mut().and_then(|game| game.voice.take()) {
            self.out.stop(id, 0);
        }
    }

    /// When the current voice ends, in the story ms `play_voice` received; `None` when unknown
    /// or silent.
    pub fn voice_end_ms(&self) -> Option<u64> {
        self.game.as_ref()?.voice?.1
    }

    /// The system music channel, heard while no game runs.
    pub fn set_system_music(&mut self, track: Option<LoopSpec>, fade: AudioFade) {
        self.system.requested.clone_from(&track);
        self.system.fade = fade;
        if self.game.is_none() {
            self.out.switch(
                &mut self.system.playing,
                track.as_ref(),
                fade,
                1.0,
                Bus::Music,
            );
        }
    }

    /// The assets that started playing since the last call.
    pub fn drain_heard(&mut self) -> Vec<AssetId> {
        std::mem::take(&mut self.out.heard)
    }

    /// The crash path: everything stops at once.
    pub fn stop_all(&mut self) {
        self.stop_game();
        if let Some(id) = self.system.playing.id.take() {
            self.out.stop(id, 0);
        }
        self.system.playing.track = None;
        self.system.requested = None;
    }

    /// Takes over from the model of a program a debugger swapped out: everything `old` plays
    /// stops before anything this model already asked for.
    pub fn take_over(&mut self, mut old: AudioModel) {
        old.stop_all();
        let mut commands = old.take_commands();
        commands.append(&mut self.out.commands);
        self.out.commands = commands;
    }
}
