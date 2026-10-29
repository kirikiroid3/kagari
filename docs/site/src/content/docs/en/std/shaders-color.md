---
title: "std::shaders::color"
---

> **Stability:** Tentative — colour filters: the CSS filter functions, a combined filter and a
> Gaussian blur.

Colour shaders for [`Screen::filter`](./screen.md#screen-filter) and
[`Entity::filter`](./screen.md#entity-filter). Every function returns a
[`Filter`](./screen-shader.md#filter). The CSS filter functions are computed on sRGB values, as
CSS computes them, so `sepia(1.0)` looks the same as CSS `sepia(1)`. Tweening between two filters
of the same function moves the amount.

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

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Amount`](#amount) | struct | the strength of a one-parameter filter |
| [`GRAYSCALE` … `CONTRAST`](#amount-shaders) | constants | the six one-parameter shaders |
| [`grayscale`, `sepia`, `saturate`, `invert`, `brightness`, `contrast`](#amount-filters) | functions | the CSS filter functions |
| [`HueRotate`](#huerotate), [`HUE_ROTATE`](#hue_rotate), [`hue_rotate`](#hue_rotate-fn) | struct, constant, function | turns every hue |
| [`Adjust`](#adjust), [`NEUTRAL`](#neutral), [`ADJUST`](#adjust-shader), [`adjust`](#adjust-fn) | struct, constants, function | all seven filters in one pass |
| [`Blur`](#blur), [`BLUR`](#blur-shader), [`blur`](#blur-fn) | struct, constant, function | a Gaussian blur |

## Struct: `Amount` {#amount}

```kis
#[shader_arg]
pub struct Amount {
    pub amount: Decimal,
}
```

The strength of a one-parameter filter, the CSS amount: 0.0 leaves `grayscale`, `sepia` and
`invert` without effect, and 1.0 leaves `saturate`, `brightness` and `contrast` without effect.

## Constants: `GRAYSCALE`, `SEPIA`, `SATURATE`, `INVERT`, `BRIGHTNESS`, `CONTRAST` {#amount-shaders}

```kis
pub const GRAYSCALE: ColorShader<Amount> = asset("shaders/color/grayscale.wgsl");
pub const SEPIA: ColorShader<Amount> = asset("shaders/color/sepia.wgsl");
pub const SATURATE: ColorShader<Amount> = asset("shaders/color/saturate.wgsl");
pub const INVERT: ColorShader<Amount> = asset("shaders/color/invert.wgsl");
pub const BRIGHTNESS: ColorShader<Amount> = asset("shaders/color/brightness.wgsl");
pub const CONTRAST: ColorShader<Amount> = asset("shaders/color/contrast.wgsl");
```

The shaders of CSS `grayscale()`, `sepia()`, `saturate()`, `invert()`, `brightness()` and
`contrast()`.

## Functions: `grayscale`, `sepia`, `saturate`, `invert`, `brightness`, `contrast` {#amount-filters}

```kis
pub const fn grayscale(amount: Decimal) -> Filter
pub const fn sepia(amount: Decimal) -> Filter
pub const fn saturate(amount: Decimal) -> Filter
pub const fn invert(amount: Decimal) -> Filter
pub const fn brightness(amount: Decimal) -> Filter
pub const fn contrast(amount: Decimal) -> Filter
```

| Function | Effect |
| -------- | ------ |
| `grayscale(a)` | removes colour: 0.0 leaves it, 1.0 is fully grey; `a` is clamped to 0.0..=1.0 |
| `sepia(a)` | tints brown: 0.0 leaves the colour, 1.0 is fully sepia; clamped to 0.0..=1.0 |
| `saturate(a)` | scales saturation: 0.0 is grey, 1.0 leaves the colour, above 1.0 oversaturates |
| `invert(a)` | inverts the colour, `a·(1 − c) + (1 − a)·c`: 0.0 leaves it, 1.0 is fully inverted; clamped to 0.0..=1.0 |
| `brightness(a)` | multiplies the colour by `a`: 1.0 leaves it, 0.0 is black |
| `contrast(a)` | scales contrast around mid grey, `a·c + 0.5 − 0.5·a`: 1.0 leaves the colour, 0.0 is mid grey |

`grayscale`, `sepia` and `saturate` use the matrices of the W3C Filter Effects specification.
Kind: `const fn` · Builtin: no.

## Struct: `HueRotate` {#huerotate}

```kis
#[shader_arg]
pub struct HueRotate {
    pub angle: Angle,
}
```

The arguments of `HUE_ROTATE`: how far the hues turn.

## Constant: `HUE_ROTATE` {#hue_rotate}

```kis
pub const HUE_ROTATE: ColorShader<HueRotate> = asset("shaders/color/hue_rotate.wgsl");
```

The shader of CSS `hue-rotate()`.

## Function: `hue_rotate(angle)` {#hue_rotate-fn}

```kis
pub const fn hue_rotate(angle: Angle) -> Filter
```

- Returns: a filter that turns every hue by `angle`, with the W3C hue-rotate matrix.
- Kind: `const fn` · Builtin: no.

## Struct: `Adjust` {#adjust}

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

The seven CSS filter amounts of [`adjust`](#adjust-fn). Each field means what the function of
the same name takes; `hue` is the angle of `hue_rotate`.

## Constant: `NEUTRAL` {#neutral}

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

The amounts that change nothing. Start from it and set the ones you need.

## Constant: `ADJUST` {#adjust-shader}

```kis
pub const ADJUST: ColorShader<Adjust> = asset("shaders/color/adjust.wgsl");
```

The seven filters in one pass (TyranoScript `[filter]`).

## Function: `adjust(a)` {#adjust-fn}

```kis
pub const fn adjust(a: Adjust) -> Filter
```

- Returns: a filter that applies the amounts of `a` in TyranoScript `[filter]` order: grayscale,
  sepia, saturate, hue, invert, brightness, contrast.
- Kind: `const fn` · Builtin: no.

Tweening between two `adjust` filters moves every amount at once, so one filter can fade from a
sepia memory to a dark night scene.

## Struct: `Blur` {#blur}

```kis
#[shader_arg]
pub struct Blur {
    pub radius: Decimal,
}
```

The arguments of `BLUR`: the standard deviation in pixels; 0.0 or less is no blur.

## Constant: `BLUR` {#blur-shader}

```kis
pub const BLUR: ColorShader<Blur> = asset("shaders/color/blur.wgsl");
```

A Gaussian blur.

## Function: `blur(radius)` {#blur-fn}

```kis
pub const fn blur(radius: Decimal) -> Filter
```

- Returns: a filter that blurs with a standard deviation of `radius` pixels, like CSS `blur()`.
- Kind: `const fn` · Builtin: no.

The blur samples a 13 × 13 grid of points spaced `radius / 2` pixels apart, out to three standard
deviations in each direction.
