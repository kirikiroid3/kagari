# Constant Evaluation

Stage 4 runs `const` code at compile time. It computes every constant and static, folds the
constant parts of story bodies, *tabulates* `const fn` calls the story makes with runtime
arguments, and evaluates the render of every step once to find keyframes and render errors. All of
it runs on one interpreter, the MIR executor in `eval/machine`, which the story exploration of
stage 5 reuses in *story mode*.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/eval/mod.rs` | `const_eval(db, program) -> ConstEval` (salsa, whole program), the `Driver`, limits `EVAL_STEP_BUDGET` and `TABLE_DOMAIN_CAP`, `Fault`, `Table`, `RenderId`/`RenderInfo` |
| `core/kagari-lang/src/eval/queries.rs` | Per-item queries `const_value`, `static_value`, `promoted_value`; `E4006` cycle recovery |
| `core/kagari-lang/src/eval/const_exprs.rs` | Which HIR expressions of a body are constant expressions (`ConstExprs`) |
| `core/kagari-lang/src/eval/tabulate.rs` | Tabulated `const fn` calls |
| `core/kagari-lang/src/eval/domain.rs` | `DomainCtx`: the values of finite types; `product` |
| `core/kagari-lang/src/eval/renders.rs` | The renders of steps: keyframe detection and render errors |
| `core/kagari-lang/src/eval/value.rs` | `ConstValue` (plain data), `Text`, `AssetKind` (five file kinds and `Shader(ShaderRole)`), `HandleKind`, `display_value` |
| `core/kagari-lang/src/eval/std_items.rs` | `StdTable`: the std items evaluation builds and inspects; the `Builtin` table |
| `core/kagari-lang/src/eval/machine/mod.rs` | `Machine`: heap, places, operators, `freeze`/`thaw`, `MAX_CALL_DEPTH`, `EvalError`/`ErrorKind` |
| `core/kagari-lang/src/eval/machine/exec.rs` | The MIR executor: frames, statements, terminators, calls |
| `core/kagari-lang/src/eval/machine/story.rs` | Story mode: story variables, `run_scene` up to the next suspension |
| `core/kagari-lang/src/eval/machine/builtins.rs` | The const `#[builtin]` functions of std |
| `core/kagari-lang/src/eval/machine/screen.rs` | The compile-time model of `Screen` |
| `core/kagari-lang/src/eval/machine/anim.rs` | Motion, easing and fraction rules |
| `core/kagari-lang/src/eval/num.rs` | Integer helpers by Rust integer type |
| `core/kagari-lang/src/eval/tests.rs` | Unit tests of the machine, screen model, tables and renders |
| `core/kagari-lang/src/debug.rs` | `render_dump`: the renders and their keyframe flags, for `tests/game_source.rs` |

## What stage 4 produces

```rust
pub struct ConstEval {
    pub consts: FxHashMap<ItemId, ConstValue>,          // every non-generic `const` (game and std)
    pub statics: FxHashMap<ItemId, ConstValue>,         // story variable initial values, handle defaults
    pub tables: FxHashMap<(ItemId, ExprId), Table>,     // tabulated calls, by story item and call
    pub renders: FxHashMap<RenderId, RenderInfo>,       // every render a step uses: keyframe or not
    pub diagnostics: Vec<Diag>,
}
```

`const_eval` is one salsa query over the whole program; it calls the per-item queries
`const_value(item, args)`, `static_value(item)` and `promoted_value(item, index)`, which other
stages call directly too (the story analysis, codegen). The driver runs four phases in order:

<!-- diagram: eval-driver (mermaid-ish)
flowchart LR
  p1["1 consts and statics: const_value, static_value (+ texts of literals)"] ==> p2["2 promoted constants: promoted_value (story bodies)"]
  p2 ==> p3["3 tabulate const fn calls: at most 65 536 rows (E4004)"]
  p3 ==> p4["4 renders: probe for keyframe, else Screen::empty() for errors"]
  p1 -.-> m["MIR executor: fresh Machine per evaluation, 1 000 000 steps, 10 000 nested calls (E4003)"]
  p2 -.-> m
  p3 -.-> m
  p4 -.-> m
  p4 ==> out["ConstEval: consts, statics, tables, renders"]
