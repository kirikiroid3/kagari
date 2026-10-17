# Relation to the Proposal

The proposal *An Analyzable Model for a Visual Novel Engine* (16 October 2026) fixes the
semantic model and sketches a surface syntax. Kagari adopts the model and replaces the surface
syntax.

## Kept

| Proposal                                                | Kagari                                                        |
| ------------------------------------------------------- | ------------------------------------------------------------- |
| Pushdown control over finite data                       | `<scene>`/`<call>`, finite variable types only               |
| Call/return between scenes, recursion allowed           | `<call>`; a scene returns at its end                          |
| Jumps only within the current scene                     | `<goto>` resolves labels in the current scene only            |
| Every loop passes through an input point                | Every `<goto>` loop must contain a step                       |
| Stable choice identifiers, not line numbers             | `id` on `<choice>` and `<choice-arm>`                         |
| The narrative core never reads presentation state       | No script element reads the screen; `render` cannot read script variables |
| Ambiguous presentation after a merge is a lint          | Compilation stage 4 consistency check                         |

## Changed

| Concern             | Proposal                                                                 | Kagari                                                     |
| ------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------- |
| Script syntax       | Indented, assembly-like text (`scene`, `label`, `jump`, `call`), as a serialization of a node graph | XML (`.kis`)                         |
| Presentation syntax | XML keyframes and deltas                                                 | Rust-like render functions (`.kirp`, `<r>`)               |
| Presentation model  | Full keyframes plus deltas; conditions read narrative state              | `Screen -> Screen` per step; branching via `<match>` in the script |
| Branching           | `if` and `when` conditions                                               | `<match>` only, exhaustive                                 |
| Data                | Booleans, enums, integers in a declared range                            | `bool`, enums, `char` (`u8`)                               |
| Extensibility       | Not addressed                                                            | `.kix` declarations backed by engine or studio code        |

## Why the swap

**The script is a tree of text.** Most of a script is dialogue with some structure around it:
which scene, which branch, which step. XML represents exactly that, mixed text and nested
structure, with no custom grammar. Every tool (text editors, a visual editor, linters, AI agents)
can parse it with an off-the-shelf parser, and structural validity can be checked by schema
before any Kagari-specific analysis runs.

**Presentation is computation.** Presentation is selecting entities, picking images, and
composing animations with parameters. Written as XML, every call becomes an element and every
argument an attribute string, and the vocabulary is fixed by the schema. As typed code, it is
short, the type checker catches errors like a coordinate animation applied to a layer change,
and the vocabulary is open: a studio adds functions by declaring them in `.kix`.

**One owner per decision.** In the proposal, presentation XML could branch on narrative state.
In Kagari, only the script branches, and representation is attached to steps in branches. All
control flow lives in one language, and the analyzer only has to understand one kind of branch.

## What the change costs

The proposal's keyframes declare a complete screen, so the screen at any step can be computed by
looking back to the nearest keyframe. A chain of `Screen -> Screen` functions has no such anchor:
showing an arbitrary step means replaying every render since the start of the run, and the
consistency check must track screens across the whole program. See
[Open Questions](./open-questions.md#keyframes).

{{#include diagrams/keyframes.svg}}
