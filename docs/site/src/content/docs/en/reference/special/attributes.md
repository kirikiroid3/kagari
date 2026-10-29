---
title: "Attributes"
---

An attribute is written `#[name]` directly before an item. Kagari has exactly five attributes and
none of them takes arguments. Games use four of them; `#[builtin]` is reserved for the standard
library.

| Attribute | Allowed on | Meaning | Misuse |
| --------- | ---------- | ------- | ------ |
| [`#[scene]`](#scene) | `pushdown fn` | A scene: may run steps, call scenes and draw random values | `E3018` |
| [`#[entry]`](#entry) | a `#[scene]` function | An entry scene: the system can start the story here | `E3018`, `E3008`, `E3009` |
| [`#[must_use]`](#must_use) | `struct`, `enum` | A value of the type must be completed, not dropped | `E3018`; warning `W3023` at the use |
| [`#[shader_arg]`](#shader_arg) | `struct` | The type can be a shader's arguments | `E3018`, `E3025`, `E3026` |
| [`#[builtin]`](#builtin) | standard library only: functions, structs, traits | Implemented by the compiler and the engine | `E1008`, `E1009`, `E1013` |

## Syntax

```kix
/// The first scene of the game.
#[scene]
#[entry]
pub pushdown fn Main() {
    s!(t!["It was the last day of summer."]);
}
```

- An attribute stands before an item: a function (including an associated function in an `impl`),
  `struct`, `enum`, `const`, `static`, `type`, `mod`, `use`, `impl` or `trait`. It comes after the
  item's doc comments and before `pub`.
- Several attributes may be stacked, on one line or on separate lines, in any order.
- `///` doc comments are comments, not attributes.
- Inner attributes (`#![…]`) do not exist, and an attribute must be followed by an item: both are
  syntax errors (`E0001`). Attributes cannot be put on statements or expressions.
- A name other than the five is `E1006`, and the attribute is ignored.
- An attribute written with arguments, such as `#[must_use("reason")]`, is `E1007`; the attribute
  still applies.

*Tentative.* The parser also accepts attributes on struct fields, tuple fields and enum variants,
but the compiler currently ignores them there, including unknown names. Do not rely on it.

## `#[scene]` {#scene}

`#[scene]` makes a `pushdown fn` a **scene**. Only a scene may:

- run a step: `s!(…)`, `choice!{…}` or a call of `push_step` (`E3002` elsewhere);
- call another scene, including itself (`E3003` elsewhere);
- draw a random value with `rng_choose` (`E3024` elsewhere).

A `pushdown fn` without `#[scene]` is an **instant procedure**. Every loop and every cycle of calls
in a scene must pass through a step (`E5001`, `E5002`). A pointer to a scene has the type
`scene fn(…) -> R`.

```kix
#[scene]
pushdown fn Greeting() {
    s!(who = "Mio", t!["Hello."]);
}
```

`#[scene]` on anything but a `pushdown fn` is `E3018`, for example on a `fn` or a `const fn`.

See [Scenes and Instant Procedures](../concepts/scenes.md).

## `#[entry]` {#entry}

`#[entry]` marks a scene as an **entry scene**: a place where the game system can start the story.
It needs `#[scene]` on the same function; without it, `#[entry]` is `E3018`. The order of the two
attributes does not matter.

- The system starts an entry scene with `nav::start(Scene, args, on_exit)`. The first argument
  must be the path of an `#[entry]` scene, not any other expression (`E2027`); `args` is the tuple
  of its parameters, and `on_exit` receives its return value.
- Like every scene, its parameter and return types must be finite (`E3008`). In addition, they must
  not contain `Text` (`E3009`), because `fn` code can build text at runtime.
- When the entry scene returns, the story is over. Its return value is the **ending**.
- The compiler analyzes the story starting from each entry scene. Write entry scenes without
  generic parameters: a generic `#[entry]` scene is not used as a starting point.

```kix
pub enum Ending { NormalEnd, AzusaEnd }

#[scene]
#[entry]
pub pushdown fn Main(cleared_azusa: bool) -> Ending {
    s!(t!["Morning."]);
    if cleared_azusa { Ending::AzusaEnd } else { Ending::NormalEnd }
}
```

See [Entry Scenes and Endings](../concepts/entry.md).

## `#[must_use]` {#must_use}

`#[must_use]` on a `struct` or an `enum` says that a value of the type is an unfinished operation.
On any other item it is `E3018`. The compiler warns (`W3023`) when:

- an expression statement produces a value of the type, or a reference to one; or
- a `let` binds such a value to a local, and the function never calls a method of the type on that
  local.

The standard library marks its pending changes `Change<T>`, `Swap` and `ChannelChange`: a change of
the screen happens only through `apply_animation`, so a forgotten `apply_animation` is caught at
compile time.

```kis
const fn roof(mut screen: Screen) -> Screen {
    screen.background().clear();                                // W3023: nothing happens
    screen.background().clear().apply_animation(fade(ms(500))); // completed
    screen
}
```

A game may mark its own types too. The warning then lists the type's methods as the ways to
complete the value.

## `#[shader_arg]` {#shader_arg}

`#[shader_arg]` on a `struct` makes it a shader argument type: the type implements `ShaderArg`, so
a shader asset such as `ColorShader<Memory>` can take it, and the engine passes its value to the
shader. On any other item it is `E3018`.

```kis
use std::screen::shader::ColorShader;

#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
```

- The struct has named fields, at least one, and no type parameters; const parameters are allowed
  (`E3025` otherwise).
- Every field has a type a shader can read: `bool`, `i32`, `u32`, `Decimal`, `Rat`, the std types
  `Color`, `Point`, `Offset`, `Size`, `Anchor`, `Angle` and `Duration`, an enum without fields, a
  `#[shader_arg]` struct, or an array `[T; N]` of one of these with `N` > 0. Any other field type,
  or an array of length 0, is `E3026` at the field.

How each field reaches the shader is described in
[Writing Shaders](../../std/guides/shaders.md#arguments) in the STD Reference.

## `#[builtin]` {#builtin}

`#[builtin]` marks an item that the compiler and the engine implement. It is allowed only in the
standard library (`E1008`); you meet it when reading `std` declarations:

```kis
#[builtin] pub struct Screen;
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
#[builtin] pub trait Eq {}
```

| On | Rule |
| -- | ---- |
| a function | It has no body (`E1009`). It is called like any function, and the kind rules apply to it as to any other function |
| a struct | It has no fields (`E1009`). The type is opaque; see [Opaque builtin types](./builtin-types.md#opaque-builtin-types) |
| a trait | Every `std` trait must be `#[builtin]` (`E1013`); see [Language Traits](./traits.md) |

A function without a body that is not `#[builtin]` is `E1010`.

## Diagnostics

| Code | When |
| ---- | ---- |
| [`E0001`](../../diagnostics/syntax.md#e0001) | An inner attribute `#![…]`, or attributes not followed by an item |
| [`E1006`](../../diagnostics/modules.md#e1006) | An unknown attribute |
| [`E1007`](../../diagnostics/modules.md#e1007) | An attribute with arguments |
| [`E1008`](../../diagnostics/modules.md#e1008) | `#[builtin]` outside the standard library |
| [`E1009`](../../diagnostics/modules.md#e1009) | A `#[builtin]` function with a body, or a `#[builtin]` struct with fields |
| [`E1013`](../../diagnostics/modules.md#e1013) | A standard library trait without `#[builtin]` |
| [`E3018`](../../diagnostics/kinds.md#e3018) | `#[scene]` not on a `pushdown fn`, `#[entry]` without `#[scene]`, `#[must_use]` not on a struct or enum, `#[shader_arg]` not on a struct |
| [`E3008`](../../diagnostics/kinds.md#e3008), [`E3009`](../../diagnostics/kinds.md#e3009) | An entry scene's parameter or return type is not finite, or contains `Text` |
| [`E2027`](../../diagnostics/types.md#e2027) | `nav::start` of something that is not an `#[entry]` scene |
| [`W3023`](../../diagnostics/kinds.md#w3023) | A `#[must_use]` value dropped or never completed |
| [`E3025`](../../diagnostics/kinds.md#e3025) | A `#[shader_arg]` struct without named fields, or with type parameters |
| [`E3026`](../../diagnostics/kinds.md#e3026) | A `#[shader_arg]` field of a type a shader cannot read, or an array of length 0 |
