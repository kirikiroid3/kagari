# Modes and the Mode Stack

A **mode** is one screen of the game system. The system machine holds a stack of mode frames.

## Base and overlays

The stack splits as `stack = base :: overlays`:

- The **base** frame is either the `Game` mode or a **root page** (Title, Gallery, Credits).
- **Overlays** are pages pushed on top of the base (Save, Load, Config, Backlog, confirmations).

Only `interface__goto`, `metafn__start` and `metafn__load` change the base, and each of them resets the whole stack. This
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
- A handler attribute such as `on-click` holds one call: of a system function, or of one of the
  [operations](#operations) directly. A system function body runs operations as tags such as
  `<interface-call page="save" />`.

### Game mode

`<game-mode>` owns the `Narrative` value. Its `<on-exit>` handler runs when the entry scene
returns. It receives the `Ending` value the entry scene returns, matches on it, and must be
exhaustive, like a `<match>` in the story.
If the matching case runs no `interface__goto`, `metafn__start`, `metafn__load` or
`metafn__quit`, the stack resets to `[start]`, the root page named by `<system start>`.

## Operations

Handlers call these operations, directly or through system functions. There is no other way for
the UI to change state. Each operation is written `metafn__x(…)` or `interface__x(…)` in an
attribute, and as a `<metafn-x>` or `<interface-x>` tag in a system function; the syntax book's
*System Script* chapter gives both forms.

| Op                                | Effect on the configuration                          | Typing condition |
| --------------------------------- | ---------------------------------------------------- | ---------------- |
| `interface__call(m)`              | `stack := stack ++ [m]`                              | `m` is an overlay, and `m.requires ⊆ ctx(stack)` |
| `interface__return()`             | drops the top frame                                  | the top is an overlay, never the base |
| `interface__goto(m)`              | `stack := [m]`                                       | `m` is a root page |
| `metafn__start(e)`                | `stack := [Game(new(e), hud₀)]`                      | `e` is an entry scene of `<game>` |
| `metafn__load(k)`                 | `stack := [Game(restore(slots[k]), hud₀)]`           | none: the same op from Title and from in-game |
| `metafn__save(k)`                 | `slots[k] := snapshot(game(stack))`                  | `game ∈ ctx(stack)` |
| `metafn__advance()`               | `game := step(game, NextStep)`                       | `game ∈ ctx(stack)`, the top is `Game` |
| `metafn__select(arm)`             | `game := step(game, arm)`                            | as `metafn__advance` |
| `metafn__revert(entry.index)`     | `game := rewind(game, i)`, then `interface__return`  | see [Backlog](./backlog.md) |
| `metafn__play_voice(entry.voice)` | plays a voice line; no state change                  | `game ∈ ctx(stack)` |
| `metafn__quit()`                  | terminates the game immediately, with no confirmation | — |
| `interface__focus_move(x)`        | moves the focus of the top frame                     | see [UI](./ui.md#focus-and-input) |
| `interface__focus_activate()`     | activates the focused element                        | — |

`metafn__quit` never asks. A button that should confirm calls `interface__call(quit)` on a
`<dialog name="quit">` the studio declares. Settings are changed with `<set>` in a system
function, or by a control's `bind`.

`metafn__load(k)` fails when `restore` reports a divergence. The divergence is shown, and the
stack is unchanged.

### Context typing

`ctx(stack)` is the **context index** of a stack: `{game}` when the base is `Game`, `{}`
otherwise. `interface__call` and `interface__return` never change the base, so `ctx` is invariant
along them, and only `interface__goto`, `metafn__start` and `metafn__load` move between contexts.

In type terms, modes are indexed by the context:

```text
Mode  : Ctx → Type
interface__call : Mode c → Stack c → Stack c
metafn__start   : Entry → Stack {game}
metafn__load    : Slot  → Stack {game}
interface__goto : RootPage → Stack {}
```

So `interface__call(save)` from the title page is a compile error: `ctx([title]) = {}`, and
`save` requires `{game}`.
