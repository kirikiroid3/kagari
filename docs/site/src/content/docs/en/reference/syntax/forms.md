---
title: "Forms"
---

A **form** is fixed syntax that builds a step, a text or a piece of UI: `s!(…)`, `choice!{…}`,
`t![…]`, and the UI forms such as `column![…]` and `button!{…}`. Each form desugars to ordinary
struct expressions, closures and calls of standard library items, which a `.kis` file may also
write by hand.

Forms are not macros. There are no macros in Kagari, and the set of forms is fixed by the
language. An unknown form name is a syntax error (`E0001`).

What steps mean for the story is in [Steps](../concepts/steps.md); what the UI forms build is in
the STD Reference, [`std::ui`](../../std/ui.md). This chapter gives their exact
syntax and desugaring.

## Where forms are allowed

- Forms are allowed only in `.kix` files. A form in a `.kis` file is `E0001`.
- A form is an expression: a name, `!`, then one of `(`, `[` or `{`. `a != b` is never a form.
- Desugaring happens before name resolution, and the desugared code always names the standard
  library items (`std::narrative::push_step`, `std::ui::button`, …), even where a local item has
  the same name.
- After desugaring, the ordinary rules apply: a step runs only in a scene (`E3002`), UI functions
  are `fn` code ([call rules](../concepts/kinds.md#call-rules)), and so on.

## General syntax

```text
form       = name "!" "(" entries ")"
           | name "!" "[" entries "]"
           | name "!" "{" entries "}"
           | "t" "!" "[" { text-item } "]"
entries    = [ entry { "," entry } [ "," ] ]
entry      = attribute | arm | expression
attribute  = name "=" expression
arm        = expression [ "if" expression ] "=>" expression
text-item  = string | ruby | effect | "{" expression "}"
ruby       = "[" string "]" "(" string { "|" string } ")"
effect     = "!" "[" ( string | ruby ) "]" "(" expression ")"
```

The delimiter tells what a form accepts:

| Delimiter | Accepts | Forms |
| --------- | ------- | ----- |
| `name!( … )` | Attributes, then a fixed number of elements | `s!`, `text!`, `image!`, `game_screen!` |
| `name![ … ]` | Elements only; no attributes | `t!`, `column!`, `row!`, `layer!` |
| `name!{ … }` | Attributes, then elements | `choice!`, `button!`, `slider!`, `toggle!`, `select!` |

Every form has exactly one delimiter. These are syntax errors (`E0001`):

- the wrong delimiter (`` `s!` is written `s!(…)` ``);
- an attribute in a `[…]` form, an attribute after an element, a duplicate attribute, an
  attribute the form does not know, a missing required attribute;
- the wrong number of elements;
- an arm (`… => …`) anywhere but in `choice!`;
- a comma between the items of `t![…]`, which are juxtaposed.

`key = expression` is an attribute only directly inside a form; anywhere else `=` is assignment.
Entries are separated by commas, and a trailing comma is allowed.

A brace form (`choice!{…}`, `button!{…}`, …) at the start of a statement is block-like: it needs no
`;` after it. `s!(…)` and the other forms need one, like any expression statement.

### Summary of the forms

| Form | Attributes | Elements | Desugars to |
| ---- | ---------- | -------- | ----------- |
| `s!(…)` | `who`, `voice`, `render`, `history` | at most one, a `t![…]` | `push_step(Say { … })` |
| `choice!{…}` | `id` (required), `render` | arms `t![…] => Variant`, `t![…] if guard => Variant` | `push_step(Choice { … })` |
| `t![…]` | — | text items, juxtaposed | `concat([…])` of `ruby`, `mono_ruby`, `effect`, `to_text` calls |
| `column![…]`, `row![…]` | — | any number | `ui::column([…])`, `ui::row([…])` |
| `layer![…]` | — | any number | `ui::layer([…])` |
| `text!(…)` | `style` | exactly one | `ui::text(TextProps { … })` |
| `image!(…)` | `fit` | exactly one | `ui::image(ImageProps { … })` |
| `game_screen!(…)` | `game` (required) | none | `ui::game_screen(GameScreenProps { … })` |
| `button!{…}` | `on_click`, `disabled`, `style` | any number | `ui::button(ButtonProps { … })` |
| `slider!{…}` | `value` (required), `on_change`, `step`, `disabled`, `style` | none | `ui::slider(SliderProps { … })` |
| `toggle!{…}` | `value` (required), `on_change`, `disabled`, `style` | any number | `ui::toggle(ToggleProps { … })` |
| `select!{…}` | `value` (required), `on_change`, `disabled`, `style` | any number | `ui::select(SelectProps { … })` |

## `s!(…)` {#s}

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
s!(who = "Azusa", history = t!["(Azusa names the date)"], t!["Today is day " {DAY} "."]);
```

| Attribute | Type | Meaning |
| --------- | ---- | ------- |
| `who` | `Text` | The speaker name. Without it, the message is narration |
| `voice` | `VoiceAsset` | The voice line played with the message |
| `render` | `Render` | How the screen changes when the step is entered |
| `history` | `Text` | The text the backlog records instead of the message |

All attributes are optional. The element, if any, must be written as a `t![…]` form (`E0001`
otherwise), and it becomes the field `text`. To show a `Text` held in a constant, interpolate it:
`t![{GREETING}]`.

Each field is `Some(value)` when the attribute or element is present and `None` otherwise, and the
whole form is a call of `push_step`, so

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
```

means

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: None,
    render: Some(azusa_smiles),
    text: Some("Did you forget again?"),
    history: None,
});
```

(`t!["Did you forget again?"]` is the string itself; see [`t![…]`](#t).) The expression has type
`()`.

## `choice!{…}` {#choice}

```kix
enum Confess { Yes, No }

