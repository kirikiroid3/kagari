# Design Decisions

**Where in the code**

- Language rules: `core/kagari-syntax` (forms, `.kis`/`.kix`), `core/kagari-lang/src/typeck/kinds*`
  (call rules, finite types, second-class references and closures), `core/kagari-lang/src/signatures`
  (traits, `Eq`).
- Pipeline and IRs: `core/kagari-lang/src/{pipeline.rs,hir,infer,thir,mir}`.
- Host ABI: `core/kagari-std/kis/std/**/*.kis` (`#[builtin]` items), `runtime_libs/kagari-engine/src/host`.
- Trust boundary: `core/kagari-bytecode/src/verify.rs`, `runtime_libs/kagari-vm/src/{program.rs,vm.rs,trap.rs}`.
- Saves: `runtime_libs/kagari-engine/src/saves`.

This chapter records the decisions that shape Kagari, each as a short record: the **Decision**,
**Why**, and the **Consequences** including the alternatives that were rejected. Each record has a
status:

| Status | Meaning |
| ------ | ------- |
| Implemented | The code does what the record says |
| Partly implemented | The record names which part exists; the rest is planned |
| Planned | Decided, not built; nothing in the code depends on it yet |

Most decisions come from the engine proposal (see [Relation to the Proposal](./proposal.md)); the
rest were made while building the compiler and the runtime (`git log` from 2026-10-22 on).

## Language

### One language with function kinds {#one-language}

*Status: Implemented.*

**Decision.** A game is written in one Rust-like language. Story, presentation and game system are
all functions; the **kind** of a function (`fn`, `pushdown fn`, `const fn`) is the only place where
the language restricts what code may do. Kinds are never inferred.

**Why.** The first design (until 2026-10-21) had an XML story script (`.kis`), a separate XML
system scope (`<system>`, `<sysfn>`), Rust-like render files (`.kirp`) and Rust-like extension
files (`.kix`): two scopes, three file kinds, two syntaxes. Every rule had to be stated per scope,
and code could not be shared across scopes. With kinds, one rule (the call rules) keeps the story
closed, and everything else is ordinary code.

**Consequences.**

- The call rules hold for direct calls, methods, builtins, function pointers and closures, because
  pointer and closure types carry the kind (`fn(A) -> B`, `const fn(A) -> B`,
  `pushdown fn(A) -> B`, `scene fn(A) -> B`). A violation is `E3001`.
