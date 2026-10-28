# The Engine

`kagari-engine` runs a compiled game. It owns the [VM](./vm.md), implements every std host
import, keeps the system machine (the mode stack and its UI) and the story machine (the running
game), models the screen, its timeline and the audio, persists settings, meta and saves, and
produces a platform-neutral draw list. It has no platform dependencies: a runtime such as the
[desktop runtime](./desktop.md) supplies storage, assets, time and thumbnails through the
`Platform` trait, draws the `Frame` with `kagari-render` and plays the `AudioCommand`s.

This chapter describes how the engine works. The user-visible behaviour is specified in the
Reference ([The Story Machine](../../../reference/en/src/concepts/story.md),
[The Game System](../../../reference/en/src/concepts/system.md)) and the STD Reference; the ABI
between std declarations and these imports is in [Builtins and the Host ABI](../std/builtins.md).

## Where in the code

| Path (`runtime_libs/kagari-engine/src/`) | Contents |
| ---------------------------------------- | -------- |
| `lib.rs` | `Engine` (`boot`, `input`, `update`, `frame`, `take_audio`, `quit`, …), `Platform`, `EngineState`, `HostCtx`, `BootError`, the `import_table!` macro |
| `host/mod.rs` | `Bindings` (every `ImportId` bound to a `StdImport` at boot) and `impl Host for HostCtx` |
| `system/` | `SystemConfig` (the decoded `System`), `SystemState`, the mode stack and `settle` (`modes.rs`), `std::system::nav` (`nav.rs`), settings and seen-set imports (`imports.rs`), the Crashed state (`crash.rs`) |
| `story/` | `GameFrame`, `Trace`, the backlog and `StoryClock` (`mod.rs`); the story driver (`driver.rs`); `std::narrative` imports (`narrative.rs`); reveal timing (`reveal.rs`) |
| `screen/` | the screen model (`model.rs`), the `&mut` parts handed to bytecode (`parts.rs`), the step runtime and timeline (`runtime.rs`), sampling (`sample.rs`), screen to `Frame` nodes (`draw.rs`), the `std::screen` imports (`mod.rs`) |
| `anim/` | easings, interpolation, motions and transition progress |
| `audio/` | `AudioCommand`, `Bus`, the channel imports (`mod.rs`) and `AudioModel` (`model.rs`) |
| `ui/` | the reactive graph (`reactive.rs`), the element arena (`element.rs`), `std::ui` imports (`imports.rs`), layout, focus and controls, styles, text styles and the layout cache (`text.rs`), drawing (`draw.rs`) |
| `input/` | `InputEvent`, `Key`, `MouseButton`, `PadButton`, dispatch (`mod.rs`) and `System::bindings` (`bindings.rs`) |
| `store/` | `settings.kisave` and `meta.kisave` (`mod.rs`), VM values as `.kisave` tables (`value.rs`) |
| `saves/` | slots, saving, autosave and loading (`mod.rs`), the file layout and choice IDs (`trace.rs`), replay and `Divergence` (`replay.rs`) |
| `values/` | `STD_ADTS` (`table.rs`, the std types the engine reads or builds, in declaration order), `StdTypes`, typed decoders (`decode.rs`), shader arguments (`shader_args.rs`) |
| `assets.rs` | `AssetStore`: lazily decoded images, fonts, shader sources, audio lengths, save thumbnails |
| `frame.rs` | the `Frame` draw list |
| `test_support.rs`, `tests/` | fixture compilation through `kagari-lang`, a fake platform, engine tests |

