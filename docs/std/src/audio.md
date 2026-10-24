# Audio

`std::screen::audio` declares looping tracks, one-shot sounds, channel fades and the `Channel`
state type. Audio is part of the screen: a render changes it like any other component, and the
channels are part of the logical screen. Every item is `const fn`.

```kis
use crate::time::{Duration, ZERO};
```

## Channels

```kis
impl Screen {
    #[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
    #[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
    #[builtin] pub const fn play(&mut self, sound: audio::Sound);
    #[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
}

#[builtin] pub struct Channel;
impl Channel {
    #[builtin] pub const fn get(&self) -> Option<Loop>;
    #[builtin] pub const fn change(&mut self, track: Loop) -> &mut ChannelChange;
    #[builtin] pub const fn stop(&mut self) -> &mut ChannelChange;
    #[builtin] pub const fn volume(&mut self) -> &mut crate::screen::Prop<Decimal>;
}
#[must_use] #[builtin] pub struct ChannelChange;
impl ChannelChange {
    #[builtin] pub const fn apply_animation(&mut self, fade: AudioFade);
}
```

- A screen has one music channel and four ambience channels, `ambience(0)` to `ambience(3)`. Each
  channel plays at most one looping track.
- `get()` is the track of the logical screen, or `None` when the channel is silent.
- `change(l)` and `stop()` are pending changes; `ChannelChange` is `#[must_use]` and happens only
  through `apply_animation`.
- `volume()` is the channel volume, a `Prop<Decimal>` in `0.0..=1.0` changed with a tween.

### The same-track rule

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

```kis
#[builtin] pub const fn replay_bgm(&mut self);   // in impl Screen
```

## System music

```kis
/// Sets the system music channel: heard while no game runs on the mode stack; while a game
/// runs, the game screen's music channel plays instead. `None` stops it.
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

`std::system::set_music` is a `fn`: system code calls it, for example on the title page. See
[System](./system.md).

## `Loop`

```kis
pub struct Loop { pub audio: AudioAsset, pub loop_start: Duration, pub loop_end: Option<Duration> }
pub const fn looped(audio: AudioAsset) -> Loop { Loop { audio, loop_start: ZERO, loop_end: None } }
impl Loop { pub const fn region(mut self, start: Duration, end: Option<Duration>) -> Loop { … } }
```

A loop plays from 0. At `loop_end` (or the end of the file, with `None`) it jumps to `loop_start`.
That is how an intro followed by a loop is written: the part before `loop_start` plays once.

```kis
screen.music().change(looped(THEME).region(secs(16), Some(secs(80)))).apply_animation(crossfade(ms(1000)));
screen.ambience(0).change(looped(RAIN)).apply_animation(crossfade(ms(2000)));
```

## `Sound`

```kis
pub struct Sound { pub audio: AudioAsset, pub volume: Decimal, pub start: Duration, pub end: Option<Duration>, pub delay: Duration }
pub const fn sound(audio: AudioAsset) -> Sound { Sound { audio, volume: 1.0, start: ZERO, end: None, delay: ZERO } }
impl Sound {
    pub const fn volume(mut self, volume: Decimal) -> Sound { … }
    pub const fn range(mut self, start: Duration, end: Option<Duration>) -> Sound { … }
    pub const fn delay(mut self, delay: Duration) -> Sound { … }
}
```

A one-shot sound plays once:

- `play(s)` starts at `cursor + s.delay` and plays the `start..end` range of the file (`None`: to
  its end).
- It is not part of the logical screen and keeps playing across steps.
- `stop_sounds(d)` fades out, over `d`, every one-shot that is playing at the cursor.
- At step entry, one-shots that have not started yet are dropped; so are the not-yet-started ones
  when the player clicks.

A burst is several one-shots with delays:

```kis
const fn knocking(mut screen: Screen) -> Screen {
    screen.play(sound(KNOCK));
    screen.play(sound(KNOCK).delay(ms(250)));
    screen.play(sound(KNOCK).volume(0.6).delay(ms(500)));
    screen
}
```

## `AudioFade`

```kis
pub struct AudioFade { pub fade_out: Duration, pub fade_in: Duration, pub overlap: bool }
pub const fn crossfade(duration: Duration) -> AudioFade { AudioFade { fade_out: duration, fade_in: duration, overlap: true } }
pub const fn fade_out_in(fade_out: Duration, fade_in: Duration) -> AudioFade { AudioFade { fade_out, fade_in, overlap: false } }
pub const fn switch() -> AudioFade { AudioFade { fade_out: ZERO, fade_in: ZERO, overlap: false } }
```

|Constructor|Effect|
|---|---|
|`crossfade(d)`|old out and new in over `d`, at the same time|
|`fade_out_in(o, i)`|old out over `o`, then new in over `i`|
|`switch()`|stops the old track and starts the new one at once|

## Volume

Final volume = master × bus × channel volume × sound volume.

|Source|Bus|
|---|---|
|the music channel|music|
|ambience channels, one-shot sounds, UI sounds|sound|
|voice|voice|

- The master and bus volumes are the `Settings` fields `master`, `music`, `sound` and `voice`,
  each a `Volume` whose `level` is `Int<0, 100>`. A muted bus counts as 0.
- Channel and sound volumes are fractions `0.0..=1.0`.

## Voice

The voice of a line is `Say::voice`, written `s!(voice = AZUSA_0072, …)`.

- It starts with the text, at `begin_text`, which is the step entry unless the render moves it.
- It stops at the next step entry, unless the `voice_sustain` setting is on.
- Auto mode waits for it if the `wait_for_voice` setting is on.
- The backlog can play it again with `Narrative::replay_voice`.
