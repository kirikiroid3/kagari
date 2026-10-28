# Saves, Replay and Determinism

A save in Kagari is not a dump of memory. It is the record of the player's selections from the
start of the story, and loading replays them. This chapter explains what a save contains, what a
studio may change in a patch without breaking existing saves, and why replay is exact.

## A save is a witness of reachability

The story is a [pushdown machine](./story.md) whose only inputs are the player's selections and the
draws of its random number generator. Given the entry scene, its arguments, the generator's seed
and the list of selections, the story's position is determined. A save records exactly that list,
the **trace**, and loading runs the story again from the entry scene, feeding it the recorded
selections, until it reaches the saved position.

So a save is a *witness that a position is reachable*, not a state: it is a path from the start.
No save of an unreachable position can be constructed, and a load never produces a combination of
story variables that the story itself could not produce. Every analysis the compiler runs on the
story therefore holds for loaded games too.

### Stable choice IDs

A trace does not record line numbers or arm positions. It records each selection as a **choice
ID** made of three parts:

| Part | Example | Taken from |
| ---- | ------- | ---------- |
| The path of the function that contains the `choice!` | `crate::story::Prologue` | the module tree and the function name |
| The choice `id` | `confess` | the `id = "…"` of the `choice!` |
| The selected arm | `crate::story::Confess::Yes` | the arm's value: the enum variant's path, or `#n` (the arm's position) when the value is not an enum variant |

For example `crate::story::Prologue/confess/crate::story::Confess::Yes`. Replay supplies the call
context, so a choice `id` only has to be unique within its function, even when the function is
called from many places or runs in a loop. Write arm values as variants of an enum you declare: an
arm recorded by position breaks when the arms are reordered.

## What a save contains

| Part | Content |
| ---- | ------- |
| The trace | the entry scene's path, the entry arguments, the PRNG seed, the choice ID of every selection, and the number of steps after the last selection |
| The exit handler | the path of the function passed to `nav::start` as `on_exit` |
| The script hash | the SHA-256 of the `.kagari` file the save was made with |
| Slot information | the local time, the speaker and text of the last line, and a 320×180 thumbnail of the screen |
| The format version | the version of the save layout |

A save contains no system state: no mode stack, no settings, no meta (see
[Two nested machines](./system.md#two-nested-machines)). The slot information is what
`saves::info(slot)` and `ui::thumbnail(slot)` show on a save page; it plays no part in loading.

## Loading and divergence

`nav::load(slot)` reads the slot and replays its trace on the running program. The replay advances
through `s!` steps on its own; at each `choice!` it must meet the recorded choice ID and select
the recorded arm. When the trace is exhausted, it advances the recorded number of steps after the
last selection and stops there. The restored game then replaces the whole mode stack.

Replay is silent: it sets no read flags or seen sets, repeats no meta writes, plays no audio and
makes no autosaves. The restored step starts again from step entry: its timeline restarts, so its
animations play again, and its voice plays again.

If the save no longer fits the program, the load stops with a **divergence**: `nav::load` returns
`Err(divergence)`, leaves the stack unchanged, and `divergence.describe()` gives a message for the
player. A load diverges when:

| Cause | Message reports |
| ----- | --------------- |
| The slot is empty or out of range | the slot is empty |
| The file cannot be read, or has an unknown format version | the save cannot be read |
| The entry scene no longer exists | the missing story |
| The entry arguments no longer fit the entry scene's parameter types | the starting values no longer fit |
| The story reaches a different choice than the recorded one | the expected and the found choice |
| The recorded arm is gone, or its guard is now false | the choice and the missing arm |
| The story ends before all recorded selections are used | the number of selections left |
| Fewer steps follow the last selection than recorded | the expected and reached step count |
| The story stops with a runtime error during replay | the error |
| Replay does not reach the position within 1,000,000 steps | the step limit |

The quick-load binding reports a divergence only in the log; a load page shows it with
`describe()`.

### What a patch may change

Because a trace holds only choice IDs and a final step count, saves survive most edits. For a save
whose position is *after* the edited place:

| Change | Effect on the save |
| ------ | ------------------ |
| Rewording text, changing speakers, voices, text effects | none |
| Editing renders, assets, the screen, animations, audio | none: the screen is rebuilt by replay |
| Adding, removing or reordering `s!` steps between two choices | none |
| Reordering the arms of a choice whose arm values are enum variants | none |
| Adding an arm to a choice | none |
| Changing the game system: UI, modes, settings, bindings | none |
| Adding or removing `s!` steps after the last selection before the saved position | the save lands on a different line; if too few steps remain, it diverges |
| Renaming or moving the function that contains a choice, its `id`, the arm's enum or variant | divergence at that choice |
| Removing an arm, or a guard that now hides the recorded arm | divergence at that choice |
| Changing story logic so that a different choice is met first, or the story ends earlier | divergence |
| Renaming or moving the entry scene, or changing its parameter types | divergence |
| Arms recorded by position (`#n`) being reordered | the save silently takes a different arm |

Changing story variables, their initial values, or conditions is safe as long as the recorded
selections still lead along the same choices; replay reports the first place where they do not.

Read flags, which drive skipping read text and marking read lines in the backlog, are kept outside
saves and keyed by the function and the position of the step among the steps of that function, in
code order. Inserting or removing a step (`s!` or `choice!`) in a function therefore shifts the
read flags of every later step in that same function: after such a patch, some lines a player has
read count as unread and the reverse. Steps in other functions are not affected.

## Snapshots and traces

The trace is the **compatibility contract**: it is the part of a save that is meaningful across
versions of the script. A **snapshot** — the story state and the logical screen at a position — is
only a cache that makes reaching a position cheaper. It depends on the exact program, so it is
trusted only when the script hash matches; on any other script the engine replays the trace.

*Tentative:* the current engine writes no snapshot into a save. Every load and every rewind replays
the trace from the start of the story. The script hash is recorded already, and it decides one
detail today: an exit handler written as a closure is restored only by the same script; a named
function is looked up by its path.

## Rewind

`game.rewind(i)` returns the running game to backlog entry `i`. It cuts the current trace at that
entry and replays it, like a load. It never changes meta: read flags and seen sets stay set.

## Save slots

A game has three kinds of slot, `SaveSlot::Manual(k)`, `SaveSlot::Quick` and `SaveSlot::Auto(k)`:

- **Manual** slots, `0..save_slots`, are written by `game.save(slot)` from a save page.
- The **quick** slot is written and read by the `QuickSave` and `QuickLoad` actions of the
  [bindings](./system.md#bindings).
- **Auto** slots, `0..auto_slots`, are written by the engine on entering every choice step and on
  quit, rotating through the slots.

A slot index outside the configured count is ignored: saving does nothing and the slot reads as
empty. The API is specified in
[`std::system::saves`](../../../../std/en/src/system-saves.md).

## Determinism

Replay is exact because everything the story does between selections is deterministic:

- **Randomness is seeded and recorded.** A scene draws a random value with `rng_choose::<T>()`.
  Each `nav::start` seeds a fresh story PRNG; the seed is part of the trace, and the generator's
  state is part of the story state. Replay therefore reproduces every draw, and a save never lets
  the player re-roll a draw by loading.
- **There are no floating-point numbers.** The language has bounded integers, Rust integers, and
  the exact types `Rat` and `Decimal` (see [Numbers](../syntax/numbers.md)). Every computation
  gives the same result on every platform.
- **Renders are pure.** A render is a `const fn` of the previous screen, so replaying the
  selections rebuilds the same logical screen (see
  [Renders are blind to the story](./presentation.md#renders-are-blind-to-the-story)).
- **The system cannot reach the story.** Settings, meta, signals and wall-clock time never flow
  into the story; meta reaches it only through the entry arguments, which the trace records.

The same model makes the story analyzable: for the compiler, a random draw is a free choice over
every value of `T`, like a selection by the player.

## Preview

*Planned.* The same machinery serves an authoring preview: jumping to a step in the editor is
replaying a trace to it. The cost is bounded by keyframes: the story is replayed from the start (or
from a snapshot, for an unchanged script), but renders run only from the last
[keyframe](./presentation.md#keyframes) on the path, without animations. Editing a render
invalidates only the screens after its first use on the current path. No preview tool ships yet.
