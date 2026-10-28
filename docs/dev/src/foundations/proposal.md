# Relation to the Proposal

**Where in the code**

- The proposal: `../proposal/vn-engine/proposal2/vn-engine-proposal.typ` (English) and
  `../proposal/vn-engine/proposal2-jp/vn-engine-proposal.typ` (Japanese), outside this repository.
- Its model in the compiler: `core/kagari-lang/src/typeck/kinds*` (kinds, finite types),
  `core/kagari-lang/src/analysis` (stage 5), `core/kagari-lang/src/eval` (renders, keyframes,
  tabulation).
- Its save format in the engine: `runtime_libs/kagari-engine/src/saves`.

The proposal *An Analyzable Model for a Visual Novel Engine* (22 October 2026 revision) fixes the
semantic model of Kagari and already uses its syntax. Its Japanese edition is the tie-breaker for
Japanese terminology in the Reference and the STD Reference. This chapter maps the proposal's
concepts to the language and the code, records what changed from the first design, and lists
which parts of the proposal's plan exist.

## The model in the language

| Proposal | Language | Code |
| -------- | -------- | ---- |
| Pushdown control over finite data | `pushdown fn` holding only finite types | `E3008`; `typeck/kinds/finite.rs` |
| Call/return between scenes, recursion allowed | Calls of `#[scene]` functions; each scene is a component of the checked state machine | Scene summaries per context in `analysis/story` |
| Jumps only within the current scene | Structured control flow; no `goto` at all | No `goto` in the grammar |
| Every loop and call cycle passes through an input point | Termination, checked interprocedurally | `E5001`, `E5002`; `analysis/story/termination.rs` |
| Integers in a declared range | `Int<lo, hi>`, saturating compound assignment | `ICLAMP` in the bytecode |
| Computed call over a finite enum of scenes | `scene fn` pointers: there are finitely many named functions | Function pointers are finite types |
| No closures in the narrative core | Closures in `pushdown fn` are second-class | `E3015` |
| Input points | Steps: `push_step`, written `s!(…)` and `choice!{…}`, only in scenes | `E3002`; `STEP` instruction |
| Stable choice identifiers, not line numbers | Function path + choice `id` + variant of an author-declared enum | `saves/trace.rs` (`choice_id`, `arm_id`) |
| The narrative core never reads presentation state | `Screen` is not a finite type; no `pushdown fn` can hold one | `E3008` |
| Presentation never reads σ | Renders are `const fn(Screen) -> Screen` passed as function pointers | `E2036` for a capturing closure where a pointer is needed |
| Keyframes plus deltas | A render that ignores its input is a keyframe; any other render is a delta | Keyframe detection in stage 4 (`eval/renders.rs`); placement check `W5011` (opt-in) |
| Ambiguous presentation after a merge is a lint | Consistency, stage 5 | `W5004` |
| Bounded screens by evaluation with a cap | A render that grows a step's set of screens past the cap | `E5005`, cap 64 |
| Liveness: dead flags are projected out | Exploration keeps only locals still used; story variables that never influence the story are reported | `mir/liveness.rs`; `W5010` (`analysis/influence.rs`) |
| Unreachable lines | Unreachable scenes and steps | `W5008`, `W5009` |
| Meta layer | Read through entry arguments, written with `Meta::write` | `Meta::write` is the std's only `pushdown fn` besides `push_step` and `rng_choose` |
| Save = trace + snapshot | A save is a witness of reachability | Trace implemented; snapshot planned (see below) |

`E2036` is `TypeCapturingClosure`: a closure that captures cannot coerce to a function pointer,
which is what keeps renders blind to the story.

## What changed from the first design

The first design (repository history up to 2026-10-21) had three file kinds and two scopes:

| First design | Now |
| ------------ | --- |
| `.kis`: XML story script, plus a separate XML system scope (`<system>`, `<sysfn>`) | `.kis`/`.kix`: one Rust-like language; story and system are functions of different kinds |
| `.kirp`: Rust-like render files, `<r>` inline renders | `const fn` renders, inline as non-capturing closures |
| `.kix`: Rust-like extension definitions | Ordinary modules; `.kix` now means "with forms" (`s!`, `choice!`, `t!`, UI forms) |
| `<scene>`, `<call>`, `<return>`, `<goto>`, `<label>` | `#[scene] pushdown fn`, calls, `return`, loops |
| `<match var>` / `<match vars>`, no `if` | Rust's `if` and `match` over any finite expression |
| `<var-*>` declarations, `<set>`, `<add>`, `<sub>` | `static mut`, `let mut`, assignment, saturating `+=` |
| `<choice-arm>` with code bodies | Arms `t![…] => V` as data; the choice returns a variant, then `match` |
| `<template>`, `<include>` | Functions and modules |
| `from-meta`, `<meta-set>` | Entry arguments, `Meta::write` |
| System functions, `<on-exit>`, handler attribute strings | `fn` code, an exit handler closure, closures in UI forms |
| `<entry>` element (replaced by entry scenes on 2026-10-20) | `#[entry]` on a `#[scene]` |

