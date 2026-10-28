# Steps

A **step** is a point where the story stops and waits for the player. There are two kinds of
step: an **ordinary step**, written `s!(…)`, and a **choice**, written `choice!{…}`. Their text is
written `t![…]`. These three are **forms**: fixed syntax, allowed only in `.kix` files, that
desugars to ordinary struct expressions and calls. The exact syntax of forms (delimiters,
attributes, elements) is in [Forms](../syntax/forms.md).

Only a [scene](./scenes.md#scenes-and-instant-procedures) may run a step (`E3002`).

## Steps are calls of `push_step` {#steps-are-calls-of-push_step}

There is one primitive that suspends the story, in `std::narrative`:

```kis
#[builtin] pub trait Step { type Output; }

#[builtin]
pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`push_step` shows the step, waits for input, and returns what the input carried. It is not pure:
it suspends the story. Two standard types implement `Step`:

| Type | Written as | `Output` |
| ---- | ---------- | -------- |
| `Say` | `s!(…)` | `()`: `NextStep` carries no information |
| `Choice<A, N>` | `choice!{…}` | `A`: the arm the player selected |

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
    history: None,
});
```

The struct form is ordinary code and may be written by hand; it is how a `.kis` file writes a
step. The full declarations are in the STD Reference,
[`std::narrative`](../../../../std/en/src/narrative.md).

## `s!`: ordinary step {#s-ordinary-step}

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
```

`s!` shows one message and waits for `NextStep`. Its attributes are the fields of `Say`:

| Attribute | Type | Meaning |
| --------- | ---- | ------- |
| `who` | `Text` | The speaker name as display text. Without `who`, the message is narration |
| `voice` | `VoiceAsset` | The voice line played with the message |
| `render` | `Render` | How the screen changes when the step is entered; see [Presentation](./presentation.md) |
| `history` | `Text` | The text the backlog records instead of the message text |

All attributes are optional; an absent attribute is `None`. The element is at most one `t![…]`,
the message text. An `s!` without text shows no message and still waits for input, for example
after a render that shows a CG. A step without `render` leaves the screen unchanged.

A step has **one speaker**. A conversation is a sequence of steps.

By default the backlog records the line with its interpolated values as they were when it was
shown. `history = t![…]` replaces the recorded text:

```kix
s!(who = "Azusa", history = t!["(Azusa names the date)"], t!["Today is day " {DAY} "."]);
```

`NextStep` carries no information, so save traces do not record it; only choices are recorded.

## Text and text effects

`t![…]` builds a value of type `Text` from juxtaposed items: plain strings, ruby
(`["数多"]("あまた")`), text with a **text effect** (`!["赤い"](color_text(rgb(200, 0, 0)))`) and
interpolated values (`{expr}`):

```kix
t!["これは" ["数多"]("あまた") "の" !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

A string literal is already a `Text`, so `who = "Mio"` needs no `t!`. A text effect is a constant
evaluated at compile time, so a message stays a constant of its step. Effects include styles
(colour, bold, emphasis, …), reveal speed, and cues inside a step: a timed pause, a click wait
(a click that continues the text inside the same step and is not recorded in the save trace), and
marks that a render can schedule animations at. In the story, `{expr}` of a finite runtime value
is [tabulated](./kinds.md#calling-const-fn-from-the-story) like any other `const fn` call.

The item syntax and the rules are in [Forms](../syntax/forms.md); every effect is listed in the
STD Reference, [`std::narrative::text`](../../../../std/en/src/narrative-text.md).

## `choice!`: choice step {#choice-choice-step}

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
| `id` | string literal | yes | [Stable identifier](#stable-choice-ids), unique within the function |
| `render` | `Render` | no | Runs when the choice is entered, as for `s!` |

Each element is an **arm**, `t![…] => Variant`, optionally with a guard,
`t![…] if condition => Variant`:

- The text is the option shown to the player.
- All arms of one choice use variants of the **same enum**, declared by the author, and each
  variant at most once. The variants must be fieldless (`E3020`). The choice has that enum type,
  so `match` on it is checked for exhaustiveness like any other `match`. A variant without an arm
  still has to be handled, or covered with `_`.

`choice!{…}` is `push_step(Choice { … })`: each arm becomes a `ChoiceArm { text, shown, value }`
whose `shown` is its guard, or `true` without one.

### Guards

```kix
t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
```

An arm with a **guard** is not shown unless its condition is true when the choice is entered. The
compiler reports a `choice!` that can be reached in a state where no arm is shown (`E5003`), since
the player would be stuck.

### Disabled arms

An option shown greyed out, such as a destination that is still locked, is not an arm of
`choice!`: it has no value and cannot be selected. It is presentation, added by the choice's
render to the screen's `ChoiceMenu`:

```kix
match choice!{
    id = "map",
    render = if VISITED_A { show_map } else { show_map_locked },
    t!["Stall A"] => Map::A,
    t!["Stall B"] if VISITED_A => Map::B,
} { … }

const fn show_map_locked(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Stall B"]);
    screen
}
```

`disabled_arm(at, text)` inserts a display-only option at position `at` of the shown list,
clamped to its length. The `ChoiceMenu` belongs to the step only: it is emptied when the next
step is entered. Because a disabled arm is not part of `choice!`, no analysis sees it; the guard
alone decides what the player can select. See the STD Reference,
[`std::screen`](../../../../std/en/src/screen.md).

### Stable choice IDs

A save records each selection as the function path, the choice `id` and the variant name, for
example `story::Prologue/confess/Confess::Yes`. Replay supplies the call context, so a choice `id`
only has to be unique within its function, even when the function is called from many places or
runs in a loop. The `id` must be a string literal, and an `id` used twice in one function is
`E3019`.

Because the ID does not depend on line numbers or arm order, saves survive reordering and
rewording of the script. Renaming the function, the choice `id` or the variant breaks the saves
that pass through it: loading such a save reports a divergence at that choice. See
[Saves, Replay and Determinism](./saves.md).
