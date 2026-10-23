# Scenes and Control Flow

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
continues.

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

- Only a scene may call [`push_step`](./forms.md#steps-are-calls-of-push_step), that is, run
  `s!` and `choice!`, because a step is not pure: it suspends the story.
- Only a scene may call another scene. Scenes may be recursive.
- Each scene is one component of the recursive state machine the analyzer checks, and each call of
  a scene is a box in it. The analyses summarize each scene once, so a scene called from many
  places is not re-analyzed per caller.

A `pushdown fn` without `#[scene]` is an **instant procedure**:

- It holds finite types, may read and write story variables, and may call other instant
  procedures and `const fn`.
- It never runs a step and never calls a scene.
- It is not recursive: the calls among instant procedures form no cycle. Its only loops are `for`
  over a constant range or array; `loop` and `while` are errors.

So an instant procedure always finishes without input, and the analyzer treats a call of one as a
single transition inside the calling scene.

`#[scene]` is an error on a function that is not a `pushdown fn`. Scenes are named in PascalCase,
which tells them apart from instant procedures at the call site; the snake_case lint does not
apply to them.

## Control flow

The story uses Rust's control flow: `if`, `if let`, `match`, `loop`, `while`, `for`, `break`,
`continue` and `return`, with Rust's loop labels for leaving nested loops.

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

`0..7` is a `Range<T>` from the standard library. In a `pushdown fn`, `for` loops only over a
range whose bounds are constant expressions, or over an array, and the loop variable has the
matching `Int` type: `day` in `for day in 0..7` is an `Int<0, 6>`.

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

- a `loop` or `while` whose body has a path around it that does not pass through a step, and
- a cycle of calls that does not pass through a step, such as a scene that calls itself before any
  step.

`for` over a constant range or array is bounded and needs no step. Every branch counts as possible:
the check does not try to prove that a stepless path is never taken.

The check is interprocedural. For each scene the compiler computes whether it *may return without
passing a step*; a call to such a scene counts as stepless, and so does every call of an instant
procedure. The computation is a least fixpoint over the call graph, linear in the size of the
program.

A story that runs in a cycle, such as a day loop, is a `loop` in one driver scene that calls each
day in turn. Scenes that call each other in a ring and never return grow the call stack on every
lap.

## Entry scenes

```kix
enum Ending { NormalEnd, AzusaEnd }

#[scene]
#[entry]
pushdown fn Main(cleared_azusa: bool) -> Ending {
    …
    if ROUTE == Route::Azusa {
        return Ending::AzusaEnd;
    }
    Ending::NormalEnd
}
```

An **entry scene** is a scene also marked `#[entry]`. It is where the system can start the story,
and it is the only door from Turing-complete code into the story.

- A game has at least one entry scene. Extra chapters and after stories are further entry scenes.
- Its parameters and return type are finite. The system passes the arguments when it starts the
  story (see [Starting the story](../system.md#starting-the-story)).
- `#[entry]` is an error without `#[scene]`. An entry scene is an ordinary scene otherwise: other
  scenes may also call it.
- Entry scenes are the roots of every narrative analysis.

### Arguments

Entry arguments are how the story reads cross-playthrough state, such as an ending cleared in an
earlier run. The system reads the [meta field](../system.md#settings-and-meta) and passes its value
when it starts the story. The arguments are stored in the save with the trace, so loading replays
with the same values even if meta changed since. The analyzer treats them as free: every analysis
holds for every argument value.

### The ending

When the entry scene returns, the story is over and control goes back to the system. The return
value tells the system which ending was reached, and the system's exit handler decides what comes
next, such as rolling the credits or returning to the title. Only the value crosses over: the
system cannot see story variables.

Because every path of `Main` must return an `Ending`, a path that forgets its ending is a type
error. Because the exit handler is an exhaustive `match`, an ending the system does not handle is a
compile error too.

## Writing meta

```kix
CLEARED_AZUSA.write(true);
```

The story writes a [meta field](../system.md#settings-and-meta) with `write`, a `pushdown fn`
method of `Meta<T>`. Meta is **write-only** from the story: reading it is `fn`-only. A write the
story can never read back cannot change its control flow, so every narrative analysis is
unaffected. Writes are not reverted by load or rewind: unlocks stay unlocked.

## Closures in the story

A `pushdown fn` may take a closure parameter and call it:

```kix
pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}

twice(|| AFFECTION_MIO += 1);
```

Closures in the story are [second-class](../values.md#closures): they can be passed and called, but
not stored, returned or captured. A closure is an instant procedure: it cannot run a step or call
a scene.

## Randomness

The story may use a deterministic pseudo-random generator whose state is part of σ, so replay
reproduces every draw. The API is an [open question](../open-questions.md#randomness).
