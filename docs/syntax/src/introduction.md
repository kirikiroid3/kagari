# Introduction

This book describes the source language of the Kagari visual novel engine.

A Kagari game is written in one Rust-like language. The story, its presentation and the game
system around it (title screen, menus, saves, settings) are all functions in that language. What
separates them is the **kind** of each function:

| Kind          | Power                                   | Used for |
| ------------- | --------------------------------------- | -------- |
| `pushdown fn` | Pushdown control over finite data       | The story: scenes, steps, choices, story variables |
| `const fn`    | Turing-complete, pure, evaluated at compile time where needed | Presentation (render functions), constants, helpers |
| `fn`          | Turing-complete                         | The game system: UI, menus, saves, settings |

The compiler checks which kind may call which (see [Function Kinds](./kinds.md)). Because the
story is made only of `pushdown fn`, and Turing-complete code can enter it only by starting an
entry scene, every narrative analysis of the engine proposal stays decidable while the rest of the
game is free to compute.

Source files come in two flavours that differ only in syntax:

| Extension | Syntax |
| --------- | ------ |
| `.kis`    | The Rust-like language |
| `.kix`    | `.kis` plus [forms](./narrative/forms.md) such as `s!(…)`, `choice!{…}` and `t![…]`, short ways to write steps, text and UI |

All files of a game are compiled together into a single `.kagari` bytecode file.

The design follows the engine proposal (*An Analyzable Model for a Visual Novel Engine*):
narrative control is a pushdown system over finite data, and presentation is a function that the
narrative never reads back. [Relation to the Proposal](./proposal.md) maps the proposal's concepts
to the language and records what changed from the first design.

## A first look

```kix
static mut VISITED_A: bool = false;

#[scene]
#[entry]
pushdown fn Day1() -> Ending {
    s!(t!["The school festival starts today."]);
    StallA();
    Ending::Normal
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

- `Day1` is an [entry scene](./narrative/scenes.md#entry-scenes): the system starts the story
  there, and its return value tells the system which ending was reached.
- Each `s!` is a **step**, a point where the engine stops and waits for the player. `t![…]` is its
  text.
- `StallA()` calls the scene `StallA` and continues when it returns.
- `hanako_jumps` is a [render function](./presentation.md): it says how the screen changes when
  the step is entered.

## Conventions

- *Tentative* marks syntax that this book fills in where the design is not settled yet: names,
  defaults, and edge-case rules. Each tentative item is either harmless to rename or is listed in
  [Open Questions](./open-questions.md).
- Code blocks tagged `kis` are plain Rust-like code; blocks tagged `kix` use forms.
- "Step" always means a point where the engine waits for player input.
- σ ("sigma") is the narrative state: position, call stack and story variables.
