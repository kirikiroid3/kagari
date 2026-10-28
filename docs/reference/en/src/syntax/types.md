# Types

Kagari is statically typed. Every value has one type, known at compile time. Types are written out
in item signatures and inferred inside function bodies.

## Type expressions

| Syntax | Type |
| ------ | ---- |
| `Path`, `Path<Args>` | a named type: primitive, struct, enum, type alias or generic parameter |
| `()` | the unit type, the empty tuple |
| `(A,)`, `(A, B)`, … | a tuple |
| `(A)` | `A`; parentheses only group |
| `[T; N]` | an array of `N` elements |
| `&T`, `&mut T` | a shared or mutable reference |
| `fn(A) -> R`, `const fn(A) -> R`, `pushdown fn(A) -> R`, `scene fn(A) -> R` | a function pointer |
| `Fn(A) -> R`, `const Fn(A) -> R`, `pushdown Fn(A) -> R` | a closure type |
| `_` | a placeholder: "infer this type" |
| `Self` | the target type inside an `impl` |

A function pointer or closure type without `-> R` returns `()`. The placeholder `_` is allowed in
`let` annotations and generic arguments inside bodies, but not in item signatures
([`E2011`](../diagnostics/types.md#e2011)). There are no slice types `[T]`, no `str`, no `!` type
syntax, no `dyn` or `impl Trait` types and no raw pointers.

## Overview

| Type | Values | Notes |
| ---- | ------ | ----- |
| `bool` | `true`, `false` | |
| `()` | `()` | the value of blocks and functions without a result |
| `Int<lo, hi>` | the integers from `lo` to `hi` | primitive; see [Numbers](./numbers.md#intlo-hi) |
| `u8` … `u128`, `i8` … `i128`, `usize`, `isize` | as in Rust | see [Rust integers](./numbers.md#rust-integers) |
| `Decimal`, `Rat` | exact decimals and fractions | see [Numbers](./numbers.md) |
| `Text` | constant text | see [Text and String](#text-and-string) |
| `String` | runtime text | |
| tuples, arrays | `(bool, Route)`, `[Int<0, 3>; 4]` | |
| `Vec<T>` | growable lists | |
| `Range<T>` | `0..4`, `3..=5` | see [Ranges](#ranges) |
| structs, enums | user-declared; `Option<T>` and `Result<T, E>` from the prelude | |
| references | `&T`, `&mut T` | see [Values, References and Closures](./values.md#references) |
| function pointers, closures | `fn(u32) -> bool`, `Fn(u32) -> bool` | see [Closure and pointer types](./values.md#closure-and-pointer-types) |
| builtin types | `Screen`, `Narrative`, `Element`, `Setting<T>`, `Meta<T>`, the asset types, … | opaque handles; see [Builtin Types](../special/builtin-types.md) |

There are no floating-point types. Which types the story may hold at runtime is restricted to
[finite types](#finite-types).

## Primitive types

`bool`, `Int` and the twelve Rust integer types are primitive types. They have no declaration
and are found after every other name (see [Paths](./modules.md#paths)). `Int` always takes exactly
two bounds, the other primitives take no generic arguments
([`E2005`](../diagnostics/types.md#e2005)). User code cannot add an `impl` to a primitive type.

## Text and String {#text-and-string}

| Type | What it is |
| ---- | ---------- |
| `Text` | Constant text of the program: a plain string literal (`"Mio"`) or rich text from `t![…]` with ruby and text effects. Every string literal has type `Text`. `Text` is [finite](#finite-types): its values are the texts written in the program, so the story may hold it |
| `String` | A string built at runtime by `fn` code. Not finite |

Both are `Eq`. `Text` is declared in `std::narrative::text`; `String` in `std`. Both are in the
prelude.

## Tuples, arrays and `Vec` {#tuples-arrays-and-vec}

- A **tuple** `(A, B, …)` groups values of different types. Its fields are `.0`, `.1`, …. A
  one-element tuple is written `(A,)`, and `()` is the unit type.
- An **array** `[T; N]` holds exactly `N` values of type `T`. `N` is a
  [type-level constant](#type-level-constants). An array is written `[a, b, c]` or `[value; N]` and
  indexed with `a[i]`; an index out of range is an error at compile time or a VM panic at
  runtime.
- `Vec<T>` is a list whose length changes at runtime: `Vec::new()`, `push`, `len`, `is_empty`,
  `get` and `set`. It is not finite. See the [STD Reference](../../../../std/en/src/std.md).

## Structs and enums

Structs and enums are nominal: two declarations with the same fields are different types. They
may be generic. The standard library declares `Option<T>` (`None`, `Some(T)`) and
`Result<T, E>` (`Ok(T)`, `Err(E)`), with their variants in the prelude. Their declaration forms
are in [Items](./items.md#structs).

## Ranges

`a..b` and `a..=b` are values of the standard struct `Range<T>`:

```kis
pub struct Range<T> {
    pub start: T,
    pub end: T,
    pub inclusive: bool,
}
```

`a..b` excludes `b`, `a..=b` includes it. As values, only `a..b` and `a..=b` exist; any other
form, such as `a..` or `..b`, is [`E2030`](../diagnostics/types.md#e2030). Patterns have their
own range forms, `a..`, `..=b`, `a..b` and `a..=b` (see
[Range patterns](./patterns.md#range-patterns)); there is no `..b` pattern, and a bare `..` in a
pattern is the rest pattern. Ranges are iterated by `for` (see
[Control Flow and Loops](./control-flow.md)).

## Generics

Functions, structs, enums, type aliases and `impl` blocks may have generic parameters:

```kis
struct Pair<T> { a: T, b: T }

fn first<T, const N: usize>(items: [T; N]) -> T {
    items[0]
}
```

- A **type parameter** `T` may have [bounds](#trait-bounds): `T: Eq`, `T: Eq + Integer`.
- A **const parameter** `const N: usize` stands for a constant of an integer type. `Int<lo, hi>`
  itself is the primitive with two `i64` const parameters.
- Generic arguments are inferred, or written explicitly: `Pair<u8>` in a type,
  `first::<u8, 3>(…)` (the turbofish) in an expression. A wrong number of arguments is
  [`E2005`](../diagnostics/types.md#e2005).
- Generic code is **monomorphized**: the compiler makes a copy for each combination of arguments
  that the program uses. In a `pushdown fn`, every type argument must be a
  [finite type](#finite-types), so the story stays a finite program.
- There are no lifetimes, no `where` clauses, no default arguments and no associated types of
  your own.

### Type-level constants

Array lengths, `Int` bounds and const generic arguments are **type-level constants**. As a
generic argument one is written as an integer literal (`Square<8>`), a negative literal
(`Int<-5, 5>`), the name of a const parameter or `const` item (`Int<0, MAX>`), or an expression in
braces (`Int<0, { MAX * 2 }>`). The expression may use only integer literals, unary `-`,
`+ - * / %`, blocks with only a final expression, const parameters and `const` items whose values
are such expressions ([`E2002`](../diagnostics/types.md#e2002)). A type given where a constant is
expected is [`E2007`](../diagnostics/types.md#e2007).

```kis
const MAX: i64 = 5;
static mut TRUST: Int<0, MAX> = 0;
static mut DOUBLE: Int<0, { MAX * 2 }> = 0;
```

## Trait bounds

A bound names a trait of the standard library. Code cannot declare or implement traits, but it can
require them:

| Trait | Implemented by | Gives |
| ----- | -------------- | ----- |
| `Eq` | every type with structural equality, see [Equality](#equality) | `==`, `!=` |
| `Integer` | `Int<lo, hi>` and the Rust integers | `into`, `saturate`; see [Conversions](./numbers.md#conversions) |
| `RandChoose` | fieldless enums and `Int<lo, hi>` | `rng_choose::<T>()` |
| `Asset` | the asset types | `asset::<A>(…)` |
| `Step` | the steps `Say` and `Choice` | `push_step`, with the associated type `Output` |
| `ToText` | `Text`, `String`, `Int<lo, hi>`, the Rust integers, `Rat`, `Decimal` | interpolation `{expr}` in `t![…]` |

A generic argument that does not satisfy a bound is
[`E2026`](../diagnostics/types.md#e2026). A bound makes the trait's items available on the
parameter: `S::Output` for `S: Step`, `value.into()` for `T: Integer`. Details are in
[Language Traits](../special/traits.md).

## Equality

There is no `derive`. `==` and `!=` need `Eq` on both sides
([`E2017`](../diagnostics/types.md#e2017)), and `Eq` is implemented automatically:

- by `bool`, `Int<lo, hi>` and the Rust integers;
- by a struct, enum, tuple or array whose fields or elements are all `Eq`, comparing structurally:
  two values are equal when they are built the same way;
- by function pointers and named functions, which compare by identity;
- by `&T` and `&mut T` when `T` is `Eq`, comparing the values they point to;
- by the builtin types that the standard library declares `Eq`: `String`, `Vec<T>` (when `T` is
  `Eq`), `Rat`, `Decimal`, `Text`, `Screen` and the asset types.

Closures are never `Eq`. Other builtin types are not `Eq`.

## Finite types

The story holds only **finite types** at runtime: types with finitely many values that the
compiler can enumerate. In short:

| Finite | Not finite |
| ------ | ---------- |
| `bool`, `()`, `Int<lo, hi>` | Rust integers, `Rat`, `Decimal` |
| structs, enums, tuples and arrays of finite types | `String`, `Vec<T>`, recursive types |
| `Text` and the asset types | `Screen` and the other builtin handles |
| `pushdown fn`, `scene fn` and `const fn` pointers | `fn` pointers, closures |
| `&T` and `&mut T` of a finite `T`, as parameters only | |

A constant expression of any type may still appear in the story, because its value is computed at
compile time. The full rule is in [Function Kinds](../concepts/kinds.md#finite-types).

## Function pointer and closure types {#function-pointer-and-closure-types}

Function pointer and closure types carry the [kind](../concepts/kinds.md) of the code they call,
so the call rules hold through them:

| Type | Holds |
| ---- | ----- |
| `fn(A) -> R`, `const fn(A) -> R` | a named function of that kind |
| `pushdown fn(A) -> R` | a named instant procedure |
| `scene fn(A) -> R` | a named scene (*tentative* syntax) |
| `Fn(A) -> R`, `const Fn(A) -> R`, `pushdown Fn(A) -> R` | a closure or a function of that kind (*tentative* syntax) |

They are described with their coercions in
[Values, References and Closures](./values.md#closure-and-pointer-types).

## Coercions

Where a value of one type is expected and another is given, these implicit conversions apply:

| From | To |
| ---- | -- |
| an integer literal | any integer type whose range contains it |
| `Int<l1, h1>` | `Int<l2, h2>` with `l2 <= l1` and `h1 <= h2` (widening) |
| `&mut T` | `&T` |
| a named function or tuple constructor | a function pointer of the same kind and signature |
| a named function, function pointer or closure | a closure type of the same kind (not for scenes) |
| a closure that captures nothing | a function pointer of the same kind |
| the type of `return`, `break`, `continue` and endless `loop` | any type |

Every other change of type is explicit, for example `x.into()`.

## Type inference

Inside a body, types are inferred from use, as in Rust. Item signatures are never inferred:
parameter types, return types and the types of constants and statics are always written. When
inference cannot decide a type, an annotation is needed
([`E2029`](../diagnostics/types.md#e2029)):

```kis
fn demo() {
    let names: Vec<Text> = Vec::new();
    let level: Int<0, 5> = 3;
}
```
