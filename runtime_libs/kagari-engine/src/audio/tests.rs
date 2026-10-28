#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use kagari_vm::AssetId;

use super::model::{AudioModel, ChannelSlot};
use super::{AudioCommand, Bus};
use crate::values::{AudioFade, LoopSpec, Volume};

fn track(id: u32, path: &str) -> LoopSpec {
    LoopSpec {
        audio: AssetId(id),
        path: Rc::from(path),
        loop_start_ms: 0,
        loop_end_ms: None,
    }
}

fn crossfade(ms: u32) -> AudioFade {
    AudioFade {
        fade_out_ms: ms,
        fade_in_ms: ms,
        overlap: true,
    }
}

fn playing_a() -> AudioModel {
    let mut audio = AudioModel::new();
    audio.begin_game();
    audio.change_channel(
        ChannelSlot::Music,
        Some(&track(0, "a.ogg")),
        crossfade(0),
        1.0,
    );
    assert_eq!(audio.take_commands().len(), 1);
    audio
}

#[test]
fn same_track_keeps_playing() {
    let mut audio = playing_a();
    audio.change_channel(
        ChannelSlot::Music,
        Some(&track(0, "a.ogg")),
        crossfade(1000),
        1.0,
    );
    assert!(audio.take_commands().is_empty());
}

fn play_b(fade_in_ms: u32, delay_ms: u32) -> AudioCommand {
    AudioCommand::Play {
        id: 2,
        path: "b.ogg".into(),
        bus: Bus::Music,
        start_ms: 0,
        end_ms: None,
        loop_region: Some((0, None)),
        gain: 1.0,
        fade_in_ms,
        delay_ms,
    }
}

#[test]
fn crossfade_between_tracks() {
    let mut audio = playing_a();
    audio.change_channel(
        ChannelSlot::Music,
        Some(&track(1, "b.ogg")),
        crossfade(1000),
        1.0,
    );
    assert_eq!(
        audio.take_commands(),
        [
            AudioCommand::Stop {
                id: 1,
                fade_out_ms: 1000
            },
            play_b(1000, 0)
        ]
    );
}

#[test]
fn fade_out_then_in() {
    let mut audio = playing_a();
    let fade = AudioFade {
        fade_out_ms: 500,
        fade_in_ms: 800,
        overlap: false,
    };
    audio.change_channel(ChannelSlot::Music, Some(&track(1, "b.ogg")), fade, 1.0);
    assert_eq!(
        audio.take_commands(),
        [
            AudioCommand::Stop {
                id: 1,
                fade_out_ms: 500
            },
            play_b(800, 500)
        ]
    );
}

#[test]
fn bus_gains_are_sent_when_they_change() {
    let mut audio = AudioModel::new();
    let full = Volume {
        level: 100,
        muted: false,
    };
    let half = Volume {
        level: 50,
        muted: false,
    };
    audio.set_volumes(full, half, full, full);
    assert_eq!(audio.take_commands().len(), 3);
    audio.set_volumes(
        full,
        half,
        Volume {
            level: 100,
            muted: true,
        },
        full,
    );
    assert_eq!(
        audio.take_commands(),
        [AudioCommand::SetBusGain {
            bus: Bus::Sound,
            gain: 0.0
        }]
    );
}

#[test]
fn system_music_comes_back_after_the_game() {
    let mut audio = AudioModel::new();
    audio.set_system_music(Some(track(0, "title.ogg")), crossfade(200));
    assert!(matches!(
        audio.take_commands().as_slice(),
        [AudioCommand::Play { id: 1, .. }]
    ));
    audio.begin_game();
    assert_eq!(
        audio.take_commands(),
        [AudioCommand::Stop {
            id: 1,
            fade_out_ms: 200
        }]
    );
    audio.end_game();
    assert!(matches!(
        audio.take_commands().as_slice(),
        [AudioCommand::Play {
            id: 2,
            fade_in_ms: 200,
            ..
        }]
    ));
    assert_eq!(audio.drain_heard(), [AssetId(0), AssetId(0)]);
}

#[test]
fn finished_one_shots_are_forgotten() {
    let sound = crate::values::SoundSpec {
        audio: AssetId(3),
        path: Rc::from("s.ogg"),
        volume: kagari_vm::Decimal::ONE,
        start_ms: 0,
        end_ms: None,
        delay_ms: 0,
    };
    let mut audio = AudioModel::new();
    audio.begin_game();
    audio.play_one_shot(&sound, Some(100), 0);
    audio.play_one_shot(&sound, Some(100), 500);
    audio.take_commands();
    audio.end_game();
    // Only the one-shot still playing at the second play is stopped.
    assert_eq!(
        audio.take_commands(),
        [AudioCommand::Stop {
            id: 2,
            fade_out_ms: 0
        }]
    );
}
