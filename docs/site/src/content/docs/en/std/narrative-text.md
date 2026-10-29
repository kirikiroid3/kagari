---
title: "std::narrative::text"
---

> **Stability:** Stable — text values, the builtins `t![…]` desugars to, text effects and text
> styles.

`std::narrative::text` declares `Text`, the type of every message, name and label; the builtin
functions that the form `t![…]` desugars to; `ToText`, the bound of interpolation; text effects
(`TextEffect` and its constructors), which style a piece of text and cue its reveal; and
`TextStyle`, the style of a whole text box.

Every function is a `const fn`. `Text`, `TextEffect` and `Mark` are re-exported from
[`std::narrative`](./narrative.md). The prelude imports `Text`, `TextEffect` and the constructors
`emphasis`, `dots`, `color_text`, `bold`, `pause`, `click_wait` and `mark`; everything else is
imported from `std::narrative::text`.

## Overview

|Item|Kind|Summary|
|---|---|---|
|[`Text`](#struct-text)|builtin struct|Constant text, plain or rich|
|[`ruby`](#function-rubybase-reading), [`mono_ruby`](#function-mono_rubybase-readings), [`effect`](#function-effectbase-effect), [`concat`](#function-concatitems)|builtin `const fn`|What `t![…]` items desugar to|
|[`ToText`](#trait-totext), [`to_text`](#function-to_textvalue)|builtin|Interpolation|
|[`Mark`](#struct-mark), [`Weight`](#enum-weight), [`Emphasis`](#enum-emphasis), [`Reveal`](#enum-reveal), [`Pause`](#enum-pause), [`Outline`](#struct-outline), [`Shadow`](#struct-shadow)|data|Parts of a text effect|
|[`TextEffect`](#struct-texteffect), [`PLAIN`](#constant-plain)|struct, constant|A text effect|
|[`color_text` … `mark`](#effect-constructors)|`const fn`|Text-effect constructors|
|[`TextAlign`](#enum-textalign), [`EmphasisStyle`](#enum-emphasisstyle), [`TextStyle`](#struct-textstyle), [`text_style`](#function-text_stylesize-color)|data, `const fn`|The style of a text box|

## Struct: `Text`

```kis
#[builtin] pub struct Text;
```

Constant text: plain (`"Mio"`) or rich (`t![…]`).

- Kind: — · Builtin: yes

- A string literal has type `Text`: `who = "Mio"` needs no `t!`.
- `Text` is `Eq` and finite: in the story, the values of `Text` are the constant texts of the
  program. `fn` code can build other texts at runtime with [`to_text`](#function-to_textvalue).
- Because of that, an entry scene's parameter and return types must not contain `Text`
  ([`E3009`](../diagnostics/kinds.md#e3009)).

## `t![…]` desugaring

`t![…]` is a form; each item becomes one call:

|Item|Desugars to|
|---|---|
|`"…"`|the string literal|
|`["数多"]("あまた")`|`ruby("数多", "あまた")`|
|`["漢字"]("かん"\|"じ")`|`mono_ruby("漢字", ["かん", "じ"])`|
|`!["赤い"](e)`|`effect("赤い", e)`; with a ruby item as base: `effect(ruby(…), e)`|
|`{expr}`|`to_text(expr)`|

The whole form is then:

- `""` with no items;
- the single item's expression with one item, so `t![{DAY}]` is `to_text(DAY)`;
- `concat([…])` of the items otherwise.

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
// concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

The form's syntax is described in [Forms](../reference/syntax/forms.md).

## Function: `ruby(base, reading)`

```kis
#[builtin] pub const fn ruby(base: Text, reading: Text) -> Text;
```

- `base` `Text` — the base text; must be a string literal
  ([`E3021`](../diagnostics/kinds.md#e3021)).
- `reading` `Text` — the reading.
- Returns: `Text` — `base` with `reading` set over it as ruby (group ruby: one reading over the
  whole base).
- Kind: `const fn` · Builtin: yes

A ruby nested inside another ruby's base drops its reading. Ruby dictionaries (ruby declared once
for recurring words) are not planned: ruby is written at each use.

## Function: `mono_ruby(base, readings)`

```kis
#[builtin] pub const fn mono_ruby<const N: usize>(base: Text, readings: [Text; N]) -> Text;
```

- `base` `Text` — the base text; must be a string literal.
- `readings` `[Text; N]` — one reading per character of `base`; must be an array literal.
- Returns: `Text` — `base` with one ruby reading per character.
- Kind: `const fn` · Builtin: yes

`N` must equal the number of base characters
([`E3021`](../diagnostics/kinds.md#e3021)).

## Function: `effect(base, effect)`

```kis
#[builtin] pub const fn effect(base: Text, effect: TextEffect) -> Text;
```

- `base` `Text` — the text to style.
- `effect` `TextEffect` — the effect; must be a constant expression
  ([`E3022`](../diagnostics/kinds.md#e3022)).
- Returns: `Text` — `base` with `effect` applied to it.
- Kind: `const fn` · Builtin: yes

## Function: `concat(items)`

```kis
#[builtin] pub const fn concat<const N: usize>(items: [Text; N]) -> Text;
```

- `items` `[Text; N]` — the parts.
- Returns: `Text` — the items one after another. No whitespace is added or folded.
- Kind: `const fn` · Builtin: yes

## Trait: `ToText`

```kis
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
```

Types a `{expr}` item of `t![…]` may interpolate: `Text`, `String`, every integer type, `Rat` and
`Decimal`.

- Kind: — · Builtin: yes

## Function: `to_text(value)`

```kis
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

- `value` `T` — the value to show.
- Returns: `Text` — `value` as text; what `{expr}` in `t![…]` desugars to.
- Kind: `const fn` · Builtin: yes

- `{expr}` needs `expr: T` with `T: ToText`.
- Integers are written in decimal, `Rat` and `Decimal` exactly.
- In the story, `to_text` of a finite value is tabulated like any `const fn` call, so a line with
  `{DAY}` is still a finite function of the story state.
- In the backlog, interpolated values are recorded as they were shown, unless the step sets
  `history`.

```kix
static mut DAY: Int<1, 7> = 1;

#[scene]
pushdown fn Morning() {
    s!(t!["Day " {DAY}]);
}
```

## Struct: `Mark`

```kis
pub struct Mark(pub u8);
```

A named point in a text's reveal, which renders can anchor the timeline on
(`Screen::at_mark`, see [`std::screen`](./screen.md)). Set with [`mark(n)`](#marks). `Mark` holds
a Rust integer, so it is not finite; the story uses it in constant expressions.

- Kind: — · Builtin: no

## Enum: `Weight`

```kis
pub enum Weight {
    Regular,
    Bold,
}
```

Font weight.

- Kind: — · Builtin: no

## Enum: `Emphasis`

```kis
pub enum Emphasis {
    Styled,
    Dots,
}
```

How emphasized text is drawn.

- `Styled` — with the window's `TextStyle::emphasis`.
- `Dots` — emphasis dots (bouten) over every character, whatever the window's style.
- Kind: — · Builtin: no

## Enum: `Reveal`

```kis
pub enum Reveal {
    Player,
    PerChar(Duration),
    Scaled(Decimal),
    Instant,
}
```

How fast text appears.

|Variant|Speed|
|---|---|
|`Player`|the `text_speed` setting|
|`PerChar(d)`|`d` per character, whatever the setting|
|`Scaled(x)`|the player's speed × `x`, with `x` > 0|
|`Instant`|all at once|

- Kind: — · Builtin: no

## Enum: `Pause`

```kis
pub enum Pause {
    Time(Duration),
    Click,
}
```

A stop in the reveal before the effect's base.

- `Time(d)` — stops for `d`.
- `Click` — stops until a click, which continues the text.
- Kind: — · Builtin: no

## Struct: `Outline`

```kis
pub struct Outline {
    pub color: Color,
    pub width: u32,
}
```

A text outline: `color`, and `width` in pixels.

- Kind: — · Builtin: no

## Struct: `Shadow`

```kis
pub struct Shadow {
    pub color: Color,
    pub offset: Offset,
    pub blur: u32,
}
```

A text shadow: `color`, `offset` in pixels, and `blur` radius in pixels.

- Kind: — · Builtin: no

## Struct: `TextEffect`

```kis
pub struct TextEffect {
    pub color: Option<Color>,
    pub size: Option<Decimal>,
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
```

Styles and cues applied to a piece of text; `None` fields inherit (leave the window's style in
place). `size` is a scale of the window's text size.

- Kind: — · Builtin: no

A text effect is a value, not a function. The effect of `!["…"](…)` must be a constant expression,
so a message is still a constant of its step. Effects combine with [`and`](#texteffectandself-other):

```kix
const RED: Color = rgb(220, 40, 40);

const LINE: Text = t!["It was " !["right there"](bold().and(color_text(RED))) "."];
```

### `TextEffect::and(self, other)`

```kis
pub const fn and(self, other: TextEffect) -> TextEffect
```

- `other` `TextEffect` — the effect laid over this one.
- Returns: `TextEffect` — a right-biased merge: fields set in `other` win, the others come from
  `self`.
- Kind: `const fn` · Builtin: no

## Constant: `PLAIN`

```kis
pub const PLAIN: TextEffect = TextEffect {
    color: None,
    size: None,
    weight: None,
    italic: None,
    font: None,
    outline: None,
    shadow: None,
    emphasis: None,
    reveal: None,
    pause: None,
    mark: None,
};
```

The effect that changes nothing: every field `None`.

## Effect constructors

Each constructor is `PLAIN` with one field set. All are `const fn`, Builtin: no.

|Constructor|Sets|In prelude|
|---|---|---|
|`pub const fn color_text(color: Color) -> TextEffect`|`color`|yes|
|`pub const fn size(scale: Decimal) -> TextEffect`|`size`: `scale` times the window's text size|no|
|`pub const fn bold() -> TextEffect`|`weight: Some(Bold)`|yes|
|`pub const fn italic() -> TextEffect`|`italic: Some(true)`|no|
|`pub const fn font(font: FontAsset) -> TextEffect`|`font`|no|
|`pub const fn outline(color: Color, width: u32) -> TextEffect`|`outline`|no|
|`pub const fn shadow(color: Color, offset: Offset, blur: u32) -> TextEffect`|`shadow`|no|
|`pub const fn emphasis() -> TextEffect`|`emphasis: Some(Styled)`|yes|
|`pub const fn dots() -> TextEffect`|`emphasis: Some(Dots)`|yes|
|`pub const fn speed(per_char: Duration) -> TextEffect`|`reveal: Some(PerChar(per_char))`|no|
|`pub const fn speed_scale(scale: Decimal) -> TextEffect`|`reveal: Some(Scaled(scale))`|no|
|`pub const fn reveal_instant() -> TextEffect`|`reveal: Some(Instant)`|no|
|`pub const fn pause(duration: Duration) -> TextEffect`|`pause: Some(Time(duration))`|yes|
|`pub const fn click_wait() -> TextEffect`|`pause: Some(Click)`|yes|
|`pub const fn mark(n: u8) -> TextEffect`|`mark: Some(Mark(n))`|yes|

### Styles

`color_text(color)`, `size(scale)`, `bold()`, `italic()`, `font(font)`, `outline(color, width)`,
`shadow(color, offset, blur)`, `emphasis()`, `dots()`.

Style fields (`color`, `size`, `weight`, `italic`, `font`, `outline`, `shadow`, `emphasis`) apply to
the effect's base text.

- `emphasis()` renders with the window's `TextStyle::emphasis`.
- `dots()` always draws emphasis dots, whatever the window's style.

```kix
const LINE: Text = t!["これは" ["数多"]("あまた") "の" !["赤い"](emphasis()) "花。"];
```

### Reveal

`speed(per_char)`, `speed_scale(scale)`, `reveal_instant()`.

`reveal` sets how fast the base appears; see [`Reveal`](#enum-reveal). `speed_scale` needs a scale
greater than 0.

```kix
use std::narrative::text::speed;

const LINE: Text = t![!["Slowly, slowly…"](speed(ms(80))) " then all at once."];
```

### Pauses

`pause(duration)`, `click_wait()`.

A pause stops the reveal before the effect's base. The base is usually `""`.

- `pause(d)` stops the reveal for `d`.
- `click_wait()` stops the reveal until a click. That click continues the text; it does not
  complete it and is not a `NextStep`. It is not recorded in saves.
- In auto mode a click pause continues after the `auto_delay` setting.

```kix
const LINE: Text = t!["Wait" ![""](pause(ms(500))) "… " ![""](click_wait()) "there it is."];
```

### Marks

`mark(n)`.

A mark names a point of the reveal, so that a render can schedule animations there (see
[`std::screen::anim`](./screen-anim.md)).

- `mark(n)` records the time the reveal reaches that point.
- A duplicate mark in one text is a const-evaluation error
  ([`E4010`](../diagnostics/eval.md#e4010)).
- A mark the step's text does not contain resolves to the end of the reveal.

```kix
use std::narrative::Mark;

const fn startled(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(1));
    // changes scheduled here start when the reveal reaches mark 1
    screen
}

#[scene]
pushdown fn Surprise() {
    s!(render = startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
}
```

Here the render `startled` can call `screen.at_mark(Mark(1))` and change a face at the moment the
reveal reaches the dash.

> **Stability:** Planned — syncing text and animation beyond marks: an animation that follows each
> revealed character, or a reveal that waits for an animation. Not designed yet.

### Clicks and the backlog

- A click during the reveal shows the full text at once, except that a `click_wait()` pause still
  waits for its own click.
- Text in the backlog is shown without pauses or reveal. Interpolated values are recorded as they
  were shown, unless the step sets `history`.

## Enum: `TextAlign`

```kis
pub enum TextAlign {
    Start,
    Center,
    End,
}
```

Horizontal text alignment.

- Kind: — · Builtin: no

## Enum: `EmphasisStyle`

```kis
pub enum EmphasisStyle {
    Dots,
    Bold,
    Italic,
}
```

How `Emphasis::Styled` text (from `emphasis()`) is drawn in a text box.

- Kind: — · Builtin: no

## Struct: `TextStyle`

```kis
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
```

The style of a text box: the message window (`ui::message`), labels and `text!` in the UI, and
label layers on the screen.

|Field|Meaning|
|---|---|
|`font`|`None`: `System::font`|
|`size`, `ruby_size`|pixels|
|`color`|the text colour|
|`line_height`|a multiple of `size`|
|`letter_spacing`|pixels between characters|
|`align`|horizontal alignment|
|`outline`, `shadow`|`None`: none|
|`emphasis`|how `Emphasis::Styled` text is drawn|
|`hanging_indent`|a line that starts with an opening bracket (`「『（`) indents its wrapped lines by the bracket's width|
|`vertical`|vertical writing|

- Kind: — · Builtin: no

## Function: `text_style(size, color)`

```kis
pub const fn text_style(size: u32, color: Color) -> TextStyle
```

- `size` `u32` — the text size in pixels.
- `color` `Color` — the text colour.
- Returns: `TextStyle` — a style with font `None`, `line_height` 1.5, `letter_spacing` 0, align
  `Start`, no outline or shadow, `ruby_size` = `size / 2`, emphasis `Dots`, `hanging_indent` false
  and `vertical` false.
- Kind: `const fn` · Builtin: no

### `TextStyle` builders

Each builder returns the style with one field replaced; builders of `Option` fields wrap the value
in `Some`. All are `const fn`, Builtin: no.

```kis
pub const fn font(mut self, font: FontAsset) -> TextStyle
pub const fn line_height(mut self, value: Decimal) -> TextStyle
pub const fn letter_spacing(mut self, value: i32) -> TextStyle
pub const fn align(mut self, value: TextAlign) -> TextStyle
pub const fn outline(mut self, value: Outline) -> TextStyle
pub const fn shadow(mut self, value: Shadow) -> TextStyle
pub const fn ruby_size(mut self, value: u32) -> TextStyle
pub const fn emphasis(mut self, value: EmphasisStyle) -> TextStyle
pub const fn hanging_indent(mut self, value: bool) -> TextStyle
pub const fn vertical(mut self, value: bool) -> TextStyle
```

|Builder|Sets|
|---|---|
|`font(font)`|`font: Some(font)`|
|`line_height(value)`|`line_height`: `value` times the size|
|`letter_spacing(value)`|`letter_spacing`: pixels between characters|
|`align(value)`|`align`|
|`outline(value)`|`outline: Some(value)`|
|`shadow(value)`|`shadow: Some(value)`|
|`ruby_size(value)`|`ruby_size`: pixels|
|`emphasis(value)`|`emphasis`: how `Emphasis::Styled` text is drawn|
|`hanging_indent(value)`|`hanging_indent`|
|`vertical(value)`|`vertical`: vertical or horizontal writing|

```kis
use std::narrative::text::{Outline, TextStyle, text_style};

const WINDOW_TEXT: TextStyle = text_style(26, WHITE)
    .outline(Outline { color: BLACK, width: 2 })
    .hanging_indent(true);
```
