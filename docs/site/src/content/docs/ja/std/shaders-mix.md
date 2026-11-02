---
title: "std::shaders::mix"
---

> **安定性:** 暫定 — 合成のシェーダー集です。W3C のブレンドモード、加算合成、すりガラスがあります。

[`Entity::mix`](./screen.md#entity-mix) で使う合成シェーダーです。どの関数も [`Mix`](./screen-shader.md#mix) を返します。`Mix` は、エンティティをその下に描かれたものへどう重ねるかを表す値です。`None` を指定すると、通常のソースオーバー合成になります。

ブレンドモードは W3C Compositing and Blending Level 1（CSS の `mix-blend-mode` の値）に準拠しています。エンティティと下の絵の両方がある場所では、エンティティの色と下の色をモードに従って混ぜ、その結果をソースオーバーで合成します。計算はリニア色空間で、レンダラーが合成に使う色に対して行います。

```kis
use std::shaders::mix::{frosted_glass, multiply};
use std::screen::rgba;

screen.entity(SHADOW).mix().change(Some(multiply())).apply_animation(instant());
screen.entity(PANEL).mix().change(Some(frosted_glass(8.0, rgba(255, 255, 255, 40)))).apply_animation(instant());
```

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`MULTIPLY` … `ADD`](#blend-shaders) | 定数 | 16 種類のブレンドモードのシェーダー |
| [`multiply()` … `add()`](#blend-modes) | 関数 | ブレンドモードを `Mix` 値として返す |
| [`FrostedGlass`](#frostedglass)、[`FROSTED_GLASS`](#frosted_glass)、[`frosted_glass`](#frosted_glass-fn) | 構造体、定数、関数 | すりガラス |

## 定数：ブレンドモードのシェーダー {#blend-shaders}

```kis
pub const MULTIPLY: MixShader<()> = asset("shaders/mix/multiply.wgsl");
pub const SCREEN: MixShader<()> = asset("shaders/mix/screen.wgsl");
pub const OVERLAY: MixShader<()> = asset("shaders/mix/overlay.wgsl");
pub const DARKEN: MixShader<()> = asset("shaders/mix/darken.wgsl");
pub const LIGHTEN: MixShader<()> = asset("shaders/mix/lighten.wgsl");
pub const COLOR_DODGE: MixShader<()> = asset("shaders/mix/color_dodge.wgsl");
pub const COLOR_BURN: MixShader<()> = asset("shaders/mix/color_burn.wgsl");
pub const HARD_LIGHT: MixShader<()> = asset("shaders/mix/hard_light.wgsl");
pub const SOFT_LIGHT: MixShader<()> = asset("shaders/mix/soft_light.wgsl");
pub const DIFFERENCE: MixShader<()> = asset("shaders/mix/difference.wgsl");
pub const EXCLUSION: MixShader<()> = asset("shaders/mix/exclusion.wgsl");
pub const HUE: MixShader<()> = asset("shaders/mix/hue.wgsl");
pub const SATURATION: MixShader<()> = asset("shaders/mix/saturation.wgsl");
pub const COLOR: MixShader<()> = asset("shaders/mix/color.wgsl");
pub const LUMINOSITY: MixShader<()> = asset("shaders/mix/luminosity.wgsl");
pub const ADD: MixShader<()> = asset("shaders/mix/add.wgsl");
```

W3C のブレンドモード（`multiply` から `luminosity` まで）と加算合成のシェーダーです。引数はありません。

## 関数：ブレンドモード {#blend-modes}

```kis
pub const fn multiply() -> Mix
pub const fn screen() -> Mix
pub const fn overlay() -> Mix
pub const fn darken() -> Mix
pub const fn lighten() -> Mix
pub const fn color_dodge() -> Mix
pub const fn color_burn() -> Mix
pub const fn hard_light() -> Mix
pub const fn soft_light() -> Mix
pub const fn difference() -> Mix
pub const fn exclusion() -> Mix
pub const fn hue() -> Mix
pub const fn saturation() -> Mix
pub const fn color() -> Mix
pub const fn luminosity() -> Mix
pub const fn add() -> Mix
```

| 関数 | W3C のモード | 効果 |
| -------- | -------- | ------ |
| `multiply()` | `multiply` | 乗算。色を掛け合わせるので、必ず暗くなる |
| `screen()` | `screen` | スクリーン。反転した色を掛け合わせるので、必ず明るくなる |
| `overlay()` | `overlay` | 下の色に応じて乗算かスクリーンを使い分ける |
| `darken()` | `darken` | チャンネルごとに、2 色のうち暗いほう |
| `lighten()` | `lighten` | チャンネルごとに、2 色のうち明るいほう |
| `color_dodge()` | `color-dodge` | 覆い焼き。エンティティの色に合わせて下の色を明るくする |
| `color_burn()` | `color-burn` | 焼き込み。エンティティの色に合わせて下の色を暗くする |
| `hard_light()` | `hard-light` | エンティティの色に応じて乗算かスクリーンを使い分ける |
| `soft_light()` | `soft-light` | `hard_light` を柔らかくしたもの |
| `difference()` | `difference` | 色の差の絶対値 |
| `exclusion()` | `exclusion` | `difference` に近いが、コントラストが低い |
| `hue()` | `hue` | エンティティの色相に、下の彩度と輝度を組み合わせる |
| `saturation()` | `saturation` | エンティティの彩度に、下の色相と輝度を組み合わせる |
| `color()` | `color` | エンティティの色相と彩度に、下の輝度を組み合わせる |
| `luminosity()` | `luminosity` | エンティティの輝度に、下の色相と彩度を組み合わせる |
| `add()` | — | エンティティの光を下の色に足す。アルファはソースオーバーで合成する |

種別：`const fn` · 組み込み：いいえ。

## 構造体：`FrostedGlass` {#frostedglass}

```kis
#[shader_arg]
pub struct FrostedGlass {
    pub radius: Decimal,
    pub tint: Color,
}
```

`FROSTED_GLASS` の引数です。`radius` はぼかしの標準偏差（ピクセル単位）、`tint` はぼかした背景の上に重ねるガラスの色です。

## 定数：`FROSTED_GLASS` {#frosted_glass}

```kis
pub const FROSTED_GLASS: MixShader<FrostedGlass> = asset("shaders/mix/frosted_glass.wgsl");
```

すりガラスのシェーダーです（TyranoScript の `[position_filter]` に相当）。

## 関数：`frosted_glass(radius, tint)` {#frosted_glass-fn}

```kis
pub const fn frosted_glass(radius: Decimal, tint: Color) -> Mix
```

- 戻り値：すりガラスの合成。エンティティに中身がある場所では、下の絵を `radius` ピクセルぼかして `tint` を重ね、その上にエンティティを描きます。エンティティが透明な場所では、背景がそのまま見えます。
- 種別：`const fn` · 組み込み：いいえ。

半透明の塗りのテキストパネルに `frosted_glass` を指定すると、パネルの背後にある場面がぼけて見えます。