let answer = choice!{
    id = "confess",
    render = azusa_waits,
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] if !MET_MIO => Confess::No,
};
```

| Attribute | Type | Required | Rule |
| --------- | ---- | -------- | ---- |
| `id` | string literal | yes | Missing or not a string literal: `E0001`. Used twice in one function: `E3019` |
| `render` | `Render` | no | As for `s!` |

Every element is an **arm**: `t![…] => Variant` or `t![…] if guard => Variant`.

- The text must be written as a `t![…]` form; a plain string literal is `E0001`.
- The guard is a `bool` expression, evaluated when the choice is entered. An arm whose guard is
  false is not offered.
- The value must be a fieldless variant of an enum (`E3020`). All arms use variants of one enum
  (`E2001` otherwise), and each variant at most once (`E3020`). The `choice!` expression has that
  enum type.

The form desugars to a `Choice` with one `ChoiceArm` per arm, in source order; an arm without a
guard has `shown: true`. The example above means

```kis
let answer = push_step(Choice {
    id: "confess",
    render: Some(azusa_waits),
    arms: [
        ChoiceArm { text: "Tell her the truth", shown: true, value: Confess::Yes },
        ChoiceArm { text: "Say nothing", shown: !MET_MIO, value: Confess::No },
    ],
});
```

A choice that can be reached with every arm hidden is `E5003`. Guards, disabled arms and stable
choice ids are explained in [Steps](../concepts/steps.md#choice-choice-step).

## `t![…]` {#t}

```kix
t!["これは" ["数多"]("あまた") "の" ["漢字"]("かん"|"じ") !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

`t![…]` builds a `Text`. Its items are juxtaposed, without commas, and are concatenated exactly:
no whitespace is added or removed between them.

| Item | Meaning | Desugars to |
| ---- | ------- | ----------- |
| `"…"` | Plain text | The string literal |
| `["数多"]("あまた")` | Ruby: one reading over the whole base (group ruby) | `ruby("数多", "あまた")` |
| `["漢字"]("かん"\|"じ")` | Ruby: one reading per character of the base (mono ruby) | `mono_ruby("漢字", ["かん", "じ"])` |
| `!["赤い"](e)` | The base shown with the text effect `e` | `effect("赤い", e)` |
| `![["数多"]("あまた")](e)` | A ruby with a text effect | `effect(ruby("数多", "あまた"), e)` |
| `{expr}` | The value of `expr` as text | `to_text(expr)` |

The whole form is

- `""` when it has no items;
- the single item's expression when it has one item, so `t!["Mio"]` is `"Mio"` and `t![{DAY}]` is
  `to_text(DAY)`;
- `concat([…])` of the items otherwise.

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
```

means

```kis
concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

`ruby`, `mono_ruby`, `effect`, `concat` and `to_text` are `const fn` builtins of
[`std::narrative::text`](../../std/narrative-text.md).

### Strings

A string item is a string literal with the escapes `\n` (line break), `\r`, `\t`, `\\`, `\"`, `\0`
and `\u{…}`. A line break typed inside the quotes is part of the text as well. A string literal
has type `Text` anywhere, so an attribute such as `who = "Mio"` needs no `t!`.

### Ruby

`[base](readings)` puts ruby over `base`, which is one string literal.

- One reading: group ruby over the whole base.
- Several readings separated by `|`: mono ruby, one reading per character of the base. The number
  of readings must equal the number of characters (`E3021`).

### Text effects

`![base](effect)` shows `base`, a string literal or a ruby item, with a **text effect**. `effect`
is an expression of type `TextEffect` and must be a [constant expression](./constants.md)
(`E3022`), so a message is still a constant of its step. Effects combine with `.and(…)`; fields
set on the right win. Interpolation is not allowed inside the base.

```kix
t!["Tell me about " !["you"](emphasis()) "."]
t!["It was " !["here"](dots().and(color_text(rgb(200, 0, 0)))) "."]
```

The effects (`color_text`, `bold`, `italic`, `size`, `font`, `outline`, `shadow`, `emphasis`,
`dots`, `speed`, `speed_scale`, `reveal_instant`, `pause`, `click_wait`, `mark`) are listed in the
STD Reference, [`std::narrative::text`](../../std/narrative-text.md).

### Pauses and marks

A **cue** is an effect that acts on the reveal of the text rather than on its look. Its base is
usually the empty string:

```kix
s!(who = "Mio", render = mio_startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
s!(t!["Read slowly." ![""](click_wait()) "\nThen go on."]);
```

| Cue | Effect |
| --- | ------ |
| `pause(d)` | The reveal stops for `d` before the base |
| `click_wait()` | The reveal stops until a click; the click continues the text in the same step |
| `mark(n)` | Names the moment the reveal reaches the base, so a render can schedule an animation there |

A mark number used twice in one text is `E4010`. The exact reveal, pause and mark rules are in the
STD Reference, [`std::narrative::text`](../../std/narrative-text.md).

### Interpolation

`{expr}` inserts the value of `expr` as text. The type of `expr` must implement the standard trait
`ToText` (`E2026` otherwise): `Text`, `String`, `Int<lo, hi>`, the Rust integers, `Rat` and
`Decimal`. In a `pushdown fn`, interpolating a finite runtime value such as a story variable is a
call of the `const fn` `to_text` with a finite argument, which the compiler
[tabulates](../concepts/kinds.md#calling-const-fn-from-the-story).

## UI forms

```kix
column![
    text!(t!["Text speed: " {speed()}]),
    slider!{value = speed(), on_change = |v| set_speed(v)},
    button!{on_click = || nav::back(), t!["Back"]},
]
```

The UI forms build `Element` values of [`std::ui`](../../std/ui.md) in `fn` code.
They desugar by these rules:

- **Elements.** Each element `e` becomes the closure `|| e`. In `column!`, `row!`, `button!` and
  `toggle!`, an element that is written as a `t![…]` form becomes `|| ui::label(t![…])`.
- **Reactive attributes.** Every attribute whose name does not start with `on_` becomes a closure:
  `key: || expr` when the attribute is required, `key: Some(|| expr)` when it is optional. The UI
  re-runs the closure when a signal it reads changes.
- **Handler attributes.** An `on_*` attribute is passed as written: `key: Some(expr)`.
- **Absent optional attributes** become `None`.
- `column!`, `row!` and `layer!` pass the array of element closures directly. The other forms pass
  a props struct whose fields are the attributes, in the order declared in `std::ui`, plus a field
  for the elements.

| Form | Target | Attributes and their types | Elements |
| ---- | ------ | -------------------------- | -------- |
| `column![…]`, `row![…]` | `ui::column(children)`, `ui::row(children)` | none | `Element`; any number |
| `layer![…]` | `ui::layer(children)` | none | `Placed`; any number; `t![…]` is not wrapped |
| `text!(…)` | `ui::text(TextProps { … })` | `style`: `TextStyle` | exactly one `Text` → `content` |
| `image!(…)` | `ui::image(ImageProps { … })` | `fit`: `Fit` | exactly one `ImageAsset` → `source` |
| `game_screen!(…)` | `ui::game_screen(GameScreenProps { … })` | `game`: `Narrative`, required | none |
| `button!{…}` | `ui::button(ButtonProps { … })` | `on_click`: `Fn()`; `disabled`: `bool`; `style`: `ButtonStyle` | `Element`s → `children` |
| `slider!{…}` | `ui::slider(SliderProps { … })` | `value`: `Int<LO, HI>`, required; `on_change`: `Fn(Int<LO, HI>)`; `step`: `u32`; `disabled`: `bool`; `style`: `SliderStyle` | none |
| `toggle!{…}` | `ui::toggle(ToggleProps { … })` | `value`: `bool`, required; `on_change`: `Fn(bool)`; `disabled`: `bool`; `style`: `ButtonStyle` | `Element`s → `children` |
| `select!{…}` | `ui::select(SelectProps { … })` | `value`: `T`, required; `on_change`: `Fn(T)`; `disabled`: `bool`; `style`: `ButtonStyle` | `ui::option(value, label)` items → `options`; `t![…]` is not wrapped |

So

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

means

```kis
std::ui::button(std::ui::ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| std::ui::label("Back")],
})
```

and

```kix
slider!{value = speed(), on_change = |v| set_speed(v)}
```

means

```kis
std::ui::slider(std::ui::SliderProps {
    value: || speed(),
    on_change: Some(|v| set_speed(v)),
    step: None,
    disabled: None,
    style: None,
})
```

A slider's range is the range of its value's `Int` type; there are no `min` or `max` attributes.

## Writing forms by hand

A `.kis` file, or a `.kix` file that prefers it, writes the desugared code directly. Every target
is an ordinary public item: `push_step`, `Say`, `Choice` and `ChoiceArm` are in the prelude, the
text builtins are in `std::narrative::text`, and the UI functions and props structs are in
`std::ui`. Hand-written code gets the same checks as the forms: a hand-written `Choice` must use a
string literal `id` (`E3019`) and an array literal of `ChoiceArm { … }` records with fieldless
variants (`E3020`).