-->
{{#include ../diagrams/eval-driver.svg}}

1. **Constants and statics.** For every file (game and std) in load order: each non-generic
   `const` with a value through `const_value`, each `static` through `static_value`. Game items
   also contribute their string literals (except asset and system-font paths) to the program's
   *texts*, as do the `Text`s inside constant values.
2. **Promoted constants.** For each `pushdown fn` body, every promoted constant through
   `promoted_value`; errors are reported at the promoted expression, and texts are collected.
3. **Tabulation** of each story body's `const fn` calls with a runtime argument.
4. **Renders** of each story body's steps.

Phases 3 and 4 need the program's asset constants (by kind) and its named non-generic functions,
the values of the asset and function-pointer types.

## Constant expressions

`const_exprs(db, program, item)` (salsa, per item) marks the HIR expressions of a body that are
*constant*:

- a literal;
- a path to a `const` item, a unit struct or variant, a named function or tuple constructor, a
  `const` generic parameter, an immutable `static` (for a `Setting`/`Meta` handle its value is the
  handle's identity), or a constant local;
- a struct, enum, tuple, array or range literal, a unary, binary or cast operator, an index, a
  shared borrow, a field access, a block, an `if` or a `match` whose operands are all constant (a
  block's statements: `let` with a constant initializer binding only immutable locals, items, and
  constant expression statements);
- a call of a `const fn` or tuple constructor whose arguments are all constant (a method's
  receiver counts as an argument and must not be borrowed mutably), including `asset::<A>("…")`;
- a closure that captures nothing.

A **constant local** is an immutable binding of a `let` with a constant initializer, or of an
`if let`/`match` arm pattern with a constant scrutinee. Parameters, story variables, results of
`fn` and `pushdown fn` calls, loops, `break`, `continue`, `return`, `?` and assignments are never
constant, and neither is an expression of type `Ty::Error`, so nothing built on an error is
evaluated.

Three consumers read `ConstExprs`: THIR promotion ([THIR and MIR](./mir.md#promotion)) folds the
maximal constant subexpressions of story bodies, tabulation skips constant arguments, and render
evaluation reads constant step records.

## The machine

### Values

The machine computes with `machine::value::Value`; results leave it as `ConstValue`:

| `Value` | `ConstValue` | Notes |
| ------- | ------------ | ----- |
| `Bool`, `Int(i128)`, `Decimal`, `Rat`, `Text`, `String` | same | Every integer type is an `i128` checked against its type's range |
| `Tuple`, `Array`, `Vec`, `Adt { item, variant, fields }` | same | `imbl::Vector`: copies are cheap and structural |
| `Asset { kind, path }`, `Fn(FnRef)`, `Handle { kind, item }` | same | |
| `Closure(ClosureValue)` | `Closure { owner, expr }` | Only a capture-less closure freezes |
| `Screen(Arc<Screen>)` | `Screen` | `Arc::make_mut` on write |
| `NewHandle { kind, init }` | — | `setting(init)`/`meta(init)`; only a `static` initializer may produce it (`E4007` otherwise) |
| `Ref(Box<Place>)` | — | A pointer to a place |
| `Pending` | — | A `Change<T>`, `Swap` or `ChannelChange` waiting for `apply_animation` |
| `Probe` | — | The opaque input screen of the keyframe test |
| `Uninit` | — | A `let` without initializer, a fresh local |

`freeze` turns a `Value` into a `ConstValue` and fails with `Abort` on references, pending changes,
capturing closures and `Uninit`, with `Inspected` on the probe; `thaw` goes back.

### Heap, places and frames

The heap is a `Vec<Value>` of cells. A **place** is `{ cell, path: [Field(i) | Index(i)], obj }`;
`obj` is set for the screen parts builtins hand out (`&mut Entity`, `&mut Prop<T>`, …: a
`ScreenObj` on the place of the screen). A reference is a `Value::Ref(place)`, so a write through
`&mut` is visible through every alias, as the language's reference semantics require.

The executor (`exec.rs`) runs MIR bodies with an explicit stack of `Frame`s (body, generic
environment, one heap cell per MIR local, current block and statement), so deep Kagari
recursion never recurses on the host stack. `GenericEnv` substitutes the generic arguments of the
running instance into types, `FnRef`s and const parameters.

Every statement and terminator costs one step, and `Repeat { count }` costs `count` more. A
`Machine` stops with `E4003` when it passes `EVAL_STEP_BUDGET` (1 000 000) steps or
`MAX_CALL_DEPTH` (10 000) nested calls. Each top-level evaluation gets a fresh machine, so the
budget applies separately to one constant, one promoted constant, one row of a table, one render
evaluation, and (in stage 5) one story segment.

### What it can evaluate

| Callee | Const mode | Story mode (stage 5) |
| ------ | ---------- | -------------------- |
| `const fn`, constructors, capture-less and capturing closures of const kind | yes | yes |
| Const `#[builtin]`s (`BUILTINS` in `std_items.rs`: `asset`, `system_font`, `Vec`, `Integer`, `Rat`, the screen API, text building, `setting`, `meta`) | yes | yes |
| Instant procedures (`pushdown fn` without `#[scene]`) | no (abort) | yes |
| Story variables (read and write) | no (abort) | yes: thawed into a cell on first access |
| `Meta::write` | no | yes, as a no-op: the story never reads meta back |
| Other runtime builtins (`Builtin::Runtime`), `fn` | no (abort) | no (abort) |
| Scenes and the suspensions `Step`, `Rng`, `SceneCall` | no (abort) | stop the segment; the analysis drives them |

An abort is not a diagnostic: it means "this cannot be evaluated here", and only happens on code
stage 3 already rejected, on errors already reported, or in the analysis' own probes.

## Constants and statics

`const_value(item, args)` runs the MIR of a `const` initializer with the parent impl's generic
arguments and freezes the result. Generic constants are evaluated per instantiation on demand;
the driver evaluates only non-generic ones, and codegen evaluates the instantiations it emits. A
fault found then is reported by emission at the use site
([Emitting Bytecode](./emit.md#constants-and-promoted-values)).

`static_value(item)` does the same for a `static`. A top-level `NewHandle` makes the static a
`Setting`/`Meta` handle whose default value is `init`; any other value is a plain static or the
initial value of a story variable.

Both queries declare a salsa `cycle_result`: a constant whose value depends on itself yields
`E4006` for every member of the cycle, each reported at its own name, while a constant that only
uses a member of a cycle gets no diagnostic of its own.

## Folding story bodies

THIR promotion makes the maximal constant subexpressions of every `pushdown fn` body promoted
constants: MIR bodies of kind `Const` in `ItemMir::promoted`. `promoted_value(item, index)` runs
one and freezes the result. The step records of `s!` and `choice!` are the typical case: their
texts, voice, render function, choice id and arm values are all constant, so the whole `Say` or
`Choice` record is one constant and the scene's MIR says `step#N(const {promoted#M})`. A guard
(`shown`) that reads a story variable makes the `Choice` record non-constant; its constant parts
are still promoted individually.

Codegen loads a promoted value as a bytecode constant, except when it holds a `Screen` (which no
bytecode constant can) or fails to evaluate; then the promoted body is emitted as a synthetic
function and called at run time.

## Tabulation

A `pushdown fn` may call a `const fn` when every argument is a constant expression or of a finite
type. When at least one argument is not constant, the driver *tabulates* the call: it evaluates
it on every combination of argument values. This proves the call terminates without error on
every input the story can give it, and it adds the call's `Text` outputs to the program's texts.

The calls are the `Call` statements of the story item's MIR (main body and closures) whose source
is a call of a `const fn` (not a constructor) with a non-constant argument. Calls are visited in
MIR order, so an inner call is tabulated before the call that consumes its result. For each
argument the driver builds its value set:

1. a constant argument: its value (for `&x`, the value of `x`);
2. the result of an already tabulated call: that table's distinct outputs;
3. a tuple, array literal or constructor call built from such parts: the product of the parts;
4. otherwise every value of the argument's type (`DomainCtx::values`).

`DomainCtx` knows the finite types: `bool`, `Int<lo, hi>`, tuples, arrays, enums and structs of
finite types, `&T` of a finite `T`, `Text` (the program's texts), the asset types (the program's
asset constants of that kind), and function pointers of kind `const`, `pushdown` or `scene` (the
named non-generic functions of matching kind and signature). Types nested deeper than 32 levels
count as infinite. A non-finite argument type makes the call untabulated without a diagnostic of
its own (stage 3 reports the finite-type violation).

When the product of the value sets exceeds `TABLE_DOMAIN_CAP` (65 536), the call is `E4004`.
Otherwise each row runs on a fresh machine: by-reference arguments get a cell, and for `&mut`
arguments the row's output is the tuple of the return value and the final values of those
arguments (`Table::writes`). A failing row reports its fault at the call with the suffix
``(with arguments `(…)`)``.

*Current use of tables.* `ConstEval::tables` is read only by tabulation itself (outputs of inner
calls) and tests. The story exploration re-runs the `const fn` in the executor on the concrete
arguments of each state, and codegen emits the `const fn` and calls it at run time. Tabulation is
therefore a totality check plus the source of computed texts, not a lowering.

## Renders and keyframes

For every `Step` terminator of a story body's main MIR whose call is `push_step(record)`,
`renders.rs` collects the candidate renders of the record:

- a constant record: its `render` field;
- a record literal with a non-constant `render` field: the renders that field can yield, looking
  through `Some(x)`, `if`, `match`, block tails and inline closures;
- anything else (a render computed at run time): every named `const fn(Screen) -> Screen` of the
  program, the values a runtime `Render` can hold.

Each distinct render (`RenderId::Fn(item)` or `RenderId::Closure(owner, expr)`) is evaluated once:

1. On `Value::Probe`. Any inspection of the probe (reading a component, comparing, projecting)
   stops with `ErrorKind::Inspected`. A render that finishes and whose output freezes never read
   its input: it is a **keyframe** (the proposal's keyframe; every other render is a *delta*).
2. Otherwise it is evaluated again on `Screen::empty()`, so its errors are found even though
   stage 5 later evaluates it on the actual screens.

The result goes to `ConstEval::renders` as `RenderInfo { keyframe }`, which the keyframe
placement check (`W5011`) and `render_dump` read. Errors are reported at the render site: the `x`
of `render = x` (which desugars to `render: Some(x)`), else the whole `render` value, else the
step.

### The screen model

`machine/screen.rs` models `std::screen::Screen` as the renders transform it:

- `LogicalScreen`: the persistent state after a render: live entities by `EntityId` with their
  components, the Hud, the camera, the screen filter, the music channel and four ambience
  channels. `Screen` equality and hashing compare only this part, as the language's `==`,
  snapshots and the consistency check do.
- `StepState`: what belongs to one step only: the timeline cursor and text start, scheduled
  animations and sounds, the choice menu's disabled arms and alignment, and despawning entities.

Effects are std values: the screen filter and an entity's `filter`, `mix`, `mask` and
`local_motion` hold `Option<Filter | Mix | Mask | LocalMotion>` (`EntityProp::{Filter, Mix, Mask,
LocalMotion}`), and an image part or the side image holds an `ImageAsset` or a `Paint`
(`ImageLayer::get` is `None` for a paint). `Entity::travel(to, motion)` (`Screen::travel`) reads
the current pose as `from`, checks the motion's easing, commits the three pose props with a
`Tween` built from the motion's duration, easing and delay, and schedules
`ScheduledKind::Travel { motion, from, to }` on `Target::Travel(id)`.

`asset("…")` evaluated in a std body names `FilePath::Std(path)` (the std shader library);
elsewhere the path is joined to the asset root. `erase_shader`/`erase_args` return their
argument: a role-typed shader is an `Asset { kind: Shader(role), .. }` whether typed
`ColorShader<A>` or erased to `ShaderAsset`.

Visible changes are `PendingChange`s that only `apply_animation` commits (`Screen::commit`),
which is how "every visible change states its animation" is enforced. At run time the engine
implements the same screen API natively ([The Engine](../runtime/engine.md)); the two
implementations must agree.

## Error reporting

A `Fault { code, message }` becomes a diagnostic only for game items: faults inside std code and
aborts are silent. The diagnostic is anchored at the code that triggered the evaluation (a
constant's name, a promoted expression, a tabulated call, a render site) and carries a related
note "evaluation failed here" at the failing expression when that is elsewhere. An `E4001`
overflow raised inside std's `time.kis` is reported as `E4013`, since it can only be a `Duration`
overflow.

| Code | Name | Raised by |
| ---- | ---- | --------- |
| `E4001` | `overflow` | Integer result outside its type (`check_int`), `Decimal`/`Rat` overflow, `Rat::new` overflow |
| `E4002` | `division-by-zero` | Integer `/` and `%`, `Decimal` and `Rat` division, `Rat::new(_, 0)` |
| `E4003` | `budget-exceeded` | Step budget or call depth exhausted |
| `E4004` | `domain-too-large` | Tabulation with more than 65 536 rows |
| `E4005` | `index-out-of-range` | Array/`Vec` indexing, `Vec::set` |
| `E4006` | `const-cycle` | Salsa cycle recovery of `const_value`/`static_value` |
| `E4007` | `handle-outside-static` | Freezing a `NewHandle` anywhere but at the top of a `static` |
| `E4008` | `invalid-motion` | Motion identity rule (`anim.rs`) |
| `E4009` | `invalid-easing` | Easing parameters (`anim.rs`) |
| `E4010` | `duplicate-mark` | A text with the same `mark(n)` twice |
| `E4011` | `begin-text-on-mark` | `begin_text` while the cursor is on a mark |
| `E4012` | `fraction-out-of-range` | Opacity, volume, anchor outside `0.0..=1.0` |
| `E4013` | `duration-overflow` | `Duration` beyond `u32` milliseconds, cursor arithmetic |

The same faults found while the story analysis runs story code or renders on real screens are
reported by stage 5 with these codes. The user-facing descriptions live in the site's Diagnostics
pages.

## Limits

| Constant | Value | Where | Past it |
| -------- | ----: | ----- | ------- |
| `EVAL_STEP_BUDGET` | 1 000 000 | `eval/mod.rs` | `E4003` |
| `MAX_CALL_DEPTH` | 10 000 | `eval/machine/mod.rs` | `E4003` ("recursion too deep") |
| `TABLE_DOMAIN_CAP` | 65 536 | `eval/mod.rs` | `E4004`; in stage 5 also `W5006` for entry arguments and draws |
| Domain nesting | 32 | `eval/domain.rs` (`MAX_DEPTH`) | type treated as infinite |

The first and third are marked *tentative* in the code: they are meant to become compiler
settings, and are not `CompileOptions` yet ([Open Questions](../process/open-questions.md)).

## Invariants

- Evaluation is deterministic: no floats, exact `Decimal`/`Rat` arithmetic, no host I/O, and hash
  maps are only iterated where the insertion order is deterministic. The same program always gives
  the same `ConstEval`, which the bytecode (and so its script hash) depends on.
- `ConstValue` is plain data: no references, no captured environments. Anything stored across
  queries or into bytecode is a `ConstValue`.
- The machine's semantics equal the VM's for every operation both implement: overflow rules,
  `Int<lo, hi>` saturation in compound assignment, division, indexing, text building, the screen
  API. A difference makes compile-time results (constants, keyframes, consistency) disagree with
  what the player sees.
- Every evaluation is bounded by the step budget and call depth; nothing in stage 4 can hang the
  compiler.
- Only game code reports faults; std faults surface as aborts.

## When you change this

- **A new const builtin.** Add it to `Builtin` and `BUILTINS` in `std_items.rs` (and the
  `BUILTINS_LEN` test), implement it in `machine/builtins.rs`, give it an intrinsic or import in
  codegen ([Emitting Bytecode](./emit.md#builtins-intrinsics-and-imports)), implement it in the
  engine, and document it in the STD Reference (en and ja). See
  [Builtins and the Host ABI](../std/builtins.md).
- **A new `E4xxx` code.** Add it to `DiagnosticCode`, raise it as a `Fault`, add a unit test in
  `eval/tests.rs`, and add one entry in each language of the site's Diagnostics pages
  (`tests/diagnostics_book.rs` enforces exactly one).
- **A change of what is constant.** `const_exprs.rs` decides promotion, tabulation and renders at
  once; update the Reference's constant-expression rules (en and ja) and check the MIR snapshots.
- **A limit.** Update the table above, the Reference's diagnostics entries that quote it, and
  [Open Questions](../process/open-questions.md) if it becomes a setting.
- Run `cargo test -p kagari-lang` (unit tests in `eval/tests.rs`, `tests/game_source.rs` for
  `--- renders ---` dumps).
