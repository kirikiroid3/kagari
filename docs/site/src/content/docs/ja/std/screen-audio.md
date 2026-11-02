---
title: "std::screen::audio"
---

> **安定性:** 安定 — 画面の音声。ループ再生するチャンネルと単発のサウンドを扱います。

音声も画面の一部です。レンダーはほかのコンポーネントと同じ方法で音声を変更し、チャンネルの状態は[論理画面](./screen.md#the-logical-screen)に含まれます。このモジュールは、ループ再生するトラック（[`Loop`](#loop)）、単発のサウンド（[`Sound`](#sound)）、チャンネルのフェード（[`AudioFade`](#audiofade)）、状態を表す型 [`Channel`](#channel) を宣言します。アイテムはどれも `const fn` か単純なデータです。

プレリュードには `looped`、`sound`、`crossfade` が含まれます。それ以外は `use std::screen::audio::…` で取り込みます。

ゲームに同梱できる音声形式は、ランタイムプロファイルによって決まります。`native_v0` で使えるのは Ogg Vorbis、Ogg FLAC、FLAC、WAV、MP3、AAC-LC（ADTS または MP4/M4A）、ALAC（MP4/M4A）で、Ogg Opus には対応していません。詳しくは[ランタイムプロファイル](./appendix/profiles.md)を参照してください。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Loop`](#loop)、`looped` | 構造体とコンストラクター | ループ再生するトラック |
| [`Sound`](#sound)、`sound` | 構造体とコンストラクター | 単発のサウンド |
| [`AudioFade`](#audiofade)、`crossfade`、`fade_out_in`、`switch` | 構造体とコンストラクター | チャンネルのトラックの切り替え方 |
| [`Channel`](#channel) | 組み込みの構造体 | 画面のチャンネル |
| [`ChannelChange`](#channelchange) | 組み込みの構造体 | 適用待ちのチャンネル変更 |

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

- 画面には音楽チャンネルが 1 つ、環境音チャンネルが 4 つ（`ambience(0)`〜`ambience(3)`）あります。各チャンネルで同時に再生できるループトラックは 1 つまでです。
- 単発のサウンドは `play` で鳴らします。チャンネルは使いません。
- `Screen::empty()` では、どのチャンネルも無音で、音量は 1.0 です。

### 同一トラックの規則 {#the-same-track-rule}

- 現在のトラックと構造的に等しい `l` を `change(l)` に渡しても、何も起きません。トラックは最初から再生し直されず、そのまま流れ続けます。そのため、シーンのステップごとに音楽を指定しても、実際に曲が変わったときにしか聞こえ方は変わりません。
- 異なるトラックを渡すと、古いトラックが `fade_out` をかけて消え、新しいトラックが `fade_in` をかけて現れます。`overlap` が true なら 2 つのフェードを同時に、false なら順に行います。
- `stop()` は現在のトラックを `fade_out` をかけて消します。

音楽はステップを越えて引き継がれます。再生中のトラックは次のステップでもそのまま流れ、勝手に最初から再生し直されることはありません。曲を頭から再生し直したいときは、レンダーで `screen.replay_bgm()` を呼びます。タイムラインのカーソルの位置から再生し直します。これはタイムラインに予約されるだけで論理画面は変わらないため、画面の等価性には影響しません。トラックがなければ `replay_bgm()` は何もしません。

### システムの音楽 {#system-music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

`std::system::set_music` は、システム用の音楽チャンネルを設定します。このチャンネルが聞こえるのは、モードスタック上でゲームが動いていない間です。ゲームが動いている間は、代わりに物語の画面の音楽チャンネルが鳴ります。`None` を渡すと停止します。`fn` なので、タイトル画面などシステム側のコードから呼びます。[`std::system`](./system.md) を参照してください。

## 構造体：`Loop` {#loop}

```kis
pub struct Loop {
    pub audio: AudioAsset,
    pub loop_start: Duration,
    pub loop_end: Option<Duration>,
}
```

ループ再生するトラックです。0 から再生を始め、`loop_end`（`None` ならファイルの終わり）に達すると `loop_start` に戻ります。`loop_start` より前はイントロとして最初の 1 回だけ再生します。

### `looped(audio)` {#looped}

```kis
pub const fn looped(audio: AudioAsset) -> Loop
```

- 戻り値：`audio` 全体をループするトラック（`loop_start` が 0、`loop_end` が `None`）。
- 種別：`const fn` · 組み込み：いいえ。

### `track.region(start, end)` {#loop-region}

```kis
pub const fn region(mut self, start: Duration, end: Option<Duration>) -> Loop
```

- 戻り値：ループ範囲を `start..end` にしたトラック。`None` はファイルの終わりを表します。
- 種別：`const fn` · 組み込み：いいえ。

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

単発のサウンドです。`volume` の範囲は `0.0..=1.0` で、`end` が `None` ならファイルの終わりまで再生します。

- `screen.play(s)` は `cursor + s.delay` の時点で、ファイルの `start..end` の範囲を再生します。
- 単発のサウンドは論理画面に含まれず、ステップが変わっても鳴り続けます。
- `screen.stop_sounds(d)` は、カーソルの時点で鳴っている単発のサウンドをすべて、`d` をかけてフェードアウトします。
- ステップの開始時点で、まだ始まっていない単発のサウンドは破棄します。プレイヤーがクリックしたときも同様です。

### `sound(audio)` {#sound-fn}

```kis
pub const fn sound(audio: AudioAsset) -> Sound
```

- 戻り値：`audio` 全体を最大音量で、遅延なしに再生するサウンド。
- 種別：`const fn` · 組み込み：いいえ。

### `sound.volume(volume)`、`sound.range(start, end)`、`sound.delay(delay)` {#sound-volume}

```kis
pub const fn volume(mut self, volume: Decimal) -> Sound
pub const fn range(mut self, start: Duration, end: Option<Duration>) -> Sound
pub const fn delay(mut self, delay: Duration) -> Sound
```

| メソッド | 戻り値 |
| ------ | ------- |
| `volume(v)` | 音量を `v`（`0.0..=1.0`）にしたサウンド |
| `range(start, end)` | ファイルの `start..end` だけを再生するサウンド |
| `delay(d)` | カーソルから `d` 遅れて鳴るサウンド |

種別：`const fn` · 組み込み：いいえ。

単発のサウンドを遅延をずらして並べると、連続した効果音になります。

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

チャンネルで古いトラックから新しいトラックへ切り替える方法です。`overlap` は、2 つのフェードを同時に行うか順に行うかを表します。コンストラクターはどれも `const fn` で、組み込みではありません。

| コンストラクター | 効果 |
| ----------- | ------ |
| `crossfade(d)` | 古いトラックのフェードアウトと新しいトラックのフェードインを、`d` をかけて同時に行う |
| `fade_out_in(o, i)` | 古いトラックを `o` かけてフェードアウトし、続けて新しいトラックを `i` かけてフェードインする |
| `switch()` | 古いトラックを止め、すぐに新しいトラックを始める |

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

ループトラックを 1 つ再生するチャンネルです。種別：`const fn` · 組み込み：はい。

- `get()` は論理画面上のトラックを返します。無音なら `None` です。
- `change(track)` はトラックを変更します。[同一トラックの規則](#the-same-track-rule)が適用されます。
- `stop()` はトラックを止めます。
- `volume()` はチャンネルの音量です。範囲 `0.0..=1.0` の [`Prop<Decimal>`](./screen.md#prop) で、補間を指定して変更します。

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

適用待ちのチャンネル変更です。変更は `apply_animation` を呼んだときにだけ反映され、古いトラックから新しいトラックへフェードします。`ChannelChange` には `#[must_use]` が付いています。種別：`const fn` · 組み込み：はい。

## 音量 {#volume}

最終的な音量 = マスター × バス × チャンネル音量 × サウンド音量。

| 音源 | バス |
| ------ | --- |
| 音楽チャンネル | music |
| 環境音チャンネル、単発のサウンド、UI の効果音 | sound |
| ボイス | voice |

- マスターとバスの音量は、`Settings` のフィールド `master`、`music`、`sound`、`voice` で決まります。いずれも `Setting<Volume>` で、`level` は `Int<0, 100>` です。ミュート中のバスは 0 として扱います。
- チャンネル音量とサウンド音量は `0.0..=1.0` の割合です。

## ボイス {#voice}

台詞のボイスは `Say::voice` で指定し、`s!(voice = AZUSA_0072, …)` のように書きます。

- ボイスはテキストと同時に [`begin_text`](./screen-anim.md#the-timeline) の時点で始まります。レンダーで動かさなければ、ステップの開始時点です。
- 次のステップが始まると止まります。ただし `voice_sustain` 設定がオンのときは止まりません。
- `wait_for_voice` 設定がオンなら、オートモードはボイスが終わるまで待ちます。
- バックログからは `Narrative::replay_voice` で再生し直せます。
