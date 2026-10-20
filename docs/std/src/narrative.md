# The Narrative Interface (`std/narrative.kix`)

The system sees the story only through the abstract type `Narrative`. Its representation
(position, call stack, σ, screen, trace) is hidden: system code is parametric in it, so it cannot
read story variables, and nothing the system does can affect the narrative analyses.

```kix
// std/narrative.kix — system scope only. Tentative names.

fn current_line(n: &Narrative) -> Line;                  // the current <s>, see Backlog
fn choices(n: &Narrative) -> Choices;                    // arms offered at the current <choice>
fn history(n: &Narrative) -> History;                    // steps passed, see Backlog
fn screen(n: &Narrative) -> &Screen;                     // drawn by <game-screen>, read by the HUD

fn new(e: Entry) -> Narrative;
fn step(n: Narrative, i: Input) -> Narrative;            // Input = NextStep | Arm(ArmId)
fn snapshot(n: &Narrative) -> Save;
fn restore(s: Save) -> Result<Narrative, Divergence>;
fn rewind(n: &Narrative, entry: u64) -> Narrative;
```

Unlike other `.kix` functions, these are engine built-ins without bodies: they run the
narrative at runtime. They are listed in a `.kix` file so that system code is type-checked
against them.

## A save is a witness of reachability

A save is the proposal's save format: a **trace** (stable choice ids, the step count after the
last choice, the script hash and the PRNG seed) and a **snapshot** used as a cache.

The trace is a path from the start. So a save is a *witness that a position is reachable*, not a
state: no save of an unreachable position can be constructed. This is exactly the requirement
that loading can jump to any position the player can reach, and only to those.

When the script changes, `restore` re-checks the witness against the new program by replaying it,
and returns a `Divergence` at the first choice id that no longer resolves, as the proposal
describes.

Because `snapshot` takes only a `&Narrative`, a save contains no settings, no meta and no system
frames.

## Rewind

`rewind(n, i)` restores the prefix of `n`'s trace up to history entry `i`. It is `restore` on a
truncated trace, so it needs no machinery of its own. It replays from the nearest choice snapshot
before `i` and runs renders from the last keyframe on the path, so it costs no more than a load.
See [Revert to here](./backlog.md#revert-to-here).

## Overlays and animations

The narrative only changes on `step`, `restore` and `rewind`. While an overlay is on top of
`Game`, no input reaches it, so it is paused.

The screen's running animations are not paused but **skipped**: when any page or dialog is pushed
over `Game`, every running animation jumps to its final state. This is well defined because the
final state of an animation is always deterministic:

- A finite animation ends in the state its last frame describes.
- An animation that loops forever has as its final state the state *before* the loop: the end of
  its finite prefix.

The same rule serves fast-forward: skipping through steps skips each step's animations to their
final states, so the screen after a skip is exactly the screen after watching. It also matches the
syntax book, where the logical screen of a step is the screen after `render`, not the frames the
engine draws while animating.

*Tentative:* when the overlay is popped, an infinite loop starts again from the beginning of its
loop; finite animations stay finished.
