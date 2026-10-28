# Using the Standard Library

`std` is the library every Kagari program is compiled with. It is written in Kagari; items marked
`#[builtin]` are provided by the compiler or the engine.

The design follows what visual-novel studios actually build: characters with named slots,
background transitions through rule images or a colour, preset motions, title cards, BGM with an
intro and a loop, voice bound to the line, galleries unlocked from what the player has seen. What
other engines leave to studio macros, `std` provides as typed, checked values.

## Modules

|Module|Contents|Kind|Page|
|---|---|---|---|
|`std`|`String`, `Vec`, `Range`, `Rat`, `Decimal`, `Option`, `Result`, `Eq`, `Integer`, `RandChoose`, `Asset`, the asset types, `asset`, `system_font`|`const fn`|[std](./std.md)|
|`std::prelude`|the names imported into every module|—|[std::prelude](./prelude.md)|
|`std::time`|`Duration`, `ZERO`, `ms`, `secs`|`const fn`|[std::time](./time.md)|
|`std::narrative`|`Step`, `push_step`, `rng_choose`, `Say`, `Choice`, `Meta`, `Narrative` and its records|`pushdown fn` (`push_step`, `rng_choose`, `Meta::write`); `fn` (`Narrative`, `Meta::get`/`set`); `const fn` (`meta`)|[std::narrative](./narrative.md)|
|`std::narrative::text`|`Text`, the `t![…]` builtins, `ToText`, text effects, `TextStyle`|`const fn`|[std::narrative::text](./narrative-text.md)|
|`std::screen`|`Screen`, entities, properties, layers, Hud, camera, choice menu, geometry, colours|`const fn`|[std::screen](./screen.md)|
|`std::screen::anim`|`Easing`, `Tween`, `Transition`, `Motion` and presets|`const fn`|[std::screen::anim](./screen-anim.md)|
|`std::screen::audio`|channels, loops, sounds, fades|`const fn`|[std::screen::audio](./screen-audio.md)|
|`std::stage`|`Character`, `hide_all`, `side_face`|`const fn`|[std::stage](./stage.md)|
|`std::system`|`System`, `Settings`, `Setting`, modes, seen sets, bindings|`fn` (handles, seen sets); `const fn` (constructors)|[std::system](./system.md)|
|`std::system::nav`|navigation between modes; starting and loading the story|`fn`|[std::system::nav](./system-nav.md)|
|`std::system::saves`|save slots and their information|`fn`|[std::system::saves](./system-saves.md)|
|`std::ui`|`Element`, signals, components, the targets of the UI forms|`fn`|[std::ui](./ui.md)|
|`std::ui::style`|`Paint`, `Look`, `ButtonStyle`, `SliderStyle`|`const fn`|[std::ui::style](./ui-style.md)|

## The prelude

Every module imports the prelude implicitly: the names of
[`std::prelude`](./prelude.md) are in scope without a `use`. A name declared in the module, or
imported by it, takes precedence over a prelude name.

Everything else is imported by path:

```kis
use std::screen::anim::rule_with;
use std::narrative::text::speed;
```

## Design rules

Every item of the library follows these rules. They tell you what to expect from an item you have
not looked up yet.

1. **Kinds.**
   - Everything that builds or changes a `Screen`, every animation, transition and text-effect
     value, and every geometry helper is a `const fn`.
   - The engine's view of a running game, settings and meta reads, navigation, saves and UI are
     `fn`.
   - `push_step`, `rng_choose` and `Meta::write` are the only `pushdown fn`.
   - Engine-owned state with timeline semantics is a `#[builtin]` type with `#[builtin]` methods.
     Plain data (geometry, colours, tweens, styles) are ordinary structs with `pub` fields plus
     `const fn` builders.
2. **Units.**
   - Positions and lengths are `i32`/`u32` pixels in the design resolution `System::resolution`.
   - Times are `time::Duration`, whole milliseconds.
   - Fractions (anchor, scale, opacity, volume) are `Decimal`, with literals like `0.5`; opacity
     and volume range over `0.0..=1.0`.
   - Settings volumes are `Int<0, 100>`.
   - There are no unit strings, and there is no second scale for any quantity.
3. **No hidden defaults.** Every animation constructor takes its duration explicitly. The only
   implicit values are the neutral ones: easing `Linear`, delay 0, rule softness 0, repeat 1,
   identity transforms.
4. **Changes need animations.** Every visible property is a `Prop<T>` (interpolated, takes a
   `Tween`) or a layer (`ImageLayer`, `LabelLayer`, `MovieLayer`, swapped with a `Transition`).
   Only non-interpolable layout settings (`set_plane`, `set_z`, `set_anchor`, `set_flip`,
   `set_blend`, `set_fill_size`, `gate`) are direct setters. `Change<T>`, `Swap` and
   `ChannelChange` are `#[must_use]`: a change with no `apply_animation` does not happen
   (warning `W3023`).
5. **Animations never change the start or end state.** The logical screen of a step is the
   persistent state after its render: entities with their components, plus the Hud, camera,
   screen filter and audio channels. That is what snapshots, equality and the consistency check
   use.
   - A `Tween` or `Transition` always runs from the old value to the value set by `change`.
   - A `Motion` is a relative path that must end at identity (offset 0, scale 1, rotation 0);
     otherwise it is a const-evaluation error.
   - The timeline, scheduled animations, one-shot sounds and the choice menu belong to the step
     only and are not part of the logical screen.
   - So a click (which completes the step's timeline), skip, or the `animations` setting being off
     can never change the logical screen.
6. **Fixed component set.** Studios add no components. They extend the library with `const fn`
   helpers and constant data. [`std::stage`](./stage.md) is an example of such a layer.
7. **Type-checkable without overloading or coercion.** Asset-typed functions use a trait bound;
   interpolation uses a trait bound. The few special signatures (`nav::start`) are listed in
   [Compiler Rules for std Items](./appendix/rules.md). Only `std` declares traits, all
   `#[builtin]`: `Eq`, `Integer`, `RandChoose`, `Asset`, `Step` and `ToText`. Games never implement
   them; `Eq` is automatic.
8. **References** are shared pointers. In `const fn` and `fn` code they may be bound to locals, so
   a render names an entity once and changes it in several statements. In a `pushdown fn` they are
   parameters only.

For example:

```kis
const fn hanako_appears(mut screen: Screen) -> Screen {
    let hanako = screen.entity(HANAKO);
    hanako.image().change(HANAKO::SMILE).apply_animation(cut());
    hanako.opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```
