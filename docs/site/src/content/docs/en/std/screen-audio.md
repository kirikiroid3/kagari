---
title: "std::screen::audio"
---

> **Stability:** Stable — audio on the screen: looping channels and one-shot sounds.

Audio is part of the screen: a render changes it like any other component, and the channels are
part of the [logical screen](./screen.md#the-logical-screen). This module declares looping tracks
([`Loop`](#loop)), one-shot sounds ([`Sound`](#sound)), channel fades ([`AudioFade`](#audiofade))
and the [`Channel`](#channel) state type. Every item is `const fn` or plain data.

The prelude re-exports `looped`, `sound` and `crossfade`. The rest needs a
`use std::screen::audio::…`.

Which audio formats a game may ship depends on the runtime profile; `native_v0` accepts Ogg
Vorbis, Ogg FLAC, FLAC, WAV, MP3, AAC-LC (ADTS or MP4/M4A) and ALAC (MP4/M4A), and rejects Ogg
Opus. See [Runtime Profiles](./appendix/profiles.md).

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Loop`](#loop), `looped` | struct + constructor | a looping track |
| [`Sound`](#sound), `sound` | struct + constructor | a one-shot sound |
| [`AudioFade`](#audiofade), `crossfade`, `fade_out_in`, `switch` | struct + constructors | how a channel changes track |
| [`Channel`](#channel) | builtin struct | a channel of the screen |
| [`ChannelChange`](#channelchange) | builtin struct | a pending channel change |

## Channels {#channels}

```kis
impl Screen {
    #[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
    #[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
    #[builtin] pub const fn play(&mut self, sound: audio::Sound);
    #[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
    #[builtin] pub const fn replay_bgm(&mut self);
}
```

- A screen has one music channel and four ambience channels, `ambience(0)` to `ambience(3)`. Each
  channel plays at most one looping track.
- One-shot sounds are played with `play` and are not on a channel.
- `Screen::empty()` has every channel silent with volume 1.0.

### The same-track rule {#the-same-track-rule}

- `change(l)` with `l` structurally equal to the current track is a no-op: the track keeps playing
  and does not restart. So every step of a scene can state its music, and only an actual change
  is heard.
- Otherwise the old track fades out over `fade_out` and the new one fades in over `fade_in`. They
  run at the same time if `overlap` is true, one after the other if it is false.
- `stop()` fades the current track out over `fade_out`.

Music carries over between steps: a track that is already playing keeps playing across steps and
never restarts on its own. A render restarts it from its start, at the timeline cursor, with
`screen.replay_bgm()`. The replay is scheduled on the timeline only; the logical screen does not
change, so it does not affect screen equality. Without a track, `replay_bgm()` does nothing.

### System music {#system-music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

`std::system::set_music` sets the system music channel: heard while no game runs on the mode
stack; while a game runs, the game screen's music channel plays instead. `None` stops it. It is a
`fn`: system code calls it, for example on the title page. See [`std::system`](./system.md).

## Struct: `Loop` {#loop}

```kis
pub struct Loop {
    pub audio: AudioAsset,
    pub loop_start: Duration,
    pub loop_end: Option<Duration>,
}
```

A track that loops: it plays from 0 and jumps from `loop_end` (or the end of the file, with
`None`) back to `loop_start`. The part before `loop_start` plays once, as an intro.

### `looped(audio)` {#looped}

```kis
pub const fn looped(audio: AudioAsset) -> Loop
```

- Returns: `audio`, looping as a whole (`loop_start` 0, `loop_end` `None`).
- Kind: `const fn` · Builtin: no.

### `track.region(start, end)` {#loop-region}

```kis
pub const fn region(mut self, start: Duration, end: Option<Duration>) -> Loop
```

- Returns: this track looping over `start..end`; `None` is the end of the file.
- Kind: `const fn` · Builtin: no.

```kis
screen.music().change(looped(THEME).region(secs(16), Some(secs(80)))).apply_animation(crossfade(ms(1000)));
screen.ambience(0).change(looped(RAIN)).apply_animation(crossfade(ms(2000)));
```

## Struct: `Sound` {#sound}

```kis
pub struct Sound {
    pub audio: AudioAsset,
    pub volume: Decimal,
    pub start: Duration,
    pub end: Option<Duration>,
    pub delay: Duration,
}
```

A one-shot sound. `volume` is `0.0..=1.0`; `end` `None` is the end of the file.

- `screen.play(s)` starts at `cursor + s.delay` and plays the `start..end` range of the file.
- A one-shot is not part of the logical screen and keeps playing across steps.
- `screen.stop_sounds(d)` fades out, over `d`, every one-shot that is playing at the cursor.
- At step entry, one-shots that have not started yet are dropped; so are the not-yet-started ones
  when the player clicks.

### `sound(audio)` {#sound-fn}

```kis
pub const fn sound(audio: AudioAsset) -> Sound
```

- Returns: `audio`, played whole at full volume, without delay.
- Kind: `const fn` · Builtin: no.

### `sound.volume(volume)`, `sound.range(start, end)`, `sound.delay(delay)` {#sound-volume}

```kis
pub const fn volume(mut self, volume: Decimal) -> Sound
pub const fn range(mut self, start: Duration, end: Option<Duration>) -> Sound
pub const fn delay(mut self, delay: Duration) -> Sound
```

| Method | Returns |
| ------ | ------- |
| `volume(v)` | this sound at volume `v`, `0.0..=1.0` |
| `range(start, end)` | this sound playing only `start..end` of the file |
| `delay(d)` | this sound, starting `d` after the cursor |

Kind: `const fn` · Builtin: no.

A burst is several one-shots with delays:

```kis
const fn knocking(mut screen: Screen) -> Screen {
    screen.play(sound(KNOCK));
    screen.play(sound(KNOCK).delay(ms(250)));
    screen.play(sound(KNOCK).volume(0.6).delay(ms(500)));
    screen
}
```

## Struct: `AudioFade` {#audiofade}

```kis
pub struct AudioFade {
    pub fade_out: Duration,
    pub fade_in: Duration,
    pub overlap: bool,
}
pub const fn crossfade(duration: Duration) -> AudioFade
pub const fn fade_out_in(fade_out: Duration, fade_in: Duration) -> AudioFade
pub const fn switch() -> AudioFade
```

How a channel goes from its old track to its new one. `overlap` says whether the fades run at the
same time or one after the other. Each constructor is `const fn`, not builtin.

| Constructor | Effect |
| ----------- | ------ |
| `crossfade(d)` | old out and new in over `d`, at the same time |
| `fade_out_in(o, i)` | old out over `o`, then new in over `i` |
| `switch()` | stops the old track and starts the new one at once |

## Struct: `Channel` {#channel}

```kis
#[builtin] pub struct Channel;
impl Channel {
    #[builtin] pub const fn get(&self) -> Option<Loop>;
    #[builtin] pub const fn change(&mut self, track: Loop) -> &mut ChannelChange;
    #[builtin] pub const fn stop(&mut self) -> &mut ChannelChange;
    #[builtin] pub const fn volume(&mut self) -> &mut crate::screen::Prop<Decimal>;
}
```

A channel that plays one looping track. Kind: `const fn` · Builtin: yes.

- `get()` is the track of the logical screen, or `None` when the channel is silent.
- `change(track)` changes the track, following [the same-track rule](#the-same-track-rule).
- `stop()` stops the track.
- `volume()` is the channel volume, a [`Prop<Decimal>`](./screen.md#prop) in `0.0..=1.0` changed
  with a tween.

```kis
screen.music().volume().change(0.5).apply_animation(tween(ms(1000)));
screen.ambience(0).stop().apply_animation(fade_out_in(ms(2000), ms(0)));
```

## Struct: `ChannelChange` {#channelchange}

```kis
#[must_use] #[builtin] pub struct ChannelChange;
impl ChannelChange {
    #[builtin] pub const fn apply_animation(&mut self, fade: AudioFade);
}
```

A pending channel change; it happens only through `apply_animation`, which fades from the old track
to the new one. `ChannelChange` is `#[must_use]`. Kind: `const fn` · Builtin: yes.

## Volume {#volume}

Final volume = master × bus × channel volume × sound volume.

| Source | Bus |
| ------ | --- |
| the music channel | music |
| ambience channels, one-shot sounds, UI sounds | sound |
| voice | voice |

- The master and bus volumes are the `Settings` fields `master`, `music`, `sound` and `voice`,
  each a `Setting<Volume>` whose `level` is `Int<0, 100>`. A muted bus counts as 0.
- Channel and sound volumes are fractions `0.0..=1.0`.

## Voice {#voice}

The voice of a line is `Say::voice`, written `s!(voice = AZUSA_0072, …)`.

- It starts with the text, at [`begin_text`](./screen-anim.md#the-timeline), which is the step
  entry unless the render moves it.
- It stops at the next step entry, unless the `voice_sustain` setting is on.
- Auto mode waits for it if the `wait_for_voice` setting is on.
- The backlog can play it again with `Narrative::replay_voice`.
