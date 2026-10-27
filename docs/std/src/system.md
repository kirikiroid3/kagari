# The Game System

`std::system` declares the program description `System`, settings, the mode types, the seen sets
and input bindings. `std::system::nav` changes the mode stack; `std::system::saves` reads and
deletes save slots. System code is `fn`; constructors are `const fn`.

```kis
pub mod nav;
pub mod saves;
```

## `System`

```kis
pub struct System {
    pub title: Text,
    pub resolution: Size,
    pub start: fn() -> RootPage,
    pub game: fn(Narrative) -> Element,
    pub save_slots: u32,
    pub auto_slots: u32,
    pub backlog: u32,
    pub font: FontAsset,          // the font of text whose style and effects name none
    pub text: TextStyle,          // default style of `ui::label` and `text!`
    pub settings: Settings,
    pub bindings: Vec<Binding>,
}
```

`main` is a `fn` in the root module that returns the `System`; the engine runs it once at boot.

|Field|Meaning|
|---|---|
|`title`|the window title|
|`resolution`|the design resolution every pixel value refers to|
|`start`|the root page the system starts on|
|`game`|the view of the game mode; it gets the `Narrative` handle|
|`save_slots`|the number of manual slots, `Manual(0)` … `Manual(save_slots − 1)`|
|`auto_slots`|the number of autosave slots|
|`backlog`|the maximum number of `Narrative::history` entries|
|`font`|the default font: the font of text whose style and effects name none|
|`text`|the default `TextStyle` of `ui::label` and `text!`|
|`settings`|the setting handles the engine reads|
|`bindings`|input bindings|

```kix
use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: ui::Title,
        game: ui::GameView,
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
```

The game view draws the game screen and the message window, usually in a `layer!`:

```kix
use std::screen::BOTTOM_CENTER;
use std::ui::message;

fn GameView(game: Narrative) -> Element {
    layer![
        game_screen!(game = game).cover(),
        message(game, text_style(26, WHITE)).width(1000).at(point(640, 690), BOTTOM_CENTER),
    ]
}
```

## Settings

```kis
#[builtin] pub struct Setting<T>;
#[builtin] pub const fn setting<T>(init: T) -> Setting<T>;
impl<T> Setting<T> {
    #[builtin] pub fn get(&self) -> T;
    #[builtin] pub fn set(&self, value: T);
}

pub struct Volume { pub level: Int<0, 100>, pub muted: bool }
pub const fn volume(level: Int<0, 100>) -> Volume { Volume { level, muted: false } }

pub struct Settings {
    pub text_speed: Setting<Duration>,           // per character; ZERO = instant
    pub auto_delay: Setting<Duration>,           // wait after a line in auto mode
    pub auto_delay_per_char: Setting<Duration>,  // added per character of the line
    pub skip_unread: Setting<bool>,
    pub master: Setting<Volume>,
    pub music: Setting<Volume>,
    pub sound: Setting<Volume>,
    pub voice: Setting<Volume>,
    pub voice_sustain: Setting<bool>,
    pub wait_for_voice: Setting<bool>,
    pub window_opacity: Setting<Int<0, 100>>,
    pub fullscreen: Setting<bool>,
    pub animations: Setting<bool>,               // false: every duration counts as 0
}
```

An immutable `static` holding `setting(init)` declares a setting: persistent, global, stored in
the config file, never in saves. `init` is used when the file has no value for it.

```kis
static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static MUSIC: Setting<Volume> = setting(volume(60));
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
```

