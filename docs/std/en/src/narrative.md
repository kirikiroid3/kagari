# `std::narrative`

> **Stability:** Stable — steps, random draws, meta fields, and the system's view of a running
> story.

`std::narrative` declares the step primitive `push_step` and the two standard steps `Say` and
`Choice` that the forms `s!(…)` and `choice!{…}` build, the story's random draw `rng_choose`,
meta fields (`Meta`), and `Narrative`, the handle through which the game system sees a running
game.

Its items have three kinds:

- `pushdown fn`: `push_step`, `rng_choose`, `Meta::write` — story code;
- `fn`: every `Narrative` method, `Meta::get`, `Meta::set` — system code;
- `const fn`: `meta` — the initializer of a meta field.

The module re-exports three items of [`std::narrative::text`](./narrative-text.md):

```kis
pub mod text;

pub use self::text::{Mark, Text, TextEffect};
```

How steps, scenes and the story fit together is described in
[Steps](../../../reference/en/src/concepts/steps.md) and
[Scenes and Instant Procedures](../../../reference/en/src/concepts/scenes.md).

## Overview

|Item|Kind|Summary|
|---|---|---|
|[`Step`](#trait-step)|builtin trait|A step the story suspends at|
|[`push_step`](#function-push_steps)|builtin `pushdown fn`|Runs a step|
|[`rng_choose`](#function-rng_choose)|builtin `pushdown fn`|Draws a random value|
|[`Say`](#struct-say)|struct|The step written `s!(…)`|
|[`Choice`](#struct-choicea-n)|struct|The step written `choice!{…}`|
|[`ChoiceArm`](#struct-choicearma)|struct|An arm of a `Choice`|
|[`Meta<T>`](#struct-metat)|builtin struct|A persistent global value outside saves|
|[`meta`](#function-metainit)|builtin `const fn`|Declares a meta field|
|[Timed input](#timed-input)|—|Planned|
|[`Narrative`](#struct-narrative)|builtin struct|The handle to the running game|
|[`Line`](#struct-line), [`Arm`](#struct-arm), [`MenuArm`](#struct-menuarm), [`Menu`](#struct-menu), [`HudState`](#struct-hudstate), [`Entry`](#enum-entry), [`PlayMode`](#enum-playmode)|records|What `Narrative` returns|

## Trait: `Step`

```kis
#[builtin] pub trait Step {
    type Output;
}
```

A step the story suspends at. `Output` is the type the story resumes with. `Say` and `Choice` are
the only steps; games cannot implement `Step`.

- Kind: — · Builtin: yes

## Function: `push_step(s)`

```kis
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

- `s` `S` — the step to show.
- Returns: `S::Output` — what the player's input carried: `()` for a `Say`, the arm's value for a
  `Choice`.
- Kind: `pushdown fn` · Builtin: yes

`push_step` shows the step, waits for input, and returns what the input carried. Only a scene may
call it; a step outside a scene is
[`E3002`](../../../reference/en/src/diagnostics/kinds.md#e3002). You normally write it through the
forms `s!(…)` and `choice!{…}`.

## Function: `rng_choose()`

```kis
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

- `T` — a fieldless enum or an `Int<lo, hi>`.
- Returns: `T` — a value drawn by the story's PRNG (xoroshiro128++), each value with equal
  probability.
- Kind: `pushdown fn` · Builtin: yes

`rng_choose::<T>()` draws a variant of a fieldless enum, or an integer of an `Int<lo, hi>`. Only a
scene may call it ([`E3024`](../../../reference/en/src/diagnostics/kinds.md#e3024)).

- The PRNG state is part of the story state and its seed is in the save, so loading a save
  reproduces every draw.
- The story analysis treats each draw as a free choice over every value of `T`: every outcome
  must be handled.

```kix
enum Coin { Heads, Tails }

#[scene]
pushdown fn Toss() {
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t!["Heads."]),
        Coin::Tails => s!(t!["Tails."]),
    }
}
```

A draw does not suspend for the player: a loop whose only suspension is `rng_choose` never passes
a step ([`E5001`](../../../reference/en/src/diagnostics/analysis.md#e5001)).

## Struct: `Say`

```kis
pub struct Say {
    pub who: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub render: Option<Render>,
    pub text: Option<Text>,
    pub history: Option<Text>,
}

impl Step for Say {
    type Output = ();
}
```

- `who` `Option<Text>` — the speaker name; `None` makes the message narration.
- `voice` `Option<VoiceAsset>` — a voice that starts with the text and is bound to the line; see
  [`std::screen::audio`](./screen-audio.md).
- `render` `Option<Render>` — a render that runs at step entry; see [`std::screen`](./screen.md).
- `text` `Option<Text>` — the message.
- `history` `Option<Text>` — the text the backlog records instead of `text`; `None` records `text`
  with its interpolated values as they were shown.
- Kind: — · Builtin: no

`s!(who = …, voice = …, render = …, history = …, t![…])` is `push_step(Say { … })`, with `None`
for each absent attribute. A `Say` waits for `NextStep` (a click or key), which carries no
information and is not recorded in saves.

```kix
use std::narrative::text::speed;

const MIO_0012: VoiceAsset = asset("voice/mio_0012.ogg");

#[scene]
pushdown fn Morning() {
    s!(who = "Mio", voice = MIO_0012, t!["Good morning."]);
    s!(history = t!["(The bell rings.)"], t![!["Ding… dong…"](speed(ms(120)))]);
}
```

## Struct: `Choice<A, N>`

```kis
pub struct Choice<A, const N: usize> {
    pub id: Text,
    pub render: Option<Render>,
    pub arms: [ChoiceArm<A>; N],
}

impl<A, const N: usize> Step for Choice<A, N> {
    type Output = A;
}
```

- `id` `Text` — the choice's stable id; saves record the selection by it.
- `render` `Option<Render>` — a render that runs at step entry.
- `arms` `[ChoiceArm<A>; N]` — the arms, in display order.
- Kind: — · Builtin: no

`choice!{ id = …, render = …, t![…] if guard => V, … }` is `push_step(Choice { … })`. Each arm
becomes a `ChoiceArm` whose `shown` is its guard, or `true` without one. The step returns the
`value` of the selected arm.

- `id` must be a string literal, unique within its function
  ([`E3019`](../../../reference/en/src/diagnostics/kinds.md#e3019)).
- `arms` must be an array literal of `ChoiceArm { … }` records; each value must be a fieldless
  enum variant, and each variant may appear at most once
  ([`E3020`](../../../reference/en/src/diagnostics/kinds.md#e3020)).
- A choice whose arms can all be hidden at once is a story-analysis error
  ([`E5003`](../../../reference/en/src/diagnostics/analysis.md#e5003)).

### Disabled arms

A disabled arm is shown greyed out and cannot be selected. It is display-only: it has no value and
is not part of `choice!`, so no story analysis sees it. The choice's render adds it through the
step's choice menu (`ChoiceMenu::disabled_arm`, see [`std::screen`](./screen.md)), and the story
picks the render that adds it:

```kix
let place = choice!{
    id = "map",
    render = if VISITED_A && VISITED_B { map_open } else { map_leave_locked },
    t!["Stall A"] => Map::A,
    t!["Stall B"] => Map::B,
    t!["Leave"] if VISITED_A && VISITED_B => Map::Leave,
};

const fn map_open(screen: Screen) -> Screen {
    screen
}

const fn map_leave_locked(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(2, t!["Leave"]);
    screen
}
```

`disabled_arm(at, text)` inserts the arm at position `at` of the shown list, clamped to its
length.

## Struct: `ChoiceArm<A>`

```kis
pub struct ChoiceArm<A> {
    pub text: Text,
    pub shown: bool,
    pub value: A,
}
```

- `text` `Text` — the label of the arm.
- `shown` `bool` — whether the arm is shown: the arm's guard, or `true`.
- `value` `A` — what the step returns when the arm is selected.
- Kind: — · Builtin: no

## Struct: `Meta<T>`

```kis
#[builtin] pub struct Meta<T>;
```

A persistent global value outside saves: "cleared route A", "watched the true ending". An
immutable `static` holding `meta(init)` declares a meta field.

- Kind: — · Builtin: yes

```kis
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- Writes are never reverted by load or rewind.
- The story can write a meta field but never read it back. Meta reaches the story only through the
  arguments of an entry scene.
- `const` code never sees meta
  ([`E3013`](../../../reference/en/src/diagnostics/kinds.md#e3013)).
- A `Meta` handle can only be the initializer of an immutable `static`
  ([`E3011`](../../../reference/en/src/diagnostics/kinds.md#e3011),
  [`E4007`](../../../reference/en/src/diagnostics/eval.md#e4007)).

|Method|Kind|Use|
|---|---|---|
|`get()`|`fn`|system code reads it; reactive|
|`set(v)`|`fn`|system code writes it|
|`write(v)`|`pushdown fn`|the story writes it|

### `Meta::get(&self)`

```kis
#[builtin] pub fn get(&self) -> T;
```

- Returns: `T` — the current value. Reactive: UI that reads it updates when it changes.
- Kind: `fn` · Builtin: yes

### `Meta::set(&self, value)`

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — the new value.
- Kind: `fn` · Builtin: yes

Sets the value from system code.

### `Meta::write(&self, value)`

```kis
#[builtin] pub pushdown fn write(&self, value: T);
```

- `value` `T` — the new value.
- Kind: `pushdown fn` · Builtin: yes

Sets the value from the story.

```kis
#[scene]
pushdown fn AzusaEnding() {
    CLEARED_AZUSA.write(true);
}
```

## Function: `meta(init)`

```kis
#[builtin] pub const fn meta<T>(init: T) -> Meta<T>;
```

- `init` `T` — the value used when the meta file has none.
- Returns: `Meta<T>` — a meta field.
- Kind: `const fn` · Builtin: yes

## Timed input

> **Stability:** Planned.

Timed input and auto play driven by the story are not supported yet. The planned API is a one-shot
timer in `fn` code that calls a closure once after a `Duration`, in the spirit of JavaScript's
`setTimeout`. There will be no repeating timer (no `setInterval` equivalent).

## Struct: `Narrative`

```kis
#[builtin] pub struct Narrative;
```

A handle to the running game, given to the game view and to game overlays. Reads are reactive.

- Kind: — · Builtin: yes

The system sees a running game only through this handle. Its representation is hidden, so system
code cannot read story variables. A handle exists only in the game view
(`System::game: fn(Narrative) -> Element`) and in game overlays (`fn(Narrative) -> GameOverlay`),
so every operation that needs a running game is typed to need one. Every method is `fn`.

|Method|Effect|
|---|---|
|`line()`|the current line|
|`menu()`|the current choice|
|`hud()`|the Hud of the current step|
|`history()`|the backlog, oldest first|
|`mode()`, `set_mode(m)`|the play mode|
|`advance()`|sends `NextStep` to the story|
|`select(arm)`|selects a shown arm of the current choice|
|`save(slot)`|writes the running game to `slot`|
|`rewind(i)`|restores the story to entry `i` of `history()`|
|`call(o)`|opens the game overlay `o`|
|`replay_voice(v)`|plays `v` again on the voice bus|

> **Stability:** Planned — NVL mode. A full-screen text mode in which several lines stay on the
> screen until a page break is not designed yet. It will change what the text window shows and how
> the backlog and `Narrative::line` describe a step.

### `Narrative::line(&self)`

```kis
#[builtin] pub fn line(&self) -> Option<Line>;
```

- Returns: `Option<Line>` — the line shown; `None` at a choice or before the first step.
- Kind: `fn` · Builtin: yes

### `Narrative::menu(&self)`

```kis
#[builtin] pub fn menu(&self) -> Option<Menu>;
```

- Returns: `Option<Menu>` — the choice shown, with its disabled arms; `None` outside a choice.
- Kind: `fn` · Builtin: yes

### `Narrative::hud(&self)`

```kis
#[builtin] pub fn hud(&self) -> HudState;
```

- Returns: `HudState` — the Hud of the current step, as the game view draws it.
- Kind: `fn` · Builtin: yes

### `Narrative::history(&self)`

```kis
#[builtin] pub fn history(&self) -> Vec<Entry>;
```

- Returns: `Vec<Entry>` — the backlog, oldest first.
- Kind: `fn` · Builtin: yes

The backlog is bounded by `System::backlog` and drops the oldest entries. An `Entry::Line` holds
the text as it was shown, or the step's `history` text when it has one. `Entry::Chosen` holds the
text of a selected arm.

### `Narrative::mode(&self)`

```kis
#[builtin] pub fn mode(&self) -> PlayMode;
```

- Returns: `PlayMode` — the play mode.
- Kind: `fn` · Builtin: yes

### `Narrative::set_mode(&self, mode)`

```kis
#[builtin] pub fn set_mode(&self, mode: PlayMode);
```

- `mode` `PlayMode` — the new play mode.
- Kind: `fn` · Builtin: yes

### `Narrative::advance(&self)`

```kis
#[builtin] pub fn advance(&self);
```

- Kind: `fn` · Builtin: yes

Advances to the next step: sends `NextStep` to the story, as a click on the text does.

### `Narrative::select(&self, arm)`

```kis
#[builtin] pub fn select(&self, arm: Arm);
```

- `arm` `Arm` — a shown arm of the current choice, from `menu()`.
- Kind: `fn` · Builtin: yes

### `Narrative::save(&self, slot)`

```kis
#[builtin] pub fn save(&self, slot: SaveSlot);
```

- `slot` `SaveSlot` — the slot to write; see [`std::system::saves`](./system-saves.md).
- Kind: `fn` · Builtin: yes

Saves the running game into `slot`.

### `Narrative::rewind(&self, entry)`

```kis
#[builtin] pub fn rewind(&self, entry: u32);
```

- `entry` `u32` — an index into `history()`.
- Kind: `fn` · Builtin: yes

Returns the story to backlog entry `entry`.

### `Narrative::call(&self, overlay)`

```kis
#[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
```

- `overlay` `fn(Narrative) -> GameOverlay` — the overlay to open; it receives this handle.
- Kind: `fn` · Builtin: yes

Opens a game overlay (pushes it on the mode stack); see [`std::system`](./system.md).

### `Narrative::replay_voice(&self, voice)`

```kis
#[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
```

- `voice` `VoiceAsset` — the voice to play.
- Kind: `fn` · Builtin: yes

Plays `voice` again on the voice bus, typically from the backlog.

## Struct: `Line`

```kis
pub struct Line {
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub read: bool,
}
```

A line of a `Say` step.

- `who`, `text`, `voice` — as in the `Say` step (`text` is the `history` text in the backlog when
  the step sets one).
- `read` `bool` — whether the player has read this line before, from the engine's read flags.
- Kind: — · Builtin: no

## Struct: `Arm`

```kis
#[builtin] pub struct Arm;
```

A selectable arm of the current choice: an opaque handle for `Narrative::select`.

- Kind: — · Builtin: yes

## Struct: `MenuArm`

```kis
pub struct MenuArm {
    pub text: Text,
    pub arm: Option<Arm>,
}
```

An arm of a shown choice.

- `text` `Text` — the label.
- `arm` `Option<Arm>` — the handle to select it; `None` for a disabled arm from the step's choice
  menu.
- Kind: — · Builtin: no

## Struct: `Menu`

```kis
pub struct Menu {
    pub arms: Vec<MenuArm>,
    pub align: VAlign,
}
```

A shown choice.

- `arms` `Vec<MenuArm>` — the shown arms in order, with the disabled arms inserted at their `at`
  positions.
- `align` `VAlign` — where the menu sits, as set by the render (`ChoiceMenu::set_align`).
- Kind: — · Builtin: no

## Struct: `HudState`

```kis
pub struct HudState {
    pub text_window: Visibility,
    pub quick_menu: Visibility,
    pub side_image: Option<ImageAsset>,
}
```

The Hud as the game view draws it.

- `text_window` `Visibility` — whether the text window is shown.
- `quick_menu` `Visibility` — whether the quick menu is shown.
- `side_image` `Option<ImageAsset>` — the side image, if any.
- Kind: — · Builtin: no

## Enum: `Entry`

```kis
pub enum Entry {
    Line(Line),
    Chosen(Text),
}
```

A backlog entry: a line, or the text of a selected arm.

- Kind: — · Builtin: no

## Enum: `PlayMode`

```kis
pub enum PlayMode {
    Normal,
    Auto,
    Skip,
}
```

How the game advances.

- `Normal` — on the player's input.
- `Auto` — after the `auto_delay` setting (plus `auto_delay_per_char` per character), waiting for
  the voice if `wait_for_voice` is on.
- `Skip` — as fast as possible. It stops at unread text unless the `skip_unread` setting is on, and
  always stops at choices.
- Kind: — · Builtin: no

## Example: a backlog

```kix
use std::narrative::Entry;
use std::ui::{each, label, scroll, spacer};

fn Backlog(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| game.history(), |entry| match entry {
            Entry::Line(line) => match line.text {
                Some(text) => label(text),
                None => spacer(),
            },
            Entry::Chosen(text) => label(text),
        })),
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

It is opened from the game view or a binding with `game.call(Backlog)`.
