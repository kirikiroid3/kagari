# The System Context (`std/system-ctx.kix`)

`std/system-ctx.kix` is the only system file the game may import. It lets render code refer to
settings and to the HUD without making the screen depend on settings values.

## The problem

Render functions are evaluated at compile time to `Screen -> Screen` functions, and the
consistency analysis compares screens. If a render could read a setting value, for example to
skip an animation when "reduce motion" is on, then:

- the render could no longer be evaluated at compile time, because the value is only known at
  runtime;
- the screen at a step would depend on the settings, so the consistency analysis would have to
  quantify over them;
- a screen snapshot taken under one setting would be wrong when loaded under another.

## Deferred references

The context is a static handle. Its projections return **deferred references**, never values:

```kix
// std/system-ctx.kix — game scope only. Tentative names.

struct SystemContext;              // a static handle, the same in every run
struct Setting<T>;                 // a reference to a setting, not its value

impl SystemContext {
    fn text_speed(&self) -> Setting<Speed>;
    fn voice_volume(&self) -> Setting<u64>;
}
```

The rule that makes this sound:

- `Setting<T>` has an **introduction** form in game code (the projections above) and **no
  elimination** form: no `.get()`, no `match`, no arithmetic. Game code can only store it in a
  component.
- The engine's systems eliminate it, at draw time.
- `Eq` on `Setting<T>` compares the reference (which setting), not the value.

By parametricity a render cannot depend on the values behind the context. So it is still
evaluated at compile time, with `ctx` as a free symbol, and two screens are compared as terms.
Equality of terms with `ctx` free implies equality for every value of the settings, so the
consistency analysis stays sound. A screen snapshot stores the reference, so a save loaded after a
settings change simply shows the new values.

## The `Hud` component

A story sometimes wants to hide the text frame, for example during a CG. That is a signal from the
game to the system. It goes through the screen, keeping the flow one-way:

```kix
// std/system-ctx.kix

enum Visibility { Shown, Hidden }

struct Hud {
    text_frame: Visibility,
    quick_menu: Visibility,
}
```

`Hud` is a resource component of the screen. A render sets it like any other component, and the
layers of the `Game` mode read it through `screen(&Narrative)`. The narrative never reads the HUD,
the HUD reads the narrative's output, and as part of the screen it is covered by the consistency
analysis and the screen snapshot.

```xml
<s>
  <r>screen.hud().text_frame.set(Visibility::Hidden);</r>
</s>
```
