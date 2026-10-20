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

## The screen is an ECS world

`Screen` follows the entity–component–system pattern:

| Part       | Defined in            | Role                                                                 |
| ---------- | --------------------- | -------------------------------------------------------------------- |
| Entities   | `.kirp`               | Spawned, changed and despawned by render functions                   |
| Components | `.kix`                | Data attached to entities: position, appearance, animation, and so on |
| Systems    | The engine            | Process entities at runtime, for example advancing animations frame by frame |

Every component type must implement `Eq`, so two screens can always be compared. Systems are not
written in any Kagari source file and are not part of the logical screen: the screen a step
shows is the set of entities and their components after `render`, not the frames the engine
draws while an animation plays.

## Walkthrough

Assume these definitions from `.kix` files:

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

A pending change that is discarded is reported as the warning `type/unapplied-change`.
`LayerImageChange` is marked [`#[must_use]`](./kix.md#attributes), and a statement whose value
has a `#[must_use]` type, such as `screen.background().change(BG::ROOF);`, is such a discard.

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
- **Blind to the script.** `render` sees only the `Screen` and constants, never script
  variables. Presentation that depends on story state is written as different `<r>` blocks in
  the arms of a `<match>`.

The script, in turn, never reads the screen. Story state determines presentation, never the
other way around.

Because a render reads only constants and its input screen, and every `.kix` function it calls
has a body the compiler can evaluate, each render is a `Screen -> Screen` function fully known at
compile time.

## Keyframes

A render whose output does not depend on its input screen is a **keyframe**: it declares the
complete screen, for example by despawning every entity and spawning the ones the step needs.
The compiler detects keyframes itself; no annotation is required.

*Tentative:* `Screen::empty()` returns a screen with no entities, so a keyframe can be written
explicitly:

```kirp
fn render(_screen: Screen) -> Screen {
    let screen = Screen::empty();
    screen.background().change(BG::ROOF).apply_animation(fade(30));
    screen
}
```

Keyframes bound how far back the engine must replay renders when it loads a save on a changed
script or jumps to a step it has no snapshot for: it replays the narrative to the target, then
runs renders only from the last keyframe on that path. They are recommended at choice points and
chapter starts. Interrupt scenes, such as a phone call that can happen anywhere, should *not*
start with a keyframe: they are meant to overlay whatever screen they interrupt.

In the terms of the engine proposal, a keyframe is its keyframe and any other render is a delta.

## Consistency

A render function transforms the *previous* screen, so the screen at a step depends on how the
step was reached:

```xml
<choice id="where">
  <choice-arm id="roof" text="Go to the roof">
    <s><r>screen.background().change(BG::ROOF).apply_animation(fade(30));</r><m who="">The wind is cold.</m></s>
  </choice-arm>
  <choice-arm id="classroom" text="Stay in the classroom">
    <s><r>screen.background().change(BG::CLASSROOM).apply_animation(fade(30));</r><m who="">It's quiet.</m></s>
  </choice-arm>
</choice>
<s><m who="Azusa">So, about tomorrow…</m></s>
```

{{#include diagrams/consistency.svg}}

The last step shows the roof on one path and the classroom on the other. If that is intended, it
is fine. If it is a leftover from a branch, it is exactly the kind of bug that is invisible to
everyone but the author.

Compilation stage 4 computes, for each step, the set of screens that can reach it, and reports
steps where that set has more than one element, together with the paths that cause it. This is
the presentation-ambiguity lint from the proposal. The analysis runs per entity and component:
for each step, the possible values of each component. Each scene is summarized once as the set
of screen changes it can make between its entry and its exit, so calls do not re-analyze the
callee.

The author fixes a report in one of two ways:

- **Insert a keyframe** after the merge, if the difference is a leftover.
- **Promote the dependency into a variable**, if it is intended: set a variable in each branch
  and represent the merged step differently in the arms of a `<match>` on it. The analyzer then
  sees the dependency as story state.

## Unbounded screens

A render may read a component from its input screen and compute a new value from it, for example
moving an entity 10 units to the right of where it is. Inside a loop, such as a hub the player
returns to, that can produce a different screen on every pass, so the set of screens at a step
has no bound and the consistency check could not finish.

Compilation stage 4 rejects such a program. The render that causes the growth is reported as an
error, and error recovery leaves it out so that the rest of the program is still checked; the
program does not compile until it is fixed.

The check must be sound, so it rejects any render whose screens it cannot prove bounded:

- Constant offsets along `<goto>` loops and `<call>` paths are analyzed exactly: the reachable
  offsets form a semilinear set, and whether that set is finite is decidable.
- For other arithmetic the check over-approximates. A render that is bounded only because it
  clamps or wraps a value is accepted when the analysis understands the clamp; otherwise it is
  rejected. Which operations the analysis understands is part of the
  [language subset](./open-questions.md#kirp-language-subset).
