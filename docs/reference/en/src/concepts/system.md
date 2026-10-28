# The Game System

The game system is everything around the story: the title screen, save and load pages, settings,
the backlog, the in-game UI. It is written as `fn` and `const fn` code, with no restriction beyond
the [call rules](./kinds.md#call-rules): it cannot call the story, only start it, and it cannot read
story variables.

This chapter gives the model of the system: two nested machines, the program description, UI
components, modes, the `Narrative` handle and the checks on the mode graph. The items it names are
specified in the STD Reference: [`std::system`](../../../../std/en/src/system.md),
[`std::system::nav`](../../../../std/en/src/system-nav.md),
[`std::system::saves`](../../../../std/en/src/system-saves.md),
[`std::narrative`](../../../../std/en/src/narrative.md) and
[`std::ui`](../../../../std/en/src/ui.md).

## Two nested machines

The whole game is two nested machines, not one machine with an interleaved stack.

- The outer **system machine** has a stack of **modes**: root pages (Title, Gallery, Credits),
  overlays (Save, Load, Config, Backlog), dialogs, and the game mode.
- The game mode holds one running story, the **story machine**: the position, call stack, story
  variables, screen, PRNG state and trace. System code sees it only through a
  [`Narrative` handle](#the-narrative-handle).

<!-- diagram: system-machines (mermaid-ish)
flowchart LR
  subgraph sys["System machine: the mode stack (top first)"]
    D["Dialog, e.g. Confirm"]
    O["GameOverlay, e.g. Save, Backlog"]
    subgraph G["Game mode (base): the System::game view"]
      S["Story machine: position, call stack, story variables, logical screen, PRNG state, trace of selections; seen only through Narrative"]
    end
    D ==> O ==> G
  end
  Settings["Settings: global, not in saves"]
  Meta["Meta: global, not in saves"]
  S ==>|save| Slots["Save slots: hold the story's trace"]
-->
{{#include ../diagrams/system-machines.svg}}

The story's call stack lives inside the game mode and never on the system stack. Three properties
follow:

- **A save cannot contain system state.** A save records the running story only. Menu frames and
  settings are not reachable from it.
- **A load replaces instead of pushing.** Loading puts the restored story into a fresh game frame.
  There is no menu stack to unwind first.
- **"The game is running" is a property of the stack.** It holds exactly when a game frame exists.

| Store | Lifetime | In a save? |
| ----- | -------- | ---------- |
| Settings | persistent, global (config file) | no |
| Meta | persistent, global (meta file: unlocks, read flags, seen sets) | no |
| Save slots | persistent | they are the saves |
| Mode stack and mode-local state | ephemeral, reset at boot | no |
| The running story | ephemeral, in the game frame | yes, as its trace; see [Saves, Replay and Determinism](./saves.md) |

## The program

```kix
// main.kix
mod story;
mod ui;

use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: ui::Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
```

`main` is a `fn` in the root module that returns a `System`. It describes the system; the engine
runs it once at boot. A program without `main` does not compile.

| Field | Meaning |
| ----- | ------- |
| `title` | The window title |
| `resolution` | The design resolution: positions and lengths in renders and UI are pixels of it; fixed for the whole game |
| `start` | The root page the system starts on, a `fn() -> RootPage` |
| `game` | The view of the game mode, a `fn(Narrative) -> Element` |
| `save_slots`, `auto_slots` | The number of manual and automatic save slots; fixed, so slot lists are finite |
| `backlog` | The maximum number of entries the backlog keeps |
| `font` | The default font: the font of text whose style and effects name none |
| `text` | The default text style of `ui::label` and `text!` |
| `settings` | The handles of the engine's settings; see [Settings and meta](#settings-and-meta) |
| `bindings` | The input bindings; see [Bindings](#bindings) |

`main` runs as ordinary code, so its calls, like every call at run time, count toward the
recursion depth: at most 2048 nested calls by default, set by the `recursion-depth` key of
[`kagari.toml`](../tools/projects.md). Deeper recursion stops the game with a runtime error.

**System music.** `std::system::set_music(track, fade)` sets the system music channel, heard
while no game runs on the mode stack, for example on the title page; while a game runs, the game
screen's music channel plays instead. `None` stops it. It is a `fn`, so only system code calls it.

## Components and signals

UI is written with the UI forms of `std::ui` in `fn` code, with the semantics of SolidJS:

```kix
// ui.kix
use crate::MUSIC;

fn Config() -> Overlay {
    let (music, set_music) = signal(MUSIC.get());
    Overlay(column![
        text!(t!["Music: " {music().level}]),
        slider!{
            value = music().level,
            on_change = |v| {
                let vol = Volume { level: v, muted: music().muted };
                set_music(vol);
                MUSIC.set(vol)
            },
        },
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

- UI forms follow the same [delimiter rules](../syntax/forms.md) as the narrative forms. In `fn`
  and `const fn` code they build **values** (of type `Element`); they are never steps.
- A **component** is a plain `fn` that returns a UI value. It runs **once**, when its mode is
  shown, and is called like any function, or passed as a pointer, as in `nav::call(Config)`.
- `signal(init)` returns a getter and a setter. Signals are first-class in `fn` code and are not
  available to the story or to `const fn`.
- An attribute value or element that reads a signal is **reactive**: it is re-evaluated whenever
  the signal changes. Other code of a component runs once. `Setting::get`, `Meta::get`, the reads
  of the `Narrative` handle, save information and the seen sets are reactive sources too.
- Handlers such as `on_click` are first-class [closures](../syntax/values.md).
- A slider's range is the range of its value's `Int` type. `Volume::level` is an `Int<0, 100>`, so
  the slider above runs from 0 to 100 without `min` or `max` attributes.

UI forms come from `std::ui`: layout (`column![…]`, `row![…]`, `layer![…]`), content
(`text!(…)`, `image!(…)`, `game_screen!(…)`), and controls (`button!{…}`, `slider!{…}`,
`toggle!{…}`, `select!{…}`). Each form desugars to a call of a `std::ui` function with a props
struct: reactive attributes become closures, handlers stay as they are, and each element becomes a
closure. The desugaring table, layout, interaction looks and focus rules are in
[`std::ui`](../../../../std/en/src/ui.md).

## Modes

The kind of a mode is the return type of its component. Each mode type wraps an `Element`, as in
`Overlay(column![…])`.

| Type | Component | Example modes | On the stack |
| ---- | --------- | ------------- | ------------ |
| `RootPage` | `fn() -> RootPage` | Title, Gallery, Credits | only as the base; never together with a game frame |
| `Overlay` | `fn() -> Overlay` | Config, Load | pushed over anything |
| `GameOverlay` | `fn(Narrative) -> GameOverlay` | Save, Backlog | pushed only over the game mode |
| `Dialog` | `fn() -> Dialog` | Confirmations | like an overlay, drawn over a still image of the frames below |
| game mode | `fn(Narrative) -> Element`, the `System::game` view | the running story | only as the base |

Input goes to the top frame only. While any overlay or dialog is on top of the game mode, the story
is paused and the screen's running animations are skipped to their final state, so the frames
below are always still.

## Operations

UI changes the system only through these operations. The navigation functions in
`std::system::nav` may be called from any `fn`:

| Operation | Effect | Condition |
| --------- | ------ | --------- |
| `nav::call(o)` | push the overlay `o` | |
| `nav::dialog(d)` | push the dialog `d` | |
| `nav::back()` | pop the top frame | ignored on a base |
| `nav::goto(p)` | reset the stack to the root page `p` | |
| `nav::start(e, args, on_exit)` | reset the stack to a new game running the entry scene `e` | |
| `nav::load(k)` | reset the stack to a game restored from slot `k` | if the save no longer fits the script, returns `Err` and leaves the stack unchanged |
| `nav::quit()` | terminate immediately, without confirmation | |

The operations on a running game are methods of the [`Narrative` handle](#the-narrative-handle):

| Operation | Effect |
| --------- | ------ |
| `game.advance()` | advance the story to its next step |
| `game.select(arm)` | select a shown arm of the current choice |
| `game.save(k)` | save the running game to slot `k` |
| `game.rewind(i)` | restore the story to backlog entry `i` |
| `game.call(o)` | push the game overlay `o` |

`nav::quit` never asks; a button that should confirm opens a `Dialog` the studio writes.

## Starting the story

```kix
button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

`nav::start` takes an [entry scene](./entry.md), its arguments, and an exit handler. This is the
only way into the story.

- `story::Main` is a pointer to an `#[entry]` scene `scene fn(P1, …, Pn) -> R`, the arguments are
  the tuple `(P1, …, Pn)`, and the exit handler is an `Fn(R)`.
- The arguments have the entry scene's finite parameter types. They are recorded in the save.
- Each `nav::start` seeds a fresh story PRNG; see [Determinism](./saves.md#determinism).
- When the entry scene returns, its value is passed to the exit handler, which must handle every
  value: an ordinary exhaustive `match`. If the handler navigates nowhere, the stack resets to
  the start page.

A **replay** of a scene from an extras page is `nav::start` of a further `#[entry]` scene, unlocked
by a `Meta<bool>` that the story writes when it first passes the scene.

## Settings and meta

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static PETALS_ON: Setting<bool> = setting(true);
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
| the handle itself | `const fn` | a [deferred reference](./presentation.md#settings-in-renders), passed to `Entity::gate` | no |

So settings never reach the story's control flow, and every story analysis holds for every
value of the settings. Meta reaches the story only through entry arguments, at start.

The engine's own settings are the handles given in `System::settings`, a `Settings` record: text
speed, the auto-mode delays, whether skip passes unread text, the master, music, sound and voice
volumes, voice sustain and waiting for voice in auto mode, window opacity, full screen, and whether
animations play (when off, every duration counts as 0). A volume is a `Volume`: a level
`Int<0, 100>` and a mute flag. A studio declares further settings as its own handles, such as
`PETALS_ON` above.

Read-text flags are meta written by the engine each time the story advances, keyed by step. The
story never sees them; the system uses them for skipping read text and to mark read lines in the
backlog.

**Seen sets** are meta the engine keeps the same way: `seen_image(i)`, `heard(a)` and `watched(v)`
in `std::system` tell whether an image was ever on a step's logical screen, a track or sound ever
played, a movie ever played. Like read flags they are never reverted by load or rewind. They back a
CG gallery, a music room and a movie room without any studio bookkeeping:

```kix
use std::system::seen_image;

fn GalleryThumb(cg: ImageAsset) -> Element {
    button!{disabled = !seen_image(cg), image!(cg)}
}
```

## The `Narrative` handle

The system sees a running story only through a `Narrative` handle in `std::narrative`. The engine
passes one to the game view (`System::game`) and to game overlays, and nowhere else. Its
representation is hidden, so system code cannot read story variables, and nothing the system does
can affect the story analyses. Every method is `fn`, and the reads are reactive:

```kis
impl Narrative {
    #[builtin] pub fn line(&self) -> Option<Line>;      // the current s! step
    #[builtin] pub fn menu(&self) -> Option<Menu>;      // the arms shown at the current choice!
    #[builtin] pub fn hud(&self) -> HudState;           // the Hud of the logical screen
    #[builtin] pub fn history(&self) -> Vec<Entry>;     // the backlog, oldest first
    #[builtin] pub fn mode(&self) -> PlayMode;          // normal, auto or skip
    #[builtin] pub fn set_mode(&self, mode: PlayMode);
    #[builtin] pub fn advance(&self);
    #[builtin] pub fn select(&self, arm: Arm);
    #[builtin] pub fn save(&self, slot: SaveSlot);
    #[builtin] pub fn rewind(&self, entry: u32);
    #[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
    #[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
}
```

`game_screen!(game = game)` draws the screen of the running story, and `ui::message(game, style)`
the current line with its reveal, effects and cues. The records `Line`, `Menu`, `HudState`, `Entry`
and `PlayMode` are specified in [`std::narrative`](../../../../std/en/src/narrative.md).

Because the handle exists only in the game view and in game overlays, everything that needs a
running game, such as saving, advancing or opening a Save overlay, cannot even be written on a
title page. Saves, loading and rewinding are described in
[Saves, Replay and Determinism](./saves.md).

## Bindings

`System::bindings` maps inputs (keys, mouse buttons and wheel, pad buttons) to actions:

| Action | Effect | Applies |
| ------ | ------ | ------- |
| `Advance` | advance the story; if the window is hidden, show it instead | while the game mode is on top |
| `HoldSkip`, `ToggleSkip` | skip while held / toggle skip mode | while the game mode is on top |
| `ToggleAuto` | toggle auto mode | while the game mode is on top |
| `HideWindow` | toggle hiding the text window | while the game mode is on top |
| `QuickSave`, `QuickLoad` | save to / load from the quick slot | while the game mode is on top |
| `Back` | the same as `nav::back()` | everywhere |
| `Open(o)` | push the overlay `o` | everywhere |
| `OpenInGame(o)` | push the game overlay `o` | while the game mode is on top |

`default_bindings()` binds Enter, Space, the left mouse button, wheel down and pad South to
`Advance`; Control and the right pad trigger to `HoldSkip`; the right mouse button and pad North to
`HideWindow`; Escape and pad East to `Back`. Focus movement and activation are built in and not
bindable.

## System checks

The system has no decidability concern: no analysis quantifies over its values. Its checks are
about the mode graph, built at compile time from the operations each mode's handlers can run.
Handler closures are finite in number and written in the source, so the graph can be collected
over all branches.

| Check | Rule | Diagnostic |
| ----- | ---- | ---------- |
| Reachability | A mode that can never be shown is a warning | `W6001` |
| Context typing | Game overlays, saving, advancing and selecting are methods of a handle that exists only in the game view and in game overlays, so a Save button on the title page does not type-check | a type error |
| No return from a base | No `nav::back()` in a root page or the game mode | `E6002` |
| No trap | Every reachable overlay and dialog can go back or reset the stack | `E6003` |
| Bounded stack | Modes that push each other in a cycle are a warning: the stack could grow without bound | `W6004` |
| Exhaustive endings | The exit handler handles every value of the entry scene's return type | `match` exhaustiveness |

Handlers are Turing-complete, so a handler can run forever; the compiler does not check that.

The graph is collected as follows:

- A **mode** is a `fn` with the signature of one of the [mode kinds](#modes); the game mode is
  `System::game`, and the root is `System::start`, both read from the `System` record `main`
  returns.
- The **code of a mode** is its component and, transitively, every function it calls or uses as a
  value, such as a handler. A function passed to an operation is that operation's target, not
  code of the mode. The exit handler of `nav::start` is code of the game mode: it runs when the
  story ends.
- An operation whose target is not a named function (a local holding a pointer, say) can reach
  every mode of its kind.
- **Bindings** count too: an `Action::Open` binding opens its overlay from every mode, an
  `Action::OpenInGame` binding opens its game overlay over the game, and an `Action::Back` binding
  is a way back from every overlay and dialog, so with `default_bindings()` no overlay is a trap.
  Bindings are input, not modes pushing each other, so they never make a cycle for the bounded
  stack check.
- A way out of an overlay or a dialog is `nav::back`, `nav::goto`, `nav::start`, `nav::load` or
  `nav::quit`.

The diagnostics are listed in [Game System (E6xxx)](../diagnostics/system.md).
