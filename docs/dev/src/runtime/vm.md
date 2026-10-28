# The VM

`runtime_libs/kagari-vm` interprets verified `.kagari` modules. It knows nothing about screens,
audio or UI: everything engine-specific reaches it through the `Host` trait. The engine
(`runtime_libs/kagari-engine`, see [The Engine](./engine.md)) owns the VM, runs `main` at boot,
drives the story fiber step by step and implements every import.

Where in the code:

- `src/program.rs`: `Program::load` (decode, verify, precompute) and `LoadError`.
- `src/vm.rs`: `Vm` (program, type table, call depth, fuel), `call`, `call_value`,
  `with_fresh_budget`.
- `src/exec/mod.rs`: frames, the instruction loop, calls, STEP/RAND/REFSTAT.
  `src/exec/data.rs`: data instructions.
- `src/value.rs`: `Value`, `Adt`, `AdtTy`, `Closure`, `Text`, `Ref`, `RefRoot`, `Proj`.
- `src/refs.rs`: reference resolution, reads, copy-on-write writes.
- `src/int.rs`, `src/ops.rs`: integer, Decimal, Rat, comparison and TOTEXT semantics.
- `src/story.rs`: `StoryFiber`, `StoryStatics`, `Suspend`. `src/rng.rs`: the PRNG.
- `src/host.rs`: `Host`, `HostObj`, `HostObject`, `HostFn`. `src/types.rs`: `TypeTable`.
- `src/trap.rs`: `Trap`, `TrapKind`.
- Tests: `src/tests.rs`, `tests/vm.rs`, `tests/story.rs` (with a `TestHost` in
  `tests/common/mod.rs`).

The byte format and the per-opcode semantics are in [The `.kagari` Format](../formats/kagari.md);
this chapter covers the machine around them.

## Design

**Decision.** A register machine over tagged values with value semantics, no raw memory and no
garbage collector: aggregates are `Rc` and copied on write; cells are `Rc<RefCell<Value>>`.

