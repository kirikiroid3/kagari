# THIR and MIR

Every body goes through two IRs after type inference: **THIR**, a typed tree with the implicit
operations of inference made explicit, and **MIR**, a control-flow graph of basic blocks over
locals and places. MIR is the single semantic model of the language: const evaluation and the
story exploration interpret it, the analyses read it, and codegen translates it. The rule that
separates the layers:

> HIR defines what something *is*; MIR defines what *happens along a path*.

HIR (with name resolution and `InferenceResult`) answers questions about the program text: the
type of an expression, what a path names, whether a `match` is exhaustive. Stage-3 checks run
there and never see THIR or MIR, except the instant-procedure recursion check, which scans MIR
call edges. Anything that depends on the order of evaluation, on which branch is taken or on what
a step suspends reads MIR.

<!-- diagram: mir-pipeline (mermaid-ish)
flowchart LR
  hir["HIR body + InferenceResult"] ==>|thir| thir["THIR: typed tree, adjustments explicit"]
  thir ==>|item_mir| mir["MIR (ItemMir): main, closures, promoted"]
  mir -.-> dump["dump_item: tests/mir.rs snapshots"]
  ce["const_exprs"] -.->|promotion, pushdown fn only| thir
  mir ==> eval["Const eval: eval/machine"]
  mir ==> story["Story analysis: analysis/story"]
  mir ==> infl["Influence, system: analysis/"]
  mir ==> inst["Instant recursion: typeck/kinds"]
  mir ==> cg["Codegen: emit/codegen"]
