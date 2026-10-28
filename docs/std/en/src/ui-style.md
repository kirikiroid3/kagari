# `std::ui::style`

> **Stability:** Stable — the looks of UI controls.

`std::ui::style` describes how backgrounds and controls look: [`Paint`](#paint) fills a box, a
[`Look`](#look) pairs a background with a text style, a [`ButtonStyle`](#buttonstyle) gives a
look for each interaction state of a button, toggle or select, and a
[`SliderStyle`](#sliderstyle) paints a slider.

Styles are plain data built by `const fn`, so a studio declares them as `const` items and passes
them to the `style` attribute of a control form or to
[`Element::background`](./ui.md#element-background).

```kis
use std::ui::style::{ButtonStyle, Look, Paint, SliderStyle, button_style};
```

## Overview {#overview}

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Paint`](#paint) | enum | What fills a box |
| [`Look`](#look) | struct | A background and a text style |
| [`ButtonStyle`](#buttonstyle) | struct | The looks of a button-like control per interaction state |
| [`button_style(idle)`](#button_style) | `const fn` | A style with only the idle look |
| [`ButtonStyle` builder methods](#buttonstyle-hover) | `const fn` | Add looks, a tween and sounds |
| [`SliderStyle`](#sliderstyle) | struct | The paints of a slider |

## Enum: `Paint` {#paint}

```kis
pub enum Paint {
    None,
    Color(Color),
    Image(ImageAsset),
    /// An image whose corners stay unscaled; `insets` are the corner sizes.
    NineSlice { image: ImageAsset, insets: Insets },
}
```

| Variant | Meaning |
| ------- | ------- |
| `None` | Nothing; the box is transparent |
| `Color(c)` | A solid colour, for example `rgba(0, 0, 0, 200)` |
| `Image(i)` | The image stretched over the box |
| `NineSlice { image, insets }` | A nine-slice image: the corners, of the sizes given by `insets`, stay unscaled; the edges and the centre stretch. For window frames and buttons that resize |

## Struct: `Look` {#look}

```kis
pub struct Look {
    pub background: Paint,
    pub text: TextStyle,
}
```

A background and a text style. The `text` style applies to labels inside the control (see
[Interaction looks](#interaction-looks)).

## Struct: `ButtonStyle` {#buttonstyle}

```kis
pub struct ButtonStyle {
    pub idle: Look,
    pub hover: Option<Look>,
    pub focus: Option<Look>,
    pub pressed: Option<Look>,
    /// Replaces every other look while the control is disabled.
    pub disabled: Option<Look>,
    /// How look changes animate.
    pub change: Tween,
    pub hover_sound: Option<Sound>,
    pub click_sound: Option<Sound>,
}
```

The looks of a button-like control (`button!`, `toggle!`, `select!`) in each interaction state,
how changes between them animate, and the sounds it plays. UI sounds play on the sound bus.

### Interaction looks {#interaction-looks}

- While the control is disabled, the `disabled` look replaces every other look. Without a
  `disabled` look, a disabled control shows `idle` at half opacity.
- Otherwise the first `Some` of `pressed` (while the pointer is held down over the control),
  `focus` (while the control has focus and focus is shown) and `hover` applies; `idle` is the
  fallback.
- A toggle whose value is `true` and that would show `idle` shows its **on look** instead: the
  `pressed` look if there is one, else the `hover` look, else `idle` with a 2 px outline in the
  idle text colour.
- Look changes animate with `change`.
- Labels inside a button or toggle use the current look's `text`. Elsewhere, `label` and `text!`
  use [`System::text`](./system.md#system) unless `text!` is given a `style`. There is no other
  cascade.

### Function: `button_style(idle)` {#button_style}

```kis
pub const fn button_style(idle: Look) -> ButtonStyle {
    ButtonStyle {
        idle,
        hover: None,
        focus: None,
        pressed: None,
        disabled: None,
        change: instant(),
        hover_sound: None,
        click_sound: None,
    }
}
```

- `idle` `Look` — the look at rest.
- Returns: `ButtonStyle` — a style with only the idle look, instant changes and no sounds.
- Kind: `const fn` · Builtin: no

Add the other parts with the builder methods below. Each takes the style by value and returns it
changed, so they chain.

### `ButtonStyle::hover(look)` {#buttonstyle-hover}

```kis
pub const fn hover(mut self, look: Look) -> ButtonStyle
```

- Returns: this style with a hover look.

### `ButtonStyle::focus(look)` {#buttonstyle-focus}

```kis
pub const fn focus(mut self, look: Look) -> ButtonStyle
```

- Returns: this style with a focus look, shown while the control has keyboard or pad focus.

### `ButtonStyle::pressed(look)` {#buttonstyle-pressed}

```kis
pub const fn pressed(mut self, look: Look) -> ButtonStyle
```

- Returns: this style with a pressed look.

### `ButtonStyle::disabled(look)` {#buttonstyle-disabled}

```kis
pub const fn disabled(mut self, look: Look) -> ButtonStyle
```

- Returns: this style with a disabled look.

### `ButtonStyle::change(tween)` {#buttonstyle-change}

```kis
pub const fn change(mut self, tween: Tween) -> ButtonStyle
```

- `tween` `Tween` — the duration, easing and delay of look changes (see
  [`std::screen::anim`](./screen-anim.md)).
- Returns: this style with look changes animated by `tween`.

### `ButtonStyle::hover_sound(sound)` {#buttonstyle-hover_sound}

```kis
pub const fn hover_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — a one-shot sound (see [`std::screen::audio`](./screen-audio.md)).
- Returns: this style playing `sound` on hover.

### `ButtonStyle::click_sound(sound)` {#buttonstyle-click_sound}

```kis
pub const fn click_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — a one-shot sound.
- Returns: this style playing `sound` on click.

All builder methods are `const fn` · Builtin: no.

```kis
use std::narrative::text::text_style;
use std::screen::rgba;
use std::ui::style::{ButtonStyle, Look, Paint, button_style};

const CLICK: AudioAsset = asset("sound/click.ogg");

const MENU_BUTTON: ButtonStyle = button_style(Look { background: Paint::None, text: text_style(28, WHITE) })
    .hover(Look { background: Paint::Color(rgba(255, 255, 255, 40)), text: text_style(28, WHITE) })
    .change(tween(ms(150)))
    .click_sound(sound(CLICK));
```

An image button swaps its background image on hover and focus:

```kis
const fn image_style(idle: ImageAsset, hover: ImageAsset) -> ButtonStyle {
    let text = text_style(1, WHITE);
    button_style(Look { background: Paint::Image(idle), text })
        .hover(Look { background: Paint::Image(hover), text })
        .focus(Look { background: Paint::Image(hover), text })
}
```

## Struct: `SliderStyle` {#sliderstyle}

```kis
pub struct SliderStyle {
    pub track: Paint,
    pub fill: Paint,
    pub thumb: Paint,
    pub thumb_size: Size,
}
```

The paints of a [`slider!`](./ui.md#slider).

- `track` — the track, from the thumb's centre at the minimum to its centre at the maximum.
- `fill` — the track from its start to the thumb's centre, drawn over `track`.
- `thumb` — the handle, of size `thumb_size`, centred at the position of the value.
- `thumb_size` — the size of the handle, in pixels.
