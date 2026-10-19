# Relation to the Proposal

The proposal *An Analyzable Model for a Visual Novel Engine* fixes the semantic model and sketches
a surface syntax. Kagari adopts the model and replaces the surface syntax. The proposal has since
been revised to take the XML script and render functions as its syntax, so the two agree; this
chapter records what changed and why.

## Kept

| Proposal                                                | Kagari                                                        |
| ------------------------------------------------------- | ------------------------------------------------------------- |
| Pushdown control over finite data                       | `<scene>`/`<call>`, finite variable types only               |
| Call/return between scenes, recursion allowed           | `<call>`; a scene returns at its end or at `<return>`         |
| Jumps only within the current scene                     | `<goto>` resolves labels in the current scene only            |
| Every loop and call cycle passes through an input point | Checked by the compiler, interprocedurally                    |
| Integers in a declared range                            | `<var-int min max>`, saturating `<add>`/`<sub>`               |
| Stable choice identifiers, not line numbers             | `id` on `<choice>` and `<choice-arm>`                         |
| The narrative core never reads presentation state       | No script element reads the screen; `render` cannot read script variables |
| Keyframes plus deltas                                   | A render that ignores its input is a keyframe; any other render is a delta |
| Ambiguous presentation after a merge is a lint          | Compilation stage 4 consistency check                         |

## Changed

| Concern             | Original proposal                                                        | Kagari                                                     |
| ------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------- |
| Script syntax       | Indented, assembly-like text (`scene`, `label`, `jump`, `call`), as a serialization of a node graph | XML (`.kis`), canonical; a node editor is a view of it |
| Presentation syntax | XML keyframes and deltas                                                 | Rust-like render functions (`.kirp`, `<r>`) over an ECS screen |
| Presentation reads story state | Keyframes contain conditions on narrative state               | Render functions are blind to it; branching via `<match>` in the script |
| Branching           | `if` and `when` conditions                                               | `<match>` only, exhaustive, with ranges and several variables |
| Extensibility       | Not addressed                                                            | `.kix` types and functions, evaluated at compile time      |

## Why the swap

**The script is a tree of text.** Most of a script is dialogue with some structure around it:
which scene, which branch, which step. XML represents exactly that, mixed text and nested
structure, with no custom grammar. Every tool (text editors, a visual editor, linters, AI agents)
can parse it with an off-the-shelf parser, and structural validity can be checked by schema
before any Kagari-specific analysis runs. It also diffs and merges as text, which a node graph
does not.

**Presentation is computation.** Presentation is selecting entities, picking images, and
composing animations with parameters. Written as XML, every call becomes an element and every
argument an attribute string, and the vocabulary is fixed by the schema. As typed code, it is
short, the type checker catches errors like a coordinate animation applied to a layer change,
and the vocabulary is open: a studio adds components and functions in `.kix`.

**One owner per decision.** In the proposal, presentation XML could branch on narrative state.
In Kagari, only the script branches, and representation is attached to steps in branches. All
control flow lives in one language, and the analyzer only has to understand one kind of branch.

## Why the models are equivalent

Render functions read only constants and their input screen; the screen is an ECS world whose
components implement `Eq`; every `.kix` function has a body. So every render is a
`Screen -> Screen` function known at compile time, and screens can be compared. A render is the
proposal's delta, and a render whose output ignores its input is its keyframe. Renders that could
make the set of screens at a step unbounded are rejected, so the consistency check is finite.

Neither model makes the screen a function of the position alone: the screen at a step is the
composition of the renders on the path that reached it, which is why both need the consistency
check.

## What preview costs

{{#include diagrams/keyframes.svg}}

- **Editing at the cursor.** The engine snapshots the story state and the screen at each choice.
  Preview restores the nearest snapshot before the cursor and runs the steps after it, with
  their renders but without animations. This is the same order of work as the proposal's.
- **Loading a save on a changed script, or jumping to a step with no snapshot.** The engine
  replays the story from the start, but runs renders only from the last keyframe on that path.
  Without a keyframe on the path, every render since the start runs.
- **Editing a render.** Snapshots after the first step that uses it on the current trace are
  invalidated and rebuilt by replay.
