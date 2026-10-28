# `std::screen::anim`

> **Stability:** Stable — animation values: easings, tweens for properties, transitions for
> layers, and relative motions.

Every change a render makes to the screen states its animation. This module declares those
values: an [`Easing`](#easing) curve, a [`Tween`](#tween) for a `Prop` change, a
[`Transition`](#transition) for a layer swap, and a [`Motion`](#motion) for a relative path on an
entity or the camera. The timeline methods of [`Screen`](./screen.md#timeline-methods) decide when
they start. Every item is `const fn` or plain data.

The prelude re-exports `Easing`, `Tween`, `Transition`, `Motion`, `tween`, `instant`, `cut`,
`fade`, `rule`, `wipe`, `through`, `motion`, `jump`, `nod`, `shake` and `quake`. The rest
(`Direction`, `TransitionKind`, `MotionKey`, `rule_with`, `masked`, `masked_with`, `blinds`)
needs a `use std::screen::anim::…`.

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Easing`](#easing) | enum | timing curve |
| [`Tween`](#tween), `tween`, `instant` | struct + constructors | how a property moves |
| [`Direction`](#direction) | enum | screen direction |
| [`TransitionKind`](#transitionkind), [`Transition`](#transition), `cut`, `fade`, `rule`, `rule_with`, `masked`, `masked_with`, `wipe`, `blinds`, `through` | enum, struct + constructors | how a layer swaps |
| [`MotionKey`](#motionkey), [`Motion`](#motion), `motion` | structs + constructor | relative paths |
| [`jump`](#jump), [`nod`](#nod), [`shake`](#shake), [`quake`](#quake) | functions | motion presets |

## The timeline {#the-timeline}

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
  `mark(n)` (see [`std::narrative::text`](./narrative-text.md#marks)); a mark the text does not
  contain resolves to the end of the reveal.
- `at_entry()` moves it back to (entry, 0).
- `begin_text()` makes the text reveal and the voice start at the cursor. Calling it while the
  cursor is anchored on a mark is a const-evaluation error.

The timeline is step-only: it is not part of the [logical screen](./screen.md#the-logical-screen).

<!-- diagram: screen-anim-timeline (mermaid-ish)
gantt
  section animations
    background fade(ms(500)) : 0, 500
    text reveal + voice      : 800, 1400
    face fade(ms(300))       : at mark(1), 300
  section cursor
    1 entry, 0 ms ==> 2 barrier() at 500 ms ==> 3 wait(ms(300)) at 800 ms, begin_text() ==> 4 at_mark(Mark(1))
-->
{{#include diagrams/screen-anim-timeline.svg}}

The render drawn above:

```kis
use std::narrative::Mark;

const fn cg_then_line(mut screen: Screen) -> Screen {
    screen.background().change(CG_001).apply_animation(fade(ms(500)));
    screen.barrier();
    screen.wait(ms(300));
    screen.begin_text();
    screen.at_mark(Mark(1));
    screen.entity(HANAKO).image().change(HANAKO::SURPRISED).apply_animation(fade(ms(300)));
    screen
}
```

A render that hides the text window until a CG is in:

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

> **Stability:** Planned — driving animation from the text reveal, and the reverse, beyond
> `at_mark` (for example an animation that follows each revealed character, or a reveal that waits
> for an animation) is not designed yet.

## Clicks, skip and the `animations` setting {#clicks-skip-and-the-animations-setting}

- A click during the step completes the whole timeline and shows the full text, except that a
  `Pause::Click` waits for its own click.
- One-shot sounds that have not started yet are dropped.
- The next click is `NextStep`. Skip mode completes at once.
- With the `animations` setting (`Settings::animations`) false, every duration counts as 0.

None of these change the logical screen, because every animation ends at the value its `change`
set, and every motion ends at identity.

## Enum: `Easing` {#easing}

```kis
pub enum Easing {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
    CubicBezier(Decimal, Decimal, Decimal, Decimal),
    Steps(u32),
}
```

A timing curve.

- The named easings use the Penner formulas ([easings.net](https://easings.net)). `In` means a slow
  start, `Out` a slow end.
- `CubicBezier(x1, y1, x2, y2)` follows CSS `cubic-bezier`. `x1` and `x2` outside `0.0..=1.0` are
  a const-evaluation error.
- `Steps(n)` jumps at the end of each of `n` equal intervals. `n = 0` is a const-evaluation error.
- The default everywhere is `Linear`.

## Struct: `Tween` {#tween}

```kis
pub struct Tween {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

How a property moves from its old value to its new one. A tween animates a `Prop<T>` from its
value at `cursor + delay` to the value set by `change`, over `duration` with `easing`. See
[`Prop`](./screen.md#prop).

### `tween(duration)` {#tween-fn}

```kis
pub const fn tween(duration: Duration) -> Tween
```

- Returns: a linear tween over `duration`, without delay.
- Kind: `const fn` · Builtin: no.

### `instant()` {#instant}

```kis
pub const fn instant() -> Tween
```

- Returns: `tween(ZERO)`, a tween that takes no time. Use it for changes that should be immediate.
- Kind: `const fn` · Builtin: no.

### `tween.ease(easing)`, `tween.delay(delay)` {#tween-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Tween
pub const fn delay(mut self, delay: Duration) -> Tween
```

`ease` returns this tween with `easing`; `delay` returns it starting `delay` after the cursor.
Kind: `const fn` · Builtin: no.

```kis
screen.entity(HANAKO).position().change(point(400, 80)).apply_animation(tween(ms(1000)).ease(Easing::InOutQuad));
```

## Enum: `Direction` {#direction}

```kis
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
```

A direction on the screen; used by wipes and blinds.

## Enum: `TransitionKind` {#transitionkind}

```kis
pub enum TransitionKind {
    Cut,
    Crossfade,
    Rule { mask: ImageAsset, softness: u8, reverse: bool },
    Wipe(Direction),
    Blinds { direction: Direction, count: u32 },
    ThroughColor { color: Color, hold: Duration },
    Masked { mask: Mask, softness: u8, reverse: bool },
}
```

How a layer swaps its old content for its new one. `e(t)` below is the eased progress, from 0
to 1.

| Variant | Effect |
| ------- | ------ |
| `Cut` | switches at the start |
| `Crossfade` | the old content fades out while the new one fades in: old opacity 1 − e(t), new opacity e(t) |
| `Rule { mask, softness, reverse }` | a pixel switches once the mask's luminance is at most the sweeping threshold |
| `Wipe(direction)` | reveals the new content moving in `direction` |
| `Blinds { direction, count }` | `count` stripes perpendicular to `direction`, each wiping |
| `ThroughColor { color, hold }` | fades to `color`, holds it for `hold`, then fades in the new content |
| `Masked { mask, softness, reverse }` | a pixel switches once the mask's value ×255 is at most the sweeping threshold |

**Rule.** A pixel shows the new content once the mask's luminance is ≤ threshold(t). The
threshold sweeps 0 → 255, or 255 → 0 with `reverse`. `softness` is the width of the blend ramp in
luminance units; 0 gives hard edges.

**Masked.** The same rule with a [mask shader](./screen-shader.md#mask) instead of a mask image:
the mask is computed over the layer's box, and a pixel shows the new content once the mask's
value × 255 is ≤ threshold(t). The mask's time is the time since the transition started, so an
animated mask moves while the transition runs. [`std::shaders::mask`](./shaders-mask.md) has
ready-made masks, so a transition needs no mask image.

## Struct: `Transition` {#transition}

```kis
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

A layer swap with its timing. The same transitions apply to `ImageLayer`, `LabelLayer` and
`MovieLayer` swaps (see [`Swap`](./screen.md#swap)). Every constructor below sets easing `Linear`
and delay 0; each is `const fn`, not builtin.

### Transition constructors {#transition-constructors}

```kis
pub const fn cut() -> Transition
pub const fn fade(duration: Duration) -> Transition
pub const fn rule(mask: ImageAsset, duration: Duration) -> Transition
pub const fn rule_with(mask: ImageAsset, softness: u8, reverse: bool, duration: Duration) -> Transition
pub const fn masked(mask: Mask, duration: Duration) -> Transition
pub const fn masked_with(mask: Mask, softness: u8, reverse: bool, duration: Duration) -> Transition
pub const fn wipe(direction: Direction, duration: Duration) -> Transition
pub const fn blinds(direction: Direction, count: u32, duration: Duration) -> Transition
pub const fn through(color: Color, duration: Duration, hold: Duration) -> Transition
```

| Constructor | Kind | Effect |
| ----------- | ---- | ------ |
| `cut()` | `Cut`, duration 0 | an immediate switch |
| `fade(d)` | `Crossfade` | a crossfade over `d` |
| `rule(mask, d)` | `Rule`, softness 0, not reversed | a rule (mask) transition with hard edges |
| `rule_with(mask, softness, reverse, d)` | `Rule` | the same, with a blend ramp of `softness` luminance units and direction |
| `masked(mask, d)` | `Masked`, softness 0, not reversed | a transition through a mask shader, with hard edges |
| `masked_with(mask, softness, reverse, d)` | `Masked` | the same, with a blend ramp of `softness` units of the value × 255 and direction |
| `wipe(dir, d)` | `Wipe(dir)` | a wipe moving in `dir` |
| `blinds(dir, count, d)` | `Blinds` | `count` stripes wiping in `dir` |
| `through(color, d, hold)` | `ThroughColor` | the old content fades to `color` over `d / 2`, holds `hold`, then the new content fades in over `d / 2`; the total is `d + hold` |

```kis
use std::screen::CENTER;
use std::screen::anim::masked;
use std::shaders::mask::radial;

const BLINDS_RULE: ImageAsset = asset("rule/blinds.png");

screen.background().change(BG::STATION).apply_animation(rule(BLINDS_RULE, ms(1000)));
screen.background().change(BG::NIGHT).apply_animation(through(BLACK, ms(1000), ms(200)));
screen.background().change(BG::ROOF).apply_animation(masked(radial(CENTER), ms(1000)));
```

### `transition.ease(easing)`, `transition.delay(delay)` {#transition-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Transition
pub const fn delay(mut self, delay: Duration) -> Transition
```

`ease` returns this transition with `easing`; `delay` returns it starting `delay` after the
cursor. Kind: `const fn` · Builtin: no.

## Struct: `MotionKey` {#motionkey}

```kis
pub struct MotionKey {
    pub at: Duration,
    pub offset: Offset,
    pub scale: Decimal,
    pub rotation: Angle,
    pub easing: Easing,
}
```

A key of a motion: a transform relative to the current value, reached at `at`. `easing` is the
easing of the segment that ends at this key.

## Struct: `Motion` {#motion}

```kis
pub struct Motion {
    pub keys: Vec<MotionKey>,
    pub repeat: u32,
    pub delay: Duration,
}
```

A relative path that starts and ends at identity. It is played on an entity
([`Entity::apply_animation`](./screen.md#entity-apply_animation)) or the camera
([`Camera`](./screen.md#camera)), and is how a character jumps, nods or shakes without any change
to its logical state.

- The path starts from identity (offset (0, 0), scale 1.0, rotation 0°) at t = 0. Keys are
  relative to the current value and are applied on top of it.
- Each key's `easing` is the easing of the segment that ends at it.
- One repetition lasts the last key's `at`; the motion plays `repeat` times, starting at
  `cursor + delay`.

**The identity rule.** A motion must end where it started, so it can never change the logical
screen. These are const-evaluation errors:

- no keys;
- an `at` that is 0 or not strictly greater than the previous key's;
- a last key that is not offset (0, 0), scale 1.0 and rotation 0°;
- `repeat` = 0.

### `motion()` {#motion-fn}

```kis
pub const fn motion() -> Motion
```

- Returns: a motion without keys, played once (`repeat: 1`, no delay). Add keys before playing it.
- Kind: `const fn` · Builtin: no.

### `motion.key(at, offset)`, `motion.key_full(at, offset, scale, rotation)` {#motion-key}

```kis
pub const fn key(self, at: Duration, offset: Offset) -> Motion
pub const fn key_full(mut self, at: Duration, offset: Offset, scale: Decimal, rotation: Angle) -> Motion
```

`key` appends a key with scale 1.0, rotation 0° and easing `Linear`; `key_full` appends a key with
the given scale and rotation and easing `Linear`. Kind: `const fn` · Builtin: no.

### `motion.ease(easing)` {#motion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Motion
```

Sets the easing of the segment that ends at the last key. On a motion without keys it does
nothing. Kind: `const fn` · Builtin: no.

### `motion.repeat(times)`, `motion.delay(delay)` {#motion-repeat}

```kis
pub const fn repeat(mut self, times: u32) -> Motion
pub const fn delay(mut self, delay: Duration) -> Motion
```

`repeat` plays this motion `times` times; 0 is an evaluation error. `delay` starts it `delay`
after the cursor. Kind: `const fn` · Builtin: no.

```kis
const fn hop_twice() -> Motion {
    motion()
        .key(ms(150), offset(0, -30)).ease(Easing::OutQuad)
        .key(ms(300), offset(0, 0)).ease(Easing::InQuad)
        .repeat(2)
}
```

## Presets {#presets}

Every preset returns to identity; scale is 1.0 and rotation 0° in every key. Each is `const fn`,
not builtin.

```kis
screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
screen.entity(AZUSA).apply_animation(shake(12, ms(400), 4));
screen.camera().apply_animation(quake(8, 4, ms(600)));
```

### Function: `jump(height, duration)` {#jump}

```kis
pub const fn jump(height: i32, duration: Duration) -> Motion
```

Rises by `height` and lands, over `duration` (repeat 1):

| `at` | offset | easing |
| ---- | ------ | ------ |
| `duration / 2` | (0, −height) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### Function: `nod(depth, duration)` {#nod}

```kis
pub const fn nod(depth: i32, duration: Duration) -> Motion
```

Dips by `depth` and comes back, over `duration` (repeat 1):

| `at` | offset | easing |
| ---- | ------ | ------ |
| `duration / 2` | (0, depth) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### Function: `shake(amplitude, duration, count)` {#shake}

```kis
pub const fn shake(amplitude: i32, duration: Duration, count: u32) -> Motion
```

Shakes sideways by `amplitude`, `count` times over `duration`: one cycle of length
c = `duration / count`, repeated `count` times. `count` = 0 is an evaluation error through
`repeat`.

| `at` | offset | easing |
| ---- | ------ | ------ |
| `c / 4` | (−amplitude, 0) | `Linear` |
| `3c / 4` | (amplitude, 0) | `Linear` |
| `c` | (0, 0) | `Linear` |

### Function: `quake(horizontal, vertical, duration)` {#quake}

```kis
pub const fn quake(horizontal: i32, vertical: i32, duration: Duration) -> Motion
```

Shakes diagonally by `horizontal` and `vertical` every 50 ms, over `duration` (repeat 1, all keys
`Linear`):

| `at` | offset |
| ---- | ------ |
| k · 50 ms, for k = 1 … (duration / 50 ms − 1), k odd | (horizontal, −vertical) |
| k · 50 ms, for k = 1 … (duration / 50 ms − 1), k even | (−horizontal, vertical) |
| `duration` | (0, 0) |
