# Functions

A function is an item that takes parameters and computes a result. Every function has a
[kind](../concepts/kinds.md), which decides what its body may do; the syntax is the same for all
kinds.

## Declarations

```text
Function   = Attribute* "pub"? Kind? "fn" Name Generics? "(" Params ")" ("->" Type)? Body
Kind       = "const" | "pushdown"
Body       = Block | ";"
```

```kis
enum Answer { Yes, No }
static mut TRUST: Int<0, 5> = 0;

fn quit_game() { nav::quit() }
pub const fn twice(x: u32) -> u32 { x * 2 }
pushdown fn reset_trust() { TRUST = 0; }
#[scene]
pub pushdown fn Ask() -> Answer { Answer::Yes }
```

- The kind keyword comes directly before `fn`, after attributes and `pub`: `pub const fn`,
  `#[scene] pub pushdown fn`. A function has at most one kind keyword. There is no `unsafe`,
  `async` or `extern`.
- A function without a body (`;`) must be `#[builtin]`, which only the standard library may use
  ([`E1010`](../diagnostics/modules.md#e1010)).
- By convention, scenes are named in PascalCase (`Ask`), other functions in snake_case.

## Kinds

| Keyword | Kind | Body |
| ------- | ---- | ---- |
| `fn` | Turing-complete runtime code: the game system and its UI | anything except the story |
| `pushdown fn` | the story; with `#[scene]` a scene, otherwise an instant procedure | finite types, story variables, steps in scenes |
| `const fn` | pure code, evaluated at compile time where needed | anything pure |

The call rules between kinds and the meaning of each kind are in
[Function Kinds](../concepts/kinds.md). The compiler never infers a kind: a function is `const`
only when it says so.

## Parameters

Each parameter is an irrefutable pattern and a type:

```kis
const fn area((w, h): (u32, u32)) -> u32 { w * h }
const fn fade_in(mut screen: Screen) -> Screen { screen }
fn ignore(_: u32) {}
```

- The type is required. There are no default values, no variadic parameters and no named
  arguments.
- The pattern must always match ([`E2038`](../diagnostics/types.md#e2038)): a name, `mut name`,
  `_`, or a tuple or struct pattern of these.
- Arguments are passed by value: the function receives a copy. A `mut` parameter lets the function
  change its copy, which is how a render function changes the screen it was given and returns it.
- To let a function change the caller's value, pass a reference `&mut T`; see
  [References](./values.md#references).
- A call with the wrong number of arguments is [`E2024`](../diagnostics/types.md#e2024).

## Return type and body {#return-type-and-body}

`-> R` declares the result type; without it, the result is `()`. The body is a block whose final
expression is the result. `return e;` returns early.

```kis
enum Mood { Warm, Polite }

const fn mood(affection: Int<0, 5>) -> Mood {
    if affection >= 4 {
        return Mood::Warm;
    }
    Mood::Polite
}
```

Every path through the body must produce a value of the return type; a path that can reach the end
without one is a type error. A `loop` without `break` produces no value and needs none.

The `?` operator ends the function early on `None` or `Err`: in a function that returns
`Option<_>`, `x?` unwraps `Some` or returns `None`; in a function that returns `Result<_, E>`, it
unwraps `Ok` or returns the `Err` ([`E2034`](../diagnostics/types.md#e2034)).

## Generic functions

```kis
fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn first<T, const N: usize>(items: [T; N]) -> T {
    items[0]
}

fn demo() -> bool {
    same(first([1u8, 2, 3]), first::<u8, 2>([1, 4]))
}
```

A function may have type parameters with [bounds](./types.md#trait-bounds) and const parameters.
Arguments are inferred from the call, or given with the turbofish `f::<…>`. Generic functions are
monomorphized: each combination of arguments used gets its own copy. In a `pushdown fn`, every
type argument must be a [finite type](./types.md#finite-types).

## Methods and associated functions

Functions in an [`impl` block](./items.md#implementations) are **associated functions** of the
target type. One whose first parameter is `self` is a **method**.

```kis
struct Gauge {
    value: Int<0, 10>,
}

impl Gauge {
    const MAX: Int<0, 10> = 10;

    pub const fn new() -> Gauge {
        Gauge { value: 0 }
    }

    pub const fn is_full(&self) -> bool {
        self.value == Self::MAX
    }

    pub const fn fill(&mut self, amount: Int<0, 10>) {
        self.value += amount;           // saturates at 10
    }
}

const fn full_gauge() -> bool {
    let mut gauge = Gauge::new();
    gauge.fill(7);
    gauge.fill(7);
    gauge.is_full()                     // true
}
```

| Receiver | Means | The method gets |
| -------- | ----- | --------------- |
| `self` | `self: Self` | a copy of the value |
| `mut self` | `mut self: Self` | a copy it may change |
| `&self` | `self: &Self` | a reference to the value |
| `&mut self` | `self: &mut Self` | a mutable reference: changes reach the caller |

- Explicit receiver types (`self: Box<Self>`) are not supported.
- `value.method(args)` calls a method. The receiver is borrowed or dereferenced automatically, so
  `gauge.fill(7)` passes `&mut gauge`. A `&mut self` method needs a mutable place
  ([`E3017`](../diagnostics/kinds.md#e3017)).
- `Type::function(args)` calls an associated function. A method can be called the same way, with
  the receiver as the first argument: `Gauge::is_full(&gauge)`. A method of a standard trait can
  also be called through the trait: `Integer::into(value)`.
- Inside the `impl`, `Self` names the target type and `self` the receiver.
- Associated functions and constants are private unless marked `pub`.
- An unknown method is [`E2022`](../diagnostics/types.md#e2022).

## Closures as arguments

A parameter of a [closure type](./values.md#closure-and-pointer-types) accepts a closure, a named
function or a function pointer of the same kind. This is how the system passes handlers, and how
the story passes small pieces of code down:

```kix
fn Counter() -> Element {
    let (count, set_count) = signal(0u32);
    button!{on_click = || set_count(count() + 1), t!["Count: " {count()}]}
}

pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}
```

A parameter of a function pointer type, such as `overlay: fn() -> Overlay` of `nav::call`, accepts
a named function or a closure that captures nothing. In the story, closures are second-class:
they can be passed and called, never stored or returned (see [Closures](./values.md#closures)).

## Function pointers

The name of a function is a value of its function type, which converts to a
[function pointer](./values.md#closure-and-pointer-types) of the same kind:

```kis
fn apply(f: fn(u32) -> u32, x: u32) -> u32 {
    f(x)
}

fn double(x: u32) -> u32 {
    x * 2
}

fn demo() -> u32 {
    apply(double, 21)
}
```

Tuple struct and tuple variant names are `const fn` constructors: `Some` is a value of type
`const fn(T) -> Option<T>`. Function pointers compare by identity. Calling something that is not a
function is [`E2025`](../diagnostics/types.md#e2025).

## Recursion and termination {#recursion-and-termination}

Whether a function may call itself, and which loops it may contain, depends on its kind:

| Kind | Recursion | Loops | What bounds it |
| ---- | --------- | ----- | -------------- |
| `fn` | allowed | all | the runtime's call depth limit; exceeding it is a VM panic |
| `const fn` | allowed | all | compile-time evaluation stops after 1,000,000 steps ([`E4003`](../diagnostics/eval.md#e4003)) |
| scene (`#[scene] pushdown fn`) | allowed, through a step | all, through a step | every loop and every cycle of calls must pass a step ([`E5001`](../diagnostics/analysis.md#e5001), [`E5002`](../diagnostics/analysis.md#e5002)) |
| instant procedure (`pushdown fn`) | not allowed ([`E3005`](../diagnostics/kinds.md#e3005)) | `for` over an array or a constant range only ([`E3004`](../diagnostics/kinds.md#e3004), [`E3006`](../diagnostics/kinds.md#e3006)) | always finishes |

So the code of the story between two inputs of the player always finishes. The rules for scenes
are explained in [Termination](../concepts/scenes.md#termination).

## Nested functions

A function may be declared inside a block, like any other item (see
[Items in blocks](./items.md#items-in-blocks)). A nested function is an ordinary item: it does not
capture the locals of the enclosing function. Use a closure to capture.

## The `main` function {#the-main-function}

The root module declares `fn main() -> System`, a plain `fn` that describes the game system: its
title, resolution, modes, settings and fonts. The engine calls it once at boot. A root module
without `main` is [`E7001`](../diagnostics/emit.md#e7001). See
[The Game System](../concepts/system.md).
