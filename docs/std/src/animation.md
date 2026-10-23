# Animation

`std::screen::anim` declares the animation values: `Easing`, `Tween` for properties, `Transition`
for layer swaps, and `Motion` for relative paths. The timeline methods of `Screen` decide when they
start. Every item is `const fn`.

```kis
use crate::screen::{Angle, Color, Offset, degrees, offset};
use crate::time::{Duration, ZERO, ms};
```

## The timeline

Every step has a timeline. A render schedules animations on it through a **cursor**, a position
made of an anchor (the step entry or a text mark) and an offset in milliseconds. At step entry the
cursor is (entry, 0 ms), and so is the text start.

```kis
impl Screen {
    #[builtin] pub const fn wait(&mut self, duration: Duration);
    #[builtin] pub const fn barrier(&mut self);
    #[builtin] pub const fn at_mark(&mut self, mark: Mark);
    #[builtin] pub const fn at_entry(&mut self);
    #[builtin] pub const fn begin_text(&mut self);
}
```

- Every animation, transition, motion and sound starts at `cursor + delay`. Statements between
  two cursor moves start together.
- `wait(d)` advances the cursor by `d`.
- `barrier()` moves the cursor to the end of everything scheduled so far: animations, transitions,
  motions, despawns and one-shot sounds. Sound lengths are resolved by the engine.
