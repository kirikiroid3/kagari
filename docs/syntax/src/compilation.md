# Compilation

A game is compiled from one entry `.kis` file into one `.kagari` bytecode file.

## Pipeline

{{#include diagrams/pipeline.svg}}

1. **Collect.** Starting from the entry script, the compiler chases
   `<include src="foo.kis" />`, `<import src="bar.kix" />` and `<represent src="baz.kirp" />`
   to obtain the full program text. Includes are spliced in place; imports are hoisted; represent
   sources are loaded for later checking. *Tentative:* template uses (`<x tpl="…">`) are
   expanded at the end of this stage, so every later stage sees plain script.
2. **Structure check.** The script must be well structured: well-formed XML, tags nested where
   they are allowed, required attributes present, label and scene names unique, every
   `<goto>` and `<call>` target defined, every `<set>` value of the variable's type, every
   `<match>` exhaustive, and every `<goto>` loop and every cycle of calls passing through a
   step (see [Termination](./kis/control-flow.md#termination)).
3. **Represent check.** Every `.kirp` file and every `<r>` block is resolved and type-checked
   against the imported `.kix` definitions, ensuring each represent function is legitimate:
   names exist, argument and return types match, and the result is a `Screen`. Since renders
   read only constants and their input screen, each one, together with the `.kix` functions it
   calls, is evaluated here to a `Screen -> Screen` function known at compile time. Renders that
   ignore their input are marked as [keyframes](./kirp.md#keyframes).
4. **High-level checks.** Whole-program properties:
   - *Consistency.* The screen at a step must not depend on which path reached it unless the
     author made that dependency explicit (see [Consistency](./kirp.md#consistency)). The
     analysis tracks, per step, the possible values of each entity's components, with one
     summary per scene.
   - *Bounded screens.* A render whose read-modify-write could make the set of screens at a step
     unbounded is an error (see [Unbounded screens](./kirp.md#unbounded-screens)). Error recovery
     leaves that render out so the rest of the program is still checked.
5. **Emit.** The script and the represent functions are compiled into bytecode.

Stages 1–3 are cheap. Stage 4 is where the analyses described in the proposal
(reachability, presentation ambiguity, liveness) run.

## `.kagari` header

A `.kagari` file starts with 16 magic bytes:

```text
....KAGARI..V000
```

| Offset | Length | Content          | Meaning                                       |
| -----: | -----: | ---------------- | --------------------------------------------- |
|      0 |      4 | `....`           | Not fixed yet                                 |
|      4 |      6 | `KAGARI` (ASCII) | Format identifier                             |
|     10 |      2 | `..`             | Not fixed yet                                 |
|     12 |      4 | `V000` (ASCII)   | Bytecode version: `V` and three decimal digits |

The bytes shown as `.` and the layout after the header are not specified yet; see
[Open Questions](./open-questions.md#bytecode-header).
