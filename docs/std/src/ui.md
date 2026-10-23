# UI

`std::ui` declares UI values, reactivity, the UI functions and the targets of the UI forms.
`std::ui::style` declares the looks of controls. UI is `fn` code; styles are `const fn` data.

```kis
pub mod style;
```

*Status:* the UI forms are designed here; their lowering is not implemented yet, and the compiler
reports each UI form with `lower/ui-form-not-implemented`.

## Elements

```kis
#[builtin] pub struct Element;
#[builtin] pub struct Placed;
pub enum Align { Start, Center, End, Stretch, SpaceBetween }
pub enum Fit { Stretch, Contain, Cover, Natural }
```

- An `Element` is a UI value. A component is a `fn` that returns one, wrapped in its mode type
  (`RootPage`, `Overlay`, `GameOverlay`, `Dialog`), or a plain `Element` for the game view.
- A `Placed` is an element with a place in a `layer!`.
- `Fit` is how an image fills its box; `Natural` keeps its own size.

## Components and reactivity

Components and reactivity follow SolidJS.

- A component runs **once**, when its mode is shown.
- Reactive sources are signal getters, `Setting::get`, `Meta::get`, the `Narrative` reads,
  `saves::info` and the seen sets.
- A reactive closure re-runs when a source it read changes. The reactive attributes and the
  elements of UI forms are such closures (see [UI form desugaring](#ui-form-desugaring)).

```kis
#[builtin] pub fn signal<T>(init: T) -> (Fn() -> T, Fn(T));
#[builtin] pub fn effect(run: Fn());
#[builtin] pub fn each<T>(items: Fn() -> Vec<T>, item: Fn(T) -> Element) -> Element;
#[builtin] pub fn show(when: Fn() -> bool, then: Fn() -> Element) -> Element;
```

|Function|Meaning|
|---|---|
|`signal(init)`|a getter and a setter; the getter is a reactive source|
|`effect(run)`|runs `run` now and again whenever a source it read changes|
|`each(items, item)`|one element per item, kept in sync with `items`|
|`show(when, then)`|`then()` while `when()` is true, else nothing|

Signals exist only in `fn` code: the story and `const fn` cannot use them.

## Content

```kis
#[builtin] pub fn label(text: Text) -> Element;
#[builtin] pub fn spacer() -> Element;
#[builtin] pub fn scroll(child: Fn() -> Element) -> Element;
#[builtin] pub fn thumbnail(slot: SaveSlot) -> Element;
/// The current line: reveal, text effects, pauses, marks and the click-wait glyph.
#[builtin] pub fn message(game: Narrative, style: TextStyle) -> Element;
```

- `label(text)` is static text; `text!(…)` is its reactive form.
- `spacer()` is an empty element that grows.
- `scroll(child)` is a scrollable box.
- `thumbnail(slot)` is the screenshot of a save slot.
- `message(game, style)` is the message window's text: the current line with its reveal, text
  effects, pauses and marks, and the click-wait glyph.

## Layout

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

- `column`/`row` lay children out as a flex container with `gap` and `align`; `grow` is a flex
  item factor.
- `layer` stacks `Placed` children: `at` places the child's anchor at a point; `cover` fills the
  layer.
- Sizes are pixels of the design resolution; `opacity` is a fraction `0.0..=1.0`.
- Layout is an engine pass over the element tree. It is recomputed on resize and never saved.

```kix
fn Title() -> RootPage {
    RootPage(layer![
        image!(fit = Fit::Cover, TITLE_BG).cover(),
        column![
            start_button(),
            button!{on_click = || nav::call(Config), t!["Config"]},
            button!{on_click = || nav::quit(), t!["Quit"]},
        ].gap(16).at(point(640, 520), std::screen::CENTER),
    ])
}
```

## Focus

- Focusable elements are button, slider, toggle and select.
- Focus follows document order unless `focus_order` is set.
- The first focusable element is focused when a mode opens.
- Up/Down/Left/Right/Tab move focus; Enter and PadSouth activate the focused control. These keys
  are built in and not bindable.

## Styles

```kis
use crate::narrative::text::TextStyle;
use crate::screen::{Color, Insets, Size};
use crate::screen::anim::{Tween, instant};
use crate::screen::audio::Sound;

pub enum Paint { None, Color(Color), Image(ImageAsset), NineSlice { image: ImageAsset, insets: Insets } }
pub struct Look { pub background: Paint, pub text: TextStyle }
pub struct ButtonStyle {
    pub idle: Look, pub hover: Option<Look>, pub focus: Option<Look>, pub pressed: Option<Look>,
    pub disabled: Option<Look>, pub change: Tween,
    pub hover_sound: Option<Sound>, pub click_sound: Option<Sound>,
}
/// Other looks None, change instant(), no sounds.
pub const fn button_style(idle: Look) -> ButtonStyle { … }
impl ButtonStyle {
    pub const fn hover(mut self, look: Look) -> ButtonStyle { … }
    pub const fn focus(mut self, look: Look) -> ButtonStyle { … }
    pub const fn pressed(mut self, look: Look) -> ButtonStyle { … }
    pub const fn disabled(mut self, look: Look) -> ButtonStyle { … }
    pub const fn change(mut self, tween: Tween) -> ButtonStyle { … }
    pub const fn hover_sound(mut self, sound: Sound) -> ButtonStyle { … }
    pub const fn click_sound(mut self, sound: Sound) -> ButtonStyle { … }
}
pub struct SliderStyle { pub track: Paint, pub fill: Paint, pub thumb: Paint, pub thumb_size: Size }
```

- `Paint::NineSlice` draws an image whose corners stay unscaled; `insets` are the corner sizes.
- UI sounds play on the sound bus.

### Interaction looks

- `disabled` replaces every other look.
- Otherwise the first `Some` of `pressed`, `focus`, `hover` wins, and `idle` is the fallback.
- Look changes animate with `change`.
- Labels inside a button or toggle use the current look's `text`. Elsewhere, `label`/`text!` use
  `System::text` unless `style` is given. There is no other cascade.

```kis
use std::narrative::text::text_style;
use std::screen::rgba;
use std::ui::style::{ButtonStyle, Look, Paint, button_style};

const MENU_BUTTON: ButtonStyle = button_style(Look { background: Paint::None, text: text_style(28, WHITE) })
    .hover(Look { background: Paint::Color(rgba(255, 255, 255, 40)), text: text_style(28, WHITE) })
    .change(tween(ms(150)))
    .click_sound(sound(CLICK));
```

## UI forms

```kis
#[builtin] pub fn column<const N: usize>(children: [Fn() -> Element; N]) -> Element;
#[builtin] pub fn row<const N: usize>(children: [Fn() -> Element; N]) -> Element;
#[builtin] pub fn layer<const N: usize>(children: [Fn() -> Placed; N]) -> Element;
pub struct TextProps { pub style: Option<Fn() -> TextStyle>, pub content: Fn() -> Text }
#[builtin] pub fn text(props: TextProps) -> Element;
pub struct ImageProps { pub fit: Option<Fn() -> Fit>, pub source: Fn() -> ImageAsset }
#[builtin] pub fn image(props: ImageProps) -> Element;
pub struct GameScreenProps { pub game: Fn() -> Narrative }
#[builtin] pub fn game_screen(props: GameScreenProps) -> Element;
pub struct ButtonProps<const N: usize> {
    pub on_click: Option<Fn()>, pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>, pub children: [Fn() -> Element; N],
}
#[builtin] pub fn button<const N: usize>(props: ButtonProps<N>) -> Element;
pub struct SliderProps<const LO: i128, const HI: i128> {
    pub value: Fn() -> Int<LO, HI>, pub on_change: Option<Fn(Int<LO, HI>)>,
    pub step: Option<Fn() -> u32>, pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> SliderStyle>,
}
#[builtin] pub fn slider<const LO: i128, const HI: i128>(props: SliderProps<LO, HI>) -> Element;
pub struct ToggleProps<const N: usize> {
    pub value: Fn() -> bool, pub on_change: Option<Fn(bool)>, pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>, pub children: [Fn() -> Element; N],
}
#[builtin] pub fn toggle<const N: usize>(props: ToggleProps<N>) -> Element;
pub struct SelectOption<T> { pub value: T, pub label: Element }
pub fn option<T>(value: T, label: Element) -> SelectOption<T> { SelectOption { value, label } }
pub struct SelectProps<T, const N: usize> {
    pub value: Fn() -> T, pub on_change: Option<Fn(T)>, pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>, pub options: [Fn() -> SelectOption<T>; N],
}
#[builtin] pub fn select<T: Eq, const N: usize>(props: SelectProps<T, N>) -> Element;
```

The forms follow the language's delimiter rules: `name![…]` takes elements only, `name!(…)` takes
attributes and a fixed number of elements, `name!{…}` takes attributes and any number of
elements.

### UI form desugaring

The lowering is not implemented yet; this table is the contract for it. Rules:

- **Reactive attributes** become `key: Some(|| expr)`, or `key: || expr` when the attribute is
  required.
- **Handler attributes** (`on_*`) become `key: Some(expr)`.
- **Absent optional attributes** become `None`. A missing required attribute, or an unknown
  attribute, is a lowering error.
- **Elements.** Each element `e` becomes `|| e`. Inside `column!`, `row!`, `button!` and
  `toggle!`, an element that is syntactically `t![…]` becomes `|| ui::label(t![…])`.

|Form|Desugars to|Attributes|Elements|
|---|---|---|---|
|`column![…]`, `row![…]`|`ui::column([…])`, `ui::row([…])`|none|`Element`|
|`layer![…]`|`ui::layer([…])`|none|`Placed`|
|`text!(…)`|`ui::text(TextProps { … })`|`style`: reactive `TextStyle`|exactly one `Text` → `content`|
|`image!(…)`|`ui::image(ImageProps { … })`|`fit`: reactive `Fit`|exactly one `ImageAsset` → `source`|
|`game_screen!(…)`|`ui::game_screen(GameScreenProps { … })`|`game`: reactive `Narrative`, required|none|
|`button!{…}`|`ui::button(ButtonProps { … })`|`on_click`: handler `Fn()`; `disabled`: reactive `bool`; `style`: reactive `ButtonStyle`|`Element` → `children`|
|`slider!{…}`|`ui::slider(SliderProps { … })`|`value`: reactive `Int<LO, HI>`, required; `on_change`: handler `Fn(Int<LO, HI>)`; `step`: reactive `u32`; `disabled`; `style`: reactive `SliderStyle`|none|
|`toggle!{…}`|`ui::toggle(ToggleProps { … })`|`value`: reactive `bool`, required; `on_change`: handler `Fn(bool)`; `disabled`; `style`: reactive `ButtonStyle`|`Element` → `children`|
|`select!{…}`|`ui::select(SelectProps { … })`|`value`: reactive `T`, required; `on_change`: handler `Fn(T)`; `disabled`; `style`: reactive `ButtonStyle`|`ui::option(v, label)` → `options`|

The slider's range is the range of its value's `Int` type: there are no `min`/`max` attributes.

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
