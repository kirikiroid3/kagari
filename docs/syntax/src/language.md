# The Core Language

The language is a subset of Rust with a few additions. Where this book does not say otherwise,
syntax and meaning are Rust's: statements end with `;`, the last expression of a block is its
value, `::` separates path segments, `.` accesses fields and calls methods.

## Items

| Item | Form | Notes |
| ---- | ---- | ----- |
| Function | `fn`, `pushdown fn`, `const fn` | The kind decides what the body may do; see [Function Kinds](./kinds.md) |
| Struct | `struct Point { x: i32, y: i32 }` | Named fields, tuple structs and unit structs |
| Enum | `enum Route { None, Hanako, Azusa }` | Variants may carry data |
| Constant | `const HANAKO: EntityId = EntityId(216);` | The initializer is a constant expression |
| Story variable | `static mut ROUTE: Route = Route::None;` | Narrative state; only `pushdown fn` may use it, see [Story variables](./narrative/overview.md#story-variables) |
| Handle | `static MUSIC: Setting<Volume> = setting(volume(60));` | An immutable static holds a handle to a setting or meta field; see [The Game System](./system.md) |
| Type alias | `type Render = const fn(Screen) -> Screen;` | |
| Implementation | `impl Point { … }` | Methods and associated functions |
| Module | `mod story;`, `mod ui { … }` | See [Source Files and Modules](./files.md) |
| Import | `use story::Day1;` | |

Items may be used before their declaration, as in Rust.

### Attributes

| Attribute     | On | Meaning |
| ------------- | -- | ------- |
| `#[scene]`    | `pushdown fn` | A scene: may run steps and call other scenes; the unit of model checking. See [Scenes](./narrative/scenes.md#scenes-and-instant-procedures) |
| `#[entry]`    | `#[scene]` function | An entry scene: the system can start the story here; see [Entry scenes](./narrative/scenes.md#entry-scenes) |
| `#[must_use]` | `struct`, `enum` | Discarding a value of this type is a warning |
| `#[builtin]`  | function, `struct`, `trait` | Implemented by the engine: a function has no body, a type has no visible fields. Only allowed in the standard library |

Any other attribute is an error.

## Types

| Type | Example |
| ---- | ------- |
| Booleans, unit | `bool`, `()` |
| Bounded integers | `Int<0, 5>`, a primitive; see [Numbers](./numbers.md) |
| Rust integers, exact numbers | `u8` … `i128`, `usize`, `isize`, `Rat`, `Decimal`; see [Numbers](./numbers.md) |
| Text | `Text`: constant text, plain (`"Mio"`) or rich (`t![…]`); string literals have type `Text`. `String`: a runtime string |
| Tuples, arrays | `(bool, Route)`, `[Int<0, 3>; 4]` |
| Structs, enums | user-declared, generic or not; `Option<T>` and `Result<T, E>` from the prelude |
| Ranges | `Range<T>`, the type of `0..4` and `3..=5` |
| Collections | `Vec<T>` |
| Function pointers | `fn(Int<0, 5>) -> bool`, `const fn(Screen) -> Screen`, `pushdown fn() -> Mood`, `scene fn() -> Answer` (*tentative*) |
| Closures | `Fn(A) -> B`, `pushdown Fn(A) -> B`, `const Fn(A) -> B` (*tentative*) |
| References | `&T`, `&mut T`; see [Values, References and Closures](./values.md) |

Which types the story may hold is restricted to [finite types](./kinds.md#finite-types).

### Generics and traits

Functions, structs, enums and impls may be generic over types and over constants:
`struct Pair<T> { a: T, b: T }`, `fn first<T, const N: usize>(items: [T; N]) -> T`. A bound may name
a standard trait (`T: Eq`). In a `pushdown fn`, every type argument must be a [finite
type](./kinds.md#finite-types); generic code is monomorphized, so the story stays a finite program.

Traits are not part of the user language. The standard library declares a fixed set of `#[builtin]`
traits, and only the standard library implements them:

| Trait | Implemented by |
| ----- | -------------- |
| `Eq` | Every type with structural equality, see below |
| `Integer` | `Int<lo, hi>` and the Rust integers; gives `into` and `saturate`, see [Numbers](./numbers.md#conversions) |
| `Asset` | The asset types |
| `Step` | The steps `Say` and `Choice` |
| `ToText` | The types `{expr}` in `t![…]` may interpolate |

A `trait` item or an `impl Trait for T` outside the standard library is an error.

### Equality

There is no `derive`. `Eq` is implemented automatically:

- by `bool`, `()`, `Int<lo, hi>`, the Rust integers, `Rat` and `Decimal`;
- by a struct, enum, tuple or array when every field or element is `Eq`, comparing structurally:
  two values are equal when they are built the same way;
- by function pointers, which compare by identity;
- by `&T` and `&mut T` when `T` is `Eq`, comparing the values they point to;
- by the standard library's `#[builtin]` types that say so, such as `Text`, `String`, `Vec<T>` and
  `Screen`.

Closures are never `Eq`. `==` and `!=` need `Eq` on both sides.

### Constant expressions

A constant expression is an expression the compiler computes at compile time:

- literals, paths to `const` items, unit structs, enum variants and named functions;
- an immutable `let` local whose initializer is a constant expression;
- struct, enum, tuple, array and range literals, operators, field accesses, blocks, `if` and
  `match` whose operands are all constant expressions;
- a call of a `const fn` whose arguments are all constant expressions; its result is a constant
  expression;
- `asset::<A>("…")`.

Story variables, parameters and the results of `fn` and `pushdown fn` calls are never constant.
The initializer of a `const` item, a story variable or a handle must be a constant expression, and a
constant expression of any type may appear in a `pushdown fn` (see [Finite
types](./kinds.md#finite-types)).

## Expressions and statements

Literals, operators, blocks, `let` and `let mut`, assignment and compound assignment, `if` and
`if let`, `match`, `loop`, `while`, `while let`, `for … in`, `break` (with a value in `loop`),
`continue`, loop labels (`'map: loop`), `return`, struct and enum literals, tuple and array
literals, field access, method calls, closures `|x| x + 1`, and ranges `0..4`, `3..=5`.

In `.kix` files, an expression may also be a **form** such as `s!(…)`, `choice!{…}` or `t![…]`.
Forms are fixed syntax for struct expressions and steps; see [Steps and
Forms](./narrative/forms.md) for the story and [The Game System](./system.md) for UI.

`match` must be exhaustive. Patterns are Rust's: literals, ranges (`3..=5`), `_`, bindings,
tuples, structs, enum variants, and alternatives with `|`.

## What is not in the language

| Rust feature | Why not, or what instead |
| ------------ | ------------------------ |
| Lifetimes, borrow checker, moves, `Drop` | Values are copied; references are shared pointers, see [Values, References and Closures](./values.md) |
| Floats | Floats are not `Eq`, and screens must compare exactly; use `Decimal` (`0.5`) or `Rat` |
| `as` casts | `x.into()` and `x.saturate()`, see [Conversions](./numbers.md#conversions) |
| User traits, `derive` | Only the standard `#[builtin]` traits exist; `Eq` is automatic |
| `unsafe`, raw pointers | Not needed |
| Macros | Functions and scenes cover what templates and includes used to do; [forms](./narrative/forms.md) are a fixed set built into the syntax |
| Qualified paths `<T as Trait>::f` | Only standard traits exist |
| `async`, threads | The story's suspension is built in; see [Narrative](./narrative/overview.md) |
| `goto`, labels as positions | Structured control flow only; see [Control flow](./narrative/scenes.md#control-flow) |
