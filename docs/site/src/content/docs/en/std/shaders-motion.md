---
title: "std::shaders::motion"
---

> **Stability:** Tentative — motion shaders: loops and settling moves around an entity's pose, and
> paths between two poses.

Two kinds of motion:

- **Local motions** for [`Entity::local_motion`](./screen.md#entity-local_motion) return a
  [`LocalMotion`](./screen-shader.md#localmotion). They move the entity around its pose for
  drawing only: the loops repeat forever, the settling moves hold still at their end.
- **Transitional motions** for [`Entity::travel`](./screen.md#entity-travel) return a
  [`TransitionalMotion`](./screen-shader.md#transitionalmotion): a path from the entity's pose to
  the pose given to `travel`, over a duration. `.ease(…)` and `.delay(…)` adjust it.

Distances are pixels and angles turn clockwise. Every function is `const fn`, not builtin.

```kis
use std::screen::{degrees, offset, pose};
use std::shaders::motion::{bounce, decaying_quake, sine_integral, sway};

screen.entity(LANTERN).local_motion().change(Some(sway(degrees(6.0), ms(2400)))).apply_animation(instant());
screen.entity(HANAKO).travel(pose(point(1200, 1080), 1.0, degrees(0.0)), bounce(40.0, ms(900)));
screen.entity(SIGN).local_motion().change(Some(decaying_quake(12, 4, ms(800)))).apply_animation(instant());
screen.entity(CARD).local_motion().change(Some(sine_integral(offset(0, -60), 12.0, ms(2000)))).apply_animation(instant());
```

## Overview

| Function | Kind | Motion |
| -------- | ---- | ------ |
| [`bob(height, period)`](#bob) | local, loop | rises and comes back |
| [`sway(angle, period)`](#sway) | local, loop | rocks around the anchor |
| [`pulse(scale, period)`](#pulse) | local, loop | grows and shrinks |
| [`spin(period)`](#spin) | local, loop | turns around the anchor |
| [`orbit(radius, period)`](#orbit) | local, loop | circles through the pose |
| [`decaying_quake(horizontal, vertical, duration)`](#decaying_quake) | local, settling | a shake that fades out |
| [`sine_integral(amplitude, rate, settle)`](#sine_integral) | local, settling | an overshooting approach |
| [`arc(height, duration)`](#arc) | transitional | a parabolic arc |
| [`bounce(overshoot, duration)`](#bounce) | transitional | an overshooting arrival |
| [`spiral(turns, duration)`](#spiral) | transitional | a spiral into the end |

The loops use the behaviour `Periodic { after: ZERO, period }`: they start at once and repeat
every `period`. Changing a loop's arguments with a tween keeps its clock, so a bob can grow from
`bob(0.0, p)` to `bob(20.0, p)` without a jump.

## Bob {#bob}

```kis
#[shader_arg]
pub struct Bob {
    pub height: Decimal,
    pub period: Duration,
}
pub const BOB: LocalMotionShader<Bob> = asset("shaders/local_motion/bob.wgsl");
pub const fn bob(height: Decimal, period: Duration) -> LocalMotion
```

Rises by `height` pixels and comes back once per `period`, forever, easing in and out:
`y − height·(0.5 − 0.5·cos(2π·t/period))`.

## Sway {#sway}

```kis
#[shader_arg]
pub struct Sway {
    pub angle: Angle,
    pub period: Duration,
}
pub const SWAY: LocalMotionShader<Sway> = asset("shaders/local_motion/sway.wgsl");
pub const fn sway(angle: Angle, period: Duration) -> LocalMotion
```

Rocks around the anchor by `angle` either way once per `period`, forever:
`rotation + angle·sin(2π·t/period)`.

## Pulse {#pulse}

```kis
#[shader_arg]
pub struct Pulse {
    pub scale: Decimal,
    pub period: Duration,
}
pub const PULSE: LocalMotionShader<Pulse> = asset("shaders/local_motion/pulse.wgsl");
pub const fn pulse(scale: Decimal, period: Duration) -> LocalMotion
```

Grows to `scale` times its size and back once per `period`, forever, around the anchor.

## Spin {#spin}

```kis
#[shader_arg]
pub struct Spin {
    pub period: Duration,
}
pub const SPIN: LocalMotionShader<Spin> = asset("shaders/local_motion/spin.wgsl");
pub const fn spin(period: Duration) -> LocalMotion
```

Turns clockwise around the anchor once per `period`, forever.

## Orbit {#orbit}

```kis
#[shader_arg]
pub struct Orbit {
    pub radius: Decimal,
    pub period: Duration,
}
pub const ORBIT: LocalMotionShader<Orbit> = asset("shaders/local_motion/orbit.wgsl");
pub const fn orbit(radius: Decimal, period: Duration) -> LocalMotion
```

Circles clockwise on a circle of `radius` pixels that passes through the pose, once per `period`,
forever. The entity starts at its pose and comes back to it every round.

## Decaying quake {#decaying_quake}

```kis
#[shader_arg]
pub struct DecayingQuake {
    pub horizontal: i32,
    pub vertical: i32,
    pub duration: Duration,
}
pub const DECAYING_QUAKE: LocalMotionShader<DecayingQuake> = asset(
    "shaders/local_motion/decaying_quake.wgsl",
);
pub const fn decaying_quake(horizontal: i32, vertical: i32, duration: Duration) -> LocalMotion
```

A shake that fades out (TyranoScript `[quake2]`): moves by `horizontal` and `vertical` pixels,
alternating direction every frame at 60 fps, with the amplitude falling off quadratically to 0
over `duration`; then holds still at the pose. The behaviour is `Stall { after: duration }`.

## Sine integral {#sine_integral}

```kis
#[shader_arg]
pub struct SineIntegral {
    pub amplitude: Offset,
    pub rate: Decimal,
}
pub const SINE_INTEGRAL: LocalMotionShader<SineIntegral> = asset(
    "shaders/local_motion/sine_integral.wgsl",
);
pub const fn sine_integral(amplitude: Offset, rate: Decimal, settle: Duration) -> LocalMotion
```

An overshooting approach, and an example of a motion defined by a differential equation. The
entity moves by `amplitude · (2/π) · Si(rate · t)`, the solution of
`dx/dt = amplitude · (2/π) · sin(rate · t) / t` from `x = 0`: it overshoots `amplitude` and
settles at it in ringing swings. `rate` is in radians per second. The behaviour is
`Stall { after: settle }`, so the motion holds where it is at `settle`.

`Si` has no closed form; the shader integrates it numerically from 0 to `t` every frame, since a
shader keeps nothing between frames. The motion only changes how the entity is drawn: the
entity's logical position stays where it was, displaced by `amplitude` on screen once it settles.

## Arc {#arc}

```kis
#[shader_arg]
pub struct Arc {
    pub height: Decimal,
}
pub const ARC: TransitionalMotionShader<Arc> = asset("shaders/transitional_motion/arc.wgsl");
pub const fn arc(height: Decimal, duration: Duration) -> TransitionalMotion
```

Moves along a parabola that peaks `height` pixels above the straight line halfway, over
`duration`: `mix(start, end, p) − (0, height · 4p(1 − p))` for progress `p`. Scale and rotation
change linearly.

## Bounce {#bounce}

```kis
#[shader_arg]
pub struct Bounce {
    pub overshoot: Decimal,
}
pub const BOUNCE: TransitionalMotionShader<Bounce> = asset(
    "shaders/transitional_motion/bounce.wgsl",
);
pub const fn bounce(overshoot: Decimal, duration: Duration) -> TransitionalMotion
```

An overshooting arrival (animate.css `bounceIn*`): moves straight, overshoots the end by
`overshoot` pixels, comes back past it by less, overshoots once more by less still, and settles,
over `duration`. Scale and rotation change linearly. When the start and end positions are the
same, the position does not bounce.

## Spiral {#spiral}

```kis
#[shader_arg]
pub struct Spiral {
    pub turns: Decimal,
}
pub const SPIRAL: TransitionalMotionShader<Spiral> = asset(
    "shaders/transitional_motion/spiral.wgsl",
);
pub const fn spiral(turns: Decimal, duration: Duration) -> TransitionalMotion
```

Spirals into the end, turning `turns` times clockwise around it while the distance shrinks
linearly, over `duration`. Scale and rotation change linearly.
