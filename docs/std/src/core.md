# Core Types

The crate root `std` declares the builtin types, `Range`, the generic sums `Option` and `Result`,
the traits, and the asset types. `std::time` declares `Duration`.

## Builtin types

```kis
#[builtin] pub struct String;
#[builtin] pub struct Vec<T>;
#[builtin] pub struct Rat;
#[builtin] pub struct Decimal;
```

|Type|Values|
|---|---|
|`String`|Runtime strings, for `fn` code|
|`Vec<T>`|Growable sequences|
|`Rat`|Exact rationals|
|`Decimal`|Exact decimals, written as literals like `0.5`; the replacement for floats and the type of every fraction in `std`|

```kis
impl Rat {
    /// `num / den` in lowest terms; `den == 0` is a division by zero.
    #[builtin] pub const fn new(num: i128, den: i128) -> Rat;
    /// This value as a `Decimal`: the integer part and at most 7 digits after the point,
    /// truncated toward zero.
    #[builtin] pub const fn to_decimal(self) -> Decimal;
}
```

`Rat` has no literal: `Rat::new(1, 3)` is one third. `Rat::new(1, 3).to_decimal()` is `0.3333333`.
`Decimal / Decimal` is exact, then truncated to 7 digits after the point the same way.

`Int<lo, hi>`, the Rust primitive integers (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`) and
`bool` are language types, not std items. `Int<lo, hi>` holds the integers `lo..=hi` and is the
only integer type the story may hold at runtime. The syntax reference describes the numeric types
and their arithmetic.

## `Range`

```kis
pub struct Range<T> {
    pub start: T,
    pub end: T,
    pub inclusive: bool,
}
```

`start..end` is a `Range` with `inclusive` false, `start..=end` one with `inclusive` true. `for`
over a range is a language rule (see [Checks](./checks.md#for-loops)).

## `Vec`

```kis
impl<T> Vec<T> {
    #[builtin] pub const fn new() -> Vec<T>;
    #[builtin] pub const fn len(&self) -> usize;
    #[builtin] pub const fn is_empty(&self) -> bool;
    #[builtin] pub const fn get(&self, index: usize) -> Option<T>;
    #[builtin] pub const fn set(&mut self, index: usize, value: T);
    #[builtin] pub const fn push(&mut self, value: T);
}
```

- `get` out of range returns `None`.
- `set` out of range is an evaluation error; in const evaluation it is a compile error.
- `for x in v` over a `Vec` is a language rule, not a method (see [Checks](./checks.md#for-loops)).
- `Vec` is a value type like every type: `push` and `set` change the place they are called on, and
  copies are unaffected.

## `Option` and `Result`

```kis
pub enum Option<T> { None, Some(T) }
pub enum Result<T, E> { Ok(T), Err(E) }
```

The prelude also imports the variants: `None`, `Some`, `Ok`, `Err`.

## Traits

|Trait|Declared in|Meaning|
|---|---|---|
|`Eq`|`std`|Types whose values compare with `==`; the bound of `ui::select`|
|`Integer`|`std`|`Int<lo, hi>` and the Rust integers; the conversions `into` and `saturate`|
|`Asset`|`std`|Types whose values are exactly the asset constants of the program|
|`Step`|`std::narrative`|Types that `push_step` can suspend on; see [Narrative](./narrative.md)|
|`ToText`|`std::narrative::text`|Types a `{expr}` item of `t![…]` may interpolate; see [Text](./text.md#interpolation)|
|`RandChoose`|`std`|Types `rng_choose` can draw: fieldless enums (implicitly) and `Int<lo, hi>`; see [Narrative](./narrative.md#rng_choose)|

```kis
/// Types with structural equality.
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

/// The integer types: `Int<lo, hi>` and Rust's integers.
#[builtin] pub trait Integer {
    /// This value as `T`; a compile error unless every value of `Self` fits in `T`.
    const fn into<T: Integer>(self) -> T;
    /// This value clamped to the range of `T`.
    const fn saturate<T: Integer>(self) -> T;
}
/// Types a story can draw at random: fieldless enums and `Int<lo, hi>`.
#[builtin] pub trait RandChoose {}
impl<const LO: i64, const HI: i64> RandChoose for Int<LO, HI> {}
/// Types whose values are exactly the asset constants of the program.
#[builtin] pub trait Asset {}
```

Every trait is `#[builtin]`, and only `std` declares or implements traits. The std traits exist so
that std signatures type-check without overloading or coercion.

`Eq` is automatic for structs, enums, tuples and arrays whose fields are all `Eq`, and for the
language's primitive types. A `#[builtin]` type has no visible fields, so `std` states it
explicitly: `String`, `Vec<T>` of an `Eq` `T`, `Rat`, `Decimal`, the asset types, `Text` and
`Screen` are `Eq`. Handles and pending changes (`Setting`, `Meta`, `Narrative`, `Entity`, `Prop`,
`Change`, the layers, `Swap`, `Channel`, `ChannelChange`, `Element`, …) are not.

`into` is lossless: `Int<0, 5>` converts to `u8` or `Int<0, 10>`, but `u8` into `Int<0, 5>` is a
compile error. `saturate` clamps, so `300u32.saturate()` into `u8` is `255`.

## Assets

```kis
#[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
#[builtin] pub struct VoiceAsset;
#[builtin] pub struct VideoAsset;
#[builtin] pub struct FontAsset;
/// A WGSL shader file (`.wgsl`): a filter, or generated content such as weather or particles.
#[builtin] pub struct ShaderAsset;
impl Asset for ImageAsset {}
impl Asset for AudioAsset {}
impl Asset for VoiceAsset {}
impl Asset for VideoAsset {}
impl Asset for FontAsset {}
impl Asset for ShaderAsset {}

#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

|Type|Used by|
|---|---|
|`ImageAsset`|image layers, rule masks, the side image, UI images and paints|
|`AudioAsset`|music and ambience loops, one-shot sounds|
|`VoiceAsset`|`Say::voice`|
|`VideoAsset`|movie layers|
|`FontAsset`|`TextEffect::font`, `TextStyle::font`|
|`ShaderAsset`|`Screen::filter`, `Entity::filter`, `Entity::shader` (`.wgsl` files)|

An asset is declared as a constant:

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
const THEME: AudioAsset = asset("bgm/theme.ogg");
```

- The type parameter is inferred from the constant's type.
- The argument must be a string literal. The path is relative to the file that declares the
  constant.
- A missing file is a compile error. Because faces, tracks and voices are constants, a misspelled
  name is a resolve error, not a runtime failure.
- The values of an asset type are exactly its constants, so every asset type is finite.

## Time

```kis
pub struct Duration { pub ms: u32 }
pub const ZERO: Duration = Duration { ms: 0 };
pub const fn ms(n: u32) -> Duration { Duration { ms: n } }
pub const fn secs(n: u32) -> Duration { Duration { ms: n * 1000 } }
impl Duration {
    pub const fn plus(self, other: Duration) -> Duration { Duration { ms: self.ms + other.ms } }
    pub const fn times(self, n: u32) -> Duration { Duration { ms: self.ms * n } }
    pub const fn div(self, n: u32) -> Duration { Duration { ms: self.ms / n } }
}
```

- Every time in `std` is a `Duration` of whole milliseconds. There are no unit strings and no
  frame counts.
- `div` truncates: `ms(1000).div(3)` is `ms(333)`.
- Overflow or underflow of a `Duration` is an evaluation error; in const evaluation it is a
  compile error.
