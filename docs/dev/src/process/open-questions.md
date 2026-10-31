# Open Questions

**Where in the code**

- Fixed limits that are open questions: `SCREEN_CAP` in `core/kagari-lang/src/analysis/story/mod.rs`,
  `EVAL_STEP_BUDGET` and `TABLE_DOMAIN_CAP` in `core/kagari-lang/src/eval/mod.rs`.
- Numbers: `core/kagari-num/src/lib.rs`, `core/kagari-std/kis/std/mod.kis`.
- Gaps found by porting a real game: the root module of the local `example/test_game1`
  (see [Testing](./testing.md#runtime-smoke-test)).

This chapter lists what is not designed or not decided. Each entry says what exists today. When a
question is answered, move the answer into the Reference, the STD Reference or
[Design Decisions](../foundations/decisions.md) and delete the entry here. Studios see none of
this: the Reference marks the affected syntax *Tentative*, and the STD Reference marks items
*Tentative* or *Planned*.

## Language

### Numbers

- **Precision of `Rat`.** Numerator and denominator are `i128`; overflow is a const-evaluation
  error or a VM panic. Should `Rat` be arbitrary-precision?
- **Conversions from `Rat` and `Decimal` to integers.** None exist. Integer conversions are
  explicit (`into`, `saturate`), and `Rat::to_decimal` exists.
- **Saturation warning.** Compound assignment on `Int<lo, hi>` saturates. The design calls for a
  warning when the analysis proves a reachable compound assignment hits a bound; the analysis does
  not report one yet.

### User traits

Only std declares traits (`E1012`, `E1013`, `E1023`). Do studios need their own traits, and how
would they interact with kind checking and tabulation? User generics exist; traits do not.

### Compiler settings that are fixed constants

These limits are compile-time constants marked *tentative* in the code. Should they become
`kagari.toml` settings, like `state-cap` and `recursion-depth`?

| Limit | Value | Diagnostic |
| ----- | ----: | ---------- |
| Const-evaluation step budget (`EVAL_STEP_BUDGET`) | 1 000 000 | `E4003` |
| Argument combinations of a tabulated call (`TABLE_DOMAIN_CAP`) | 65 536 | `E4004` |
| Screens at one step (`SCREEN_CAP`) | 64 | `E5005` |

### Bounded-screen cap

How large may the set of screens at a step grow before the unbounded-screen check rejects the
render that grows it? A fixed number, a setting per project, or a cap per entity and component?
Counted per step (today) or per scene? The check is sound but may reject some bounded programs;
is that acceptable in practice?

### Lints

- A lint that suggests `const` for a `fn` that qualifies (the design never infers `const`).
- When the consistency warning `W5004` fires, should tooling offer to insert a keyframe, to promote
  a story variable, or both? `W5004` already carries a keyframe hint; no editor action exists.

## Story model

- **Timed input.** Are auto-advance, timed choices and quick-time events inputs of the story? One
  proposal: a step whose output is a finite event enum (a selected arm or a timeout), recorded in
  the trace like a choice.
- **Meta layer scope.** The story reads meta only through entry arguments and writes it with
  `Meta::write`. Is that enough for titles that make the meta layer part of the story?
- **Combinatorial choices.** The proposal's shuffle operator (`each_once` with declared
  dependencies) for "talk to each guest once, in any order", and partial-order reduction in the
  analysis. Not designed in the language yet.
- **Contracts.** Assertions on steps and scenes ("this scene requires `MET_B`") checked with
  counterexamples. Not designed.
- **Symbolic analysis.** The analysis is explicit-state with a state cap (`W5006`). Symbolic
  reachability with decision diagrams, per-line reachability conditions and witness generation
  (jumping to a line under a condition) are not started.

## Runtime

- **Snapshots and preview.** Saves are traces only; every load and rewind replays from the entry
  scene with every render. Choice-point snapshots, keyframe-bounded replay and preview at the
  cursor are planned (see [Relation to the Proposal](../foundations/proposal.md#what-preview-costs)).
  Scene hot swap exists for debugging ([The Debugger](../runtime/debugger.md#hot-swap)); it keeps
  in-memory call snapshots only and does not keep menus open over the game.
- **VM limits.** A heap cap, and cooperative scheduling where a run that exhausts its instruction
  budget yields instead of trapping (needed for a browser runtime).
- **Extension types.** Studio-defined screen data with native engine systems, identified by path
  and structural hash. Planned only; the std component set is fixed today (see
  [Design Decisions](../foundations/decisions.md#extensions)).

## Standard library and game system

These are known gaps of `std`; none is designed yet.

- **NVL mode.** A full-screen text mode where several lines stay on the screen until a page break.
  It changes what the text window shows and how the backlog and `Narrative::line` describe a step.
- **Layered sprites.** Auto-blink and lip sync on layered character sprites: parts that animate on
  their own, and lip sync driven by the voice.
- **Syncing text and animation.** Driving animation from the text reveal, and the reverse, beyond
  `at_mark`: an animation that follows each revealed character, or a reveal that waits for an
  animation.
- **Audio in system modes.** `system::set_music` plays music outside the game, but what is playing
  is not exposed: the current track, a music room's playback state.
- **Resolutions and themes.** A game has one design resolution, `System::resolution`. Open: styling
  themes shared across modes, and several resolutions with a rule for choosing between them.

The TyranoScript tutorial port (`example/test_game1`, local only) found these further gaps:

- Renders cannot pass presentation state to the game view: moving or restyling the message
  window, vertical or full-screen text, a default font change mid-story, swapping the menu button
  for a button bar, and link-style choices inside the window (`Menu` has only `align`).
- Opening a URL from the game.
- A text-reveal preview outside the game view (a text-speed sample in a settings page).
- A slide (push) transition.
- Hiding the text window from a UI button in step with the `Action::HideWindow` binding.
- Bindings to arbitrary `fn`s (for example a fullscreen toggle) and touch input.
- Fades and transitions of UI elements.

## Answered since the proposal

These were open in the proposal and are now decided; they are listed so they are not reopened by
accident.

| Question | Answer | Where |
| -------- | ------ | ----- |
| Default range for counters | None: every `Int` states its bounds; no prelude alias | Reference, *Numbers* |
| Unbounded recursion or a depth cap | Recursion between scenes is allowed and analyzed; run-time depth is capped by `recursion_depth` (default 2048) | [Design Decisions](../foundations/decisions.md#pushdown) |
| Literal syntax for Rust integers, `Rat`, `Decimal` | Integer suffixes (`5u8`); `0.5` is a `Decimal`; no `Rat` literal, `Rat::new(num, den)` | Reference, *Numbers* |
| Overflow at run time | A VM panic; compound assignment on `Int<lo, hi>` saturates | [Design Decisions](../foundations/decisions.md#numbers) |
| Precision of `Decimal` division | Exact, then truncated to 7 digits after the point | `kagari-num` |
| Text interpolation | `{expr}` in `t![…]` for `T: ToText` (`Text`, `String`, `Int<lo, hi>`) | STD Reference, `std::narrative::text` |
| Randomness | `rng_choose::<T>()` in scenes only, xoroshiro128++ seeded from the trace | STD Reference, `std::narrative` |
