# Numbers

There are two families of numbers. Bounded integers are the only numbers the story may hold at
runtime; the Turing-complete kinds also have Rust's integers and two exact non-integer types.
There are no floats: screens must be compared for equality, and exact numbers keep that
comparison decidable.

| Type | Values | Allowed in |
| ---- | ------ | ---------- |
| `Int<lo, hi>` | integers from `lo` to `hi`, inclusive | every kind |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | as in Rust | `fn`, `const fn`; constants anywhere |
| `Rat` | exact rationals | `fn`, `const fn`; constants anywhere |
| `Decimal` | exact decimals | `fn`, `const fn`; constants anywhere |

"Constants anywhere" means a constant expression of these types may appear in a `pushdown fn`,
for example `fade(30)` inside a render chosen at compile time, because it is folded. A
`pushdown fn` may not hold one at runtime: a local, parameter, return value or story variable of
such a type is an error (see [Finite types](./kinds.md#finite-types)).

## `Int<lo, hi>`

```kis
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

`Int<lo, hi>` is an integer in a declared, inclusive range. `lo` and `hi` are constant
expressions with `lo <= hi`. The analyzer represents a variable with as many bits as its range
needs, so a counter from 0 to 5 costs 3 bits. Use the smallest range that fits.

An integer literal has type `Int<n, n>` and converts to any `Int` type whose range contains it.

*Tentative.* Arithmetic is exact on ranges:

| Expression | Type |
| ---------- | ---- |
| `a + b` with `a: Int<l1, h1>`, `b: Int<l2, h2>` | `Int<l1 + l2, h1 + h2>` |
| `a - b` | `Int<l1 - h2, h1 - l2>` |
| `a * b` | the smallest range containing the four corner products |
| `a == b`, `a < b`, … | `bool` |

A value converts to an `Int` type whose range contains its range; anything else is an error. Two
explicit ways narrow a value:

- `x.saturate()` clamps to the range of the type it converts to.
- Compound assignment saturates at the bounds of the variable: `x += 1` and `x -= 2` are
  `x = (x + 1).saturate()` and `x = (x - 2).saturate()`.

Saturation keeps every step total. When the analyzer can prove that a reachable compound
assignment hits a bound, it reports a warning, since a counter that silently saturates is often a
sign that its range is too small.

Patterns match `Int` values with literals and ranges, which is how thresholds are written:

```kis
match AFFECTION_MIO {
    3..=5 => <MioSmiles />,
    _ => <MioShrugs />,
}
```

## Rust integers, `Rat` and `Decimal`

The Turing-complete kinds have Rust's integer types with Rust's operators, and two exact types:

- `Rat` is an exact rational number: a numerator and a positive denominator in lowest terms.
- `Decimal` is an exact decimal number, for values that are naturally written in base ten, such
  as durations or positions in a layout.

*Tentative:* overflow of a Rust integer during compile-time evaluation is a compile error.
Literal syntax for `Rat` and `Decimal`, their precision, conversions, and runtime overflow are
[open questions](./open-questions.md#numbers).
