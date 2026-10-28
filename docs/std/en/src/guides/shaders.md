# Writing Shaders

A shader is a WGSL file in the asset directory that the renderer runs on the GPU. This guide shows
how to write one for each of the six roles of [`std::screen::shader`](../screen-shader.md): what
the file defines, the arguments it reads, and the time it sees. The ready-made shaders of
[`std::shaders`](../shaders.md) follow the same rules, and their sources are good examples.

## A first shader {#a-first-shader}

A colour filter that desaturates what it covers, with the strength as its argument. The Kagari
side declares the argument type and the asset constant:

```kis
use std::screen::shader::{ColorShader, filter};

/// The arguments of `MEMORY`.
#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");

const fn flashback(mut screen: Screen) -> Screen {
    screen.filter().change(Some(filter(MEMORY, Memory { strength: 0.8 }))).apply_animation(tween(ms(800)));
    screen
}
```

The WGSL file declares `Args` with the same fields and defines `effect`:

```wgsl
// The flashback filter: desaturates the stage by `strength`.

struct Args {
    strength: f32,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = kg_source(uv);
    let gray = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(mix(c.rgb, vec3<f32>(gray), clamp(args.strength, 0.0, 1.0)), c.a);
}
```

The file holds only `Args`, `effect` and its own helpers. Kagari puts a **prelude** for the role in
front of it, which declares the bindings (such as `args`), the entry points and the `kg_` helpers.
Never declare bindings, `@vertex`, `@fragment` or `@compute` functions yourself.

## Roles {#roles}

The asset type chooses the role, and the role chooses the `effect` function the file defines:

| Asset type | `effect` | Input |
| ---------- | -------- | ----- |
| `ColorShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` is the position on the screen, 0..1 over the design resolution; `kg_source(uv)` reads what the filter covers |
| `MixShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` as for colour; `kg_source(uv)` reads the entity (its opacity applied), `kg_backdrop(uv)` what is below it. Returns the **final** pixel, so the shader composites |
| `TextureShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` is 0..1 over the paint; `kg_size()` is the paint's size in pixels |
| `MaskShader<A>` | `fn effect(uv: vec2<f32>) -> f32` | `uv` is 0..1 over the masked box; `kg_size()` is its size in pixels. Returns 0.0 to 1.0; other values are clamped |
| `LocalMotionShader<A>` | `fn effect(state: KgState, t: f32) -> KgState` | `state` is the pose of the entity at this frame; `t` the mapped time |
| `TransitionalMotionShader<A>` | `fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState` | the start and end poses, the eased progress 0..1 and the time |

WGSL reserves some words that Kagari allows as names, such as `from`; a parameter or an `Args`
field cannot be called that in the WGSL file. The parameter names of `effect` are free.

A mix shader replaces normal compositing. The mix prelude carries the helpers of the W3C
compositing formulas that the std blend modes use: `kg_straight(c)` (the straight colour,
clamped), `kg_composite(s, b, m)` (source `s` over backdrop `b` with the mixed colour `m`), and the
blend functions `kg_hard_light`, `kg_soft_light`, `kg_dodge`, `kg_burn`, `kg_lum`,
`kg_clip_color`, `kg_set_lum`, `kg_sat` and `kg_set_sat`. A multiply mode is:

```wgsl
alias Args = KgNoArgs;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    return kg_composite(s, b, kg_straight(b) * kg_straight(s));
}
```

A local motion that bobs the entity up and down:

```wgsl
struct Args {
    height: f32,
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    s.position.y -= args.height * (0.5 - 0.5 * cos(2.0 * KG_PI * t / args.period));
    return s;
}
```

## Conventions {#conventions}

- **Colours** are premultiplied and in linear light, everywhere: what `kg_source` and
  `kg_backdrop` return, `Color` arguments, and what `effect` returns. To work on sRGB values as
  CSS does, un-premultiply and convert, then convert back:
  `kg_srgb_to_linear`, `kg_linear_to_srgb`, `kg_unpremultiply`, `kg_premultiply`.
- **Positions and sizes** are pixels of the design resolution, y down.
- **`KgState`** is `{ position: vec2<f32>, scale: f32, rotation: f32 }`: `position` is where the
  entity's anchor sits, `scale` its scale, `rotation` its rotation in radians, clockwise. Opacity
  is not part of a motion.
- **Time** is in seconds: `t` and `kg_time()`.
- **The prelude's names** start with `kg_`, `Kg` or `KG_`; do not use those prefixes for your own
  names.

The prelude provides, by role:

| Helper | Roles | Value |
| ------ | ----- | ----- |
| `KG_PI` | all | π |
| `kg_time()` | all | the effect time in seconds |
| `kg_srgb_to_linear(c)`, `kg_linear_to_srgb(c)` | all | transfer functions on `vec3<f32>` |
| `kg_unpremultiply(c)`, `kg_premultiply(c)` | all | between premultiplied and straight `vec4<f32>`; a fully transparent colour un-premultiplies to 0 |
| `kg_size()` | texture, mask | the size in pixels of the paint or the masked box |
| `kg_target_size()`, `kg_texel()` | colour, mix, texture, mask | the design resolution in pixels, and one pixel in `uv` units |
| `kg_source(uv)` | colour, mix | the content under the effect |
| `kg_backdrop(uv)`, `kg_opacity()` | mix | what is below the entity, and the entity's opacity |

## Arguments {#arguments}

