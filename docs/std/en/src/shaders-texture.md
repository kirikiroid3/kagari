# `std::shaders::texture`

> **Stability:** Tentative — generated image content: solid colour, gradients, Gaussian spots,
> rain and snow.

Texture shaders for [`ImageLayer::paint`](./screen.md#imagelayer). Every function returns a
[`Paint`](./screen-shader.md#paint) of `size` pixels, shown by an image layer in place of an image
and swapped with transitions like one. A paint can be masked with
[`Paint::masked`](./screen-shader.md#paint-masked). Positions inside a paint are pixels from its
top-left corner.

```kis
use std::screen::{CENTER, degrees, rgba, size};
use std::shaders::texture::{GaussianSpot, gaussian_spots, radial_gradient, snow};

screen.background().paint(radial_gradient(CENTER, hex(0xFFE8C0), hex(0x402010), size(1920, 1080))).apply_animation(fade(ms(500)));
screen.entity(WEATHER).set_plane(Plane::Front);
screen.entity(WEATHER).image().paint(snow(WHITE, 40, 60.0, 3.0, size(1920, 1080))).apply_animation(fade(ms(1000)));
screen.entity(WEATHER).opacity().change(1.0).apply_animation(tween(ms(1000)));
screen.entity(GLOW).image().paint(gaussian_spots([
    GaussianSpot { position: point(120, 90), color: rgba(255, 200, 120, 200), radius: 40.0 },
    GaussianSpot { position: point(280, 200), color: rgba(120, 200, 255, 160), radius: 60.0 },
], size(400, 300))).apply_animation(fade(ms(300)));
```

## Overview

| Function | Arguments | Content |
| -------- | --------- | ------- |
| [`solid(color, size)`](#solid) | [`Solid`](#solid) | one colour |
| [`linear_gradient(angle, from, to, size)`](#linear_gradient) | [`LinearGradient`](#linear_gradient) | a linear gradient |
| [`radial_gradient(center, inner, outer, size)`](#radial_gradient) | [`RadialGradient`](#radial_gradient) | a radial gradient |
| [`gaussian_spots(spots, size)`](#gaussian_spots) | [`GaussianSpot`](#gaussian_spots), [`GaussianSpots<N>`](#gaussian_spots) | soft round spots of light |
| [`rain(color, density, speed, angle, size)`](#rain) | [`Rain`](#rain) | falling rain streaks |
| [`snow(color, density, speed, radius, size)`](#snow) | [`Snow`](#snow) | falling snowflakes |

Each section below lists the argument struct, the shader constant and the function, which is
`const fn`, not builtin.

## Solid {#solid}

```kis
#[shader_arg]
pub struct Solid {
    pub color: Color,
}
pub const SOLID: TextureShader<Solid> = asset("shaders/texture/solid.wgsl");
pub const fn solid(color: Color, size: Size) -> Paint
```

`size` pixels of `color`. Masked, it draws shapes: `solid(WHITE, s).masked(rounded_rect(16.0, 1.0))`
is a rounded white panel.

## Linear gradient {#linear_gradient}

```kis
#[shader_arg]
pub struct LinearGradient {
    pub angle: Angle,
    pub from: Color,
    pub to: Color,
}
pub const LINEAR_GRADIENT: TextureShader<LinearGradient> = asset(
    "shaders/texture/linear_gradient.wgsl",
);
pub const fn linear_gradient(angle: Angle, from: Color, to: Color, size: Size) -> Paint
```

`size` pixels going from `from` at the start edge to `to` at the end edge along `angle` (0° left to
right, 90° top to bottom), over the same ramp as [`mask::linear`](./shaders-mask.md#linear). The
colours mix in linear light.

## Radial gradient {#radial_gradient}

```kis
#[shader_arg]
pub struct RadialGradient {
    pub center: Anchor,
    pub inner: Color,
    pub outer: Color,
}
pub const RADIAL_GRADIENT: TextureShader<RadialGradient> = asset(
    "shaders/texture/radial_gradient.wgsl",
);
pub const fn radial_gradient(center: Anchor, inner: Color, outer: Color, size: Size) -> Paint
```

`size` pixels going from `inner` at `center` (fractions of the paint) to `outer` at the farthest
corner, over the same ramp as [`mask::radial`](./shaders-mask.md#radial).

## Gaussian spots {#gaussian_spots}

```kis
#[shader_arg]
pub struct GaussianSpot {
    pub position: Point,
    pub color: Color,
    pub radius: Decimal,
}
#[shader_arg]
pub struct GaussianSpots<const N: usize> {
    pub count: u32,
    pub spots: [GaussianSpot; N],
}
pub const fn gaussian_spots<const N: usize>(spots: [GaussianSpot; N], size: Size) -> Paint
```

`size` pixels with the sum of `spots`. A spot is a soft round light at `position` (pixels from the
paint's top-left corner) with `color` at its centre, falling off as a Gaussian of standard
deviation `radius` pixels; a `radius` of 0.0 or less draws nothing. Where the summed alpha exceeds
1.0, the sum is scaled down to alpha 1.0.

- At most 64 spots are drawn; spots after the 64th are ignored.
- `GaussianSpots<N>` is the argument type; `count` is `N`. The shader has no public constant,
  because its type depends on `N`.
- Tweening between two `gaussian_spots` paints with the same `N` moves every spot.

## Rain {#rain}

```kis
#[shader_arg]
pub struct Rain {
    pub color: Color,
    pub density: u32,
    pub speed: Decimal,
    pub angle: Angle,
    pub length: Decimal,
}
pub const RAIN: TextureShader<Rain> = asset("shaders/texture/rain.wgsl");
pub const fn rain(color: Color, density: u32, speed: Decimal, angle: Angle, size: Size) -> Paint
```

`size` pixels of rain: the paint is cut into columns 8 pixels wide, `density` percent of them hold
a drop, and each drop is a streak of `color` falling at `speed` pixels per second, tilted from
straight down by `angle`. `rain` draws streaks 24 pixels long; build a `Rain` with
[`paint`](./screen-shader.md#paint-fn) and `RAIN` for another `length`.

## Snow {#snow}

```kis
#[shader_arg]
pub struct Snow {
    pub color: Color,
    pub density: u32,
    pub speed: Decimal,
    pub radius: Decimal,
}
pub const SNOW: TextureShader<Snow> = asset("shaders/texture/snow.wgsl");
pub const fn snow(
    color: Color,
    density: u32,
    speed: Decimal,
    radius: Decimal,
    size: Size,
) -> Paint
```

`size` pixels of swaying snowflakes in three layers of depth. `density` percent of the cells of a
48-pixel grid hold a flake. In the nearest layer a flake has `radius` pixels and falls at `speed`
pixels per second; the two farther layers are smaller and slower.
