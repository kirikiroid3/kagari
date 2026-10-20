# Open Questions

## Game system

The syntax of `<system>` is in [System Script](./kis/system.md); its semantics are proposed in
the *Kagari STD Book* (`docs/std`). Most of its element and attribute names are still tentative.

## Bytecode header

The magic is `....KAGARI..V000`. What are the bytes shown as `.` (for example non-printable bytes
that detect text-mode transfer corruption, as PNG does)? Is the version only ever `V` plus three
decimal digits? What follows the header?

## Unapplied changes

A layer change without `apply_animation(TransitionAnimation)` does not happen. Is such a dropped
pending change a compile error, a warning, or silently allowed?

## Kirp language subset

Which parts of Rust syntax does `.kirp` have: `let` bindings, `match`, loops, closures, user
functions besides `render`? Loops must keep `render` total.

## Asset binding

How is an `ImageId` constant bound to an image file? A numeric id in `.kix` plus a separate
manifest, or a path in the declaration itself?

## Text markup

What markup does the text of `<m>` support besides plain text: ruby, line breaks, emphasis?

## Speakers

`who` in [`<m>`](./kis/steps.md#m-message) is free text for now, and an empty `who` is narration.
Should `who` name a declared character, a closed set checked at compile time, and how are
characters declared?

## Template hygiene

A template containing a `<label>` produces duplicate labels when used twice in one scene. Should
template labels be renamed per use, forbidden, or left as an error?

## Choice IDs

Choice and arm `id`s are tentatively written by the author: readable in save divergence
reports, but a rename breaks saves. Generated ids survive renames but are unreadable. Which, or
both?

## Keyframe placement

The compiler detects [keyframes](./kirp.md#keyframes) and they are recommended at choice points
and chapter starts. Should they be *required* there, so that loading and jumping always replay a
bounded amount of presentation, or only suggested by a lint?

## Compound conditions

Conditions over several variables are written as a `<match vars="…">` with one pattern per
variable. Is that readable enough for writers, or should there be a small guard expression
language (for example `visited_a and visited_b`) that compiles to the same decision?

## Unbounded-screen analysis

Which arithmetic in `.kirp` does the [unbounded-screen check](./kirp.md#unbounded-screens)
understand precisely (constant offsets, clamps, wrapping), and which does it reject as possibly
unbounded?
