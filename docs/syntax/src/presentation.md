# Presentation

The story says *when* the screen changes; render functions say *how*.

## Render functions

```kis
type Render = const fn(Screen) -> Screen;   // in the prelude
```

A **render** is a `const fn` from the screen before a step to the screen after it, including the
animations that lead from one to the other. A step takes it as its `render` attribute:

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);

const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).cord.apply_animation(jump(20));
    screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());
    screen
}
```

When the story reaches the step, the engine runs the render on the current screen, shows the
result, and waits. A step without `render` leaves the screen unchanged.

### Inline renders

A closure that captures nothing coerces to a `const fn` pointer, so a short render can be written
in place:

```kix
s!(
    render = |mut screen| { screen.background().change(BG::ROOF).apply_animation(fade(30)); screen },
    t!["The wind on the roof is cold."]
);
```

The closure may use constants and call `const fn`, but it captures nothing: a closure that uses a
local or a story variable is not a `Render`.

### Composing renders

A step has one `render`. Several renders are combined by an ordinary `const fn`:

```kis
const fn roof_scene(screen: Screen) -> Screen {
    azusa_smiles(roof_background(screen))
}
```

## Walkthrough

```kis
screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());
```

1. `screen.entity(AZUSA)` selects the entity whose id is `AZUSA`, say `217`. It is a
   [projection](./values.md#references-are-second-class) of the screen: changing it changes the
   screen.
2. `.appearance.emotion` is a `Layer`. `.change(AZUSA::CLOSE_EYE_SMILE)` asks to change that layer
   to the image `AZUSA::CLOSE_EYE_SMILE`. The result is a `LayerImageChange`, a pending change: a
   projection of the layer that lives until the end of the statement.
3. Because this *changes* a layer, it needs a `TransitionAnimation`. Without one the change does
   not happen.
4. `.apply_animation(instant_switch())` switches the image without any animation, and the change
   takes effect.

## The screen is an ECS world

`Screen` follows the entity–component–system pattern:

| Part | Defined in | Role |
| ---- | ---------- | ---- |
| Entities | render functions | Spawned, changed and despawned by renders |
| Components | structs in `std::screen` and studio modules | Data attached to entities: position, appearance, animation |
| Systems | the engine | Process entities at runtime, for example advancing animations frame by frame |

Every component type has [structural equality](./language.md#types), so two screens can always be
compared. Systems are not written in Kagari and are not part of the logical screen: the screen a
step shows is the set of entities and their components after its render, not the frames the
engine draws while an animation plays.

A studio adds components, animations and helper functions as ordinary structs and `const fn` in
its own modules. The compiler type-checks renders against them like against the standard library.

## Changes need transitions

Changing what a layer shows produces a pending change, and only `apply_animation` with a
`TransitionAnimation` turns it into an actual change on the screen. Every visible change therefore
states how it is animated, even if the answer is "instantly".

Different properties take different animation types, so the type checker catches mismatches such
as passing a `CordAnimation` to a layer change:

| Target | Method | Animation type |
| ------ | ------ | -------------- |
| `Cord` (coordinates) | `apply_animation` | `CordAnimation` |
| `LayerImageChange` | `apply_animation` | `TransitionAnimation` |

`LayerImageChange` is `#[must_use]`, so a statement that ends with a change still pending, such as
`screen.background().change(BG::ROOF);`, is a warning.

## Renders are blind to the story

A render is a `const fn`, so it is:

- **Deterministic.** The same input screen always gives the same output screen. Saves and preview
  replay the story, and replay must reproduce the screen exactly.
