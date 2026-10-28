# Emitting Bytecode

Stage 6 lowers a checked program to a `kagari_bytecode::Module`, verifies it with the same
verifier a loader runs, and encodes it into the bytes of `game.kagari`. Code comes from the MIR of
each function; constants, statics and promoted values come from const evaluation. This chapter
covers what the compiler puts into the module and how; the byte layout and the verifier rules are
in [The `.kagari` Format](../formats/kagari.md), the execution model in [The VM](../runtime/vm.md).

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/emit/mod.rs` | `build(db, program) -> Build`; the `Emitter`: tables, statics, roots, the instance queue, debug info |
| `core/kagari-lang/src/emit/codegen/mod.rs` | `lower_instance`, `FnCodegen`: block layout, emission, labels, line table, emission-time faults |
| `core/kagari-lang/src/emit/codegen/regalloc.rs` | Block layout (reverse postorder) and register allocation |
| `core/kagari-lang/src/emit/codegen/place.rs` | Reads and writes of locals, story variables and projections |
| `core/kagari-lang/src/emit/codegen/rvalue.rs` | Assignments, operands, constants |
| `core/kagari-lang/src/emit/codegen/call.rs` | Calls, builtins (intrinsics or imports), closures, constructor and builtin wrappers |
| `core/kagari-lang/src/emit/codegen/term.rs` | Terminators, including `STEP` and `RAND` |
| `core/kagari-lang/src/emit/tests.rs` | Missing `main`, reachability of functions, the slider import |
| `core/kagari-lang/tests/bytecode.rs` | Snapshot: warnings and disassembly of each `tests/bytecode/*` fixture; decode round trip |
| `core/kagari-bytecode` | `Module`, `Instr`, `encode`/`decode`, `verify`/`verify_code`, `disassemble`, `word_offsets` |

## The build entry point

```rust
pub struct Build {
    pub diagnostics: Vec<Diagnostic>,   // those of `check`, then those of emission
    pub module: Option<Module>,         // `None` on any error
    pub bytes: Option<Vec<u8>>,         // `module` encoded
    pub asset_files: Vec<FilePath>,     // the file of each `module.assets` entry, by index
}
```

`build` runs `check` (stages 1–5). Any error stops it. In debug builds it then validates the MIR
of every body ([THIR and MIR](./mir.md#liveness-validation-dumps)) and panics on a violation.
`emit` finds `fn main` (a parameterless `fn main` of the root module), lowers the module, and
returns the module plus emission errors. Then:

1. With `main`, `verify(&module)` checks everything; without it, `verify_code(&module)` checks the
   code only, so the entry scenes and statics of a library-like root are still lowered and checked.
2. A verifier failure is `E7003` (`invalid-bytecode`, an internal compiler error) at the root file.
3. No `main` is `E7001` (`missing-main`), reported after lowering so that emission faults are not
   hidden behind it.
4. Otherwise `encode(&module)` gives the bytes.

`kagari build` writes the bytes and packs `asset_files` into archives
([Projects and the CLI](../tooling/cli.md)).

<!-- diagram: emit-pipeline (mermaid-ish)
flowchart LR
  roots["roots: statics, main, entry scenes"] ==> queue["instance queue: request(Instance), interned to FuncId"]
  queue ==> cg["codegen: RPO layout, regalloc, statements, terminators"]
  cg ==> module["Module: tables, functions, debug lines"]
  module ==> verify["verify, then encode: .kagari bytes"]
  cg ==>|CALL, FUNC, CLOSURE and constants request more instances| queue
  inputs["MIR (item_mir) and const-eval values"] -.-> cg
  verify -.- codes["E7001, E7002, E7003"]
-->
{{#include ../diagrams/emit-pipeline.svg}}

## What goes into the module

| Table | Filled by | Contents |
| ----- | --------- | -------- |
| `options.max_call_depth` | `Emitter::new` | `CompileOptions::recursion_depth` |
| `strings` | `string` | Paths, names, text: interned on first use |
| `types` | `type_id` | Every ADT the code or a constant mentions: path, `is_enum`, variant names, field names (`None` for positional) |
| `int_types` | `int_type` | `(lo, hi, bits, signed)` of every integer type an instruction checks against; entry 0 is always `i128` |
| `imports` | `import` | Host functions: std path, kind (`fn`, `const fn`, instant), parameter count (`self` included; `std::ui::slider` + 2) |
| `assets` | `asset` | Kind and path of every asset a constant or `asset("…")` names, relative to the asset root (system fonts under `system-fonts/`); `Build::asset_files` holds the source file of each |
| `constants` | `constant`, `text` | Every non-immediate value; children interned first, so a constant refers only to lower indices |
| `statics` | `statics` | Game story variables, then the `Setting`/`Meta` handles of the game and std, each with its initial value |
| `functions` | `request`, `run` | Path, kind, flags (`entry`, `main`, `closure`, `synthetic`), `params`, `regs`, code |
| `debug` | `finish` | Source files (relative to the root file's directory) and, per function, `(code word, source byte offset)` pairs |

The module has no script hash and no separate choice table. The engine computes the script hash
itself, as the SHA-256 of the whole `.kagari` file, and records it in saves
([Saves, Traces and Snapshots](../runtime/saves.md)). Choice ids, arm texts, `shown` flags, arm
values and renders travel inside the `Choice` record constant that a `STEP` instruction passes to
the engine.

## Statics

`Emitter::statics` runs first. It takes the game's `static mut` items (story variables) in file and
item order, then every immutable `static` of the game and std whose value is a `Setting` or `Meta`
handle. All statics get their `StaticId` before any initial value is interned, because a handle
constant may name another static. Each initial value is `static_value` as a constant; a static
without a constant value is an internal error (`E7003`). Plain immutable statics are not statics in
the bytecode: their values are loaded as constants, or computed by an initializer function when
they hold a `Screen`.

## Instances and reachability

A bytecode function is an `Instance`:

| Instance | Function | Path |
| -------- | -------- | ---- |
| `Fn(FnRef)` | An item with a body, generic arguments substituted (monomorphized per instantiation) | `crate::m::f::<u32>` |
| `Closure { owner, expr }` | A closure of the owner instance's body | `crate::Start::{closure#0}` |
| `Ctor(FnRef)` | Synthetic: a tuple struct or variant constructor used as a value | the constructor's path |
| `Builtin(FnRef)` | Synthetic: a builtin used as a value, a wrapper that lowers like a direct call | the builtin's path |
| `ConstInit(item, args)` | Synthetic: a `const`/`static` initializer whose value holds a `Screen` | the item's path |
| `Promoted(owner, index)` | Synthetic: a promoted constant that holds a `Screen` or fails to evaluate | `…::{promoted#N}` |

`request(instance)` interns an instance to a `FuncId` on first use, pushes an empty `Function`
with its kind, flags and parameter count, and queues it. Duplicate paths get a `#n` suffix. The
roots are requested in `roots`: `fn main` and every non-generic game entry scene, in item order;
statics requested their function values before. `run` then lowers the queue in FIFO order, and
lowering requests more instances (`CALL` targets, `FUNC` values, `CLOSURE` bodies, function
constants). Only functions reachable this way are emitted: an unused function is not in the
module, while a function used only as a constant's value is.

Interning everything on first use, in a deterministic traversal, makes the output a function of
the program alone: the same source gives the same bytes, and so the same script hash.

## Codegen of one function

`lower_instance` emits constructor and builtin wrappers directly (`call.rs`) and everything else
from its MIR body (`FnCodegen`):

1. **Layout.** Blocks are laid out in reverse postorder, so a block's first successor usually
   follows it and its jump can be omitted.
2. **Register allocation** (below).
3. **Prologue.** `CELL` creates the heap cell of every celled local, initialized from its argument
   register or uninitialized.
4. **Body.** Each statement, then each terminator, is emitted; the scratch pool is reset after
   each. The source offset of the statement's `src` is recorded for the line table whenever it
   changes.
5. **Finish.** Block labels are resolved to instruction indices; the function's `regs` is the
   highest register used. More than 65 535 registers, or more code words than a `u32` counts, is
   `E7002` (`function-too-large`) at the item's name.

### Register allocation

`regalloc.rs` assigns each MIR local a `Loc`:

- the arguments (locals `1..=arg_count`) are `r0..`, fixed and never reused;
- every celled local (`Body::needs_cell`) gets a dedicated register holding the reference to its
  cell (`Loc::Cell`);
- every other local shares registers by linear scan over live intervals; an interval covers every
  position where the local is live (block boundaries from `mir::liveness`, plus its uses and
  definitions), with no holes. Within a statement all reads happen before the write, so an
  interval ending at a statement's uses may share a register with one starting at its definition;
- a local that is never read gets no register (`Loc::Dead`): writes to it go to scratch.

Above the locals is one **argument window**, sized to the largest call, aggregate or closure of
the body, and above that the **scratch** pool for temporaries within a statement.

### Places

A place is walked from its base with a cursor that is either the value in a register or a register
holding a reference to it. Value operands of the VM follow references, so a read projects with
`FIELD`/`INDEX` in both cases; a write projects a reference with `REFFIELD`/`REFINDEX` and ends with
`STORE`. A story variable starts as `REFSTAT` of its static; a celled local as its cell register;
`LOAD` reads through a reference.

### Rvalues

| Rvalue | Instructions |
| ------ | ------------ |
| `IntBin`, `IntNeg`, `IntNot`, `IntClamp` | `IntBin`/`IntUn` with the `IntTypeId` of the (substituted) type; the VM checks the result against it |
| `BoolNot`, `BoolBin` | `NOT`, `BoolBin` |
| `DecBin`, `DecNeg`, `RatBin`, `RatNeg` | `DecBin`, `DECNEG`, `RatBin`, `RATNEG` |
| `Cmp` | `Cmp` with the operator |
| `Aggregate` | Operands into the argument window, then `TUPLE`, `ARRAY` or `ADT` (type and variant) |
| `Repeat`, `Len` | `REPEAT`, `LEN` |
| `Ref` | A reference to the place (a celled local's cell, `REFFIELD`/`REFINDEX`, `REFSTAT`) |
| `Closure` | Captures into the window, `CLOSURE` of the requested closure instance |
| `Cast` | `UNREACHABLE` (casts have no instruction; every `as` is rejected as `E2019` first) |

### Constants and promoted values

`load_value` emits `BOOL`, `INT` (when the value fits `i64`) or an empty `TUPLE` directly and
everything else as `CONST` of an interned constant. Symbolic MIR constants are resolved here:

| `ConstOperand` | Emitted as |
| -------------- | ---------- |
| `Value(v)` | `load_value(v)` |
| `Item { item, args }` | `const_value(item, args)` with the instance's generic arguments substituted; a value holding a `Screen` calls a `ConstInit` instance; a fault is reported with its `E4xxx` code at the use (a generic constant instantiated only by run-time code) and the instruction is `UNREACHABLE` |
| `Param` | `INT` of the substituted const generic argument |
| `Promoted(i)` | `promoted_value`; a value holding a `Screen`, or one that fails, calls a `Promoted` instance at run time |
| `Fn(f)` | `FUNC` of the function, constructor wrapper or builtin wrapper |
| `Handle(s)` | `CONST` of a `Handle` constant naming the static |
| `Static(s)` | A handle as above, a plain value as a constant, or a `ConstInit` call for a value holding a `Screen` |

`Screen` values never become constants: the screen is engine state, so a constant that holds one
is rebuilt at run time by its initializer.

### Calls

| Callee | Emitted as |
| ------ | ---------- |
| A function with a body (`Callee::Fn`) | Arguments into the window, `CALL` of the requested instance (scenes and instant procedures included) |
| A constructor | `ADT` |
| A function value (`Callee::Value`) | `CALLVAL` with the expected kind taken from the callee's type (`fn`, `const fn`, instant, scene) |
| A builtin | An intrinsic instruction, or `CALLIMP` of an import |

A scene call is an ordinary `CALL`: the VM knows from the callee's kind that it is a scene.

### Builtins: intrinsics and imports

A call of a std `#[builtin]` reads its operands where they are and becomes an intrinsic when one
exists, otherwise a host call:

| Builtin | Instruction |
| ------- | ----------- |
| `push_step` | `STEP dst, record` |
| `rng_choose::<T>` | `RAND dst, domain`: `Enum(type)` for a fieldless enum, `Int(int type)` for `Int<lo, hi>` |
| `asset::<A>("…")`, `system_font("…")` with a constant path | `CONST` of an `Asset` constant (the path resolved against the asset root or the system font directories); otherwise an import call |
| `Vec::new`, `len`, `get`, `set`, `push` | `VECNEW`, `LEN`, `VECGET` (with the `Option` type), `VECSET`, `VECPUSH` |
| `Vec::is_empty` | `LEN`, `INT 0`, `EQ` |
| `Integer::into` | `MOVE` |
| `Integer::saturate` | `ICLAMP` to the result type |
| `ruby`, `mono_ruby`, `effect`, `concat`, `to_text` | `RUBY`, `MRUBY`, `EFFECT`, `CONCAT`, `TOTEXT` |
| `Rat::new`, `Rat::to_decimal` | `RATNEW`, `RATDEC` |
| `std::ui::slider::<LO, HI>` | `CALLIMP` with three arguments: `props`, then `LO` and `HI` loaded by `INT` (imports carry no generic arguments) |
| Every other builtin (the screen API, `setting`, `meta`, `Meta::write`, runtime builtins) | `CALLIMP` of an import named by its std path; arguments must be consecutive registers, so they are moved into the window unless they already are |

The full builtin list, and which side implements what, is in
[Builtins and the Host ABI](../std/builtins.md).

### Terminators

| Terminator | Instructions |
| ---------- | ------------ |
| `Goto` | `JMP`, omitted when the target is laid out next |
| `SwitchBool` | `JMPNOT` or `JMPIF` (plus `JMP` when neither target is next) |
| `SwitchVariant` | A chain of `ISVAR` + `JMPIF`; the target laid out next (or, without `otherwise`, the last) is reached without a test |
| `Return` | `RET` of `_0` |
| `Unreachable` | `UNREACHABLE` (a VM panic if reached) |
| `Step` | `STEP` with the record operand; the outcome is written to `dest` |
| `Rng` | `RAND` with the domain of the drawn type |
| `SceneCall` | `CALL`/`CALLVAL`, as a call statement |

## Story functions in bytecode

Scenes are functions of kind `scene`, instant procedures of kind instant, `const fn` and renders
of kind `const fn`; an entry scene has the `entry` flag and `main` the `main` flag. A scene's code
is ordinary register code with `STEP` and `RAND` where its MIR had suspensions: the VM suspends
there, and the engine resumes it with the outcome. Story variables are statics of kind story
variable, read and written through `REFSTAT`. The `Say`/`Choice` record a step shows is usually
one `CONST` (the promoted record), so a step costs two instructions. An excerpt of
`tests/snapshots/bytecode__bytecode@005__story__main.kix.snap`:

```text
fn crate::Start [scene, entry] params=0 regs=6
  0000 CELL r0, r4
  0001 CONST r4, #17 std::narrative::Say {std::Option::None, std::Option::None, std::Option::None, std::Option::Some {"It begins."}, …}
  0002 STEP r5, r4
  0003 CONST r4, #27 std::narrative::Choice {"route", std::Option::Some {crate::Start::{closure#0}}, [std::narrative::ChoiceArm {"Stay", true, crate::Route::Stay}, std::narrative::ChoiceArm {"Leave", true, crate::Route::Leave}]}
  0004 STEP r1, r4
  0005 ISVAR r4, r1, crate::Route::Leave
  0006 JMPIF r4, @0009
  0007 CALL r4, (), crate::Visit
  …
  0027 RAND r1, crate::Coin
  …
fn crate::count [instant] params=0 regs=3
  0000 REFSTAT r1, crate::VISITS
  0001 INT r2, 1
  0002 IADD r0, r1, r2, i128
  0003 ICLAMP r1, r0, Int<0,3>
  0004 REFSTAT r2, crate::VISITS
  0005 STORE r2, r1
```

`VISITS += 1` on an `Int<0, 3>` computes in `i128` and clamps, as MIR's `IntClamp` says.

## Diagnostics

| Code | When |
| ---- | ---- |
| `E7001` `missing-main` | The root module has no parameterless `fn main` |
| `E7002` `function-too-large` | A function needs more than 65 535 registers or more code words than the format can count |
| `E7003` `invalid-bytecode` | The verifier rejects the module, a static has no constant value, or another violated MIR invariant (internal compiler error) |
| `E4xxx` | A const-evaluation fault found only while emitting (a generic constant's instantiation) |

## Invariants

- Emission runs only on a program without errors, and its output passes `kagari_bytecode::verify`;
  a verifier failure is always a compiler bug.
- The output is deterministic: every table is interned on first use during a deterministic
  traversal (statics, then roots in item order, then FIFO). Hash maps are lookups only, except the
  debug file list, which is sorted by id.
- A constant refers only to lower-indexed constants; statics have their ids before any constant
  is interned.
- Only reachable functions are emitted; generic functions are emitted per instantiation.
- Codegen follows MIR exactly: it adds no semantics of its own beyond choosing instructions, so
  const evaluation and the story analysis (which run the same MIR) agree with the VM.

## When you change this

- **A new instruction or intrinsic.** Add it to `kagari-bytecode` (encoding, decoding, verifier,
  disassembler), to the VM, and to codegen; bump the format version if it is not backward
  compatible; update [The `.kagari` Format](../formats/kagari.md) and [The VM](../runtime/vm.md).
- **A new builtin lowered as an import.** Nothing changes in codegen, but the engine must provide
  the import under the builtin's std path ([Builtins and the Host ABI](../std/builtins.md)).
- **A new table or module field.** `Emitter`, `finish`, the encoder/decoder and its round-trip test
  in `tests/bytecode.rs`, the format chapter.
- **Anything that changes the emitted bytes** (layout, register allocation, instruction choice)
  changes the script hash of every game, which invalidates existing saves' snapshots and closure
  restoration ([Saves, Traces and Snapshots](../runtime/saves.md)); review the bytecode snapshots
  (`cargo test -p kagari-lang --test bytecode`, then `cargo insta review`).
- **A new `E7xxx` code.** `DiagnosticCode`, a test in `emit/tests.rs`, one entry per language in the
  Reference's diagnostics appendix.