- The bytecode carries the kind of every function and import, and the load-time verifier checks
  the call rules again (see [The trust boundary](#trust-boundary)).
- Rejected: keeping the XML story for tool-friendliness and node-editor round-tripping. What that
  gave up is in [Relation to the Proposal](./proposal.md#what-the-xml-gave-up).
- Rejected: inferring `const`. The compiler never assumes it. A lint that suggests `const` for a
  qualifying `fn` is planned; none exists yet.

### The story is a pushdown system over finite data {#pushdown}

*Status: Implemented.*

**Decision.** The story is made of `pushdown fn` code. Everything a `pushdown fn` holds at run time
(locals, parameters, return values, story variables) is of a *finite type*. Calls, including
recursive calls between scenes, are allowed; `goto` is not.

**Why.** Reachability is decidable for a pushdown automaton; undecidability comes from unbounded
data (two counters already make a Minsky machine). Finite data plus an unbounded call stack keeps
the questions the engine needs ("can this step be reached with `MET_B` false?", "does every step
terminate?") decidable, and recursion buys succinctness: shared scenes are written once instead of
once per call site.

**Consequences.**

- Holding a non-finite value in story code is `E3008`. Finite types are `bool`, `()`,
  `Int<lo, hi>`, enums, structs, tuples and arrays of finite types, `Option` of one, `Text`, asset
  types, function pointers, and `&T`/`&mut T` parameters; see
  [Finite types](../../../reference/en/src/concepts/kinds.md).
- Generics are monomorphized, so a generic story is still a finite program.
- Scenes (`#[scene]`) are the components of the recursive state machine the analyzer explores.
  Instant procedures (a `pushdown fn` without `#[scene]`) never step, never recurse (`E3005`) and
  have only bounded loops (`E3004`, `E3006`), so each call is one transition inside a scene.
- Every loop and call cycle of a scene must pass through a step (`E5001`, `E5002`).
- The analysis is explicit-state (proposal phase 2) with a state cap (`state_cap`, default
  200 000; `W5006` past it). Symbolic reachability with decision diagrams is planned; see
  [Open Questions](../process/open-questions.md).
- The run-time call stack is capped by `recursion_depth` (default 2048), written into the
  bytecode as `max_call_depth`. The proposal's alternative, capping recursion at compile time to
  get a finite automaton, was rejected.
- Rejected: a finite automaton without calls (no succinctness); unbounded integers in the story
  (undecidable). `Int<lo, hi>` compound assignment saturates instead of overflowing, so every step
  stays total.

### Presentation is a `const fn` the story never reads back {#presentation}

*Status: Implemented.*

**Decision.** Each step may carry a render, a `const fn(Screen) -> Screen`, passed as a function
pointer or a non-capturing closure. The story never reads the screen: `Screen` is not a finite
type, so no `pushdown fn` can hold one.

**Why.** Imperative presentation commands create hidden φ-nodes at merge points: the screen
depends on the path, not on the story state. Renders that see only the screen and are evaluated at
compile time make the screen at each step computable, so the compiler can report a screen that
depends on the path (*consistency*) and can find keyframes (renders that ignore their input).

**Consequences.**

- Renders are blind to story variables by construction: a `const fn` cannot read them, and a
  render argument is a function pointer, so it captures nothing.
- The compiler evaluates every render on every screen that reaches its step (stage 5). A screen
  that depends on the path is `W5004`. A render that grows a step's set of screens past the fixed
  cap of 64 (`SCREEN_CAP` in `src/analysis/story/mod.rs`) is `E5005`; the cap is an
  [open question](../process/open-questions.md).
- Screen components must have decidable equality, which is one reason there are no floats
  ([below](#numbers)).
- Animations are not part of the logical screen and are skipped on replay.
- Keyframes are detected at compile time (stage 4). The runtime does not use them yet: a load
  replays every render from the start (see [Saves](#saves)).

### Turing-complete code enters the story only through entry scenes {#entry-scenes}

*Status: Implemented.*

**Decision.** A `fn` cannot call a `pushdown fn`. Its only way into the story is to start an
`#[entry]` scene through `nav::start`, with arguments of finite types. The only ways back are the
entry scene's return value, which the system's exit handler receives, and meta that the story
writes (`Meta::write`) and never reads.

**Why.** With a single door, everything from the start of the story to any step is pushdown code
over finite data, and the analyzer only needs the functions reachable from the entry scenes. It
quantifies over all values of the entry arguments, so the guarantees hold for every run the system
can start.

**Consequences.**

- `nav::start` has a special signature checked by the compiler (`E2027`); see the STD Reference,
  [Compiler Rules for std Items](../../../std/en/src/appendix/rules.md).
- The system sees a running story only through the opaque `Narrative` handle.
- Story variables used outside story code are `E3007`.
- The menus, saves and settings code is ordinary Turing-complete code with first-class closures,
  signals and collections: no narrative analysis depends on it.

### Mutable value semantics; second-class references and closures in the story {#values}

*Status: Implemented.*

**Decision.** Every type is a value type: assignment, argument passing and return copy. There are
no lifetimes, no borrow checker, no moves and no destructors. `&T`/`&mut T` are shared pointers
to places. In `fn` and `const fn` references and closures are first-class. In `pushdown fn` they
are second-class: references only as parameters (`E3014`), closures only passed down and called
(`E3015`), and a place passed as `&mut` may not also be passed as another argument of the same call
(`E3016`).

**Why.** A borrow checker is a large cost for studio code that runs in a VM, where aliasing can
surprise a program but never corrupt memory. In the story, second-class references make a
`&mut x` argument behave exactly like an in-out parameter of a finite type, so story state never
holds a pointer. A first-class closure in the story would put captured environments into the
state and make the story a higher-order recursion scheme, whose model checking is
n-EXPTIME-complete for order n (Ong, LICS 2006). Named function pointers do not have this problem:
there are finitely many named functions.

**Consequences.**

- The VM stores values copy-on-write; programs cannot observe sharing.
- References are `Ref { root, path }` in the VM; chains of references collapse
  (see [The `.kagari` Format](../formats/kagari.md)).
- Closures in the story are instant: they cannot step or call a scene, so every step is a point
  in a named scene.

### No floats, exact numbers, and no `goto` {#numbers}

*Status: Implemented.*

**Decision.** There are no floating-point types. Story code has `Int<lo, hi>` (a primitive with
`i64` bounds). Turing-complete code also has Rust's integers, `Decimal` (the type of `0.5`) and
`Rat` (`Rat::new(num, den)`). Integer conversions are explicit (`into`, `saturate`); `as` is
`E2019`. Control flow is structured: no `goto`, no labels as positions.

**Why.** Screens must compare exactly, and floats are not `Eq` (`NaN != NaN`). Exact numbers also
give identical results on every platform, which deterministic replay of saves needs. Structured
control flow makes irreducible control flow inexpressible, which keeps the analyses and the
load-time checks of bytecode cheap.

**Consequences.**

- `Decimal` and `Rat` live in `kagari-num`, shared by the compiler and the VM, so const
  evaluation and run time compute the same values. Both are `i128` based; `Rat::to_decimal` and
  `Decimal` division truncate to 7 digits.
- Overflow, division by zero and out-of-range results are const-evaluation errors (`E4001`,
  `E4002`) and VM panics at run time. Compound assignment on `Int<lo, hi>` saturates.
- There is no default `Int` range and no prelude alias for one.
- The renderer uses floats internally; that is presentation, never game state.

### No user traits, automatic `Eq`, no `derive` {#traits}

*Status: Implemented.*

**Decision.** Only `std` declares traits, all `#[builtin]` (`Eq`, `Integer`, `RandChoose`, `Asset`,
`Step`, `ToText`), and only `std` implements them. `Eq` is automatic for every type whose fields are
all `Eq`; closures are never `Eq`. User-declared generics exist; user traits do not.

**Why.** Traits interact with kind checking and tabulation in ways the design has not worked out,
and no studio use case needed them. Automatic structural `Eq` is what screen comparison and
consistency need, and it removes `derive` boilerplate.

**Consequences.**

- A `trait` item outside std is `E1012`, a non-`#[builtin]` trait `E1013`, an `impl Trait for T`
  outside std `E1023`. `==` on a non-`Eq` type is `E2017`.
- Without user traits there is no overloading; std signatures are type-checkable without coercion,
  and the few special signatures are listed in the std–checker contract
  ([The std–Checker Contract](../std/contract.md)).
- User traits are an [open question](../process/open-questions.md).

### `.kis` and `.kix` differ only in syntax {#file-kinds}

*Status: Implemented.*

**Decision.** A `.kix` file is a `.kis` file that may also contain forms (`s!(…)`, `choice!{…}`,
`t![…]` and the UI forms). The extension never decides what code may do; any kind may appear in
either file. Forms are a fixed set built into the parser, not macros; there are no macros.

**Why.** The first design tied semantics to file kinds. Tying only syntax to the extension keeps
forms out of plain code (where `!` would be confusing next to Rust habits) without a second rule
system.

**Consequences.**

- The parser accepts forms in both kinds; the syntax validator reports them in `.kis`
  ("forms are only allowed in `.kix` files", `E0001`).
- Forms are desugared during HIR lowering (`src/hir/forms.rs`), so later stages see ordinary
  calls and struct expressions.
- Adding a form changes the lexer/parser, HIR lowering, the formatter, the tree-sitter grammar and
  the Reference in both languages.

## Standard library and host

### `#[builtin]` std items are the host ABI {#builtin-abi}

*Status: Implemented.*

**Decision.** The standard library is Kagari source (`core/kagari-std/kis/std`). Items marked
`#[builtin]` have no body (functions) or no visible fields (types); the engine implements them.
Each builtin function the compiler does not turn into an intrinsic instruction becomes an import
named by its std path, such as `std::narrative::Meta::write`.

**Why.** The std path is a stable, readable name for the ABI between bytecode and host, and the
declarations double as the user-facing signatures. Marking ABI items distinguishes them from
ordinary std code written in Kagari (`std::stage`, for example).

**Consequences.**

- `#[builtin]` outside std is `E1008`; a builtin with a body is `E1009`.
- Renaming or moving a builtin breaks every compiled game: the engine binds imports by path at
  boot and fails with `BootError::UnsupportedImport` for an unknown one.
- Details: [Builtins and the Host ABI](../std/builtins.md).

### Builtin types are opaque handles {#opaque-handles}

*Status: Implemented.*

**Decision.** Builtin types with engine state (`Screen`, `Entity`, `Narrative`, `Element`,
`Divergence`, …) are opaque at run time: a `Host` value the program can pass back to imports but
cannot look into.

**Why.** The engine's internal layout must not leak into bytecode, so it can change without a
bytecode version bump.

**Consequences.** LOAD, STORE, REFFIELD and REFINDEX on a host part are VM panics. Equality on host
objects is defined by the engine (`HostObject::eq_object`).

### The engine build is the trust boundary {#trust-boundary}

*Status: Implemented.*

**Decision.** Bytecode cannot load native code. The only native code a game reaches is the set of
std imports compiled into the engine. A loader decodes and verifies every module before running it.

**Why.** A `.kagari` file may be hand-crafted or malicious. Restricting it to verified bytecode
plus a fixed host table means a game can only do what the engine build allows, and the VM has no
raw memory to corrupt.

**Consequences.**

- `kagari_vm::Program::load` decodes, verifies (`kagari_bytecode::verify`) and rejects any
  required feature it lacks; the engine then binds every import by std path.
- The verifier checks indices, acyclic constants, register ranges, no fall-through, argument
  counts and the call rules of function kinds. Value types are not checked statically: values are
  tagged and a mismatch is a VM panic.
- The compiler verifies its own output before writing it (`E7003` if that fails).
- Details: [The `.kagari` Format](../formats/kagari.md) and [The VM](../runtime/vm.md).

### Run-time limits: budgets and caps {#vm-limits}

*Status: Partly implemented.*

**Decision.** Every VM run is bounded: an instruction budget, a call-depth cap and, planned, a
heap cap.

**Why.** Turing-complete system code can loop forever; a game must stop in a defined way instead
of freezing the window (or, on a future web runtime, the browser tab).

**Consequences.**

- Implemented: `FUEL_PER_RUN` = 100 000 000 instructions per top-level run (`Vm::set_fuel_per_run`
  changes it); nested runs started by imports share it. Exhausting it is the trap
  `FuelExhausted`. `REPEAT` charges its count.
- Implemented: `max_call_depth` from the bytecode, counted across nested runs and story frames
  (`CallDepth` trap). Reference chains longer than `MAX_REF_HOPS` also trap.
- Implemented: a save replay runs at most 1 000 000 steps (`Divergence::TooLong`).
- Planned: a heap cap. Planned: cooperative scheduling, where a run that exhausts its budget yields
  instead of trapping.

### Saves are traces; snapshots are caches {#saves}

*Status: Partly implemented.*

**Decision.** A save is a *trace*: the entry scene's path and arguments, the stable choice IDs of
every selection, the step count after the last choice, the PRNG seed and the script hash. The
trace is the compatibility contract across script versions. A *snapshot* of the story state and
the screen is only a cache, trusted only when the script hash matches.

**Why.** A trace is a witness that a position is reachable, so no save of an unreachable position
can exist, and it survives script changes as long as its choice IDs resolve. The VM's internal
state layout then never has to stay stable across versions.

**Consequences.**

- Implemented: the trace with choice IDs `function path/choice id/arm`, where the arm is the
  variant path of the arm's enum value. Loading replays the trace from the entry scene and reports a
  `Divergence` at the first ID that no longer resolves.
- Implemented: the script hash is the lowercase hex SHA-256 of the bytecode. Today it decides only
  whether a closure exit handler recorded in the save can be trusted.
- Planned: snapshots at choices, replay bounded by keyframes, and preview/hot reload. Today every
  load and rewind replays from the start with every render. See [Saves, Traces and
  Snapshots](../runtime/saves.md).
- The story PRNG (xoroshiro128++, seeded from the trace) is the only randomness; only scenes draw
  (`E3024`), and the analysis explores every outcome.

### Extension types are identified by path and structural hash {#extensions}

*Status: Planned.*

**Decision.** Studios will extend the screen with *extensions*: data types declared in Kagari,
systems implemented natively in the engine, and native functions in the import table. An extension
type is identified by its path plus a hash of its structure, so a game built against one version
of an extension is rejected by an engine that has a different one.

**Why.** Identifying by path alone lets a changed layout be misread silently.

**Consequences.**

- Nothing of this exists yet. The std component set is fixed (studios add no components; they
  extend `std` with `const fn` helpers, as `std::stage` does).
- The bytecode reserves the Features section for required engine extensions. The compiler emits
  none, and the VM rejects any module with a required feature
  (`LoadError::RequiredFeature`).

## Compiler architecture

### HIR says what something is; MIR says what happens along a path {#hir-mir}

*Status: Implemented.*

**Decision.** Item trees, def maps, signatures, HIR bodies and type inference answer what an item
or expression *is*: its name, type, kind. MIR, a control-flow graph of basic blocks over locals
and places, answers what *happens* along a path: evaluation order, branches, suspensions. Between
them, THIR is the typed tree that MIR building reads (coercions, auto-derefs and auto-borrows made
explicit).

**Why.** The checks that a studio sees as type and kind errors need names and types, not control
flow. The analyses need control flow, not syntax. Early design notes called the MIR "KIR"; the code
calls it MIR (`kagari_lang::mir`).

**Consequences.**

- Type, pattern and kind checks run on HIR plus the inference result; no THIR is needed for them.
- Termination, liveness, influence, instant recursion, system checks, const evaluation, story
  exploration and codegen run on MIR.
- Details: [Name Resolution and HIR](../compiler/resolve.md), [THIR and MIR](../compiler/mir.md).

### Checks are independent queries that run before lowering {#checks-as-queries}

*Status: Implemented.*

**Decision.** Every check is a salsa query per item (`pattern_checks`, `kind_checks`, `infer`,
body resolution) or per program. THIR and MIR are built only for items whose resolution and
inference have no errors. Whole-program analysis (stage 5) runs only when stages 1–4 report no
error, and bytecode is emitted only when `check` reports no error.

**Why.** Lowering code stays simple because it may assume well-typed input; diagnostics stay
independent of lowering; the language server can show errors without building MIR. Query results
hold no text ranges (diagnostics carry anchors resolved in `check`), so editing inside one item
re-runs only that item's queries and those of its users.

**Consequences.**

- A new check is a new query or part of an existing per-item one, and `check` in
  `src/pipeline.rs` collects it.
- `tests/incremental.rs` guards incrementality: editing one scene re-runs only that scene's
  queries.
- Details: [Pipeline and Queries](../compiler/pipeline.md).

### One semantics: everything runs on MIR {#one-semantics}

*Status: Implemented (since 2026-10-27, commit `e6ee978`).*

**Decision.** Const evaluation and the story exploration run a MIR interpreter, and codegen emits
bytecode from MIR. The language's semantics are defined once.

**Why.** Before, an HIR interpreter, a story CFG and an HIR-to-bytecode lowering implemented the
semantics three times, and they drifted.

**Consequences.** Rejected: separate lowering paths per consumer. A semantic change touches MIR
building (and the VM if it adds an instruction), not three places.

### `match` compiles to decision trees {#decision-trees}

*Status: Implemented.*

**Decision.** MIR building compiles patterns to decision trees with Jacobs' algorithm
(`src/mir/build/matches.rs`). Exhaustiveness, unreachable arms and refutability are checked
separately, on HIR, with Maranget-style usefulness and constructor splitting
(`src/typeck/exhaustive.rs`).

**Why.** Decision trees test each place once and give plain `SwitchVariant`/`SwitchBool`
terminators that every MIR consumer understands. Exhaustiveness is a check, so it belongs with the
other checks before lowering.
