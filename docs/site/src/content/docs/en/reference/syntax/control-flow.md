---
title: "Control Flow and Loops"
---

Kagari's control flow is Rust's structured control flow: `if`, `if let`, `match`, `loop`, `while`,
`while let`, `for`, `break`, `continue` and `return`, with labels on loops. There is no `goto`.
What a loop may do depends on the [kind](../concepts/kinds.md) of the function it is in; the
[last section](#rules-by-function-kind) lists the rules.

## `if` and `if let`

```kis
if AFFECTION_MIO >= 3 {
    MioSmiles();
} else if AFFECTION_MIO == 0 {
    MioIgnores();
} else {
    MioShrugs();
}

let label = if cleared { "Clear" } else { "New" };
```

- The condition is a `bool` expression (`E2001` otherwise), written without parentheses. The
  branches are blocks; braces are required.
- With an `else`, `if` is an expression whose value is the value of the branch taken; both
  branches have one type. Without `else`, its type is `()`, and the block must have type `()`.
- A struct expression in the condition must be parenthesized; see
  [Expressions](./expressions.md#struct-tuple-and-array-expressions).

`if let pattern = value { … } else { … }` runs the first block when the pattern matches, with the
pattern's bindings in scope there. The pattern may be refutable. An `if let` cannot be combined
with other conditions by `&&`; nest a second `if` instead.

```kis
if let Some(slot) = last_slot {
    show_slot(slot);
}
```

## `match`

```kix
match choice!{
    id = "confess",
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] => Confess::No,
} {
    Confess::Yes => AzusaConfession(),
    Confess::No => s!(t!["I kept quiet."]),
}
```

`match value { pattern => expression, … }` runs the first arm whose pattern matches and whose
guard (`pattern if condition => …`) is true. Its value is the value of that arm; all arms have one
type. A `match` must be exhaustive (`E2037`). Patterns, guards and exhaustiveness are in
[Patterns](./patterns.md#match-and-exhaustiveness).

## `loop`

```kis
let mut n = 0;
let first_big = loop {
    n += 1;
    if n * n > 50 {
        break n;
    }
};
```

`loop { … }` repeats its body until a `break` leaves it. `break value` makes `value` the value of
the loop; all `break`s of one loop give values of one type, and a plain `break` gives `()`. A
`loop` without any `break` never ends; its type is `!`, so it fits wherever a value of any type is
expected, such as the end of a scene that returns an ending.

## `while` and `while let`

```kis
while count < 10 {
    count += 1;
}

while let Some(next) = queue.get(index) {
    index += 1;
}
```

`while condition { … }` runs the body as long as the condition is true. `while let pattern = value
{ … }` runs it as long as the pattern matches. Both have type `()`; `break` inside them takes no
value (`E2001`).

## `for` loops

```kis
for day in 0..7 { … }
for i in 1..=3 { … }
for name in ["Mio", "Azusa", "Hanako"] { … }
for score in scores { … }
```

`for pattern in iterable { … }` runs the body once per item. The pattern must be irrefutable
(`E2038`). The loop has type `()`, and `break` inside it takes no value.

| Iterable | Items | Order |
| -------- | ----- | ----- |
| `[T; N]` | The `N` elements, of type `T` | First to last |
| `Vec<T>` | The elements, of type `T` | First to last |
| `a..b`, `a..=b`: a `Range<T>` of a Rust integer or `Int` type | The integers from `a` to `b` (excluded or included) | Ascending |

Any other type is `E2031`, including references to arrays. A range over anything but integers
(for example `Decimal`) is `E2031` as well.

When the range is written in the `for` itself, the loop variable's type covers exactly its
values: `day` in `for day in 0..7` is an `Int<0, 6>`, `i` in `for i in 1..=3` is an `Int<1, 3>`,
and with `a` and `b` of type `Int<0, 9>`, `k` in `for k in a..b` is an `Int<0, 8>`. A range value
of type `Range<Int<lo, hi>>` from elsewhere, such as a parameter, gives an `Int<lo, hi>`.

## Labels, `break` and `continue`

```kis
'outer: for row in 0..4 {
    for col in 0..4 {
        if blocked(row, col) {
            continue 'outer;
        }
        if done(row, col) {
            break 'outer;
        }
    }
}
```

| Expression | Effect |
| ---------- | ------ |
| `break` | Leaves the innermost loop |
| `break value` | Leaves the innermost `loop` with a value |
| `break 'label`, `break 'label value` | Leaves the loop labelled `'label` |
| `continue` | Starts the next iteration of the innermost loop; in a `for` loop, with the next item |
| `continue 'label` | Starts the next iteration of the loop labelled `'label` |

A label is `'name:` written before `loop`, `while` or `for`. Labels exist only on loops: a label on
a block or any other expression is a syntax error (`E0001`). A label that no enclosing loop
declares is `E1019`; `break` or `continue` outside a loop is `E1020`. `break` and `continue` have
type `!`.

## `return`

`return value` leaves the function with `value`; `return` alone returns `()`. It has type `!`.
Without `return`, a function returns the value of its body block. A function with a return type
must produce a value of that type on every path; a path that reaches the end of the body without
one is a type error (`E2001`), and so is `return;` in a function that returns a value.

## Rules by function kind

The constructs above are available everywhere, with these restrictions:

| Construct | `fn` | `const fn` | Scene | Instant procedure, closure in the story |
| --------- | ---- | ---------- | ----- | --------------------------------------- |
| `if`, `if let`, `match` | yes | yes | yes | yes |
| `loop`, `while`, `while let` | yes | yes | yes, if every path around the loop passes a step (`E5001`) | no (`E3004`) |
| `for` over an array | yes | yes | yes | yes |
| `for` over a range | yes | yes | bounds must be constant expressions (`E3006`) | bounds must be constant expressions (`E3006`) |
| `for` over a `Vec<T>` | yes | yes | only a constant one (`E3006`) | only a constant one (`E3006`) |
| Recursion | yes | yes | only through a step (`E5002`) | no (`E3005`) |

In a `pushdown fn`, a `for` loop may iterate over any array, over a range whose two bounds are
[constant expressions](./constants.md), or over any other constant expression. A `Vec<T>` is not a
finite type, so the story never holds one at runtime.

When a `const fn` is evaluated at compile time, the evaluation is limited to 1 000 000 steps
(`E4003`), so a loop that does not end there is reported instead of hanging the build.

### Termination in the story

Everything the story does between two steps runs instantly, without input from the player, so it
must always finish. The language makes this checkable:

- An **instant procedure** (a `pushdown fn` without `#[scene]`) and a closure in the story have no
  unbounded loop and no recursion: no `loop` or `while` (`E3004`), `for` only over arrays and
  constant ranges (`E3006`), and no cycle of calls among instant procedures (`E3005`). So every
  call of one finishes.
- In a **scene**, `loop` and `while` are allowed, but every path around the loop body must pass a
  step (`E5001`), and a scene may call itself, directly or through other scenes, only after a
  step (`E5002`). A call of a scene that may return without passing a step counts as stepless.

```kix
static mut READY: bool = false;

#[scene]
pushdown fn Wait() {
    loop {
        if READY {
            s!(t!["Ready."]);
        }
        // E5001: when READY is false, the loop goes around without a step
    }
}
```

A hub that waits for the player on every lap is accepted:

```kix
enum Map { A, Leave }

#[scene]
pushdown fn FestivalHub() {
    loop {
        match choice!{
            id = "map",
            t!["Stall A"] => Map::A,
            t!["Leave"] => Map::Leave,
        } {
            Map::A => StallA(),
            Map::Leave => break,
        }
    }
}
```

The check counts every branch as possible; it does not try to prove that a stepless path is never
taken. A `for` loop in a scene over more than 1 048 576 items makes the story analysis give up
(`W5007`). The full rules are in
[Scenes and Instant Procedures](../concepts/scenes.md#termination).

## No `goto`

There is no `goto` and no label that marks a position in the code. A hub is a `loop`, a scene that
ends early `return`s, and a branch is an `if` or a `match`. Control can never jump into another
function or into the middle of a block or loop, so calls and returns always nest, which the story
analysis relies on.
