---
title: "Expressions and Statements"
---

Kagari is an expression language with Rust's syntax: a block is an expression, `if` and `match`
produce values, and statements end with `;`. This chapter lists every expression the parser
accepts and the rules the compiler adds. Control-flow expressions (`if`, `match`, loops, `break`,
`continue`, `return`) are in [Control Flow and Loops](./control-flow.md); the `.kix` forms
(`s!(…)`, `t![…]`, `column![…]`, …) are in [Forms](./forms.md).

## Blocks and statements

```kis
const fn label_width(chars: Int<0, 40>) -> u32 {
    let n: u32 = chars.into();
    let padding = 8;
    n * 16 + padding
}
```

A block `{ … }` runs its statements in order. Its value is the final expression when that
expression has no `;` after it; otherwise the block's value is `()`. A block opens a scope: the
bindings of its `let` statements end at its closing `}`.

A block contains:

| Statement | Form | Notes |
| --------- | ---- | ----- |
| `let` statement | `let pat: Type = expr;` | See [below](#let-and-let-mut) |
| Expression statement | `expr;` | The value is discarded. Discarding a `#[must_use]` value is `W3023` |
| Block-like expression | `if …`, `match …`, `loop`, `while`, `for`, `{ … }`, a `name!{…}` form | Needs no `;` |
| Item | `fn`, `struct`, `enum`, `const`, `use`, … | An item declared inside a block. A file module (`mod name;`) is `E1005` |
| Empty statement | `;` | Ignored |

**Block-like expressions end a statement.** At the start of a statement, a block, `if`, `match`,
`loop`, `while`, `for` or a brace form such as `choice!{…}` is a complete statement without `;`.
Nothing after it continues the expression, so `if c { a } else { b } - 1` is two statements, and
`{ … } (x)` is a block followed by a parenthesized expression, not a call. Write
`(if c { a } else { b }) - 1` to use the value.

A missing `;` between two expression statements is a syntax error (`E0001`).

### `let` and `let mut` {#let-and-let-mut}

```kis
let n = 3;
let mut total: u32 = 0;
let (x, y) = (1, 2);
let Point { x: px, y: py } = origin();
let mut later: bool;
```

`let` binds the variables of a pattern to a value:

- The pattern must be irrefutable: it must match every value of the type (`E2038`). See
  [Patterns](./patterns.md#refutability).
- The type annotation `: Type` is optional; without it the type is inferred.
- The initializer `= expr` is optional. A binding declared without one must be assigned before it
  is read; reading it earlier stops the game with a run-time panic.
- A binding is immutable unless the pattern marks it `mut`. Assigning to an immutable binding is
  `E3017`, **including the first assignment of a binding declared without an initializer**:
  declare such a binding `let mut`.
- In a `pushdown fn`, a local holds a value of a [finite type](../concepts/kinds.md#finite-types)
  (`E3008`) and never a closure or a reference (`E3015`, `E3014`).

## Literals

| Literal | Examples | Type |
| ------- | -------- | ---- |
| Integer | `42`, `1_000`, `0xFF`, `0o17`, `0b1010`, `5u8`, `300i64` | An integer type inferred from context; a suffix fixes it. See [Numbers](./numbers.md) |
| Decimal | `0.5`, `1.0`, `12.75` | `Decimal` |
| String | `"Mio"`, `"line one\nline two"` | `Text` |
| Boolean | `true`, `false` | `bool` |
| Unit | `()` | `()` |

There are no character, byte, raw-string or floating-point literals, and no exponent notation.
`-5` is the unary `-` applied to `5`; it is typed as one negative literal. The exact token syntax
(escapes, suffixes, digit separators) is in [Lexical Structure](./lexical.md).

## Paths

```kis
ROUTE
Route::Azusa
std::time::ms(500)
Self::new()
super::helpers::clamp(x)
asset::<ImageAsset>("bg/room.png")
rng_choose::<Coin>()
```

A path names a local, a parameter, an item (function, constant, static, unit struct, enum
variant) or an associated item. Segments are separated by `::`; `self`, `super`, `crate` and
`Self` work as in Rust (see [Source Files and Modules](./modules.md)).

In an expression, generic arguments are written with the turbofish `::<…>`. A generic argument is
a type, an integer literal (optionally with `-`) or a block `{ … }` holding a
[type-level constant](./constants.md#type-level-constants).

A path in an expression that names something other than a value, such as a module or a struct
with named fields, is `E2009`. An unknown name is `E1016`.

## Operators

### Precedence and associativity

From the parser, strongest binding first:

| Operators | Associativity |
| --------- | ------------- |
| Paths, literals, `( … )`, blocks, forms, `if`, `match`, loops | — |
| Field `a.b`, tuple field `a.0`, method call `a.m(…)`, call `f(…)`, index `a[i]`, `?` | left to right |
| Unary `-`, `!`, `*` (dereference), `&`, `&mut` | prefix |
| `as` (always an error, `E2019`) | left |
| `*` `/` `%` | left |
| `+` `-` | left |
| `<<` `>>` | left |
| `&` | left |
| `^` | left |
| `\|` | left |
| `==` `!=` `<` `>` `<=` `>=` | none: `a < b < c` is a syntax error |
| `&&` | left |
| `\|\|` | left |
| `..` `..=` | left; ranges are not meant to be chained |
| `=` `+=` `-=` `*=` `/=` `%=` `^=` `&=` `\|=` `<<=` `>>=` | right |
| `return`, `break`, closures `\|…\| body` | extend as far right as possible |

The table is Rust's, except that ranges associate to the left instead of being non-associative.
The comparison operators cannot be chained: write `a < b && b < c`.

### Operand types

| Operator | Operands | Result |
| -------- | -------- | ------ |
| `+` `-` `*` `/` `%` | two values of the same Rust integer type; two `Int` values; two `Decimal`s; two `Rat`s | See [Numbers](./numbers.md): `Int` arithmetic computes the result's range |
| `&` `\|` `^` | two values of the same Rust integer type, or two `bool`s | The operand type |
| `<<` `>>` | two Rust integers, possibly of different types | The left operand's type |
| `==` `!=` | two values of one type that is `Eq` (`E2017` otherwise) | `bool` |
| `<` `>` `<=` `>=` | two integers of one type, two `Int` values, two `Decimal`s or two `Rat`s | `bool` |
| `&&` `\|\|` | two `bool`s; the right side runs only when needed | `bool` |
| unary `-` | a signed Rust integer, an `Int` (the range is negated), `Decimal` or `Rat` | The operand type |
| unary `!` | `bool` (logical not) or a Rust integer (bitwise not) | The operand type |
| unary `*` | a reference `&T` or `&mut T` (`E2033` otherwise) | `T` |
| `&e`, `&mut e` | any expression; `&mut` needs a mutable place (`E3017`) | `&T`, `&mut T` |

Integer types never mix implicitly: an operator on an `Int` and a Rust integer, or on two different
Rust integer types, is an error (`E2014`, `E2001`); convert one side with `.into()` or
`.saturate()`. Bitwise operators need Rust integers; on `Int` they are `E2015`. A division or
remainder whose `Int` divisor range contains 0 is `E2016`. Ordering is defined only for numbers:
`<` on tuples, `Text` or enums is `E2015`. Equality on `Eq` types compares structurally; see
[Language Traits](../special/traits.md).

### `as`

`as` parses with Rust's precedence, but every cast is an error (`E2019`). Convert integers with
`x.into()` (lossless, checked at compile time) or `x.saturate()` (clamped); see
[Numbers](./numbers.md).

## Assignment and compound assignment

```kis
total = 0;
AFFECTION_MIO += 1;
point.x = 10;
scores[i] -= 1;
```

`place = value` stores a value; `place op= value` is `place = place op value` for the operators
`+ - * / % ^ & | << >>`. An assignment is an expression of type `()`.

The left side must be a mutable place (`E3017`):

- a local or parameter declared `mut`;
- a story variable (`static mut`), in a `pushdown fn`;
- a field `a.b` or element `a[i]` of a mutable place;
- `*r` where `r` is a `&mut T`, or a field or element reached through a `&mut T`.

A constant, an immutable `static`, a temporary such as a call result, and anything reached through
a `&T` cannot be assigned.

Compound assignment to an `Int` place **saturates** at the bounds of its type, and its right side
may be any `Int` value: `AFFECTION_MIO += 1` on an `Int<0, 5>` that holds 5 leaves 5. See
[Numbers](./numbers.md).

## Calls and method calls

```kis
fade(ms(500))
Confession()
screen.entity(AZUSA).image()
x.into()
scores.get(0)
```

`f(a, b)` calls a function, a function pointer or a closure; `f` may be any expression. The number
of arguments must match (`E2024`), and a value that cannot be called is `E2025`. Which function
kinds may call which is in [Function Kinds](../concepts/kinds.md#call-rules).

`receiver.method(args)` calls a method declared in an `impl` of the receiver's type, or one that a
standard trait provides. The receiver is borrowed or dereferenced automatically to match the
method's `self`, `&self` or `&mut self`; a `&mut self` method needs a mutable place (`E3017`).
`Type::function(args)` calls an associated function. Generic arguments of a method use the
turbofish: `x.m::<T>(…)`. An unknown method is `E2022`; method syntax on an associated function
without `self` is `E2023`.

## Fields and indexing

```kis
point.x
pair.0
nested.0.1
grid[row][col]
```

- `a.name` reads a named field; `a.0`, `a.1`, … read the fields of a tuple or a tuple struct.
  `t.0.1` is two field accesses. References are dereferenced automatically. An unknown field is
  `E2020`.
- `a[i]` reads an element of an array or a `Vec<T>` (`E2032` for other types). The index is an
  integer; an integer literal index is a `usize`. An index past the end is a run-time panic, or
  `E4005` during [const evaluation](./constants.md#const-evaluation-errors).

## Struct, tuple and array expressions

```kis
Point { x: 1, y: 2 }
Point { x, y }
Shape::Rect { w: 10, h: 20 }
EntityId(216)
Route::None
(1, true)
(5,)
[1, 2, 3]
[false; 8]
```

| Expression | Notes |
| ---------- | ----- |
| `Path { field: value, … }` | A struct or a struct-like enum variant. Every field must be given (`E2021`); an unknown field is `E2020`. `Path { x }` is short for `Path { x: x }`. There is no struct update syntax `..base` |
| `Path(a, b)` | A tuple struct or tuple variant: its constructor is a function |
| `Path` | A unit struct or unit variant |
| `(a, b)`, `(a,)`, `()` | A tuple; a one-element tuple needs the trailing comma, `(a)` is just `a` |
| `[a, b, c]` | An array of the listed elements, of one type |
| `[value; N]` | An array of `N` copies; `N` is a [type-level constant](./constants.md#type-level-constants) |

A struct expression cannot appear directly in the condition of `if` and `while`, the scrutinee of
`match` or the iterable of `for`, because the `{` would open the body. Wrap it in parentheses:
`if p == (Point { x: 0, y: 0 }) { … }`.

## Ranges

```kis
0..4      // 0, 1, 2, 3
3..=5     // 3, 4, 5
```

`a..b` (end excluded) and `a..=b` (end included) build a `Range<T>`, a std struct with the fields
`start`, `end` and `inclusive`. `T` is a type both bounds convert to: `0..7` is a
`Range<Int<0, 7>>`. A range expression needs both bounds: `a..`, `..b`, `..=b` and `..` are `E2030`.
As patterns, `a..` and `..=b` exist, but `..b` does not, and `..` alone is the rest pattern; see
[Range patterns](./patterns.md#range-patterns). Ranges are used by `for` loops; see
[Control Flow and Loops](./control-flow.md#for-loops).

## Closures

```kis
|x| x + 1
|| nav::back()
|a: u32, b: u32| a * b
|mut screen| { screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant()); screen }
|v| -> u32 { v * 2 }
```

`|params| body` is a closure. A parameter is an irrefutable pattern with an optional type; `||`
has no parameters. The body is any expression; with a return type `-> T`, the body must be a
block. There is no `move`: a closure always captures the variables it uses by value.

What a closure may do and how it is typed (`Fn(A) -> B`, `pushdown Fn(A) -> B`,
`const Fn(A) -> B`) is in [Values, References and Closures](./values.md). In short:

- A closure that captures nothing converts to a function pointer of its kind; this is how a render
  is written inline. A capturing closure where a function pointer is required is `E2036`.
- In a `pushdown fn`, closures are second-class: they may be passed as arguments and called, never
  stored in a local, returned or captured (`E3015`).

## The `?` operator {#the-try-operator}

```kis
fn first_score(scores: Vec<u32>) -> Option<u32> {
    let first = scores.get(0)?;
    Some(first * 10)
}
```

`expr?` on an `Option<T>` gives the `T` of `Some`, or returns `None` from the enclosing function. On
a `Result<T, E>` it gives the `T` of `Ok`, or returns the `Err`. The enclosing function must
return an `Option`, or a `Result` with the same error type; otherwise, and on any other type, it is
`E2034`. `?` is never a constant expression.
