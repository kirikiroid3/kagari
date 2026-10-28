# System Recipes

Worked examples of a game system: the program and its title page, the game view, a settings page,
save and load pages, a CG gallery, a music room, replay unlocking and a backlog. They use
[`std::system`](../system.md), [`std::system::nav`](../system-nav.md),
[`std::system::saves`](../system-saves.md), [`std::ui`](../ui.md) and
[`std::ui::style`](../ui-style.md). UI forms need a `.kix` file.

The recipes assume a project laid out like this:

```text
src/
  main.kix      // main, settings, bindings
  story.kix     // the scenes
  ui.kix        // the modes
```

Set `entry = "src/main.kix"` in `kagari.toml`. Each file below is the concatenation of its
snippets in page order, so the page as a whole compiles. Asset paths name files under the
project's `assets/` directory.

## The program and a title page {#the-program}

`main` returns the [`System`](../system.md#system). The setting handles are immutable `static`s;
the engine reads the ones listed in `Settings`.

```kix
// main.kix
mod story;
mod ui;

use std::narrative::text::text_style;
use std::screen::size;
use std::system::{Action, Binding, Input, Key, bind, default_bindings};

pub static TEXT_SPEED: Setting<Duration> = setting(ms(30));
pub static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
pub static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
pub static SKIP_UNREAD: Setting<bool> = setting(false);
pub static MASTER: Setting<Volume> = setting(volume(100));
pub static MUSIC: Setting<Volume> = setting(volume(60));
pub static SOUND: Setting<Volume> = setting(volume(100));
pub static VOICE: Setting<Volume> = setting(volume(100));
pub static VOICE_SUSTAIN: Setting<bool> = setting(false);
pub static WAIT_FOR_VOICE: Setting<bool> = setting(true);
pub static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
pub static FULLSCREEN: Setting<bool> = setting(false);
pub static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

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
        bindings: bindings(),
    }
}

const fn bindings() -> Vec<Binding> {
    let mut list = default_bindings();
    list.push(bind(Input::Key(Key::A), Action::ToggleAuto));
    list.push(bind(Input::Key(Key::L), Action::OpenInGame(ui::Backlog)));
    list
}
```

The title page is a [`RootPage`](../system.md#rootpage). It starts the story with
[`nav::start`](../system-nav.md#start), opens overlays with `nav::call`, switches pages with
`nav::goto`, and plays the system music with [`set_music`](../system.md#set_music).

```kix
// ui.kix
use std::screen::{CENTER, rgba};
use std::system::set_music;
use std::ui::{Align, Fit};
use std::ui::style::{ButtonStyle, Look, Paint, button_style};
use std::narrative::text::text_style;
use crate::story;

const TITLE_BG: ImageAsset = asset("image/title.png");
const TITLE_THEME: AudioAsset = asset("music/title.ogg");

const MENU_BUTTON: ButtonStyle = button_style(Look { background: Paint::None, text: text_style(28, WHITE) })
    .hover(Look { background: Paint::Color(rgb(60, 60, 90)), text: text_style(28, WHITE) })
    .focus(Look { background: Paint::Color(rgb(60, 60, 90)), text: text_style(28, WHITE) })
    .change(tween(ms(150)));

pub fn Title() -> RootPage {
    set_music(Some(looped(TITLE_THEME)), crossfade(ms(500)));
    RootPage(layer![
        image!(fit = Fit::Cover, TITLE_BG).cover(),
        column![
            button!{style = MENU_BUTTON, on_click = || start_game(), t!["Start"]},
            button!{style = MENU_BUTTON, on_click = || nav::call(Load), t!["Load"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(Gallery), t!["Gallery"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(MusicRoom), t!["Music"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(Replays), t!["Replay"]},
            button!{style = MENU_BUTTON, on_click = || nav::call(Config), t!["Config"]},
            button!{style = MENU_BUTTON, on_click = || nav::dialog(ConfirmQuit), t!["Quit"]},
        ].gap(12).at(point(640, 470), CENTER),
    ])
}

fn start_game() {
    nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending)
}

fn ConfirmQuit() -> Dialog {
    Dialog(column![
        t!["Quit the game?"],
        row![
            button!{on_click = || nav::quit(), t!["Yes"]},
            button!{on_click = || nav::back(), t!["No"]},
        ].gap(24),
    ].align(Align::Center, Align::Center).background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

The `Title` component runs once each time the page is shown, so `set_music` restarts the title
theme whenever the player comes back to the title. `nav::quit` never asks; the `ConfirmQuit`
dialog does.

If the story returns an ending, handle it in the exit handler, and record it in a
[`Meta`](../narrative.md) value:

```kix
pub static CLEARED_AZUSA: Meta<bool> = meta(false);

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}

fn Credits() -> RootPage {
    RootPage(column![
        t!["Thank you for playing."],
        button!{on_click = || nav::goto(Title), t!["Back to title"]},
    ])
}
```

`story.kix` starts with the entry scene and its result; [Replay scenes](#replay) adds the rest.

```kix
// story.kix
pub enum Ending {
    AzusaEnd,
    NormalEnd,
}

#[scene]
#[entry]
pub pushdown fn Main(cleared: bool) -> Ending {
    s!(t!["Hello."]);
    FestivalNight();
    if cleared { Ending::AzusaEnd } else { Ending::NormalEnd }
}

const fn festival(screen: Screen) -> Screen {
    screen
}
```

## The game view {#the-game-view}

[`System::game`](../system.md#system) draws the game screen, the message window, the choice
buttons and a quick menu. Everything it reads from the `Narrative` handle is reactive.

```kix
use std::narrative::{MenuArm, PlayMode};
use std::narrative::text::TextStyle;
use std::screen::{BOTTOM_CENTER, insets};
use std::system::saves::SaveSlot;
use std::ui::{each, label, message, show};

const WINDOW_STYLE: TextStyle = text_style(26, WHITE);
const NAME_STYLE: TextStyle = text_style(28, hex(0xFFE0A0));

fn who_text(game: Narrative) -> Text {
    match game.line() {
        Some(line) => match line.who {
            Some(who) => who,
            None => "",
        },
        None => "",
    }
}

fn menu_arms(game: Narrative) -> Vec<MenuArm> {
    match game.menu() {
        Some(menu) => menu.arms,
        None => Vec::new(),
    }
}

fn arm_button(game: Narrative, arm: MenuArm) -> Element {
    button!{
        style = MENU_BUTTON,
        disabled = match arm.arm {
            Some(_) => false,
            None => true,
        },
        on_click = || match arm.arm {
            Some(a) => game.select(a),
            None => {}
        },
        label(arm.text),
    }.width(500)
}

fn toggle_mode(game: Narrative, mode: PlayMode) {
    if game.mode() == mode { game.set_mode(PlayMode::Normal) } else { game.set_mode(mode) }
}

pub fn GameView(game: Narrative) -> Element {
    let (hidden, set_hidden) = signal(false);
    layer![
        game_screen!(game = game).cover(),
        show(
            || !hidden(),
            || column![
                text!(style = NAME_STYLE, who_text(game)),
                message(game, WINDOW_STYLE).grow(1),
                row![
                    button!{on_click = || game.save(SaveSlot::Quick), t!["Q.Save"]},
                    button!{on_click = || { let _ = nav::load(SaveSlot::Quick); }, t!["Q.Load"]},
                    button!{on_click = || game.call(Save), t!["Save"]},
                    button!{on_click = || nav::call(Load), t!["Load"]},
                    button!{on_click = || toggle_mode(game, PlayMode::Auto), t!["Auto"]},
                    button!{on_click = || toggle_mode(game, PlayMode::Skip), t!["Skip"]},
                    button!{on_click = || game.call(Backlog), t!["Log"]},
                    button!{on_click = || nav::call(Config), t!["Config"]},
                    button!{on_click = || set_hidden(true), t!["Hide"]},
                ].gap(8),
            ]
                .width(1200)
                .height(200)
                .padding(insets(16, 40, 16, 40))
                .background(Paint::Color(rgba(0, 0, 0, 160))),
        ).at(point(640, 710), BOTTOM_CENTER),
        show(
            || has_menu(game),
            || each(|| menu_arms(game), |arm| arm_button(game, arm)).gap(24),
        ).at(point(640, 300), CENTER),
        show(|| hidden(), || button!{on_click = || set_hidden(false)}.width(1280).height(720)).cover(),
    ]
}

fn has_menu(game: Narrative) -> bool {
    match game.menu() {
        Some(_) => true,
        None => false,
    }
}
```

`each` keeps one button per arm; a disabled arm (`arm.arm` is `None`) is shown greyed out.

## A settings page {#settings-page}

A config page is an [`Overlay`](../system.md#overlay), so it can be opened from the title and
from the game. Settings are read with `get` and written with `set`; `get` is reactive, so every
control shows the current value without a signal.

```kix
use std::ui::option;
use crate::{AUTO_DELAY, FULLSCREEN, MUSIC, SKIP_UNREAD, SOUND, TEXT_SPEED, VOICE, WINDOW_OPACITY};

fn volume_row(name: Text, handle: Setting<Volume>) -> Element {
    row![
        label(name).width(200),
        slider!{
            value = handle.get().level,
            on_change = |v| handle.set(Volume { level: v, muted: handle.get().muted }),
        }.width(400),
        toggle!{
            value = handle.get().muted,
            on_change = |m| handle.set(Volume { level: handle.get().level, muted: m }),
            t!["Mute"],
        },
    ].gap(16).align(Align::Start, Align::Center)
}

fn speed_button(name: Text, handle: Setting<Duration>, value: Duration) -> Element {
    toggle!{value = handle.get().ms == value.ms, on_change = |_on| handle.set(value), label(name)}
}

fn Config() -> Overlay {
    Overlay(column![
        volume_row("Music", MUSIC),
        volume_row("Sound", SOUND),
        volume_row("Voice", VOICE),
        row![
            label("Text speed").width(200),
            speed_button("Slow", TEXT_SPEED, ms(60)),
            speed_button("Normal", TEXT_SPEED, ms(30)),
            speed_button("Fast", TEXT_SPEED, ms(10)),
            speed_button("Instant", TEXT_SPEED, ms(0)),
        ].gap(8),
        row![
            label("Auto delay").width(200),
            speed_button("Short", AUTO_DELAY, ms(800)),
            speed_button("Normal", AUTO_DELAY, ms(1300)),
            speed_button("Long", AUTO_DELAY, ms(3000)),
        ].gap(8),
        row![
            label("Window").width(200),
            select!{
                value = WINDOW_OPACITY.get(),
                on_change = |v| WINDOW_OPACITY.set(v),
                option(50, label("50%")),
                option(80, label("80%")),
                option(100, label("100%")),
            },
        ].gap(8),
        toggle!{value = SKIP_UNREAD.get(), on_change = |v| SKIP_UNREAD.set(v), t!["Skip unread text"]},
        toggle!{value = FULLSCREEN.get(), on_change = |v| FULLSCREEN.set(v), t!["Fullscreen"]},
        button!{on_click = || nav::back(), t!["Back"]},
    ].gap(16).padding(insets(40, 80, 40, 80)).background(Paint::Color(rgba(0, 0, 0, 220))))
}
```

A slider's range is the range of its value's `Int` type, so the volume sliders run from 0 to 100
without `min` or `max`.

> **Stability:** Planned — a text-reveal preview outside the game view (a sample line that shows
> the current text speed) is not available yet.

## Save and load pages {#save-and-load}

Saving needs the running game, so the save page is a [`GameOverlay`](../system.md#gameoverlay)
that receives the `Narrative` handle; the load page is an ordinary overlay, usable from the title.
[`saves::info`](../system-saves.md#info) is reactive, so a slot updates as soon as it is written
or deleted.

```kix
use std::system::saves;
use std::system::saves::SaveSlot;
use std::ui::{scroll, thumbnail};

fn slot_text(slot: SaveSlot) -> Text {
    match saves::info(slot) {
        Some(info) => match info.text {
            Some(text) => text,
            None => "",
        },
        None => "Empty",
    }
}

fn slots() -> Vec<u32> {
    let mut list = Vec::new();
    for k in 0..20 {
        list.push(k);
    }
    list
}

fn slot_view(slot: SaveSlot) -> Element {
    row![
        thumbnail(slot).width(160).height(90),
        text!(slot_text(slot)),
    ].gap(16)
}

fn Save(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| slots(), |k| row![
            button!{on_click = || game.save(SaveSlot::Manual(k)), slot_view(SaveSlot::Manual(k))},
            button!{on_click = || saves::delete(SaveSlot::Manual(k)), t!["Delete"]},
        ])).grow(1),
        button!{on_click = || nav::back(), t!["Back"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}

fn Load() -> Overlay {
    let (error, set_error) = signal("");
    Overlay(column![
        scroll(|| each(|| slots(), |k| button!{
            on_click = || match nav::load(SaveSlot::Manual(k)) {
                Ok(()) => {}
                Err(divergence) => set_error(divergence.describe()),
            },
            slot_view(SaveSlot::Manual(k)),
        })).grow(1),
        text!(error()),
        button!{on_click = || nav::back(), t!["Back"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

- An empty slot shows "Empty", and its thumbnail is empty.
- [`nav::load`](../system-nav.md#load) returns `Err` when the script changed so that the save no
  longer fits; the stack stays unchanged and the page shows `Divergence::describe()`.
- Autosaves are `SaveSlot::Auto(0)` … `Auto(auto_slots - 1)`; the engine writes them at every
  choice and on quit. A load page may list them the same way.

## A CG gallery {#gallery}

[`seen_image`](../system.md#seen_image) tells whether an image was ever on the screen, so a gallery
needs no bookkeeping in the story. The source of `image!` is reactive.

```kix
use std::system::seen_image;
use std::ui::Fit;

mod CG {
    pub const FIREWORKS: ImageAsset = asset("cg/fireworks.png");
    pub const ROOF: ImageAsset = asset("cg/roof.png");
    pub const STATION: ImageAsset = asset("cg/station.png");
}
const LOCKED_TILE: ImageAsset = asset("cg/locked.png");

fn cg_or_lock(cg: ImageAsset) -> ImageAsset {
    if seen_image(cg) { cg } else { LOCKED_TILE }
}

fn cg_tile(cg: ImageAsset, set_viewing: Fn(Option<ImageAsset>)) -> Element {
    button!{
        disabled = !seen_image(cg),
        on_click = || set_viewing(Some(cg)),
        image!(fit = Fit::Cover, cg_or_lock(cg)),
    }.width(320).height(180)
}

fn Gallery() -> RootPage {
    let (viewing, set_viewing) = signal(None);
    RootPage(layer![
        column![
            row![
                cg_tile(CG::FIREWORKS, set_viewing),
                cg_tile(CG::ROOF, set_viewing),
                cg_tile(CG::STATION, set_viewing),
            ].gap(24),
            button!{on_click = || nav::goto(Title), t!["Back"]},
        ].gap(24).at(point(640, 360), CENTER),
        show(
            || match viewing() {
                Some(_) => true,
                None => false,
            },
            || button!{
                on_click = || set_viewing(None),
                image!(fit = Fit::Contain, match viewing() {
                    Some(cg) => cg,
                    None => LOCKED_TILE,
                }),
            }.width(1280).height(720),
        ).cover(),
    ])
}
```

The gallery is a root page, reached with `nav::goto(Gallery)` and left with `nav::goto(Title)`.
`nav::back()` would be the error `E6002` here, because a root page is a base.

## A music room {#music-room}

[`heard`](../system.md#heard) tells whether a track ever played, and
[`set_music`](../system.md#set_music) plays a track on the system music channel, which is heard
while no game runs.

```kix
use std::system::{heard, set_music};

const THEME: AudioAsset = asset("music/theme.ogg");
const EVENING: AudioAsset = asset("music/evening.ogg");

fn track_button(title: Text, track: AudioAsset) -> Element {
    button!{
        disabled = !heard(track),
        on_click = || set_music(Some(looped(track)), crossfade(ms(500))),
        text!(if heard(track) { title } else { "???" }),
    }
}

fn MusicRoom() -> RootPage {
    RootPage(column![
        track_button("Theme", THEME),
        track_button("Evening", EVENING),
        button!{on_click = || set_music(None, crossfade(ms(500))), t!["Stop"]},
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ].gap(12))
}
```

> **Stability:** Planned — reading what is currently playing (to highlight the current track, or
> show a position or a pause state) is not designed yet.

## Replay scenes {#replay}

A replay is a further `#[entry]` scene, started with [`nav::start`](../system-nav.md#start). It is
unlocked by a `Meta<bool>` that the story writes with `write` when the player first reaches the
scene.

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

- A replay starts a fresh story: story variables have their initial values. State the replayed
  scene needs is passed as entry arguments.
- `Meta::get` is reactive, so the button enables itself as soon as the meta value changes.
- The story only writes meta (`write` is a `pushdown fn`); it never reads it. Meta reaches the
  story only through entry arguments.

## A backlog {#backlog}

A backlog is a game overlay that lists `Narrative::history()`. It is opened from the game view
with `game.call(Backlog)` or from a binding with `Action::OpenInGame(Backlog)`.

```kix
use std::narrative::{Entry, Line};
use std::ui::scroll;

fn line_text(line: Line) -> Text {
    match line.text {
        Some(text) => text,
        None => "",
    }
}

fn entry_view(entry: Entry) -> Element {
    match entry {
        Entry::Line(line) => text!(line_text(line)),
        Entry::Chosen(text) => label(text),
    }
}

pub fn Backlog(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| game.history(), |entry| entry_view(entry))).grow(1),
        button!{on_click = || nav::back(), t!["Close"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

The entries, voice replay (`Narrative::replay_voice`) and rewinding (`Narrative::rewind`) are
described in [`std::narrative`](../narrative.md).
