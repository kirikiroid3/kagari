---
title: "Language Traits"
---

Traits in Kagari are not a tool for games. The standard library declares a fixed set of traits,
each marked [`#[builtin]`](./attributes.md#builtin), and only the standard library implements them.
They exist so that `std` signatures type-check without overloading or implicit conversions, and so
that the compiler can state its rules ("`==` needs `Eq`", "`rng_choose` draws a `RandChoose`").

A game uses these traits in one way only: as **bounds** on its own generic parameters. Two of
them, `Eq` and `RandChoose`, also hold automatically for suitable types a game declares, and
`ShaderArg` holds for the structs a game marks [`#[shader_arg]`](./attributes.md#shader_arg).

## Overview

| Trait | Declared in | In the prelude | Implemented by | Used by |
| ----- | ----------- | -------------- | -------------- | ------- |
| [`Eq`](#eq) | `std` | yes | primitives, function pointers, and structurally every type whose parts are `Eq`; some builtin types | `==`, `!=`, `ui::select` |
| [`Integer`](#integer) | `std` | yes | `Int<lo, hi>` and the Rust integers | `into`, `saturate` |
| [`ToText`](#totext) | `std::narrative::text` | no | `Text`, `String`, integers, `Rat`, `Decimal` | `{expr}` in `t![…]`, `to_text` |
| [`RandChoose`](#randchoose) | `std` | yes | `Int<lo, hi>` and every fieldless enum | `rng_choose` |
| [`Asset`](#asset) | `std` | no | the five file asset types and the six shader types | `asset` |
| [`Step`](#step) | `std::narrative` | no | `Say`, `Choice<A, N>` | `push_step`, `s!`, `choice!` |
| [`ShaderArg`](#shaderarg) | `std::screen::shader` | no | `()` and `#[shader_arg]` structs | the shader constructors such as `filter` |
| [`Shader`](#shader) | `std::screen::shader` | no | the six shader types | `std` internals |

A trait outside the prelude is named by its path, for example `std::narrative::text::ToText`, or
imported with `use`.

## Rules for games {#rules-for-games}

- A `trait` item in a game is an error (`E1012`).
- An `impl Trait for Type` in a game is an error (`E1023`). For `Eq` the message points out that
  `Eq` is automatic.
- There is no `derive`, no user-declared associated type, and no qualified path such as
  `<T as Trait>::f`.
- A game may write inherent `impl` blocks for its own structs and enums, with methods and
  associated functions; that is unrelated to traits.

Inside `std`, every trait must be `#[builtin]` (`E1013`).

## Bounds

Functions, structs, enums and `impl` blocks may be generic over types and constants. A type
parameter may carry bounds, joined with `+`:

```kis
use std::narrative::text::{ToText, to_text};

const fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

const fn describe<T: Eq + ToText>(a: T, b: T) -> Text {
    if a == b { to_text(a) } else { "different" }
}
```

- Bounds are written only in the parameter list. There are no `where` clauses.
- A bound must name a trait (`E2008`). A type argument that does not implement a bound is
  `E2026`.
- Inside generic code, a type parameter implements exactly the traits of its bounds: `a == b` on a
  `T` without `T: Eq` is `E2017`.
- In a `pushdown fn`, every type argument must be a [finite type](../concepts/kinds.md#finite-types).
  Generic code is instantiated for each set of arguments, so the story stays a finite program.

## `Eq`

```kis
#[builtin] pub trait Eq {}
```

`Eq` marks types whose values compare with `==` and `!=`. Both operands must have the same type,
and that type must be `Eq` (`E2017`). The `std::ui` function `select` also needs `T: Eq` for its
value type.

`Eq` is implemented automatically; there is nothing to write:

| Type | `Eq` |
| ---- | ---- |
| `bool`, `()`, `Int<lo, hi>`, the Rust integers | always |
| function pointers and named functions | always; they compare by identity |
| tuples, arrays | when every element type is `Eq` |
| structs and enums declared in Kagari (yours or `std`'s) | when every field of every variant is `Eq`; two values are equal when they are built the same way |
| `&T`, `&mut T` | when `T` is `Eq`; they compare the values they point to |
| `#[builtin]` types | only through an `impl Eq` in `std`: `String`, `Vec<T>` of an `Eq` `T`, `Rat`, `Decimal`, `Text`, `Screen`, the six asset types, the six shader types and `ShaderArgs` |
| closures | never |

So `Setting<T>`, `Meta<T>`, `Narrative`, `Element`, `Placed`, `Arm`, `Divergence`, the screen parts
(`Entity`, `Prop<T>`, the layers, `Hud`, `ChoiceMenu`, `Camera`, `Channel`) and the pending changes
(`Change<T>`, `Swap`, `ChannelChange`) are not `Eq`, and neither is any struct or enum that
contains one of them. There are no floats in Kagari partly so that every number type is `Eq`.

## `Integer`

```kis
#[builtin] pub trait Integer {
    const fn into<T: Integer>(self) -> T;
    const fn saturate<T: Integer>(self) -> T;
}
```

`Integer` is implemented by `Int<lo, hi>` and by `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`,
`i32`, `i64`, `i128`, `usize` and `isize`. Its two methods are the only conversions between integer
types; the target type is inferred from context.

| Method | Result |
| ------ | ------ |
| `x.into()` | The same value. A compile error (`E2018`) unless every value of the source type fits in the target type |
| `x.saturate()` | The value clamped to the range of the target type |

```kis
static MUSIC: Setting<Volume> = setting(volume(60));

fn set_music_level(level: u32) {
    let level: Int<0, 100> = level.saturate();
    MUSIC.set(volume(level));
}

const fn label_width(chars: Int<0, 40>) -> u32 {
    let n: u32 = chars.into();
    n * 16
}
```

There are no `as` casts (`E2019`). See [Numbers](../syntax/numbers.md).

## `ToText`

```kis
#[builtin] pub trait ToText {}
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

`ToText` marks the types an `{expr}` item of `t![…]` may interpolate: `{expr}` is a call of
`to_text(expr)`. It is implemented by `Text`, `String`, `Int<lo, hi>`, every Rust integer, `Rat`
and `Decimal`. `bool` and enums are not `ToText`; match on them and write the text instead.
Interpolating another type is `E2026`.

In a `pushdown fn`, `to_text` of a finite runtime value, such as an `Int<0, 5>` story variable, is
tabulated like any `const fn` call, so the line stays a finite function of the story state.

## `RandChoose`

```kis
#[builtin] pub trait RandChoose {}
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

`RandChoose` marks the types a scene can draw at random with `rng_choose::<T>()`. It is
implemented by `Int<lo, hi>` and, automatically, by every enum that has at least one variant and no
variant with fields. `bool` and enums with data are not `RandChoose`. Only a scene may call
`rng_choose` (`E3024`).

```kix
enum Weather { Sunny, Rainy, Snowy }

#[scene]
pushdown fn Morning() {
    match rng_choose::<Weather>() {
        Weather::Sunny => s!(t!["The sun is out."]),
        Weather::Rainy => s!(t!["It is raining."]),
        Weather::Snowy => s!(t!["Snow covers the yard."]),
    }
}
```

See [Randomness](../concepts/scenes.md#randomness).

## `Asset`

```kis
#[builtin] pub trait Asset {}
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

`Asset` marks the types whose values are exactly the asset constants of the program:
`ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset` and `FontAsset`, and the six shader types
`ColorShader<A>`, `MixShader<A>`, `TextureShader<A>`, `MaskShader<A>`, `LocalMotionShader<A>`
and `TransitionalMotionShader<A>`. It is the bound of `asset`, which is how an asset constant is
declared. See [Asset types](./builtin-types.md#asset-types).

## `Step`

```kis
#[builtin] pub trait Step {
    type Output;
}
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`Step` marks the values the story can suspend on. It is implemented by `Say`, the step written
`s!(…)`, with `Output = ()`, and by `Choice<A, N>`, the step written `choice!{…}`, with `Output = A`,
the type of the arms' values. `push_step` runs the step and resumes with its output. It is the only
trait with an associated type. Only a scene may run a step (`E3002`). See
[Steps](../concepts/steps.md).

## `ShaderArg`

```kis
#[builtin] pub trait ShaderArg {}
```

`ShaderArg` marks the types a shader can take as its arguments: the unit type `()` and every
struct marked [`#[shader_arg]`](./attributes.md#shader_arg). The constructors of
`std::screen::shader` bound their argument type with it, as in
`filter<A: ShaderArg>(shader: ColorShader<A>, args: A)`. See
[`std::screen::shader`](../../std/screen-shader.md).

## `Shader`

```kis
#[builtin] pub trait Shader {}
```

`Shader` marks the six shader types. The standard library uses it to accept a shader of any role;
a game has no use for it.

## Closure types are not traits

`Fn(A) -> B`, `const Fn(A) -> B` and `pushdown Fn(A) -> B` look like Rust's `Fn` traits, but in
Kagari they are types: closure types that carry a [kind](../concepts/kinds.md). They cannot be used
as bounds. See [Function pointer and closure types](./builtin-types.md#function-types).
