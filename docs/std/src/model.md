# The System Model

## Two nested machines

The whole game is **two nested pushdown machines**, not one machine with an interleaved stack.

- The outer **system machine** has a stack of [modes](./modes.md): Title, Game, Save, Load,
  Config, Backlog, confirmation dialogs.
- Exactly one kind of mode, `Game`, carries a value of the abstract type
  [`Narrative`](./narrative.md): the narrative configuration of the syntax book (position, call
  stack, variables σ, screen, trace).

The narrative's own call stack lives *inside* that value and never on the system stack. Three
properties follow directly:

- **A save cannot contain system state.** A save is `snapshot(&Narrative)`. Menu frames and
  settings are not reachable from the value being saved, so they cannot end up in it.
- **A load replaces instead of pushing.** Loading puts a new `Narrative` into a fresh `Game`
  frame. There is no menu stack to unwind first.
- **"The game is running" is a property of the stack.** It holds exactly when a `Game` frame
  exists.

## State

| Store            | Lifetime             | Lives in                                   | In a save? |
| ---------------- | -------------------- | ------------------------------------------ | ---------- |
| Settings         | persistent, global   | the config file                            | no         |
| Meta             | persistent, global   | the meta file: read flags, unlocks, gallery | no        |
| Save slots       | persistent           | `[Option<Save>; N]`                        | they are the saves |
| Mode stack       | ephemeral            | memory, reset to the start mode at boot    | no         |
| Mode-local state | ephemeral            | the frame: selected tab, page of slots, auto and skip flags | no |
| `Narrative`      | ephemeral            | the `Game` frame                           | yes, as trace and snapshot |

## Configuration

```text
C     = (stack : NonEmpty<Frame>, settings : Settings, meta : Meta, slots : Slots)
Frame = Page(name, local) | Game(n : Narrative, hud : HudState)

C₀    = ([Page(start, init)], read(config), read(meta), read(slots))
```

The start mode is fixed by the program, so the system always starts at the same position. The
persistent stores are inputs of `C₀`, not part of that position.

## Information flow

| From → to           | Channel                                                           | Direction |
| ------------------- | ----------------------------------------------------------------- | --------- |
| Narrative → system  | `std/narrative.kix`: current line, choices, history, the screen   | read-only |
| System → narrative  | Inputs only: `NextStep`, an arm selection, `restore`, `rewind`    | inputs    |
| Settings → game     | [`Setting<T>`](./system-ctx.md) references, resolved by the engine | opaque   |
| Narrative → meta    | [`<meta-set>`](./settings-meta.md#writing-meta-from-the-narrative) | write-only |
| Meta → narrative    | [Copied into σ at `start`](./settings-meta.md#reading-meta-from-the-narrative) | opt-in |

Settings never reach narrative control flow, so every analysis of the syntax book holds for
every value of the settings.