The restart (commit `c0bafaa`, 2026-10-22) dropped all crate sources and kept only the directory
layout. Its first draft wrote steps as JSX-style elements; the same day they became the fixed forms
`s!(…)`, `choice!{…}` and `t![…]` (commit `1eb20c2`), and `#[scene]` became the unit of model
checking, separating scenes from instant procedures.

### Why one language with kinds

In the first design the pushdown restriction was a property of a *file format and a scope*: XML
`<game>` content was pushdown, `<system>` content followed other rules, and `.kirp`/`.kix` code
followed a third set. Every rule had to be stated per scope, and code could not be shared across
them.

The restriction is now a property of *functions*. One rule, the call rules between kinds, keeps
the story closed, and everything else, including the system, is ordinary code. The analyses of
the proposal need exactly what that rule guarantees: everything that runs from the start of the
story to any step is a `pushdown fn` or a tabulated `const fn`. See
[One language with function kinds](./decisions.md#one-language).

### What the XML gave up

The first design chose XML because every tool could parse it, a schema could check it, and a node
editor could round-trip it. The story is now code:

- Tools need Kagari's parser. The repository ships it as a library (`kagari-syntax`), a language
  server (`kagari lsp`) and a tree-sitter grammar (`tooling/tree-sitter-kagari`) for editors.
- A node editor can still show scenes as graphs (each scene is a component of a recursive state
  machine, each call of a scene a box), but it can no longer edit every construct in place. The
  text is canonical in version control. No node editor exists.
- The text still diffs and merges as text, and `kagari fmt` gives it one canonical layout.

## The proposal's phased plan, today

| Phase | Proposal deliverable | State in the code |
| ----- | -------------------- | ----------------- |
| 0 | Parser for `.kis` and `.kix`; type and kind checker; narrative IR (an RSM) and interpreter; trace saves with snapshot cache | Done, except the snapshot cache. The narrative IR is MIR; the interpreter is the VM's story fiber |
| 1 | Const evaluation of renders over the ECS screen; keyframe detection; choice-point snapshots with the screen; hot-reload preview | Const evaluation and keyframe detection done. No snapshots, no preview |
| 2 | Explicit-state analyzer for small scripts; unreachable, ambiguity, liveness lints | Done (`W5004`, `W5008`–`W5010`), with a state cap (`W5006`) |
| 3 | Symbolic reachability (MDDs); per-line reachability conditions; witness jump; contracts with counterexamples | Not started |
| 4 | Incremental query engine for live analysis; `each_once` with declared dependencies; partial-order reduction | The compiler is an incremental salsa database with per-item queries, used by the language server. `each_once` and partial-order reduction not started |

The remaining items are tracked in [Open Questions](../process/open-questions.md).

## What preview costs

The proposal's preview design, still planned:

- **Editing at the cursor.** The engine snapshots σ and the screen at each choice. Preview
  restores the nearest snapshot before the cursor and runs the steps after it, with their renders
  but without animations.
- **Loading a save on a changed script, or jumping to a step with no snapshot.** The engine
  replays the story from the start, but runs renders only from the last keyframe on that path.
  Without a keyframe on the path, every render since the start runs.
- **Editing a render.** Snapshots after the first step that uses it on the current trace are
  invalidated and rebuilt by replay. If a recorded choice no longer exists, preview stops there and
  highlights it.

What the runtime does today:

- **Loading a save** (`nav::load`) and **rewinding** (`Narrative::rewind`) both replay the trace
  from the entry scene into a fresh game frame, running every step and every render, without
  audio, timeline ticks, read flags, seen sets, meta writes or autosaves. A replay stops at
  1 000 000 steps (`Divergence::TooLong`), at the first choice ID that no longer resolves, or at a
  VM panic, and leaves the running game unchanged.
- There are no snapshots, so the cost of a load grows with the length of the trace. Keyframes are
  detected by the compiler but not yet used to shorten replay.

See [Saves, Traces and Snapshots](../runtime/saves.md) for the implementation.
