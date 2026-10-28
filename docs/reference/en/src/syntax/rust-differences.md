# Differences from Rust

Kagari's syntax is a subset of Rust's with a few additions, and where this book does not say
otherwise, a construct means what it means in Rust. This chapter collects what a Rust programmer
needs to unlearn: what is missing and what replaces it, and where a familiar construct behaves
differently.

## What is not in the language

| Rust feature | Why not, or what instead |
| ------------ | ------------------------ |
| Lifetimes, the borrow checker, moves, `Drop` | Values are copied; references are shared pointers. See [Values, References and Closures](./values.md) |
| Floats (`f32`, `f64`) | Floats are not `Eq` and differ across platforms, and screens must compare exactly. Use `Decimal` (`0.5`) or `Rat`; see [Numbers](./numbers.md) |
| `as` casts | `x.into()` (lossless, checked) and `x.saturate()` (clamped). `as` parses but is `E2019` |
| User traits, `impl Trait for T`, `derive` | Only the standard library's `#[builtin]` traits exist (`E1012`, `E1023`); `Eq` is implemented automatically. See [Language Traits](../special/traits.md) |
| `dyn Trait`, `impl Trait` types, `where` clauses, `<T as Trait>::f` | Only standard traits exist; a bound is written in the generic parameter list (`T: Eq`) |
| Macros (`macro_rules!`, `println!`, `vec!`, …) | Functions and scenes cover what templates and includes did; the [forms](./forms.md) are a fixed set built into the syntax |
| `unsafe`, raw pointers, `extern` | Not needed: code runs in a verified VM |
| `async`, threads | The story's suspension at steps is built in; see [Steps](../concepts/steps.md) |
| `goto`, labels as positions | Structured control flow only; see [Control Flow and Loops](./control-flow.md#no-goto) |
| `char`, byte strings, raw strings, `&str` | A string literal is a `Text`; `String` is a runtime string |
| `Box`, `Rc`, `HashMap`, slices | Arrays `[T; N]` and `Vec<T>`; every type is a value type |
| The `Iterator` trait, `.iter()`, `.map(…)` chains | `for` loops over arrays, `Vec<T>` and integer ranges directly |
| `move` closures | A closure always captures by value |
| `let … else`, `if let … && …` chains, labelled blocks | `match`, nested `if let`, and labels on loops |
| Slice patterns, `&` patterns, `ref`, `name @ pattern` | See [Patterns](./patterns.md#pattern-forms) for the supported forms |
| Struct update syntax `S { a: 1, ..base }` | Write every field |
| `#[derive]`, `#[cfg]`, `#[test]` and other attributes | Only `#[scene]`, `#[entry]` and `#[must_use]` (and `#[builtin]` in the standard library). Any other attribute is `E1006`. See [Attributes](../special/attributes.md) |
| `pub(crate)`, `pub(super)` | Only `pub` |
| Open ranges `a..`, `..b`, `..=b`, `..` as values | A range expression needs both bounds (`E2030`). In patterns, `a..` and `..=b` exist, but `..b` does not; see [Range patterns](./patterns.md#range-patterns) |

## Things that work differently

### Function kinds

Every function has a kind: `fn`, `pushdown fn` or `const fn`, and a `pushdown fn` may be a
`#[scene]`. The kind decides what the function may call and do, and a `const fn` is pure: it
cannot read story variables, settings or meta. See [Function Kinds](../concepts/kinds.md).

### Numbers

- `Int<lo, hi>` is a primitive bounded integer. Arithmetic on it computes the range of the result,
  and compound assignment saturates at the bounds instead of overflowing.
- Integer types never convert implicitly, apart from integer literals and widening one `Int` type
  to another that contains its range.
- Overflow and division by zero are compile errors in constant code and panics at run time; there
  is no wrapping arithmetic.
- `match` on an `Int<lo, hi>` is exhaustive when it covers `lo` to `hi`, without `_`.

See [Numbers](./numbers.md).

### Values and references

Assignment, argument passing and returning copy values. `&T` and `&mut T` are shared pointers with
no borrow checker: two `&mut` to one place may coexist in `fn` and `const fn` code. In a
`pushdown fn`, references and closures are second-class: they may be passed and called, never
stored or returned. See [Values, References and Closures](./values.md).

### Equality and ordering

There is no `PartialEq` or `PartialOrd`. `==` and `!=` use the standard trait `Eq`, which every
type with structural equality implements automatically; closures never do (`E2017`). `<`, `>`,
`<=` and `>=` exist only for integers, `Int`, `Decimal` and `Rat` (`E2015`).

### Statics

`static mut` declares a [story variable](../concepts/story.md#story-variables): safe, but usable
only from `pushdown fn` code (`E3007`). An immutable `static` may only hold a `Setting` or `Meta`
handle (`E3011`); use `const` for other constants.

### Bindings and patterns

- Assigning to a binding counts as mutation even the first time: a `let` without initializer must
  be `let mut` to be assigned later (`E3017`).
- A struct pattern may leave out fields without writing `..`.
- `..b` is not a pattern; only `..=b` is.

### The `?` operator {#the-try-operator}

`?` works on `Option` and `Result` as in Rust, but there is no `From` conversion of the error: the
function must return a `Result` with the same error type (`E2034`).

### Strings and text

A string literal has type `Text`, the constant display text of the game, not `&str`. In `.kix`
files, rich text is written with [`t![…]`](./forms.md#t); `String` is the type of text built at
runtime by `fn` code.

### Files

A `.kis` file holds plain code. A `.kix` file may also use [forms](./forms.md). See
[Source Files and Modules](./modules.md).
