---
title: "std::shaders::mix"
---

> **Stability:** Tentative — mixes: the W3C blend modes, additive compositing and frosted glass.

Mix shaders for [`Entity::mix`](./screen.md#entity-mix). Every function returns a
[`Mix`](./screen-shader.md#mix): how the entity is composited over what is drawn below it. `None`
is normal source-over compositing.

The blend modes follow W3C Compositing and Blending Level 1 (the CSS `mix-blend-mode` values): the
mode mixes the entity's colour with the colour below where both are present, and the result is
composited source-over. They work in linear light, on the colours the renderer composites.

```kis
use std::shaders::mix::{frosted_glass, multiply};
use std::screen::rgba;

screen.entity(SHADOW).mix().change(Some(multiply())).apply_animation(instant());
screen.entity(PANEL).mix().change(Some(frosted_glass(8.0, rgba(255, 255, 255, 40)))).apply_animation(instant());
```

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`MULTIPLY` … `ADD`](#blend-shaders) | constants | the sixteen blend mode shaders |
| [`multiply()` … `add()`](#blend-modes) | functions | the blend modes as `Mix` values |
| [`FrostedGlass`](#frostedglass), [`FROSTED_GLASS`](#frosted_glass), [`frosted_glass`](#frosted_glass-fn) | struct, constant, function | frosted glass |

## Constants: blend mode shaders {#blend-shaders}

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

The shaders of the W3C blend modes `multiply` to `luminosity`, and of additive compositing. They
take no arguments.

## Functions: blend modes {#blend-modes}

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

| Function | W3C mode | Effect |
| -------- | -------- | ------ |
| `multiply()` | `multiply` | multiplies the colours: always darker |
| `screen()` | `screen` | multiplies the inverted colours: always lighter |
| `overlay()` | `overlay` | multiply or screen, by the colour below |
| `darken()` | `darken` | the darker of the two colours, per channel |
| `lighten()` | `lighten` | the lighter of the two colours, per channel |
| `color_dodge()` | `color-dodge` | brightens the colour below to reflect the entity |
| `color_burn()` | `color-burn` | darkens the colour below to reflect the entity |
| `hard_light()` | `hard-light` | multiply or screen, by the entity's colour |
| `soft_light()` | `soft-light` | a softer `hard_light` |
| `difference()` | `difference` | the absolute difference of the colours |
| `exclusion()` | `exclusion` | like `difference`, with lower contrast |
| `hue()` | `hue` | the entity's hue with the saturation and luminosity below |
| `saturation()` | `saturation` | the entity's saturation with the hue and luminosity below |
| `color()` | `color` | the entity's hue and saturation with the luminosity below |
| `luminosity()` | `luminosity` | the entity's luminosity with the hue and saturation below |
| `add()` | — | adds the entity's light to what is below; the alphas composite source-over |

Kind: `const fn` · Builtin: no.

## Struct: `FrostedGlass` {#frostedglass}

```kis
#[shader_arg]
pub struct FrostedGlass {
    pub radius: Decimal,
    pub tint: Color,
}
```

The arguments of `FROSTED_GLASS`: `radius` is the blur's standard deviation in pixels, and `tint`
the glass colour laid over the blurred backdrop.

## Constant: `FROSTED_GLASS` {#frosted_glass}

```kis
pub const FROSTED_GLASS: MixShader<FrostedGlass> = asset("shaders/mix/frosted_glass.wgsl");
```

Frosted glass (TyranoScript `[position_filter]`).

## Function: `frosted_glass(radius, tint)` {#frosted_glass-fn}

```kis
pub const fn frosted_glass(radius: Decimal, tint: Color) -> Mix
```

- Returns: a mix that, where the entity has content, blurs what is below it by `radius` pixels and
  lays `tint` over it, then draws the entity on top. Where the entity is transparent, the
  backdrop shows unchanged.
- Kind: `const fn` · Builtin: no.

A text panel with a translucent fill and `frosted_glass` blurs the scene behind it.
