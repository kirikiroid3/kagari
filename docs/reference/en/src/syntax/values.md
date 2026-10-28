# Values, References and Closures

*Tentative.* This chapter fixes the memory model. It is simpler than Rust's on purpose: no
lifetimes, no borrow checker and no moves.

## Value semantics

Every type is a value type. Assignment, passing an argument and returning a value copy it;
changing the copy never changes the original. The implementation may share storage and copy on
write, but a program cannot observe it.

```kis
struct Point { x: i32, y: i32 }

fn demo() -> i32 {
    let a = Point { x: 1, y: 2 };
    let mut b = a;      // a copy
    b.x = 10;
    a.x                 // still 1: `a` is unchanged and still usable
}
```

There are no moves and no destructors. A value is gone when nothing holds it.

## Places and mutability {#places-and-mutability}

A **place** is something that holds a value: a local, a parameter, a story variable, a field or
element of one of these, or the target of a reference (`*r`). Assignment, compound assignment,
`&mut` and a method with a `&mut self` receiver need a **mutable** place:

- a `let mut` local or a `mut` parameter;
- a story variable (`static mut`), in `pushdown fn` code;
- a field or element of a mutable place;
- a place reached through `&mut`.

A place reached through `&T`, a local or parameter without `mut`, a constant and a handle are
read-only. Changing them is [`E3017`](../diagnostics/kinds.md#e3017).

## References

`&T` and `&mut T` let code read or change a value without copying it. `&x` and `&mut x` take a
reference to the place `x`, and `*r` is the place a reference points to. Field access and method
calls look through references automatically.

```kis
fn bump(counter: &mut u32) {
    *counter += 1;
}

fn demo() -> u32 {
    let mut n: u32 = 0;
    bump(&mut n);
    let r = &mut n;
    let s = r;          // copies the pointer, not the value
    *s += 1;
    n                   // 2
}
```

A reference is a shared pointer to a place. A reference keeps its place alive, and copying a
reference copies the pointer, not the value.

- In `fn` and `const fn` code, references are ordinary values: they may be bound to locals,
  stored in fields and collections, returned, and captured by closures.
- Writes through one reference are visible through every other reference to the same place. There
  is no borrow checker: two `&mut` to one place may coexist. The game runs in a VM, so this can
  only surprise the program, never corrupt memory.
- `&mut T` converts implicitly to `&T`.
- `==` on references compares the values they point to.
- The type of a `const` item, a story variable or a handle cannot contain a reference
  ([`E3012`](../diagnostics/kinds.md#e3012)).

Render functions show the common pattern: the screen is passed in by value, changed through
methods that take `&mut self`, and returned.

```kis
const HANAKO: EntityId = EntityId(216);

const fn hanako_appears(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```

### References in the story

In a `pushdown fn`, references are **second-class**: they can be passed down, never kept.

- A reference type may appear only as the type of a parameter, including a method receiver
  (`&self`, `&mut self`). A local, a field or a captured variable cannot hold one
  ([`E3014`](../diagnostics/kinds.md#e3014)).
- A function may return a reference only if its receiver is one; the result lives until the end of
  the statement that produced it ([`E3014`](../diagnostics/kinds.md#e3014)).
- **Exclusivity.** In one call, a place passed as `&mut` may not also be passed as any other
  argument, so two parameters never alias when one of them can change
  ([`E3016`](../diagnostics/kinds.md#e3016)).

```kis
static mut TRUST: Int<0, 5> = 0;
static mut DOUBT: Int<0, 5> = 0;

pushdown fn shift(from: &mut Int<0, 5>, to: &mut Int<0, 5>) {
    *from -= 1;
    *to += 1;
}

pushdown fn reassure() {
    shift(&mut DOUBT, &mut TRUST);    // fine
    shift(&mut TRUST, &mut TRUST);    // E3016: the same place twice
}
```

Because a reference never outlives the call it was passed to and never aliases another argument,
a `pushdown fn` that takes `&mut x` behaves exactly as if it received a copy of `x` and the caller
stored the result back on return. A `&mut` argument of a `pushdown fn` is an in-out parameter of a
finite type, and story state never holds a pointer. This keeps references harmless for the
[analyses of the story](../concepts/kinds.md#why-this-works).

## Closures

```kis
fn scaled(values: [i32; 3], factor: i32) -> [i32; 3] {
    let scale = |v: i32| v * factor;                            // captures a copy of `factor`
    let clamp = |v: i32| -> i32 { if v > 100 { 100 } else { v } };
    [clamp(scale(values[0])), clamp(scale(values[1])), clamp(scale(values[2]))]
}
```

A closure is written `|parameters| body`. Parameter types may be omitted when the context gives
them. With an explicit return type `-> R`, the body must be a block. There is no `move`.

A closure **captures by value**: it copies the local variables it uses when it is created.
Changing a captured copy inside the closure does not change the original; shared mutable state
goes through references, story variables or [signals](../concepts/system.md). Capturing a
reference copies the pointer, so writes through it reach the original place.

A closure has a [kind](../concepts/kinds.md), like a function. Its kind is the kind of the closure
type it is passed to or stored as; without one, it is the kind of the function that contains it.
The body is checked with the rules of that kind. What may be done with a closure depends on the
kind of the code that creates it:

| Kind | Closures are | May be |
| ---- | ------------ | ------ |
| `fn` | first-class | stored, returned, put into UI values (handlers) |
| `const fn` | first-class | stored, returned |
| `pushdown fn` | second-class | passed as an argument and called; never stored, returned or captured by another closure ([`E3015`](../diagnostics/kinds.md#e3015)) |

A closure in the story is an [instant procedure](../concepts/scenes.md#scenes-and-instant-procedures):
it cannot run a step ([`E3002`](../diagnostics/kinds.md#e3002)) or call a scene
([`E3003`](../diagnostics/kinds.md#e3003)), and it has no `loop` or `while`
([`E3004`](../diagnostics/kinds.md#e3004)).

```kix
static mut AFFECTION_MIO: Int<0, 5> = 0;

pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}

#[scene]
pushdown fn Compliment() {
    twice(|| AFFECTION_MIO += 1);
    s!(t!["Mio smiles."]);
}
```

Closures are never `Eq` ([`E2017`](../diagnostics/types.md#e2017)).

### Why closures in the story are second-class {#why-closures-in-the-story-are-second-class}

A first-class closure in the story would let story state contain closures that contain closures,
without bound. That is a higher-order pushdown system: still decidable, but the cost of analysis
grows by an exponential with each order (Ong, *On model-checking trees generated by higher-order
recursion schemes*, LICS 2006). A second-class closure is a function plus finite captured values
that disappear when the call returns, so the story stays an ordinary pushdown system.

## Closure and pointer types {#closure-and-pointer-types}

Function pointer types and closure types carry the [kind](../concepts/kinds.md) of the code they
call:

| Type | Holds |
| ---- | ----- |
| `fn(A) -> R` | a named `fn` |
| `const fn(A) -> R` | a named `const fn`, or a tuple struct or tuple variant constructor |
| `pushdown fn(A) -> R` | a named instant procedure |
| `scene fn(A) -> R` | a named [scene](../concepts/scenes.md#scenes-and-instant-procedures) (*tentative* syntax; `scene` is a keyword only here) |
| `Fn(A) -> R`, `const Fn(A) -> R`, `pushdown Fn(A) -> R` | a closure or function of that kind (*tentative* syntax) |

The kind must match exactly: a `const fn` is not a value of type `fn(A) -> R`, and a scene is not
a value of type `pushdown fn(A) -> R`. These conversions apply implicitly:

- A named function or constructor converts to a function pointer of its kind.
- A named function or a function pointer converts to the closure type of the same kind. Scenes
  have no closure type.
- A closure that captures nothing converts to a function pointer of the same kind; this is how a
  render is written inline (see [Inline renders](../concepts/presentation.md#inline-renders)). A
  closure that captures a variable cannot ([`E2036`](../diagnostics/types.md#e2036)).

```kis
const fn double(x: u32) -> u32 { x * 2 }

fn demo() -> Option<u32> {
    let f: const fn(u32) -> u32 = double;
    let wrap: const fn(u32) -> Option<u32> = Some;     // a variant constructor
    let g: const Fn(u32) -> u32 = f;                   // pointer to closure type
    wrap(g(f(1)))
}
```

A function pointer is a value of a finite set, the named functions of the program, so
`pushdown fn`, `scene fn` and `const fn` pointers are [finite](./types.md#finite-types) and may
be held by the story. Function pointers compare by identity.

Calls through pointers and closures follow the same [call rules](../concepts/kinds.md#call-rules)
as direct calls, because the type tells the compiler the kind of the callee.
