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
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
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
    render = |mut screen| { screen.background().change(BG::ROOF).apply_animation(fade(ms(500))); screen },
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
screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
```

1. `screen.entity(AZUSA)` selects the entity whose id is `AZUSA`, say `EntityId(217)`, and spawns
   it if the screen has none. It is a [reference](./values.md#references) into the
   screen: changing it changes the screen.
2. `.image()` is the entity's main `ImageLayer`. `.change(AZUSA::CLOSE_EYE_SMILE)` asks to swap
   that layer to the image `AZUSA::CLOSE_EYE_SMILE`. The result is a `&mut Swap`, a pending
   change: a reference into the layer.
3. Because this swaps a layer, it needs a `Transition`. Without one the change does not happen.
4. `.apply_animation(cut())` switches the image at once, and the change takes effect.

References may be bound to locals, so a render that changes the same entity several times can name
it once:

```kis
let azusa = screen.entity(AZUSA);
azusa.image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
azusa.opacity().change(1.0).apply_animation(tween(ms(300)));
```

## The screen is an ECS world

`Screen` follows the entity–component–system pattern:

| Part | Defined in | Role |
| ---- | ---------- | ---- |
| Entities | render functions | Spawned, changed and despawned by renders; identified by an `EntityId` constant |
| Components | `std::screen`, a fixed set | Data attached to entities: plane, z, position, anchor, scale, rotation, opacity, filter shader, generated-content shader, fill, image parts, label, movie, flip, blend |
| Resources | `std::screen`, a fixed set | Data of the screen itself: the Hud, the choice menu, the camera, the screen filter shader, the audio channels |
| Systems | the engine | Process entities at runtime, for example advancing animations frame by frame |

Every component type is [`Eq`](./language.md#equality), so two screens can always be
compared. Systems are not written in Kagari and are not part of the logical screen: the screen a
step shows is the set of entities and their components after its render, not the frames the
engine draws while an animation plays.

The component set is fixed: a studio adds no components. It extends the library with `const fn`
helpers and constant data in its own modules, such as character slots, face images and recipes
like a dip to black. `std::stage` is the standard example of such a layer: its characters are
plain `const fn` over `std::screen`. The compiler type-checks studio helpers like any other code.

## Changes need animations

A change of anything visible produces a pending change, and only `apply_animation` turns it into
an actual change on the screen. Every visible change therefore states how it is animated, even if
the answer is "instantly".

Different targets take different animation types, so the type checker catches mismatches such as
passing a `Transition` to a position change:

| Target | Changed with | Animation type |
| ------ | ------------ | -------------- |
| A property `Prop<T>`: position, scale, rotation, opacity, filter and shader (`Option<ShaderAsset>`), fill; Hud visibilities; camera offset, zoom and rotation; channel volume | `.change(v)`, giving a `Change<T>` | `Tween`: duration, easing, delay |
| A layer: `ImageLayer` (including the background and the side image), `LabelLayer`, `MovieLayer` | `.change(x)` or `.clear()`, giving a `Swap` | `Transition`: cut, crossfade, rule, wipe, blinds, through a colour |
| An entity or the camera as a whole | `.apply_animation(m)` directly | `Motion`: a relative path that returns to where it started |
| A music or ambience channel | `.change(track)` or `.stop()`, giving a `ChannelChange` | `AudioFade` |

Filters, weather and particles are shaders. The screen and each entity have a `filter()`, an
`Option<ShaderAsset>` that post-processes what they draw (`None` is no filter). Weather and
particles are entities whose `shader()` is set: the shader generates their content, drawn in the
entity's plane and ordered by its `z`, the analog of CSS `z-index`.

Music carries over between steps: a step whose render leaves the music channel on the track that
is already playing does not restart it. A render restarts the track from its start with
`screen.replay_bgm()`.

```kis
const fn mio_enters(mut screen: Screen) -> Screen {
    screen.entity(MIO).position().change(point(640, 80)).apply_animation(instant());
    screen.entity(MIO).image().change(MIO::NEUTRAL).apply_animation(cut());
    screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)).ease(Easing::OutQuad));
    screen
}
```

A spawned entity starts invisible, with opacity 0.0, so every appearance states its animation.

`Change<T>`, `Swap` and `ChannelChange` are `#[must_use]`, so a statement that ends with a change
still pending, such as `screen.background().change(BG::ROOF);`, is a warning. Only layout
settings that cannot be interpolated are set directly: `set_plane`, `set_z`, `set_anchor`,
`set_flip`, `set_blend`, `set_fill_size` and `gate`.

### Animations never change the logical screen

