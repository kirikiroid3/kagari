# Saves, Traces and Snapshots

A Kagari save is not a dump of the running game. It is a **trace**: the entry scene, its
arguments, the PRNG seed and the stable IDs of every choice taken, plus how many lines were
advanced after the last one. Loading replays that trace on the current program. This chapter
describes the save model, the file contents, replay, and what the design requires of the rest of
the engine. The byte format of `.kisave` files is in
[Archives and Save Databases](../formats/archive.md); the user-facing description is in the
Reference ([Saves, Replay and Determinism](https://kagari.suitsu31.club/en/reference/concepts/saves/)) and the
STD page [`std::system::saves`](https://kagari.suitsu31.club/en/std/system-saves/).

## Where in the code

| Path | Contents |
| ---- | -------- |
| `runtime_libs/kagari-engine/src/saves/mod.rs` | slot names, the slot index, `save_slot`, `autosave`, `load_slot`, `load_all`, the `std::system::saves` imports, `on_exit` restoration |
| `runtime_libs/kagari-engine/src/saves/trace.rs` | `SaveFile`, `write`, `parse`, `parse_head`, `choice_id`, `arm_id` |
| `runtime_libs/kagari-engine/src/saves/replay.rs` | `replay`, `Divergence`, `DivergenceObj` |
| `runtime_libs/kagari-engine/src/story/mod.rs` | `Trace`, `TracePos`, the backlog positions |
| `runtime_libs/kagari-engine/src/story/driver.rs` | where the trace grows (`select`, `next_step`) and where replay suppresses side effects |
| `runtime_libs/kagari-engine/src/store/` | `settings.kisave`, `meta.kisave`, and VM values as `.kisave` tables (`value.rs`) |
| `runtime_libs/kagari-vm/src/story.rs`, `rng.rs` | `StoryFiber`, `StoryStatics`, the PRNG |
| `core/kagari-archive/src/save/` | the `.kisave` container (`Save`, tables, arrays, scalars) |
| `runtime_libs/kagari-engine/src/tests/saves.rs` | round trips, divergence, rewind |

## A save is a witness

**Decision.** The canonical content of a save is the trace from the entry scene, not the state.
**Why.** The proposal's goal is that every saved position is reachable and every save survives a
script patch as far as the story it records still exists. A trace is a witness of reachability:
replaying it from the entry scene with the stored arguments and seed reaches the saved position,
so no save of an unreachable position can be built. State dumps would tie saves to the VM's frame
layout, the order of story variables and the compiler's lowering, all of which change between
builds ([Relation to the Proposal](../foundations/proposal.md)).
**Consequences.** Story state (the call stack, story variables, the PRNG state, the screen) is
never written; a load costs a replay from the start. Everything the story does must be
deterministic given the trace ([Determinism](#determinism)). A patch that removes a recorded
choice, or changes what the arguments mean, makes the save diverge with an explanation instead of
loading a wrong state.

## What a save file holds

A save slot is a `.kisave` database whose root table has these fields (format version 1):

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `version` | `i64` | `1`; any other value is `Divergence::Version` |
| `script_hash` | string | lowercase hex SHA-256 of the `game.kagari` that wrote the save |
| `entry` | string | the path of the `#[entry]` scene, e.g. `crate::story::Main` |
| `args` | array | the entry arguments, as stored values (below) |
| `seed` | `u64` | the PRNG seed `nav::start` received |
| `choices` | array of strings | the choice ID of every selection, in order |
| `steps_after_last_choice` | `u64` | lines advanced since the last selection (or since the start) |
| `on_exit` | string, optional | the function path of the exit handler, when it can be named |
| `time` | table `year`, `month`, `day`, `hour`, `minute`, `second` | `Platform::local_time` at save time |
| `who`, `text` | text, optional | the speaker and text of the last line in the backlog |
| `who_plain`, `text_plain` | string, optional | the same, flattened to plain characters |
| `thumbnail` | bytes, optional | a 320×180 PNG of the game mount |

Stored values (`store/value.rs`) are scalars for booleans, integers (`I128`), `Decimal`
(mantissa, scale), `Rat` (numerator, denominator) and strings; everything else is a table tagged
by the field `"$"`: `text`, `tuple`, `array`, `vec` (with `items`), `adt` (with `type` path,
`variant` name and `fields`), `asset` (`path`) and `handle` (static `path`). Types, variants,
assets and statics are stored **by path**, never by index, so a rebuild that renumbers them still
reads old saves. Functions, closures, references and host objects are not storable.

`on_exit` is stored when the handler is a function pointer or a closure without captures.
Restoring it ([Loading](#loading-and-replay)) looks the path up again.

### Slots and store names

| Slot | Store name |
| ---- | ---------- |
| `SaveSlot::Manual(k)`, `k < System::save_slots` | `save-manual-<k>.kisave` |
| `SaveSlot::Quick` | `save-quick.kisave` |
| `SaveSlot::Auto(k)`, `k < System::auto_slots` | `save-auto-<k>.kisave` |
| settings | `settings.kisave` |
| meta, read flags, seen sets, next autosave slot | `meta.kisave` |

A slot outside the configured range is treated as empty: `saves::info` returns `None`,
`saves::delete` and `Narrative::save` do nothing, `nav::load` returns `Divergence::Empty`. Store
names match `[a-z0-9-]+\.kisave`; the desktop store rejects anything else
([The Desktop Runtime](./desktop.md#stores)).

## Choice IDs and positions

The trace records a selection as a **choice ID**:

```text
<function path>/<choice id>/<arm id>
```

- `<function path>` is the path of the scene or instant procedure containing the `choice!` step,
  e.g. `crate::Start`.
- `<choice id>` is the choice's `id`, flattened to plain characters. It only needs to be unique
  within its function: replay supplies the call context.
- `<arm id>` is the arm value's enum variant path when the value is an enum value, e.g.
  `crate::Pick::B`; otherwise `#<index>` of the arm in the list.

For example `crate::Start/pick/crate::Pick::B`. IDs never contain line numbers or step indices, so
inserting lines, reordering arms or moving the choice inside its function keeps them valid.
Renaming the function, the choice `id`, the enum or the variant changes the ID.

The position after the last choice is a count of `Say` steps (`steps_after_last_choice`). Each
backlog entry stores the `TracePos { choices, steps }` it was reached at, which is what
`Narrative::rewind` cuts the trace to.

Read flags are keyed by **step ID** instead: `<function path>#<id>`, where the id is the step's
0-based index among the step forms of its function in source order (see
[MIR](../compiler/mir.md#step-ids)), carried by the `STEP` instruction. Only `Say` steps are
marked read, when the player moves past them (not during replay). They live in `meta.kisave`,
not in saves: the `read` table has one field per scene (per function, so each monomorphized
instance such as `crate::Remember::<crate::day::Mood>` has its own), named by the function path
and valued by an array of `u64` segments `[a0, a1, a2, a3, …]`: each pair is an inclusive range
of read step ids, pairs ascending, never overlapping or touching (a scene read straight through
is one pair `[0, n]`). Fields are written in path order. `ReadSet` (`store/read.rs`) merges
inserts with neighbours and looks up by binary search.

On load, malformed arrays are normalized (sorted, merged; reversed pairs and a trailing odd value
dropped). Scenes this program lacks are kept by path and written back, like the seen sets'
retained asset paths. Entries of the former format (`{path}#{ordinal}` → `true`, keyed by code
layout order) are not arrays and are dropped: they cannot be mapped to source-order ids
reliably, so read flags from before are lost once.

## Saving

`save_slot(slot)` writes the running game when it is at a step (`current` is not `None`):

1. The listing text is the last `Line` entry of the backlog.
2. The thumbnail: the game mount alone (`modes.first()`, the base) is drawn into a `Frame` at the
   design resolution and passed to `Platform::render_thumbnail` at 320×180; the RGBA is encoded
   as PNG. A missing or wrongly sized result saves without a thumbnail.
3. The `SaveFile` is encoded and written with `Platform::write_store`. On success the slot index,
   the thumbnail (under a new generation) and the `SaveInfo(slot)` reactive source are updated.

Write failures are logged, never trapped. Saves are written by `Narrative::save`, the
`QuickSave` action, and autosave.

**Autosave** writes `SaveSlot::Auto(auto_next % auto_slots)` and advances `auto_next` (kept in
`meta.kisave`). It runs when a choice step is entered (not during replay) and on quit when a game
is at a step and has not crashed. With `auto_slots = 0` it does nothing.

## Loading and replay

<!-- diagram: saves-load (mermaid-ish)
flowchart LR
  req["load request: nav::load, QuickLoad"] ==> read["read store: save-*.kisave"] ==> parse["parse: version 1, fields"] ==> replay["replay: fresh fiber, saved seed"] ==> restore["NavOp::Restore: installed by settle"]
  rewind["rewind(i): live trace, cut"] ==> replay
  read -.->|"empty, unreadable"| div["Divergence: Err(d) from nav::load, else logged"]
  parse -.->|"version, args"| div
  replay -.->|"trace no longer fits"| div
-->
{{#include ../diagrams/saves-load.svg}}

`load_slot` reads the store, checks `version`, parses the file, restores `on_exit` and calls
`replay`. `replay(trace, on_exit)`:

1. Finds the entry function by path; it must still be `#[entry]` (`MissingEntry`).
2. Starts a fresh `StoryFiber` with the stored arguments, fresh story statics and the stored seed
   (`Args` when the arguments no longer fit the parameters).
3. Swaps in a new `GameFrame` with an empty trace, sets `SystemState::replaying`, and runs the
   loop under `Vm::with_fresh_budget`:
   - at a `Choice` with a recorded ID left: the current choice's prefix
     `<function path>/<choice id>/` must match the recorded ID (`ChoiceChanged`), and a shown arm
     with the recorded arm ID must exist (`ArmGone`); it is selected through `driver::select`, so
     the new trace is rebuilt as it goes;
   - at a `Say`: advance with `driver::next_step` and count the step;
   - with no recorded choice left, stop when the step count equals `steps_after_last_choice`;
   - a story that returns early is `EndedEarly` (choices left) or `StepsUnreachable`; a trap is
     `Panicked` with its first line; more than 1 000 000 steps is `TooLong`.
4. Restores the live game and the `replaying` flag whatever the outcome, and returns the replayed
   frame with the next generation number.

The loop in step 3 is the engine's drive loop: a debugger uses the same loop to follow a trace and
then steer the story on ([The Engine](./engine.md#debugger-access)), so a debugger's moves and a
load reach the same states.

On success `load_slot` pushes `NavOp::Restore(frame)`; `settle` unmounts every mode, installs the
frame with a fresh story clock, mounts the game view, syncs every audio channel to the logical
screen without fades, and restarts the current line's voice. The current step's timeline starts
again from its entry, so its animations play once more. On failure the live game and the mode
stack are untouched: `nav::load` returns `Err(Divergence)`, and `QuickLoad` logs the description.

While `replaying` is set, entering and leaving steps records no read flags, no seen images, no
`Meta::write`, no voice, no timeline audio, no autosave and no progress log. Settings, meta, read
flags and seen sets are therefore never reverted or doubled by a load.

**`on_exit`.** A stored path that names a one-parameter function is restored as a function
pointer. A capture-free closure is restored only when the save's `script_hash` equals the running
program's: closure functions have synthetic paths that are not stable across builds. Otherwise,
when the running game has the same entry scene, its handler is kept; else there is none, and the
end of the story resets the stack to `System::start`.

**Rewind.** `Narrative::rewind(i)` cuts a copy of the live trace at backlog entry `i`
(`TracePos`) and replays it exactly like a load; success queues a `Restore`, failure logs a
warning. It costs one replay from the entry scene.

### Divergence

`Divergence` is a host object; `Divergence::describe` returns the player-facing text.

| Variant | When | Description |
| ------- | ---- | ----------- |
| `Empty` | no file, or the slot is out of range | This save slot is empty. |
| `Unreadable(e)` | the container or a field is malformed | This save cannot be read: … |
| `Version(v)` | `version` is not 1 | This save has format version … |
| `MissingEntry(p)` | no `#[entry]` function at the path | The story `…` this save was made in no longer exists. |
| `Args(p)` | the arguments no longer decode or fit | The starting values of `…` no longer fit the story. |
| `ChoiceChanged { expected, found }` | replay meets another choice first | The story no longer reaches the choice `…` |
| `ArmGone { choice, arm }` | the recorded arm is not shown | The choice `…` no longer offers `…` |
| `EndedEarly { remaining }` | the story returns with choices left | The story now ends before the saved position … |
| `StepsUnreachable { expected, reached }` | too few lines after the last choice | The saved position is … lines after the last choice … |
| `Panicked(m)` | a trap during replay | The story stopped with an error while replaying the save: … |
| `TooLong` | more than 1 000 000 replayed steps | Replaying the save did not reach its position … |

## Snapshots and the script hash

The proposal pairs the trace with a **snapshot**: the position, call stack, story variables, PRNG
state and screen, used as a cache that is trusted only when the script hash matches, and rebuilt
by replay otherwise. Keyframes bound how many renders a replay must run.

**Status.** Save snapshots are not implemented. Every load and every rewind replays the trace from
the entry scene and runs every render on the way. The `script_hash` is recorded in every save, but
the only thing it gates today is restoring a closure `on_exit` handler.

In-memory **call snapshots** exist for debugging: when a runtime opens a debugger channel on a
debug build, the fiber records the story variables, the values behind `&mut` arguments, the
PRNG, the trace position and the screen at every scene call
([The VM](./vm.md#call-snapshots)). Scene hot swap
unwinds to one to call a changed scene again ([The Engine](./engine.md#hot-swap)). They are never
written and never used by a load. After a swap the live trace is cut at the call, so it stays a
witness: a save made after a swap loads on the new program by normal replay, as long as the code
the trace passes before the call has not changed what it reaches.

The old design notes describe the planned save cache; implementing it needs:

- a serialized form of the fiber (frames, story-variable cells, PRNG) and of the screen that is
  stable for one script hash;
- the rule that a snapshot is used only when the save's hash equals the running program's hash,
  and is otherwise discarded and rebuilt from the trace;
- keyframe-aware replay that skips renders before the last keyframe on the path.

The trace remains the compatibility contract either way; a snapshot may only make loads faster.
See [Open Questions](../process/open-questions.md).

## Compatibility across game patches

| Change in the new build | Effect on old saves |
| ----------------------- | ------------------- |
| Lines added, removed or edited before the last recorded choice | none: only choices are matched |
| Lines added or removed between the last choice and the saved line | the load lands that many lines off; with too few lines, `StepsUnreachable` |
| Choice moved within its function, arms reordered | none |
| Choice `id`, its function, the arm enum or variant renamed | `ChoiceChanged` / `ArmGone` |
| An arm hidden by a new guard | `ArmGone` |
| Entry scene renamed or no longer `#[entry]` | `MissingEntry` |
| Entry parameter types changed (incompatible stored values) | `Args` |
| Randomness drawn in a different order before the saved position | replay follows the new program; the position may change or diverge |
| Asset renamed | saves are unaffected unless an entry argument holds the asset (then `Args`); the seen sets keep the old path |
| Statics, types or assets renumbered | none: everything is stored by path |
| Code without steps inserted, or code moved without changing step order | read flags unaffected (step ids are source-order ordinals) |
| Steps inserted or removed before a line in the same function | the step ids of that function's later steps shift, so their read flags move with the ids |
| Generic scene instantiated with another type | that instance has its own read flags (its path carries the type arguments) |

The save container and field layout are versioned (`version = 1`); a reader must reject unknown
versions rather than guess.

## Determinism

Replay is exact only if the story's path depends on nothing but the trace. The design guarantees
this as follows.

- **Story code is `pushdown fn` and `const fn`.** It cannot call `fn`, read settings, meta, the
  clock, input or the file system. Meta reaches the story only through entry arguments, which are
  recorded. Settings reach renders only as deferred references (`Entity::gate`), which the engine
  resolves at draw time, never in the story.
- **Randomness.** The story PRNG is xoroshiro128++ seeded with two SplitMix64 outputs of the
  recorded `seed`; its state lives in the fiber, so replay draws the same numbers
  ([The VM](./vm.md)). Only `nav::start` asks the platform for a seed.
- **Host imports reachable from the story** (`std::screen`, `std::screen::audio`, `Meta::write`)
  must be deterministic functions of their arguments and the screen value. They may not read
  engine time, settings or platform state into a value the story can observe.
- **Floating point** is used only for presentation (easing, interpolation, gains, layout) and
  never flows back into story values.
- **Fuel.** The instruction budget per run is fixed, so a story that exhausts it does so at the
  same point on every replay.
- **Side effects off during replay.** Everything persistent or audible is gated on
  `SystemState::replaying`.
- **Headless runs** are deterministic end to end: the seed is fixed (`--seed`, default 1), the
  save time is fixed (2026-01-01 00:00:00), and engine time advances only through `wait` in 16 ms
  ticks ([The Desktop Runtime](./desktop.md#headless-mode)).

## Invariants

- The trace is the only story state a save holds; story state is never serialized.
- Choice IDs are built from paths and names only (`choice_id`, `arm_id`), never indices of steps
  or lines. An arm without an enum value falls back to `#<index>`; keep that the exception.
- Types, variants, assets and statics are stored by path.
- Replay goes through the same `driver::select` and `driver::next_step` as live play, so live and
  replayed traces cannot drift apart.
- Replay never changes the live game, the mode stack or any store on failure.
- Every save field added later must be optional for readers, or bump `version`.

## When you change this

- The save layout or choice-ID format: bump the `version` field (and keep reading version 1 or
  add a `Divergence` for it), update [Archives and Save Databases](../formats/archive.md), the
  Reference [Saves, Replay and Determinism](https://kagari.suitsu31.club/en/reference/concepts/saves/) and the
  STD page `system-saves.md` (en and ja), and `tests/saves.rs`.
- Anything that adds a source of nondeterminism to story code (a new import usable from
  `pushdown fn` or `const fn`): check it against [Determinism](#determinism) and the
  [std–checker contract](../std/contract.md).
- New `Divergence` variants: `describe`, the STD page for `Divergence`, and the table above.
- Implementing snapshots: this chapter, [Open Questions](../process/open-questions.md), the
  Reference saves page.
