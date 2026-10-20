# Steps and Labels

A step is a point where the engine stops and waits for the player. There are two kinds: `<s>`
and `<choice>`. `<label>` names a position between steps.

## `<s>`: ordinary step

```xml
<s>
    <m who="">It was the last day of summer.</m>
</s>

<s>
<r>screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());</r>
<m who="Azusa">Did you forget again?</m>
</s>
```

An `<s>` shows the messages of its [`<m>`](#m-message) children, in document order. Optional
`<r>` and `<represent>` children describe how the screen changes when the step is entered (see
[Presentation Tags](./presentation.md)).

Text directly inside `<s>`, outside any `<m>`, is a comment: it is allowed, never shown, and
ignored by the compiler. Everything the player reads is inside an `<m>`.

```xml
<s>
    Azusa glances at the clock. (A note for the writers; never shown.)
    <m who="Azusa">We're late.</m>
</s>
```

An `<s>` without `<m>` shows no message. It still waits for input, for example after a render
that hides the text frame to show a CG.

When the engine receives the `NextStep` input while on an `<s>`, the step counter advances by one:
execution continues with whatever follows the `<s>`, running instant elements until the next
step.

`NextStep` carries no information, so save traces do not need to record it; only choice
selections are recorded.

## `<m>`: message

```xml
<s>
    <m who="Azusa" voice="AZUSA_0072">ぬっ</m>
</s>
<s>
<m who="">The wind stopped.</m>
</s>
```

`<m>` is one message of an `<s>`: who says it, the text, and an optional voice line. It may only
appear directly inside an `<s>`, and its content is text only.

| Attribute | Required | Meaning                                                                                                             |
|-----------|----------|---------------------------------------------------------------------------------------------------------------------|
| `who`     | yes      | The speaker name, as plain display text. An empty `who` is narration: no name is shown                              |
| `voice`   | no       | A constant of type `VoiceAsset` from an imported `.kix` file, as a path such as `AZUSA_0072` or `VOICE::AZUSA_0072` |

The attribute is `who`, not `char`, because `char` is already the unsigned 8-bit
[variable type](./data.md#variable-declarations).

A `voice` that does not name a `VoiceAsset` constant is an error in
compilation stage 3.

*Tentative:* further text markup inside `<m>` (ruby, line breaks, emphasis) is not designed yet;
see [Open Questions](../open-questions.md#text-markup).

## `<choice>` and `<choice-arm>`: choice step

```xml
<choice id="confess">
    <choice-arm id="yes" text="Tell her the truth">
        <set var="route" val="Azusa"/>
        <call scene="azusa_confession"/>
    </choice-arm>
    <choice-arm id="no" text="Say nothing">
        <s>
            <m who="">I kept quiet.</m>
        </s>
    </choice-arm>
</choice>
```

`<choice>` is a single step. On entering it the engine shows one option per `<choice-arm>` and
waits for the player to select one. The selected arm's body runs, then execution continues after
`</choice>`.

| Element        | Attribute | Required | Meaning                                                  |
|----------------|-----------|----------|----------------------------------------------------------|
| `<choice>`     | `id`      | yes      | *Tentative.* Stable identifier, unique within the scene  |
| `<choice-arm>` | `id`      | yes      | *Tentative.* Stable identifier, unique within the choice |
| `<choice-arm>` | `text`    | yes      | *Tentative.* Option text shown to the player             |

The body of a `<choice-arm>` may contain anything a scene body may contain, including further
steps, or nothing at all.

The pair `scene/choice/arm` (for example `prologue/confess/yes`) is the stable choice ID that
save files record. Because it does not depend on line numbers or arm order, saves survive
reordering and rewording of the script.

`<choice>` may also contain `<r>` and `<represent>` children, which run when the choice is
entered, like those of `<s>`.

### Conditional arms

*Tentative.* An arm that is offered only under a condition is wrapped in a
[`<match>`](./control-flow.md#match-branch-on-variables) inside the `<choice>`. In this position
the bodies of the `<do>` elements may contain only `<choice-arm>` elements:

```xml
<choice id="map">
    <choice-arm id="a" text="Go to stall A">...</choice-arm>
    <match vars="visited_a visited_b visited_c">
        <cases>
            <case val="true true true"/>
            <do>
                <choice-arm id="leave" text="Leave"/>
            </do>
        </cases>
        <cases>
            <case val="_ _ _"/>
            <do/>
        </cases>
    </match>
</choice>
```

When the choice is entered, the engine offers the arms whose enclosing matches select them. Arm
ids stay unique within the whole `<choice>`, across all branches. The compiler reports a
`<choice>` that can be reached in a state where no arm is offered, since the player would be
stuck.

## `<label>`: position marker

```xml
<label id="map"/>
```

`<label>` names the position of the next step. It is not a step and does nothing when executed.
It exists only as a target for [`<goto>`](./control-flow.md#goto-jump-within-the-scene).

- Label ids are unique within a scene. Different scenes may reuse the same id.
- A label placed after the last step of a scene marks the scene's end; jumping there returns to
  the caller, like [`<return>`](./control-flow.md#return-leave-the-scene-early).
- A `<goto>` may only reach labels in its own block or an enclosing one (see
  [`<goto>`](./control-flow.md#goto-jump-within-the-scene)).
