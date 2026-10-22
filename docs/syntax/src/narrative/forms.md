# Steps and Forms

A **step** is a point where the story stops and waits for the player. There are two kinds of
step: an ordinary step, written `s!(…)`, and a choice, written `choice!{…}`. Their text is written
`t![…]`. These three are **forms**: fixed syntax, allowed only in `.kix` files, that desugars to
ordinary struct expressions and calls.

Forms are not macros. There are no macros in Kagari; the set of forms is fixed by the language
(the narrative forms of this chapter and the [UI forms](../system.md#components-and-signals)).

## Delimiters

Each form has exactly one delimiter, and the delimiter says what it accepts:

| Delimiter | Accepts | Example |
| --------- | ------- | ------- |
| `name!( … )` | Attributes, then a fixed number of elements | `s!(who = "Mio", t!["…"])`: at most one `t![]` |
| `name![ … ]` | Elements only, any number; no attributes | `t!["…" ["数多"]("あまた")]` |
| `name!{ … }` | Attributes, then any number of elements | `choice!{ id = "map", t!["…"] => Map::A, … }` |

- An attribute is `key = expression`. Attributes always come before elements; optional attributes
  may be omitted.
- Attributes and elements are separated by commas, except the items of `t![…]`, which are
  juxtaposed.
- `key = value` is allowed only inside forms. Outside them, `=` is ordinary assignment.

## Steps are calls of `push_step`

There is one primitive that suspends the story, in `std::narrative`:

```kis
pub trait Step { type Output; }

#[builtin]
pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`push_step` shows the step, waits for input, and returns what the input carried. It is not pure:
it suspends the story. Only a [scene](./scenes.md#scenes-and-instant-procedures) may call it.
Two standard types implement `Step`:

| Type | Written as | `Output` |
| ---- | ---------- | -------- |
| `Say` | `s!(…)` | `()`: `NextStep` carries no information |
| `Choice<A>` | `choice!{…}` | `A`: the arm the player selected |

So

```kix
s!(who = "Azusa", voice = AZUSA_0072, render = hanako_jumps, t!["ぬっ"]);
```

means

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: Some(AZUSA_0072),
    render: Some(hanako_jumps),
    text: Some(t!["ぬっ"]),
});
```

The struct form is ordinary code and may be written by hand; it is how a `.kis` file writes a
step.

## `s!`: ordinary step

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
```

`s!` shows one message and waits for `NextStep`.

| Attribute | Type | Meaning |
| --------- | ---- | ------- |
| `who` | `Text` | The speaker name as display text. Without `who`, the message is narration |
| `voice` | `VoiceAsset` | The voice line played with the message |
| `render` | `Render` | How the screen changes when the step is entered; see [Presentation](../presentation.md) |

All attributes are optional. The element is at most one `t![…]`, the message text. An `s!` without
text shows no message and still waits for input, for example after a render that shows a CG. A
step without `render` leaves the screen unchanged.

A step has **one speaker**. A conversation is a sequence of steps.

`NextStep` carries no information, so save traces do not record it; only choices are recorded.

## `t!`: text

```kix
t!["これは" ["数多"]("あまた") "の" !["赤い"](color_text(RED)) "花。"]
```

`t![…]` builds a value of type `Text`. Its items are juxtaposed and concatenated directly:

| Item | Meaning |
| ---- | ------- |
| `"…"` | Plain text. Rust string escapes apply: `\"`, `\\`, `\n` for a line break |
| `["数多"]("あまた")` | Ruby over the whole base text (group ruby) |
| `["漢字"]("かん"\|"じ")` | Ruby per character (mono ruby); the number of readings must equal the number of base characters |
| `!["赤い"](color_text(RED))` | Text shown with a text effect; see below |
| `{expr}` | *Tentative:* the value of a `Text` or `Int` expression; see [Open Questions](../open-questions.md#text-interpolation) |

- Text is exactly the content of its strings. No whitespace is added or folded; a line break is
  `\n`.
- The base of a ruby is a string literal. The base of a text effect is a string literal or a ruby
  item.
- A plain string literal is also a `Text`: `who = "Mio"` needs no `t!`.

### Text effects

`!["赤い"](color_text(RED))` shows its base with a **text effect**. The target in parentheses is a
constant expression of type `TextEffect` (*tentative* name): a value describing how the text is
rendered, not a function. It is evaluated at compile time, so a message is still a constant of its
step, and the backlog, snapshots and the [consistency check](../presentation.md#consistency) treat
it like any other text.

There is no separate markup for emphasis; emphasis is an effect from the standard library:

```kix
t!["Tell me about " !["you"](emphasis()) "."]
```

## `choice!`: choice step

```kix
enum Confess { Yes, No }

let answer = choice!{
    id = "confess",
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] => Confess::No,
};

match answer {
    Confess::Yes => AzusaConfession(),
    Confess::No => s!(t!["I kept quiet."]),
}
```

`choice!` is a single step. The engine shows one option per offered arm and waits for the player
to select one. The choice then evaluates to that arm's variant, and ordinary code branches on it.
Arms are data, not blocks of code: what happens after the selection is written after the choice.

| Attribute | Type | Required | Meaning |
| --------- | ---- | -------- | ------- |
| `id` | string literal | yes | Stable identifier, unique within the function |
| `render` | `Render` | no | Runs when the choice is entered, as for `s!` |

Each element is an arm, `t![…] => Variant`, optionally with a guard, `t![…] if condition =>
Variant`:

- The text is the option shown to the player.
- All arms of one choice use variants of the **same enum**, declared by the author, and each
  variant at most once. The variants must be fieldless. The choice has that enum type, so `match`
  on it is checked for exhaustiveness like any other `match`. A variant without an arm still has
  to be handled, or covered with `_`.

### Guards

```kix
t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
```

An arm with a guard is not shown unless its condition is true when the choice is entered. The
compiler reports a `choice!` that can be reached in a state where no arm is shown, since the player
would be stuck.

### Stable choice IDs

A save records each selection as the function path, the choice `id` and the variant name, for
example `story::Prologue/confess/Confess::Yes`. Replay supplies the call context, so a choice `id`
only has to be unique within its function, even when the function is called from many places or
runs in a loop.

Because the ID does not depend on line numbers or arm order, saves survive reordering and
rewording of the script. Renaming the function, the choice `id` or the variant breaks the saves
that pass through it: loading such a save reports a divergence at that choice.
