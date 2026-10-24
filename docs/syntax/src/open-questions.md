# Open Questions

## Numbers

- Literal syntax for `Rat`: how is `1/3` written? (`0.5` is a `Decimal`.)
- Precision: is `Rat` arbitrary-precision, and how many digits does `Decimal` keep?
- Overflow of Rust integers at runtime in `fn` code: wrap, saturate, or stop with an error?
- Conversions between integers and `Rat` or `Decimal`, and between `Rat` and `Decimal`. (Integer
  conversions are [explicit](./numbers.md#conversions).)

## Default integer ranges

Bounded integers saturate, with a warning when the analyzer proves a bound is reached. Should the
standard library suggest default ranges for common counters such as affection?

## Recursion depth

Allow unbounded recursion (full pushdown) or cap the stack depth (a finite automaton, simpler
analysis)? No current use case needs unbounded recursion.

## Timed input

Are auto-advance, timed choices and quick-time events input events of the story? A natural form is
a step that returns a finite enum of events, recorded in the trace like a choice.

## Randomness

The proposal keeps a deterministic PRNG whose state is in σ. What is the API: a `pushdown fn` that
draws an `Int<lo, hi>`? The analyzer would treat each draw as a free choice of a value in the
range.

## Text interpolation

An [interpolated value](./narrative/forms.md#t-text) makes a line depend on story state, so
backlog entries must carry the interpolated values. Are those values recorded in the history, or
recomputed by replay?

## Keyframe placement

The compiler detects [keyframes](./presentation.md#keyframes) and they are recommended at choice
points and chapter starts. Should they be *required* there, so that loading and jumping always
replay a bounded amount of presentation, or only suggested by a lint?

## Bounded-screen cap

How large may the set of screens at a step grow before the [unbounded-screen
check](./presentation.md#unbounded-screens) rejects the render that grows it? A fixed number, a
setting per project, or a cap per entity and component? The compiler currently uses a fixed 64.

## State cap

The [analysis](./compilation.md#how-the-analysis-runs) explores every state of the story and gives
up past a fixed number of states (currently 200,000), with a warning; the reachability claims are
then skipped. Should the cap be a project setting, and should a story over the cap be an error
instead, since its unreachable steps go unreported?

## φ policy

When the consistency check fires, should the editor offer to insert a keyframe, to promote a story
variable, or both?

## Game system

[The Game System](./system.md) has one design resolution, fixed by `System::resolution`. Still
open: styling themes shared across modes, and support for several resolutions with a rule for
choosing between them.

## Diagnostics

Diagnostic codes and their wording are not designed yet.

## Bytecode header

The magic is `....KAGARI..V000`. What are the bytes shown as `.` (for example non-printable bytes
that detect text-mode transfer corruption, as PNG does)? Is the version only ever `V` plus three
decimal digits? What follows the header?
