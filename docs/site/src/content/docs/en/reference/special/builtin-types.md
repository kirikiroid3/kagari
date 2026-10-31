---
title: "Builtin Types"
---

Most types a game uses are ordinary declarations in `std`, written in Kagari like your own. A few
types are known to the compiler by name: they have their own syntax, their own checks, or no
visible representation at all. This chapter lists them and says what is special about each: whether
it is opaque, whether it is [finite](../concepts/kinds.md#finite-types), whether it is `Eq`, in
which [kinds](../concepts/kinds.md) it is useful, and what copying it means. The methods are
documented in the [STD Reference](../../std/usage.md).

Every type in Kagari has value semantics: assignment, passing an argument and returning copy the
value (see [Values, References and Closures](../syntax/values.md)). For the handle types below,
the value that is copied is the handle, so both copies name the same stored thing.

## Overview

| Type | Defined as | Finite | `Eq` | Typical kinds | STD page |
| ---- | ---------- | ------ | ---- | ------------- | -------- |
| `bool`, `()` | language | yes | yes | all | — |
| `Int<lo, hi>` | language | yes | yes | all | — |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | language | no | yes | `fn`, `const fn` | — |
| `Rat`, `Decimal` | `#[builtin]` struct | no | yes | `fn`, `const fn` | [`std`](../../std/std.md) |
| `String` | `#[builtin]` struct | no | yes | `fn`, `const fn` | [`std`](../../std/std.md) |
| `Text` | `#[builtin]` struct | yes | yes | all | [`std::narrative::text`](../../std/narrative-text.md) |
| `ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset` | `#[builtin]` structs | yes | yes | all | [`std`](../../std/std.md) |
| `ColorShader<A>`, `MixShader<A>`, `TextureShader<A>`, `MaskShader<A>`, `LocalMotionShader<A>`, `TransitionalMotionShader<A>` | `#[builtin]` structs | no | yes | `const fn` | [`std::screen::shader`](../../std/screen-shader.md) |
| `Vec<T>` | `#[builtin]` struct | no | if `T: Eq` | `fn`, `const fn` | [`std`](../../std/std.md) |
| `Range<T>` | struct | if `T` is | if `T: Eq` | all | [`std`](../../std/std.md) |
| `Option<T>`, `Result<T, E>` | enums | if the arguments are | if the arguments are | all | [`std`](../../std/std.md) |
| `Setting<T>`, `Meta<T>` | `#[builtin]` structs | no | no | `fn` (see below) | [`std::system`](../../std/system.md), [`std::narrative`](../../std/narrative.md) |
| `Narrative` | `#[builtin]` struct | no | no | `fn` | [`std::narrative`](../../std/narrative.md) |
| `Screen` | `#[builtin]` struct | no | yes | `const fn` | [`std::screen`](../../std/screen.md) |
| `Entity`, `Prop<T>`, `ImageLayer`, `LabelLayer`, `MovieLayer`, `Hud`, `ChoiceMenu`, `Camera`, `Channel` | `#[builtin]` structs | no | no | `const fn` | [`std::screen`](../../std/screen.md), [`std::screen::audio`](../../std/screen-audio.md) |
| `Change<T>`, `Swap`, `ChannelChange` | `#[must_use]` `#[builtin]` structs | no | no | `const fn` | [`std::screen`](../../std/screen.md), [`std::screen::audio`](../../std/screen-audio.md) |
| `Element`, `Placed` | `#[builtin]` structs | no | no | `fn` | [`std::ui`](../../std/ui.md) |
| `Arm`, `Divergence` | `#[builtin]` structs | no | no | `fn` | [`std::narrative`](../../std/narrative.md), [`std::system::saves`](../../std/system-saves.md) |
| `Render` | type alias of `const fn(Screen) -> Screen` | yes | yes | all | [`std::screen`](../../std/screen.md) |
| `Duration` | struct | no | yes | all, as constants in the story | [`std::time`](../../std/time.md) |
| function pointers | language | all kinds but `fn` | yes | all | — |
| closures `Fn(…)` | language | no | no | all; second-class in the story | — |

"Finite" decides whether a [`pushdown fn`](../concepts/kinds.md#pushdown-fn) may hold a value of
the type at runtime (`E3008`). A constant expression of any type may still appear in the story,
because it is folded at compile time.

## Opaque builtin types {#opaque-builtin-types}

A struct declared with [`#[builtin]`](./attributes.md#builtin) is implemented by the compiler and
the engine. It has no visible fields:

- You cannot write a struct literal for it, read a field of it, or destructure it in a pattern.
  Values come only from `std` functions and methods, from literals (`Text`, `Decimal`), or from the
  engine (`Narrative`).
- It is `Eq` only when `std` says so with an `impl Eq`. The structural `Eq` rule does not look
  inside it (see [Language Traits](./traits.md#eq)).
- It is finite only if it is `Text` or an asset type. Every other builtin type counts as infinite.
- Its representation is never part of the `.kagari` file, so it can change between engine versions
  without breaking games.

Only `std` may declare builtin types (`E1008`). Most of them are in the prelude; the screen state
types (`Entity`, `Prop<T>`, the layers, …) are not, because you receive them as `&mut` references
from `Screen` methods and rarely name them.

### `Text`

`Text` is constant text: plain (`"Mio"`) or rich (built by [`t![…]`](../syntax/forms.md), with
ruby, text effects and interpolation). A string literal has type `Text`, not `String`.

```kis
const NAME: Text = "Mio";
```

- **Finite.** The values of `Text` in the story are the constant texts of the program, so a story
  variable, parameter or return value of a `pushdown fn` may be `Text`.
- **Not in entry scenes.** `fn` code can build new text at runtime with `to_text`, so the
  parameter and return types of an [entry scene](../concepts/entry.md) must not contain `Text`
  (`E3009`).
- **Interpolation.** `{expr}` inside `t![…]` is a call of `to_text(expr)`, which needs
  `expr: T` with [`T: ToText`](./traits.md#totext). In a `pushdown fn`, `to_text` of a finite
  runtime value is tabulated like any `const fn` call, so the line stays a finite function of the
  story state.
- `Eq`: yes.

### `String`

`String` is a runtime string for `fn` code. It is not finite and is `Eq` and `ToText`.

*Tentative.* The current `std` declares no function that creates or changes a `String`; text a
game shows is `Text`.

### Asset types {#asset-types}

`ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset` and `FontAsset` name files packed with the
game, and so do the six shader types (`ColorShader<A>` and the others), which name WGSL files
(`.wgsl`) of one role with arguments of type `A`. A value is created only by a constant call of
`asset` or `system_font`:

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const THEME: AudioAsset = asset("bgm/theme.ogg");
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

- The type argument of `asset::<A>` is inferred from the constant's type and must implement
  [`Asset`](./traits.md#asset).
- The argument must be a string literal (`E2028`), not empty (`E1028`), inside the asset
  directory (`E1029`) and outside its reserved top-level directories `system-fonts` and `std`
  (`E1032`); a missing file is `E1027`. `system_font` has its own rules (`E1030`, `E1031`); see
  the STD Reference, [`std`](../../std/std.md).
- **Finite.** The values of a file asset type are exactly the asset constants of the program, so
  the story may hold them, and a misspelled asset is a name-resolution error instead of a runtime
  failure. The shader types are not finite; a shader is used by renders, not by the story.
- `Eq`: yes.
- `ShaderAsset`, the role-less identity of a shader file that effect values hold, is not `Asset`
  and not in the prelude; games do not name it. See
  [`std::screen::shader`](../../std/screen-shader.md).

### `Setting<T>` and `Meta<T>` {#setting-and-meta}

A setting and a meta field are persistent global values stored outside saves. Each is declared by
an immutable `static` that holds a **handle**:

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- An immutable `static` must hold a `Setting<T>` or a `Meta<T>` (`E3011`), and `setting(…)` and
  `meta(…)` may be called only as the initializer of a `static` (`E4007`). The initializer gives
  the value used when the stored file has none.
- The handle itself is a value: it can be passed to functions and stored in records such as
  `Settings`. Copying it copies the handle; both copies read and write the same stored value.
- **Kinds.** `get` and `set` are `fn`. `Meta::write` is a `pushdown fn`: the story writes meta and
  never reads it. A `const fn` may use a `Setting` handle only as a value, as when passing it to
  `Entity::gate`; calling `get` there is `E3001`. `const` code may not mention a meta field at all
  (`E3013`).
- Not finite and not `Eq`.

See [The Game System](../concepts/system.md) and
[Settings in renders](../concepts/presentation.md#settings-in-renders).

### `Narrative`

`Narrative` is the system's only view of a running story. The engine passes one to the game view
(`System::game`, a `fn(Narrative) -> Element`) and to game overlays (`fn(Narrative) -> GameOverlay`),
and nowhere else. Its representation is hidden, so system code cannot read story variables. All
its methods are `fn`, and its reads are reactive. A copy names the same running game. Not finite,
not `Eq`.

### `Screen` and its parts {#screen}

`Screen` is the screen a step shows together with the step's timeline. It is the input and output
of every [render](#render). All its methods are `const fn`.

- **Value.** A render receives a copy of the previous screen and returns a new one; changing the
  copy never changes the caller's screen.
- **Parts.** `Entity`, `Prop<T>`, `ImageLayer`, `LabelLayer`, `MovieLayer`, `Hud`, `ChoiceMenu`,
  `Camera` and `Channel` are parts of a screen. You get them as `&mut` references from methods such
  as `screen.entity(id)` or `screen.music()`; a change through such a reference changes that screen.
- **Pending changes.** `Change<T>`, `Swap` and `ChannelChange` are pending changes returned by
  `change`, `clear` and `stop`. They are [`#[must_use]`](./attributes.md#must_use): nothing happens
  until `apply_animation` is called, and a pending change that is dropped is warning `W3023`.
- `Screen` is `Eq`, so the compiler can compare screens for
  [consistency](../concepts/presentation.md#consistency). The parts are not `Eq`. None of them is
  finite.

### `Element` and `Placed` {#element}

`Element` is a UI element tree, laid out by the engine; `Placed` is an element positioned inside a
`layer![…]`. Both are built by the UI forms and `std::ui` functions, which are all `fn`. Not finite,
not `Eq`.

### `Arm` and `Divergence`

`Arm` is a selectable arm of the current choice, read from `Narrative::menu` and passed back to
`Narrative::select`. `Divergence` is the error of `nav::load` when a save no longer fits the
program. Both exist only in `fn` code. Not finite, not `Eq`.

## Numbers

| Type | Defined as | Finite | Notes |
| ---- | ---------- | ------ | ----- |
| `Int<lo, hi>` | language primitive | yes | The integers `lo..=hi`; `lo` and `hi` are constant `i64` values with `lo <= hi` (`E2004`). The only integer type the story may hold at runtime |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | language primitives | no | Rust's integers with Rust's operators |
| `Decimal` | `#[builtin]` struct in `std` | no | Exact decimal; a literal with a decimal point (`0.5`) is a `Decimal`. The replacement for floats |
| `Rat` | `#[builtin]` struct in `std` | no | Exact rational; no literal, written `Rat::new(num, den)` |

All of them are `Eq`; integers, `Rat` and `Decimal` are ordered (`<`, `<=`, …). Integer types never
mix in arithmetic (`E2014`) and convert only through the [`Integer`](./traits.md#integer) methods
`into` and `saturate`; there are no `as` casts (`E2019`). Arithmetic, literal typing, saturation and
overflow are described in [Numbers](../syntax/numbers.md).

## `Vec<T>` {#vec}

`Vec<T>` is a growable list for `fn` and `const fn` code. It is a value like every type: `push` and
`set` change the place they are called on, and copies are unaffected.

- `v[i]` indexes it with any integer type, and `for x in v` iterates it; both are built into the
  language for `Vec<T>` and arrays.
- `get` out of range returns `None`; `set` out of range is an evaluation error (`E4005` during
  compile-time evaluation).
- Not finite. `Eq` when `T: Eq`.

## `Range<T>` {#range}

`Range<T>` is an ordinary `std` struct with public fields `start`, `end` and `inclusive`. The
compiler knows it in two places:

- **Range expressions.** `a..b` and `a..=b` build a `Range<T>`, where `T` is a type both bounds
  convert to: `0..7` is a `Range<i32>` unless its use expects another integer type, and with
  `last: Int<0, 7>`, `0..last` is a `Range<Int<0, 7>>`. Only `a..b` and `a..=b` are range values;
  other forms such as `a..`, `..b` or `..` are `E2030`. Patterns have their own range forms (`a..`,
  `..=b`, `a..b`, `a..=b`); see [Range patterns](../syntax/patterns.md#range-patterns).
- **`for` loops.** `for` iterates a `Range<T>` whose `T` is an integer type (`E2031` otherwise). In
  a `pushdown fn`, the bounds must be constant expressions (`E3006`).

## `Option<T>` and `Result<T, E>` {#option-and-result}

Both are ordinary generic enums of `std`, and the prelude imports their variants `None`, `Some`,
`Ok` and `Err`. The compiler knows them for the `?` operator: on an `Option` it needs a function
that returns `Option`, and on a `Result` one that returns `Result` with the same error type
(`E2034`). There is no error conversion. They are finite and `Eq` when their type arguments are.

## Function pointer and closure types {#function-types}

Function types carry the [kind](../concepts/kinds.md) of the code they call, so the call rules hold
through pointers and closures too.

| Type | Holds | Finite | Closure counterpart |
| ---- | ----- | ------ | ------------------- |
| `fn(A) -> B` | a named `fn` | no | `Fn(A) -> B` |
| `const fn(A) -> B` | a named `const fn`, or a closure that captures nothing | yes | `const Fn(A) -> B` |
| `pushdown fn(A) -> B` | a named instant procedure | yes | `pushdown Fn(A) -> B` |
| `scene fn(A) -> B` | a named scene | yes | none |
| `Fn(A) -> B`, `const Fn(A) -> B`, `pushdown Fn(A) -> B` | a closure or function of that kind | no | — |

- A named function coerces to a pointer or closure type of exactly its kind. A closure that captures
  nothing coerces to a pointer of its kind; a capturing one cannot (`E2036`). A pointer coerces to
  the closure type of the same kind.
- `pushdown` and `scene` are keywords only directly before `fn` (and `pushdown` before `Fn`).
- **Finite.** A pointer of a story or `const` kind is finite, because its values are the finitely
  many named functions of the program. Closures are never finite; in the story they are
  [second-class](../syntax/values.md).
- **`Eq`.** Function pointers compare by identity. Closures are never `Eq` (`E2017`).

### `Render`

```kis
pub type Render = const fn(Screen) -> Screen;
```

`Render`, in `std::screen` and the prelude, is an alias, not a new type. A render is a `const fn`
from the screen before a step to the screen after it; steps take one as their `render` attribute.
Because it is a `const fn` pointer, `Render` is finite and `Eq`, and a non-capturing closure can be
written inline wherever a `Render` is expected. See
[Render functions](../concepts/presentation.md#render-functions).

```kis
const fn clear_background(mut screen: Screen) -> Screen {
    screen.background().clear().apply_animation(fade(ms(500)));
    screen
}

const CLEAR: Render = clear_background;
```

## `Duration`

```kis
pub struct Duration { pub ms: u32 }
```

`Duration` is an ordinary struct of `std::time`, not a builtin type. It is listed here because
every time in `std` is a `Duration` of whole milliseconds: there are no unit strings and no frame
counts. The prelude has `Duration`, `ms(n)` and `secs(n)`.

- It holds a `u32`, so it is not finite: the story uses durations only in constant expressions,
  such as `fade(ms(500))` inside a render.
- `Eq`: yes, structurally.
- `div` rounds down: `ms(1000).div(3)` is `ms(333)`. A duration that does not fit in `u32`
  milliseconds, whether built by `secs`, by `plus` or `times`, or reached by a screen's timeline,
  is `E4013` during compile-time evaluation.
