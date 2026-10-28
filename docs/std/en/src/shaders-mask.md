# `std::shaders::mask`

> **Stability:** Tentative — procedural masks: ramps and shapes computed per pixel, so masks and
> transitions need no mask image.

Mask shaders for [`Entity::mask`](./screen.md#entity-mask), [`Paint::masked`](./screen-shader.md#paint-masked)
and [`anim::masked`](./screen-anim.md#transition-constructors). Every function returns a
[`Mask`](./screen-shader.md#mask) whose value lies in 0.0..=1.0 over the masked box: the entity's
box, the paint, or the layer of a transition.

- As an alpha mask, 1.0 keeps a pixel and 0.0 hides it.
- In a transition, a pixel switches once its value × 255 is at most the sweeping threshold, so low
  values go first. `masked_with(…, reverse = true, …)` sweeps the other way.

Positions are measured in pixels of the masked box, so a radial mask stays round on a wide box.

```kis
use std::screen::{CENTER, degrees};
use std::screen::anim::{masked, masked_with};
use std::shaders::mask::{clock, linear, noise, rounded_rect};

screen.background().change(BG::NIGHT).apply_animation(masked_with(linear(degrees(0.0)), 32, false, ms(800)));
screen.background().change(BG::ROOF).apply_animation(masked(noise(24.0, 7), ms(1200)));
screen.entity(CLOCK_FACE).image().change(CLOCK::FACE).apply_animation(masked(clock(CENTER, degrees(-90.0)), ms(1000)));
screen.entity(PORTRAIT).mask().change(Some(rounded_rect(24.0, 2.0))).apply_animation(instant());
```

## Overview

| Function | Arguments | Value |
| -------- | --------- | ----- |
| [`linear(angle)`](#linear) | [`LinearMask`](#linear) | a ramp across the box |
| [`radial(center)`](#radial) | [`RadialMask`](#radial) | distance from a point |
| [`diamond(center)`](#diamond) | [`DiamondMask`](#diamond) | the L1 distance from a point |
| [`clock(center, start)`](#clock) | [`ClockMask`](#clock) | the angle around a point |
| [`stripes(angle, count)`](#stripes) | [`StripesMask`](#stripes) | repeated linear ramps |
| [`checker(columns, rows)`](#checker) | [`CheckerMask`](#checker) | a checkerboard of ramps |
| [`noise(scale, seed)`](#noise) | [`NoiseMask`](#noise) | smooth random values |
| [`gaussian(center, sigma)`](#gaussian) | [`GaussianMask`](#gaussian) | a Gaussian spot |
| [`rounded_rect(radius, feather)`](#rounded_rect) | [`RoundedRectMask`](#rounded_rect) | a rounded rectangle |

Each section below lists the argument struct, the shader constant and the function, which is
`const fn`, not builtin. In the formulas, `p` is the pixel's position in the box, `size` the box
size and `c = center × size`.

## Linear {#linear}

```kis
#[shader_arg]
pub struct LinearMask {
    pub angle: Angle,
}
pub const LINEAR: MaskShader<LinearMask> = asset("shaders/mask/linear.wgsl");
pub const fn linear(angle: Angle) -> Mask
```

A linear ramp: 0.0 at the edge the direction `angle` starts from, 1.0 at the opposite edge. 0° runs
left to right, 90° top to bottom. With `d = (cos angle, sin angle)` and
`e = (|d.x|·w + |d.y|·h) / 2`, the value is `dot(p − size/2, d) / (2e) + 0.5`.

## Radial {#radial}

```kis
#[shader_arg]
pub struct RadialMask {
    pub center: Anchor,
}
pub const RADIAL: MaskShader<RadialMask> = asset("shaders/mask/radial.wgsl");
pub const fn radial(center: Anchor) -> Mask
```

A circular ramp: 0.0 at `center` (fractions of the box), 1.0 at the farthest corner. As a
transition it opens a circle from `center`.

## Diamond {#diamond}

```kis
#[shader_arg]
pub struct DiamondMask {
    pub center: Anchor,
}
pub const DIAMOND: MaskShader<DiamondMask> = asset("shaders/mask/diamond.wgsl");
pub const fn diamond(center: Anchor) -> Mask
```

Like `radial`, with the distance measured as `|dx| + |dy|`: the reveal grows as a diamond.

## Clock {#clock}

```kis
#[shader_arg]
pub struct ClockMask {
    pub center: Anchor,
    pub start: Angle,
}
pub const CLOCK: MaskShader<ClockMask> = asset("shaders/mask/clock.wgsl");
pub const fn clock(center: Anchor, start: Angle) -> Mask
```

A clock sweep: rises from 0.0 to 1.0 turning clockwise around `center`, starting at `start`
(0° points right, 90° down, so −90° starts at twelve o'clock).

## Stripes {#stripes}

```kis
#[shader_arg]
pub struct StripesMask {
    pub angle: Angle,
    pub count: u32,
}
pub const STRIPES: MaskShader<StripesMask> = asset("shaders/mask/stripes.wgsl");
pub const fn stripes(angle: Angle, count: u32) -> Mask
```

`count` stripes, each a `linear` ramp along `angle`: blinds in any direction. A `count` of 0 counts
as 1.

## Checker {#checker}

```kis
#[shader_arg]
pub struct CheckerMask {
    pub columns: u32,
    pub rows: u32,
}
pub const CHECKER: MaskShader<CheckerMask> = asset("shaders/mask/checker.wgsl");
pub const fn checker(columns: u32, rows: u32) -> Mask
```

A checkerboard of `columns` × `rows` cells, each a left-to-right ramp. The cells of one colour cover
0.0..0.5 and the others 0.5..1.0, so a transition wipes one colour of cells, then the other. A
count of 0 counts as 1.

## Noise {#noise}

```kis
#[shader_arg]
pub struct NoiseMask {
    pub scale: Decimal,
    pub seed: u32,
}
pub const NOISE: MaskShader<NoiseMask> = asset("shaders/mask/noise.wgsl");
pub const fn noise(scale: Decimal, seed: u32) -> Mask
```

Smooth random values on a grid of `scale`-pixel cells (at least 1 pixel), varied by `seed`. As a
transition it is a dissolve; a small `scale` gives a fine grain, a large one soft blotches.

## Gaussian {#gaussian}

```kis
#[shader_arg]
pub struct GaussianMask {
    pub center: Anchor,
    pub sigma: Decimal,
}
pub const GAUSSIAN: MaskShader<GaussianMask> = asset("shaders/mask/gaussian.wgsl");
pub const fn gaussian(center: Anchor, sigma: Decimal) -> Mask
```

1.0 at `center`, falling off as `exp(−|p − c|² / (2·sigma²))` with `sigma` in pixels: a vignette
or a spotlight. A `sigma` of 0.0 or less gives 0.0 everywhere.

## Rounded rectangle {#rounded_rect}

```kis
#[shader_arg]
pub struct RoundedRectMask {
    pub radius: Decimal,
    pub feather: Decimal,
}
pub const ROUNDED_RECT: MaskShader<RoundedRectMask> = asset("shaders/mask/rounded_rect.wgsl");
pub const fn rounded_rect(radius: Decimal, feather: Decimal) -> Mask
```

1.0 inside the box with its corners rounded by `radius` pixels, 0.0 outside, softened over
`feather` pixels across the edge. Use it to round the corners of a portrait or a panel.
