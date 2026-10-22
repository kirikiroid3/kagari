# The Game System

*Tentative.* This whole chapter is a sketch. The model (two nested machines, the narrative
interface, saves as witnesses) is settled; the names of types, elements and functions are not.

The game system is everything around the story: the title screen, save and load pages, settings,
the backlog, the in-game UI. It is written as `fn` and `const fn` code, with no restriction beyond
the [call rules](./kinds.md#call-rules): it cannot call the story, only start it, and it cannot read
story variables.

## Two nested machines

The whole game is two nested machines, not one machine with an interleaved stack.

- The outer **system machine** has a stack of **modes**: root pages (Title, Gallery, Credits),
  overlays (Save, Load, Config, Backlog), dialogs, and the game mode.
- The game mode holds one value of the abstract type `Narrative`: the position, call stack,
  story variables, screen and trace of a running story.

The story's call stack lives inside that value and never on the system stack. Three properties
follow:

- **A save cannot contain system state.** A save is `snapshot(&Narrative)`. Menu frames and
  settings are not reachable from the value being saved.
- **A load replaces instead of pushing.** Loading puts a new `Narrative` into a fresh game frame.
  There is no menu stack to unwind first.
- **"The game is running" is a property of the stack.** It holds exactly when a game frame exists.

| Store | Lifetime | In a save? |
| ----- | -------- | ---------- |
| Settings | persistent, global (config file) | no |
| Meta | persistent, global (meta file: unlocks, read flags, gallery) | no |
| Save slots | persistent | they are the saves |
| Mode stack and mode-local state | ephemeral, reset at boot | no |
| `Narrative` | ephemeral, in the game frame | yes, as trace and snapshot |

## The program

```kix
// main.kix
mod story;
mod ui;

fn main() -> System {
    System {
        start: ui::Title,      // the root page the system starts on
        game: ui::GameView,    // the view of the game mode
        save_slots: 6,
    }
}
```

`main` is a `fn` in the root module. It describes the system; the engine runs it once at boot.
The number of save slots is fixed, so the slot list in the UI is finite.

## Components and signals

UI is written with elements in `fn` code, with the semantics of SolidJS:

```kix
#[component]
fn Config() -> Overlay {
    let (speed, set_speed) = signal(TEXT_SPEED.get());
    Overlay(
        <column>
            <text>Text speed: {speed()}</text>
            <slider min={0} max={10} value={speed()} on_change={|v| { set_speed(v); TEXT_SPEED.set(v) }} />
            <button on_click={|| nav::back()}>Back</button>
        </column>
    )
}
```

- A `#[component] fn` runs **once**, when its mode is shown, and returns a UI value. It may be used
  as a capitalized tag in other UI code.
- In `fn` and `const fn` code an element is a **value** (of type `Element`), not a step.
- `signal(init)` returns a getter and a setter. Signals are first-class in `fn` code and are not
  available to the story or to `const fn`.
- An `{expr}` in an element is **reactive**: it is re-evaluated whenever a signal it reads
  changes. Code outside elements runs once.
- Handlers such as `on_click` are first-class [closures](./values.md#closures).

Intrinsic UI elements come from `std::ui`: layout (`<column>`, `<row>`, `<layer>`), content
(`<text>`, `<image>`, `<game-screen>`), and controls (`<button>`, `<slider>`, `<toggle>`,
`<select>`). Their props, styling and focus rules are not designed yet.

## Modes

The kind of a mode is the return type of its component:

| Type | Mode | On the stack |
| ---- | ---- | ------------ |
| `RootPage` | Title, Gallery, Credits | only as the base; never together with a game frame |
| `Overlay` | Config, Load | pushed over anything |
| `GameOverlay` | Save, Backlog | pushed only over a game base |
| `Dialog` | Confirmations | like an overlay, drawn over a still image of the frames below |

Input goes to the top frame only. While any overlay or dialog is on top of the game mode, the story
is paused and the screen's running animations are skipped to their final state, so the frames
below are always still.

## Operations

UI changes the system only through these operations in `std::system` (names tentative):

| Operation | Effect | Condition |
| --------- | ------ | --------- |
| `nav::call(m)` | push the overlay or dialog `m` | `GameOverlay` only above a game base |
| `nav::back()` | pop the top frame | the top is not the base |
| `nav::goto(p)` | reset the stack to the root page `p` | |
| `nav::start(e, args, on_exit)` | reset the stack to a new game running the entry scene `e` | |
| `nav::load(k)` | reset the stack to a game restored from slot `k` | fails on divergence, see below |
| `nav::save(k)` | write a snapshot of the running game to slot `k` | a game is running |
| `nav::advance()` | send `NextStep` to the story | the top is the game mode |
| `nav::select(arm)` | select an arm of the current choice | the top is the game mode |
| `nav::rewind(i)` | restore the story to backlog entry `i` | a game is running |
| `nav::quit()` | terminate immediately, without confirmation | |

`nav::quit` never asks; a button that should confirm calls a `Dialog` the studio writes.

## Starting the story

```kix
<button on_click={|| nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending)}>Start</button>

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

`nav::start` takes an [entry scene](./narrative/scenes.md#entry-scenes), its arguments, and an exit
handler. This is the only way into the story.

- The arguments have the entry scene's finite parameter types. They are recorded in the save.
- When the entry scene returns, its value is passed to the exit handler, which must handle every
  value: an ordinary exhaustive `match`. If the handler navigates nowhere, the stack resets to
  the start page.

## Settings and meta

```kis
static TEXT_SPEED: Setting<Int<0, 10>> = setting(5);
static BGM_VOLUME: Setting<Int<0, 100>> = setting(80);
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

An immutable `static` holding a `Setting<T>` or `Meta<T>` declares a setting or a meta field. The
initializer gives the value used when the stored file has no value for it, for example on first
boot or after an update adds a field. Both are persistent and global, stored outside saves.

| Method | Kind | Setting | Meta |
| ------ | ---- | ------- | ---- |
| `get()` | `fn` | yes | yes |
| `set(v)` | `fn` | yes | yes |
| `write(v)` | `pushdown fn` | no | yes: the story writes meta, write-only |
| the handle itself | `const fn` | a [deferred reference](./presentation.md#settings-in-renders) | no |

So settings never reach the story's control flow, and every narrative analysis holds for every
value of the settings. Meta reaches the story only through entry arguments, at start.

Read-text flags are meta written by the engine on each `NextStep`, keyed by step. The story never
sees them; the system uses them for skip-read-text and to mark read lines in the backlog.

## The narrative interface

The system sees a running story only through the abstract type `Narrative` in `std::narrative`.
Its representation is hidden, so system code cannot read story variables, and nothing the system
does can affect the narrative analyses.

```kis
// std::narrative, fn-only, engine built-ins
fn current_line(n: &Narrative) -> Line;        // the current <s>
fn choices(n: &Narrative) -> Choices;          // the arms offered at the current <choice>
fn history(n: &Narrative) -> History;          // the steps passed, for the backlog
fn screen(n: &Narrative) -> &Screen;           // drawn by <game-screen>, read for the Hud
fn snapshot(n: &Narrative) -> Save;
fn restore(s: Save) -> Result<Narrative, Divergence>;
fn rewind(n: &Narrative, entry: u64) -> Narrative;
```

### A save is a witness of reachability

A save is the proposal's save format:

- the **trace**: the entry scene's path and arguments, the [stable choice
  IDs](./narrative/elements.md#stable-choice-ids) of every selection, the step count after the last
  choice, the script hash and the PRNG seed;
- a **snapshot** of σ and the screen, used as a cache. Function pointers in σ are stored by path.

The trace is a path from the start, so a save is a *witness that a position is reachable*, not a
state: no save of an unreachable position can be constructed. When the script changes, `restore`
replays the trace on the new program and returns a `Divergence` at the first choice ID that no
longer resolves. `nav::load` then shows the divergence and leaves the stack unchanged.

`rewind(n, i)` is `restore` on the trace cut at backlog entry `i`. It replays from the nearest
choice snapshot and runs renders from the last keyframe, so it costs no more than a load.

## System checks

The system has no decidability concern: no analysis quantifies over its values. Its checks are
about the mode graph, built at compile time from the operations each mode's handlers can run.
Handler closures are finite in number and written in the source, so the graph can be collected
over all branches.

| Check | Rule |
| ----- | ---- |
| Reachability | A mode that can never be shown is a warning |
| Context typing | `nav::call` of a `GameOverlay`, `nav::save`, `nav::advance` and similar need a game base; a Save button on the title page is an error |
| No return from a base | No `nav::back()` in a root page or the game mode |
| No trap | Every reachable overlay can go back or reset the stack |
| Bounded stack | Modes that push each other in a cycle are a warning: the stack could grow without bound |
| Exhaustive endings | The exit handler handles every value of the entry scene's return type; this is `match` exhaustiveness |

Handlers are Turing-complete, so a handler can run forever; the compiler does not check that.
None of the proposal's goals depend on it.
