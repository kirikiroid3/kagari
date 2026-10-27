# Introduction

This book is the reference of `std`, the standard library of Kagari. It lists every item of the
library with its exact signature and its semantics. The language itself (syntax, kinds, scenes,
forms) is described in the syntax reference; this book assumes it.

The library is written in Kagari. Its sources are `core/kagari-std/kis/std/**`. Items marked
`#[builtin]` have no body: the engine implements them. Everything else is ordinary code that a
studio could have written itself.

The design follows what visual-novel studios actually build: characters with named slots,
background transitions through rule images or a colour, preset motions, title cards, BGM with an
intro and a loop, voice bound to the line, galleries unlocked from what the player has seen. What
other engines leave to studio macros, `std` provides as typed, checked values.

## Design rules

Every item of the library follows these rules.

1. **Kinds.**
   - Everything that builds or changes a `Screen`, every animation/transition/text-effect value
     and every geometry helper is a `const fn`.
   - The engine's view of a running game, settings and meta reads, navigation, saves and UI are
     `fn`.
   - `push_step` and `Meta::write` are the only `pushdown fn`.
   - Engine-owned state types with timeline semantics are `#[builtin]` structs whose methods are
     `#[builtin]`. Plain data (geometry, colours, tweens, styles) are ordinary std structs with
     `pub` fields plus `const fn` builders.
2. **Units.**
   - Positions and lengths are `i32`/`u32` pixels in the design resolution `System::resolution`.
   - Times are `time::Duration`, whole milliseconds.
   - Fractions (anchor, scale, opacity, volume) are `Decimal`, with literals like
     `0.5`; opacity and volume range over `0.0..=1.0`.
   - Settings volumes are `Int<0, 100>`.
   - There are no unit strings, and there is no second scale for any quantity.
3. **No hidden defaults.** Every animation constructor takes its duration explicitly. The only
   implicit values are the neutral ones: easing `Linear`, delay 0, rule softness 0, repeat 1,
   identity transforms.
4. **Changes need animations.** Every visible property is a `Prop<T>` (interpolated, takes a
   `Tween`) or a layer (`ImageLayer`/`LabelLayer`/`MovieLayer`, swapped with a `Transition`).
   Only non-interpolable layout settings (`set_plane`, `set_z`, `set_anchor`, `set_flip`,
   `set_blend`, `set_fill_size`, `gate`) are direct setters. `Change<T>`, `Swap` and
   `ChannelChange` are `#[must_use]`. A change with no `apply_animation` does not happen.
5. **Animations never change the start or end state.** The logical screen of a step is the
   persistent state after its render: entities with their components, plus the Hud, camera,
   screen filter and audio channels. That is what snapshots, equality and the consistency check
   use.
   - A `Tween`/`Transition` always runs from the old value to the value set by `change`.
   - A `Motion` is a relative path that must end at identity (offset 0, scale 1, rotation 0). A
     Motion that does not is a const-evaluation error.
   - The timeline, scheduled animations, one-shot sounds and the `ChoiceMenu` belong to the step
     only and are not part of the logical screen.
   - So a click (which completes the step's timeline), skip, or the `animations` setting being off
     can never change the logical screen.
6. **Fixed component set.** Studios add no components. They extend the library with `const fn`
   helpers and constant data. [`std::stage`](./stage.md) is the reference of such a layer.
7. **Type-checkable without overloading or coercion.** Asset-typed functions use a trait bound.
   Interpolation uses a trait bound. Special signatures are listed in [Checks](./checks.md)
   (`nav::start`). Only `std` declares traits, all `#[builtin]`: `Eq`, `Integer`, `Step`, `Asset`
   and `ToText`. Users never implement them; `Eq` is automatic.
8. **References** are shared pointers. In `const fn` and `fn` code they may be bound to locals,
   so a render names an entity once and changes it in several statements. In a `pushdown fn` they
   are parameters only.

For example:

```kis
const fn hanako_appears(mut screen: Screen) -> Screen {
    let hanako = screen.entity(HANAKO);
    hanako.image().change(HANAKO::SMILE).apply_animation(cut());
    hanako.opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```

## Modules

|Module|Contents|Kind|Chapter|
|---|---|---|---|
|`std`|`String`, `Vec`, `Range`, `Rat`, `Decimal`, `Option`, `Result`, `Eq`, `Integer`, `Asset`, the asset types, `asset`|`const fn`|[Core Types](./core.md)|
|`std::time`|`Duration`, `ms`, `secs`|`const fn`|[Core Types](./core.md#time)|
|`std::narrative`|`Step`, `push_step`, `Say`, `Choice`, `Meta`, `Narrative`|`pushdown fn` (`push_step`, `Meta::write`); `fn` (`Narrative`, `Meta::get`/`set`); `const fn` (`meta`)|[Narrative](./narrative.md)|
|`std::narrative::text`|`Text`, text effects, `ToText`, `TextStyle`|`const fn`|[Text](./text.md)|
|`std::screen`|`Screen`, `Entity`, `Prop`, layers, Hud, camera, geometry, colours|`const fn`|[Screen](./screen.md)|
|`std::screen::anim`|`Easing`, `Tween`, `Transition`, `Motion`, presets|`const fn`|[Animation](./animation.md)|
|`std::screen::audio`|`Channel`, `Loop`, `Sound`, `AudioFade`|`const fn`|[Audio](./audio.md)|
|`std::stage`|`Character`, `hide_all`, `side_face`|`const fn`|[Stage](./stage.md)|
|`std::system`|`System`, `Settings`, `Setting`, modes, seen sets, bindings|`fn` (`Setting::get`/`set`, seen sets); `const fn` (constructors)|[The Game System](./system.md)|
|`std::system::nav`|navigation between modes, starting and loading the story|`fn`|[The Game System](./system.md#navigation)|
|`std::system::saves`|save slots and their info|`fn`|[The Game System](./system.md#saves)|
|`std::ui`|`Element`, signals, components, UI-form targets|`fn`|[UI](./ui.md)|
|`std::ui::style`|`Paint`, `Look`, `ButtonStyle`, `SliderStyle`|`const fn`|[UI](./ui.md#styles)|

## The prelude

Every module imports the prelude implicitly. A name declared in the module, or imported by it,
takes precedence over a prelude name.

```kis
pub use crate::{String, Vec, Rat, Decimal, Range, Option, Result, Eq, Integer, RandChoose, ImageAsset, AudioAsset, VoiceAsset, VideoAsset, FontAsset, ShaderAsset, asset, system_font};
pub use crate::Option::{None, Some};
pub use crate::Result::{Err, Ok};
pub use crate::time::{Duration, ms, secs};
pub use crate::screen::{Screen, Render, EntityId, Part, Point, Offset, Size, Anchor, Color, Plane, Visibility, point, offset, rgb, hex, BLACK, WHITE, TRANSPARENT};
pub use crate::screen::anim::{Easing, Tween, Transition, Motion, tween, instant, cut, fade, rule, wipe, through, motion, jump, nod, shake, quake};
pub use crate::screen::audio::{looped, sound, crossfade};
pub use crate::stage::{Character, character};
pub use crate::narrative::{Text, TextEffect, Say, Choice, ChoiceArm, push_step, Meta, meta, Narrative};
pub use crate::narrative::text::{emphasis, dots, color_text, bold, pause, click_wait, mark};
pub use crate::system::{nav, Setting, setting, System, Settings, Volume, volume, RootPage, Overlay, GameOverlay, Dialog};
pub use crate::ui::{signal, Element};
```

Everything else is imported by path, for example `use std::screen::anim::rule_with;` or
`use std::narrative::text::speed;`.