-->
{{#include ../diagrams/mir-pipeline.svg}}

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/thir/mod.rs` | THIR data types: `Thir`, `ThirExpr(Kind)`, `ThirPat(Kind)`, `ThirCallee`, `ThirClosure`, `ThirPromoted` |
| `core/kagari-lang/src/thir/build/mod.rs` | `thir(db, program, item)` (salsa, per item) and the `Builder` |
| `core/kagari-lang/src/thir/build/{expr,call,pat}.rs` | Expressions and statements; calls and method calls; patterns with explicit dereferences |
| `core/kagari-lang/src/thir/build/promote.rs` | Promotion of the maximal constant subexpressions of `pushdown fn` bodies |
| `core/kagari-lang/src/mir/mod.rs` | MIR data types: `ItemMir`, `Body`, `BodyKind`, `MirSource`, `Place`, `Operand`, `Rvalue`, `Terminator`, `LocalSet` |
| `core/kagari-lang/src/mir/build/mod.rs` | `item_mir(db, program, item)` (salsa, per item), `ItemCx`, the per-body `Builder`, `BlockAnd`/`unpack!` |
| `core/kagari-lang/src/mir/build/{place,operand,rvalue,into}.rs` | Expressions as places, operands, rvalues, or written into a destination |
| `core/kagari-lang/src/mir/build/control.rs` | Loops (`for` desugaring), `break`/`continue`/`return`, `?` |
| `core/kagari-lang/src/mir/build/call.rs` | Calls; `push_step`, `rng_choose` and scene calls as suspension terminators |
| `core/kagari-lang/src/mir/build/matches.rs` | Patterns compiled to decision trees (Jacobs' algorithm) |
| `core/kagari-lang/src/mir/build/closure.rs` | The bodies of an item: main, closures, promoted constants |
| `core/kagari-lang/src/mir/build/cleanup.rs` | CFG cleanup (jump threading, dead blocks, merges, renumbering) and `needs_cell` |
| `core/kagari-lang/src/mir/liveness.rs` | Backward liveness of locals at block boundaries |
| `core/kagari-lang/src/mir/validate.rs` | Structural MIR checks (debug builds, before emission) |
| `core/kagari-lang/src/mir/pretty.rs` | `dump_item`: textual MIR for snapshots |
| `core/kagari-lang/tests/mir.rs` | Snapshot of the MIR of every item of each `tests/bytecode/*` fixture |

## THIR

`thir(db, program, item) -> Option<Thir>` builds the typed tree of one body: a `fn`, a `const`
initializer or a `static` initializer. It returns `None` when the item has no body, when its body
resolution or inference reported an *error*, or when std failed to load. Every later stage
(`item_mir`, const evaluation, the analyses, emission) silently skips an item without THIR, so an
item with errors never reaches the interpreters.

### What THIR adds over HIR and inference

Inference records its decisions in side tables of `InferenceResult` (`expr_types`, `coercions`,
`fields`, `method_calls`, `paths`, `records`, `closures`, `pat_ctors`, …). THIR folds them into
the tree so that the MIR builder never consults inference:

| Concern | In HIR + inference | In THIR |
| ------- | ------------------ | ------- |
| Types | `expr_types` side table | `ThirExpr::ty` on every node, `ThirPat::ty` on every pattern |
| Coercions | `coercions` side table | A `Coerce { kind, expr }` node around the coerced expression |
| Auto-deref of field and index bases | `fields[expr].derefs` | Explicit `Deref` nodes (index bases follow every reference layer) |
| Method calls | `method_calls`: receiver adjustment, chosen function | `Call` with the adjusted receiver as first argument |
| Callees | A path or an expression | `ThirCallee::{Direct, Builtin, Value}` plus `CalleeKind` |
| Paths | `paths[expr]: ValueRef` | `Local`, `ConstItem`, `ConstParam`, `FnItem`, `Handle`, `StaticValue`, `StoryVar`, or an `Adt` literal for a unit constructor |
| Records, ranges | `Record` with names; `Range` syntax | `Adt { ctor, fields: (index, value) }`; a range is a `Range` struct literal with an explicit `inclusive` field |
| `&&`, `\|\|` | `Binary` | `Logical`, separate from the other operators |
| Compound assignment | `Assign { op }` | `AssignOp { saturate }`: `true` for an `Int<lo, hi>` target |
| `?` | `Try` | `Try { kind: TryKind }` with the success and failure variant indices |
| `for` | `For` | `For { kind: ForKind::{Array, Vec, Range} }` |
| `break`/`continue` | Labels | The target loop's `ExprId`, from name resolution |
| Patterns | Binding modes inferred | `ThirPat::derefs` (references followed before the test), `Binding { by_ref }`, `..` expanded to wildcards, path patterns as `Adt` or `Const` |
| Closures | Inline expression | Listed in `Thir::closures` with their captures (in order of first use), parameters and return type |
| Story constants | — | `Promoted(id)` nodes, see below |

Nodes that cannot be evaluated (a missing expression, an unresolved field) become
`ThirExprKind::Error` / `ThirPatKind::Error`; they lower to `Unreachable`.

### Promotion

For `pushdown fn` bodies only, the builder consults `eval::const_exprs` (see [Constant
Evaluation](./eval.md#constant-expressions)) and replaces every *maximal* constant subexpression by
a `ThirExprKind::Promoted(id)` node. The promoted tree goes to `Thir::promoted` as a
`ThirPromoted { src, root, const_locals }`: `const_locals` is a prelude of `let pat = source;` for
every constant local the root reads, in dependency order. A literal, a path, and an expression of
type `()` or `!` is *trivial* and stays inline. Nothing inside a promoted expression is promoted
on its own. The typical promoted constant is the `Say` or `Choice` record of a step, which is why
the steps of a scene carry `step#N(const {promoted#M})` in MIR.

### Why THIR exists

**Decision.** Insert a typed tree between HIR + `InferenceResult` and MIR, built per item and
cached by salsa; MIR building reads only THIR.

**Why.** Every MIR lowering function would otherwise repeat inference's implicit operations:
look up the coercion of each operand, insert the receiver's derefs and borrows, resolve method
dispatch, decide whether a path is a story variable or a handle, expand `..` in patterns. Doing
it once, in a tree shaped like the source, keeps the MIR builder about control flow and places,
as in rustc. Promotion also needs a tree-level place to cut out constant subexpressions before
the CFG exists.

**Consequences.** One more IR to keep in sync with inference: a new adjustment recorded by
inference must become an explicit THIR node, or MIR will silently miss it. The checks of stage 3
keep using HIR + `InferenceResult` directly; THIR serves MIR building only.

## MIR

`item_mir(db, program, item) -> Option<ItemMir>` lowers one item's THIR; it is `None` exactly
when the item has no THIR.

```rust
pub struct ItemMir {
    pub main: Body,
    pub closures: Vec<(ExprId, Body)>, // sorted by ExprId
    pub promoted: Vec<Body>,
    pub promoted_src: Vec<ExprId>,     // root HIR expression of each promoted constant
    pub loops: Vec<LoopInfo>,          // `loop`/`while` of `main` (not `for`): header + region
}
```

A `Body` has a `MirSource` (`Item`, `Closure { owner, expr }` or `Promoted { owner, index }`), a
`BodyKind`, an arena of `LocalDecl`s, `arg_count`, an arena of blocks with `entry` (always block
0), and `needs_cell`.

| `BodyKind` | Source |
| ---------- | ------ |
| `Fn` | `fn`, and closures of kind plain |
| `ConstFn` | `const fn`, and `const` closures |
| `Instant` | a `pushdown fn` without `#[scene]`, and `pushdown` closures |
| `Scene` | a `#[scene] pushdown fn` |
| `Const` | a `const`/`static` initializer, and every promoted constant |

### Locals

Local `_0` is the return place. Locals `1..=arg_count` are the arguments: `self` first, then the
parameters; for a closure, the captured locals first, then its parameters. A plain by-value
parameter binding is its own argument local; any other parameter pattern gets an anonymous
argument that the body destructures on entry. `LocalKind` records whether a local is the return
place, an anonymous argument, a source local (`User(LocalId)`) or a temporary. Temporaries are
immutable unless created with `temp_mut` (loop counters, `&&`/`||` results).

`needs_cell` is the set of locals whose storage is borrowed (`Ref` of a place rooted at the local
without a dereference) or written through a projection. They live in a heap cell at run time;
codegen gives them a register holding a reference ([Emitting Bytecode](./emit.md#register-allocation)).

### Places, operands, rvalues

```rust
pub struct Place { pub base: PlaceBase, pub proj: SmallVec<[Proj; 2]> }
pub enum PlaceBase { Local(Local), StoryVar(ItemId) }        // a story variable is a place
pub enum Proj { Deref, Field(u32), Index(Local), Downcast(u32) }
pub enum Operand { Copy(Place), Const(ConstOperand) }
```

There is no move operand: Kagari has value semantics, so every read of a place is a copy (the
const evaluator uses persistent `imbl` vectors to keep that cheap). `ConstOperand` is a literal
`Value`, a `const` `Item` with generic arguments, a const generic `Param`, a `Promoted(id)`, a
function value `Fn(FnRef)`, a `Handle` (the identity of a `Setting`/`Meta` static), or a plain
immutable `Static`. Constants stay symbolic in MIR: their values come from the const-evaluation
queries (`const_value`, `promoted_value`, `static_value`), which the interpreters and codegen ask
when they need them, so no value is baked into a user's MIR.

Rvalues are chosen by operand type, so later stages never dispatch on types for arithmetic:

| Rvalue | Meaning |
| ------ | ------- |
| `Use(op)` | Copy |
| `IntBin { op, ty }`, `IntNeg`, `IntNot` | Integer arithmetic checked against `ty` (a Rust integer or `Int<lo, hi>`) |
| `IntClamp { ty }` | Saturate to `ty`: compound assignment to `Int<lo, hi>` computes in `i128` and clamps |
| `BoolNot`, `BoolBin` | `!`, `&`, `\|`, `^` on `bool` |
| `DecBin`, `DecNeg`, `RatBin`, `RatNeg` | `Decimal` and `Rat` arithmetic |
| `Cmp { op }` | Comparisons |
| `Aggregate { kind: Tuple \| Array \| Adt(ctor) }`, `Repeat` | Construction |
| `Ref { mutable, place }` | Borrow |
| `Len(place)` | Length of an array or `Vec` |
| `Closure { expr, captures }` | A closure value; captures are copies, in the order of the closure body's leading arguments |
| `Cast` | `expr as ty`; unreachable in practice, since every `as` is the type error `E2019` |

Statements are `Assign(Place, Rvalue)` and `Call { dest, callee, args }`, where
`Callee::{Fn(FnRef), Builtin(Builtin, FnRef), Value(Operand)}`. Every statement and terminator
records the HIR `ExprId` being lowered (`src`, `term_src`): diagnostics, line tables and the
story analysis anchor on it.

### Terminators and suspensions

| Terminator | Meaning |
| ---------- | ------- |
| `Goto(bb)` | Jump |
| `SwitchBool { cond, then, other, short_circuit }` | Branch; `short_circuit: Some(local)` marks the branches of `&&`/`\|\|`, which only decide that local (the influence analysis does not count them as conditions) |
| `SwitchVariant { place, adt, targets, otherwise }` | Branch on an enum's variant |
| `Return` | Return `_0` |
| `Unreachable` | Cannot happen in a well-typed program; an abort at run time |
| `Step { arg, dest, next, id }` | `push_step(arg)`: show a step, resume at `next` with the outcome in `dest`; `id` is its [step ID](#step-ids) |
| `Rng { ty, dest, next }` | `rng_choose::<ty>()`: draw a value |
| `SceneCall { callee, args, dest, next }` | Call a scene |

The last three are **suspensions** (`Terminator::is_suspension`). The story can stop for the
player only there, so they end their block: everything between two suspensions is one *segment*
of instant code, which is what the story analysis runs at once. Suspensions are legal only in
`Scene` bodies. Calls of instant procedures, `const fn` and `fn` stay statements: they never
suspend. Which call becomes which is decided by the THIR `CalleeKind`
(`Step`, `Rng`, `Scene`, or `Plain`/`Const`/`Instant` for a statement).

The MIR of the `Start` scene of `tests/bytecode/005__story/main.kix`:

<!-- diagram: mir-scene-cfg (mermaid-ish)
flowchart LR
  bb0["bb0: step#0(promoted#0)"] ==> bb1["bb1 choice!: step#1(promoted#1)"]
  bb1 ==> bb2["bb2: switch_variant(_2)"]
  bb2 ==>|Stay| bb3["bb3: scene_call Visit()"]
  bb2 ==>|Leave| bb4["bb4: story ROUTE = Leave"]
  bb3 ==> bb5["bb5-bb10: count(); if ... s!"]
  bb4 ==> bb5
  bb5 ==> bb11["bb11: rng::<Coin>()"]
  bb11 ==> bb12["bb12: switch_variant(_10)"]
  bb12 ==>|Heads| bb13["bb13: step#3(promoted#3)"]
  bb12 ==>|Tails| bb14["bb14: step#4(promoted#4)"]
  bb13 ==> bb15["bb15: Meta::write; return"]
  bb14 ==> bb15
  %% highlighted: suspension terminators (Step, Rng, SceneCall)
-->
{{#include ../diagrams/mir-scene-cfg.svg}}

In the textual dump (`mir::pretty`):

```text
scene crate::Start() -> () {
    let mut _0: (); // return
    let _2: Route; // route
    …
    bb0 (entry): {
        _1 = step#0(const {promoted#0}) -> bb1;
    }
    bb1: {
        _2 = step#1(const {promoted#1}) -> bb2;
    }
    bb2: {
        switch_variant(_2) -> [Stay: bb3, Leave: bb4];
    }
    bb3: {
        _3 = scene_call crate::Visit() -> bb5;
    }
    bb4: {
        story crate::ROUTE = crate::Route::Leave;
        goto -> bb5;
    }
    …
}
```

### Step IDs {#step-ids}

Each `Step` terminator carries a `mir::StepId(u32)` (printed `#N`): the step's index, 0-based,
among the step forms of its item in **source order**. A step form is a `push_step` call, whether
written `s!(…)`, `choice!{…}` or `push_step(…)`. `ItemCx::new` (`mir/build/mod.rs`) collects the
THIR calls with `CalleeKind::Step`, sorts their HIR `ExprId`s and numbers them: HIR lowering
allocates expressions in source order (a form's call after its arguments), so `ExprId` order is
source order. The emitter copies the id into the `STEP` instruction, and the runtime names a
step `{function path}#{id}` (read flags, traps, the progress log).

- **Not text ranges.** Sorting by source position would make `item_mir` depend on the source
  map, so every edit that moves text would invalidate it; `ExprId`s are stable across such edits.
- **Not code layout.** The emitted block order (reverse postorder) can differ from source order,
  e.g. a step after a `loop` with an early `break` is laid out before the loop's step
  (`tests/bytecode/007__step_ids`: layout `#0, #2, #1`). IDs do not depend on it.
- **Stability.** Inserting code without steps leaves every ID unchanged; inserting or removing a
  step shifts the IDs of the later steps of the same function only.
- **Dead steps count.** Numbering is over all step forms of the item, not over reachable MIR, so
  removing dead code elsewhere does not renumber.

### Patterns as decision trees

`matches.rs` compiles `match`, `if let`, `while let`, `let` with a refutable or destructuring
pattern, and parameter patterns with one routine, `decision_tree`, in the style of Jacobs'
algorithm:

1. Each arm becomes a *row*: a list of tests (a place and the pattern it must match), the bindings
   collected so far, and the arm index. The place of a test already has the pattern's `derefs`
   applied.
2. Rows are *simplified* until every remaining test is refutable: wildcards vanish, bindings move
   to the row's bindings, tuple and struct patterns become tests on their fields, and an
   or-pattern becomes one row per alternative (all leading to the same arm).
3. The first test of the first row is branched on: an enum variant with `SwitchVariant` (fields
   read through `Downcast`), a literal, constant or range with `Cmp` and `SwitchBool`. Every row is
   specialized to each branch and compiled recursively.
4. When the first row has no tests left it matched: its bindings are assigned (`Ref` for a
   by-reference binding, a copy otherwise) and its guard is tested; a false guard falls back to
   compiling the remaining rows.

The fall-through block of a `match` is `Unreachable`: exhaustiveness was proven in stage 3, by a
separate Maranget-style usefulness check on HIR (`typeck/exhaustive.rs`). When a guard might
write to the scrutinee (`is_pure` is false for some guard), the scrutinee is first copied into a
temporary.

### Other lowerings

- **Evaluation order.** A callee value is evaluated first, then the arguments left to right
  (the receiver first). An operand followed by an expression with effects is copied into a
  temporary first (`as_stable_operand`, `as_operands`). Assignment evaluates the value, then the
  place.
- **`for`.** A counter loop over a temporary copy of the iterable. Over `a..b`:
  `i = a; while i < b { body; i += 1 }`. Over `a..=b`:
  `if a <= b { i = a; loop { body; if i == b break; i += 1 } }`, so an inclusive range ending at
  the type's maximum never increments past it. Over a `Range` value that is not a range
  expression, its `inclusive` field decides at run time. The counter has the element type and is
  incremented with `IntBin` in `i128`. Over an array or `Vec`, a `usize` index counts to `Len`.
  `continue` jumps to the increment. `for` loops are not recorded in `ItemMir::loops`: they are
  bounded, so termination does not look at them.
- **`?`.** `SwitchVariant` on the `Option`/`Result`; the failure branch writes `None` or
  `Err(e)` to `_0` and returns.
- **`&&` / `||`.** Two `SwitchBool`s that assign `true`/`false` to a result local, both marked
  `short_circuit`.
- **Diverging code.** `break`, `continue`, `return` and aborts terminate their block and return a
  fresh block for the code after them, which nothing reaches; cleanup removes it.

### Builder structure

The builder follows rustc's MIR builder. Every lowering function takes the block control is in and
returns the block where control continues together with its result, `BlockAnd<T>`; the macro
`unpack!(block = expr)` rebinds `block` and yields the result. Lowerings come in four flavours,
chosen by the expression's `Category`: constant operand, place, rvalue, or "into" (control flow,
calls and assignments written into a destination).

`ItemCx` holds what all bodies of an item share: the THIR, signatures, std table, the
declaration of every bound local (type and mutability, with `by_ref` bindings typed as
references) and the type of each closure. `Builder` holds one body under construction: the
locals arena, pending blocks, the stack of enclosing loops (`LoopScope` with `break`/`continue`
targets and the `break` destination), the map from source locals to MIR locals, and the current
`src`. A block keeps the first terminator it is given; `abort` terminates a block with
`Unreachable` and returns a fresh dead block for the code after it.

`ItemCx::build` lowers `main`, then every closure, then every promoted constant (whose prelude
binds its constant locals before the root). `Builder::finish` hands the blocks to
`cleanup::simplify`, which threads jumps through empty `Goto` blocks, removes unreachable blocks,
merges a `Goto` into a successor with no other predecessor when both are in the same loops,
renumbers blocks in reverse postorder (block 0 stays the entry, loop headers are kept, loop
regions are renumbered), and then computes `needs_cell`.

### Liveness, validation, dumps

- `liveness::block_liveness(body)` is a backward data-flow over the CFG giving live-in and
  live-out sets per block. A use is every operand, every index local, and the base local of every
  read place; an `Assign` (or call/suspension `dest`) to a bare local defines it; a write through
  a projection uses the base; `Return` uses `_0`. The story analysis derives the kept locals of
  each suspension from it, and register allocation its live intervals.
- `validate::validate(db, program, body)` checks, and `emit::build` runs it on every body in debug
  builds, panicking on a violation: (1) suspensions only in `Scene` bodies; (2) `SceneCall`
  callees are scenes and no call statement targets a scene, step or draw; (3) call statements
  follow the kind rules, with `Const` bodies treated as `const fn`; (4) story variables only in
  `Instant`/`Scene` bodies; (5) every referenced local and block exists and
  `arg_count < locals.len()`. These mirror the bytecode verifier's kind rules
  ([The `.kagari` Format](../formats/kagari.md)), so a MIR bug is caught before it becomes
  `E7003`.
- `pretty::dump_item(db, program, item)` prints the main body, closures and promoted constants
  without offsets or `ExprId`s, so the dump is stable under edits that do not change the code.

## Invariants

- MIR is built only from THIR; THIR exists only for items without resolution or inference errors.
- Suspensions are terminators and appear only in `Scene` bodies; every other call is a statement.
- Every statement and terminator that can fail or suspend carries the `src` of the HIR expression
  it came from.
- `_0` is the return place; arguments are `1..=arg_count`, closure captures first.
- `ItemMir::closures` is sorted by `ExprId`, `promoted_src[i]` is the root of `promoted[i]`, and
  promoted constants are `BodyKind::Const` with no arguments.
- `ItemMir::loops` lists exactly the `loop` and `while` loops of `main`, with regions covering
  every block allocated while lowering the condition and body.
- MIR is a pure function of the item's THIR: no text ranges, no global state. Salsa caching and
  the stable `dump_item` output depend on this.

## When you change this

- **A new expression form or adjustment.** Add the THIR node, lower it in the right MIR flavour,
  then handle it in every MIR consumer: `eval/machine/exec.rs`, `emit/codegen`,
  `mir/liveness.rs`, `mir/validate.rs`, `mir/pretty.rs`, `analysis/influence.rs`,
  `analysis/system/summary.rs`, `typeck/kinds/instant.rs`. The compiler's exhaustive matches find
  most of them.
- **A new terminator or suspension.** Also `Terminator::successors`/`is_suspension`, the cleanup
  pass (`map_targets`), the story analysis (`segment.rs`, `liveness.rs`, `termination.rs`,
  `explore/`), and codegen `term.rs`.
- Update the MIR snapshots: run `cargo test -p kagari-lang --test mir` and accept the new
  `tests/snapshots/mir__*.snap` with `cargo insta review` (or `INSTA_UPDATE=always`) after
  reading the diff; the bytecode snapshots usually change too (see [Testing](../process/testing.md)).
- Keep [Constant Evaluation](./eval.md), [Story and System Analysis](./analysis.md) and
  [Emitting Bytecode](./emit.md) in step; a user-visible semantic change also needs the Reference
  (en and ja).
