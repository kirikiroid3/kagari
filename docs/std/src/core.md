# Core Types

The crate root `std` declares the builtin types, the generic sums `Option` and `Result`, the
traits, and the asset types. `std::time` declares `Duration`.

## Builtin types

```kis
#[builtin] pub struct Int<const LO: i128, const HI: i128>;
#[builtin] pub struct String;
#[builtin] pub struct Vec<T>;
#[builtin] pub struct Rat;
#[builtin] pub struct Decimal;
```

|Type|Values|
|---|---|
|`Int<LO, HI>`|The integers `LO..=HI`; the finite integer type of the story|
|`String`|Runtime strings, for `fn` code|
|`Vec<T>`|Growable sequences|
|`Rat`|Exact rationals|
|`Decimal`|Exact decimals, written as literals like `0.5`; the type of every fraction in `std`|

The Rust primitive integers (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`) and `bool` are
language types, not std items. The syntax reference describes the numeric types and their
arithmetic.

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
|`Asset`|`std`|Types whose values are exactly the asset constants of the program|
|`Step`|`std::narrative`|Types that `push_step` can suspend on; see [Narrative](./narrative.md)|
|`ToText`|`std::narrative::text`|Types a `{expr}` item of `t![…]` may interpolate; see [Text](./text.md#interpolation)|

```kis
pub trait Eq {}
/// Types whose values are exactly the asset constants of the program.
pub trait Asset {}
```

The std traits exist so that std signatures type-check without overloading or coercion. Users
never implement them; user traits are an open question of the language.

## Assets

```kis
#[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
#[builtin] pub struct VoiceAsset;
#[builtin] pub struct VideoAsset;
#[builtin] pub struct FontAsset;
impl Asset for ImageAsset {}
impl Asset for AudioAsset {}
impl Asset for VoiceAsset {}
impl Asset for VideoAsset {}
impl Asset for FontAsset {}

#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

|Type|Used by|
|---|---|
|`ImageAsset`|image layers, rule masks, the side image, UI images and paints|
|`AudioAsset`|music and ambience loops, one-shot sounds|
|`VoiceAsset`|`Say::voice`|
|`VideoAsset`|movie layers|
|`FontAsset`|`TextEffect::font`, `TextStyle::font`|

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
