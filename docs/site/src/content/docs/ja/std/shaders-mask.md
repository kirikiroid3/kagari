---
title: "std::shaders::mask"
---

> **安定性:** 暫定 — 手続き的なマスクです。ランプや形を画素ごとに計算するので、マスクやトランジションにマスク画像が要りません。

[`Entity::mask`](./screen.md#entity-mask)、[`Paint::masked`](./screen-shader.md#paint-masked)、[`anim::masked`](./screen-anim.md#transition-constructors) のためのマスクシェーダーです。どの関数も、マスクする矩形（エンティティの矩形、ペイント、トランジションのレイヤー）の上で値が 0.0..=1.0 に収まる [`Mask`](./screen-shader.md#mask) を返します。

- アルファマスクとしては、1.0 で画素を残し、0.0 で隠します。
- トランジションでは、値 × 255 が移動するしきい値以下になった画素から切り替わるので、値の小さい画素が先に切り替わります。`masked_with(…, reverse = true, …)` は逆向きに進みます。

位置はマスクする矩形のピクセル単位で測るので、横長の矩形でも放射マスクは円のままです。

```kis
use std::screen::{CENTER, degrees};
use std::screen::anim::{masked, masked_with};
use std::shaders::mask::{clock, linear, noise, rounded_rect};

screen.background().change(BG::NIGHT).apply_animation(masked_with(linear(degrees(0.0)), 32, false, ms(800)));
screen.background().change(BG::ROOF).apply_animation(masked(noise(24.0, 7), ms(1200)));
screen.entity(CLOCK_FACE).image().change(CLOCK::FACE).apply_animation(masked(clock(CENTER, degrees(-90.0)), ms(1000)));
screen.entity(PORTRAIT).mask().change(Some(rounded_rect(24.0, 2.0))).apply_animation(instant());
```

## 概要 {#overview}

| 関数 | 引数 | 値 |
| -------- | --------- | ----- |
| [`linear(angle)`](#linear) | [`LinearMask`](#linear) | 矩形を横切るランプ |
| [`radial(center)`](#radial) | [`RadialMask`](#radial) | 点からの距離 |
| [`diamond(center)`](#diamond) | [`DiamondMask`](#diamond) | 点からの L1 距離 |
| [`clock(center, start)`](#clock) | [`ClockMask`](#clock) | 点の周りの角度 |
| [`stripes(angle, count)`](#stripes) | [`StripesMask`](#stripes) | 繰り返す線形ランプ |
| [`checker(columns, rows)`](#checker) | [`CheckerMask`](#checker) | ランプの市松模様 |
| [`noise(scale, seed)`](#noise) | [`NoiseMask`](#noise) | 滑らかな乱数値 |
| [`gaussian(center, sigma)`](#gaussian) | [`GaussianMask`](#gaussian) | ガウスの光点 |
| [`rounded_rect(radius, feather)`](#rounded_rect) | [`RoundedRectMask`](#rounded_rect) | 角丸矩形 |

以下の各節に、引数の構造体、シェーダー定数、関数を載せます。関数は `const fn` で、ビルトインではありません。式の中で、`p` は矩形内の画素の位置、`size` は矩形のサイズ、`c = center × size` です。

## 線形 {#linear}

```kis
#[shader_arg]
pub struct LinearMask {
    pub angle: Angle,
}
pub const LINEAR: MaskShader<LinearMask> = asset("shaders/mask/linear.wgsl");
pub const fn linear(angle: Angle) -> Mask
```

線形のランプです。方向 `angle` の始まる辺で 0.0、反対の辺で 1.0 になります。0° は左から右、90° は上から下です。`d = (cos angle, sin angle)`、`e = (|d.x|·w + |d.y|·h) / 2` とすると、値は `dot(p − size/2, d) / (2e) + 0.5` です。

## 放射 {#radial}

```kis
#[shader_arg]
pub struct RadialMask {
    pub center: Anchor,
}
pub const RADIAL: MaskShader<RadialMask> = asset("shaders/mask/radial.wgsl");
pub const fn radial(center: Anchor) -> Mask
```

円形のランプです。`center`（矩形に対する割合）で 0.0、最も遠い角で 1.0 になります。トランジションでは `center` から円が開きます。

## ひし形 {#diamond}

```kis
#[shader_arg]
pub struct DiamondMask {
    pub center: Anchor,
}
pub const DIAMOND: MaskShader<DiamondMask> = asset("shaders/mask/diamond.wgsl");
pub const fn diamond(center: Anchor) -> Mask
```

`radial` と同様ですが、距離を `|dx| + |dy|` で測ります。表示される範囲はひし形に広がります。

## 時計 {#clock}

```kis
#[shader_arg]
pub struct ClockMask {
    pub center: Anchor,
    pub start: Angle,
}
pub const CLOCK: MaskShader<ClockMask> = asset("shaders/mask/clock.wgsl");
pub const fn clock(center: Anchor, start: Angle) -> Mask
```

時計回りのワイプです。`start` から `center` の周りを時計回りに進みながら 0.0 から 1.0 へ上がります（0° は右、90° は下を向くので、−90° は 12 時の位置から始まります）。

## 縞 {#stripes}

```kis
#[shader_arg]
pub struct StripesMask {
    pub angle: Angle,
    pub count: u32,
}
pub const STRIPES: MaskShader<StripesMask> = asset("shaders/mask/stripes.wgsl");
pub const fn stripes(angle: Angle, count: u32) -> Mask
```

`count` 本の縞で、それぞれが `angle` に沿った `linear` のランプです。任意の方向のブラインドになります。`count` が 0 なら 1 として扱います。

## 市松 {#checker}

```kis
#[shader_arg]
pub struct CheckerMask {
    pub columns: u32,
    pub rows: u32,
}
pub const CHECKER: MaskShader<CheckerMask> = asset("shaders/mask/checker.wgsl");
pub const fn checker(columns: u32, rows: u32) -> Mask
```

`columns` × `rows` のマスの市松模様で、各マスは左から右へのランプです。一方の色のマスが 0.0..0.5、もう一方が 0.5..1.0 を受け持つので、トランジションは一方の色のマスを拭き取ってから、もう一方を拭き取ります。数が 0 なら 1 として扱います。

## ノイズ {#noise}

```kis
#[shader_arg]
pub struct NoiseMask {
    pub scale: Decimal,
    pub seed: u32,
}
pub const NOISE: MaskShader<NoiseMask> = asset("shaders/mask/noise.wgsl");
pub const fn noise(scale: Decimal, seed: u32) -> Mask
```

`scale` ピクセル（最低 1 ピクセル）のマスの格子上の滑らかな乱数値で、`seed` で変わります。トランジションではディゾルブになります。`scale` が小さいと細かい粒に、大きいと柔らかいまだらになります。

## ガウス {#gaussian}

```kis
#[shader_arg]
pub struct GaussianMask {
    pub center: Anchor,
    pub sigma: Decimal,
}
pub const GAUSSIAN: MaskShader<GaussianMask> = asset("shaders/mask/gaussian.wgsl");
pub const fn gaussian(center: Anchor, sigma: Decimal) -> Mask
```

`center` で 1.0 となり、`exp(−|p − c|² / (2·sigma²))`（`sigma` はピクセル単位）で減衰します。ビネットやスポットライトになります。`sigma` が 0.0 以下なら、どこでも 0.0 です。

## 角丸矩形 {#rounded_rect}

```kis
#[shader_arg]
pub struct RoundedRectMask {
    pub radius: Decimal,
    pub feather: Decimal,
}
pub const ROUNDED_RECT: MaskShader<RoundedRectMask> = asset("shaders/mask/rounded_rect.wgsl");
pub const fn rounded_rect(radius: Decimal, feather: Decimal) -> Mask
```

角を `radius` ピクセルで丸めた矩形の内側で 1.0、外側で 0.0 となり、境界をまたぐ `feather` ピクセルでぼかされます。立ち絵やパネルの角を丸めるのに使います。
