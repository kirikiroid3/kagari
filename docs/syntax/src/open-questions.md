# Open Questions

## Traits and generics

The standard library uses generics (`Option<T>`, `step<S: Step>`) and the traits `Eq` and `Step`.
May studios declare their own generic functions and traits? In a `pushdown fn`, every type
parameter would have to be instantiated with a finite type, and monomorphization keeps the story a
finite program; in `fn` and `const fn` there is no such constraint.

## Numbers

- Literal syntax for `Rat` and `Decimal`: is `0.5` a `Decimal`, and how is `1/3` written as a
  `Rat`?
- Precision: is `Rat` arbitrary-precision, and how many digits does `Decimal` keep?
- Overflow of Rust integers at runtime in `fn` code: wrap, saturate, or stop with an error?
- Conversions between Rust integers, `Int<lo, hi>`, `Rat` and `Decimal`.

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

## Text whitespace

The [tentative rule](./narrative/elements.md#text) joins lines with a space except between CJK
characters. Is that right for mixed Japanese and English text, or should every line break be
written explicitly?

## Text markup

What markup does message text support besides plain text: ruby, line breaks, emphasis?

## Text interpolation

If message text interpolates a story value, a line is no longer a function of its step, and
backlog entries must carry the interpolated values. Is interpolation needed, and if so, are those
values recorded in the history or recomputed by replay?

## Keyframe placement

The compiler detects [keyframes](./presentation.md#keyframes) and they are recommended at choice
points and chapter starts. Should they be *required* there, so that loading and jumping always
replay a bounded amount of presentation, or only suggested by a lint?

## Bounded-screen cap

How large may the set of screens at a step grow before the [unbounded-screen
check](./presentation.md#unbounded-screens) rejects the render that grows it? A fixed number, a
setting per project, or a cap per entity and component?

## φ policy

When the consistency check fires, should the editor offer to insert a keyframe, to promote a story
variable, or both?

## Game system

[The Game System](./system.md) is a sketch. Its element names, the mode types, the operations, UI
props, styling and focus rules are all open, and so is the design resolution (one fixed resolution
per game, or several with a rule for choosing).

## Diagnostics

Diagnostic codes and their wording are not designed yet.

## Bytecode header

The magic is `....KAGARI..V000`. What are the bytes shown as `.` (for example non-printable bytes
that detect text-mode transfer corruption, as PNG does)? Is the version only ever `V` plus three
decimal digits? What follows the header?
