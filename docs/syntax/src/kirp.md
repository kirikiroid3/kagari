# Representing Code (`.kirp`)

Representing code describes how the screen changes when a step is entered. It is written in a
Rust-like language whose vocabulary (types, methods, functions, constants) comes entirely from
imported [`.kix`](./kix.md) files.

## The `render` function

A `.kirp` file must contain the function

```kirp
fn render(screen: Screen) -> Screen {
    screen.entity(HANAKO).cord.apply_animation(jump(20));
    screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());
    screen
}
```

`render` receives the screen as it was before the step and returns the screen after it,
including the animations that lead from one to the other. A script uses it with
[`<represent src="…" />`](./kis/presentation.md#represent-representing-code-from-a-file);
[`<r>`](./kis/presentation.md#r-inline-representing-code) is the same function written inline,
with the signature and the final `screen` supplied by a hidden wrapper.

## Walkthrough

Assume these declarations from `.kix` files:

| Name                     | Kind     | Type                         |
| ------------------------ | -------- | ---------------------------- |
| `HANAKO`, `AZUSA`        | constant | `EntityId`                   |
| `AZUSA::CLOSE_EYE_SMILE` | constant | `ImageId`                    |
| `jump`                   | function | `fn(u64) -> CordAnimation`   |
| `instant_switch`         | function | `fn() -> TransitionAnimation` |

```kirp
screen.entity(HANAKO).cord.apply_animation(jump(20));
```

Selects the entity `HANAKO`, takes its coordinates (`cord`), and animates them with a jump of
height 20.

```kirp
screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());
```

1. `screen.entity(AZUSA)` selects the entity whose id is `AZUSA`, say `217`.
2. `.appearance.emotion` is a `Layer`. `.change(AZUSA::CLOSE_EYE_SMILE)` asks the state manager
   to change that layer to the image `AZUSA::CLOSE_EYE_SMILE`, an `ImageId`. The result is a
   `LayerImageChange`, a pending change.
3. Because this *changes* a `Layer`, it needs a `TransitionAnimation`. Without one the change
   does not happen.
4. `.apply_animation(instant_switch())` tells the `LayerImageChange` to switch the image without
   any animation, and the change takes effect.

## Changes need transitions

The rule in step 3 is general: changing what a layer shows produces a pending change, and only
`apply_animation` with a `TransitionAnimation` turns it into an actual change on the screen.
Every visible change therefore states how it is animated, even if the answer is "instantly".

Different properties take different animation types, so the type checker catches mismatches such
as passing a `CordAnimation` to a layer change:

| Target                  | Method                 | Animation type        |
| ----------------------- | ---------------------- | --------------------- |
| `Cord` (coordinates)    | `apply_animation`      | `CordAnimation`       |
| `LayerImageChange`      | `apply_animation`      | `TransitionAnimation` |

Whether a pending change that is never animated is a compile error or a warning is an
[open question](./open-questions.md#unapplied-changes).

## Language

The syntax follows Rust: statements end with `;`, the last expression of a block is its value,
`::` separates path segments, `.` accesses fields and calls methods. Only the subset needed to
build screens is intended; the exact subset is an
[open question](./open-questions.md#kirp-language-subset).

Representing code must be:

- **Total.** Every `render` terminates. Rendering is part of entering a step, and the engine
  must always reach the step.
- **Deterministic.** The same input screen always gives the same output screen. Saves and
  preview replay the story, and replay must reproduce the screen exactly.
- **Blind to the script.** `render` sees only the `Screen`, never script variables. Presentation
  that depends on story state is written as different `<r>` blocks in the arms of a `<match>`.

The script, in turn, never reads the screen. Story state determines presentation, never the
other way around.

## Consistency

A render function transforms the *previous* screen, so the screen at a step depends on how the
step was reached:

```xml
<choice id="where">
  <choice-arm id="roof" text="Go to the roof">
    <s><r>screen.background().change(BG::ROOF).apply_animation(fade(30));</r>The wind is cold.</s>
  </choice-arm>
  <choice-arm id="classroom" text="Stay in the classroom">
    <s><r>screen.background().change(BG::CLASSROOM).apply_animation(fade(30));</r>It's quiet.</s>
  </choice-arm>
</choice>
<s>"So, about tomorrow…"</s>
```

{{#include diagrams/consistency.svg}}

The last step shows the roof on one path and the classroom on the other. If that is intended, it
is fine. If it is a leftover from a branch, it is exactly the kind of bug that is invisible to
everyone but the author.

Compilation stage 4 computes, for each step, the set of screens that can reach it, and reports
steps where that set has more than one element, together with the paths that cause it. This is
the presentation-ambiguity lint from the proposal. The author fixes a report by setting the
screen explicitly after the merge; how to mark a dependency as intended is an
[open question](./open-questions.md#path-dependent-presentation).