- `get` and `set` are `fn`; `get` is reactive. In a `const fn` a handle is only a value, passed to
  `Entity::gate` (see [Screen](./screen.md#gate)).
- `Settings` lists the handles the engine reads. A studio may declare further settings for its own
  UI.

|Setting|Meaning|
|---|---|
|`text_speed`|reveal time per character; `ZERO` is instant|
|`auto_delay`, `auto_delay_per_char`|in auto mode, the wait after a line: `auto_delay` plus `auto_delay_per_char` per character; also the wait of a click pause|
|`skip_unread`|whether skip mode also skips unread text|
|`master`, `music`, `sound`, `voice`|bus volumes; see [Audio](./audio.md#volume)|
|`voice_sustain`|whether a voice keeps playing into the next step|
|`wait_for_voice`|whether auto mode waits for the voice|
|`window_opacity`|the opacity of the text window, in percent|
|`fullscreen`|fullscreen or windowed|
|`animations`|`false`: every duration counts as 0|

A config overlay edits a setting through a signal:

```kix
fn Config() -> Overlay {
    let (music, set_music) = signal(MUSIC.get());
    Overlay(column![
        text!(t!["Music: " {music().level}]),
        slider!{
            value = music().level,
            on_change = |v| { let vol = Volume { level: v, muted: music().muted }; set_music(vol); MUSIC.set(vol) },
        },
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

## Modes

```kis
pub struct RootPage(pub Element);
pub struct Overlay(pub Element);
pub struct GameOverlay(pub Element);
pub struct Dialog(pub Element);
```

The system is a stack of modes. The kind of a mode is the return type of its component:

|Mode|Component signature|Examples|On the stack|
|---|---|---|---|
|root page|`fn() -> RootPage`|Title, Gallery, Credits|only as the base; never together with a game|
|game mode|`fn(Narrative) -> Element` (`System::game`)|the game view|only as the base|
|overlay|`fn() -> Overlay`|Config, Load|pushed over anything|
|game overlay|`fn(Narrative) -> GameOverlay`|Save, Backlog|pushed only over a game base, through `Narrative::call`|
|dialog|`fn() -> Dialog`|confirmations|pushed over anything; drawn over a still image of the modes below|

Input goes to the top mode only. While an overlay or dialog is on top of the game mode, the story
is paused and the screen's running animations are completed, so the modes below are still.

## Navigation

```kis
#[builtin] pub fn call(overlay: fn() -> Overlay);
#[builtin] pub fn dialog(dialog: fn() -> Dialog);
#[builtin] pub fn back();
#[builtin] pub fn goto(page: fn() -> RootPage);
#[builtin] pub fn start<E, A, R>(entry: E, args: A, on_exit: Fn(R));
#[builtin] pub fn load(slot: SaveSlot) -> Result<(), Divergence>;
#[builtin] pub fn quit();
```

|Operation|Effect|
|---|---|
|`nav::call(o)`|pushes the overlay `o`|
|`nav::dialog(d)`|pushes the dialog `d`|
|`nav::back()`|pops the top mode; ignored on a base|
|`nav::goto(p)`|replaces the whole stack with the root page `p`|
|`nav::start(e, args, on_exit)`|replaces the stack with a new game running the `#[entry]` scene `e`|
|`nav::load(k)`|replaces the stack with the game restored from slot `k`|
|`nav::quit()`|quits at once, without confirmation|
|`game.advance()`|sends `NextStep` to the story|
|`game.select(arm)`|selects a shown arm of the current choice|
|`game.save(k)`|writes the running game to slot `k`|
|`game.rewind(i)`|restores the story to backlog entry `i`|
|`game.call(o)`|pushes the game overlay `o`|

The last five are methods of the [`Narrative`](./narrative.md#the-narrative-handle) handle. A handle exists
only in the game view and in game overlays, so "needs a running game" is enforced by types: a Save
button on the title page does not type-check.

- **`nav::start`.** `entry` is a pointer to an `#[entry]` scene, `args` the tuple of its
  arguments, and `on_exit` gets its return value. If the handler navigates nowhere, the stack
  resets to the start page. The typing of this signature is special; see
  [Checks](./checks.md#navstartentry-args-on_exit).
- **`nav::load`.** On divergence it returns `Err` and leaves the stack unchanged.
- **`nav::quit`** never asks; a button that should confirm calls a `Dialog` the studio writes.

```kix
fn start_button() -> Element {
    button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}
}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

## Saves

```kis
pub enum SaveSlot { Manual(u32), Quick, Auto(u32) }
pub struct Timestamp { pub year: u16, pub month: u8, pub day: u8, pub hour: u8, pub minute: u8, pub second: u8 }
pub struct SaveInfo { pub time: Timestamp, pub who: Option<Text>, pub text: Option<Text> }
#[builtin] pub struct Divergence;
impl Divergence { #[builtin] pub fn describe(&self) -> Text; }
#[builtin] pub fn info(slot: SaveSlot) -> Option<SaveInfo>;
#[builtin] pub fn delete(slot: SaveSlot);
```

- `info(slot)` is the slot's metadata, or `None` if it is empty. It is reactive. Its `who` and
  `text` are the line at save time; `time` is local time.
- A `Manual(k)` slot with `k >= save_slots`, or an `Auto(k)` slot with `k >= auto_slots`, is a
  runtime no-op: `info` → `None` and `save` does nothing.
- `delete(slot)` empties the slot.
- `Divergence::describe` is a description of where a save no longer fits the changed script, for
  the player.
- `ui::thumbnail(slot)` shows the slot's screenshot.

**Autosave.** At every choice step entry and on quit, the engine writes `SaveSlot::Auto(k)`,
rotating through `k` in `0..auto_slots`. There is one quick slot, `SaveSlot::Quick`, written and
read by the `QuickSave` and `QuickLoad` actions.

```kix
use std::system::saves;
use std::system::saves::SaveSlot;
use std::ui::thumbnail;

fn slot_text(slot: SaveSlot) -> Text {
    match saves::info(slot) {
        Some(info) => match info.text {
            Some(text) => text,
            None => "",
        },
        None => "Empty",
    }
}

fn Load() -> Overlay {
    let (error, set_error) = signal("");
    Overlay(column![
        button!{
            on_click = || match nav::load(SaveSlot::Manual(0)) {
                Ok(()) => {}
                Err(divergence) => set_error(divergence.describe()),
            },
            thumbnail(SaveSlot::Manual(0)),
            text!(slot_text(SaveSlot::Manual(0))),
        },
        text!(error()),
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

## Seen sets

```kis
#[builtin] pub fn seen_image(image: ImageAsset) -> bool;
#[builtin] pub fn heard(audio: AudioAsset) -> bool;
#[builtin] pub fn watched(video: VideoAsset) -> bool;
/// Sets the system music channel: heard while no game runs on the mode stack; while a game
/// runs, the game screen's music channel plays instead. `None` stops it.
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

These read engine-maintained meta:

- an image is seen once it is in a step's logical screen, on any image layer;
- audio is heard once a `Loop` starts in a channel or a one-shot plays;
- a movie is watched once it plays.

They are never reverted by load or rewind, just like read flags. They are reactive. They back the
CG gallery, the music room and the movie room without any studio bookkeeping.

`set_music(track, fade)` plays music outside the game: a music room or the title page sets the
system music channel with it. It is heard only while no game runs on the mode stack. What is
currently playing cannot be read yet.

### Gallery

```kix
use std::system::seen_image;
use std::ui::Fit;

const LOCKED_TILE: ImageAsset = asset("gallery_locked.png");

fn cg_or_lock(cg: ImageAsset) -> ImageAsset {
    if seen_image(cg) { cg } else { LOCKED_TILE }
}

fn cg_tile(cg: ImageAsset) -> Element {
    image!(fit = Fit::Cover, cg_or_lock(cg)).width(320).height(180)
}

fn Gallery() -> RootPage {
    RootPage(column![
        row![cg_tile(CG::FIREWORKS), cg_tile(CG::ROOF), cg_tile(CG::STATION)],
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ])
}
```

The source of `image!` is reactive, so a tile unlocks as soon as the set changes.

### Music room

```kix
use std::system::heard;

fn track_title(title: Text, track: AudioAsset) -> Text {
    if heard(track) { title } else { "???" }
}

fn MusicRoom() -> RootPage {
    RootPage(column![
        text!(track_title("Theme", THEME)),
        text!(track_title("Evening", EVENING)),
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ])
}
```

Playing a track from a system mode is not in `std` yet; see [Open Questions](./open-questions.md).

### Replay

A replay is a further `#[entry]` scene, started with `nav::start`. It is unlocked by a `Meta<bool>`
that the story writes when the player first reaches the scene:

```kix
// story.kix
pub static SEEN_FESTIVAL: Meta<bool> = meta(false);

#[scene]
pushdown fn FestivalNight() {
    s!(render = festival, t!["The lanterns came on one by one."]);
    // …
    SEEN_FESTIVAL.write(true);
}

#[scene]
#[entry]
pub pushdown fn FestivalReplay() {
    FestivalNight();
}
```

```kix
// ui.kix
fn Replays() -> RootPage {
    RootPage(column![
        button!{
            disabled = !story::SEEN_FESTIVAL.get(),
            on_click = || nav::start(story::FestivalReplay, (), on_replay_end),
            t!["Festival night"],
        },
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ])
}

fn on_replay_end(_end: ()) {
    nav::goto(Replays)
}
```

A replay starts a fresh story: story variables have their initial values. State the replayed scene
needs is passed as entry arguments.

## Bindings

```kis
pub enum Key {
    Enter, Escape, Space, Backspace, Tab, Up, Down, Left, Right, PageUp, PageDown, Home, End,
    Shift, Control, Alt, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
}
pub enum MouseButton { Left, Right, Middle, Back, Forward, WheelUp, WheelDown }
pub enum PadButton {
    South, East, West, North, LeftShoulder, RightShoulder, LeftTrigger, RightTrigger,
    Start, Select, LeftStick, RightStick, DpadUp, DpadDown, DpadLeft, DpadRight,
}
pub enum Input { Key(Key), Mouse(MouseButton), Pad(PadButton) }
pub enum Action {
    Advance, HoldSkip, ToggleSkip, ToggleAuto, HideWindow, QuickSave, QuickLoad, Back,
    Open(fn() -> Overlay),
    OpenInGame(fn(Narrative) -> GameOverlay),
}
pub struct Binding { pub input: Input, pub action: Action }
pub const fn bind(input: Input, action: Action) -> Binding { Binding { input, action } }
/// Enter, Space, MouseLeft, WheelDown, PadSouth → Advance; Control, PadRightTrigger → HoldSkip;
/// MouseRight, PadNorth → HideWindow; Escape, PadEast → Back.
pub const fn default_bindings() -> Vec<Binding> { … }
```

|Action|Effect|Applies|
|---|---|---|
|`Advance`|`NextStep`, or completes the step's timeline first|game mode on top|
|`HoldSkip`|skip mode while held|game mode on top|
|`ToggleSkip`, `ToggleAuto`|switches skip or auto mode|game mode on top|
|`HideWindow`|hides the text window|game mode on top|
|`QuickSave`, `QuickLoad`|writes or loads `SaveSlot::Quick`|game mode on top|
|`OpenInGame(o)`|pushes the game overlay `o`|game mode on top|
|`Back`|`nav::back()`; ignored on a base|everywhere|
|`Open(o)`|pushes the overlay `o`|everywhere|

- **Focus keys are built in** and not bindable. Up/Down/Left/Right/Tab move focus; Enter and
  PadSouth activate the focused control.
- Bindings are a `Vec`, so a studio extends the defaults:

```kis
use std::system::{Action, Binding, Input, Key, bind, default_bindings};

const fn bindings() -> Vec<Binding> {
    let mut list = default_bindings();
    list.push(bind(Input::Key(Key::A), Action::ToggleAuto));
    list.push(bind(Input::Key(Key::L), Action::OpenInGame(Backlog)));
    list
}
```
