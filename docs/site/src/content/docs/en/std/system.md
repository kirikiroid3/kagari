---
title: "std::system"
---

> **Stability:** Stable — the program description, settings, mode types, seen sets and input
> bindings.

`std::system` describes the game system: everything around the story, such as the title page,
the save and load pages, the settings page, the backlog and the in-game UI. A game's `main`
function returns a [`System`](#system) value, and the engine runs the program it describes.

System code is `fn` code. Constructors (`setting`, `volume`, `bind`, `default_bindings`) are
`const fn`, so they may appear in `static` and `const` initializers.

The module has two submodules:

- [`std::system::nav`](./system-nav.md) changes the mode stack.
- [`std::system::saves`](./system-saves.md) reads and deletes save slots.

```kis
pub mod nav;
pub mod saves;
```

The prelude imports `nav`, `Setting`, `setting`, `System`, `Settings`, `Volume`, `volume`,
`RootPage`, `Overlay`, `GameOverlay` and `Dialog`. Import the other items by path, for example
`use std::system::{seen_image, default_bindings};`.

## Overview {#overview}

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Setting<T>`](#setting) | builtin struct | A persistent global setting, stored outside saves |
| [`setting(init)`](#setting-fn) | `const fn` | Declares a setting |
| [`Volume`](#volume), [`volume(level)`](#volume-fn) | struct, `const fn` | A bus volume |
| [`Settings`](#settings) | struct | The setting handles the engine reads |
| [`RootPage`](#rootpage), [`Overlay`](#overlay), [`GameOverlay`](#gameoverlay), [`Dialog`](#dialog) | structs | The mode types |
| [`System`](#system) | struct | The program, returned by `main` |
| [`seen_image`](#seen_image), [`heard`](#heard), [`watched`](#watched) | `fn` | The seen sets |
| [`set_music`](#set_music) | `fn` | The system music channel |
| [`Key`](#key), [`MouseButton`](#mousebutton), [`PadButton`](#padbutton), [`Input`](#input) | enums | Bindable inputs |
| [`Action`](#action) | enum | What a binding does |
| [`Binding`](#binding), [`bind`](#bind), [`default_bindings`](#default_bindings) | struct, `const fn` | Input bindings |

## Struct: `Setting<T>` {#setting}

```kis
#[builtin] pub struct Setting<T>;
```

A handle to a persistent, global setting. Its value is stored in the config file, never in a
save, so loading a save or rewinding never changes it.

An immutable `static` whose initializer is [`setting(init)`](#setting-fn) declares a setting:

```kis
static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static MUSIC: Setting<Volume> = setting(volume(60));
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static PETALS_ON: Setting<bool> = setting(true);
```

- The engine reads the handles listed in [`Settings`](#settings). A studio may declare any
  further settings for its own UI and renders, such as `PETALS_ON` above.
- In `fn` code, call [`get`](#setting-get) and [`set`](#setting-set).
- In a `const fn` (a render), a handle is only a value: it may be passed to `Entity::gate` (see
  [`std::screen`](./screen.md)), which draws an entity only while a `Setting<bool>` is `true`.
  `get` and `set` are `fn` and cannot be called there. See
  [Compiler Rules](./appendix/rules.md).
- Settings never reach the story's control flow, so every story analysis holds for every value of
  every setting. Compare [`Meta<T>`](./narrative.md), which the story may write.

### `setting(init)` {#setting-fn}

```kis
#[builtin] pub const fn setting<T>(init: T) -> Setting<T>;
```

- `init` `T` — the value used when the config file has no value for this setting, for example on
  first boot or after an update adds the setting.
- Returns: `Setting<T>` — the handle.
- Kind: `const fn` · Builtin: yes

### `Setting::get()` {#setting-get}

```kis
#[builtin] pub fn get(&self) -> T;
```

- Returns: `T` — the current value.
- Kind: `fn` · Builtin: yes

`get` is a reactive source: a reactive attribute or [`effect`](./ui.md#effect) that calls it
re-runs when the value changes.

### `Setting::set(value)` {#setting-set}

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — the new value. It is written to the config file.
- Kind: `fn` · Builtin: yes

```kix
button!{on_click = || FULLSCREEN.set(!FULLSCREEN.get()), t!["Fullscreen"]}
```

## Struct: `Volume` {#volume}

```kis
pub struct Volume {
    pub level: Int<0, 100>,
    pub muted: bool,
}
```

The value of a volume setting.

- `level` — the volume in percent. Because it is an `Int<0, 100>`, a
  [`slider!`](./ui.md#slider) bound to it runs from 0 to 100 without any range attributes.
- `muted` — `true` silences the bus without losing `level`.

### `volume(level)` {#volume-fn}

```kis
pub const fn volume(level: Int<0, 100>) -> Volume {
    Volume { level, muted: false }
}
```

- `level` `Int<0, 100>` — the level in percent.
- Returns: `Volume` — an unmuted volume at `level`.
- Kind: `const fn` · Builtin: no

## Struct: `Settings` {#settings}

```kis
pub struct Settings {
    /// Per character; `ZERO` is instant.
    pub text_speed: Setting<Duration>,
    /// The wait after a line in auto mode.
    pub auto_delay: Setting<Duration>,
    /// Added to `auto_delay` per character of the line.
    pub auto_delay_per_char: Setting<Duration>,
    /// Whether skip mode also skips unread text.
    pub skip_unread: Setting<bool>,
    pub master: Setting<Volume>,
    pub music: Setting<Volume>,
    pub sound: Setting<Volume>,
    pub voice: Setting<Volume>,
    /// Whether a voice keeps playing into the next step.
    pub voice_sustain: Setting<bool>,
    /// Whether auto mode waits for the voice.
    pub wait_for_voice: Setting<bool>,
    pub window_opacity: Setting<Int<0, 100>>,
    pub fullscreen: Setting<bool>,
    /// `false`: every duration counts as 0.
    pub animations: Setting<bool>,
}
```

The setting handles the engine itself reads, given in [`System::settings`](#system). Each field
is a handle declared with [`setting`](#setting-fn); the struct only tells the engine which
handle plays which role.

| Field | Meaning |
| ----- | ------- |
| `text_speed` | The reveal time per character; `ZERO` reveals a line at once |
| `auto_delay`, `auto_delay_per_char` | In auto mode, the wait after a line: `auto_delay` plus `auto_delay_per_char` per character of the line. Also the wait of a click pause in auto mode |
| `skip_unread` | Whether skip mode also skips text the player has not read |
| `master`, `music`, `sound`, `voice` | The bus volumes; see [`std::screen::audio`](./screen-audio.md) |
| `voice_sustain` | Whether a voice keeps playing into the next step |
| `wait_for_voice` | Whether auto mode waits for the voice to end |
| `window_opacity` | The opacity of the text window, in percent |
| `fullscreen` | Fullscreen (`true`) or windowed |
| `animations` | `false`: every duration counts as 0, so animations and transitions end at once |

## Mode types {#mode-types}

The system is a stack of **modes**. The kind of a mode is the return type of its component, a
`fn` that builds its UI. Each mode type wraps an [`Element`](./ui.md#element), as in
`Overlay(column![…])`.

| Mode | Component signature | Examples | On the stack |
| ---- | ------------------- | -------- | ------------ |
| root page | `fn() -> RootPage` | Title, Gallery, Credits | Only as the base; never together with a game |
| game mode | `fn(Narrative) -> Element` ([`System::game`](#system)) | The game view | Only as the base |
| overlay | `fn() -> Overlay` | Config, Load | Pushed over any mode |
| game overlay | `fn(Narrative) -> GameOverlay` | Save, Backlog | Pushed only over a game base, through `Narrative::call` |
| dialog | `fn() -> Dialog` | Confirmations | Pushed over any mode; drawn over a still image of the modes below |

- Input goes to the top mode only.
- While an overlay or dialog is on top of the game mode, the story is paused and the screen's
  running animations are completed, so the modes below are still.
- Only the game view and game overlays receive a `Narrative` handle, so an operation that needs a
  running game (saving, opening the backlog) cannot be written in a root page: it does not
  type-check.
- [`std::system::nav`](./system-nav.md) lists the operations that change the stack. The compiler
  checks the graph of modes these operations build (unreachable modes, modes that cannot be left,
  an unbounded stack); see the
  [game system diagnostics](../diagnostics/system.md).

### Struct: `RootPage` {#rootpage}

```kis
pub struct RootPage(pub Element);
```

A base page of the mode stack, such as a title page, a gallery or the credits. The
[`System::start`](#system) page and the targets of [`nav::goto`](./system-nav.md#goto) are root
pages.

### Struct: `Overlay` {#overlay}

```kis
pub struct Overlay(pub Element);
```

A mode pushed over any mode by [`nav::call`](./system-nav.md#call) or
[`Action::Open`](#action), such as a config or load page.

### Struct: `GameOverlay` {#gameoverlay}

```kis
pub struct GameOverlay(pub Element);
```

A mode pushed over the game by `Narrative::call` or [`Action::OpenInGame`](#action). Its
component receives the running game's `Narrative` handle, so it can save, read the backlog or
rewind. Save pages, backlogs and in-game menus are game overlays.

### Struct: `Dialog` {#dialog}

```kis
pub struct Dialog(pub Element);
```

A modal mode pushed by [`nav::dialog`](./system-nav.md#dialog), such as a confirmation. It is
drawn over a still image of the modes below.

## Struct: `System` {#system}

```kis
pub struct System {
    pub title: Text,
    /// The design resolution every pixel value refers to.
    pub resolution: Size,
    pub start: fn() -> RootPage,
    pub game: fn(Narrative) -> Element,
    pub save_slots: u32,
    pub auto_slots: u32,
    /// The maximum number of backlog entries.
    pub backlog: u32,
    /// The default font: the font of text whose style and effects name none.
    pub font: FontAsset,
    /// The default style of `ui::label` and `text!`.
    pub text: TextStyle,
    pub settings: Settings,
    pub bindings: Vec<Binding>,
}
```

The program. `main` is a `fn` in the root module that returns a `System`; the engine runs it once
at boot. Every field is required.

| Field | Meaning |
| ----- | ------- |
| `title` | The window title |
| `resolution` | The design resolution. Every position and length in renders and UI is in pixels of it |
| `start` | The root page the system starts on |
| `game` | The view of the game mode. It receives the `Narrative` handle and usually draws `game_screen!` and the message window |
| `save_slots` | The number of manual slots: `SaveSlot::Manual(0)` … `Manual(save_slots - 1)` |
| `auto_slots` | The number of autosave slots: `SaveSlot::Auto(0)` … `Auto(auto_slots - 1)` |
| `backlog` | The maximum number of `Narrative::history` entries |
| `font` | The default font: the font of any text whose `TextStyle` and text effects name no font. Required |
| `text` | The default `TextStyle` of [`ui::label`](./ui.md#label) and [`text!`](./ui.md#text) outside buttons and toggles |
| `settings` | The setting handles the engine reads |
| `bindings` | The input bindings |

`main` runs as ordinary code, so its calls count toward the recursion depth like any other call.

```kix
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
        start: Title,
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

fn Title() -> RootPage {
    RootPage(button!{on_click = || nav::quit(), t!["Quit"]})
}
```

A complete title page, game view and settings page are in
[System Recipes](./guides/system.md).

## Seen sets {#seen-sets}

The seen sets are meta values the engine keeps by itself. They record what the player has ever
seen, heard or watched, across all plays. Like read-text flags they are never reverted by load or
rewind. They are reactive sources. They back a CG gallery, a music room and a movie room without
any studio bookkeeping.

### Function: `seen_image(image)` {#seen_image}

```kis
#[builtin] pub fn seen_image(image: ImageAsset) -> bool;
```

- `image` `ImageAsset` — the image.
- Returns: `bool` — whether `image` has ever been on a step's logical screen, on any image layer.
- Kind: `fn` · Builtin: yes

### Function: `heard(audio)` {#heard}

```kis
#[builtin] pub fn heard(audio: AudioAsset) -> bool;
```

- `audio` `AudioAsset` — the track or sound.
- Returns: `bool` — whether `audio` has ever played: a `Loop` started in a channel, or a
  one-shot sound played.
- Kind: `fn` · Builtin: yes

### Function: `watched(video)` {#watched}

```kis
#[builtin] pub fn watched(video: VideoAsset) -> bool;
```

- `video` `VideoAsset` — the movie.
- Returns: `bool` — whether `video` has ever played.
- Kind: `fn` · Builtin: yes

## Function: `set_music(track, fade)` {#set_music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

- `track` `Option<Loop>` — the track to play; `None` stops the system music.
- `fade` `AudioFade` — how the channel goes from the old track to the new one, for example
  `crossfade(ms(500))`.
- Kind: `fn` · Builtin: yes

Sets the **system music channel**. It is heard while no game runs on the mode stack, for example
on the title page or in a music room. While a game runs, the game screen's music channel plays
instead. Only system code can call it, because it is a `fn`. See
[`std::screen::audio`](./screen-audio.md) for `Loop`, `looped` and `AudioFade`.

> **Stability:** Planned — reading what is currently playing (the current track, and a music
> room's playback state such as position or pause) is not designed yet.

## Bindings {#bindings}

[`System::bindings`](#system) maps inputs to actions. It is a `Vec<Binding>`, so a studio starts
from [`default_bindings()`](#default_bindings) and adds or replaces entries.

**Focus keys are built in and not bindable.** The arrow keys, the D-pad, Tab and Shift+Tab
move focus between controls; Enter and `PadButton::South` activate the focused control (see
[Focus](./ui.md#focus)).

### Enum: `Key` {#key}

```kis
pub enum Key {
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Shift,
    Control,
    Alt,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
}
```

A keyboard key: Enter, Escape, Space, Backspace, Tab, the arrow keys, PageUp/PageDown, Home/End,
the modifiers Shift, Control and Alt, F1–F12, the letters A–Z and the digits `Digit0`–`Digit9`.
Modifiers are keys of their own; there are no key combinations.

### Enum: `MouseButton` {#mousebutton}

```kis
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelUp,
    WheelDown,
}
```

A mouse button or a wheel direction. `Back` and `Forward` are the side buttons.

### Enum: `PadButton` {#padbutton}

```kis
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    Start,
    Select,
    LeftStick,
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}
```

A gamepad button, named by position, not by label: `South`, `East`, `West` and `North` are the
bottom, right, left and top face buttons. `LeftStick` and `RightStick` are the stick presses.
`PadButton::South` and the D-pad also drive focus (see [Focus](./ui.md#focus)).

### Enum: `Input` {#input}

```kis
pub enum Input {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}
```

An input that can be bound.

### Enum: `Action` {#action}

```kis
pub enum Action {
    Advance,
    HoldSkip,
    ToggleSkip,
    ToggleAuto,
    HideWindow,
    QuickSave,
    QuickLoad,
    Back,
    Open(fn() -> Overlay),
    OpenInGame(fn(Narrative) -> GameOverlay),
}
```

What a binding does.

| Action | Effect | Applies when |
| ------ | ------ | ------------ |
| `Advance` | Sends `NextStep`, or first completes the step's running timeline | The game mode is on top |
| `HoldSkip` | Skip mode while the input is held | The game mode is on top |
| `ToggleSkip`, `ToggleAuto` | Switches skip mode or auto mode on or off | The game mode is on top |
| `HideWindow` | Hides the text window | The game mode is on top |
| `QuickSave`, `QuickLoad` | Writes or loads [`SaveSlot::Quick`](./system-saves.md#saveslot) | The game mode is on top |
| `OpenInGame(o)` | Pushes the game overlay `o` | The game mode is on top |
| `Back` | [`nav::back()`](./system-nav.md#back); ignored on a base | Always |
| `Open(o)` | Pushes the overlay `o` | Always |

A binding with `Action::Back` also counts as a way out of every mode for the
[`E6003`](../diagnostics/system.md) check.

### Struct: `Binding` {#binding}

```kis
pub struct Binding {
    pub input: Input,
    pub action: Action,
}
```

An input bound to an action.

### Function: `bind(input, action)` {#bind}

```kis
pub const fn bind(input: Input, action: Action) -> Binding {
    Binding { input, action }
}
```

- `input` `Input` — the input.
- `action` `Action` — what it does.
- Returns: `Binding` — `input` bound to `action`.
- Kind: `const fn` · Builtin: no

### Function: `default_bindings()` {#default_bindings}

```kis
pub const fn default_bindings() -> Vec<Binding> {
    let mut bindings = Vec::new();
    bindings.push(bind(Input::Key(Key::Enter), Action::Advance));
    bindings.push(bind(Input::Key(Key::Space), Action::Advance));
    bindings.push(bind(Input::Mouse(MouseButton::Left), Action::Advance));
    bindings.push(bind(Input::Mouse(MouseButton::WheelDown), Action::Advance));
    bindings.push(bind(Input::Pad(PadButton::South), Action::Advance));
    bindings.push(bind(Input::Key(Key::Control), Action::HoldSkip));
    bindings.push(bind(Input::Pad(PadButton::RightTrigger), Action::HoldSkip));
    bindings.push(bind(Input::Mouse(MouseButton::Right), Action::HideWindow));
    bindings.push(bind(Input::Pad(PadButton::North), Action::HideWindow));
    bindings.push(bind(Input::Key(Key::Escape), Action::Back));
    bindings.push(bind(Input::Pad(PadButton::East), Action::Back));
    bindings
}
```

- Returns: `Vec<Binding>` — the default bindings below.
- Kind: `const fn` · Builtin: no

| Input | Action |
| ----- | ------ |
| `Key::Enter`, `Key::Space`, `MouseButton::Left`, `MouseButton::WheelDown`, `PadButton::South` | `Advance` |
| `Key::Control`, `PadButton::RightTrigger` | `HoldSkip` |
| `MouseButton::Right`, `PadButton::North` | `HideWindow` |
| `Key::Escape`, `PadButton::East` | `Back` |

Extend the defaults in a `const fn`:

```kis
use std::system::{Action, Binding, Input, Key, MouseButton, bind, default_bindings};

const fn bindings() -> Vec<Binding> {
    let mut list = default_bindings();
    list.push(bind(Input::Key(Key::A), Action::ToggleAuto));
    list.push(bind(Input::Key(Key::L), Action::OpenInGame(Backlog)));
    list.push(bind(Input::Mouse(MouseButton::WheelUp), Action::OpenInGame(Backlog)));
    list
}
```

> **Stability:** Planned — binding an input to an arbitrary `fn` (for example a fullscreen
> toggle) and touch input are not supported yet.
