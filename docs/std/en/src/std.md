# `std`

> **Stability:** Stable — the builtin types, `Range`, `Option` and `Result`, the std traits, and
> the asset types.

The crate root `std` declares the types every program needs: runtime strings and lists, exact
numbers, ranges, the generic sums `Option` and `Result`, the traits that std signatures are written
with, and the asset types with `asset` and `system_font`. Every function of the root is a
`const fn`.

The root also declares the submodules:

```kis
pub mod prelude;
pub mod time;
pub mod screen;
pub mod stage;
pub mod narrative;
pub mod ui;
pub mod system;
```

`Int<lo, hi>`, the Rust primitive integers (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`) and
`bool` are language types, not std items. `Int<lo, hi>` holds the integers `lo..=hi` and is the
only integer type the story may hold at runtime. See
[Numbers](../../../reference/en/src/syntax/numbers.md).

## Overview

|Item|Kind|Summary|
|---|---|---|
|[`String`](#struct-string)|builtin struct|A runtime string|
|[`Vec<T>`](#struct-vect)|builtin struct|A growable list|
|[`Rat`](#struct-rat)|builtin struct|An exact rational number|
|[`Decimal`](#struct-decimal)|builtin struct|An exact decimal number|
|[`Range<T>`](#struct-ranget)|struct|`start..end` and `start..=end`|
|[`Option<T>`](#enum-optiont)|enum|An optional value|
|[`Result<T, E>`](#enum-resultt-e)|enum|A success value or an error value|
|[`Eq`](#trait-eq)|builtin trait|Structural equality|
|[`Integer`](#trait-integer)|builtin trait|The integer types|
|[`RandChoose`](#trait-randchoose)|builtin trait|Types the story can draw at random|
|[`Asset`](#trait-asset)|builtin trait|The asset types|
|[`ImageAsset` … `ShaderAsset`](#asset-types)|builtin structs|One type per kind of asset file|
|[`asset`](#function-assetpath)|builtin `const fn`|Loads an asset of the program|
|[`system_font`](#function-system_fontpath)|builtin `const fn`|Loads a font installed on the build machine|

## Struct: `String`

```kis
#[builtin] pub struct String;
```

A runtime string, built by `fn` code. String literals have type `Text`, not `String`; `String` is
what `fn` code builds at runtime. `String` is `Eq` and can be interpolated into `t![…]` (it is
`ToText`). It is not finite, so the story cannot hold one.

- Kind: — · Builtin: yes

## Struct: `Vec<T>`

```kis
#[builtin] pub struct Vec<T>;
```

A growable list. `Vec` is a value type like every type: `push` and `set` change the place they are
called on, and copies are unaffected. `Vec<T>` is `Eq` when `T` is. `for x in v` over a `Vec` is a
language rule, not a method (see [Compiler Rules](./appendix/rules.md#for-loops)). `Vec` is not
finite.

- Kind: — · Builtin: yes

### `Vec::new()`

```kis
#[builtin] pub const fn new() -> Vec<T>;
```

- Returns: `Vec<T>` — an empty list.
- Kind: `const fn` · Builtin: yes

### `Vec::len(&self)`

```kis
#[builtin] pub const fn len(&self) -> usize;
```

- Returns: `usize` — the number of elements.
- Kind: `const fn` · Builtin: yes

### `Vec::is_empty(&self)`

```kis
#[builtin] pub const fn is_empty(&self) -> bool;
```

- Returns: `bool` — whether the list has no elements.
- Kind: `const fn` · Builtin: yes

### `Vec::get(&self, index)`

```kis
#[builtin] pub const fn get(&self, index: usize) -> Option<T>;
```

- `index` `usize` — the position, from 0.
- Returns: `Option<T>` — the element at `index`, or `None` when `index` is out of range.
- Kind: `const fn` · Builtin: yes

### `Vec::set(&mut self, index, value)`

```kis
#[builtin] pub const fn set(&mut self, index: usize, value: T);
```

- `index` `usize` — the position to replace.
- `value` `T` — the new element.
- Kind: `const fn` · Builtin: yes

Replaces the element at `index`. An `index` out of range is an evaluation error; in const
evaluation it is
[`E4005`](../../../reference/en/src/diagnostics/eval.md#e4005).

### `Vec::push(&mut self, value)`

```kis
#[builtin] pub const fn push(&mut self, value: T);
```

- `value` `T` — the element to append.
- Kind: `const fn` · Builtin: yes

Appends `value` at the end.

```kis
const fn first_three() -> Vec<u32> {
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v
}
```

## Struct: `Rat`

```kis
#[builtin] pub struct Rat;
```

An exact rational number. `Rat` has no literal; build one with `Rat::new`. `Rat` is `Eq` and
`ToText` (written exactly). It is not finite.

- Kind: — · Builtin: yes

### `Rat::new(num, den)`

```kis
#[builtin] pub const fn new(num: i128, den: i128) -> Rat;
```

- `num` `i128` — the numerator.
- `den` `i128` — the denominator.
- Returns: `Rat` — `num / den` in lowest terms.
- Kind: `const fn` · Builtin: yes

`den == 0` is a division by zero
([`E4002`](../../../reference/en/src/diagnostics/eval.md#e4002) in const evaluation).
`Rat::new(1, 3)` is one third.

### `Rat::to_decimal(self)`

```kis
#[builtin] pub const fn to_decimal(self) -> Decimal;
```

- Returns: `Decimal` — this value with the integer part and at most 7 digits after the point,
  truncated toward zero.
- Kind: `const fn` · Builtin: yes

`Rat::new(1, 3).to_decimal()` is `0.3333333`.

## Struct: `Decimal`

```kis
#[builtin] pub struct Decimal;
```

An exact decimal number, written as a literal like `0.5`: the exact replacement for floats (Kagari
has no floating-point type) and the type of every fraction in `std`. `Decimal / Decimal` is exact,
then truncated toward zero to 7 digits after the point. `Decimal` is `Eq` and `ToText` (written
exactly). It is not finite.

- Kind: — · Builtin: yes

## Struct: `Range<T>`

```kis
pub struct Range<T> {
    pub start: T,
    pub end: T,
    pub inclusive: bool,
}
```

- `start` `T` — the first value.
- `end` `T` — the end bound.
- `inclusive` `bool` — `false` for `start..end` (end excluded), `true` for `start..=end`.
- Kind: — · Builtin: no

`a..b` and `a..=b` are `Range` values. Both bounds are required in expressions; `a..`, `..b` and
`..` exist only in patterns. `0..7` is a `Range<Int<0, 7>>`. See
[Compiler Rules](./appendix/rules.md#type-rules) for the typing and
[`for` loops](./appendix/rules.md#for-loops) for iteration.

## Enum: `Option<T>`

```kis
pub enum Option<T> {
    None,
    Some(T),
}
```

An optional value. The prelude imports the type and its variants `None` and `Some`.

- Kind: — · Builtin: no

## Enum: `Result<T, E>`

```kis
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

A success value or an error value. The prelude imports the type and its variants `Ok` and `Err`.
`nav::load` returns a `Result`.

- Kind: — · Builtin: no

## Trait: `Eq`

```kis
#[builtin] pub trait Eq {}
impl Eq for String {}
impl<T: Eq> Eq for Vec<T> {}
impl Eq for Rat {}
impl Eq for Decimal {}
impl Eq for ImageAsset {}
impl Eq for AudioAsset {}
impl Eq for VoiceAsset {}
impl Eq for VideoAsset {}
impl Eq for FontAsset {}
impl Eq for ShaderAsset {}
impl Eq for narrative::Text {}
impl Eq for screen::Screen {}
```

Types whose values compare with `==` and `!=`; the bound of `ui::select`.

- Kind: — · Builtin: yes

`Eq` is automatic for the primitive types, `Int`, function pointers, and structs, enums, tuples
and arrays whose fields are all `Eq`. A `#[builtin]` type has no visible fields, so `std` states it
explicitly with the `impl`s above: `String`, `Vec<T>` of an `Eq` `T`, `Rat`, `Decimal`, the asset
types, `Text` and `Screen` are `Eq`. Handles and pending changes (`Setting`, `Meta`, `Narrative`,
`Entity`, `Prop`, `Change`, the layers, `Swap`, `Channel`, `ChannelChange`, `Element`, …) and
closures are not. Comparing values of a non-`Eq` type is
[`E2017`](../../../reference/en/src/diagnostics/types.md#e2017).

## Trait: `Integer`

```kis
#[builtin] pub trait Integer {
    const fn into<T: Integer>(self) -> T;
    const fn saturate<T: Integer>(self) -> T;
}
impl Integer for u8 {}
impl Integer for u16 {}
impl Integer for u32 {}
impl Integer for u64 {}
impl Integer for u128 {}
impl Integer for i8 {}
impl Integer for i16 {}
impl Integer for i32 {}
impl Integer for i64 {}
impl Integer for i128 {}
impl Integer for usize {}
impl Integer for isize {}
impl<const LO: i64, const HI: i64> Integer for Int<LO, HI> {}
```

The integer types: `Int<lo, hi>` and Rust's integers.

- Kind: — · Builtin: yes

### `Integer::into(self)`

```kis
const fn into<T: Integer>(self) -> T;
```

- Returns: `T` — this value as `T`.
- Kind: `const fn` · Builtin: yes

Lossless conversion. It is a compile error
([`E2018`](../../../reference/en/src/diagnostics/types.md#e2018)) unless every value of `Self`
fits in `T`: `Int<0, 5>` converts to `u8` or `Int<0, 10>`, but `u8` into `Int<0, 5>` does not
compile.

### `Integer::saturate(self)`

```kis
const fn saturate<T: Integer>(self) -> T;
```

- Returns: `T` — this value clamped to the range of `T`.
- Kind: `const fn` · Builtin: yes

`saturate` has no fitting condition: `300u32.saturate()` into `u8` is `255`.

```kis
const fn level(n: u32) -> Int<0, 10> {
    n.saturate()
}
```

## Trait: `RandChoose`

```kis
#[builtin] pub trait RandChoose {}
impl<const LO: i64, const HI: i64> RandChoose for Int<LO, HI> {}
```

Types a story can draw at random with
[`narrative::rng_choose`](./narrative.md#function-rng_choose): fieldless enums (implicitly) and
`Int<lo, hi>`.

- Kind: — · Builtin: yes

## Trait: `Asset`

```kis
#[builtin] pub trait Asset {}
impl Asset for ImageAsset {}
impl Asset for AudioAsset {}
impl Asset for VoiceAsset {}
impl Asset for VideoAsset {}
impl Asset for FontAsset {}
impl Asset for ShaderAsset {}
```

Types whose values are exactly the asset constants of the program; the bound of
[`asset`](#function-assetpath).

- Kind: — · Builtin: yes

Every trait of `std` is `#[builtin]`, and only `std` declares or implements traits. A `trait` item
or an `impl Trait for T` in a game is an error; see
[Compiler Rules](./appendix/rules.md#traits).

## Asset types

```kis
#[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
#[builtin] pub struct VoiceAsset;
#[builtin] pub struct VideoAsset;
#[builtin] pub struct FontAsset;
#[builtin] pub struct ShaderAsset;
```

One opaque type per kind of asset file. All six are `Eq` and `Asset`, and all are finite: the
values of an asset type are exactly its constants.

- Kind: — · Builtin: yes

|Type|File|Used by|
|---|---|---|
|`ImageAsset`|An image: PNG, JPEG or WebP|image layers, rule masks, the side image, UI images and paints|
|`AudioAsset`|A music or sound file|music and ambience loops, one-shot sounds|
|`VoiceAsset`|A voice file, bound to a line|`Say::voice`, `Narrative::replay_voice`|
|`VideoAsset`|A movie file|movie layers|
|`FontAsset`|A font file|`System::font`, `TextEffect::font`, `TextStyle::font`|
|`ShaderAsset`|A WGSL shader file (`.wgsl`): a filter, or generated content such as weather or particles|`Screen::filter`, `Entity::filter`, `Entity::shader`|

Which file formats a runtime decodes is listed in [Runtime Profiles](./appendix/profiles.md).

- `VideoAsset` — Profile: not in `native_v0`.
- `ShaderAsset` — Profile: not in `native_v0`.

## Function: `asset(path)`

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

- `path` `Text` — a string literal, relative to the program's asset directory.
- Returns: `A` — the asset.
- Kind: `const fn` · Builtin: yes

An asset is declared as a constant:

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
const THEME: AudioAsset = asset("bgm/theme.ogg");
```

- The type parameter `A` is inferred from the constant's type and must be an asset type
  ([`E2026`](../../../reference/en/src/diagnostics/types.md#e2026) otherwise).
- The argument must be a string literal
  ([`E2028`](../../../reference/en/src/diagnostics/types.md#e2028)) and must not be empty
  ([`E1028`](../../../reference/en/src/diagnostics/modules.md#e1028)).
- The path is relative to the asset directory (`[assets] dir` in `kagari.toml`, by default
  `assets`; without a manifest, the root file's directory) and must not leave it
  ([`E1029`](../../../reference/en/src/diagnostics/modules.md#e1029)).
- A missing file is a compile error
  ([`E1027`](../../../reference/en/src/diagnostics/modules.md#e1027)). Because faces, tracks and
  voices are constants, a misspelled name is a resolve error, not a runtime failure.
- `kagari build` checks the format of every asset the program uses against the project's runtime
  profiles; see [Runtime Profiles](./appendix/profiles.md).

## Function: `system_font(path)`

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

- `path` `Text` — a string literal, relative to one of the system font directories.
- Returns: `FontAsset` — the font, packed with the game.
- Kind: `const fn` · Builtin: yes

The standard library ships no fonts, and `System::font` must name one. A font installed on the
machine that builds the game can be named directly:

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
const SANS: FontAsset = system_font("truetype/dejavu/DejaVuSans.ttf");
```

- The directories are searched in order and the first that has the file wins:
  - Windows: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, then `%WINDIR%\Fonts`;
  - macOS: `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts`;
  - elsewhere: `$XDG_DATA_HOME/fonts`, `~/.fonts`, then `fonts` in each of `$XDG_DATA_DIRS`.
- A font found in none of them is
  [`E1030`](../../../reference/en/src/diagnostics/modules.md#e1030).
- `[assets] system-fonts = false` in `kagari.toml` forbids system fonts; any `system_font` is then
  [`E1031`](../../../reference/en/src/diagnostics/modules.md#e1031). System fonts are allowed by
  default.
- `kagari build` packs a system font like any other asset, into `system-fonts.kiarchive`, under
  the path `system-fonts/` followed by the path written in the source. The top-level
  `system-fonts` directory of the asset directory is therefore reserved
  ([`E1032`](../../../reference/en/src/diagnostics/modules.md#e1032)).
- A system font is a `FontAsset` constant like one from `asset("…")`.
