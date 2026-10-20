# Modes and the Mode Stack

A **mode** is one screen of the game system. The system machine holds a stack of mode frames.

## Base and overlays

The stack splits as `stack = base :: overlays`:

- The **base** frame is either the `Game` mode or a **root page** (Title, Gallery, Credits).
- **Overlays** are pages pushed on top of the base (Save, Load, Config, Backlog, confirmations).

Only `goto`, `start` and `load` change the base, and each of them resets the whole stack. This
gives a simple answer to "which UI goes along with the game":

| UI                                           | Is                                   | Lifetime |
| -------------------------------------------- | ------------------------------------ | -------- |
| Text frame, quick menu, choice buttons       | layers of the `Game` mode            | the `Game` frame |
| Title screen, gallery                        | root pages                           | never on the stack with a `Game` frame |
| Config, Load                                 | overlays with no requirement         | pushed from anywhere |
| Save, Backlog                                | overlays with `requires="game"`      | only above a `Game` base |

## Pages and layers

| Kind      | Where it lives          | Input |
| --------- | ----------------------- | ----- |
| **Page**  | a frame                 | the top frame gets all input |
| **Layer** | inside one frame's view | routed within the frame |

Input goes to the top frame only, so every frame below the top is blocked, whatever is drawn.

A **dialog** is not a separate kind. It blocks everything under it exactly as a page does, so the
two are logically the same; the only difference is visual. `<dialog>` is a page whose view is
drawn over a still image of the frames below it, where `<page>` hides them. Both are `Page` frames
in the configuration.

- Save, Load and Config are pages. Pushed over `Game`, they pause the narrative but keep it
  alive, which is why Save can read it.
- "Overwrite this slot?" and an in-game menu over a dimmed game are dialogs.
- The text frame and the quick menu are layers of `Game`.

While any page or dialog is on top of `Game`, the screen's animations are skipped to their final
state (see [Overlays and animations](./narrative.md#overlays-and-animations)), so the frames below
are always still.

*Tentative:* UI above *every* frame, such as a notification toast, is an **ambient layer**
declared directly in `<system>`. It is the only UI outside the stack discipline, so it should be
rare.

## Declaring modes

Modes are declared in `<system>` as `<page>`, `<dialog>` and one `<game-mode>`, and a mode's view
is split into named `<layer>`s. The syntax book's *System Script* chapter gives the elements, their
attributes and an example. In terms of this chapter:

- A `<page root="true">` is a root page. Every other `<page>` and every `<dialog>` is an overlay.
- `requires="game"` on an overlay is its context requirement, checked as in
  [Context typing](#context-typing).
- An interaction attribute such as `on-click` holds one of the [operations](#operations).

### Game mode

`<game-mode>` owns the `Narrative` value. Its `<on-exit>` handler runs when the entry scene
returns. It matches on the entry's exported `exit` variable and must be exhaustive, like a
`<match>` in the story.

## Operations

UI attributes such as `on-click` name one of these operations. There is no other way for the UI to
change state.

| Op              | Effect on the configuration                          | Typing condition |
| --------------- | ---------------------------------------------------- | ---------------- |
| `push(m)`       | `stack := stack ++ [m]`                              | `m` is an overlay, and `m.requires ⊆ ctx(stack)` |
| `pop`           | drops the top frame                                  | the top is an overlay, never the base |
| `goto(m)`       | `stack := [m]`                                       | `m` is a root page |
| `start(e)`      | `stack := [Game(new(e), hud₀)]`                      | `e` is an `<entry>` of `<game>` |
| `load(k)`       | `stack := [Game(restore(slots[k]), hud₀)]`           | none: the same op from Title and from in-game |
| `save(k)`       | `slots[k] := snapshot(game(stack))`                  | `game ∈ ctx(stack)` |
| `advance`       | `game := step(game, NextStep)`                       | `game ∈ ctx(stack)`, the top is `Game` |
| `select(arm)`   | `game := step(game, arm)`                            | as `advance` |
| `revert(i)`     | `game := rewind(game, i)`, then `pop`                | see [Backlog](./backlog.md) |
| `set(x, v)`     | `settings.x := v`                                    | `v` has the type of setting `x` |
| `play_voice(v)` | plays a voice line; no state change                  | `v : VoiceId` |
| `quit`          | terminates the game immediately, with no confirmation | — |
| `ask_quit`      | `push(quit)`: pushes the quit-confirmation dialog    | — |

`quit` never asks. A button that should confirm uses `ask_quit`, which pushes the dialog named
`quit`. *Tentative:* `std/ui.kix` provides a default `quit` dialog whose confirm button runs
`quit`; a studio replaces it by declaring its own `<dialog name="quit">`.

`load(k)` fails when `restore` reports a divergence. The divergence is shown, and the stack is
unchanged.

### Context typing

`ctx(stack)` is the **context index** of a stack: `{game}` when the base is `Game`, `{}`
otherwise. `push` and `pop` never change the base, so `ctx` is invariant along them, and only
`goto`, `start` and `load` move between contexts.

In type terms, modes are indexed by the context:

```text
Mode  : Ctx → Type
push  : Mode c → Stack c → Stack c
start : Entry → Stack {game}
load  : Slot  → Stack {game}
goto  : RootPage → Stack {}
```

So `push(save)` from the title page is a compile error: `ctx([title]) = {}`, and `save` requires
`{game}`.
