---
title: "Constant Expressions"
---

A **constant expression** is an expression the compiler computes at compile time. Constants carry
everything the story cannot hold at runtime, such as durations, colours and renders, and they let
the compiler check values such as text effects, motions and asset paths before the game runs.

## Definition

An expression is constant when it is one of:

- a literal;
- a path to a `const` item, a unit struct or unit variant, a named function or a tuple
  struct or tuple variant constructor, a const generic parameter, an immutable `static` (a
  `Setting` or `Meta` handle; the value is the handle itself), or a constant local (below);
- a struct, enum, tuple, array or range expression, an operator (unary, binary, index, shared
  borrow `&`), a field access, a block, an `if`, `if let` or a `match`, whose operands are all
  constant. The statements of such a block are `let` statements with a constant initializer that
  bind only immutable variables, items, and constant expression statements;
- a call of a `const fn` or of a tuple constructor whose arguments are all constant. In a method
  call the receiver counts as an argument, and it must not be borrowed mutably: a call of a
  `&mut self` method is not constant;
- `asset::<A>("…")`, which is such a call;
- a closure that captures nothing.

A **constant local** is an immutable binding of a `let` whose initializer is constant, or of an
`if let` or `match` arm pattern whose scrutinee is constant.

These are never constant: parameters, story variables (`static mut`), `let mut` bindings, the
results of `fn` and `pushdown fn` calls, loops, `break`, `continue`, `return`, `?`, assignments,
`&mut` borrows and closures that capture variables.

```kis
const FADE: Transition = fade(ms(500));       // a const fn call with constant arguments
const CENTER: Point = point(1280 / 2, 720);   // an operator on literals
const DIM: Color = {
    let base = rgb(40, 40, 60);               // a constant local
    Color { r: base.r, g: base.g, b: base.b, a: 160 }
};
```

## `const fn` calls

A call of a `const fn` with constant arguments is a constant expression: the compiler runs the
function and uses its result. The function's body does not have to be a constant expression
itself. It may use `let mut`, loops and assignment; it only has to be a `const fn`, which is pure
(see [Function Kinds](../concepts/kinds.md#const-fn)).

```kis
const fn triangle(n: u32) -> u32 {
    let mut total = 0;
    for i in 1..=n {
        total += i;
    }
    total
}

const T10: u32 = triangle(10);   // 55, computed at compile time
```

A compile-time evaluation runs at most 1 000 000 steps (`E4003`).

## Where constant expressions are required

| Where | Requirement | Diagnostic |
| ----- | ----------- | ---------- |
| Initializer of a `const` item | A constant expression | `E3010` |
| Initializer of a story variable (`static mut`) | A constant expression | `E3010` |
| Initializer of a handle (immutable `static`) | A constant expression that makes a `Setting` or `Meta` handle | `E3010`, `E3011`, `E4007` |
| Array length `[T; N]`, repeat count `[value; N]` | A [type-level constant](#type-level-constants) | `E2002`, `E2003` |
| Bounds of `Int<lo, hi>` | Type-level constants that fit `i64`, with `lo <= hi` | `E2002`, `E2003`, `E2004` |
| Const generic arguments | Type-level constants | `E2002`, `E2003` |
| Effect of a text effect `![…](effect)` in `t![…]` | A constant expression of type `TextEffect` | `E3022` |
| A value of a non-finite type in a `pushdown fn` | A constant expression | `E3008` |
| Range bounds of a `for` loop in a `pushdown fn` | Constant expressions | `E3006` |
| Path of `asset::<A>(…)` and `system_font(…)` | A string literal | `E2028` |
| `id` of `choice!{…}` | A string literal | `E0001`, `E3019` |

### Constants in the story

The story holds only values of [finite types](../concepts/kinds.md#finite-types) at runtime, but a
constant expression of any type may appear in a `pushdown fn`, because the compiler computes it.
`render = fade_to_room`, `fade(ms(500))` and `rgb(200, 0, 0)` are all fine in a scene; a `Duration`
computed from a story variable is not (`E3008`).

In a `pushdown fn`, every largest constant subexpression is computed at compile time, so its
errors (such as an overflow or an invalid motion) are compile errors, not run-time panics. A call
of a `const fn` whose arguments are not all constant, but all of finite types, is computed at
compile time for every possible argument value instead; see
[Calling `const fn` from the story](../concepts/kinds.md#calling-const-fn-from-the-story).

## Type-level constants

Array lengths, `Int` bounds and const generic arguments are part of a type, so the compiler needs
their values while it checks types. They use a smaller language:

- integer literals;
- unary `-` and the operators `+`, `-`, `*`, `/`, `%`;
- a block `{ … }` that holds only a final expression;
- a const generic parameter by itself (`N`, but not `N + 1`);
- a path to a `const` item whose initializer is itself a type-level constant.

Anything else, including a `const fn` call, is `E2002`; so is a `const` that depends on itself.
Overflow, division by zero or an integer literal too large is `E2003`. A const generic argument
other than a literal is written in braces:

```kis
const SLOTS: usize = 4;
const MAX_LOVE: i64 = 10;

static mut FLAGS: [bool; SLOTS] = [false; SLOTS];
static mut LOVE: Int<0, { MAX_LOVE * 2 }> = 0;
```

## Const-evaluation errors

When the compiler computes a constant and the computation fails, the error is a compile error with
an `E4xxx` code. See [Constant Evaluation (E4xxx)](../../diagnostics/eval.md) for examples and fixes.

| Code | Error |
| ---- | ----- |
| [`E4001`](../../diagnostics/eval.md#e4001) | An integer result does not fit its type |
| [`E4002`](../../diagnostics/eval.md#e4002) | Division by zero, including `Rat::new(n, 0)` |
| [`E4003`](../../diagnostics/eval.md#e4003) | The evaluation ran more than 1 000 000 steps |
| [`E4004`](../../diagnostics/eval.md#e4004) | A `const fn` call from the story has more than 65 536 argument combinations to compute |
| [`E4005`](../../diagnostics/eval.md#e4005) | An index past the end of an array or `Vec` |
| [`E4006`](../../diagnostics/eval.md#e4006) | A `const` whose value depends on itself |
| [`E4007`](../../diagnostics/eval.md#e4007) | A `Setting` or `Meta` handle made outside the initializer of a `static` |
| [`E4008`](../../diagnostics/eval.md#e4008) | A malformed motion or shake |
| [`E4009`](../../diagnostics/eval.md#e4009) | An easing with parameters out of range |
| [`E4010`](../../diagnostics/eval.md#e4010) | The same text mark twice in one text |
| [`E4011`](../../diagnostics/eval.md#e4011) | `begin_text` while the timeline is anchored on a text mark |
| [`E4012`](../../diagnostics/eval.md#e4012) | A fraction outside `0.0..=1.0`, such as an opacity, a volume or an anchor |
| [`E4013`](../../diagnostics/eval.md#e4013) | A `Duration` that does not fit in `u32` milliseconds |
