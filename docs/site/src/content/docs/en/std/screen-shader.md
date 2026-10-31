---
title: "std::screen::shader"
---

> **Stability:** Tentative — shader roles, typed shader arguments, and the effect values the
> screen holds: filters, mixes, masks, paints and motions.

A **shader** is a WGSL file (`.wgsl`) that the renderer runs on the GPU. Every shader has one of
six **roles**, and the type of its asset constant states the role and the type of its arguments:

```kis
use std::screen::shader::{ColorShader, filter};

#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
```

A shader is never put on the screen alone. A constructor of this module pairs it with its
arguments into an **effect value**, and the screen holds effect values:

| Role | Asset type | Effect value | Constructor | Where it goes |
| ---- | ---------- | ------------ | ----------- | ------------- |
| colour | [`ColorShader<A>`](#colorshader) | [`Filter`](#filter) | [`filter`](#filter-fn) | [`Screen::filter`](./screen.md#screen-filter), [`Entity::filter`](./screen.md#entity-filter) |
| mix | [`MixShader<A>`](#mixshader) | [`Mix`](#mix) | [`mix`](#mix-fn) | [`Entity::mix`](./screen.md#entity-mix) |
| mask | [`MaskShader<A>`](#maskshader) | [`Mask`](#mask) | [`mask`](#mask-fn) | [`Entity::mask`](./screen.md#entity-mask), [`Paint::masked`](#paint-masked), [`anim::masked`](./screen-anim.md#transition-constructors) |
| texture | [`TextureShader<A>`](#textureshader) | [`Paint`](#paint) | [`paint`](#paint-fn) | [`ImageLayer::paint`](./screen.md#imagelayer) |
| local motion | [`LocalMotionShader<A>`](#localmotionshader) | [`LocalMotion`](#localmotion) | [`local_motion`](#local_motion-fn) | [`Entity::local_motion`](./screen.md#entity-local_motion) |
| transitional motion | [`TransitionalMotionShader<A>`](#transitionalmotionshader) | [`TransitionalMotion`](#transitionalmotion) | [`transitional_motion`](#transitional_motion-fn) | [`Entity::travel`](./screen.md#entity-travel) |

The constructors take the arguments with the shader's argument type `A`, so a shader cannot be
given arguments of another type, and a shader of one role cannot be used where another role is
expected. Effect values hide their fields; the constructors are the only way to build them.

Effect values are `Eq`: two effects are equal when they use the same shader file with equal
arguments. They are part of the [logical screen](./screen.md#the-logical-screen), so two screens
whose filters differ only in an argument are different screens. Shader types and effect values
are not finite.

`std` ships ready-made shaders in [`std::shaders`](./shaders.md). How to write a shader of each
role, the argument layout, and the time a shader sees are described in
[Writing Shaders](./guides/shaders.md). Nothing of this module is in the prelude; import it with
`use std::screen::shader::…`.

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`ShaderArg`](#shaderarg), [`Shader`](#shader) | builtin traits | argument types, shader types |
| [`ColorShader<A>`](#colorshader) … [`TransitionalMotionShader<A>`](#transitionalmotionshader) | builtin structs | a shader file of one role |
| [`ShaderArgs`](#shaderargs) | builtin struct | arguments with their type erased |
| [`Filter`](#filter), `filter` | struct + constructor | a colour filter |
| [`Mix`](#mix), `mix` | struct + constructor | a way to composite |
| [`Mask`](#mask), `mask` | struct + constructor | an alpha mask |
| [`Paint`](#paint), `paint` | struct + constructor | generated image content |
| [`LocalMotionBehavior`](#localmotionbehavior) | enum | how a local motion's time runs |
| [`LocalMotion`](#localmotion), `local_motion` | struct + constructor | a motion around the entity's pose |
| [`TransitionalMotion`](#transitionalmotion), `transitional_motion` | struct + constructor | a path from one pose to another |

## Trait: `ShaderArg` {#shaderarg}

```kis
#[builtin] pub trait ShaderArg {}
```

The types a shader can take as its arguments: the unit type `()` and every struct marked
[`#[shader_arg]`](../reference/special/attributes.md#shader_arg). The compiler
implements it; a game cannot. A `#[shader_arg]` struct has named fields of the types listed in
[Arguments](./guides/shaders.md#arguments).

## Trait: `Shader` {#shader}

```kis
#[builtin] pub trait Shader {}
```

The six shader asset types below.

## Struct: `ColorShader<A>` {#colorshader}

```kis
#[builtin] pub struct ColorShader<A>;
impl<A> Asset for ColorShader<A> {}
impl<A> Eq for ColorShader<A> {}
impl<A> Shader for ColorShader<A> {}
```

A WGSL file that recolours what is drawn below it, with arguments of type `A`; used by
[`filter`](#filter-fn). Declared with `asset("….wgsl")`.

## Struct: `MixShader<A>` {#mixshader}

```kis
#[builtin] pub struct MixShader<A>;
impl<A> Asset for MixShader<A> {}
impl<A> Eq for MixShader<A> {}
impl<A> Shader for MixShader<A> {}
```

A WGSL file that composites an entity over what is below it; used by [`mix`](#mix-fn).

## Struct: `TextureShader<A>` {#textureshader}

```kis
#[builtin] pub struct TextureShader<A>;
impl<A> Asset for TextureShader<A> {}
impl<A> Eq for TextureShader<A> {}
impl<A> Shader for TextureShader<A> {}
```

A WGSL file that generates image content; used by [`paint`](#paint-fn).

## Struct: `MaskShader<A>` {#maskshader}

```kis
#[builtin] pub struct MaskShader<A>;
impl<A> Asset for MaskShader<A> {}
impl<A> Eq for MaskShader<A> {}
impl<A> Shader for MaskShader<A> {}
```

A WGSL file that computes an alpha value per pixel; used by [`mask`](#mask-fn).

## Struct: `LocalMotionShader<A>` {#localmotionshader}

```kis
#[builtin] pub struct LocalMotionShader<A>;
impl<A> Asset for LocalMotionShader<A> {}
impl<A> Eq for LocalMotionShader<A> {}
impl<A> Shader for LocalMotionShader<A> {}
```

A WGSL file that moves an entity around its pose; used by [`local_motion`](#local_motion-fn).

## Struct: `TransitionalMotionShader<A>` {#transitionalmotionshader}

```kis
#[builtin] pub struct TransitionalMotionShader<A>;
impl<A> Asset for TransitionalMotionShader<A> {}
impl<A> Eq for TransitionalMotionShader<A> {}
impl<A> Shader for TransitionalMotionShader<A> {}
```

A WGSL file that moves an entity from one pose to another; used by
[`transitional_motion`](#transitional_motion-fn).

## Struct: `ShaderArgs` {#shaderargs}

```kis
#[builtin] pub struct ShaderArgs;
impl Eq for ShaderArgs {}
```

Shader arguments with their type erased, as the effect values hold them. Values compare by the
arguments they hold. A game never builds one directly.

## Struct: `Filter` {#filter}

```kis
pub struct Filter {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

A colour filter: a colour shader with its arguments. A filter recolours what it covers: the
`Background`, `Stage` and `Front` planes for the screen filter, the entity for an entity filter.

### `filter(shader, args)` {#filter-fn}

```kis
pub const fn filter<A: ShaderArg>(shader: ColorShader<A>, args: A) -> Filter
```

- `shader` `ColorShader<A>` — the shader.
- `args` `A` — its arguments.
- Returns: `Filter` — the filter `shader` with `args`.
- Kind: `const fn` · Builtin: no.

```kis
screen.filter().change(Some(filter(MEMORY, Memory { strength: 0.8 }))).apply_animation(tween(ms(800)));
```

## Struct: `Mix` {#mix}

```kis
pub struct Mix {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

A way to composite an entity over what is below it: a mix shader with its arguments. The shader
sees the entity and the backdrop and returns the composited pixel, so blend modes and effects such
as frosted glass are mixes.

### `mix(shader, args)` {#mix-fn}

```kis
pub const fn mix<A: ShaderArg>(shader: MixShader<A>, args: A) -> Mix
```

- Returns: `Mix` — the mix `shader` with `args`.
- Kind: `const fn` · Builtin: no.

## Struct: `Mask` {#mask}

```kis
pub struct Mask {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

An alpha mask: a mask shader with its arguments. The shader returns a value from 0.0 to 1.0 for
each pixel of a box. A mask is used in three places:

- [`Entity::mask`](./screen.md#entity-mask): the entity's alpha is multiplied by the mask over
  the entity's box.
- [`Paint::masked`](#paint-masked): the paint's alpha is multiplied by the mask over the paint.
- [`anim::masked`](./screen-anim.md#transition-constructors): a transition switches a pixel once
  the mask's value is reached by a sweeping threshold.

### `mask(shader, args)` {#mask-fn}

```kis
pub const fn mask<A: ShaderArg>(shader: MaskShader<A>, args: A) -> Mask
```

- Returns: `Mask` — the mask `shader` with `args`.
- Kind: `const fn` · Builtin: no.

## Struct: `Paint` {#paint}

```kis
pub struct Paint {
    texture: ShaderAsset,
    args: ShaderArgs,
    size: Size,
    mask: Option<Mask>,
}
```

Generated image content of `size` pixels: a texture shader with its arguments, optionally masked.
An [`ImageLayer`](./screen.md#imagelayer) shows a paint in place of an image with
`ImageLayer::paint`; it is placed, sized, [resized](./screen.md#resize), flipped and swapped with
transitions like an image of `size` pixels. Gradients, light spots and weather are paints.

### `paint(shader, args, size)` {#paint-fn}

```kis
pub const fn paint<A: ShaderArg>(shader: TextureShader<A>, args: A, size: Size) -> Paint
```

- `size` `Size` — the size of the content in pixels.
- Returns: `Paint` — the content `shader` draws with `args`, `size` pixels large, unmasked.
- Kind: `const fn` · Builtin: no.

### `paint.masked(mask)` {#paint-masked}

```kis
pub const fn masked(mut self, mask: Mask) -> Paint
```

- Returns: this paint with its alpha multiplied by `mask`, computed over the paint's box. A
  second `masked` replaces the first mask.
- Kind: `const fn` · Builtin: no.

```kis
use std::screen::{degrees, size};
use std::shaders::mask::rounded_rect;
use std::shaders::texture::linear_gradient;

screen.entity(PANEL).image().paint(linear_gradient(degrees(90.0), WHITE, BLACK, size(400, 300)).masked(rounded_rect(32.0, 4.0))).apply_animation(fade(ms(300)));
```

## Enum: `LocalMotionBehavior` {#localmotionbehavior}

```kis
pub enum LocalMotionBehavior {
    Periodic { after: Duration, period: Duration },
    Stall { after: Duration },
}
```

How a local motion's time runs. A local motion shader receives a time `t` in seconds; the
behaviour maps the time since the motion started before the shader sees it:

| Variant | `t` the shader sees |
| ------- | ------------------- |
| `Periodic { after, period }` | runs from 0 to `after` once, then loops the following `period`: `after + ((time − after) mod period)`. A `period` of zero holds at `after` |
| `Stall { after }` | runs until `after`, then holds: `min(time, after)` |

`Periodic { after: ZERO, period }` is a plain loop; a non-zero `after` plays an intro before the
loop. `Stall` suits motions that settle, such as a decaying shake.

## Struct: `LocalMotion` {#localmotion}

```kis
pub struct LocalMotion {
    shader: ShaderAsset,
    args: ShaderArgs,
    behavior: LocalMotionBehavior,
}
```

A motion around the entity's pose: a local motion shader with its arguments and time rule. The
shader receives the pose the entity has at that frame (with tweens and
[`Motion`](./screen-anim.md#motion)s applied) and returns the pose to draw. It changes only how
the entity is drawn: the logical position, scale and rotation stay.

### `local_motion(shader, args, behavior)` {#local_motion-fn}

```kis
pub const fn local_motion<A: ShaderArg>(shader: LocalMotionShader<A>, args: A, behavior: LocalMotionBehavior) -> LocalMotion
```

- `behavior` `LocalMotionBehavior` — how the motion's time runs.
- Returns: `LocalMotion` — the local motion `shader` with `args`, its time mapped by `behavior`.
- Kind: `const fn` · Builtin: no.

## Struct: `TransitionalMotion` {#transitionalmotion}

```kis
pub struct TransitionalMotion {
    shader: ShaderAsset,
    args: ShaderArgs,
    duration: Duration,
    easing: Easing,
    delay: Duration,
}
```

A path from one pose to another: a transitional motion shader with its arguments and timing.
[`Entity::travel`](./screen.md#entity-travel) moves an entity along it. The shader receives the
start pose, the end pose, the eased progress from 0 to 1 and the time, and returns the pose to
draw. When the duration is over, the entity is at the end pose whatever the shader returned.

### `transitional_motion(shader, args, duration)` {#transitional_motion-fn}

```kis
pub const fn transitional_motion<A: ShaderArg>(shader: TransitionalMotionShader<A>, args: A, duration: Duration) -> TransitionalMotion
```

- `duration` `Duration` — how long the travel takes.
- Returns: `TransitionalMotion` — the motion `shader` with `args` over `duration`, with easing
  `Linear` and no delay.
- Kind: `const fn` · Builtin: no.

### `motion.ease(easing)`, `motion.delay(delay)` {#transitionalmotion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> TransitionalMotion
pub const fn delay(mut self, delay: Duration) -> TransitionalMotion
```

`ease` returns this motion with `easing` on its progress; `delay` returns it starting `delay`
after the cursor. The easing is checked like a tween's: an invalid `CubicBezier` or `Steps(0)` is
a const-evaluation error. Kind: `const fn` · Builtin: no.

```kis
use std::screen::{degrees, pose};
use std::screen::anim::Easing;
use std::shaders::motion::arc;

screen.entity(BIRD).travel(pose(point(900, 400), 1.0, degrees(0.0)), arc(200.0, ms(1200)).ease(Easing::OutQuad));
```
