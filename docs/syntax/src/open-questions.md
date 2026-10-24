# Open Questions

## Numbers

- Precision of `Rat`: its numerator and denominator are `i128` today. Should it be
  arbitrary-precision?
- Conversions from `Rat` and `Decimal` to integers. (Integer conversions are
  [explicit](./numbers.md#conversions); `Rat::to_decimal` exists.)

## Bounded-screen cap

How large may the set of screens at a step grow before the [unbounded-screen
check](./presentation.md#unbounded-screens) rejects the render that grows it? A fixed number, a
setting per project, or a cap per entity and component? The compiler currently uses a fixed 64.

## Game system

[The Game System](./system.md) has one design resolution, fixed by `System::resolution`. Still
open: styling themes shared across modes, and support for several resolutions with a rule for
choosing between them.
