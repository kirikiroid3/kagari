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
| `#[builtin]`  | function, `struct` | Implemented by the engine: a function has no body, a type has no visible fields. Only allowed in the standard library |

Any other attribute is an error.

## Types

| Type | Example |
| ---- | ------- |
| Booleans, unit | `bool`, `()` |
| Bounded integers | `Int<0, 5>`; see [Numbers](./numbers.md) |
| Rust integers, exact numbers | `u8` … `i128`, `usize`, `isize`, `Rat`, `Decimal`; see [Numbers](./numbers.md) |
| Text | `Text`: constant text, plain (`"Mio"`) or rich (`t![…]`); string literals have type `Text`. `String`: a runtime string |
| Tuples, arrays | `(bool, Route)`, `[Int<0, 3>; 4]` |
| Structs, enums | user-declared; `Option<T>` from the prelude |
| Collections | `Vec<T>` |
| Function pointers | `fn(Int<0, 5>) -> bool`, `const fn(Screen) -> Screen`, `pushdown fn() -> Mood`, `scene fn() -> Answer` (*tentative*) |
| Closures | `Fn(A) -> B`, `pushdown Fn(A) -> B`, `const Fn(A) -> B` (*tentative*) |
| References | `&T`, `&mut T`, second-class; see [Values, References and Closures](./values.md) |

Which types the story may hold is restricted to [finite types](./kinds.md#finite-types).

Every type built without closures has structural equality: two values are equal when they are
built the same way. There is no `derive`. Function pointers compare by identity, and closures
cannot be compared. *Tentative:* the standard traits `Eq`, `Step`, `Asset` and `ToText` are the
only traits; user traits and user generics are an [open question](./open-questions.md#traits-and-generics).
Const generic arguments exist for `Int<lo, hi>` and standard library types.

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
| Lifetimes, borrow checker, moves, `Drop` | Values have value semantics; references are second-class, see [Values, References and Closures](./values.md) |
| Floats | Screens must be comparable exactly; use `Rat` or `Decimal` |
| `unsafe`, raw pointers | Not needed |
| Macros | Functions and scenes cover what templates and includes used to do; [forms](./narrative/forms.md) are a fixed set built into the syntax |
| Qualified paths `<T as Trait>::f` | Only standard traits exist |
| `async`, threads | The story's suspension is built in; see [Narrative](./narrative/overview.md) |
| `goto`, labels as positions | Structured control flow only; see [Control flow](./narrative/scenes.md#control-flow) |