The `A` of a shader type is the shader's argument type: `()` or a struct marked
[`#[shader_arg]`](../../../../reference/en/src/special/attributes.md#shader_arg). A
`#[shader_arg]` struct has named fields and no type parameters (const parameters are allowed), and
every field has one of the types below. The WGSL file declares `struct Args` with the matching
members in the same order, or `alias Args = KgNoArgs;` when the argument type is `()`. Members
match by position, so a WGSL member may have another name, for example where WGSL reserves the
Kagari field's name. The prelude binds the value as `args`.

| Kagari field type | WGSL type | Value |
| ----------------- | --------- | ----- |
| `bool` | `u32` | 0 or 1 |
| `i32`, `u32` | `i32`, `u32` | as is |
| `Decimal`, `Rat` | `f32` | the nearest `f32` |
| `Color` | `vec4<f32>` | premultiplied linear light |
| `Point`, `Offset`, `Size` | `vec2<f32>` | pixels |
| `Anchor` | `vec2<f32>` | fractions |
| `Angle` | `f32` | radians, clockwise |
| `Duration` | `f32` | seconds |
| an enum without fields | `u32` | the index of the variant in its declaration, from 0 |
| a `#[shader_arg]` struct | a struct with the same members in order | field by field |
| `[T; N]`, `N` > 0 | `array<T', N>` | element by element |

The value is laid out with the WGSL storage rules (alignment and padding included), so a matching
WGSL declaration reads it as written. Arrays and nested structs work:

```kis
use std::screen::{Color, Point};

#[shader_arg]
pub struct Spot {
    pub position: Point,
    pub color: Color,
    pub radius: Decimal,
}

#[shader_arg]
pub struct Spots {
    pub count: u32,
    pub spots: [Spot; 4],
}
```

```wgsl
struct Spot {
    position: vec2<f32>,
    color: vec4<f32>,
    radius: f32,
}

struct Args {
    count: u32,
    spots: array<Spot, 4>,
}
```

- The compiler checks the Kagari side: `#[shader_arg]` on anything but a struct is
  [`E3018`](../../../../reference/en/src/diagnostics/kinds.md#e3018), a struct without named fields
  or with type parameters is [`E3025`](../../../../reference/en/src/diagnostics/kinds.md#e3025), and
  a field of another type or an array of length 0 is
  [`E3026`](../../../../reference/en/src/diagnostics/kinds.md#e3026).
- Nothing checks yet that the WGSL `Args` matches the Kagari type: keep them in step by hand.
  *Planned:* a build check that compares them.
- The laid-out value may be at most 16384 bytes. A larger value draws the slot without the effect
  and logs a warning.

## Effect time {#effect-time}

Every effect sees a time in seconds through `kg_time()` (and `t` for motions):

- **A filter, mix, mask, local motion or paint** runs on a clock of its **slot**: the screen
  filter, an entity's filter, mix, mask or local motion, or an image layer's paint. The clock
  starts at 0 when the slot gets a new shader, at the start of the tween or transition that sets
  it. It keeps running while later changes keep the same shader, whatever they do to the
  arguments, and across steps. For a paint, the same shader means the same texture shader and the
  same mask shader; for a local motion, the same shader and the same behaviour.
- When the game moves on to the next step, the clocks of unchanged slots carry over. When nothing
  carries a clock over, as after a load, a slot that already holds the effect at the step entry
  starts its clock at the step entry.
- **A mask in a transition** counts from the start of the transition.
- **A travel** counts from its start; `progress` is the travel's eased progress from 0 to 1.
- With the `animations` setting off, every effect time is 0 and travels take no time.

A local motion's [`LocalMotionBehavior`](../screen-shader.md#localmotionbehavior) maps its clock
before the shader sees it: `Periodic { after, period }` plays `0..after` once and then loops
`after..after + period`; `Stall { after }` stops at `after`. A loop shader can then rely on `t`
staying small, and a settling motion can stop exactly where its formula ends.

Shaders have no memory between frames: each frame computes the effect from the time and the
arguments alone. A motion that follows a differential equation solves it from 0 to `t` in the
shader every frame, as [`sine_integral`](../shaders-motion.md#sine_integral) does.

## Tweening arguments {#tweening-arguments}

An effect slot is a `Prop`, so `change(…).apply_animation(tween(…))` animates it. When the old and
the new effect use the same shader (and, for a local motion, the same behaviour), the tween moves
the arguments: `f32` values and vectors (decimals, colours, points, sizes, angles, durations)
follow the tween's easing, and `u32`/`i32` values (integers, `bool`, enums) switch at the start.
Any other change switches at the start of the tween. The effect clock keeps running during an
argument tween.

```kis
use std::shaders::color::blur;

screen.filter().change(Some(blur(0.0))).apply_animation(instant());
screen.wait(ms(500));
screen.filter().change(Some(blur(6.0))).apply_animation(tween(ms(1500)));
```

## Checking shaders {#checking-shaders}

`kagari build` checks every shader the game uses, under each role it is used with: it puts the
role's prelude in front of the file and validates the result. Each problem prints a line with the
asset path, the role and the line in your file (or no line when the problem is not in a line of
your file, such as a missing `Args`), and the build fails with exit status 1:

```text
error: shader `memory.wgsl` (color): line 9: …
```

A file used under two roles is checked once per role. At run time, a GPU that cannot run a role
(for example one without storage buffers or compute shaders) draws the slot as if it were `None`
and logs a warning; see [Shader effects](../screen.md#shader-effects).

Shader files are ordinary assets: `asset("…")` paths are relative to the asset directory, and the
build packs the files into the game's archives. The `std` shaders are packed into
`std.kiarchive`; the top-level `std` directory of the asset directory is reserved for them.