The **logical screen** of a step is the persistent state after its render: the entities with
their components, and the Hud, camera, screen filter and audio channels. Snapshots, screen
equality and the [consistency check](#consistency) use only the logical screen, and animations
never change it:

- A `Tween` or a `Transition` always runs from the old value to the value set by `change`. The
  logical value is the new one from the moment `change` runs.
- A `Motion` is a relative path that must end where it started. A motion that does not is a
  compile error.
- The step's timeline, its scheduled animations, one-shot sounds and the choice menu belong to the
  step only and are not part of the logical screen.

So a click, which completes the step's animations, skip mode, or the player turning animations
off can never change the logical screen.

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

`Screen::empty()` returns a screen with only the background entity, `BACKGROUND`, with no image:
the Hud shows the text window and the quick menu, the camera is neutral, the screen filter is `None`,
and every audio channel is silent. A keyframe can therefore be written explicitly:

```kis
const fn roof_evening(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(BG::ROOF_EVENING).apply_animation(fade(ms(500)));
    screen
}
```

Keyframes bound how far back the engine must replay renders when it loads a save on a changed
script or jumps to a step it has no snapshot for: it replays the story to the target, then runs
renders only from the last keyframe on that path.

{{#include diagrams/keyframes.svg}}

Keyframes are recommended at choice points and chapter starts. The keyframe-placement check is
opt-in, through `--keyframe-check` or `CompileOptions::keyframe_check`: when it is on, every
`choice!` and the first step of each entry scene need a keyframe render, and a step without one
gets the warning `W5011`. A step with no render is not a keyframe. Interrupt scenes, such as a phone
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

The compiler computes, for each step, the screens that can reach it, and reports a step whose
shown screen (the logical screen after its render) is not determined by the story state, together
with the renders that last changed the screen on the differing paths. This is the
presentation-ambiguity lint from the proposal, the warning `W5004`.

*The story state* at a step is everything the story itself could tell apart: the step, the values
of the story variables, the locals of every frame on the call stack that are still used later, the
entry arguments, and the call sites on the stack. Two arrivals at a step that agree on all of it
must show the same screen. So an interrupt scene, such as a phone call that can happen anywhere,
may show a different screen at each of its call sites, but not two screens for one call site. A
merge is reported once, at the first step after it; the steps after that one differ for the same
reason.

Each scene is explored once per entry state and entry screen, so a call does not re-analyze the
callee for a screen it was already entered with.

The author fixes a report in one of two ways:

- **Insert a keyframe** at the first inconsistent step, the one reported, if the difference is a
  leftover. This is the fix the warning suggests.
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
screens that reach its step, and stops when a step's set grows past a fixed cap, currently 64
screens. The render that produced the new screen is reported as an error
(`E5005`); error recovery leaves it out, as if it left the screen unchanged,
and computes the sets again, so that the rest of the program is still checked.

The rule is sound: when the program is accepted, the computed sets are exact. It may reject a
render that is bounded only above the cap. The cap is an [open question](./open-questions.md#bounded-screen-cap).

## Settings in renders

A render sometimes needs a setting, for example to draw a decorative effect only when the player
enabled it. It cannot read the setting's value: the value is only known at runtime, the screen at
a step would then depend on the settings, and a snapshot taken under one setting would be wrong
under another.

A render uses a **deferred reference** instead. `PETALS_ON` in `static PETALS_ON: Setting<bool>`
is a handle to a setting, not its value (see [Settings and meta](./system.md#settings-and-meta)).
In a `const fn` the handle has no way to read the value: no `.get()`, no `match`, no arithmetic. A
render can only pass it to `Entity::gate`, and the engine reads the value at draw time: the entity
is drawn only while the setting is true.

```kis
static PETALS_ON: Setting<bool> = setting(true);
const PETALS: EntityId = EntityId(40);
const PETALS_IMAGE: ImageAsset = asset("petals.png");

const fn petals(mut screen: Screen) -> Screen {
    screen.entity(PETALS).set_plane(Plane::Front);
    screen.entity(PETALS).gate(Some(PETALS_ON));
    screen.entity(PETALS).image().change(PETALS_IMAGE).apply_animation(cut());
    screen.entity(PETALS).opacity().change(1.0).apply_animation(tween(ms(500)));
    screen
}
```

The handle is part of the logical screen. Equality on handles compares which setting they refer
to, so a render using one is still a function of its input screen, and screens are still
comparable.

## The `Hud`

A story sometimes wants to hide the text window, for example during a CG. That is a request from
the story to the system. It goes through the screen, keeping the flow one-way:

```kis
pub enum Visibility { Shown, Hidden }

impl Hud {
    #[builtin] pub const fn text_window(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn quick_menu(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn side_image(&mut self) -> &mut ImageLayer;
}
```

`Hud` is a resource of the screen, reached with `screen.hud()`. A render changes it like any other
property or layer, and the game view of the system reads it through the `Narrative` handle. The
story never reads the HUD, the HUD reads the story's output, and as part of the logical screen it
is covered by the consistency analysis and the screen snapshot.

```kix
s!(render = |mut screen| { screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant()); screen });
```

A `Prop<Visibility>` interpolates as a fade, so `tween(ms(300))` instead of `instant()` fades the
window out. The side image is the speaker's face shown next to the text, swapped with a
`Transition` like any layer.

## The standard screen API

The screen API is defined by the standard library book. Chapter *Screen* covers geometry and
colours, planes and drawing order, entities with their spawn defaults and despawning, properties
and layers, the Hud, the choice menu, the camera, filter and generated-content shaders, and blend modes. Chapter *Animation*
covers the step's timeline (`wait`, `barrier`, `at_mark`, `begin_text`), what a click and skip
mode do to it, easings, tweens, transitions and motions with their presets. Chapter *Audio* covers
channels, loops and one-shot sounds, and chapter *Stage* the character layer built on top.

`#[builtin]` functions have no body: the engine implements them, as the interface to the ECS
world of the screen. Everything else is ordinary `const fn` code, evaluated by the compiler like
any studio function.