**Why.** Kagari has mutable value semantics with second-class references, so values never form
cycles except through a cell that stores a reference to itself (bounded, see below). Reference
counting is then exact and deterministic. Tagged values make every operation checkable, so a
malformed program can only panic, never corrupt memory; the verifier only has to guarantee what
the loop does not re-check (see [Relying on the verifier](#relying-on-the-verifier)). A register
VM matches the MIR the compiler lowers from (places and temporaries map to registers).

**Consequences.** The VM is single-threaded (`Rc`). Copying a value is O(1); the first write to
a shared aggregate clones one level (`Rc::make_mut`). `Value::Host` objects follow the same
rule through `HostObj::make_mut`.

## Loading

`Program::load(bytes)`:

1. `decode`, then `verify` (both from `kagari-bytecode`); failures are `LoadError::Decode` /
   `LoadError::Verify`.
2. Refuse any feature with the required flag (`LoadError::RequiredFeature`): this VM implements
   no optional features.
3. Build every constant as a `Value` in index order (children are clones of earlier entries).
   A Rat with `den == 0` or a text constant with non-Text parts is `LoadError::BadConstant`.
4. Precompute: function and type indexes by path (`find_function`, `find_type`; the lowest id
   wins on duplicates), the `main` function, per function the instruction indices of its STEPs
   (for step ordinals), per function the `(instruction, byte offset)` debug pairs, and
   `std::Option`'s type id with a cached `None` (for VECGET). Unit, the empty array, the empty
   Vec and the empty path are shared values.

The engine boots by `Program::load`, binding every import to an engine implementation (an
unknown import path fails the boot with `BootError::UnsupportedImport`), then running `main`.

## Values, cells and references

`Value` has one variant per run-time tag (`Uninit`, `Bool`, `Int`, `Decimal`, `Rat`, `Text`,
`String`, `Tuple`, `Array`, `Vec`, `Adt`, `Asset`, `Func`, `Closure`, `Handle`, `Ref`, `Host`,
`HostFn`). There is no separate heap object table: a "heap cell" is an
`Rc<RefCell<Value>>` created by CELL, and a story variable is a cell in `StoryStatics`.

`Ref { root, path }`: `root` is `RefRoot::Cell` or `RefRoot::Host` (a host part handed out by
an import); `path` is `Proj::Field(u16)` / `Proj::Index(u64)` projections.
`refs::resolve` turns a reference into an equivalent one with no reference along its path or at
its target, following stored references ("references to references collapse"). A self-referential
cell would loop forever; resolution stops after 65 536 hops with `FuelExhausted`. Writes go
through `refs::with_mut`, which unshares every aggregate along the path. A cell already
borrowed (a re-entrant write through an alias) is a host trap ("cell already borrowed").

`AdtTy` is either `Module(TypeId)` or `Synthetic(u32)`. The engine builds std values (an
`Option`, a `Duration`, …) even when the program never interned that type; `TypeTable::resolve`
returns the module's id when the path exists and otherwise registers a stable synthetic type
with the given variant names. Equality compares `AdtTy`, so both sides must resolve the same
way; the engine resolves all std types once at boot (`StdTypes::resolve`).

## Frames and runs

A `Frame` is `{ func, pc, regs }`. Parameters arrive in `r0..params`; every other register
starts `Uninit`. `pc` is an instruction index and advances only once an instruction completes,
so a caller's `pc` stays on its CALL while the callee runs; RET pops the frame and writes the
value into the `dst` of the caller's CALL or CALLVAL.

Each *run* executes on its own `Vec<Frame>` until the bottom frame returns or a STEP suspends:

| Entry point | Used for | Kinds |
| ----------- | -------- | ----- |
| `Vm::call(host, func, args)` | `main`, renders, UI callbacks, any engine call into bytecode | any non-scene; arity checked (`KindMismatch`) |
| `Vm::call_value(host, callee, kind, args)` | calling a `Func`/`Closure`/`HostFn` value the engine holds (signals, `on_exit`) | CALLVAL rules |
| `StoryFiber::resume(vm, host, input)` | the story | the entry scene and everything it calls |

Runs nest: an import may call back into the VM (`Vm::call` is re-entrant). Nested runs share
the call depth and the fuel of the outermost run.

On a trap, every frame of the run is unwound into `Trap::backtrace` as
`(function, instruction index)`, innermost first, and the run's stack is cleared.

## The story fiber

The story state σ is *frames + story statics + PRNG* (`StoryFiber`). `StoryFiber::start(vm,
entry, args, statics, seed)` checks that `entry` is a scene and creates the first frame; nothing
runs yet. `StoryStatics::fresh(program)` makes one cell per story-variable static from its init
constant.

`resume(vm, host, input)` runs until the next STEP or the entry scene's return:

- On a STEP, the run returns `Suspend::Step { record, func, ordinal }`: the step record
  (auto-deref'd), the function containing the STEP, and the STEP's ordinal among that
  function's STEPs in code order. The top frame's `pc` stays on the STEP.
- The next `resume` writes `input` (the step's result: `()` for the first resume and for a
  `Say`, the chosen arm's `value` for a `Choice`) into the STEP's `dst` register and continues.
- `Suspend::Returned(value)` ends the fiber (the ending value). A fiber that returned or trapped
  refuses further resumes.

STEP, RAND and REFSTAT outside a story run are `KindMismatch` (the verifier already restricts
them to scenes and instant procedures; a plain `Vm::call` of an instant procedure has no story
context).

## Budgets {#budgets}

| Limit | Value | Where | On excess |
| ----- | ----- | ----- | --------- |
| Instruction budget (fuel) | `FUEL_PER_RUN` = 100 000 000 per top-level run, changeable with `Vm::set_fuel_per_run` | 1 per executed instruction; REPEAT additionally costs its count | `FuelExhausted` |
| Call depth | `Options::max_call_depth` (compiler option `recursion_depth`, default 2048) | every pushed frame, across nested runs and the fiber's suspended frames | `CallDepth` |
| Reference chain | 65 536 hops | `refs::resolve` | `FuelExhausted` |
| Registers per frame | 65 535 | the format (`u16`) | `E7002` at compile time |

The outermost run refills the fuel (`Vm::enter` at nesting 0). `with_fresh_budget` runs host
work, such as replaying a save inside an import, as if it were top-level: each run inside gets a
full budget, and the enclosing run's remaining fuel is restored afterwards.

A resumed fiber re-adds its suspended frames to the depth before running and subtracts them when
it suspends again, so story recursion counts against `max_call_depth` like any other.

There is no heap cap: memory is bounded only indirectly (fuel bounds how much a run can
allocate, REPEAT is charged per element). See [Open Questions](../process/open-questions.md).

### Scheduling

The VM does not preempt. The engine calls into it synchronously from its frame update: a story
run lasts from one STEP to the next, a render or UI callback runs to completion. Exhausting the
budget is a trap, not a yield; the budget exists so that a runaway loop in `fn` code stops the
game with a message instead of freezing the window or a browser tab. Story code cannot loop
without a step (the compiler's stepless-loop and stepless-recursion checks, `E5001`/`E5002`),
so normal story runs are short.

## The host interface {#host}

```rust
pub trait Host {
    fn import(&mut self, vm: &mut Vm, import: ImportId, args: Vec<Value>) -> Result<Value, Trap>;
    fn call_host_fn(&mut self, vm: &mut Vm, f: &HostFn, args: Vec<Value>) -> Result<Value, Trap>;
}
```

- CALLIMP calls `Host::import` with the import id and the **raw** argument registers: a
  reference argument (`&self`, `&mut Screen`) arrives as a `Value::Ref`; the import reads it
  with `Vm::load` or edits through `Vm::with_mut`. The engine binds each `ImportId` to an
  implementation once at boot by the import's path (see
  [Builtins and the Host ABI](../std/builtins.md)).
- `Value::Host(HostObj)` carries an engine object (`Screen`, `Element`, `Narrative`, …).
  `HostObject` provides `clone_object` (deep copy for copy-on-write), `type_name` (for traps),
  `eq_object` (structural equality for EQ; `None` means not comparable, which panics) and
  downcasting. Copies share the `Rc`; an import that mutates calls `HostObj::make_mut`.
- A host *part* is a `Ref` whose root is `RefRoot::Host`: an import returns `&mut Entity`
  or `&mut Prop<T>` this way. Bytecode can only pass it back to imports.
- `Value::HostFn { id, params }` is a callable the engine hands to bytecode (a signal getter or
  setter). CALLVAL with expected kind `fn` dispatches it to `Host::call_host_fn`; any other
  kind is `KindMismatch`. EQ on it panics like on closures.
- `Trap::host(message)` is how an import rejects arguments; it becomes `TrapKind::Host`.

## Traps {#traps}

`TrapKind` values and their causes are listed in
[The `.kagari` Format](../formats/kagari.md#panics). `Trap::render(program)` produces
`vm panic: {kind} at {path} ({file}@{offset})`, then one `called from …` line per outer frame;
without debug info the location is `instruction {i}`. The engine stops the game on the first
trap and shows the message on the crash screen; later traps are ignored.

## Relying on the verifier {#relying-on-the-verifier}

The interpreter never uses unchecked indexing (the crate denies `unwrap`, `panic` and
unchecked arithmetic), but it relies on `verify` for *meaning*:

- Register numbers, table indices and jump targets are in range, and code cannot fall off its
  end. If one were not, the loop reports `fault()`, an `Unreachable` trap that indicates a
  verifier bug, not a program error.
- Call arity of CALL and CALLIMP, the closure flag of CLOSURE, and the function kind rules are
  not re-checked in the loop; CALLVAL's are, because the callee is a run-time value.
- The `main` flag is unique and `main` takes no parameters; entry functions are scenes.
- Constants refer only to lower constants, so the one-pass constant build in `Program::load`
  is complete.

Everything about value *types* is checked at run time, so the verifier does not need a type
system. The engine trusts nothing more: imports re-check the tags and std types of their
arguments and trap with `Host` or `TagMismatch`.

## Invariants

- No `unsafe`, no panics: every failure is a `Trap` or a `LoadError`.
- Values have value semantics: only `Ref` aliases. Any new aggregate or host object must
  copy on write.
- A STEP's ordinal and the function path are what the engine records about the story's
  position; do not renumber STEPs without considering saves (see
  [Saves, Traces and Snapshots](./saves.md)).
- Determinism: identical program, seed and inputs give identical runs (no floats, no hash
  iteration order in semantics, the PRNG specified in the format chapter).

## When you change this

- New instruction or semantics change: [The `.kagari` Format](../formats/kagari.md) first, then
  `exec/`, tests in `tests/vm.rs`; load the `kagari-bytecode` skill.
- New budget or limit: document it here and in the Reference's runtime chapter if a studio can
  hit it.
- Host interface change: update every `Host` implementation (`kagari-engine/src/host/mod.rs`,
  `kagari-engine/src/test_support.rs`, `kagari-vm/tests/common/mod.rs`).
