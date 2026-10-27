# Text

`std::narrative::text` declares `Text`, the builtins `t![…]` desugars to, text effects and text
styles. `Text`, `TextEffect` and `Mark` are re-exported from `std::narrative`.

## `Text`

```kis
#[builtin] pub struct Text;
#[builtin] pub const fn ruby(base: Text, reading: Text) -> Text;
#[builtin] pub const fn mono_ruby<const N: usize>(base: Text, readings: [Text; N]) -> Text;
#[builtin] pub const fn effect(base: Text, effect: TextEffect) -> Text;
#[builtin] pub const fn concat<const N: usize>(items: [Text; N]) -> Text;
```

- A string literal has type `Text`: `who = "Mio"` needs no `t!`.
- `ruby(base, reading)` sets one reading over the whole base (group ruby).
- `mono_ruby(base, readings)` sets one reading per character of the base; `N` must equal the
  number of base characters.
- `effect(base, e)` shows `base` with the text effect `e`.
- `concat(items)` puts the items one after another. No whitespace is added or folded.

In the story, the values of `Text` are the constant texts of the program, so `Text` is a finite
type. `fn` code can build other texts at runtime with [`to_text`](#interpolation).

## `t![…]` desugaring

`t![…]` is a form; each item becomes one call:

|Item|Desugars to|
|---|---|
|`"…"`|the string literal|
|`["数多"]("あまた")`|`ruby("数多", "あまた")`|
|`["漢字"]("かん"\|"じ")`|`mono_ruby("漢字", ["かん", "じ"])`|
|`!["赤い"](e)`|`effect("赤い", e)`, with a ruby item as base: `effect(ruby(…), e)`|
|`{expr}`|`to_text(expr)`|

The whole form is then:

- `""` with no items;
- the single item's expression with one item, so `t![{DAY}]` is `to_text(DAY)`;
- `concat([…])` of the items otherwise.

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
// concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

## Interpolation

```kis
/// Types a `{expr}` item of `t![…]` may interpolate.
#[builtin] pub trait ToText {}
impl ToText for Text {}
impl ToText for String {}
impl<const LO: i64, const HI: i64> ToText for Int<LO, HI> {}
impl ToText for u8 {}
impl ToText for u16 {}
impl ToText for u32 {}
impl ToText for u64 {}
impl ToText for u128 {}
impl ToText for i8 {}
impl ToText for i16 {}
impl ToText for i32 {}
impl ToText for i64 {}
impl ToText for i128 {}
impl ToText for usize {}
impl ToText for isize {}
impl ToText for Rat {}
impl ToText for Decimal {}
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

- `{expr}` needs `expr: T` with `T: ToText`.
- Integers are written in decimal, `Rat` and `Decimal` exactly.
- In the story, `to_text` of a finite value is tabulated like any `const fn` call, so a line with
  `{DAY}` is still a finite function of the story state. See [Checks](./checks.md#type-rules).

```kix
static mut DAY: Int<1, 7> = 1;

s!(t!["Day " {DAY}]);
```

## Text effects

```kis
pub struct Mark(pub u8);
pub enum Weight { Regular, Bold }
pub enum Emphasis { Styled, Dots }
pub enum Reveal { Player, PerChar(Duration), Scaled(Decimal), Instant }
pub enum Pause { Time(Duration), Click }
pub struct Outline { pub color: Color, pub width: u32 }
pub struct Shadow { pub color: Color, pub offset: Offset, pub blur: u32 }

pub struct TextEffect {
    pub color: Option<Color>,
    pub size: Option<Decimal>,        // scale of the window's text size
    pub weight: Option<Weight>,
    pub italic: Option<bool>,
    pub font: Option<FontAsset>,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
    pub emphasis: Option<Emphasis>,
    pub reveal: Option<Reveal>,
    pub pause: Option<Pause>,
    pub mark: Option<Mark>,
}
/// The effect that changes nothing: every field `None`.
pub const PLAIN: TextEffect = TextEffect { … };
impl TextEffect {
    /// Right-biased merge: fields set in `other` win.
    pub const fn and(self, other: TextEffect) -> TextEffect { … }
}
```

A text effect is a value, not a function. The target of `!["…"](…)` is a constant expression, so
a message is still a constant of its step. A `None` field leaves the window's style in place.
Effects combine with `and`:

```kix
const RED: Color = rgb(220, 40, 40);

t!["It was " !["right there"](bold().and(color_text(RED))) "."]
```

### Constructors

Each constructor is `PLAIN` with one field set.

|Constructor|Sets|
|---|---|
|`pub const fn color_text(color: Color) -> TextEffect`|`color`|
|`pub const fn size(scale: Decimal) -> TextEffect`|`size`: a scale of the window's text size|
|`pub const fn bold() -> TextEffect`|`weight: Bold`|
|`pub const fn italic() -> TextEffect`|`italic: true`|
|`pub const fn font(font: FontAsset) -> TextEffect`|`font`|
|`pub const fn outline(color: Color, width: u32) -> TextEffect`|`outline`|
|`pub const fn shadow(color: Color, offset: Offset, blur: u32) -> TextEffect`|`shadow`|
|`pub const fn emphasis() -> TextEffect`|`emphasis: Styled`|
|`pub const fn dots() -> TextEffect`|`emphasis: Dots` (emphasis dots, bouten)|
|`pub const fn speed(per_char: Duration) -> TextEffect`|`reveal: PerChar(per_char)`|
|`pub const fn speed_scale(scale: Decimal) -> TextEffect`|`reveal: Scaled(scale)`|
|`pub const fn reveal_instant() -> TextEffect`|`reveal: Instant`|
|`pub const fn pause(duration: Duration) -> TextEffect`|`pause: Time(duration)`|
|`pub const fn click_wait() -> TextEffect`|`pause: Click`|
|`pub const fn mark(n: u8) -> TextEffect`|`mark: Mark(n)`|

The prelude imports `emphasis`, `dots`, `color_text`, `bold`, `pause`, `click_wait` and `mark`.

### Styles

Style fields (`color`, `size`, `weight`, `italic`, `font`, `outline`, `shadow`, `emphasis`) apply
to the effect's base text.

- `Emphasis::Styled` renders with the window's `TextStyle::emphasis`.
- `Emphasis::Dots` always draws emphasis dots, whatever the window's style.

### Reveal

`reveal` sets how fast the base appears:

|Variant|Speed|
|---|---|
|`Player`|the `text_speed` setting|
|`Scaled(x)`|the player's speed × `x`, with `x` > 0|
|`PerChar(d)`|`d` per character, whatever the setting|
|`Instant`|the base at once|

```kix
t![!["Slowly, slowly…"](speed(ms(80))) " then all at once."]
```

### Pauses

A pause stops the reveal before the effect's base. The base is usually `""`.

- `Pause::Time(d)` stops the reveal for `d`.
- `Pause::Click` stops the reveal until a click. That click continues the text; it does not
  complete it and is not a `NextStep`. It is not recorded in saves.
- In auto mode a click pause continues after the `auto_delay` setting.

```kix
t!["Wait" ![""](pause(ms(500))) "… " ![""](click_wait()) "there it is."]
```

### Marks

A mark names a point of the reveal, so that a render can schedule animations there (see
[Animation](./animation.md#the-timeline)).

- `mark(n)` records the time the reveal reaches that point.
- A duplicate mark in one text is a const-evaluation error.
- A mark the step's text does not contain resolves to the end of the reveal.

```kix
s!(render = startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
```

Here `startled` can call `screen.at_mark(Mark(1))` and change a face at the moment the reveal
reaches the dash.

### Clicks and the backlog

- A click during the reveal shows the full text at once, except that a `Pause::Click` still waits
  for its own click.
- Text in the backlog is shown without pauses or reveal. Interpolated values are recorded as they
  were shown, unless the step sets `history`.
- Ruby dictionaries (ruby declared once for recurring words) are not planned: ruby is written at
  each use in `t![…]`.

## `TextStyle`

```kis
pub enum TextAlign { Start, Center, End }
pub enum EmphasisStyle { Dots, Bold, Italic }
pub struct TextStyle {
    pub font: Option<FontAsset>,
    pub size: u32,
    pub color: Color,
    pub line_height: Decimal,
    pub letter_spacing: i32,
    pub align: TextAlign,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
    pub ruby_size: u32,
    pub emphasis: EmphasisStyle,
    pub hanging_indent: bool,
    pub vertical: bool,
}
/// font None, line_height 1.5, letter_spacing 0, Start, no outline/shadow,
/// ruby_size = size / 2, emphasis Dots, hanging_indent false, vertical false.
pub const fn text_style(size: u32, color: Color) -> TextStyle { … }
impl TextStyle {
    pub const fn font(mut self, font: FontAsset) -> TextStyle { … }
    pub const fn line_height(mut self, value: Decimal) -> TextStyle { … }
    pub const fn letter_spacing(mut self, value: i32) -> TextStyle { … }
    pub const fn align(mut self, value: TextAlign) -> TextStyle { … }
    pub const fn outline(mut self, value: Outline) -> TextStyle { … }
    pub const fn shadow(mut self, value: Shadow) -> TextStyle { … }
    pub const fn ruby_size(mut self, value: u32) -> TextStyle { … }
    pub const fn emphasis(mut self, value: EmphasisStyle) -> TextStyle { … }
    pub const fn hanging_indent(mut self, value: bool) -> TextStyle { … }
    pub const fn vertical(mut self, value: bool) -> TextStyle { … }
}
```

A `TextStyle` is the style of a text box: the message window (`ui::message`), labels and
`text!` in the UI, and label layers on the screen.

|Field|Meaning|
|---|---|
|`font`|`None`: `System::font`|
|`size`, `ruby_size`|pixels|
|`line_height`|a multiple of `size`|
|`letter_spacing`|pixels between characters|
|`emphasis`|how `Emphasis::Styled` text is drawn|
|`hanging_indent`|a line that starts with an opening bracket (`「『（`) indents its wrapped lines by the bracket's width|
|`vertical`|vertical writing|

Each builder returns the style with one field replaced; builders of `Option` fields wrap the value
in `Some`.

```kis
use std::narrative::text::{Outline, TextStyle, text_style};

const WINDOW_TEXT: TextStyle = text_style(26, WHITE)
    .outline(Outline { color: BLACK, width: 2 })
    .hanging_indent(true);
```
