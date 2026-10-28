[日本語](../../ja/src/introduction.md)

# Introduction

This book, *The Kagari Reference*, describes how to write and ship a visual novel with Kagari:
the concepts of the engine, the language, its special types and attributes, the tools, the
runtime, and the diagnostics of the compiler.

## What Kagari is

Kagari is a visual novel engine. It consists of:

- **a language**, close to Rust, in which the whole game is written: the story, its presentation,
  and the game system around it (title screen, menus, saves, settings);
- **a compiler**, the `kagari` command, that checks a project, analyzes its story and builds it
  into one `.kagari` file plus asset archives;
- **a runtime** that plays the built game on Windows, macOS and Linux.

The parts of a game differ in the **kind** of function they are written in:

| Kind          | Power                                   | Used for |
| ------------- | --------------------------------------- | -------- |
| `pushdown fn` | Pushdown control over finite data       | The story: scenes, steps, choices, story variables |
| `const fn`    | Turing-complete, pure, evaluated at compile time where needed | Presentation (render functions), constants, helpers |
| `fn`          | Turing-complete                         | The game system: UI, menus, saves, settings |

The compiler checks which kind may call which. Because the story is made only of `pushdown fn`, and
Turing-complete code can enter it only by starting an entry scene, the compiler can answer
questions about the story that no tool can answer for arbitrary code: whether every choice offers
an option, whether a scene can be reached, whether a step shows the same screen however it is
reached. [Philosophy: Analyzable by Construction](./overview/philosophy.md) explains why; the rest
of the game stays free to compute.

## Who this book is for

The book is for studios that make games with Kagari: scenario writers who write scenes, scripters
who stage them, and engineers who build the game system and ship the game. It assumes basic
programming knowledge. Knowing Rust helps, since the language reuses Rust's syntax and many of its
rules; [Differences from Rust](./syntax/rust-differences.md) lists where Kagari departs from it.

## How this book is organized

| Part | Contents |
| ---- | -------- |
| Overview | The [architecture](./overview/architecture.md) of a Kagari game, from source files to the running game, and the [philosophy](./overview/philosophy.md) behind the design |
| Core Concepts | The model the language implements: [function kinds](./concepts/kinds.md), [the story machine](./concepts/story.md), [steps](./concepts/steps.md), [scenes and instant procedures](./concepts/scenes.md), [entry scenes and endings](./concepts/entry.md), [presentation](./concepts/presentation.md), [the game system](./concepts/system.md), and [saves, replay and determinism](./concepts/saves.md) |
| Syntax | The language, from [lexical structure](./syntax/lexical.md) to [forms](./syntax/forms.md) and [constant expressions](./syntax/constants.md) |
| Special Types, Traits and Attributes | [Builtin types](./special/builtin-types.md), the fixed set of [language traits](./special/traits.md), and [attributes](./special/attributes.md) |
| Building and Running | [Projects](./tools/projects.md), [the `kagari` CLI](./tools/cli.md), [editor support](./tools/editors.md), [the runtime](./tools/runtime.md) and [headless mode](./tools/headless.md) |
| Appendices | A [quick reference](./appendix/quick-reference.md), a [glossary](./appendix/glossary.md), and every [diagnostic](./diagnostics/index.md) the compiler reports |

Readers new to Kagari should read the Overview and Core Concepts in order, then use the Syntax
part as a reference. Each concept chapter links to the syntax it uses.

## A first look

```kix
static mut VISITED_A: bool = false;

const STALL_A: ImageAsset = asset("bg/stall_a.png");

enum Ending { Normal }

#[scene]
#[entry]
pushdown fn Day1() -> Ending {
    s!(t!["The school festival starts today."]);
    StallA();
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
    s!(who = "Hanako", render = at_stall_a, t!["Welcome to stall A!"]);
}

const fn at_stall_a(mut screen: Screen) -> Screen {
    screen.background().change(STALL_A).apply_animation(fade(ms(500)));
    screen
}
```

- `Day1` is an [entry scene](./concepts/entry.md): the game system starts the story there, and its
  return value tells the system which ending was reached.
- Each `s!(…)` is a [step](./concepts/steps.md), a point where the story stops and waits for the
  player. `t![…]` is its text, and `who` names the speaker.
- `StallA()` calls the [scene](./concepts/scenes.md) `StallA`; the story continues after the call
  when the scene returns. The second visit shows the other line.
- `VISITED_A` is a [story variable](./concepts/story.md): global state of one run of the story,
  of a finite type.
- `at_stall_a` is a [render function](./concepts/presentation.md): it says how the screen changes
  when its step is entered, here a half-second crossfade to a new background.

A complete game also has a `fn main` that describes the [game system](./concepts/system.md) and a
[project](./tools/projects.md) around the sources. `kagari init` creates both from a template:

```sh
kagari init my-game
cd my-game
kagari build
kagari-desktop-runtime target/v0
```

## Conventions

- *Tentative* marks design that is not settled yet: names, defaults and edge-case rules that may
  change in a later version.
- Code blocks tagged `kis` are plain code, valid in both `.kis` and `.kix` files. Code blocks
  tagged `kix` use [forms](./syntax/forms.md) such as `s!(…)`, `choice!{…}` and `t![…]`, which are
  allowed only in `.kix` files.
- **Step** always means a point where the story waits for player input: an `s!(…)` or a
  `choice!{…}`. Everything between two steps runs without waiting.
- **σ** ("sigma") is the narrative state: the position in the story, the call stack with the locals
  of each frame, and the story variables.
- Diagnostics are named by their code, such as `E5003` or `W5004`; [Diagnostics](./diagnostics/index.md)
  lists them all.
- A **studio** is the team that writes a game; **the engine** is the runtime that plays it.

## Other books

- [*Kagari STD Reference*](../../../std/en/src/about.md) documents every item of the standard
  library `std`: the screen, animations, audio, text, the system, saves and UI.
- [*Kagari Develop Guide*](../../../dev/src/introduction.md) is for contributors to Kagari itself:
  the design decisions, compiler internals, file formats and runtime internals.