- `at_mark(m)` moves the cursor to (mark `m`, 0). A mark is the time the text reveal reaches
  `mark(n)` (see [Text](./text.md#marks)); a mark the text does not contain resolves to the end
  of the reveal.
- `at_entry()` moves it back to (entry, 0).
- `begin_text()` makes the text reveal and the voice start at the cursor. Calling it while the
  cursor is anchored on a mark is a const-evaluation error.

The timeline is step-only: it is not part of the logical screen.

```kis
const fn cg_then_text(mut screen: Screen) -> Screen {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen.background().change(CG_001).apply_animation(fade(ms(800)));
    screen.barrier();
    screen.hud().text_window().change(Visibility::Shown).apply_animation(tween(ms(200)));
    screen.begin_text();
    screen
}
```

A mid-line cue schedules on a mark:

```kis
const fn startled(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(1));
    screen.entity(HANAKO).image().change(HANAKO::SURPRISED).apply_animation(fade(ms(300)));
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

## Clicks, skip and the `animations` setting

- A click during the step completes the whole timeline and shows the full text, except that a
  `Pause::Click` waits for its own click.
- One-shot sounds that have not started yet are dropped.
- The next click is `NextStep`. Skip mode completes at once.
- With the `animations` setting false, every duration counts as 0.

None of these change the logical screen, because every animation ends at the value its `change`
set, and every motion ends at identity.

## `Easing`

```kis
pub enum Easing {
    Linear,
    InQuad, OutQuad, InOutQuad, InCubic, OutCubic, InOutCubic, InQuart, OutQuart, InOutQuart,
    InQuint, OutQuint, InOutQuint, InSine, OutSine, InOutSine, InExpo, OutExpo, InOutExpo,
    InCirc, OutCirc, InOutCirc, InBack, OutBack, InOutBack, InElastic, OutElastic, InOutElastic,
    InBounce, OutBounce, InOutBounce,
    CubicBezier(Decimal, Decimal, Decimal, Decimal),
    Steps(u32),
}
```

- The named easings use the Penner (easings.net) formulas. `In` means a slow start, `Out` a slow
  end.
- `CubicBezier(x1, y1, x2, y2)` follows CSS `cubic-bezier`. `x1` and `x2` outside `0.0..=1.0` are
  a const-evaluation error.
- `Steps(n)` jumps at the end of each of `n` equal intervals. `n = 0` is an error.
- The default everywhere is `Linear`.

## `Tween`

```kis
pub struct Tween { pub duration: Duration, pub easing: Easing, pub delay: Duration }
pub const fn tween(duration: Duration) -> Tween { Tween { duration, easing: Easing::Linear, delay: ZERO } }
pub const fn instant() -> Tween { tween(ZERO) }
impl Tween {
    pub const fn ease(mut self, easing: Easing) -> Tween { … }
    pub const fn delay(mut self, delay: Duration) -> Tween { … }
}
```

A tween animates a `Prop<T>` from its value at `cursor + delay` to the value set by `change`, over
`duration` with `easing`. `instant()` is a zero-length tween.

```kis
screen.entity(HANAKO).position().change(point(400, 80)).apply_animation(tween(ms(1000)).ease(Easing::InOutQuad));
```

## `Transition`

```kis
pub enum Direction { Left, Right, Up, Down }
pub enum TransitionKind {
    Cut,
    Crossfade,
    Rule { mask: ImageAsset, softness: u8, reverse: bool },
    Wipe(Direction),
    Blinds { direction: Direction, count: u32 },
    ThroughColor { color: Color, hold: Duration },
}
pub struct Transition { pub kind: TransitionKind, pub duration: Duration, pub easing: Easing, pub delay: Duration }
pub const fn cut() -> Transition { … }
pub const fn fade(duration: Duration) -> Transition { … }
pub const fn rule(mask: ImageAsset, duration: Duration) -> Transition { … }
pub const fn rule_with(mask: ImageAsset, softness: u8, reverse: bool, duration: Duration) -> Transition { … }
pub const fn wipe(direction: Direction, duration: Duration) -> Transition { … }
pub const fn blinds(direction: Direction, count: u32, duration: Duration) -> Transition { … }
pub const fn through(color: Color, duration: Duration, hold: Duration) -> Transition { … }
impl Transition {
    pub const fn ease(mut self, easing: Easing) -> Transition { … }
    pub const fn delay(mut self, delay: Duration) -> Transition { … }
}
```

Every constructor sets easing `Linear` and delay 0. `e(t)` below is the eased progress, from 0 to
1.

|Constructor|Kind|Effect|
|---|---|---|
|`cut()`|`Cut`, duration 0|switches at the start|
|`fade(d)`|`Crossfade`|old opacity 1 − e(t), new opacity e(t)|
|`rule(mask, d)`|`Rule`, softness 0, not reversed|a rule-image wipe; see below|
|`rule_with(mask, softness, reverse, d)`|`Rule`|the same, with softness and direction|
|`wipe(dir, d)`|`Wipe(dir)`|reveals the new image moving in direction `dir`|
|`blinds(dir, count, d)`|`Blinds`|`count` stripes perpendicular to `dir`, each wiping|
|`through(color, d, hold)`|`ThroughColor`|the old image fades to `color` over `d / 2`, holds `hold`, then the new image fades in over `d / 2`; the total is `d + hold`|

- **Rule.** A pixel shows the new image once the mask's luminance is ≤ threshold(t). The threshold
  sweeps 0 → 255, or 255 → 0 with `reverse`. `softness` is the width of the blend ramp in
  luminance units.
- The same transitions apply to `ImageLayer`, `LabelLayer` and `MovieLayer` swaps.

```kis
const BLINDS_RULE: ImageAsset = asset("rule/blinds.png");

screen.background().change(BG::STATION).apply_animation(rule(BLINDS_RULE, ms(1000)));
screen.background().change(BG::NIGHT).apply_animation(through(BLACK, ms(1000), ms(200)));
```

## Motion

```kis
pub struct MotionKey { pub at: Duration, pub offset: Offset, pub scale: Decimal, pub rotation: Angle, pub easing: Easing }
pub struct Motion { pub keys: Vec<MotionKey>, pub repeat: u32, pub delay: Duration }
pub const fn motion() -> Motion { Motion { keys: Vec::new(), repeat: 1, delay: ZERO } }
impl Motion {
    /// Appends a key with scale 1.0, rotation 0°, easing Linear.
    pub const fn key(self, at: Duration, offset: Offset) -> Motion { self.key_full(at, offset, 1.0, degrees(0.0)) }
    /// Appends a key with easing Linear.
    pub const fn key_full(mut self, at: Duration, offset: Offset, scale: Decimal, rotation: Angle) -> Motion { … }
    /// Sets the easing of the segment that ends at the last key.
    pub const fn ease(mut self, easing: Easing) -> Motion { … }
    pub const fn repeat(mut self, times: u32) -> Motion { … }
    pub const fn delay(mut self, delay: Duration) -> Motion { … }
}
```

A motion is a relative path played on an entity or the camera with `apply_animation`. It is how a
character jumps, nods or shakes without any change to its logical state.

- The path starts from identity (offset (0, 0), scale 1.0, rotation 0°) at t = 0. Keys are
  relative to the property's current value and are applied on top of it.
- Each key's `easing` is the easing of the segment that ends at it.
- One repetition lasts the last key's `at`; the motion plays `repeat` times, starting at
  `cursor + delay`.

**The identity rule.** A motion must end where it started, so it can never change the logical
screen. These are const-evaluation errors:

- no keys;
- an `at` that is 0 or not strictly greater than the previous key's;
- a last key that is not offset (0, 0), scale 1.0 and rotation 0°;
- `repeat` = 0.

```kis
const fn hop_twice() -> Motion {
    motion()
        .key(ms(150), offset(0, -30)).ease(Easing::OutQuad)
        .key(ms(300), offset(0, 0)).ease(Easing::InQuad)
        .repeat(2)
}
```

### Presets

```kis
pub const fn jump(height: i32, duration: Duration) -> Motion { … }
pub const fn nod(depth: i32, duration: Duration) -> Motion { … }
pub const fn shake(amplitude: i32, duration: Duration, count: u32) -> Motion { … }
pub const fn quake(horizontal: i32, vertical: i32, duration: Duration) -> Motion { … }
```

Every preset returns to identity. Scale is 1.0 and rotation 0° in every key.

**`jump(h, d)`** (repeat 1):

|`at`|offset|easing|
|---|---|---|
|`d / 2`|(0, −h)|`OutQuad`|
|`d`|(0, 0)|`InQuad`|

**`nod(p, d)`** (repeat 1):

|`at`|offset|easing|
|---|---|---|
|`d / 2`|(0, p)|`OutQuad`|
|`d`|(0, 0)|`InQuad`|

**`shake(a, d, n)`**: one cycle of length c = d / n, repeated `n` times. `n` = 0 is an error
through `repeat`.

|`at`|offset|easing|
|---|---|---|
|`c / 4`|(−a, 0)|`Linear`|
|`3c / 4`|(a, 0)|`Linear`|
|`c`|(0, 0)|`Linear`|

**`quake(h, v, d)`** (repeat 1): a key every 50 ms, all `Linear`.

|`at`|offset|
|---|---|
|k · 50 ms, for k = 1 … (d / 50 − 1), k odd|(h, −v)|
|k · 50 ms, for k = 1 … (d / 50 − 1), k even|(−h, v)|
|`d`|(0, 0)|

```kis
screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
screen.entity(AZUSA).apply_animation(shake(12, ms(400), 4));
screen.camera().apply_animation(quake(8, 4, ms(600)));
```
