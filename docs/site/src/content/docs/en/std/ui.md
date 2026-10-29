---
title: "std::ui"
---

> **Stability:** Stable — UI values, reactivity, and the functions the UI forms desugar to.

`std::ui` is the UI toolkit of the game system: the element type, reactive signals, content and
layout functions, and the targets of the UI forms (`column![…]`, `button!{…}`, `text!(…)` and so
on). [`std::ui::style`](./ui-style.md) declares the looks of controls.

UI is `fn` code. A **component** is a `fn` that returns an [`Element`](#element), wrapped in its
mode type ([`RootPage`, `Overlay`, `GameOverlay`, `Dialog`](./system.md#mode-types)), or a plain
`Element` for the game view. Styles are `const fn` data.

The prelude imports `signal` and `Element`. Import other items by path, for example
`use std::ui::{Fit, each, show, thumbnail};`.

```kis
pub mod style;
```

## Overview {#overview}

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Element`](#element), [`Placed`](#placed) | builtin structs | A UI element; an element placed in a layer |
| [`Align`](#align), [`Fit`](#fit) | enums | Flex alignment; how an image fills its box |
| [`signal`](#signal), [`effect`](#effect), [`each`](#each), [`show`](#show) | `fn` | Reactivity |
| [`label`](#label), [`spacer`](#spacer), [`scroll`](#scroll), [`thumbnail`](#thumbnail), [`message`](#message) | `fn` | Content |
| [`Element` methods](#layout) | `fn` | Size, spacing, alignment, paint, focus order, placement |
| [`column`](#column), [`row`](#row), [`layer`](#layer) | `fn` | Targets of the layout forms |
| [`text`](#text), [`image`](#image), [`game_screen`](#game_screen) | `fn` | Targets of the content forms |
| [`button`](#button), [`slider`](#slider), [`toggle`](#toggle), [`select`](#select), [`option`](#option) | `fn` | Targets of the control forms |

## Elements {#elements}

### Struct: `Element` {#element}

```kis
#[builtin] pub struct Element;
```

A UI element tree, laid out by the engine. An `Element` is an opaque value: build one with the
functions and forms of this module, then refine it with the [layout methods](#layout).

### Struct: `Placed` {#placed}

```kis
#[builtin] pub struct Placed;
```

An element placed in a [`layer![…]`](#layer). Make one with [`Element::at`](#element-at) or
[`Element::cover`](#element-cover).

### Enum: `Align` {#align}

```kis
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}
```

Flex alignment of the children of a row or column, used by [`Element::align`](#element-align).

| Variant | Along the row or column (`main`) | Across it (`cross`) |
| ------- | -------------------------------- | ------------------- |
| `Start` | Packed at the start | At the start |
| `Center` | Centred | Centred |
| `End` | Packed at the end | At the end |
| `Stretch` | Same as `Start` | Children without a fixed size are stretched to the full cross size |
| `SpaceBetween` | The free space is shared equally between the children (with one child, same as `Start`) | Same as `Start` |

### Enum: `Fit` {#fit}

```kis
pub enum Fit {
    Stretch,
    Contain,
    Cover,
    Natural,
}
```

How an image fills its box, used by [`image!`](#image):

| Variant | Meaning |
| ------- | ------- |
| `Stretch` | Fills the box, ignoring the aspect ratio |
| `Contain` | The largest size that fits inside the box, keeping the aspect ratio, centred in the box |
| `Cover` | Fills the box, keeping the aspect ratio; the image is cropped equally on both sides of the overflowing axis |
| `Natural` | The image's own pixel size, at the top-left corner of the box |

## Components and reactivity {#components-and-reactivity}

Components and reactivity follow [SolidJS](https://www.solidjs.com/):

- A component runs **once**, when its mode is shown.
- **Reactive sources** are signal getters, `Setting::get`, `Meta::get`, the reads of the
  `Narrative` handle, [`saves::info`](./system-saves.md#info) and the
  [seen sets](./system.md#seen-sets).
- A **reactive closure** re-runs when a source it read changes. The reactive attributes and the
  elements of the UI forms are such closures (see [UI forms](#ui-forms)), so
  `text!(t!["Music: " {music().level}])` updates whenever `music` changes, while the rest of the
  component does not run again.
- Handlers such as `on_click` are ordinary closures; they run when the event happens.

Signals exist only in `fn` code: the story and `const fn` cannot use them.

### Function: `signal(init)` {#signal}

```kis
#[builtin] pub fn signal<T>(init: T) -> (Fn() -> T, Fn(T));
```

- `init` `T` — the initial value.
- Returns: `(Fn() -> T, Fn(T))` — a getter and a setter. The getter is a reactive source.
- Kind: `fn` · Builtin: yes

```kix
let (hidden, set_hidden) = signal(false);
```

### Function: `effect(run)` {#effect}

```kis
#[builtin] pub fn effect(run: Fn());
```

- `run` `Fn()` — the closure to run.
- Kind: `fn` · Builtin: yes

Runs `run` now, and again whenever a source it read changes.

### Function: `each(items, item)` {#each}

```kis
#[builtin] pub fn each<T>(items: Fn() -> Vec<T>, item: Fn(T) -> Element) -> Element;
```

- `items` `Fn() -> Vec<T>` — the items; reactive.
- `item` `Fn(T) -> Element` — builds the element of one item.
- Returns: `Element` — one element per item, kept in sync with `items`.
- Kind: `fn` · Builtin: yes

```kix
scroll(|| each(|| game.history(), |entry| entry_view(entry)))
```

### Function: `show(when, then)` {#show}

```kis
#[builtin] pub fn show(when: Fn() -> bool, then: Fn() -> Element) -> Element;
```

- `when` `Fn() -> bool` — the condition; reactive.
- `then` `Fn() -> Element` — builds the content.
- Returns: `Element` — `then()` while `when()` is `true`, else nothing.
- Kind: `fn` · Builtin: yes

## Content {#content}

### Function: `label(text)` {#label}

```kis
#[builtin] pub fn label(text: Text) -> Element;
```

- `text` `Text` — the text.
- Returns: `Element` — static text.
- Kind: `fn` · Builtin: yes

The text uses [`System::text`](./system.md#system), or the current look's text style inside a
button or toggle (see [Interaction looks](./ui-style.md#interaction-looks)). `text!(…)` is the
reactive form with an optional style. Inside `column!`, `row!`, `button!` and `toggle!`, a bare
`t![…]` element becomes `label(t![…])`.

### Function: `spacer()` {#spacer}

```kis
#[builtin] pub fn spacer() -> Element;
```

- Returns: `Element` — an empty element that grows, pushing its siblings apart.
- Kind: `fn` · Builtin: yes

### Function: `scroll(child)` {#scroll}

```kis
#[builtin] pub fn scroll(child: Fn() -> Element) -> Element;
```

- `child` `Fn() -> Element` — the content.
- Returns: `Element` — a scrollable box around the content.
- Kind: `fn` · Builtin: yes

### Function: `thumbnail(slot)` {#thumbnail}

```kis
#[builtin] pub fn thumbnail(slot: SaveSlot) -> Element;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — the slot.
- Returns: `Element` — the screenshot stored with the save in `slot`.
- Kind: `fn` · Builtin: yes

### Function: `message(game, style)` {#message}

```kis
#[builtin] pub fn message(game: Narrative, style: TextStyle) -> Element;
```

- `game` `Narrative` — the running game.
- `style` `TextStyle` — the text style of the window (see
  [`std::narrative::text`](./narrative-text.md)).
- Returns: `Element` — the current line with its reveal, text effects, pauses and marks, and the
  click-wait glyph.
- Kind: `fn` · Builtin: yes

`message` is the body of the message window. It only exists where a `Narrative` handle exists:
in the game view and in game overlays.

> **Stability:** Planned — NVL mode, a full-screen text mode where several lines stay on the
> screen until a page break, is not designed yet. It will change what the text window shows and
> how the backlog and `Narrative::line` describe a step.

## Layout {#layout}

```kis
impl Element {
    #[builtin] pub fn width(self, px: u32) -> Element;
    #[builtin] pub fn height(self, px: u32) -> Element;
    #[builtin] pub fn padding(self, insets: Insets) -> Element;
    #[builtin] pub fn gap(self, px: u32) -> Element;
    #[builtin] pub fn align(self, main: Align, cross: Align) -> Element;
    #[builtin] pub fn grow(self, factor: u32) -> Element;
    #[builtin] pub fn background(self, paint: Paint) -> Element;
    #[builtin] pub fn opacity(self, opacity: Decimal) -> Element;
    #[builtin] pub fn focus_order(self, order: u32) -> Element;
    #[builtin] pub fn at(self, position: Point, anchor: Anchor) -> Placed;
    #[builtin] pub fn cover(self) -> Placed;
}
```

Layout is flexbox-like:

- [`column![…]`](#column) and [`row![…]`](#row) are flex containers; `gap` and `align` set the
  spacing and alignment of their children, and `grow` is a child's flex grow factor.
- [`layer![…]`](#layer) stacks [`Placed`](#placed) children: `at` places a child's anchor at a
  point, and `cover` fills the layer.
- Sizes and positions are pixels of the design resolution
  ([`System::resolution`](./system.md#system)).
- Layout is an engine pass over the element tree. It is recomputed on resize and is never saved.

All methods take the element by value and return the refined element, so they chain:
`column![…].gap(16).align(Align::Center, Align::Center)`. Each is a builtin `fn`.

### `Element::width(px)` {#element-width}

```kis
#[builtin] pub fn width(self, px: u32) -> Element;
```

- `px` `u32` — a fixed width in pixels.

### `Element::height(px)` {#element-height}

```kis
#[builtin] pub fn height(self, px: u32) -> Element;
```

- `px` `u32` — a fixed height in pixels.

### `Element::padding(insets)` {#element-padding}

```kis
#[builtin] pub fn padding(self, insets: Insets) -> Element;
```

- `insets` `Insets` — the inner spacing; build it with `std::screen::insets(top, right, bottom,
  left)` (see [`std::screen`](./screen.md)).

### `Element::gap(px)` {#element-gap}

```kis
#[builtin] pub fn gap(self, px: u32) -> Element;
```

- `px` `u32` — the spacing between the children of a row or column.

### `Element::align(main, cross)` {#element-align}

```kis
#[builtin] pub fn align(self, main: Align, cross: Align) -> Element;
```

- `main` `Align` — the alignment of the children along the row or column.
- `cross` `Align` — the alignment across it.

### `Element::grow(factor)` {#element-grow}

```kis
#[builtin] pub fn grow(self, factor: u32) -> Element;
```

- `factor` `u32` — the flex grow factor: how much of the free space of its row or column the
  element takes.

### `Element::background(paint)` {#element-background}

```kis
#[builtin] pub fn background(self, paint: Paint) -> Element;
```

- `paint` [`Paint`](./ui-style.md#paint) — what is drawn behind the element.

### `Element::opacity(opacity)` {#element-opacity}

```kis
#[builtin] pub fn opacity(self, opacity: Decimal) -> Element;
```

- `opacity` `Decimal` — the element's opacity, `0.0..=1.0`.

### `Element::focus_order(order)` {#element-focus_order}

```kis
#[builtin] pub fn focus_order(self, order: u32) -> Element;
```

- `order` `u32` — the element's position in the focus order (see [Focus](#focus)).

### `Element::at(position, anchor)` {#element-at}

```kis
#[builtin] pub fn at(self, position: Point, anchor: Anchor) -> Placed;
```

- `position` `Point` — a point of the layer.
- `anchor` `Anchor` — the point of the element placed there, such as `TOP_LEFT` or `CENTER`
  (see [`std::screen`](./screen.md)).
- Returns: `Placed` — for a `layer![…]`.

### `Element::cover()` {#element-cover}

```kis
#[builtin] pub fn cover(self) -> Placed;
```

- Returns: `Placed` — the element filling the whole layer.

```kix
use std::screen::CENTER;
use std::ui::Fit;

fn Title() -> RootPage {
    RootPage(layer![
        image!(fit = Fit::Cover, TITLE_BG).cover(),
        column![
            button!{on_click = || nav::start(story::Main, (), |_end| nav::goto(Title)), t!["Start"]},
            button!{on_click = || nav::call(Config), t!["Config"]},
            button!{on_click = || nav::quit(), t!["Quit"]},
        ].gap(16).at(point(640, 520), CENTER),
    ])
}
```

## Focus {#focus}

- The focusable elements are buttons, sliders, toggles and selects.
- Focus follows document order unless [`focus_order`](#element-focus_order) is set.
- The first focusable element is focused when a mode opens. Focus is shown from the first focus
  key on, and Enter and `PadButton::South` activate the focused control only while focus is
  shown.
- Up, Down, Left and Right, the D-pad, Tab and Shift+Tab move focus. On a focused slider or
  select, Left and Right change the value instead: a slider by its `step`, a select to the
  previous or next option.
- These keys are built in and not bindable (see [Bindings](./system.md#bindings)).

## UI forms {#ui-forms}

The UI forms are written in `.kix` files and desugar to calls of the functions below with a props
struct. In `fn` and `const fn` code they build values; they are never steps. They follow the
language's delimiter rules (see the Reference chapter
[Forms](../reference/syntax/forms.md)):

- `name![…]` takes elements only;
- `name!(…)` takes attributes and a fixed number of elements;
- `name!{…}` takes attributes and any number of elements.

### Desugaring {#desugaring}

- **Reactive attributes** become `key: Some(|| expr)`, or `key: || expr` when the attribute is
  required.
- **Handler attributes** (`on_*`) become `key: Some(expr)`.
- **Absent optional attributes** become `None`. A missing required attribute, an unknown
  attribute, or the wrong number of elements (`text!` and `image!` take exactly one,
  `game_screen!` and `slider!` none) is a syntax error.
- **Elements.** Each element `e` becomes `|| e`. Inside `column!`, `row!`, `button!` and
  `toggle!`, an element that is syntactically `t![…]` becomes `|| ui::label(t![…])`.

| Form | Desugars to | Attributes | Elements |
| ---- | ----------- | ---------- | -------- |
| `column![…]`, `row![…]` | `ui::column([…])`, `ui::row([…])` | none | `Element` |
| `layer![…]` | `ui::layer([…])` | none | `Placed` |
| `text!(…)` | `ui::text(TextProps { … })` | `style`: reactive `TextStyle` | exactly one `Text` → `content` |
| `image!(…)` | `ui::image(ImageProps { … })` | `fit`: reactive `Fit` | exactly one `ImageAsset` → `source` |
| `game_screen!(…)` | `ui::game_screen(GameScreenProps { … })` | `game`: reactive `Narrative`, required | none |
| `button!{…}` | `ui::button(ButtonProps { … })` | `on_click`: handler `Fn()`; `disabled`: reactive `bool`; `style`: reactive `ButtonStyle` | `Element` → `children` |
| `slider!{…}` | `ui::slider(SliderProps { … })` | `value`: reactive `Int<LO, HI>`, required; `on_change`: handler `Fn(Int<LO, HI>)`; `step`: reactive `u32`; `disabled`: reactive `bool`; `style`: reactive `SliderStyle` | none |
| `toggle!{…}` | `ui::toggle(ToggleProps { … })` | `value`: reactive `bool`, required; `on_change`: handler `Fn(bool)`; `disabled`: reactive `bool`; `style`: reactive `ButtonStyle` | `Element` → `children` |
| `select!{…}` | `ui::select(SelectProps { … })` | `value`: reactive `T`, required; `on_change`: handler `Fn(T)`; `disabled`: reactive `bool`; `style`: reactive `ButtonStyle` | `ui::option(v, label)` → `options` |

For example,

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

desugars to

```kis
ui::button(ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| ui::label(t!["Back"])],
})
```

and

```kix
slider!{value = music().level, on_change = |v| set_level(v)}
```

to

```kis
ui::slider(SliderProps {
    value: || music().level,
    on_change: Some(|v| set_level(v)),
    step: None,
    disabled: None,
    style: None,
})
```

### Function: `column(children)` {#column}

```kis
#[builtin] pub fn column<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

What `column![…]` desugars to: the children stacked top to bottom.

### Function: `row(children)` {#row}

```kis
#[builtin] pub fn row<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

What `row![…]` desugars to: the children side by side, left to right.

### Function: `layer(children)` {#layer}

```kis
#[builtin] pub fn layer<const N: usize>(children: [Fn() -> Placed; N]) -> Element;
```

What `layer![…]` desugars to: the [`Placed`](#placed) children stacked, later children on top.

### Struct: `TextProps` {#textprops}

```kis
pub struct TextProps {
    pub style: Option<Fn() -> TextStyle>,
    pub content: Fn() -> Text,
}
```

The attributes and content of `text!(…)`.

### Function: `text(props)` {#text}

```kis
#[builtin] pub fn text(props: TextProps) -> Element;
```

What `text!(…)` desugars to: reactive text. Without `style`, it uses
[`System::text`](./system.md#system), or the current look's text style inside a button or toggle.

```kix
text!(style = NAME_STYLE, who_text(game))
```

### Struct: `ImageProps` {#imageprops}

```kis
pub struct ImageProps {
    pub fit: Option<Fn() -> Fit>,
    pub source: Fn() -> ImageAsset,
}
```

The attributes and source of `image!(…)`.

### Function: `image(props)` {#image}

```kis
#[builtin] pub fn image(props: ImageProps) -> Element;
```

What `image!(…)` desugars to: an image. The source is reactive, so
`image!(if seen_image(cg) { cg } else { LOCKED })` switches as soon as the condition changes.

### Struct: `GameScreenProps` {#gamescreenprops}

```kis
pub struct GameScreenProps {
    pub game: Fn() -> Narrative,
}
```

The attributes of `game_screen!(…)`.

### Function: `game_screen(props)` {#game_screen}

```kis
#[builtin] pub fn game_screen(props: GameScreenProps) -> Element;
```

What `game_screen!(…)` desugars to: the screen of the running game, as drawn by the story's
renders. The game view usually puts it first in a `layer![…]`, with `.cover()`.

```kix
use std::narrative::text::text_style;
use std::screen::BOTTOM_CENTER;
use std::ui::message;

fn GameView(game: Narrative) -> Element {
    layer![
        game_screen!(game = game).cover(),
        message(game, text_style(26, WHITE)).width(1000).at(point(640, 690), BOTTOM_CENTER),
    ]
}
```

### Struct: `ButtonProps` {#buttonprops}

```kis
pub struct ButtonProps<const N: usize> {
    pub on_click: Option<Fn()>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

The attributes and children of `button!{…}`.

### Function: `button(props)` {#button}

```kis
#[builtin] pub fn button<const N: usize>(props: ButtonProps<N>) -> Element;
```

What `button!{…}` desugars to: a focusable button. `on_click` runs on click or activation; a
disabled button ignores input and shows its disabled look (see
[Interaction looks](./ui-style.md#interaction-looks)). A button paints its current look's
background over its whole box and lays its children out centred on top, so a button without
children is drawn by the background alone, which makes image buttons:

```kix
button!{style = image_style(START, START_HOVER), on_click = || start_game()}.width(300).height(70)
```

### Struct: `SliderProps` {#sliderprops}

```kis
pub struct SliderProps<const LO: i64, const HI: i64> {
    pub value: Fn() -> Int<LO, HI>,
    pub on_change: Option<Fn(Int<LO, HI>)>,
    pub step: Option<Fn() -> u32>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> SliderStyle>,
}
```

The attributes of `slider!{…}`. The range is the range of the value's `Int` type: there are no
`min` or `max` attributes.

### Function: `slider(props)` {#slider}

```kis
#[builtin] pub fn slider<const LO: i64, const HI: i64>(props: SliderProps<LO, HI>) -> Element;
```

What `slider!{…}` desugars to: a focusable slider from `LO` to `HI`, moving in steps of `step`.
`Volume::level` is an `Int<0, 100>`, so a volume slider needs no range:

```kix
slider!{
    value = MUSIC.get().level,
    on_change = |v| MUSIC.set(Volume { level: v, muted: MUSIC.get().muted }),
}
```

### Struct: `ToggleProps` {#toggleprops}

```kis
pub struct ToggleProps<const N: usize> {
    pub value: Fn() -> bool,
    pub on_change: Option<Fn(bool)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

The attributes and children of `toggle!{…}`.

### Function: `toggle(props)` {#toggle}

```kis
#[builtin] pub fn toggle<const N: usize>(props: ToggleProps<N>) -> Element;
```

What `toggle!{…}` desugars to: a focusable on/off control. Activating it (click, Enter or
`PadButton::South`) calls `on_change` with `!value()`. While the value is `true`, the toggle shows
its on look (see [Interaction looks](./ui-style.md#interaction-looks)).

```kix
toggle!{value = SKIP_UNREAD.get(), on_change = |v| SKIP_UNREAD.set(v), t!["Skip unread text"]}
```

### Struct: `SelectOption<T>` {#selectoption}

```kis
pub struct SelectOption<T> {
    pub value: T,
    pub label: Element,
}
```

An option of a `select!{…}`.

### Function: `option(value, label)` {#option}

```kis
pub fn option<T>(value: T, label: Element) -> SelectOption<T> {
    SelectOption { value, label }
}
```

- `value` `T` — the value the option stands for.
- `label` `Element` — what the option shows.
- Returns: `SelectOption<T>` — the option `value`, shown as `label`.
- Kind: `fn` · Builtin: no

### Struct: `SelectProps<T, N>` {#selectprops}

```kis
pub struct SelectProps<T, const N: usize> {
    pub value: Fn() -> T,
    pub on_change: Option<Fn(T)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub options: [Fn() -> SelectOption<T>; N],
}
```

The attributes and options of `select!{…}`.

### Function: `select(props)` {#select}

```kis
#[builtin] pub fn select<T: Eq, const N: usize>(props: SelectProps<T, N>) -> Element;
```

What `select!{…}` desugars to: a focusable control that cycles through its options. It shows the
label of the option whose value equals `value()`, so `T` must be `Eq`. Activating it calls
`on_change` with the value of the next option, wrapping around after the last one; with focus,
Right and Left step to the next and the previous option. When no option equals `value()`, the
next value is the first option's.

```kix
use std::ui::{label, option};

select!{
    value = WINDOW_OPACITY.get(),
    on_change = |v| WINDOW_OPACITY.set(v),
    option(50, label("50%")),
    option(80, label("80%")),
    option(100, label("100%")),
}
```

> **Stability:** Planned — fades and transitions of UI elements are not supported yet; only a
> control's look changes animate (see [`ButtonStyle::change`](./ui-style.md#buttonstyle-change)).
