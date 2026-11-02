---
title: "std::shaders::color"
---

> **安定性:** 暫定 — 色フィルターのシェーダー集です。CSS のフィルター関数、それらをまとめたフィルター、ガウスぼかしがあります。

[`Screen::filter`](./screen.md#screen-filter) と [`Entity::filter`](./screen.md#entity-filter) で使う色シェーダーです。どの関数も [`Filter`](./screen-shader.md#filter) を返します。CSS のフィルター関数は CSS と同じく sRGB の値で計算するため、`sepia(1.0)` は CSS の `sepia(1)` と同じ見た目になります。同じ関数で作った 2 つのフィルターの間を補間すると、効果の強さが連続的に変わります。

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
| [`Amount`](#amount) | 構造体 | 引数が 1 つのフィルターの強さ |
| [`GRAYSCALE` … `CONTRAST`](#amount-shaders) | 定数 | 引数が 1 つの 6 種類のシェーダー |
| [`grayscale`、`sepia`、`saturate`、`invert`、`brightness`、`contrast`](#amount-filters) | 関数 | CSS のフィルター関数 |
| [`HueRotate`](#huerotate)、[`HUE_ROTATE`](#hue_rotate)、[`hue_rotate`](#hue_rotate-fn) | 構造体、定数、関数 | すべての色相を回転させる |
| [`Adjust`](#adjust)、[`NEUTRAL`](#neutral)、[`ADJUST`](#adjust-shader)、[`adjust`](#adjust-fn) | 構造体、定数、関数 | 7 種類のフィルターを 1 パスでまとめてかける |
| [`Blur`](#blur)、[`BLUR`](#blur-shader)、[`blur`](#blur-fn) | 構造体、定数、関数 | ガウスぼかし |

## 構造体：`Amount` {#amount}

```kis
#[shader_arg]
pub struct Amount {
    pub amount: Decimal,
}
```

引数が 1 つのフィルターの強さで、CSS のフィルター関数に渡す値と同じ意味です。`grayscale`、`sepia`、`invert` は 0.0 のとき、`saturate`、`brightness`、`contrast` は 1.0 のときに効果がなくなります。

## 定数：`GRAYSCALE`、`SEPIA`、`SATURATE`、`INVERT`、`BRIGHTNESS`、`CONTRAST` {#amount-shaders}

```kis
pub const GRAYSCALE: ColorShader<Amount> = asset("shaders/color/grayscale.wgsl");
pub const SEPIA: ColorShader<Amount> = asset("shaders/color/sepia.wgsl");
pub const SATURATE: ColorShader<Amount> = asset("shaders/color/saturate.wgsl");
pub const INVERT: ColorShader<Amount> = asset("shaders/color/invert.wgsl");
pub const BRIGHTNESS: ColorShader<Amount> = asset("shaders/color/brightness.wgsl");
pub const CONTRAST: ColorShader<Amount> = asset("shaders/color/contrast.wgsl");
```

CSS の `grayscale()`、`sepia()`、`saturate()`、`invert()`、`brightness()`、`contrast()` に当たるシェーダーです。

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
| `grayscale(a)` | 色を抜く。0.0 で元のまま、1.0 で完全なグレー。`a` は 0.0..=1.0 に丸める |
| `sepia(a)` | 茶色がかった色にする。0.0 で元のまま、1.0 で完全なセピア。0.0..=1.0 に丸める |
| `saturate(a)` | 彩度を `a` 倍にする。0.0 でグレー、1.0 で元のまま、1.0 を超えると元より鮮やかになる |
| `invert(a)` | 色を反転する（`a·(1 − c) + (1 − a)·c`）。0.0 で元のまま、1.0 で完全に反転。0.0..=1.0 に丸める |
| `brightness(a)` | 色に `a` を掛ける。1.0 で元のまま、0.0 で黒 |
| `contrast(a)` | 中間のグレーを基準にコントラストを `a` 倍にする（`a·c + 0.5 − 0.5·a`）。1.0 で元のまま、0.0 で一面の中間のグレー |

`grayscale`、`sepia`、`saturate` は、W3C の Filter Effects 仕様にある行列で計算します。
種別：`const fn` · 組み込み：いいえ。

## 構造体：`HueRotate` {#huerotate}

```kis
#[shader_arg]
pub struct HueRotate {
    pub angle: Angle,
}
```

`HUE_ROTATE` の引数で、色相を回転させる角度です。

## 定数：`HUE_ROTATE` {#hue_rotate}

```kis
pub const HUE_ROTATE: ColorShader<HueRotate> = asset("shaders/color/hue_rotate.wgsl");
```

CSS の `hue-rotate()` に当たるシェーダーです。

## 関数：`hue_rotate(angle)` {#hue_rotate-fn}

```kis
pub const fn hue_rotate(angle: Angle) -> Filter
```

- 戻り値：すべての色相を `angle` だけ回転させるフィルター。計算には W3C の hue-rotate 行列を使います。
- 種別：`const fn` · 組み込み：いいえ。

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

[`adjust`](#adjust-fn) に渡す、7 種類の CSS フィルターの強さです。各フィールドの意味は同じ名前の関数の引数と同じで、`hue` だけは `hue_rotate` の角度に当たります。

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

何も変えない設定値です。これを元に、必要なフィールドだけを書き換えて使ってください。

## 定数：`ADJUST` {#adjust-shader}

```kis
pub const ADJUST: ColorShader<Adjust> = asset("shaders/color/adjust.wgsl");
```

7 種類のフィルターを 1 パスでまとめてかけるシェーダーです（TyranoScript の `[filter]` に相当）。

## 関数：`adjust(a)` {#adjust-fn}

```kis
pub const fn adjust(a: Adjust) -> Filter
```

- 戻り値：`a` の各フィルターを、TyranoScript の `[filter]` と同じ順序（grayscale、sepia、saturate、hue、invert、brightness、contrast）でかけるフィルター。
- 種別：`const fn` · 組み込み：いいえ。

2 つの `adjust` フィルターの間を補間すると、すべての強さが同時に変わります。そのため、フィルター 1 つで、セピア調の回想から暗い夜の場面へなめらかに移り変わらせることができます。

## 構造体：`Blur` {#blur}

```kis
#[shader_arg]
pub struct Blur {
    pub radius: Decimal,
}
```

`BLUR` の引数で、ぼかしの標準偏差（ピクセル単位）です。0.0 以下ではぼかしません。

## 定数：`BLUR` {#blur-shader}

```kis
pub const BLUR: ColorShader<Blur> = asset("shaders/color/blur.wgsl");
```

ガウスぼかしのシェーダーです。

## 関数：`blur(radius)` {#blur-fn}

```kis
pub const fn blur(radius: Decimal) -> Filter
```

- 戻り値：CSS の `blur()` と同じように、標準偏差 `radius` ピクセルでぼかすフィルター。
- 種別：`const fn` · 組み込み：いいえ。

ぼかしでは、`radius / 2` ピクセル間隔に並んだ 13 × 13 の格子点から色を取ります。範囲は上下左右それぞれ標準偏差の 3 倍までです。
