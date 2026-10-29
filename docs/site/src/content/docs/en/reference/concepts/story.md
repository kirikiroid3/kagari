---
title: "The Story Machine"
---

The **story** is the set of `pushdown fn` of a game. This chapter gives its execution model and
the story variables. [Steps](./steps.md) describes what the player sees, and
[Scenes and Instant Procedures](./scenes.md) how the story moves.

## Execution model

The engine runs the story as a pushdown machine:

- The **position** is a scene plus a point in its body.
- The **call stack** holds one frame per active call of a `pushdown fn`: the point to return to
  and that call's locals.
- The **variables** are the locals of the frames and the [story variables](#story-variables). All
  have [finite types](./kinds.md#finite-types).

Together these are σ (sigma), the **narrative state**. Code falls into two groups:

| Kind | Examples | Waits for input |
| ---- | -------- | --------------- |
| Steps | `s!(…)`, `choice!{…}` | Yes |
| Everything else | calls, `if`, `match`, loops, assignments, `return` | No |

When the story reaches a step it runs the step's [render](./presentation.md), shows the step, and
suspends. Input resumes it: `NextStep` for `s!`, an arm selection for `choice!`. Everything
between two steps runs instantly. Because data is finite and every loop and every cycle of calls
must pass through a step ([Termination](./scenes.md#termination)), the code between two inputs
always finishes, and questions about which steps can be reached in which states can be decided by
the compiler.

Presentation is one-way: the story chooses renders, but no story code can read the screen.

## Story variables

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
static mut MET_MIO: bool = false;
```

A `static mut` is a **story variable**: global narrative state that lives for one run of the
story.

- Its type must be [finite](./kinds.md#finite-types) (`E3008`), and its initializer a
  [constant expression](../syntax/constants.md) (`E3010`). Every run starts from the initializer,
  so a read can never see an unset variable.
- Only a `pushdown fn` may read or write it. `fn` and `const fn` code naming a story variable is
  `E3007`, which is what keeps the system from reading the story.
- Its type cannot contain a reference (`E3012`).
- It is written without `unsafe`: the story is single-threaded.

Locals of a `pushdown fn` are ordinary `let` and `let mut` bindings. They live in the call's
frame, and a recursive call gets its own copies.

```kis
enum Route { None, Hanako, Azusa, Both }
```

Enums are ordinary Rust enums. Variants may carry data, as long as the data is finite.

An immutable `static` is not a story variable: it must hold a `Setting<T>` or `Meta<T>` handle
(`E3011`); see [The Game System](./system.md).

## Example: the festival hub

Stalls visited from a map, each stall remembering whether it was visited. Scenes `StallB`,
`StallC` and `DormNight` are omitted.

```kix
static mut VISITED_A: bool = false;
static mut VISITED_B: bool = false;
static mut VISITED_C: bool = false;

enum Ending { Normal }
enum Map { A, B, C, Leave }

#[scene]
#[entry]
pushdown fn Day1() -> Ending {
    s!(t!["The school festival starts today."]);
    FestivalHub();
    DormNight();
    Ending::Normal
}

#[scene]
pushdown fn FestivalHub() {
    loop {
        match choice!{
            id = "map",
            t!["Go to stall A"] => Map::A,
            t!["Go to stall B"] => Map::B,
            t!["Go to stall C"] => Map::C,
            t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
        } {
            Map::A => StallA(),
            Map::B => StallB(),
            Map::C => StallC(),
            Map::Leave => break,
        }
    }
}

#[scene]
pushdown fn StallA() {
    if VISITED_A {
        s!(t!["The stall is already packing up."]);
        return;
    }
    VISITED_A = true;
    s!(who = "Hanako", render = hanako_jumps, t!["Welcome to stall A!"]);
}

const fn hanako_jumps(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

- `choice!` is one step. It evaluates to the variant of the arm the player selected, and an
  ordinary `match` branches on it.
- `Leave` is offered only after all three stalls were visited (the `if` guard).
- Each stall returns to the hub's loop, which shows the map again. `break` leaves the loop, the
  hub returns, and `Day1` continues with `DormNight()`.
- Every path around the loop passes through the `choice!` step, so the loop is allowed
  ([Termination](./scenes.md#termination)).

Because the stack remembers where to return, `FestivalHub` can be called from `Day1` and from a
`Day2` and returns to the right place each time, without being duplicated:

<!-- diagram: story-call-stack (mermaid-ish)
flowchart LR
  subgraph day1["Day1"]
    d1hub["FestivalHub()"] ==> dorm["DormNight()"]
  end
  subgraph day2["Day2"]
    d2hub["FestivalHub()"] ==> check["EndingCheck()"]
  end
  subgraph hub["#[scene] pushdown fn FestivalHub"]
    start(["start"]) ==> loop["loop: map and stalls"] ==> ret(["return"])
  end
  d1hub -.->|"call: push day1"| start
  d2hub -.->|"call: push day2"| start
  ret -.->|"return: pop"| dorm
  ret -.->|"return: pop"| check
  subgraph stack["stack inside StallA on day 2 (top first)"]
    s1["StallA: current scene"]
    s2["FestivalHub: resume after StallA()"]
    s3["Day2: resume after FestivalHub()"]
  end
-->
{{#include ../diagrams/story-call-stack.svg}}
