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
   arguments are [tabulated](./kinds.md#calling-const-fn-from-the-story). Every render a step
   uses is evaluated once on an opaque input screen: a render that finishes without reading its
   input is marked as a [keyframe](./presentation.md#keyframes); any other render is evaluated
   again on `Screen::empty()`, so its evaluation errors are reported.
5. **Analysis.** Whole-program properties:
   - *Termination.* Every loop and every cycle of calls passes through a step (see
     [Termination](./narrative/scenes.md#termination)).
   - *Stuck choices.* No `choice!` is reachable with every arm's guard false.
   - *Consistency.* Renders are evaluated on the screens that reach their steps, and the screen
     at a step does not depend on which path reached it unless the author made that dependency
     explicit (see [Consistency](./presentation.md#consistency)).
   - *Bounded screens.* No render makes the set of screens at a step grow past the cap (see
     [Unbounded screens](./presentation.md#unbounded-screens)).
   - *Reachability and liveness.* Unreachable steps and story variables that never influence
     anything are reported.
   - *System checks.* The [mode-graph checks](./system.md#system-checks).
6. **Emit.** The story, the renders and the system code are compiled into
   [bytecode](./bytecode.md), which is verified before it is written. Only the functions reachable
   from `fn main`, the entry scenes and the statics (by calls or as function values) are emitted.
   Emit diagnostics are `E7xxx`: a root file without `fn main()` is `E7001`.

Stages 1–4 are cheap. Stage 5 is where the analyses described in the proposal run.

### How the analysis runs

Stage 5 runs only on a program that stages 1–4 accepted without errors: it executes the story,
which needs well-typed, well-kinded code.

**Termination** is checked first, on the control flow of each scene alone: every branch counts
as possible. A program with a stepless loop or a stepless call cycle is not analyzed further,
since running it could never reach the next step.

**The states of the story** are then computed exactly. The story is a recursive state machine
over finite data, and the compiler explores it from every entry scene and every value of its
arguments. A state is a position in a scene, the locals of the scene that are still used later,
and the values of the story variables; instant code between two steps runs in the const
evaluator. Each scene is summarized per *context*, its arguments and the story variables when it
is entered, as the set of values and story variables it can return with, so a scene called from
many places in the same state is explored once. The exploration has a cap on the number of
states: the state cap (default 200 000, `--state-cap`). A story with more states, or an entry scene
whose arguments take more values than the [tabulation](./kinds.md#calling-const-fn-from-the-story)
limit, gets the warning `W5006`: what was found is still reported, but the
analyses that need every state (unreachable steps and scenes) are skipped. So is a path the
analysis cannot follow, such as one that holds a closure capturing locals across a step; it is
reported where it stops (`W5007`, a warning).

On these states:

- a step that is a `choice!` with no arm shown is a *stuck choice*, reported with the story
  variables of one such state;
- an evaluation error in story code (an index out of range, for example) is reported where the
  story runs into it;
- a scene no entry scene reaches, and a step of a reached scene that no state reaches, are
  reported as unreachable.

**Screens** are computed over the same states, since renders never influence the story. The
story starts on `Screen::empty()`; a step's render runs on every screen that reaches the step,
and its errors on any of them are reported. The screens then decide
[consistency](./presentation.md#consistency) and [bounded
screens](./presentation.md#unbounded-screens).

**Liveness** is a dependence analysis over every `pushdown fn`: a story variable *influences*
the story when its value can flow, through locals, parameters, return values and other story
variables, into a condition, a `match` scrutinee or guard, a `for` iterable, a step, a meta
write, a call through a pointer or closure, or the ending of an entry scene. Every other story
variable is reported: never read, read only into variables that do not influence the story, or
never used at all.

## Compiler options

| Option | Default | Meaning |
| ------ | ------: | ------- |
| `state_cap` (`--state-cap <n>`) | 200 000 | Most story states the analysis explores (`W5006` past it) |
| `recursion_depth` (`--recursion-depth <n>`) | 2048 | Most nested calls at run time, counting every call frame (`fn`, closures, scenes, instant procedures); deeper is a [VM panic](./bytecode.md#panics). Written into the bytecode |
| `keyframe_check` (`--keyframe-check`) | off | Runs the [keyframe-placement check](./presentation.md#keyframes) (`W5011`) |

In Rust these are the fields of `kagari_lang::CompileOptions`, passed to
`RootDatabase::program_with`.

The `kagari` command-line tool (`cargo run -p kagari-cli -- …`) runs the compiler:

```text
kagari check <root> [--resolutions] [opts]   print the diagnostics; exit 1 on an error
kagari build <root> [-o <out>] [opts]        write the bytecode (default: <root> with extension .kagari)
kagari disasm <file> [--all]                 decode, verify and disassemble a .kagari file
opts: --state-cap <n> --recursion-depth <n> --keyframe-check
```

Diagnostics are numbered; see the Kagari Diagnostics book.

## `.kagari` header

A `.kagari` file starts with 16 magic bytes:

```text
....KAGARI..V000
```

| Offset | Length | Content          | Meaning                                       |
| -----: | -----: | ---------------- | --------------------------------------------- |
|      0 |      4 | `00 00 00 00`    | Zero bytes                                    |
|      4 |      6 | `KAGARI` (ASCII) | Format identifier                             |
|     10 |      2 | `00 00`          | Zero bytes                                    |
|     12 |      4 | `V000` (ASCII)   | Bytecode version: `V` and three decimal digits |

Once the format is stable, the version increases only on a breaking change. The layout after the
header is specified in [The `.kagari` Format](./bytecode.md).
