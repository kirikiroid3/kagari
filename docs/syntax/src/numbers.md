# Numbers

There are two families of numbers. Bounded integers are the only numbers the story may hold at
runtime; the Turing-complete kinds also have Rust's integers and two exact non-integer types.
There are no floats: a float is not `Eq` (`NaN != NaN`), and screens must be compared for
equality. `Decimal` takes the place of floats, and `Rat` covers exact fractions.

| Type | Values | Allowed in |
| ---- | ------ | ---------- |
| `Int<lo, hi>` | integers from `lo` to `hi`, inclusive | every kind |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | as in Rust | `fn`, `const fn`; constants anywhere |
| `Rat` | exact rationals | `fn`, `const fn`; constants anywhere |
| `Decimal` | exact decimals | `fn`, `const fn`; constants anywhere |

"Constants anywhere" means a constant expression of these types may appear in a `pushdown fn`,
for example `fade(ms(500))` inside a render chosen at compile time, because it is folded. A
`pushdown fn` may not hold one at runtime: a local, parameter, return value or story variable of
such a type is an error (see [Finite types](./kinds.md#finite-types)).

## `Int<lo, hi>`

```kis
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

`Int<lo, hi>` is a primitive type: an integer in a declared, inclusive range. `lo` and `hi` are
constant expressions of type `i64` with `lo <= hi`. The analyzer represents a variable with as
many bits as its range needs, so a counter from 0 to 5 costs 3 bits. Use the smallest range that
fits.

An integer literal has type `Int<n, n>` and converts to any integer type whose range contains it:
an `Int` type or a Rust integer. A suffix fixes the type: `5u8` is a `u8`.

*Tentative.* Arithmetic is exact on ranges:

| Expression | Type |
| ---------- | ---- |
| `a + b` with `a: Int<l1, h1>`, `b: Int<l2, h2>` | `Int<l1 + l2, h1 + h2>` |
| `a - b` | `Int<l1 - h2, h1 - l2>` |
| `a * b` | the smallest range containing the four corner products |
| `a == b`, `a < b`, … | `bool` |

A value converts implicitly to an `Int` type whose range contains its range. Any other change of
integer type is explicit, see [Conversions](#conversions). Compound assignment saturates at the
bounds of the variable: `x += 1` and `x -= 2` are `x = (x + 1).saturate()` and
`x = (x - 2).saturate()`.

Saturation keeps every step total. When the analyzer can prove that a reachable compound
assignment hits a bound, it reports a warning, since a counter that silently saturates is often a
sign that its range is too small.

Patterns match `Int` values with literals and ranges, which is how thresholds are written:

```kis
match AFFECTION_MIO {
    3..=5 => MioSmiles(),
    _ => MioShrugs(),
}
```

## Conversions

Apart from literals and the widening of one `Int` type to another, integer types never convert
implicitly. The `Integer` trait of the standard library, implemented by `Int<lo, hi>` and every
Rust integer, gives two explicit conversions. The target type is inferred from context:

| Method | Result |
| ------ | ------ |
| `x.into()` | The same value. A compile error unless every value of the source type fits in the target type |
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

There are no `as` casts.

## Rust integers, `Rat` and `Decimal`

The Turing-complete kinds have Rust's integer types with Rust's operators, and two exact types:

- `Decimal` is an exact decimal number, for values that are naturally written in base ten, such
  as opacities, volumes or positions in a layout. A literal with a decimal point, such as `0.5`
  or `1.0`, is a `Decimal`. It replaces floats and is `Eq`.
- `Rat` is an exact rational number: a numerator and a positive denominator in lowest terms.

*Tentative:* overflow of a Rust integer during compile-time evaluation is a compile error.
Literal syntax for `Rat`, the precision of both types, their conversions, and runtime overflow are
[open questions](./open-questions.md#numbers).
