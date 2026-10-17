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

## Keyframes

`Screen -> Screen` render functions have no equivalent of the proposal's keyframes. Without a step
that declares the complete screen, previewing or loading at an arbitrary step requires running
every render since the start, and the consistency check must carry screens through the whole
program. Should there be a way to declare a full screen (for example a render that starts from an
empty screen), and should scene entry or choice points be natural places for it?

## Path-dependent presentation

When the [consistency check](./kirp.md#consistency) reports a step whose screen depends on the
path, and the dependency is intended, how does the author say so? Options include an attribute on
the step, or requiring the dependency to be expressed through a variable and `<match>`.

## Unapplied changes

A layer change without `apply_animation(TransitionAnimation)` does not happen. Is such a dropped
pending change a compile error, a warning, or silently allowed?

## Kirp language subset

Which parts of Rust syntax does `.kirp` have: `let` bindings, `match`, loops, closures, user
functions besides `render`? Loops must keep `render` total.

## Kix implementations

`.kix` declares functions without bodies. How is a declaration bound to its implementation for
engine presets and for studio extensions: a native plugin interface, built-in primitives only, or
bodies written in `.kirp` syntax that compose primitives?

## Asset binding

How is an `ImageId` constant bound to an image file? A numeric id in `.kix` plus a separate
manifest, or a path in the declaration itself?

## Conditional choice arms

There is no `if`, and `<choice-arm>` has no condition. How is an arm offered only when a condition
holds (for example "Leave" only after all stalls were visited)? Wrapping arms in `<match>` inside
`<choice>` is one option; an attribute on `<choice-arm>` that names an enum or bool value is
another.

## Arithmetic on char

`<set>` only assigns literals. Affection counters need increments. Should `<set>` support
arithmetic, and on overflow does `char` saturate, wrap, or fail the build when the analyzer can
prove overflow is reachable?

## Text markup

What markup does the text of `<s>` support: speaker name, ruby, line breaks, emphasis, voice?

## Template hygiene

A template containing a `<label>` produces duplicate labels when used twice in one scene. Should
template labels be renamed per use, forbidden, or left as an error?

## Choice IDs

Choice and arm `id`s are tentatively written by the author: readable in save divergence
reports, but a rename breaks saves. Generated ids survive renames but are unreadable. Which, or
both?
