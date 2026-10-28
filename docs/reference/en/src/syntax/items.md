# Items

An **item** is a declaration that belongs to a module: a function, a type, a constant, a static,
an implementation, a module or an import. Items may appear at the top of a file, in an inline
module, in a block (see [Items in blocks](#items-in-blocks)), and, for functions and constants, in
an `impl`.

## Overview

| Item | Form | See |
| ---- | ---- | --- |
| Function | `fn f() {}`, `const fn f() {}`, `pushdown fn f() {}` | [Functions](./functions.md) |
| Struct | `struct Point { x: i32, y: i32 }`, `struct Id(u32);`, `struct Marker;` | [Structs](#structs) |
| Enum | `enum Route { None, Hanako, Azusa }` | [Enums](#enums) |
| Constant | `const HANAKO: EntityId = EntityId(216);` | [Constants](#constants) |
| Story variable | `static mut ROUTE: Route = Route::None;` | [Story variables](#story-variables) |
| Handle | `static MUSIC: Setting<Volume> = setting(volume(60));` | [Handles](#handles) |
| Type alias | `type Pair<T> = (T, T);` | [Type aliases](#type-aliases) |
| Implementation | `impl Point { … }` | [Implementations](#implementations) |
| Module | `mod story;`, `mod ui { … }` | [Source Files and Modules](./modules.md#modules) |
| Import | `use story::Day1;` | [`use` declarations](./modules.md#use-declarations) |

The general shape of an item is:

```text
Item       = Attribute* "pub"? ItemBody
Attribute  = "#" "[" Path "]"
```

Items may be used before their declaration. The order of items in a file has no meaning.

## Structs

```kis
struct Point { x: i32, y: i32 }     // record struct
struct Affection(Int<0, 5>);        // tuple struct
struct Marker;                      // unit struct
struct Pair<T> { a: T, b: T }       // generic struct
```

- A **record struct** has named fields and is built with `Point { x: 1, y: 2 }`.
- A **tuple struct** has numbered fields, accessed as `.0`, `.1`, …. Its name is also a
  constructor function: `Affection(3)`. The constructor is a `const fn` and can be used as a value
  of a `const fn(…) -> …` pointer type.
- A **unit struct** has no fields. Its name is its only value.
- A struct may be generic over types and constants; see [Generics](./types.md#generics). There
  are no `where` clauses and no default generic arguments.
- Fields may be marked `pub` and may carry attributes. The compiler does not check field
  visibility yet; see [Visibility](./modules.md#visibility).
- There is no `derive`. Equality is automatic; see [Equality](./types.md#equality).

## Enums

```kis
enum Route { None, Hanako, Azusa }

enum Ending {
    Normal,
    Good(Int<0, 3>),
    Bad { reason: Route },
}
```

- A variant is a unit variant, a tuple variant or a record variant, like the three struct forms.
- Variants are named through the enum, `Route::Hanako`, unless imported: `use Route::*;` imports
  every variant. `Option` and `Result` have their variants in the [prelude](./modules.md#the-prelude).
- Tuple variant names are constructor functions, like tuple struct names: `Ending::Good(2)`.
- Explicit discriminants (`A = 1`) are a syntax error. Enums have no numeric value; convert with a
  `match`.
- An enum whose variants all carry no data is a **fieldless enum**. Fieldless enums implement
  `RandChoose` and are the values of `choice!` arms.

## Constants

```kis
const HANAKO: EntityId = EntityId(216);
const MAX_AFFECTION: Int<0, 100> = 5 * 20;
const ROOF: ImageAsset = asset("bg/roof.png");
```

- The type is required.
- The initializer must be a [constant expression](./constants.md)
  ([`E3010`](../diagnostics/kinds.md#e3010)): it is computed by the compiler. A constant without
  a value is [`E1011`](../diagnostics/modules.md#e1011).
- The type cannot contain a reference ([`E3012`](../diagnostics/kinds.md#e3012)).
- A constant may be used anywhere, also in the story, whatever its type, because its value is
  known at compile time.
- A constant may also appear in an `impl` (an **associated constant**, used as `Point::ORIGIN` or
  `Self::ORIGIN`) and in a block.
- A constant initializer is checked like `const` code: it may call only `const fn`.

## Statics

`static` declares a value with one place for the whole run of the game. There are exactly two
uses, and the compiler tells them apart by `mut`.

### Story variables

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

A `static mut` is a **story variable**: narrative state, saved and restored with the story.

- Its type must be [finite](../concepts/kinds.md#finite-types)
  ([`E3008`](../diagnostics/kinds.md#e3008)) and cannot contain a reference
  ([`E3012`](../diagnostics/kinds.md#e3012)).
- Its initializer is a constant expression ([`E3010`](../diagnostics/kinds.md#e3010)).
- Only `pushdown fn` code may read or write it
  ([`E3007`](../diagnostics/kinds.md#e3007)).

See [Story variables](../concepts/story.md#story-variables) for their meaning.

### Handles

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

An immutable `static` is a **handle**: it holds a `Setting<T>` or a `Meta<T>`
([`E3011`](../diagnostics/kinds.md#e3011)). Its initializer is a call of `setting(…)` or
`meta(…)` with a constant argument. These calls are allowed only in the initializer of a `static`
([`E4007`](../diagnostics/eval.md#e4007)). Use `const` for an ordinary constant. Settings and meta
are described in [The Game System](../concepts/system.md).

## Type aliases

```kis
type Pair<T> = (T, T);
type Board = Square<8>;
```

A type alias gives a type another name; it does not create a new type. It may be generic. An alias
without a type is [`E1011`](../diagnostics/modules.md#e1011), and an alias that expands to itself
is [`E2010`](../diagnostics/types.md#e2010). An alias of a struct or enum can name its associated
items and can be the target of an `impl`: `Board::new()`.

## Implementations

```kis
struct Square<const N: u64> {
    side: u64,
}

impl<const N: u64> Square<N> {
    const CELLS: u64 = N * N;

    pub const fn new() -> Self {
        Self { side: N }
    }

    pub const fn cells(&self) -> u64 {
        self.side * Self::CELLS
    }
}
```

An **inherent implementation** `impl Type { … }` adds associated functions and constants to a
struct or enum. Functions with a `self` parameter are **methods**; see
[Methods and associated functions](./functions.md#methods-and-associated-functions).

- The target is a struct or enum, written with its generic arguments; the `impl` declares the
  generic parameters it uses: `impl<T> Pair<T> { … }`. The target may also be a struct or enum of
  the standard library. Primitive types cannot be extended
  ([`E1022`](../diagnostics/modules.md#e1022)).
- Inside the `impl`, `Self` is the target type.
- Associated items are private unless marked `pub`. Two associated items of one type with the same
  name are [`E1014`](../diagnostics/modules.md#e1014), even in different `impl` blocks.
- A type may have several `impl` blocks, in any module of the game.
- `impl Trait for Type` is reserved for the standard library
  ([`E1023`](../diagnostics/modules.md#e1023)).

## Traits

User code does not declare traits ([`E1012`](../diagnostics/modules.md#e1012)) and does not
implement them ([`E1023`](../diagnostics/modules.md#e1023)). The standard library declares a fixed
set of traits, which code uses as bounds on generic parameters (see
[Trait bounds](./types.md#trait-bounds)). They are described in
[Language Traits](../special/traits.md).

## Modules and imports

`mod` declares a module and `use` imports names. Both are described in
[Source Files and Modules](./modules.md).

## Items in blocks

A block may contain items next to its statements. The items are visible in the whole block,
including before their declaration, and in nested blocks; they are not visible outside the block.

```kis
fn uses() -> u64 {
    const STEP: u64 = 2;
    fn helper(x: u64) -> u64 {
        x + STEP
    }
    use crate::widen as w;
    helper(1) + w(3u8)
}

fn widen<T: Integer>(value: T) -> u64 {
    Integer::into(value)
}
```

An item in a block is an ordinary item: a nested function does not capture the locals of the
enclosing function, and a nested `const` is computed at compile time. A file module (`mod name;`)
cannot be declared in a block ([`E1005`](../diagnostics/modules.md#e1005)); an inline module can.

## Attributes

An attribute is written `#[name]` before an item, before `pub`. The compiler knows four
attributes:

| Attribute | On |
| --------- | -- |
| `#[scene]` | a `pushdown fn` |
| `#[entry]` | a `#[scene]` function |
| `#[must_use]` | a struct or an enum |
| `#[builtin]` | a function, struct or trait of the standard library |

Any other attribute is [`E1006`](../diagnostics/modules.md#e1006), an attribute with arguments
(`#[scene(x)]`) is [`E1007`](../diagnostics/modules.md#e1007), and an attribute on the wrong item
is [`E3018`](../diagnostics/kinds.md#e3018). Inner attributes `#![…]` are a syntax error. The
meaning of each attribute is in [Attributes](../special/attributes.md).
