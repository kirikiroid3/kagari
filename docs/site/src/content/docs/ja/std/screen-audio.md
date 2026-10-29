---
title: "std::screen::audio"
---

> **安定性:** 安定 — 画面上のオーディオです。ループするチャンネルとワンショットサウンドを扱います。

オーディオは画面の一部です。レンダーはほかのコンポーネントと同じようにオーディオを変更し、チャンネルは[論理画面](./screen.md#the-logical-screen)に含まれます。このモジュールは、ループするトラック（[`Loop`](#loop)）、ワンショットサウンド（[`Sound`](#sound)）、チャンネルのフェード（[`AudioFade`](#audiofade)）、状態型 [`Channel`](#channel) を宣言します。アイテムはすべて `const fn` か単純なデータです。

プレリュードは `looped`、`sound`、`crossfade` を再エクスポートします。残りには `use std::screen::audio::…` が必要です。

ゲームに同梱できる音声形式はランタイムプロファイルによって決まります。`native_v0` は Ogg Vorbis、FLAC、WAV、MP3 を受け付け、Ogg Opus は受け付けません。[ランタイムプロファイル](./appendix/profiles.md) を参照してください。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Loop`](#loop)、`looped` | 構造体 + コンストラクター | ループするトラック |
| [`Sound`](#sound)、`sound` | 構造体 + コンストラクター | ワンショットサウンド |
| [`AudioFade`](#audiofade)、`crossfade`、`fade_out_in`、`switch` | 構造体 + コンストラクター | チャンネルのトラックの切り替え方 |
| [`Channel`](#channel) | ビルトイン構造体 | 画面のチャンネル |
| [`ChannelChange`](#channelchange) | ビルトイン構造体 | 保留中のチャンネル変更 |

## チャンネル {#channels}

```kis
impl Screen {
    #[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
    #[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
    #[builtin] pub const fn play(&mut self, sound: audio::Sound);
    #[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
    #[builtin] pub const fn replay_bgm(&mut self);
}
```

- 画面は音楽チャンネル 1 つと、環境音チャンネル 4 つ（`ambience(0)` 〜 `ambience(3)`）を持ちます。各チャンネルはループするトラックを最大 1 つ再生します。
- ワンショットサウンドは `play` で再生し、チャンネルには乗りません。
- `Screen::empty()` ではすべてのチャンネルが無音で、音量は 1.0 です。

### 同一トラック規則 {#the-same-track-rule}

- 現在のトラックと構造的に等しい `l` での `change(l)` は何もしません。トラックは再開されずに再生を続けます。そのためシーンの各ステップで音楽を明示しても、実際に変わったときだけ聞こえ方が変わります。
- そうでなければ、古いトラックが `fade_out` かけてフェードアウトし、新しいトラックが `fade_in` かけてフェードインします。`overlap` が true なら同時に、false なら順番に行われます。
- `stop()` は現在のトラックを `fade_out` かけてフェードアウトします。

音楽はステップをまたいで引き継がれます。すでに再生中のトラックはステップをまたいで再生を続け、ひとりでに再開することはありません。レンダーは `screen.replay_bgm()` で、タイムラインのカーソル位置からトラックを先頭から再開できます。再開はタイムラインにだけスケジュールされ、論理画面は変わらないので画面の等価性には影響しません。トラックがなければ `replay_bgm()` は何もしません。

### システム音楽 {#system-music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

`std::system::set_music` はシステム音楽チャンネルを設定します。モードスタック上でゲームが実行されていない間に聞こえ、ゲームの実行中はゲーム画面の音楽チャンネルが代わりに鳴ります。`None` は停止です。これは `fn` で、たとえばタイトルページなどシステムのコードから呼びます。[`std::system`](./system.md) を参照してください。

## 構造体：`Loop` {#loop}

```kis
pub struct Loop {
    pub audio: AudioAsset,
    pub loop_start: Duration,
    pub loop_end: Option<Duration>,
}
```

ループするトラックです。0 から再生し、`loop_end`（`None` ならファイルの終わり）から `loop_start` へ戻ります。`loop_start` より前の部分はイントロとして 1 回だけ再生されます。

### `looped(audio)` {#looped}

```kis
pub const fn looped(audio: AudioAsset) -> Loop
```

- 戻り値：全体をループする `audio`（`loop_start` 0、`loop_end` `None`）。
- 種別：`const fn` · ビルトイン：いいえ。

### `track.region(start, end)` {#loop-region}

```kis
pub const fn region(mut self, start: Duration, end: Option<Duration>) -> Loop
```

- 戻り値：`start..end` をループするこのトラック。`None` はファイルの終わりです。
- 種別：`const fn` · ビルトイン：いいえ。

```kis
screen.music().change(looped(THEME).region(secs(16), Some(secs(80)))).apply_animation(crossfade(ms(1000)));
screen.ambience(0).change(looped(RAIN)).apply_animation(crossfade(ms(2000)));
```

## 構造体：`Sound` {#sound}

```kis
pub struct Sound {
    pub audio: AudioAsset,
    pub volume: Decimal,
    pub start: Duration,
    pub end: Option<Duration>,
    pub delay: Duration,
}
```

ワンショットサウンドです。`volume` は `0.0..=1.0`、`end` が `None` ならファイルの終わりです。

- `screen.play(s)` は `cursor + s.delay` で始まり、ファイルの `start..end` の範囲を再生します。
- ワンショットは論理画面に含まれず、ステップをまたいで再生を続けます。
- `screen.stop_sounds(d)` はカーソル位置で再生中のすべてのワンショットを `d` かけてフェードアウトします。
- ステップのエントリーでは、まだ始まっていないワンショットは破棄されます。プレイヤーがクリックしたときも、まだ始まっていないものは破棄されます。

### `sound(audio)` {#sound-fn}

```kis
pub const fn sound(audio: AudioAsset) -> Sound
```

- 戻り値：全体を最大音量で、遅延なしに再生する `audio`。
- 種別：`const fn` · ビルトイン：いいえ。

### `sound.volume(volume)`、`sound.range(start, end)`、`sound.delay(delay)` {#sound-volume}

```kis
pub const fn volume(mut self, volume: Decimal) -> Sound
pub const fn range(mut self, start: Duration, end: Option<Duration>) -> Sound
pub const fn delay(mut self, delay: Duration) -> Sound
```

| メソッド | 戻り値 |
| ------ | ------- |
| `volume(v)` | 音量 `v`（`0.0..=1.0`）のこのサウンド |
| `range(start, end)` | ファイルの `start..end` だけを再生するこのサウンド |
| `delay(d)` | カーソルから `d` 後に始まるこのサウンド |

種別：`const fn` · ビルトイン：いいえ。

複数のワンショットに遅延を付けると連続音になります。

```kis
const fn knocking(mut screen: Screen) -> Screen {
    screen.play(sound(KNOCK));
    screen.play(sound(KNOCK).delay(ms(250)));
    screen.play(sound(KNOCK).volume(0.6).delay(ms(500)));
    screen
}
```

## 構造体：`AudioFade` {#audiofade}

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

チャンネルが古いトラックから新しいトラックへどう移るかです。`overlap` は 2 つのフェードを同時に行うか順番に行うかを表します。コンストラクターはいずれも `const fn` で、ビルトインではありません。

| コンストラクター | 効果 |
| ----------- | ------ |
| `crossfade(d)` | 古いトラックのアウトと新しいトラックのインを `d` かけて同時に行う |
| `fade_out_in(o, i)` | 古いトラックを `o` かけてアウトし、その後新しいトラックを `i` かけてインする |
| `switch()` | 古いトラックを止め、新しいトラックを即座に始める |

## 構造体：`Channel` {#channel}

```kis
#[builtin] pub struct Channel;
impl Channel {
    #[builtin] pub const fn get(&self) -> Option<Loop>;
    #[builtin] pub const fn change(&mut self, track: Loop) -> &mut ChannelChange;
    #[builtin] pub const fn stop(&mut self) -> &mut ChannelChange;
    #[builtin] pub const fn volume(&mut self) -> &mut crate::screen::Prop<Decimal>;
}
```

ループするトラックを 1 つ再生するチャンネルです。種別：`const fn` · ビルトイン：はい。

- `get()` は論理画面上のトラックで、チャンネルが無音なら `None` です。
- `change(track)` はトラックを変更します。[同一トラック規則](#the-same-track-rule)に従います。
- `stop()` はトラックを停止します。
- `volume()` はチャンネル音量で、`0.0..=1.0` の [`Prop<Decimal>`](./screen.md#prop) をトゥイーンで変更します。

```kis
screen.music().volume().change(0.5).apply_animation(tween(ms(1000)));
screen.ambience(0).stop().apply_animation(fade_out_in(ms(2000), ms(0)));
```

## 構造体：`ChannelChange` {#channelchange}

```kis
#[must_use] #[builtin] pub struct ChannelChange;
impl ChannelChange {
    #[builtin] pub const fn apply_animation(&mut self, fade: AudioFade);
}
```

保留中のチャンネル変更です。`apply_animation` を通してのみ起こり、古いトラックから新しいトラックへフェードします。`ChannelChange` は `#[must_use]` です。種別：`const fn` · ビルトイン：はい。

## 音量 {#volume}

最終音量 = マスター × バス × チャンネル音量 × サウンド音量。

| 音源 | バス |
| ------ | --- |
| 音楽チャンネル | music |
| 環境音チャンネル、ワンショットサウンド、UI サウンド | sound |
| ボイス | voice |

- マスターとバスの音量は `Settings` のフィールド `master`、`music`、`sound`、`voice` で、それぞれ `level` が `Int<0, 100>` の `Setting<Volume>` です。ミュートされたバスは 0 として扱われます。
- チャンネル音量とサウンド音量は割合 `0.0..=1.0` です。

## ボイス {#voice}

行のボイスは `Say::voice` で、`s!(voice = AZUSA_0072, …)` と書きます。

- ボイスはテキストと同時に [`begin_text`](./screen-anim.md#the-timeline) で始まります。レンダーが動かさない限り、これはステップのエントリーです。
- 次のステップのエントリーで止まります。ただし `voice_sustain` 設定がオンなら止まりません。
- `wait_for_voice` 設定がオンなら、オートモードはボイスを待ちます。
- バックログからは `Narrative::replay_voice` で再生し直せます。
