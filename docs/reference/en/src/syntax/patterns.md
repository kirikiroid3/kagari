# Patterns

A pattern describes the shape of a value and binds variables to its parts. Patterns are Rust's,
with fewer forms. They appear in:

| Place | Must be irrefutable |
| ----- | ------------------- |
| `let pat = value;` | yes |
| Function and closure parameters | yes |
| `for pat in iterable` | yes |
| `match` arms | no |
| `if let pat = value`, `while let pat = value` | no |

## Pattern forms

| Pattern | Example | Matches |
| ------- | ------- | ------- |
| Wildcard | `_` | Anything; binds nothing |
| Binding | `x`, `mut x` | Anything; binds it to `x` |
| Literal | `0`, `-1`, `true`, `"Mio"`, `0.5` | A value equal to the literal |
| Range | `3..=5`, `0..10`, `6..`, `..=2` | An integer in the range |
| Path | `Route::None`, `ORIGIN`, `Self::EMPTY` | A unit variant, a unit struct or a constant |
| Tuple | `(a, _, true)`, `(first, ..)` | A tuple, element by element |
| Tuple struct / variant | `Some(x)`, `EntityId(id)`, `Pair(a, ..)` | A tuple struct or tuple variant, field by field |
| Struct / struct variant | `Point { x, y: 0 }`, `Shape::Rect { w, .. }` | A struct or struct-like variant, by field name |
| Alternatives | `Map::A \| Map::B`, `\| 1 \| 2` | Any of the alternatives |
| Parenthesized | `(Some(_) \| None)` | The inner pattern |

There are no slice or array patterns (`[a, b]`), no reference patterns (`&x`), no `ref` or
`ref mut` bindings and no `name @ pattern` bindings.

### Bindings

An identifier pattern binds a new variable, unless the name resolves to a unit struct, a unit
variant or a constant in scope; then it is a path pattern that compares against that value, as in
Rust:

```kis
const LIMIT: u32 = 3;

const fn at_limit(n: u32) -> bool {
    match n {
        LIMIT => true,   // the constant `LIMIT`, not a new binding
        _ => false,
    }
}
```

`mut x` makes the binding mutable. A name bound twice in one pattern is `E1024`. In an
alternative `A | B`, every alternative must bind the same names (`E1025`). A `static` cannot be
used in a pattern (`E2035`).

When a value of type `&T` or `&mut T` is matched against a pattern that is not a binding, the
reference is matched through, and bindings inside bind references to the parts (`&T` or `&mut T`),
as in Rust's default binding mode.

### Literal patterns

An integer literal must fit the matched type (`E2013`); it may be negative. A string literal
matches a `Text`, a decimal literal matches a `Decimal`, `true` and `false` match a `bool`. A
literal of another type than the matched value is `E2001`.

### Range patterns

```kis
match AFFECTION_MIO {
    0 => MioIgnores(),
    1..=2 => MioShrugs(),
    3.. => MioSmiles(),
}
```

| Form | Matches |
| ---- | ------- |
| `a..=b` | `a` to `b`, both included |
| `a..b` | `a` to `b`, `b` excluded |
| `a..` | `a` and above |
| `..=b` | up to `b`, included |

A bound is an integer literal (optionally negative) or a path to an integer constant. Range
patterns match integers only, both Rust integers and `Int<lo, hi>`; on any other type they are
`E2001`. There is no `..b` pattern: `..` alone is the rest pattern below.

### Rest patterns

`..` inside a tuple, tuple-struct or struct pattern stands for all the fields not written:
`(first, ..)`, `Pair(a, ..)`, `Shape::Rect { w, .. }`. In a tuple or tuple-struct pattern it may
appear once, at any position: `(.., last)` matches the last element. In a struct pattern it comes
last.

### Struct patterns

`Point { x, y: 0 }` matches a `Point` whose `y` is 0 and binds `x`. A field written alone (`x`,
`mut x`) binds a variable of the same name; `field: pattern` matches the field against a pattern.
Fields not listed match anything. Unlike Rust, the trailing `..` is optional; write it to make the
omission visible. An unknown field is `E2020`; a pattern of the wrong shape (a tuple pattern on a
struct variant, the wrong number of fields in a tuple pattern, an enum name instead of a variant)
is `E2035`.

## `match` and exhaustiveness

```kix
match (VISITED_A, VISITED_B) {
    (true, true) => FestivalEnd(),
    (true, false) | (false, true) => s!(t!["One more stall."]),
    (false, false) => {}
}
```

A `match` arm is `pattern => expression`, optionally with a guard: `pattern if condition =>
expression`. Arms are tried in order; the first arm whose pattern matches and whose guard is true
runs. A comma separates arms; it may be omitted after an arm whose body is a block.

A `match` must be **exhaustive**: every value of the scrutinee's type must be matched by some arm
(`E2037`). The message lists values that no arm matches. An arm that can never run because
earlier arms cover it is a warning (`W2039`).

The compiler knows the values of these types:

| Type | Values the check knows |
| ---- | ---------------------- |
| `bool` | `true`, `false` |
| `Int<lo, hi>` | Exactly `lo` to `hi`, so `Int<0, 2>` is covered by `0`, `1` and `2` without `_` |
| Rust integers | Their full range, so a `match` on a `u8` needs arms up to `255` or a `_` |
| Enums | Their variants, recursively into the fields |
| Structs, tuples, arrays | One shape, recursively into the fields |
| `&T`, `&mut T` | The values of `T` |
| `Text`, `String`, `Decimal`, `Rat`, generic parameters, other builtin types | Unlimited: a literal covers only itself, so a `_` or binding arm is required |

Guards are not taken into account: an arm with a guard covers nothing for the check. A constant
used as a pattern covers the value of its initializer when the initializer is a literal, a
constructor, or a tuple or struct of those; otherwise it covers only itself.

Because the [choice](./forms.md#choice) of a `choice!{…}` is an ordinary enum value, a `match` on
it is checked like any other: a variant without an arm in the choice must still be handled or
covered by `_`.

## Refutability

A pattern is **irrefutable** when it matches every value of its type. `let`, `for` and parameter
patterns must be irrefutable (`E2038`):

```kis
let (a, b) = pair;          // irrefutable
let Point { x, .. } = p;    // irrefutable
let Some(v) = maybe;        // E2038: `None` is not covered
```

Use `if let` or `match` for a refutable pattern:

```kis
if let Some(v) = maybe {
    use_value(v);
}
```

There is no `let … else`, and `if let` cannot be combined with other conditions by `&&`.
