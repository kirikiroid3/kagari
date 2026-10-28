# Architecture

This chapter is the big picture: how a Kagari game goes from source files to a running game, what
each part of a game is written in, and how the parts may talk to each other. Later chapters define
each piece precisely.

## From source to running game

<!-- diagram: architecture-pipeline (mermaid-ish)
flowchart LR
  subgraph project["kagari.toml project"]
    src["src/: .kis, .kix"]
    assets["assets/: images, audio, fonts"]
  end
  project ==> build["kagari build: check, analyze, emit, pack assets"]
  build ==> diags["diagnostics (kagari check, editor)"]
  subgraph out["target/v0/"]
    kagari["game.kagari (bytecode)"]
    archives["*.kiarchive (asset archives)"]
  end
  build ==> out
  out ==> runtime["desktop runtime, profile native_v0"]
  runtime <==> data["data directory: settings, meta, saves"]
  runtime ==> game["running game"]
-->
{{#include ../diagrams/architecture-pipeline.svg}}

**The project.** A game is a [project](../tools/projects.md): a directory with a `kagari.toml`
manifest, source files under `src/`, and assets under `assets/`. Compilation starts at one root
file, `src/main.kis` by default, and follows its `mod` declarations to every other source file. A
source file is either `.kis` (the plain language) or `.kix` (the language plus
[forms](../syntax/forms.md) such as `s!(…)` and `choice!{…}`); the extension selects the syntax
only, never the meaning. Assets are named in the source by constants such as
`const BG_ROOF: ImageAsset = asset("bg/roof.png");`, and the compiler checks that each file exists.

**The build.** [`kagari build`](../tools/cli.md) compiles the whole program at once. It parses and
resolves every file, checks types and [function kinds](../concepts/kinds.md), evaluates constants
and renders, analyzes the story and the game system, and emits the program. Any error stops the
build; warnings do not. The result goes to `<out-dir>/<target>/`, `target/v0/` by default:

| Output | Content |
| ------ | ------- |
| `game.kagari` | The compiled program: story, renders and game system |
| `*.kiarchive` | The assets the program references, one archive per top-level directory of the asset directory, `data.kiarchive` for files directly in it, and `system-fonts.kiarchive` for installed fonts the game names |

Before it writes anything, the build checks that every runtime profile listed in the manifest can
decode every referenced asset. `kagari check` reports the same diagnostics without writing output,
and the language server shows them in the [editor](../tools/editors.md) as you type.

**The runtime.** The [desktop runtime](../tools/runtime.md), `kagari-desktop-runtime`, plays a
built game on Windows, macOS and Linux. It loads `game.kagari` from the game directory (by
default, the directory of the executable, so a shipped game puts the executable next to
`game.kagari` and the archives), verifies the program before running any of it, runs `fn main`
once to get the description of the game system, and shows the start page. Settings, meta and save
slots go to a data directory outside the game directory. The runtime implements the runtime
profile `native_v0`: the whole standard library except video and shader assets. A
[headless mode](../tools/headless.md) runs the same game without a window, driven by a script, for
automated tests.

## What each part of a game is written in

A game has three parts, and each is written in one [kind](../concepts/kinds.md) of function:

| Part | Kind | Contains |
| ---- | ---- | -------- |
| The story | `pushdown fn` | Scenes (`#[scene]`), instant procedures, [story variables](../concepts/story.md) (`static mut`); the [steps](../concepts/steps.md) where the player reads and chooses |
| The presentation | `const fn` | [Render functions](../concepts/presentation.md) that change the screen at each step, staging helpers such as character recipes, text effects, constants |
| The game system | `fn` | `fn main`, the title page, menus, save and load pages, settings, the backlog, the game view: everything around the story |

Types, constants, `static` handles for settings and meta, and asset constants are ordinary items
shared by all three parts. A module may mix kinds freely; the kind belongs to the function, not to
the file.

<!-- diagram: architecture-layers (mermaid-ish)
flowchart TB
  system["Game system: fn (main, title, menus, save and load, settings, game view)"]
  story["Story: pushdown fn (scenes, steps, choices, story variables)"]
  pres["Presentation: const fn (renders, staging helpers, text effects, constants)"]
  system ==>|"starts an entry scene: nav::start, finite arguments"| story
  system <==>|"Narrative handle: input, lines, saves"| story
  story ==>|"ending value; write-only meta"| system
  story ==>|"picks a render for each step"| pres
  pres -.->|"never: renders see only the screen; the story never reads it"| story
  system ==>|"may call const fn"| pres
-->
{{#include ../diagrams/architecture-layers.svg}}

The [call rules](../concepts/kinds.md#call-rules) fix which arrows exist:

- `fn` may call `fn` and `const fn`. It cannot call a `pushdown fn` and cannot read a story
  variable.
- `pushdown fn` may call `pushdown fn` and `const fn`, never `fn`. Every value it holds at run time
  has a [finite type](../concepts/kinds.md#finite-types).
- `const fn` may call only `const fn`. It cannot read story variables, settings, meta or anything
  else outside its arguments.

The rules hold through function pointers and closures too, because their types carry the kind.

## Two nested machines

At run time a game is two machines, one inside the other.

- The outer **system machine** keeps a stack of [modes](../concepts/system.md): a root page such
  as the title or a gallery, or the game mode, at the base; overlays such as Config, Save or
  Backlog, and dialogs, pushed on top. Input goes to the top mode only.
- The **game mode** holds one running story: the [story machine](../concepts/story.md). It is a
  pushdown machine: its state, σ, is the position in a scene, the call stack of the scenes and
  instant procedures that are running, and the story variables. With it the game mode keeps the
  current screen and the trace of the player's choices.

The story's call stack lives inside the game mode, never on the mode stack. So a save holds the
running story and nothing of the menus around it; loading a save replaces the game mode instead of
pushing onto the stack; and "the game is running" means exactly that the stack has a game mode.
While an overlay or dialog is on top of the game mode, the story is paused.

| State | Lifetime | In a save? |
| ----- | -------- | ---------- |
| Settings | Persistent, global | No |
| Meta: unlocks, read text, seen images | Persistent, global | No |
| Save slots | Persistent | They are the saves |
| The mode stack and the state of each mode | Until the game quits | No |
| The running story: σ and the screen | Until the story ends or another replaces it | Yes, as the trace that reaches it |

## How the parts talk

Information flows one way through the three parts. The story decides what happens, the
presentation decides how it looks, and the game system decides everything around it; no part reads
back what a later part made of its output.

### From the story to the presentation

Each step names its render: `s!(render = roof, …)`. When the story reaches the step, the engine
runs the render on the current screen and shows the result. The render receives only the screen,
so it cannot depend on story variables, and the story cannot read the screen, because `Screen` is
not a finite type. Presentation that depends on the story is chosen by the story, by passing a
different render in each branch.

### Between the game system and the story

The game system and the story meet at four points only:

| Point | Direction | What passes |
| ----- | --------- | ----------- |
| [Entry scenes](../concepts/entry.md) | system to story | `nav::start(entry, args, on_exit)` starts a story at an `#[entry]` scene with arguments of finite types. This is the only way into the story |
| The [`Narrative` handle](../concepts/system.md#the-narrative-handle) | both ways | The game view and game overlays get a handle to the running story. Through it they show the current line, choice and backlog, and they advance, select an arm, save or rewind. They cannot read story variables through it |
| [Endings](../concepts/entry.md) | story to system | When the entry scene returns, its value goes to the exit handler `on_exit`, which must handle every possible ending |
| Meta | story to system | The story writes meta with `Meta::write` and never reads it back during the run. The system reads meta and may pass it to the next story as an entry argument |

Settings never reach the story. A render that depends on a setting does not read its value; it
gates an entity on the setting, and the engine applies the setting when it draws.

## A run of a game

1. The runtime loads and verifies `game.kagari`, runs `fn main`, and shows the start page given
   in the `System` record it returns.
2. Pages, overlays and dialogs are components: plain `fn` that build UI. A button handler calls a
   navigation function such as `nav::call(Config)` or `nav::start(…)`.
3. `nav::start` replaces the mode stack with a new game mode running the entry scene.
4. The story runs instantly up to its first step. The step's render changes the screen, the game
   view shows the line, and the story waits.
5. Player input resumes the story: advancing past an `s!` step, or selecting an arm of a
   `choice!`. The story runs to the next step. The engine autosaves at each choice.
6. When the entry scene returns, the exit handler receives the ending and navigates, for example
   to the credits.

The [compiler's analyses](./philosophy.md) cover steps 3 to 6 for every possible run: every entry
scene, every value of its arguments and every sequence of choices.
