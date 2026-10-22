# Steps and Elements

A **step** is a point where the story stops and waits for the player. There are two kinds of
step: `<s>`, an ordinary step, and `<choice>`. In a `.kix` file they are written as elements.

## Elements are sugar

There is one primitive that suspends the story, in `std::narrative`:

```kis
pub trait Step { type Output; }

#[builtin]
pub pushdown fn step<S: Step>(s: S) -> S::Output;
```

`step` shows the step, waits for input, and returns what the input carried. Two standard types
implement `Step`:

| Type | Written as | `Output` |
| ---- | ---------- | -------- |
| `Say` | `<s>` | `()`: `NextStep` carries no information |
| `Choice<A>` | `<choice>` | `A`: the arm the player selected |

In a `pushdown fn`, an element expression is a call of `step` on a value built from the element:

- The **outermost** element of an element expression is passed to `step`.
- Elements nested inside it are not steps. They are data: props and children of the outer one.

So

```kix
<s render={hanako_jumps}><m who="Azusa" voice={AZUSA_0072}>ぬっ</m></s>;
```

means

```kis
step(Say {
    render: Some(hanako_jumps),
    messages: [Message { who: Some("Azusa"), voice: Some(AZUSA_0072), text: "ぬっ" }],
});
```

The struct form is ordinary code and may be written by hand, also in a `.kis` file. A tag whose
name starts with a capital letter is not a step but a [scene call](./scenes.md#scene-tags).

An element is an expression: `<s>…</s>` has type `()` and `<choice>…</choice>` the type of its
arms. As a statement it ends with `;`, except where Rust allows a block-like expression without
one, such as a `match` arm.

## `<s>`: ordinary step

```kix
<s>It was the last day of summer.</s>;

<s render={azusa_smiles}>
    <m who="Azusa">Did you forget again?</m>
</s>;

<s>
    Azusa glances at the clock.
    <m who="Azusa">We're late.</m>
</s>;
```

An `<s>` shows its messages, in order, and waits for `NextStep`. Its children are:

- **Text**, which is a narration message: a message without a speaker.
- **`<m>`** elements, each a message with a speaker.

| Prop | Type | Required | Meaning |
| ---- | ---- | -------- | ------- |
| `render` | `Render` | no | How the screen changes when the step is entered; see [Presentation](../presentation.md) |

An empty `<s />` shows no message and still waits for input, for example after a render that
hides the text frame to show a CG. A step without `render` leaves the screen unchanged.

`NextStep` carries no information, so save traces do not record it; only choices are recorded.

## `<m>`: message

```kix
<m who="Azusa" voice={AZUSA_0072}>ぬっ</m>
```

`<m>` is one message of an `<s>`: who says it, the text, and an optional voice line. It may only
appear directly inside an `<s>`, and its children are text only.

| Prop | Type | Required | Meaning |
| ---- | ---- | -------- | ------- |
| `who` | `Text` | no | The speaker name as display text. Without `who`, the message is narration |
| `voice` | `VoiceAsset` | no | The voice line played with the message |

## Text

Text children follow the JSX rules, adapted for Japanese (*tentative*, see [Open
Questions](../open-questions.md#text-whitespace)):

- Each line is trimmed, and lines that are empty after trimming are dropped.
- The remaining lines are joined with one space, except that no space is inserted when the
  characters on both sides of the join are CJK.
- `{` and `<` start an expression or an element. To write them as text, use a string expression:
  `{"<"}`.
- A comment is written `{/* … */}`.

*Tentative.* `{expr}` inside message text interpolates a value of type `Text` or `Int`, for example
a name chosen by the story. See [Open Questions](../open-questions.md#text-interpolation) for how
the backlog records such text.

Further markup (ruby, emphasis, explicit line breaks) is not designed yet; see [Open
Questions](../open-questions.md#text-markup).

## `<choice>` and `<arm>`: choice step

```kix
enum Confess { Yes, No }

let answer = <choice id="confess">
    <arm id={Confess::Yes}>Tell her the truth</arm>
    <arm id={Confess::No}>Say nothing</arm>
</choice>;

match answer {
    Confess::Yes => <AzusaConfession />,
    Confess::No => <s>I kept quiet.</s>,
}
```

`<choice>` is a single step. The engine shows one option per offered arm and waits for the player
to select one. The choice then evaluates to that arm's `id`, and ordinary code branches on it.
Arms are data, not blocks of code: what happens after the selection is written after the choice.

| Element | Prop | Type | Required | Meaning |
| ------- | ---- | ---- | -------- | ------- |
| `<choice>` | `id` | string literal | yes | Stable identifier, unique within the function |
| `<choice>` | `render` | `Render` | no | Runs when the choice is entered, as for `<s>` |
| `<arm>` | `id` | an enum variant | yes | The value the choice returns when this arm is selected |
| `<arm>` | `when` | `bool` | no | The arm is offered only when this is `true`; default `true` |

- The children of `<choice>` are `<arm>` elements only. The text child of an `<arm>` is the option
  text.
- All arms of one choice use variants of the **same enum**, declared by the author, and each
  variant at most once. The choice has that enum type, so `match` on it is checked for
  exhaustiveness like any other `match`. A variant without an arm still has to be handled, or
  covered with `_`.
- The variants must be fieldless.

### Conditional arms

```kix
<arm id={Map::Leave} when={VISITED_A && VISITED_B && VISITED_C}>Leave</arm>
```

`when` is evaluated when the choice is entered. The compiler reports a `<choice>` that can be
reached in a state where no arm is offered, since the player would be stuck.

### Stable choice IDs

A save records each selection as the function path, the choice `id` and the variant name, for
example `story::Prologue/confess/Confess::Yes`. Replay supplies the call context, so a choice `id`
only has to be unique within its function, even when the function is called from many places or
runs in a loop.

Because the ID does not depend on line numbers or arm order, saves survive reordering and
rewording of the script. Renaming the function, the choice `id` or the variant breaks the saves
that pass through it: loading such a save reports a divergence at that choice.
