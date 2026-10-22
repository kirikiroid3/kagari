# Relation to the Proposal

The proposal *An Analyzable Model for a Visual Novel Engine* (22 October 2026 revision) fixes the
semantic model and uses the syntax of this book. This chapter maps its concepts to the language
and records what changed from the first design.

## The model in the language

| Proposal | Language |
| -------- | -------- |
| Pushdown control over finite data | `pushdown fn` holding only [finite types](./kinds.md#finite-types) |
| Call/return between scenes, recursion allowed | Calls of `pushdown fn`, plain or as [scene tags](./narrative/scenes.md#scene-tags) |
| Jumps only within the current scene | Structured control flow; no `goto` at all |
| Every loop and call cycle passes through an input point | [Termination](./narrative/scenes.md#termination), checked interprocedurally |
| Integers in a declared range | `Int<lo, hi>`, saturating compound assignment |
| Computed call over a finite enum of scenes | `pushdown fn` pointers: there are finitely many named functions |
| No closures in the narrative core | Closures in `pushdown fn` are [second-class](./values.md#closures) |
| Input points | Steps: [`step`](./narrative/elements.md#elements-are-sugar), written `<s>` and `<choice>` |
| Stable choice identifiers, not line numbers | Function path + choice `id` + variant of an author-declared enum |
| The narrative core never reads presentation state | `Screen` is not a finite type; no `pushdown fn` can hold one |
| Presentation never reads σ | Renders are `const fn(Screen) -> Screen` passed as function pointers |
| Keyframes plus deltas | A render that ignores its input is a keyframe; any other render is a delta |
| Ambiguous presentation after a merge is a lint | [Consistency](./presentation.md#consistency) in compilation stage 5 |
| Meta layer | Read through [entry arguments](./narrative/scenes.md#arguments), written with `Meta::write` |
| Save = trace + snapshot | [A save is a witness of reachability](./system.md#a-save-is-a-witness-of-reachability) |

## What changed from the first design

The first design had three file kinds and two scopes:

| First design | Now |
| ------------ | --- |
| `.kis`: XML story script, plus a separate XML system scope (`<system>`, `<sysfn>`) | `.kis`/`.kix`: one Rust-like language; story and system are functions of different kinds |
| `.kirp`: Rust-like render files, `<r>` inline renders | `const fn` renders, inline as non-capturing closures |
| `.kix`: Rust-like extension definitions | Ordinary modules; `.kix` now means "with elements" |
| `<scene>`, `<call>`, `<return>`, `<goto>`, `<label>` | `pushdown fn`, calls and scene tags, `return`, loops |
| `<match var>` / `<match vars>`, no `if` | Rust's `if` and `match` over any finite expression |
| `<var-*>` declarations, `<set>`, `<add>`, `<sub>` | `static mut`, `let mut`, assignment, saturating `+=` |
| `<choice-arm>` with code bodies | `<arm>` as data; the choice returns a variant, then `match` |
| `<template>`, `<include>` | Functions and modules |
| `from-meta`, `<meta-set>` | Entry arguments, `Meta::write` |
| System functions, `<on-exit>`, handler attribute strings | `fn` code, an exit handler closure, closures in UI elements |

### Why one language with kinds

In the first design the pushdown restriction was a property of a *file format and a scope*: XML
`<game>` content was pushdown, `<system>` content followed other rules, and `.kirp`/`.kix` code
followed a third set. Every rule had to be stated per scope, and code could not be shared across
them.

The restriction is now a property of *functions*. One rule (the [call
rules](./kinds.md#call-rules)) keeps the story closed, and everything else, including the system,
is ordinary code. The analyses of the proposal need exactly what that rule guarantees: everything
that runs from the start of the story to any step is a `pushdown fn` or a tabulated `const fn`.

### What the XML gave up

The first design chose XML because every tool could parse it and a schema could check it, and
because a node editor could round-trip it. The story is now code: tools need Kagari's parser, and a
node editor can still show scenes as graphs (each scene is a component of a recursive state
machine, each scene tag a box) but can no longer edit every construct in place. The text still
diffs and merges as text.

## What preview costs

- **Editing at the cursor.** The engine snapshots σ and the screen at each choice. Preview
  restores the nearest snapshot before the cursor and runs the steps after it, with their renders
  but without animations.
- **Loading a save on a changed script, or jumping to a step with no snapshot.** The engine
  replays the story from the start, but runs renders only from the last
  [keyframe](./presentation.md#keyframes) on that path. Without a keyframe on the path, every
  render since the start runs.
- **Editing a render.** Snapshots after the first step that uses it on the current trace are
  invalidated and rebuilt by replay.
