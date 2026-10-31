---
title: "Scenes and Instant Procedures"
---

The story is made of **scenes**, `pushdown fn` marked `#[scene]`, and **instant procedures**, the
other `pushdown fn`. Scenes call each other and return; within a scene, control flow is Rust's.
There is no `goto`.

## Calls

```kix
FestivalHub();                // call a scene
let answer = Confession();    // a scene that returns a value
visit(Map::A);                // call an instant procedure
```

All calls use ordinary call syntax. A call pushes a frame holding the return point and the
callee's locals, then runs the callee. When the callee returns, its frame is popped and the caller
continues (see [The Story Machine](./story.md#execution-model)).

## Scenes and instant procedures

```kix
#[scene]
pushdown fn StallA() {
    if VISITED_A {
        s!(t!["The stall is already packing up."]);
        return;
    }
    visit(Map::A);
    s!(who = "Hanako", t!["Welcome to stall A!"]);
}

pushdown fn visit(stall: Map) {
    match stall {
        Map::A => VISITED_A = true,
        Map::B => VISITED_B = true,
        Map::C => VISITED_C = true,
        Map::Leave => {}
    }
}
```

`#[scene]` marks a `pushdown fn` as a **scene**, the unit the story is checked in:

- Only a scene may call [`push_step`](./steps.md#steps-are-calls-of-push_step), that is, run
  `s!` and `choice!`, because a step is not pure: it suspends the story. Elsewhere this is
  `E3002`.
- Only a scene may call another scene (`E3003`). Scenes may be recursive.
- Only a scene may draw a [random value](#randomness) (`E3024`).
- The compiler analyzes each scene once, as one component of the whole story, so a scene called
  from many places is not re-analyzed per caller.

A `pushdown fn` without `#[scene]` is an **instant procedure**:

- It holds finite types, may read and write story variables, and may call other instant
  procedures and `const fn`.
- It never runs a step, never calls a scene and never draws a random value.
- It is not recursive: the calls among instant procedures form no cycle (`E3005`). Its only loops
  are `for` over a constant range or an array; `loop` and `while` are `E3004`.

So an instant procedure always finishes without input, and the compiler treats a call of one as a
single transition inside the calling scene.

| | Scene (`#[scene] pushdown fn`) | Instant procedure (`pushdown fn`) |
| - | ------------------------------ | --------------------------------- |
| Run steps (`s!`, `choice!`, `push_step`) | yes | no (`E3002`) |
| Call scenes | yes | no (`E3003`) |
| Call instant procedures and `const fn` | yes | yes |
| Draw randomness (`rng_choose`) | yes | no (`E3024`) |
| Read and write story variables, write meta | yes | yes |
| Recursion | yes, through a step | no (`E3005`) |
| `loop`, `while` | yes, every lap through a step | no (`E3004`) |
| `for` over a constant range or an array | yes | yes |

`#[scene]` on a function that is not a `pushdown fn` is `E3018`. Scenes are named in PascalCase,
which tells them apart from instant procedures at the call site; the snake_case lint does not
apply to them.

## Control flow

The story uses Rust's control flow: `if`, `if let`, `match`, `loop`, `while`, `for`, `break`,
`continue` and `return`, with loop labels for leaving nested loops. The syntax is in
[Control Flow and Loops](../syntax/control-flow.md).

```kix
#[scene]
pushdown fn Week() {
    'week: for day in 0..7 {
        match Day(day) {
            DayEnd::Normal => continue,
            DayEnd::Hospital => break 'week,
        }
    }
}
```

In a `pushdown fn`, `for` loops only over a range whose bounds are constant expressions, or over
an array (`E3006`), and the loop variable must have a finite type: `day` in `for day in 0..7`
takes the `Int` type the parameter of `Day` expects. A loop variable that nothing constrains is an
`i32`, which is not finite (`E3008`); type a bound instead, as in `for lap in 0..LAPS` with
`const LAPS: Int<0, 3> = 3;`.

There is no `goto` and no labelled position to jump to. A hub is a `loop`, a scene that ends
early `return`s, and a branch is an `if` or a `match`. Two consequences:

- Control can never jump into another function or into the middle of a block. Call/return
  nesting, which the analysis relies on, holds by construction.
- Irreducible control flow, such as a jump into the middle of a loop, cannot be written. No story
  needs it.

Conditions are ordinary boolean expressions, and several variables are matched as a tuple:

```kix
match (VISITED_A, VISITED_B, VISITED_C) {
    (true, true, true) => FestivalEnd(),
    _ => {}
}
```

In summary:

| Need | Write |
| ---- | ----- |
| Call a scene | `Name(x)` |
| Leave a scene | `return;` / `return value;` |
| Branch | `if`, `match` (exhaustive, with ranges and tuples) |
| Hub or repeated choice | `loop { match choice!{…} { … } }` with `break` |
| Bounded repetition | `for i in 0..n` |
| Change a counter | `x += 1;` |
| Write meta | `META_FIELD.write(v);` ([Entry Scenes and Endings](./entry.md#writing-meta)) |
| Draw at random (scenes only) | `rng_choose::<T>()` |
| End the story | return from the entry scene ([Entry Scenes and Endings](./entry.md#the-ending)) |

## Returning a value

```kix
enum Answer { Accepted, Rejected }

#[scene]
pushdown fn Confession() -> Answer {
    match choice!{
        id = "confess",
        t!["Tell her the truth"] => Confess::Yes,
        t!["Say nothing"] => Confess::No,
    } {
        Confess::Yes => Answer::Accepted,
        Confess::No => Answer::Rejected,
    }
}

#[scene]
pushdown fn Day3() {
    match Confession() {
        Answer::Accepted => AzusaRoute(),
        Answer::Rejected => s!(t!["The wind stopped."]),
    }
}
```

A scene that needs to report how it ended declares a return type of a finite type and returns the
outcome. Rust's typing makes every path produce a value: a path that can reach the end of the body
without one is a type error. A loop that never ends needs no value.

## Termination

Everything between two steps runs instantly, so it must always finish. The compiler rejects:

- a `loop` or `while` in a scene whose body has a path around it that does not pass through a
  step (`E5001`), and
- a cycle of calls that does not pass through a step, such as a scene that calls itself before any
  step (`E5002`).

`for` over a constant range or array is bounded and needs no step. Every branch counts as
possible: the check does not try to prove that a stepless path is never taken.

The check looks through calls. A call of a scene that *may return without passing a step* counts
as stepless, and so does every call of an instant procedure. So a loop whose only step is inside
a called scene is accepted only if that scene steps on every path.

A story that runs in a cycle, such as a day loop, is a `loop` in one driver scene that calls each
day in turn. Scenes that call each other in a ring and never return grow the call stack on every
lap.

## Closures in the story

A `pushdown fn` may take a closure parameter and call it:

```kix
pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}

twice(|| AFFECTION_MIO += 1);
```

Closures in the story are [second-class](../syntax/values.md): they can be passed and called, but
not stored, returned or captured (`E3015`). A closure is an instant procedure: it cannot run a
step or call a scene.

## Randomness

A scene draws a random value with `rng_choose::<T>()`, where `T: RandChoose`: a fieldless enum or
an `Int<lo, hi>`. Each value has equal probability. Only a scene may draw; a call from an instant
procedure is `E3024`, and a `T` that is not `RandChoose` is `E2026`.

```kix
enum Coin { Heads, Tails }

match rng_choose::<Coin>() {
    Coin::Heads => s!(t!["Heads."]),
    Coin::Tails => s!(t!["Tails."]),
}
```

The generator is xoroshiro128++. Its state is part of the narrative state and its seed is in the
save, so replay reproduces every draw (see [Saves, Replay and Determinism](./saves.md)). The
compiler treats each draw as a free choice over every value of `T`, like a choice of the player.