Related crates: `runtime_libs/kagari-vm` ([The VM](./vm.md)), `runtime_libs/kagari-text` and
`runtime_libs/kagari-render` ([Text and Rendering](./text-render.md)),
`runtime_libs/kagari-compat` ([below](#profile-native_v0-and-kagari-compat)).

## Structure

<!-- diagram: engine-components (mermaid-ish)
flowchart LR
  loop["Runtime loop (app.rs, headless/)"] ==>|"input, update, frame, quit"| engine
  subgraph engine["Engine"]
    vm["Vm (StoryFiber, fuel)"] ==>|CALLIMP| host["HostCtx (imports by std path)"]
    host ==>|call_value| vm
    host ==> state
    subgraph state["EngineState"]
      system["SystemState: modes, stores, saves"]
      ui["UiState: signals, mounts, focus"]
      game["GameFrame: fiber, screen, trace"]
      audio["AudioModel: channels, voice"]
      assets["AssetStore: images, fonts, thumbs"]
      text["TextSystem: styles, layout cache"]
      nav["pending_nav: NavOps applied by settle()"]
    end
  end
  engine ==>|calls| platform["Platform: stores, assets, time, seed, thumbnails"]
  engine ==> render["kagari-render: Frame to wgpu"]
  engine ==> kira["kira (desktop): AudioCommand"]
  engine ==> ktext["kagari-text: shaping, layout"]
-->
{{#include ../diagrams/engine-components.svg}}

`Engine` holds three things: the `Vm`, the `EngineState` and the time of the last call. The `Vm`
stays outside `EngineState` so that a host import can borrow both: every VM entry receives a
`HostCtx { state: &mut EngineState, platform: &mut dyn Platform, now_ms }` as its `Host`, and an
import that must run bytecode (a render, a component, a reactive closure) calls back into the
`Vm` it was given.

| `EngineState` field | Contents |
| ------------------- | -------- |
| `program` | the loaded `Program` (`Rc`) |
| `std` | `StdTypes`: the `AdtTy` of every std type in `values/table.rs` |
| `assets` | `AssetStore` |
| `text` | `TextSystem`: the system font and text style, the layout cache |
| `audio` | `AudioModel` |
| `system` | `SystemState`: bound imports, the decoded `System`, the stores, the mode stack, held inputs, the save index, the window state, the crash, the script hash |
| `ui` | `UiState`: the reactive graph, the element arena, the mounted modes |
| `game` | `Option<GameFrame>`: the running game; "a game is running" is exactly `game.is_some()` |
| `pending_nav` | navigation requested during a VM entry, applied by `settle` |

The `Platform` trait is everything the engine asks of the machine:

| Method | Used for |
| ------ | -------- |
| `read_store`, `write_store`, `delete_store` | `settings.kisave`, `meta.kisave`, `save-*.kisave` |
| `read_asset` | asset bytes by bytecode asset path |
| `audio_duration_ms` | voice and one-shot lengths (auto mode, `wait_for_voice`, timeline lengths) |
| `render_thumbnail` | the 320×180 save thumbnail of a `Frame` |
| `local_time` | the save timestamp |
| `random_seed` | the PRNG seed of a new story (`nav::start`) |
| `set_title` | called once after `main` returned, before the first store access (the desktop data directory depends on the title) |

**Decision.** The engine has no platform code; a `Platform` implementation, a renderer for
`Frame` and a player for `AudioCommand` make a runtime.
**Why.** The windowed runtime, the headless runtime and the engine's own tests drive the same
engine, and a later web runtime needs no engine fork.
**Consequences.** Everything time-dependent takes `now_ms` from the caller; the engine never reads
a clock. Assets are decoded lazily through `read_asset`.

## Boot

`Engine::boot(bytes, platform, now_ms)`:

1. `Program::load` parses and verifies the bytecode ([The VM](./vm.md)); a failure is
   `BootError::Load`.
2. The script hash, the lowercase hex SHA-256 of the whole `game.kagari`, is computed for saves.
3. `Bindings::new` resolves every import of the module by its std path (below). An unknown path is
   `BootError::UnsupportedImport`.
4. `EngineState::new` resolves the std types and initializes every `Setting` and `Meta` static to
   its init constant (`Stores::initial`).
5. `main` runs. Navigation requested during `main` is dropped with a warning.
6. `SystemConfig::decode` reads the returned `System` by field index (`title`, `resolution`,
   `start`, `game`, `save_slots`, `auto_slots`, `backlog`, `font`, `text`, `settings`,
   `bindings`). The 13 handles of `Settings` must be `Setting` statics. A malformed value is a
   `BootError::Trap` with the message `` `main` returned a malformed `System`: <field> ``; a zero
   resolution side is an error too.
7. `platform.set_title(title)`, then the stores are loaded (`settings.kisave`, `meta.kisave`).
8. Every font asset is loaded in asset order; the system font and the `System::text` style are
   decoded into the `TextSystem`. A failure is `BootError::Asset`.
9. The window state is set (title, design resolution), the volume and fullscreen settings are
   applied, and every in-range save slot's listing and thumbnail are read (`saves::load_all`).
10. `NavOp::Goto(System::start)` is pushed and `settle` runs. A trap from here on does not fail
    the boot: the engine starts in the Crashed state.

The desktop runtime maps `BootError::Trap` to exit status 3 and every other boot error to 2
([The Desktop Runtime](./desktop.md#exit-statuses)).

## The frame loop

The runtime calls the engine at three points. Each takes `now_ms`, the runtime's monotonic
millisecond clock (engine time).

<!-- diagram: engine-loop (mermaid-ish)
flowchart LR
  input["input()"] ==> dispatch["dispatch: UI, then bindings"] ==> s1["settle"] ==> h1["flush_heard"]
  update["update()"] ==> tick["driver::tick: auto, skip"] ==> hud["HUD shown"] ==> s2["settle"] ==> flush["ui::flush"] ==> s3["settle"] ==> stick["screen.tick: fire audio"] ==> fl["flush: heard, stores"]
  frame["frame()"] ==> layout["layout::run"] ==> draw["draw mounts: still, crash"] ==> f["Frame"] ==> renderer["Renderer (kagari-render)"]
-->
{{#include ../diagrams/engine-loop.svg}}

| Call | Steps |
| ---- | ----- |
| `input(event, platform, now)` | `input::dispatch` (the top mount's UI first, then the bindings), `settle`, record newly heard audio |
| `update(platform, now)` | `driver::tick` (auto and skip, only while the game is on top), recompute the HUD fade state, `settle`, `ui::flush` (re-run dirty reactive computations), `settle` again (effects may navigate), `screen.tick` (fire the audio of timeline items that started), record newly heard audio, write dirty stores |
| `frame(now) -> Frame` | the crash frame when crashed; else `ui::layout::run`, then the mounts are drawn bottom to top |
| `take_audio()` | the `AudioCommand`s queued since the last call |
| `window()` | the `WindowState` the game asks for: title, design resolution, fullscreen, quit |
| `quit(platform)` | autosave a game that is at a step, write the stores, set `WindowState::quit` |

Every call is a no-op once the game has crashed or quit. A trap from any step is caught by
`Engine::with_ctx` and turns into the Crashed state ([Errors](#errors-traps-and-the-crashed-state)).

## Host calls

The VM executes `CALLIMP` by calling `Host::import(vm, ImportId, args)`; `HostCtx` implements it
by indexing `Bindings` with `ImportId.0` and calling the bound `StdImport`.

- **Import tables.** Each std module area declares an enum with `import_table!`, one variant per
  std path: `ScreenImport` (`std::screen::*`), `AudioImport` (`std::screen::audio::*`),
  `NarrativeImport`, `SystemImport`, `NavImport`, `SavesImport`, `UiImport`.
  `StdImport::from_path` asks them in that order; the first that knows the path wins. Each enum
  has a `call(self, ctx, vm, args)` that matches on the variant.
- **Binding at boot.** `Bindings::new` maps every import of the module once. A path no table
  knows fails the boot with ``the bytecode imports `<path>`, which this runtime does not provide``. So
  a program can only call what this engine implements, and an unknown import is found before any
  code runs.
- **Arguments.** `values::arg(vm, path, args, i)` returns argument `i` by value, loading a
  cell-rooted `Value::Ref` through `vm.load`. `&mut self` receivers stay references: the screen
  imports resolve them to a screen part (`screen/parts.rs`).
- **Std values.** Std structs and enums are positional `Value::Adt`s. `values/table.rs` lists the
  path, variants and fields of every std type the engine reads or builds, in `.kis` declaration
  order; `StdTypes::resolve` maps each to the program's type, or a synthetic type when the program
  never mentions it. Decoders read fields by index (for example `TextEffect` has 11 fields,
  `TextStyle` 12, `System` 11).
- **Host objects.** Opaque std types are `Value::Host(HostObj)`: `ScreenObj`, `NarrativeObj`,
  `ArmObj`, `DivergenceObj`, UI elements and placed elements, screen parts and pending changes.
  Signal getters and setters are `Value::HostFn`; the VM calls them through
  `Host::call_host_fn`, which `ui::reactive::call_host_fn` implements.
- **Re-entry.** An import that runs code calls `vm.call_value(ctx, f, kind, args)` with the kind
  the callee must have: renders run as `FnKind::Const`, components, handlers and reactive closures
  as `FnKind::Fn`. The VM checks the kind.
- **Errors.** An import reports misuse with `Trap::host(message)`; there is no recoverable error
  channel. Expected failures are std values instead (`nav::load` returns `Result`).
- **Static initializers.** `std::system::setting` and `std::narrative::meta` are imports only so
  that the compiler can type them; they are evaluated at compile time into static init
  constants. Calling them at run time traps.

**Decision.** Imports are resolved by std path string at boot, and every engine-side type
decoder reads std values positionally against a checked table.
**Why.** The bytecode is untrusted input ([Design Decisions](../foundations/decisions.md)): it
cannot load native code, only name imports the engine build provides. Binding once at boot keeps
dispatch an array index.
**Consequences.** Renaming a std item or reordering its fields is an ABI change: update the
import table or `values/table.rs`. `values/tests.rs` compiles
`tests/fixtures/std_types/main.kix` and checks `STD_ADTS` against the compiled program.

Each outermost VM entry gets a fresh instruction budget (`kagari_vm::FUEL_PER_RUN`, 10⁸); a
runaway handler traps instead of hanging the frame. Save replay runs under
`Vm::with_fresh_budget`, so each replayed step has its own budget.

## The mode stack and navigation

`SystemState::modes` is the system machine's stack, bottom to top. Each `Mode` has a kind and the
`ModeId` of its UI mount.

| `ModeKind` | Component | Pushed by | Narrative handle | Autofocus |
| ---------- | --------- | --------- | ---------------- | --------- |
| `Root` | `fn() -> RootPage` | `nav::goto`, boot, the end of a story | no | yes |
| `Game` | `System::game`, `fn(Narrative) -> Element` | `nav::start`, a load | yes | no |
| `Overlay` | `fn() -> Overlay` | `nav::call`, `Action::Open` | no | yes |
| `GameOverlay` | `fn(Narrative) -> GameOverlay` | `Narrative::call`, `Action::OpenInGame` | yes | yes |
| `Dialog` | `fn() -> Dialog` | `nav::dialog` | no | yes |

Navigation is deferred. `nav::*`, `Narrative::call` and the bindings push a `NavOp` onto
`pending_nav`; `settle` applies them after the VM returns. `settle` loops until nothing is left:
it pops the first `NavOp`; when there is none and the game mode is on top, it applies the game's
pending story input; it stops early once the game has crashed or quit.

| `NavOp` | Effect |
| ------- | ------ |
| `Call(f)`, `Dialog(f)` | push an `Overlay` / `Dialog` |
| `CallGame { component, generation }` | push a `GameOverlay` if the game of that generation is still the base; else ignored with a warning |
| `Back` | pop the top mode unless it is the base; resume the story clock when the game is on top again |
| `Goto(f)` | unmount everything, drop the game, push `Root` |
| `Start { entry, args, on_exit, seed }` | create a `StoryFiber`, unmount everything, install a new `GameFrame` with a fresh `Trace`, push `Game`, resume the fiber to its first step |
| `Restore(frame)` | unmount everything, install the replayed `GameFrame` with a fresh story clock, push `Game`, restart the current line's voice |
| `Quit` | `quit_now`: autosave, write stores, set `quit` |

Pushing any mode over the game pauses the story clock and completes every running animation of
the current step (`ScreenRuntime::complete_all`), so the screen below is still. Clearing the
stack ends the game's audio and lets the system music return.

`nav::start` checks that its first argument is a pointer to an `#[entry]` function and that the
arguments are a tuple; the seed comes from `Platform::random_seed`. `nav::load` is the one
navigation that does work immediately, because it returns a `Result`: it replays the save at
once and queues a `Restore` only on success ([Saves](./saves.md)).

**Decision.** Navigation is queued and applied after the outermost VM entry returns.
**Why.** UI handlers run inside the reactive graph and inside a mounted mode. Unmounting that
mode, or the whole stack, while its code runs would free the computations and elements the VM is
using. Deferral also gives a deterministic order: operations apply in the order they were
requested.
**Consequences.** A handler that calls `nav::goto` and then reads state still sees the old stack.
Story input (`Narrative::advance`, `select`) is deferred the same way.

## The system UI: signals and reactivity

UI follows SolidJS semantics. A component runs once, untracked; reactive closures re-run when a
source they read changes. The implementation is `ui/reactive.rs`.

- **Graph.** Computations (`Comp`) live in generational slots (`CompId { index, generation }`).
  Each has a kind, an owner, the child computations, elements and signals its last run created,
  the sources it read, a creation sequence number and a dirty flag.
- **Kinds.** `Root` (a mount's scope) and `Row` (an `each` row or `show` branch) are scopes that
  never re-run. `Effect`, `Prop` (a reactive attribute writing one `PropSlot` of an element),
  `Child` (an element closure), `Each`, `Show` and `SelectOption` re-run.
- **Sources.** `SourceKey`: `Signal(n)`, `Setting(id)`, `Meta(id)`, `NarrativeLine`,
  `NarrativeMenu`, `NarrativeHud`, `NarrativeHistory`, `NarrativeMode`, `SaveInfo(slot)`,
  `SeenImage(id)`, `Heard(id)`, `Watched(id)`. A read import calls `ui.track(key)`; a write calls
  `ui.invalidate(key)`.
- **Tracking.** A stack of `Run { owner, tracking }` says who owns what is created and who records
  what is read. `run_comp` resets the computation (disposing what its last run made), pushes
  itself as owner and tracker, calls its closure, then applies the result to its element.
  `run_untracked` (components, handlers) pushes an owner without a tracker.
- **Signals.** `signal(init)` stores the value in the graph and returns a getter `HostFn` with an
  even id and a setter with the next odd id. The setter invalidates only when the new value is
  not equal (`value_eq`). A signal belongs to the computation that created it and dies with it;
  using it afterwards traps (`signal used after its mode was closed`).
- **Flush.** Invalidation queues dirty computations by creation sequence. `ui::flush` re-runs
  them in creation order, batch by batch, at most 100 rounds; a graph still dirty after that traps
  (`reactive update did not settle`). Flush runs only in `update`, so several invalidations in
  one frame cost one re-run.
- **Mounts.** `ui::mount` allocates a `ModeId` (never reused), runs the component under a new
  `Root` scope with the `Narrative` handle as argument for `Game` and `GameOverlay`, unwraps the
  mode newtype, and records the root element. Unmounting disposes the root scope and with it every
  computation, element and signal of the mode.
- **Layout, focus, drawing.** `ui/layout.rs` measures and arranges every mounted tree in design
  pixels when `layout_dirty` is set. `ui/focus.rs` handles keyboard, pad and mouse input for the
  top mount only: focus order (`focus_order` first, then document order), activation, sliders,
  selects, scrolling. Unconsumed input goes on to the bindings. `ui/draw.rs` turns each mount into
  `Frame` nodes ([Text and Rendering](./text-render.md#how-the-engine-builds-a-frame)).

Engine events invalidate the matching sources: entering a step invalidates `NarrativeLine`,
`NarrativeMenu` and `NarrativeHud`; a backlog push `NarrativeHistory`; a play-mode change
`NarrativeMode`; a save or delete `SaveInfo(slot)`; a new seen image `SeenImage(id)`.

## The story driver

A running game is a `GameFrame` (`story/mod.rs`):

| Field | Meaning |
| ----- | ------- |
| `generation` | the game's number; `Narrative` and `Arm` handles carry it, and a handle of an ended game traps or is ignored |
| `fiber` | the `StoryFiber`: the story's call stack, its story-variable cells and its PRNG ([The VM](./vm.md)) |
| `screen` | the `ScreenRuntime` of the current step |
| `current` | `None`, `Say(SayStep)` or `Choice(ChoiceStep)` |
| `step_serial`, `step_entered` | the step counter and the story time of the step's entry |
| `reveal` | the `RevealRun` of the current line |
| `trace` | what a save records ([Saves](./saves.md)) |
| `backlog` | `BacklogEntry`s with the trace position each was reached at, capped at `System::backlog` |
| `mode`, `hide_window`, `skip_ready_at` | play mode (`Normal`, `Auto`, `Skip`), hidden window, skip pacing |
| `clock` | `StoryClock`: engine time minus the time spent paused under overlays |
| `on_exit`, `pending_input`, `hud_shown` | the exit handler, the deferred story input, the HUD fade state |

The story variables, the call stack and the PRNG state exist only inside the fiber. System code
reaches the game only through the `Narrative` handle, so nothing outside the story can read or
write story state, and the engine never serializes it: a load rebuilds it by replay.

**Resuming.** `driver::resume` takes the fiber out of the frame, resumes it with an input value
(unit after a `Say`, the arm's value after a selection) and puts it back. The fiber runs until the
next `STEP` (`Suspend::Step { record, func, ordinal }`) or until the entry scene returns
(`Suspend::Returned`).

**Entering a step.** `enter_step` receives the step record:

1. The record must be a `std::narrative::Say` or `std::narrative::Choice`.
2. The previous voice stops, unless `voice_sustain` is on.
3. The render runs: `render(screen)` as `const fn` with the screen the next step receives
   (`render_input`), or the screen unchanged when the step has no render. The result becomes the
   new `ScreenRuntime` (below); the images it shows are loaded; new images enter the seen set.
4. A `Say` records its read key (`<function path>#<ordinal>`, the index of the `STEP` among its
   function's `STEP`s in code order), pushes a backlog line (the `history` text when given),
   builds the reveal plan at the player's text speed, and starts the voice at the text start.
5. A `Choice` decodes its id (a text, flattened to plain characters), its arms (text, shown, value)
   and each arm's ID ([Saves](./saves.md#choice-ids-and-positions)); a choice with no shown arm
   traps. The disabled arms and the menu alignment come from the step's screen.
6. The backlog is trimmed, the timeline ticks once, the narrative sources are invalidated, and a
   choice is autosaved.

While a save is replayed (`SystemState::replaying`), steps are entered without read flags, seen
sets, meta writes, voices, timeline ticks, autosaves or logs.

**Advancing.** `Narrative::advance` and `Action::Advance` set `pending_input` (the first input of
a frame wins); `settle` applies it while the game is on top. The Advance chain is:

1. if the timeline or the reveal is still running, complete both (the current reveal segment);
2. else, if the reveal waits at a click pause, release it;
3. else go to the next step: mark the line read, count it in the trace
   (`steps_after_last_choice`), resume the fiber.

**Selecting.** `Narrative::select(arm)` checks that the `Arm` handle belongs to this game and to
the current step (`generation` and `step_serial`); a stale arm is ignored with a warning. The
selection pushes a `Chosen` backlog entry, appends the choice ID to the trace, resets the step
count and resumes the fiber with the arm's value.

**Finishing.** When the entry scene returns, the step becomes `None`, the exit handler runs as
`fn` with the returned value, and when it navigated nowhere the stack resets to `System::start`.

**Play modes.** `driver::tick` runs once per `update` while the game is on top. Hold-skip (a held
`HoldSkip` binding) makes the effective mode `Skip`.

| Mode | Behaviour |
| ---- | --------- |
| `Skip` | at an unread line without `skip_unread`, falls back to `Normal`; else completes the step at once and goes on 50 ms later; ends at a choice |
| `Auto` | at a click pause, releases it after `auto_delay`; at the end of a line, goes on after `auto_delay + auto_delay_per_char × units`, and after the voice ends when `wait_for_voice`; paused while the window is hidden |

Fallbacks when a setting has no value: text speed 30 ms, `auto_delay` 3000 ms, per-character
delay 0, `skip_unread` false, `wait_for_voice` true, `voice_sustain` false, window opacity 100 %,
animations on.

## Screen state and the timeline

`std::screen::Screen` is the host object `ScreenObj` (`screen/model.rs`), a port of the
compiler's `core/kagari-lang/src/eval/machine/screen.rs` with typed fields
([Constant Evaluation](../compiler/eval.md)). It has two parts:

- `LogicalScreen`, the persistent state after a render: entities (`BTreeMap<u32, Entity>` with
  plane, z, position, anchor, scale, rotation, opacity, filter, mix, mask, local motion, fill,
  parts (an image or a paint each), label, movie, flip and gate), the HUD (text window, quick
  menu, side image), the camera, the screen filter, the music channel and four ambience
  channels. Screen equality compares only this part; the consistency analysis relies on that.
- `StepState`, which belongs to one step: the timeline cursor, the text start cursor, the
  scheduled items, the choice menu's disabled arms and alignment, and despawning entities.

Imports never mutate a screen in place behind the VM's back: `&mut Entity`, `&mut Prop<T>` and
the other parts are host references into the screen value the render holds (`screen/parts.rs`). A
change such as `prop.change(v)` returns a pending change; `apply_animation` commits it with a
`Tween`, `Transition` or `AudioFade`, which updates the logical screen at once and schedules an
item on the timeline.

**The step runtime.** `ScreenRuntime` (`screen/runtime.rs`) holds the returned screen, the logical
screen of the step before (`base`), the entry time and one `ItemState { start, length_ms, fired }`
per scheduled item. Each item starts at a `Cursor`:

| Anchor | Start |
| ------ | ----- |
| `Entry` | step entry |
| `Mark(n)` | when the reveal reaches `Mark(n)` (unresolved while the reveal waits at an earlier click pause; a mark the text lacks resolves to the reveal end) |
| `Barrier(n)` | the end of the first `n` items (`barrier()`) |

plus the animation's delay. Item lengths are the tween or transition duration (plus the hold of
`ThroughColor`), the motion's last key times its repeat count, the travel's duration, the fade
lengths of an audio change, or the sound length. With the `animations` setting off, delays and
visual lengths are 0; audio lengths are kept. The text start (where the reveal and the voice
begin) is computed from the `begin_text` cursor, ignoring items that depend on a mark.

`tick(now)` resolves the starts it can and fires the audio of items that have started.
`complete_all(now)` (a click, skip, an overlay) ends every resolved item now and drops one-shots
that have not started. Items waiting on a mark stay pending.

**Sampling.** `screen/sample.rs` computes the `VisualScreen` at a story time: per property and
layer target, the latest committed item that has started is active, and it starts from the value
of the latest earlier item on the same target (the cut-to-end rule), or from the base. Easing
applies to tweens; transitions give a layer pair with a progress; motions add their relative
path. Entities are sorted by `(plane, z, id)`. Drawing is described in
[Text and Rendering](./text-render.md#how-the-engine-builds-a-frame).

### Shader effects

A `Filter`, `Mix`, `Mask` or `LocalMotion` value, the texture and mask of a `Paint` and the shader
of a `TransitionalMotion` decode into an `EffectSpec` (`values/decode.rs`): the shader asset, the
argument value as the program built it, the argument converted for the shader (`ArgValue`,
`values/shader_args.rs`), its bytes in WGSL storage layout (`kagari_shader::pack`) and, for a
local motion, its `LocalMotionBehavior`. Conversion happens when the value is set, so a value a
shader cannot read traps in the import that received it (`` `<import path>`: shader arguments
contain a … value ``). Two effects are equal when their shader, behaviour and argument values
are, as in the compiler. Arguments over `kagari_shader::MAX_ARGS_BYTES` (16384) are logged and
the effect is not drawn.

| Argument value | `ArgValue` |
| -------------- | ---------- |
| `bool` | `U32` 0 or 1 |
| integer | `I32` when it fits, else `U32` when it fits, else a trap |
| `Decimal`, `Rat` | `F32` (nearest) |
| `Color` | `Vec4`, premultiplied linear |
| `Point`, `Offset`, `Size` | `Vec2` in px |
| `Anchor` | `Vec2` of fractions |
| `Angle` | `F32` in radians, clockwise |
| `Duration` | `F32` in seconds |
| a fieldless enum variant (`TypeTable::is_enum`) | `U32` of the variant index |
| another struct | `Struct` of its fields, in order |
| `[T; N]` | `Array` |
| `()` | `U32(0)` (`KgNoArgs`) |

**Sampling an effect.** `sample_effect` follows the property's chain like `sample_prop`. When the
active tween runs between two effects of the same shader and behaviour, the arguments interpolate
(`kagari_shader::lerp` at the eased progress: floats mix, integers switch to the new value);
otherwise the new effect shows from the tween's start. The result is a `frame::ShaderEffect`:
the shader, the packed arguments and the effect time.

**The effect clock.** Each effect has a slot (`EffectSlot`: the screen filter, an entity's
filter, mix, mask or local motion, or the paint of an image layer) and a key (`EffectKey`: the
shader, the paint's mask shader, the local motion's behaviour); changing only the arguments
keeps the key. At time `t`, the clock's origin is the earliest start of the trailing run of
started items on the slot that all carry the current key. When that run is every started item
and the step's base has the same key, the origin is the one carried over from the previous step
(`ScreenRuntime::carried`, filled by `enter_step` from the previous runtime's `final_origins`);
if none was carried, or the run is empty, it is the step entry. The effect time is
`(t − origin) / 1000` seconds, 0 when animations are off. The `from` side of a layer
transition takes its origin just before the transition started; the mask of a `Masked`
transition runs on the seconds since the transition started.

**Local motion.** `LocalMotionBehavior::map_ms` maps the clock before the shader sees it:
`Stall { after }` holds at `after`; `Periodic { after, period }` runs to `after`, then loops
`period` (a zero period holds at `after`).

**Travel.** `Entity::travel(to, motion)` reads the entity's pose as `from`, checks the motion's
easing, commits three tweens (position, scale, rotation to `to`, with the motion's duration,
easing and delay), so the logical pose is `to` at once, then schedules
`ScheduledKind::Travel { motion, from, to }` on `Target::Travel(id)` with the motion's delay.
While the latest started travel runs, the entity gets a `MotionStage::Transitional` with the
eased progress and the seconds since the travel started; afterwards the tweens have reached
`to`. A local motion adds a `MotionStage::Local` after it; the stage input is the pose the CPU
sampled (tweens and keyframed motions included).

**Assets.** `enter_step` loads the source of every shader the new and the base logical screen
use, and of the effects, paints, masked transitions and travels the step schedules
(`AssetStore::ensure_shader`; a source that is not UTF-8 is an asset error). The renderer reads
them with `AssetStore::shader_source`.

## Audio model

`AudioModel` (`audio/model.rs`) turns screen and system audio into `AudioCommand`s; the platform
plays them. Every gain is linear amplitude.

| Source | Bus |
| ------ | --- |
| the game's music channel, the system music | `Music` |
| ambience channels, one-shot sounds, UI sounds | `Sound` |
| voices (`Say::voice`, `Narrative::replay_voice`) | `Voice` |

| Command | Meaning |
| ------- | ------- |
| `SetBusGain { bus, gain }` | master × bus volume (0 when either is muted), sent when it changes |
| `Play { id, path, bus, start_ms, end_ms, loop_region, gain, fade_in_ms, delay_ms }` | start a sound under an engine id |
| `SetGain { id, gain, tween_ms }` | channel volume tween (linear ramp; easing is ignored) |
| `Stop { id, fade_out_ms }` | stop a sound |
| `Restart { id }` | `replay_bgm()`: restart the music channel's sound |

The game's channels follow the logical screen: a channel change on the timeline switches the
track with its `AudioFade` (overlapping or sequential). `begin_game` stops the system music with
its fade-out; `end_game` stops all game audio and restores the requested system music;
`sync_game` (after `Start` and `Restore`) makes every channel play its logical track without
fades. `stop_all` is the crash path. Every sound that starts records its asset in `heard`;
`flush_heard` moves new entries into the heard set.

## Input and bindings

`InputEvent` is `Key`, `MouseButton`, `MouseMove` (design pixels; outside the view means no hover),
`Wheel` (notches, positive is up), `Pad` and `Focus`. `input::dispatch`:

- drops repeated presses and unmatched releases (`SystemState::held`);
- offers a press to the top mount's UI first; when the UI consumes it, its release is consumed too;
- fires every binding of an unconsumed press, in list order; the wheel fires `WheelUp`/`WheelDown`
  bindings;
- on the release of a `HoldSkip` input, ends hold-skip; `Focus(false)` releases everything held.

`Action::Back` and `Action::Open` work in every mode; the other actions only while the game mode is
on top. `Advance` first shows a hidden window; `ToggleSkip`/`ToggleAuto` toggle against `Normal`;
`QuickSave` and `QuickLoad` use `SaveSlot::Quick`; `OpenInGame` queues a `CallGame`. Focus
movement and activation are built in (`Enter`, `Tab`, arrows, pad D-pad and `South`) and not
bindable.

## Settings, meta and seen sets

`store/mod.rs` keeps two stores:

| Store | Contents |
| ----- | -------- |
| `settings.kisave` | every `Setting` static by static path |
| `meta.kisave` | `values` (every `Meta` static by path), `read` (read keys), `seen_image`, `heard`, `watched` (asset paths), `auto_next` (the next autosave slot) |

Both start at the statics' init constants and are overlaid from disk at boot. A stored value
replaces the initial one only if it decodes against the current program and has the same shape
(`same_shape`: same ADT type and, for the same variant, recursively the same field shapes); else
it is dropped with a warning. Writes set a dirty flag; dirty stores are written at the end of every
`update` and on quit. Values must be storable: booleans, integers, `Decimal`, `Rat`, strings,
texts whose effect values are storable, assets, handles, tuples, arrays, vectors and ADTs of
those. `Setting::set` and `Meta::set` trap on anything else
(``a `Meta` value cannot be stored: it holds a <tag>``).

`Setting::set` invalidates `Setting(id)`; when the setting is a volume or `fullscreen` of
`System::settings`, the engine re-applies the bus gains and the window state. `Meta::write` (from
the story) is ignored while replaying, so replaying a save never writes meta twice.

Seen sets hold asset ids of this program plus the paths of stored assets the program no longer
has (`retained`), which are written back unchanged, so an update that drops an asset does not
erase its entry. An image is seen when it is on a step's logical screen; audio is heard when it
starts. Read flags, seen sets and meta are never reverted by a load or a rewind.

## Assets

`AssetStore` decodes on first use and keeps everything for the process lifetime: images
(`image` crate, to straight-alpha RGBA8), fonts (all loaded at boot, in asset order, into
`kagari_text::Fonts`), shader sources (UTF-8 WGSL), audio lengths (through
`Platform::audio_duration_ms`, cached) and save thumbnails (with a generation counter so the
renderer can tell a replaced thumbnail). A step's images, shaders, transition contents and rule
masks are loaded at step entry, so a missing image or shader traps there rather than
mid-animation.

## Profile `native_v0` and `kagari-compat`

A runtime profile names the set of decoders a runtime provides. `kagari-compat` defines the
profiles (`RuntimeProfile`, currently only `NativeV0` = `native_v0`), content sniffing of asset
bytes (`sniff`, rule order matters: the first matching rule wins, every read is bounds-checked),
and the table of which formats a profile decodes (`supports`, `supports_kind`). Its only users are
`kagari-project` (parsing `build.runtime-compatibility`) and `kagari build` (rejecting assets a
listed profile cannot decode; [Projects and the CLI](../tooling/cli.md)). The runtime itself does
not consult it.

| Kind | `native_v0` decodes | Implemented by |
| ---- | ------------------- | -------------- |
| Image | PNG, baseline and progressive JPEG, lossy, lossless and extended WebP | `image` with features `png`, `jpeg`, `webp` (engine) |
| Audio, voice | Ogg Vorbis, FLAC, WAV (PCM, IEEE float), MP3 | `kira` with features `ogg`, `vorbis`, `flac`, `wav`, `pcm`, `mp3` (desktop) |
| Font | TrueType, OpenType (CFF), collections (face 0) | `skrifa` (`kagari-text`) |
| Shader | WGSL of the six roles | `kagari-shader` (preludes, validation), `kagari-render` (pipelines) |
| Video | none | movies are modelled in the screen but never drawn |

The table and the decoder features must agree: a format `supports` accepts must decode at run
time. `watched` is never set by `native_v0`, since no movie plays.

## Errors: traps and the Crashed state

There are three kinds of failure:

- **Rust panics.** Every runtime crate denies `unwrap`, `expect`, `panic!` and unchecked
  arithmetic (`#![deny(clippy::unwrap_used, clippy::panic, clippy::expect_used,
  clippy::arithmetic_side_effects)]`). Engine code must not panic on any input, including
  malformed bytecode, saves and stores.
- **Traps.** A VM panic ([The VM](./vm.md)) or a host trap. `system::crash::crash` stops the game:
  it logs the trap, drops pending navigation and story input, stops all audio, renders the trap
  message with `Trap::render(program)` and lays it out in the system font at 18 px. Later traps
  are ignored and every `Engine` call does nothing. `frame()` returns a black frame with the
  message at (32, 32); `Engine::crash()` returns the message for the runtime.
- **Expected failures.** A divergent save, an unreadable store or a missing archive are not
  traps: `nav::load` returns `Err(Divergence)`, and the others are logged and fall back (initial
  store values, an empty slot, a silent sound).

## Invariants

- The engine never reads a clock, the file system or a random source directly; only `now_ms` and
  `Platform`. Story-visible behaviour depends only on the bytecode, the trace, the seed and the
  inputs ([Saves](./saves.md#determinism)).
- Navigation and story input are applied only by `settle`, after the outermost VM entry.
- Nothing outside the fiber holds story state; the system sees the game only through
  `Narrative`, `Arm` and the `SourceKey`s.
- `values/table.rs` matches the std `.kis` declarations field for field; every std `#[builtin]`
  the compiler emits as an import has an `import_table!` entry.
- The engine's screen model and the compiler's `eval/machine/screen.rs` agree on every operation,
  default and equality rule; the consistency analysis is only sound if they do.
- `kagari_text::units` is the unit of both the reveal plan and the layout; a glyph's `unit` must
  index the same grapheme the reveal counts.
- While `replaying`, nothing persistent or audible happens.
- No Rust panic on any input.

## When you change this

- A new or changed std builtin: the `.kis` declaration in `core/kagari-std`, the import table
  entry, `values/table.rs` and decoders when a type changes, the fixture
  `tests/fixtures/std_types/main.kix`, the STD Reference page (en and ja), and
  [Builtins and the Host ABI](../std/builtins.md).
- A screen operation: also the compiler's `eval/machine/screen.rs` and
  [Constant Evaluation](../compiler/eval.md); the STD pages `screen.md`, `screen-anim.md`,
  `screen-audio.md`.
- Mode, navigation, play-mode or binding behaviour: the Reference
  [The Game System](../../../reference/en/src/concepts/system.md) (en and ja), the STD pages
  `system.md`, `system-nav.md`, `narrative.md`, and the system checks in the compiler when the mode
  graph changes.
- A new runtime profile or format: `kagari-compat` (`profile.rs`, `format.rs`, `sniff.rs`, tests),
  the decoder features in the workspace `Cargo.toml`, the STD appendix
  [Runtime Profiles](../../../std/en/src/appendix/profiles.md), the Reference
  [Projects](../../../reference/en/src/tools/projects.md) (en and ja).
- Engine tests live in `src/tests/` and next to each module; they compile fixtures with
  `kagari-lang` (`test_support::compile`). Run `cargo test -p kagari-engine`, then a headless smoke
  run ([The Desktop Runtime](./desktop.md#headless-mode)).
- Engine source comments still cite the old books (`docs/syntax/src/system.md`,
  `docs/std/src/text.md`, `ui.md`, `checks.md`); point them at the new chapters when touching those
  files.
