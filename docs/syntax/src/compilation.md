# Compilation

A game is compiled from one root file into one `.kagari` bytecode file.

## Pipeline

{{#include diagrams/pipeline.svg}}

1. **Parse.** Starting from the root file, the compiler follows `mod` declarations to every `.kis`
   and `.kix` file of the game. Forms are desugared here: `s!` and `choice!` become calls of
   `push_step`, `t!` a text value, a UI form a value (see [Steps and Forms](./narrative/forms.md)
   and [The Game System](./system.md)), so later stages see ordinary code.
2. **Resolve.** Modules, `use` declarations and paths are resolved; every name must exist and be
   visible. Asset paths are checked to exist.
3. **Type and kind check.** Types, `match` exhaustiveness and return values are checked as in
   Rust. On top of that:
   - the [call rules](./kinds.md#call-rules) between `fn`, `pushdown fn` and `const fn`, through
     pointers and closures too;
   - [finite types](./kinds.md#finite-types) for everything a `pushdown fn` holds at runtime;
   - story variables used only from `pushdown fn`;
   - [references and closures](./values.md): mutability, and second-class use in `pushdown fn`;
   - user code declaring no traits and implementing none, `Eq` derived from fields, and the
     explicit [integer conversions](./numbers.md#conversions);
   - `#[scene]` and `#[entry]` rules: steps and scene calls only in scenes; instant procedures
     without recursion or unbounded loops; the arms of each `choice!` being distinct variants of
     one enum; ruby readings matching their base text.
4. **Const evaluation.** Constants are evaluated. `const fn` called from the story with finite
   arguments are [tabulated](./kinds.md#calling-const-fn-from-the-story). Renders are evaluated on
   the screens that reach their steps; renders that ignore their input are marked as
   [keyframes](./presentation.md#keyframes).
5. **Analysis.** Whole-program properties:
   - *Termination.* Every loop and every cycle of calls passes through a step (see
     [Termination](./narrative/scenes.md#termination)).
   - *Stuck choices.* No `choice!` is reachable with every arm's guard false.
   - *Consistency.* The screen at a step does not depend on which path reached it unless the
     author made that dependency explicit (see [Consistency](./presentation.md#consistency)).
   - *Bounded screens.* No render makes the set of screens at a step grow past the cap (see
     [Unbounded screens](./presentation.md#unbounded-screens)).
   - *Reachability and liveness.* Unreachable steps and story variables that never influence
     anything are reported.
   - *System checks.* The [mode-graph checks](./system.md#system-checks).
6. **Emit.** The story, the renders and the system code are compiled into bytecode.

Stages 1–4 are cheap. Stage 5 is where the analyses described in the proposal run.

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
