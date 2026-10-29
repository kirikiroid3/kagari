---
title: "std::shaders::color"
---

> **安定性:** 暫定 — 色フィルターです。CSS のフィルター関数、まとめたフィルター、ガウスぼかしを含みます。

[`Screen::filter`](./screen.md#screen-filter) と [`Entity::filter`](./screen.md#entity-filter) のための色シェーダーです。どの関数も [`Filter`](./screen-shader.md#filter) を返します。CSS のフィルター関数は CSS と同じく sRGB の値で計算するので、`sepia(1.0)` は CSS の `sepia(1)` と同じ見た目になります。同じ関数の 2 つのフィルターの間をトゥイーンすると、量が動きます。

```kis
use std::shaders::color::{NEUTRAL, adjust, blur, grayscale};

/// A dark, slightly harder look.
const fn night() -> std::screen::shader::Filter {
    let mut a = NEUTRAL;
    a.brightness = 0.6;
    a.contrast = 1.2;
    adjust(a)
}

screen.filter().change(Some(grayscale(1.0))).apply_animation(tween(ms(800)));
screen.entity(BG_BLUR).filter().change(Some(blur(4.0))).apply_animation(tween(ms(500)));
screen.filter().change(Some(night())).apply_animation(tween(ms(800)));
```

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Amount`](#amount) | 構造体 | パラメーターが 1 つのフィルターの強さ |
| [`GRAYSCALE` … `CONTRAST`](#amount-shaders) | 定数 | パラメーターが 1 つの 6 つのシェーダー |
| [`grayscale`、`sepia`、`saturate`、`invert`、`brightness`、`contrast`](#amount-filters) | 関数 | CSS のフィルター関数 |
| [`HueRotate`](#huerotate)、[`HUE_ROTATE`](#hue_rotate)、[`hue_rotate`](#hue_rotate-fn) | 構造体、定数、関数 | すべての色相を回す |
| [`Adjust`](#adjust)、[`NEUTRAL`](#neutral)、[`ADJUST`](#adjust-shader)、[`adjust`](#adjust-fn) | 構造体、定数、関数 | 7 つのフィルターを 1 パスで |
| [`Blur`](#blur)、[`BLUR`](#blur-shader)、[`blur`](#blur-fn) | 構造体、定数、関数 | ガウスぼかし |

## 構造体：`Amount` {#amount}

```kis
#[shader_arg]
pub struct Amount {
    pub amount: Decimal,
}
```

パラメーターが 1 つのフィルターの強さで、CSS の量と同じです。`grayscale`、`sepia`、`invert` は 0.0 で、`saturate`、`brightness`、`contrast` は 1.0 で効果がなくなります。

## 定数：`GRAYSCALE`、`SEPIA`、`SATURATE`、`INVERT`、`BRIGHTNESS`、`CONTRAST` {#amount-shaders}

```kis
pub const GRAYSCALE: ColorShader<Amount> = asset("shaders/color/grayscale.wgsl");
pub const SEPIA: ColorShader<Amount> = asset("shaders/color/sepia.wgsl");
pub const SATURATE: ColorShader<Amount> = asset("shaders/color/saturate.wgsl");
pub const INVERT: ColorShader<Amount> = asset("shaders/color/invert.wgsl");
pub const BRIGHTNESS: ColorShader<Amount> = asset("shaders/color/brightness.wgsl");
pub const CONTRAST: ColorShader<Amount> = asset("shaders/color/contrast.wgsl");
```

CSS の `grayscale()`、`sepia()`、`saturate()`、`invert()`、`brightness()`、`contrast()` のシェーダーです。

## 関数：`grayscale`、`sepia`、`saturate`、`invert`、`brightness`、`contrast` {#amount-filters}

```kis
pub const fn grayscale(amount: Decimal) -> Filter
pub const fn sepia(amount: Decimal) -> Filter
pub const fn saturate(amount: Decimal) -> Filter
pub const fn invert(amount: Decimal) -> Filter
pub const fn brightness(amount: Decimal) -> Filter
pub const fn contrast(amount: Decimal) -> Filter
```

| 関数 | 効果 |
| -------- | ------ |
| `grayscale(a)` | 色を抜く。0.0 でそのまま、1.0 で完全な灰色。`a` は 0.0..=1.0 に収められる |
| `sepia(a)` | 茶色に色付けする。0.0 でそのまま、1.0 で完全なセピア。0.0..=1.0 に収められる |
| `saturate(a)` | 彩度を拡大縮小する。0.0 で灰色、1.0 でそのまま、1.0 を超えると過飽和 |
| `invert(a)` | 色を反転する（`a·(1 − c) + (1 − a)·c`）。0.0 でそのまま、1.0 で完全に反転。0.0..=1.0 に収められる |
| `brightness(a)` | 色に `a` を掛ける。1.0 でそのまま、0.0 で黒 |
| `contrast(a)` | 中間の灰色を中心にコントラストを拡大縮小する（`a·c + 0.5 − 0.5·a`）。1.0 でそのまま、0.0 で中間の灰色 |

`grayscale`、`sepia`、`saturate` は W3C Filter Effects 仕様の行列を使います。
種別：`const fn` · ビルトイン：いいえ。

## 構造体：`HueRotate` {#huerotate}

```kis
#[shader_arg]
pub struct HueRotate {
    pub angle: Angle,
}
```

`HUE_ROTATE` の引数で、色相を回す角度です。

## 定数：`HUE_ROTATE` {#hue_rotate}

```kis
pub const HUE_ROTATE: ColorShader<HueRotate> = asset("shaders/color/hue_rotate.wgsl");
```

CSS の `hue-rotate()` のシェーダーです。

## 関数：`hue_rotate(angle)` {#hue_rotate-fn}

```kis
pub const fn hue_rotate(angle: Angle) -> Filter
```

- 戻り値：W3C の hue-rotate 行列で、すべての色相を `angle` だけ回すフィルター。
- 種別：`const fn` · ビルトイン：いいえ。

## 構造体：`Adjust` {#adjust}

```kis
#[shader_arg]
pub struct Adjust {
    pub grayscale: Decimal,
    pub sepia: Decimal,
    pub saturate: Decimal,
    pub hue: Angle,
    pub invert: Decimal,
    pub brightness: Decimal,
    pub contrast: Decimal,
}
```

[`adjust`](#adjust-fn) が使う 7 つの CSS フィルターの量です。各フィールドの意味は同名の関数が受け取る値と同じで、`hue` は `hue_rotate` の角度です。

## 定数：`NEUTRAL` {#neutral}

```kis
pub const NEUTRAL: Adjust = Adjust {
    grayscale: 0.0,
    sepia: 0.0,
    saturate: 1.0,
    hue: Angle { degrees: 0.0 },
    invert: 0.0,
    brightness: 1.0,
    contrast: 1.0,
};
```

何も変えない量です。これを出発点にして、必要なものだけを設定してください。

## 定数：`ADJUST` {#adjust-shader}

```kis
pub const ADJUST: ColorShader<Adjust> = asset("shaders/color/adjust.wgsl");
```

7 つのフィルターを 1 パスで適用します（TyranoScript の `[filter]`）。

## 関数：`adjust(a)` {#adjust-fn}

```kis
pub const fn adjust(a: Adjust) -> Filter
```

- 戻り値：`a` の量を TyranoScript の `[filter]` の順序（grayscale、sepia、saturate、hue、invert、brightness、contrast）で適用するフィルター。
- 種別：`const fn` · ビルトイン：いいえ。

2 つの `adjust` フィルターの間をトゥイーンするとすべての量が同時に動くので、1 つのフィルターでセピアの回想から暗い夜の場面へフェードできます。

## 構造体：`Blur` {#blur}

```kis
#[shader_arg]
pub struct Blur {
    pub radius: Decimal,
}
```

`BLUR` の引数で、ピクセル単位の標準偏差です。0.0 以下ならぼかしません。

## 定数：`BLUR` {#blur-shader}

```kis
pub const BLUR: ColorShader<Blur> = asset("shaders/color/blur.wgsl");
```

ガウスぼかしです。

## 関数：`blur(radius)` {#blur-fn}

```kis
pub const fn blur(radius: Decimal) -> Filter
```

- 戻り値：CSS の `blur()` と同じく、標準偏差 `radius` ピクセルでぼかすフィルター。
- 種別：`const fn` · ビルトイン：いいえ。

ぼかしは、`radius / 2` ピクセル間隔で並ぶ 13 × 13 の格子点をサンプリングし、各方向に標準偏差の 3 倍まで届きます。
