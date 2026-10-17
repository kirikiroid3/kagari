# Steps and Labels

A step is a point where the engine stops and waits for the player. There are two kinds: `<s>`
and `<choice>`. `<label>` names a position between steps.

## `<s>`: ordinary step

```xml
<s>It was the last day of summer.</s>

<s>
  <r>screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());</r>
  "Did you forget again?"
</s>
```

The text content of `<s>` is what the step displays. Optional `<r>` and `<represent>` children
describe how the screen changes when the step is entered (see
[Presentation Tags](./presentation.md)).

When the engine receives the `NextStep` input while on an `<s>`, the step counter advances by one:
execution continues with whatever follows the `<s>`, running instant elements until the next
step.

`NextStep` carries no information, so save traces do not need to record it; only choice
selections are recorded.

*Tentative:* text markup inside `<s>` (speaker name, ruby, voice) is not designed yet; see
[Open Questions](../open-questions.md#text-markup).

## `<choice>` and `<choice-arm>`: choice step

```xml
<choice id="confess">
  <choice-arm id="yes" text="Tell her the truth">
    <set var="route" val="Azusa" />
    <call scene="azusa_confession" />
  </choice-arm>
  <choice-arm id="no" text="Say nothing">
    <s>I kept quiet.</s>
  </choice-arm>
</choice>
```

`<choice>` is a single step. On entering it the engine shows one option per `<choice-arm>` and
waits for the player to select one. The selected arm's body runs, then execution continues after
`</choice>`.

| Element        | Attribute | Required | Meaning                                                      |
| -------------- | --------- | -------- | ------------------------------------------------------------ |
| `<choice>`     | `id`      | yes      | *Tentative.* Stable identifier, unique within the scene      |
| `<choice-arm>` | `id`      | yes      | *Tentative.* Stable identifier, unique within the choice     |
| `<choice-arm>` | `text`    | yes      | *Tentative.* Option text shown to the player                 |

The body of a `<choice-arm>` may contain anything a scene body may contain, including further
steps, or nothing at all.

The pair `scene/choice/arm` (for example `prologue/confess/yes`) is the stable choice ID that
save files record. Because it does not depend on line numbers or arm order, saves survive
reordering and rewording of the script.

`<choice>` may also contain `<r>` and `<represent>` children, which run when the choice is
entered, like those of `<s>`.

There is no syntax yet for arms that are only available under a condition; see
[Open Questions](../open-questions.md#conditional-choice-arms).

## `<label>`: position marker

```xml
<label id="map" />
```

`<label>` names the position of the next step. It is not a step and does nothing when executed.
It exists only as a target for [`<goto>`](./control-flow.md#goto-jump-within-the-scene).

- Label ids are unique within a scene. Different scenes may reuse the same id.
- A label placed after the last step of a scene marks the scene's end; jumping there returns to
  the caller.
