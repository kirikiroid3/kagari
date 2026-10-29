---
title: "Numbers"
---

There are two families of numbers. **Bounded integers** `Int<lo, hi>` are the only numbers the
story may hold at runtime. The Turing-complete kinds also have **Rust's integers** and two exact
non-integer types, `Decimal` and `Rat`. There are no floats: a float is not `Eq` (`NaN != NaN`),
and screens must be compared for equality. `Decimal` takes the place of floats, and `Rat` covers
exact fractions.

| Type | Values | Held at runtime by |
| ---- | ------ | ------------------ |
| `Int<lo, hi>` | the integers from `lo` to `hi`, inclusive | every kind |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | as in Rust | `fn`, `const fn` |
| `Decimal` | exact decimal numbers | `fn`, `const fn` |
| `Rat` | exact fractions | `fn`, `const fn` |

A constant expression of any of these types may still appear in a `pushdown fn`, for example
`fade(ms(500))` inside a render, because its value is computed at compile time. What a
`pushdown fn` may not do is hold one at runtime: a local, parameter, return value or story
variable of such a type is an error (see [Finite types](../concepts/kinds.md#finite-types)).

## `Int<lo, hi>` {#intlo-hi}

```kis
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

`Int<lo, hi>` is a primitive type: an integer in a declared, inclusive range.

- `lo` and `hi` are [type-level constants](./types.md#type-level-constants) that fit in `i64`,
  with `lo <= hi` ([`E2004`](../../diagnostics/types.md#e2004)).
- There is no default range: every `Int` type states its bounds.
- The analyzer stores a story variable in as many bits as its range needs, so a counter from 0 to
  5 costs 3 bits. Use the smallest range that fits.

Patterns match `Int` values with literals and ranges, which is how thresholds are written:

```kis
match AFFECTION_MIO {
    3..=5 => MioSmiles(),
    _ => MioShrugs(),
}
```

## Integer literals

An integer literal without a suffix has the flexible type "the integer `n`". It takes the integer
type its context expects, `Int<lo, hi>` or a Rust integer, as long as that type's range contains
`n`; otherwise it is [`E2013`](../../diagnostics/types.md#e2013). When nothing determines the type,
the literal is `Int<n, n>`. A suffix fixes the type: `5u8` is a `u8`. Integer literals never
become `Decimal` or `Rat`: write `1.0`, not `1`.

```kis
let a: Int<0, 5> = 3;     // Int<0, 5>
let b: u32 = 3;           // u32
let c = 3u8;              // u8
let d: Int<0, 5> = 9;     // E2013: 9 does not fit
```

Operations on literals alone are computed on the literal ranges: `2 * 3` is "the integer 6". The
compiler represents integers with 128 bits, so a `u128` value above `i128::MAX` cannot be written
as a literal.

## Arithmetic on `Int` {#arithmetic-on-int}

*Tentative.* Arithmetic on bounded integers is exact: the result type is the range of all possible
results, so it never overflows at runtime.

| Expression (`a: Int<l1, h1>`, `b: Int<l2, h2>`) | Type |
| ----------------------------------------------- | ---- |
| `a + b` | `Int<l1 + l2, h1 + h2>` |
| `a - b` | `Int<l1 - h2, h1 - l2>` |
| `a * b` | the smallest range containing the four corner products |
| `a / b` | the smallest range containing the four corner quotients |
| `a % b` | a range bounded by the dividend and by the largest divisor magnitude minus one |
| `-a` | `Int<-h1, -l1>` |
| `a == b`, `a < b`, … | `bool` |

- Division truncates toward zero, and `%` takes the sign of the dividend, as in Rust.
- `/` and `%` need a divisor whose range does not contain 0
  ([`E2016`](../../diagnostics/types.md#e2016)): narrow the type first, for example with a `match`.
- The bitwise operators `& | ^ << >>` and `!` are not defined on `Int`
  ([`E2015`](../../diagnostics/types.md#e2015)).
- `Int` and a Rust integer cannot be mixed in one operation or comparison
  ([`E2014`](../../diagnostics/types.md#e2014)); convert one side first.
- A result range that does not fit in 128 bits is [`E2015`](../../diagnostics/types.md#e2015).

### Widening and compound assignment {#widening-and-compound-assignment}

A value converts implicitly to an `Int` type whose range contains its range: an `Int<1, 6>` can be
stored in an `Int<0, 10>`. To store a value whose range is wider, convert it with `.saturate()`
(see [Conversions](#conversions)).

**Compound assignment saturates.** `x += e`, `x -= e`, `x *= e`, `x /= e` and `x %= e` on an
`Int` variable compute the exact result and clamp it to the bounds of `x`: `x += 1` is
`x = (x + 1).saturate()`. The right side may be any `Int`. Saturation keeps every step total: it
is defined behaviour, never a runtime error. `/=` and `%=` still need a divisor range without 0,
and the bitwise compound assignments are not defined on `Int`.

```kis
static mut AFFECTION_MIO: Int<0, 5> = 0;

pushdown fn praise() {
    AFFECTION_MIO += 1;     // stays 5 once it is 5
}
```

A counter that silently saturates is often a sign that its range is too small.

## Conversions

Apart from literals and the widening of one `Int` type to another, integer types never convert
implicitly. The `Integer` trait of the standard library, implemented by `Int<lo, hi>` and every
Rust integer, gives two explicit conversions. The target type is inferred from context, or written
with a turbofish (`x.into::<u32>()`):

| Method | Result |
| ------ | ------ |
| `x.into()` | The same value. A compile error unless every value of the source type fits in the target type ([`E2018`](../../diagnostics/types.md#e2018)) |
| `x.saturate()` | The value clamped to the range of the target type |

```kis
const fn label_width(chars: Int<0, 40>) -> u32 {
    let n: u32 = chars.into();      // every Int<0, 40> fits in u32
    n * 16
}

fn set_level(level: u32) {
    let level: Int<0, 100> = level.saturate();
    MUSIC.set(volume(level));
}
```

There are no `as` casts ([`E2019`](../../diagnostics/types.md#e2019)). There is no conversion
between integers and `Decimal`; `Rat::new` builds a `Rat` from two `i128`, and `Rat::to_decimal`
converts a `Rat` to a `Decimal`.

## Rust integers

The Turing-complete kinds have Rust's integer types: `u8`, `u16`, `u32`, `u64`, `u128`, `i8`,
`i16`, `i32`, `i64`, `i128`, `usize` and `isize`. `usize` and `isize` have 64 bits on every
platform.

| Operators | Meaning |
| --------- | ------- |
| `+ - * / %` | arithmetic; both operands have the same type |
| `& \| ^` | bitwise and, or, exclusive or |
| `<< >>` | shifts; the amount may have any integer type |
| `-x` | negation, signed types only ([`E2015`](../../diagnostics/types.md#e2015)) |
| `!x` | bitwise not |
| `== != < <= > >=` | comparison |

- Arithmetic is checked. Overflow, division by zero and a result outside the type are errors
  (see [Errors](#errors-at-compile-time-and-at-runtime)); nothing wraps silently.
- `/` truncates toward zero, `%` takes the sign of the dividend.
- A shift amount must be smaller than the width of the type. `<<` drops the bits shifted out;
  `>>` is arithmetic for signed types.
- Two different Rust integer types cannot be mixed; convert with `.into()` or `.saturate()`.

## `Decimal` {#decimal}

`Decimal` is an exact decimal number, for values that are naturally written in base ten, such as
opacities, volumes or positions in a layout. A literal with a decimal point, such as `0.5` or
`1.0`, is a `Decimal`. It replaces floats and is `Eq`: `0.1 + 0.2 == 0.3` is `true`.

| Operators | Meaning |
| --------- | ------- |
| `+ - *` | exact |
| `/` | computed exactly, then cut to at most 7 digits after the point, truncating toward zero: `1.0 / 3.0` is `0.3333333` |
| `-x` | negation |
| `== != < <= > >=` | comparison |

`%` is not supported on `Decimal`. A `Decimal` holds a 128-bit significand; a result that does not
fit is an overflow error.

## `Rat` {#rat}

`Rat` is an exact rational number: a numerator and a positive denominator in lowest terms, both
`i128`. There is no `Rat` literal. A value is written `Rat::new(num, den)`, which reduces the
fraction (`Rat::new(2, -4)` is `-1/2`); `den == 0` is a division by zero.

It has the same operators as `Decimal`, and `/` is exact. `Rat::to_decimal` converts to a
`Decimal`: it keeps the integer part and at most 7 digits after the point, truncating toward zero
(`Rat::new(1, 3).to_decimal()` is `0.3333333`, `Rat::new(-2, 3).to_decimal()` is `-0.6666666`).

```kis
const fn rats() -> Rat {
    let a = Rat::new(1, 3);
    a * a - a / Rat::new(2, 1)      // 1/9 - 1/6 = -1/18
}
```

## Errors at compile time and at runtime {#errors-at-compile-time-and-at-runtime}

| Error | During compile-time evaluation | At runtime |
| ----- | ------------------------------ | ---------- |
| Overflow, result outside the type, bad shift amount | [`E4001`](../../diagnostics/eval.md#e4001) | VM panic |
| Division by zero, `Rat::new(_, 0)` | [`E4002`](../../diagnostics/eval.md#e4002) | VM panic |

A VM panic is a defined stop of the game, handled by the [runtime](../tools/runtime.md). Code
evaluated at compile time (constants, renders, `const fn` called from the story) reports these
errors as diagnostics at the call that needed the value, so the story itself never fails this
way. Compound assignment on `Int<lo, hi>` saturates and never fails.
