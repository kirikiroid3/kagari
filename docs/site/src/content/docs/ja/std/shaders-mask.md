---
title: "std::shaders::mask"
---

> **安定性:** 暫定 — 手続き的なマスクのシェーダー集です。グラデーションや図形をピクセルごとに計算するため、マスクにもトランジションにもマスク画像が要りません。

[`Entity::mask`](./screen.md#entity-mask)、[`Paint::masked`](./screen-shader.md#paint-masked)、[`anim::masked`](./screen-anim.md#transition-constructors) で使うマスクシェーダーです。どの関数も [`Mask`](./screen-shader.md#mask) を返します。マスクの値は、マスクをかける矩形（エンティティの矩形、塗り、トランジションのレイヤーのいずれか）全体で 0.0..=1.0 の範囲に収まります。

- アルファマスクとして使う場合、1.0 のピクセルは残り、0.0 のピクセルは隠れます。
- トランジションでは、しきい値が徐々に上がっていき、値 × 255 がしきい値以下になったピクセルから切り替わります。そのため、値の小さいピクセルほど先に切り替わります。`masked_with(…, reverse = true, …)` を使うと逆向きに進みます。

位置はマスクをかける矩形のピクセル単位で測るので、横長の矩形でも放射状のマスクは真円のままです。

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
| [`linear(angle)`](#linear) | [`LinearMask`](#linear) | 矩形を横切る線形のグラデーション |
| [`radial(center)`](#radial) | [`RadialMask`](#radial) | ある点からの距離 |
| [`diamond(center)`](#diamond) | [`DiamondMask`](#diamond) | ある点からの L1 距離 |
| [`clock(center, start)`](#clock) | [`ClockMask`](#clock) | ある点を中心とした角度 |
| [`stripes(angle, count)`](#stripes) | [`StripesMask`](#stripes) | 線形のグラデーションの繰り返し |
| [`checker(columns, rows)`](#checker) | [`CheckerMask`](#checker) | グラデーションを市松模様に並べたもの |
| [`noise(scale, seed)`](#noise) | [`NoiseMask`](#noise) | 滑らかな乱数 |
| [`gaussian(center, sigma)`](#gaussian) | [`GaussianMask`](#gaussian) | ガウス状の光の玉 |
| [`rounded_rect(radius, feather)`](#rounded_rect) | [`RoundedRectMask`](#rounded_rect) | 角丸矩形 |

以下の各節では、引数の構造体、シェーダーの定数、関数をまとめて示します。関数はどれも `const fn` で、組み込みではありません。式の中の `p` は矩形内でのピクセルの位置、`size` は矩形の大きさで、`c = center × size` とします。

## 線形 {#linear}

```kis
#[shader_arg]
pub struct LinearMask {
    pub angle: Angle,
}
pub const LINEAR: MaskShader<LinearMask> = asset("shaders/mask/linear.wgsl");
pub const fn linear(angle: Angle) -> Mask
```

線形のグラデーションです。`angle` の方向の始点側の辺で 0.0、反対側の辺で 1.0 になります。0° で左から右、90° で上から下に向かいます。`d = (cos angle, sin angle)`、`e = (|d.x|·w + |d.y|·h) / 2` とすると、値は `dot(p − size/2, d) / (2e) + 0.5` です。

## 放射状 {#radial}

```kis
#[shader_arg]
pub struct RadialMask {
    pub center: Anchor,
}
pub const RADIAL: MaskShader<RadialMask> = asset("shaders/mask/radial.wgsl");
pub const fn radial(center: Anchor) -> Mask
```

円形のグラデーションです。`center`（矩形の大きさに対する割合）で 0.0、そこから最も遠い角で 1.0 になります。トランジションに使うと、`center` から円が広がるように切り替わります。

## ひし形 {#diamond}

```kis
#[shader_arg]
pub struct DiamondMask {
    pub center: Anchor,
}
pub const DIAMOND: MaskShader<DiamondMask> = asset("shaders/mask/diamond.wgsl");
pub const fn diamond(center: Anchor) -> Mask
```

`radial` と同じですが、距離を `|dx| + |dy|` で測ります。そのため、ひし形に広がるように現れます。

## 時計回り {#clock}

```kis
#[shader_arg]
pub struct ClockMask {
    pub center: Anchor,
    pub start: Angle,
}
pub const CLOCK: MaskShader<ClockMask> = asset("shaders/mask/clock.wgsl");
pub const fn clock(center: Anchor, start: Angle) -> Mask
```

時計の針のように回るワイプです。`center` を中心に、`start` の角度から時計回りに進むにつれて 0.0 から 1.0 へ増えます。0° が右、90° が下を指すので、−90° を指定すると 12 時の位置から始まります。

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

`count` 本の縞です。各縞は `angle` 方向の `linear` のグラデーションになっているので、好きな向きのブラインドを作れます。`count` に 0 を指定すると 1 として扱います。

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

`columns` × `rows` マスの市松模様です。各マスは左から右へのグラデーションです。片方の色のマスが 0.0..0.5、もう片方の色のマスが 0.5..1.0 の範囲を受け持つため、トランジションでは片方の色のマスが先に切り替わり、続いてもう片方が切り替わります。列数や行数に 0 を指定すると 1 として扱います。

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

一辺 `scale` ピクセル（最小 1 ピクセル）の格子の上で滑らかに変化する乱数です。`seed` を変えると模様が変わります。トランジションに使うとディゾルブになります。`scale` が小さいと細かい粒状に、大きいと柔らかいまだら状になります。

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

`center` で 1.0 になり、そこから `exp(−|p − c|² / (2·sigma²))` に従って減っていきます（`sigma` はピクセル単位）。周辺減光（ビネット）やスポットライトに使えます。`sigma` が 0.0 以下のときは、どこでも 0.0 になります。

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

角を半径 `radius` ピクセルで丸めた矩形の内側で 1.0、外側で 0.0 になります。境界は `feather` ピクセルの幅でなめらかにぼかします。立ち絵やパネルの角を丸めたいときに使います。