- **Total where it runs.** The compiler evaluates every render on every screen that can reach its
  step (see [Consistency](#consistency)), within the evaluation budget. A render that does not
  finish is a compile error, so the engine always reaches the step.
- **Blind to the story.** Its only input is the screen. A `const fn` cannot read story variables,
  and a `render` attribute is a function pointer, so it cannot capture any either.

The story, in turn, never reads the screen: `Screen` is not a [finite
type](./kinds.md#finite-types), so no `pushdown fn` can hold one. Story state determines
presentation, never the other way around.

Presentation that depends on the story is chosen by the story:

```kix
match AFFECTION_MIO {
    3..=5 => s!(who = "Mio", render = mio_smiles, t!["You came."]),
    _ => s!(who = "Mio", render = mio_neutral, t!["Oh. It's you."]),
}
```

A render pointer is a finite value, so it may also be chosen by an expression,
`render = if MET_MIO { mio_smiles } else { mio_neutral }`, and stored in story state; the analyzer
sees the choice as part of σ either way.

## Keyframes

A render whose output does not depend on its input screen is a **keyframe**: it declares the
complete screen. The compiler detects keyframes itself; no annotation is required.

*Tentative:* `Screen::empty()` returns a screen with no entities, so a keyframe can be written
explicitly:

```kis
const fn roof_evening(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(BG::ROOF_EVENING).apply_animation(fade(30));
    screen
}
```

Keyframes bound how far back the engine must replay renders when it loads a save on a changed
script or jumps to a step it has no snapshot for: it replays the story to the target, then runs
renders only from the last keyframe on that path.

{{#include diagrams/keyframes.svg}}

Keyframes are recommended at choice points and chapter starts. Interrupt scenes, such as a phone
call that can happen anywhere, should *not* start with a keyframe: they are meant to overlay
whatever screen they interrupt.

In the terms of the engine proposal, a keyframe is its keyframe and any other render is a delta.

## Consistency

A render transforms the *previous* screen, so the screen at a step depends on how the step was
reached:

```kix
match choice!{
    id = "where",
    t!["Go to the roof"] => Where::Roof,
    t!["Stay in the classroom"] => Where::Classroom,
} {
    Where::Roof => s!(render = roof, t!["The wind is cold."]),
    Where::Classroom => s!(render = classroom, t!["It's quiet."]),
}
s!(who = "Azusa", t!["So, about tomorrow…"]);
```

{{#include diagrams/consistency.svg}}

The last step shows the roof on one path and the classroom on the other. If that is intended, it
is fine. If it is a leftover from a branch, it is exactly the kind of bug that is invisible to
everyone but the author.

The compiler computes, for each step, the set of screens that can reach it, and reports steps
where that set has more than one element, together with the paths that cause it. This is the
presentation-ambiguity lint from the proposal. The sets are tracked per entity and component, and
each scene is summarized once as the set of screen changes it can make between its entry and its
exit, so calls do not re-analyze the callee.

The author fixes a report in one of two ways:

- **Insert a keyframe** after the merge, if the difference is a leftover.
- **Promote the dependency into a story variable**, if it is intended: set a variable in each
  branch and choose the render of the merged step by it. The analyzer then sees the dependency as
  story state.

## Unbounded screens

A render may read a component from its input screen and compute a new value from it, for example
moving an entity 10 units to the right of where it is. Inside a loop, such as a hub the player
returns to, that can produce a different screen on every pass, so the set of screens at a step
has no bound and the consistency check could not finish.

Renders are Turing-complete `const fn`, so whether that set is finite cannot be decided in
general. *Tentative:* the compiler computes the sets as a fixpoint, evaluating each render on the
screens that reach its step, and stops when a step's set grows past a fixed cap. The render that
made it grow is reported as an error; error recovery leaves it out so that the rest of the program
is still checked.

The rule is sound: when the program is accepted, the computed sets are exact. It may reject a
render that is bounded only above the cap. The cap is an [open question](./open-questions.md#bounded-screen-cap).

## Settings in renders

A render sometimes needs a setting, for example to show text at the player's chosen speed. It
cannot read the setting's value: the value is only known at runtime, the screen at a step would
then depend on the settings, and a snapshot taken under one setting would be wrong under another.

A render uses a **deferred reference** instead. `BGM_VOLUME` in `static BGM_VOLUME: Setting<…>`
is a handle to a setting, not its value (see [Settings and meta](./system.md#settings-and-meta)).
In a `const fn` the handle has no way to read the value: no `.get()`, no `match`, no arithmetic. A
render can only store it in a component, and the engine's systems read the value at draw time.
Equality on handles compares which setting they refer to, so a render using one is still a
function of its input screen, and screens are still comparable.

## The `Hud` component

A story sometimes wants to hide the text frame, for example during a CG. That is a request from
the story to the system. It goes through the screen, keeping the flow one-way:

```kis
enum Visibility { Shown, Hidden }

struct Hud {
    text_frame: Visibility,
    quick_menu: Visibility,
}
```

`Hud` is a resource component of the screen. A render sets it like any other component, and the
game view of the system reads it from the screen. The story never reads the HUD, the HUD reads
the story's output, and as part of the screen it is covered by the consistency analysis and the
screen snapshot.

```kix
s!(render = |mut screen| { screen.hud().text_frame = Visibility::Hidden; screen });
```

## The standard screen API

*Tentative.* The shapes below are the ones this book's examples use; the real library replaces
them.

```kis
// std::screen
pub struct Screen;

impl Screen {
    #[builtin] pub const fn empty() -> Screen;
    #[builtin] pub const fn entity(&mut self, id: EntityId) -> &mut Entity;
    #[builtin] pub const fn background(&mut self) -> &mut Layer;
    #[builtin] pub const fn hud(&mut self) -> &mut Hud;
}

pub type EntityId = u64;
pub type ImageId = u64;

pub struct Entity { pub cord: Cord, pub appearance: Appearance }
pub struct Cord { pub x: i64, pub y: i64 }
pub struct Appearance { pub emotion: Layer }
pub struct Layer { pub image: ImageId }

impl Cord {
    #[builtin] pub const fn apply_animation(&mut self, animation: CordAnimation);
}

impl Layer {
    #[builtin] pub const fn change(&mut self, image: ImageId) -> &mut LayerImageChange;
}

#[must_use]
pub struct LayerImageChange { pub image: ImageId }

impl LayerImageChange {
    #[builtin] pub const fn apply_animation(&mut self, transition: TransitionAnimation);
}

pub struct CordAnimation { pub height: u64, pub frames: u64 }
pub struct TransitionAnimation { pub frames: u64 }

pub const fn jump(height: u64) -> CordAnimation { CordAnimation { height, frames: 12 } }
pub const fn instant_switch() -> TransitionAnimation { TransitionAnimation { frames: 0 } }
pub const fn fade(frames: u64) -> TransitionAnimation { TransitionAnimation { frames } }
```

`#[builtin]` functions have no body: the engine implements them, as the interface to the ECS
world of the screen. Everything else is ordinary `const fn` code, evaluated by the compiler like
any studio function.
