# Narrative

`std::narrative` declares the step primitive, the two standard steps, meta fields, and
`Narrative`, the system's handle to a running game.

```kis
pub mod text;
pub use self::text::{Mark, Text, TextEffect};
```

## `push_step`

```kis
#[builtin] pub trait Step { type Output; }
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`push_step` shows the step, waits for input, and returns what the input carried. It is one of the
three `pushdown fn` of the library, with `rng_choose` and `Meta::write`, and only a `#[scene]` may
call it.

## `rng_choose`

```kis
/// Types a story can draw at random: fieldless enums and `Int<lo, hi>`.
#[builtin] pub trait RandChoose {}
/// A value of `T` drawn by the story's PRNG (xoroshiro128++); only scenes can draw.
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

`rng_choose::<T>()` draws a value of `T`: a variant of a fieldless enum, or an integer of an
`Int<lo, hi>`, each with equal probability. Only a scene may call it (`E3024`). The PRNG state is
part of the story state and its seed is in the save, so replay reproduces every draw; the analyzer
treats each draw as a free choice over every value of `T`.

## `Say`

```kis
pub struct Say {
    pub who: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub render: Option<Render>,
    pub text: Option<Text>,
    /// The text the backlog records instead of `text`; `None` records `text` with its
    /// interpolated values as they were shown.
    pub history: Option<Text>,
}
impl Step for Say { type Output = (); }
```

`s!(who = …, voice = …, render = …, history = …, t![…])` is `push_step(Say { … })`, with `None` for each absent
attribute.

- `who` is the speaker name; without it the message is narration.
- `voice` starts with the text and is bound to the line; see [Audio](./audio.md#voice).
- `render` runs at step entry; see [Screen](./screen.md#step-entry).
- A `Say` waits for `NextStep`, which carries no information and is not recorded in saves.
- `history` is the text the backlog records. By default the backlog records `text` with its
  interpolated values as they were when the line was shown; when `history` is set,
  `Entry::Line(line).text` is the `history` text instead.

## `Choice`

```kis
pub struct Choice<A, const N: usize> {
    pub id: Text,
    pub render: Option<Render>,
    pub arms: [ChoiceArm<A>; N],
}
pub struct ChoiceArm<A> {
    pub text: Text,
    pub shown: bool,
    pub value: A,
}
impl<A, const N: usize> Step for Choice<A, N> { type Output = A; }
```

`choice!{ id = …, render = …, t![…] if guard => V, … }` is `push_step(Choice { … })`. Each arm
becomes a `ChoiceArm` whose `shown` is its guard, or `true` without one. The step returns the
`value` of the selected arm. The selection is recorded in saves by the choice's stable id.

### Disabled arms

A disabled arm is shown greyed out and cannot be selected. It is display-only: it has no value and
is not part of `choice!`, so no narrative analysis sees it. It is added by the choice's render
through the step's `ChoiceMenu` (see [Screen](./screen.md#choicemenu)), and the story picks the
render that adds it:

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

`disabled_arm(at, text)` inserts the arm at position `at` of the shown list, clamped to its length.

## `Meta`

```kis
#[builtin] pub struct Meta<T>;
#[builtin] pub const fn meta<T>(init: T) -> Meta<T>;
impl<T> Meta<T> {
    #[builtin] pub fn get(&self) -> T;
    #[builtin] pub fn set(&self, value: T);
    #[builtin] pub pushdown fn write(&self, value: T);
}
```

An immutable `static` holding `meta(init)` declares a meta field: persistent, global, outside
saves. `init` is the value used when the meta file has none.

```kis
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

|Method|Kind|Use|
|---|---|---|
|`get()`|`fn`|system code reads it; reactive|
|`set(v)`|`fn`|system code writes it|
|`write(v)`|`pushdown fn`|the story writes it; the story can never read it back|

Writes are never reverted by load or rewind. Meta reaches the story only through the arguments of
an entry scene.

## Timed input (planned)

Timed input and auto play driven by the story are not supported yet. The planned API is a one-shot
timer in `fn` code that calls a closure once after a `Duration`, in the spirit of JavaScript's
`setTimeout`. There will be no repeating timer (no `setInterval` equivalent).

## The `Narrative` handle

```kis
/// A handle to the running game, given to the game view and to game overlays. Reads are reactive.
#[builtin] pub struct Narrative;
impl Narrative {
    #[builtin] pub fn line(&self) -> Option<Line>;
    #[builtin] pub fn menu(&self) -> Option<Menu>;
    #[builtin] pub fn hud(&self) -> HudState;
    #[builtin] pub fn history(&self) -> Vec<Entry>;
    #[builtin] pub fn mode(&self) -> PlayMode;
    #[builtin] pub fn set_mode(&self, mode: PlayMode);
    #[builtin] pub fn advance(&self);
    #[builtin] pub fn select(&self, arm: Arm);
    #[builtin] pub fn save(&self, slot: SaveSlot);
    #[builtin] pub fn rewind(&self, entry: u32);
    #[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
    #[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
}
```

The system sees a running game only through this handle. Its representation is hidden, so system
code cannot read story variables. A handle exists only in the game view
(`System::game: fn(Narrative) -> Element`) and in game overlays (`fn(Narrative) -> GameOverlay`),
so every operation that needs a running game is typed to need one. Every method is `fn`.

|Method|Effect|
|---|---|
|`line()`|the current line; `None` at a choice, or before the first step|
|`menu()`|the current choice; `Some` only at a choice|
|`hud()`|the Hud of the current step, as the game view draws it|
|`history()`|the backlog, oldest first|
|`mode()`, `set_mode(m)`|the play mode|
|`advance()`|sends `NextStep` to the story|
|`select(arm)`|selects a shown arm of the current choice|
|`save(slot)`|writes the running game to `slot`; see [Saves](./system.md#saves)|
|`rewind(i)`|restores the story to entry `i` of `history()`|
|`call(o)`|pushes the game overlay `o`|
|`replay_voice(v)`|plays `v` again on the voice bus, for the backlog|

### Records

```kis
pub struct Line { pub who: Option<Text>, pub text: Option<Text>, pub voice: Option<VoiceAsset>, pub read: bool }
#[builtin] pub struct Arm;
pub struct MenuArm { pub text: Text, pub arm: Option<Arm> }   // None: a ChoiceMenu disabled arm
pub struct Menu { pub arms: Vec<MenuArm>, pub align: VAlign }
pub struct HudState { pub text_window: Visibility, pub quick_menu: Visibility, pub side_image: Option<ImageAsset> }
pub enum Entry { Line(Line), Chosen(Text) }
pub enum PlayMode { Normal, Auto, Skip }
```

- **`Line::read`** is whether the player has read the line before, from the engine's read flags.
- **`menu()`.** Its arms are the shown arms in order, with the disabled arms inserted at their
  `at` positions. A disabled arm has `arm: None`; `Arm` is an opaque handle for `select`.
- **`history()`** is bounded by `System::backlog` and drops the oldest entries. `Entry::Chosen`
  holds the text of a selected arm. An `Entry::Line` holds the text as it was shown, or the step's
  `history` text when it has one.
- **`PlayMode::Skip`** stops at unread text unless the `skip_unread` setting is on. It always stops
  at choices.
- **`PlayMode::Auto`** advances after the `auto_delay` settings, and waits for the voice if
  `wait_for_voice` is on.

### Example: a backlog

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
