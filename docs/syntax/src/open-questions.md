# Open Questions

## Game system

The pushdown machine is strong enough to build the game system on top of it: title screen,
settings, save and load, gallery, read-text tracking. How to design that system is not decided.
Questions it has to answer:

- Are system screens written as `.kis` scenes, or are they a separate layer outside the story?
- Where does system state live? Settings and unlocks persist across runs and must not become
  part of the story's narrative state, or every analysis would have to consider them. The
  proposal's *meta layer* (state read at the start of a run and written at defined points) is
  one candidate.
- If a save menu is a scene called on top of the story, the saved call stack must exclude the
  menu's own frames, and loading must replace the story stack rather than push onto it.
- Some games make the meta layer part of the story on purpose. Which meta fields may the script
  read, and at which points may it write them?

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

What markup does the text of `<s>` support: speaker name, ruby, line breaks, emphasis, voice?

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
