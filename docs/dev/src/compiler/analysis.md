# Story and System Analysis

Stage 5 checks whole-program properties. The story is a recursive state machine over finite
data, so its questions (does every loop reach a step, can this choice offer nothing, which screens
reach this step) are decidable, and the compiler answers them by running the story on every
reachable state. The game system is Turing-complete, so its checks stay on the mode graph and
never quantify over values. This chapter describes the algorithms, their state, their limits and
where they report.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/analysis/mod.rs` | `analysis(db, program)` (salsa): story, influence and system checks; `story_graph` |
| `core/kagari-lang/src/analysis/story/mod.rs` | `story_analysis` (salsa, `no_eq`): collects scenes and story variables, runs the story analyses in order and keeps the explored graph; `SCREEN_CAP` |
| `core/kagari-lang/src/analysis/story/graph.rs` | The explored graph as `kagari_debug::StoryGraph`, the form `.kagari-cache` files store |
| `core/kagari-lang/src/cache.rs` | `source_fingerprint`, `analysis_diagnostics`, reusing an offered analysis cache |
| `core/kagari-lang/src/analysis/story/termination.rs` | Stepless loops (`E5001`) and stepless call cycles (`E5002`) |
| `core/kagari-lang/src/analysis/story/liveness.rs` | `scene_kept` (salsa, per scene): the locals each suspension keeps |
| `core/kagari-lang/src/analysis/story/segment.rs` | Running a scene from one suspension to the next in the MIR executor |
| `core/kagari-lang/src/analysis/story/explore/mod.rs` | The exploration: contexts, worklist, stuck choices, caps |
| `core/kagari-lang/src/analysis/story/explore/model.rs` | The explored state space (`Explored`, `Context`, `Node`, interners) |
| `core/kagari-lang/src/analysis/story/explore/report.rs` | `W5006`–`W5009` and `E5003` reporting, witnesses |
| `core/kagari-lang/src/analysis/story/screens.rs` | Screens per step: render errors, bounded screens (`E5005`), consistency (`W5004`) |
| `core/kagari-lang/src/analysis/story/keyframes.rs` | Opt-in keyframe placement check (`W5011`) |
| `core/kagari-lang/src/analysis/story/tests.rs` | Unit tests of the keyframe check |
| `core/kagari-lang/src/analysis/influence.rs` | Unused story variables (`W5010`): dependence graph over all `pushdown fn` |
| `core/kagari-lang/src/analysis/system/mod.rs` | The mode graph and its checks (`W6001`–`W6004`) |
| `core/kagari-lang/src/analysis/system/code.rs` | The code of each mode: transitive closure of per-item summaries |
| `core/kagari-lang/src/analysis/system/summary.rs` | `system_summary` (salsa, per item): navigation operations, calls and bindings from MIR |
| `core/kagari-lang/tests/game_source/analysis/*` | Snapshot fixtures: system, bindings, influence, termination, story, screens, rng |
| `core/kagari-lang/tests/analysis_cache.rs` | The persisted graph (step IDs, arm IDs, draws, kept locals) and reuse of a cached analysis (salsa events) |

## When stage 5 runs

`pipeline::check` collects the diagnostics of stages 1–4 (`front_diagnostics`) and asks for the
stage-5 diagnostics (`cache::analysis_diagnostics`) only when none of them is an error
(`analysis_runs`): the analyses execute the story and assume well-typed, well-kinded code.
`analysis_diagnostics` returns the diagnostics of an offered analysis cache when its fingerprint
matches (see [Reusing the analysis](#reusing-the-analysis)), else resolves those of `analysis`.
Inside `analysis`, the three parts are independent:

```rust
let mut diagnostics = story::story_analysis(db, program).diagnostics.clone();
diagnostics.extend(influence::check(db, program));
diagnostics.extend(system::check(db, program));
```

`story_analysis` runs in a fixed order:

1. Collect every scene (a `#[scene] pushdown fn` with a body, its MIR and kept locals) and every
   story variable (`static mut`), in load order.
2. **Termination.** On any `E5001`/`E5002`, stop: running a story with a stepless loop could never
   reach the next step.
3. **Exploration** from every game entry scene. Without one, stop.
4. **Screens** over the explored states (also when the exploration was incomplete: a difference
   found on reachable states is real).
5. **Reachability** (`W5008`, `W5009`), only when the exploration was complete.
6. **Keyframe placement** (`W5011`), only when `CompileOptions::keyframe_check` is on.

It returns the diagnostics together with the `Explored` graph and the item of every scene. The
query is `no_eq`: the graph is large, has no cheap equality, and every change that reaches it
changes the diagnostics anyway.

Diagnostics are reported only for scenes and story variables of the game crate.

## Termination

Every `loop` and `while` of a scene, and every cycle of scene calls, must pass a step. The check
runs on each scene's MIR alone and treats every branch as possible.

1. **May return without a step.** A least fixpoint over all scenes: a scene may return without a
   step when a path from its entry reaches `Return` passing no `Step` and calling only scenes that
   may themselves return without a step. Each round is a DFS per scene (`reach`); a round flips at
   least one scene or ends the fixpoint, so there are at most as many rounds as scenes.
2. **Stepless loops (`E5001`).** For each `LoopInfo` of a scene, a DFS from the loop header that
   stays inside the loop's region and stops at steps and at calls of scenes that cannot return
   without a step. Reaching the header again is a stepless loop, reported at the loop.
3. **Stepless recursion (`E5002`).** The scene calls reached from each scene's entry without a
   step form a graph; Tarjan's algorithm finds its strongly connected components. A component with
   more than one scene, or one scene with a call to itself, is reported once, at its first call
   site in source order, with the other call sites as related notes.

What counts:

- `Step` stops a path; `Rng` does not (a draw is not input).
- A call of an instant procedure is a statement and so stepless; instant procedures cannot loop or
  recurse (stage 3 checks that).
- `for` loops are bounded and are not in `ItemMir::loops`.
- A call through a `scene fn` pointer may reach every scene whose signature matches the pointer
  type; a parameter or return type mentioning a generic parameter matches anything.

The cost is linear in the size of the scenes' MIR per round.

## Kept locals

A story state must contain exactly what can influence the future, so `scene_kept` (salsa, per
scene) computes, for every suspension block, the locals the state keeps:

- the live-in set of the suspension's successor (`mir::liveness`), minus the suspension's `dest`
  when it is a bare local;
- the locals the suspension itself reads: the step record operand, the callee value and the
  arguments of a scene call (states that differ in the step shown or the call made are different
  states);
- for a reference argument of a scene call, the place it points to rather than the reference;
- every `&mut` parameter of the scene, at every suspension: their final values are part of every
  exit.

A local that dies at a suspension is dropped from the state, which is what keeps the state space
small: two paths that differ only in a dead local merge.

## Exploration

The exploration computes the reachable states of the story, summarizing each scene per
*context*, as reachability of recursive state machines is computed with call/return summaries.

<!-- diagram: analysis-exploration (mermaid-ish)
flowchart LR
  subgraph A["context (Start, args (), globals G0)"]
    entry ==>|segment| s0["Step s!"]
    s0 ==> s1["Step choice!"]
    s1 ==>|Stay| call["Call Visit()"]
    s1 ==>|Leave| r1["Rng Coin (ROUTE = Leave)"]
    call ==>|exit e0| r2["Rng Coin (ROUTE = Stay, VISITS = 1)"]
    r1 ==> exits["exits (ret, globals, outs)"]
    r2 ==> exits
  end
  subgraph B["context (Visit, args (), globals G1)"]
    bentry["entry"] ==> bs0["Step s!"]
    bs0 ==> e0["exit e0: ret ()"]
  end
  call ==>|Call: find or create| bentry
  e0 -.->|Return task per exit| r2
-->
{{#include ../diagrams/analysis-exploration.svg}}

### State representation

Everything is interned (`model.rs`): each `ConstValue` once in a `ValueInterner` (`ValueId`),
each frame once (`FrameId`: the kept locals of a block in ascending order, `None` for an
uninitialized local), each assignment of story variables once (`GlobalsId`: an `imbl::Vector` of
`ValueId`s by variable position, so an update shares structure with its predecessor).

| Concept | Key | Contents |
| ------- | --- | -------- |
| Context (`CtxId`) | `(FnRef, args: [ValueId], GlobalsId)` | A scene entered with argument values and story variables; its entry target, nodes, exits and callers |
| Node (`NodeId`, per context) | `(BasicBlock, FrameId, GlobalsId)` | A suspension state: `Step { expr, render, choice, outcomes, succ }` (`outcomes`: per successor, the arm's position in the choice's arm list and the resume value; `(0, ())` for a `Say`), `Call { callee: CtxId, succ: [(ExitId, Target)] }` or `Rng { draws, succ }` (`draws`: the drawn value per successor) |
| Exit (`ExitId`, per context) | `ExitData { ret, globals, outs }` | A way the context returns: value, story variables, final values of `&mut` parameters |
| `Target` | — | Where a resumption leads: `Node(id)`, `Exit(id)`, or `Dead` (an error, or a path the analysis gave up) |

A **state** is therefore a position (the suspension block), the kept locals of the scene's frame,
and the values of the story variables, inside a context that fixes the arguments. The call stack
is not part of a state: a call node refers to the callee's context, and the caller resumes once
per exit of it.

### Algorithm

The roots are the contexts of every non-generic game entry scene with every combination of its
argument values (`DomainCtx::values`, product capped at `TABLE_DOMAIN_CAP`), all starting from
the story variables' initial values (`static_value`). A LIFO worklist of tasks drives the rest:

| Task | Does |
| ---- | ---- |
| `Start(ctx)` | Runs the context's scene from its entry |
| `Step { ctx, node, outcome, value }` | Resumes after a step or draw node with one outcome value |
| `Return { ctx, node, exit }` | Resumes after a call node with one exit of the callee |

Each task runs one **segment** (`segment::run`) and *settles* where it stopped:

- **`Step`**: the record is frozen. A `Say` has one outcome, `()`; a `Choice` one per arm whose
  `shown` field is `true`. A new node is added and a `Step` task queued per outcome. No outcome is
  a **stuck choice**: the first state found is kept as a witness for `E5003`.
- **`Rng`**: one outcome per value of the drawn type; more than `TABLE_DOMAIN_CAP` values is
  `W5006` at the draw, and the path ends.
- **`SceneCall`**: the callee value is resolved to a scene, its arguments frozen, and the callee's
  context found or created (which queues its `Start`). The caller registers in the callee's
  `callers`, and a `Return` task is queued for every exit the callee already has.
- **`Return`**: the exit is interned in the context; a new exit queues a `Return` task for every
  caller registered so far.
- **Fault**: an evaluation error in story code (index out of range, overflow, budget) is
  reported at the scene expression that was running, with a related note at the failing site, and
  the path ends.
- **Abort**: the path cannot be followed (see below): `W5007`, and the path ends.

A state that already exists as a node of the context returns its `Target` instead of a new node:
this is the deduplication that makes the search finite. Calls with the same callee, arguments and
story variables share one context, so a scene called from many places in the same state is
explored once.

### Segments

`segment::run` builds a fresh `Machine` in story mode for every segment. It loads the kept locals
of the resumed suspension (thawing their values; a reference parameter gets a fresh cell and a
reference to it), writes the suspension's result into its `dest`, writes the callee's `outs` back
through the places the `&mut` arguments borrowed, and runs `Machine::run_scene` until the scene
reaches a suspension, `Return` or `Unreachable`. Calls of instant procedures and `const fn` run
inside the segment. Story variables are thawed from the state the first time the segment touches
them, and only those are frozen back (`Snapshot::globals`) and set into a copy of the state's
`Globals`.

At the stop, the kept locals are frozen. A value that cannot be frozen (a capturing closure or a
reference held across a step) turns the stop into an abort. `SEGMENT_FUEL` (1 000 000 blocks per
segment) guards against bugs only: termination guarantees that every loop passes a step, and the
machine's step budget fires first (`E4003`, reported as a fault in story code).

### Caps and completeness

| Limit | Value | Past it |
| ----- | ----: | ------- |
| `CompileOptions::state_cap` | 200 000 nodes | `W5006` at the first entry scene's name; exploration stops |
| Entry arguments | `TABLE_DOMAIN_CAP` (65 536) combinations | `W5006` at the entry scene; that scene is skipped |
| Draw values | 65 536 | `W5006` at the draw |
| Unfollowable path | — | `W5007` where it stopped |

The state count is the number of nodes over all contexts, checked whenever a task is taken.
Every one of these sets `Explored::complete = false`, which suppresses the claims that need every
state: unreachable scenes and steps. Stuck choices, faults, render errors and screen diagnostics
found on the states that were reached are still reported.

Cost: each node runs at most one segment per outcome, and each call node one segment per callee
exit. The state space is the product of reachable program points, kept-local values and story
variable values, so it grows with the size of the story variables' types; `state_cap` bounds the
work.

### Reachability

With a complete exploration, a game scene with no context is `W5008`, and a `Step` terminator of
a reached scene whose block is no node's block in any context is `W5009`.

`W5009` and the stuck-choice `E5003` carry a related note at the step naming its
[step ID](./mir.md#step-ids): ``step `crate::Main#2` `` (the item path and the `Step`
terminator's id; for a generic scene the item path without type arguments). They are built by
`SceneData::step_diagnostic` (`analysis/story/mod.rs`). Example from
`tests/analysis/005__story`:

```text
analysis/005__story/main.kix:30:9: warning[W5009]: this step is never reached
  note: analysis/005__story/main.kix:30:9: step `crate::Main#2`
```

## Screens

Renders never influence the story, so screens are computed afterwards over the explored graph
(`screens.rs`). The story starts on `Screen::empty()`.

- An **instance** is a context entered with one screen: `(CtxId, ScreenId)`, where screens are
  interned `LogicalScreen`s. Each instance records, per node, the screens that reach it with their
  *producer* (the step whose render last changed the screen), and per exit the screens it leaves
  with.
- Propagation is a worklist: a step node applies its render to each arriving screen and passes the
  result to every successor; a draw passes the screen unchanged; a call node enters the callee's
  instance for that screen and continues with the screens of its exits.
- `apply` memoizes `(render, screen) → screen`. A render runs in a fresh machine on a `Screen`
  whose logical part is the arriving screen; its faults are reported at the render site with their
  `E4xxx` code.

**Bounded screens (`E5005`).** When the union of screens reaching one step exceeds `SCREEN_CAP`
(64), the producer of the last new screen (or the step itself) is reported, *disabled* (treated as
leaving the screen unchanged), and propagation starts over. This repeats until no step overflows,
so the rest of the program is still checked. The rule is sound: an accepted program's sets are
exact. It rejects renders that are bounded only above the cap. The cap is tentative
([Open Questions](../process/open-questions.md)).

**Consistency (`W5004`).** The screen a step shows must be a function of the story state: the
step, the kept locals of every frame on the call stack, the story variables, and the call sites.
Contexts and nodes already contain everything but the call stack, so the check walks the call
tree from the roots, grouping the instances of a context entered from the same call node with the
same set of screens. For each step node, the screens arriving over the group are run through the
render; more than one distinct result is an inconsistency. Reports are deduplicated per *merge*:
a step whose only differing producer is an already reported step is skipped, steps with the same
set of producers are reported once, at the first in source order, with up to two producer render
sites as related notes. This is the presentation-ambiguity lint of the proposal.

## Keyframe placement (`W5011`)

Opt-in through `[build] keyframe-check` / `CompileOptions::keyframe_check`. Loading a save on a
changed script or jumping to a step without a snapshot replays renders from the last keyframe,
so the steps a player lands on directly should have keyframe renders: every reached `choice!`,
and the first steps of every root context (found through draws and into called scenes and back
out, memoized per `(context, target)`). A step's render is a keyframe when stage 4 marked it so
(`RenderInfo::keyframe`); a step without a render is not a keyframe.

## Unused story variables (`W5010`)

`influence.rs` builds a flow-insensitive dependence graph over the MIR of every `pushdown fn` of
the program (game and std, closures included). Its nodes are the story variables, the locals of
each body, and the parameters and return value of each `pushdown fn`; an edge `A → B` means the
value of `A` may depend on `B`. A `&mut` reference binds both ways: a write through a reference
local or parameter flows back into the places it was bound to.

Sinks are the values the story observably depends on: branch conditions (`if`, `while`, patterns,
guards, `for` bounds, `?`), step arguments, arguments of `pushdown` builtins such as
`Meta::write`, arguments of calls through pointers or closures, and the return value of an entry
scene (its ending). The `SwitchBool`s of `&&`/`||` only decide their result local and are not
sinks. Since every condition is a sink, control dependence adds nothing and is not recorded.

Each item's `InfluenceSummary` (edges, sinks, story variables read and written) is a salsa query,
so editing one body recomputes one summary; `check` unions them and marks everything a sink
reaches. A game story variable that is not influential is reported in one of three forms: read
but never influencing, only assigned, or never used.

## System analysis (`E6xxx`)

The game system needs no decidability argument: its checks are about the mode graph, collected
over all branches of the handlers, which are finite in number and written in the source.

- **Modes.** Every non-generic game `fn` with signature `fn() -> RootPage`, `fn() -> Overlay`,
  `fn() -> Dialog` or `fn(Narrative) -> GameOverlay`, plus the game mode (the `System::game` view).
  The root is `System::start`. Both are read from the `System` record `fn main` returns.
- **Code of a mode.** Its component's body and, transitively, the bodies of every function,
  constant and static it calls or uses as a value (`system_summary`, per item, from MIR; a
  `pushdown fn` never runs system code and is not followed; a trait method expands to every impl of
  it). A function named directly as the target of a navigation operation is an edge, not code. The
  exit handlers passed to `nav::start` are code of the game mode.
- **Edges.** `nav::call` (overlay), `nav::dialog`, `nav::goto` (root page), `nav::start` and
  `nav::load` (game), `Narrative::call` (game overlay); an operation whose target is not a named
  mode reaches every mode of its kind. Bindings count: `Action::Open(x)` opens its overlay from
  every mode, `Action::OpenInGame(x)` a game overlay from the game and game overlays, and
  `Action::Back` is a way back from every pushed mode.

| Code | Check |
| ---- | ----- |
| `W6001` | Breadth-first reachability from the start page over the operations of reached modes and the bindings: an unreached mode, or a game that no reachable mode starts or loads |
| `E6002` | `nav::back()` in the code of a root page or the game mode (a base of the stack) |
| `E6003` | A reachable overlay, game overlay or dialog whose code never runs `nav::back`, `nav::goto`, `nav::start`, `nav::load` or `nav::quit`, unless a `Back` binding exists |
| `W6004` | A cycle of pushes (`nav::call`, `nav::dialog`, `Narrative::call` with named targets) among pushed modes; bindings never count as pushes |

Handlers can still run forever; nothing in the proposal depends on them terminating, so the
compiler does not check it. Exhaustive endings are ordinary `match` exhaustiveness in the exit
handler.

## The persisted story graph

`analysis::story_graph(db, program)` turns the `Explored` graph of `story_analysis` into a
`kagari_debug::StoryGraph` (`story/graph.rs`), the form `.kagari-cache` files store (layout in
[`.kagari-cache` Files](../formats/cache.md)). It is computed on demand: the language server never
pays for it. Every index (values, frames, story-variable sets, contexts, nodes, exits) is the
`Explored` index, so the graph is the analysis's own state space, renamed for readers that do not
link the compiler:

| `Explored` | `StoryGraph` |
| ---------- | ------------ |
| `ConstValue` | `Value`, named the way saves name values: ADTs by type path (`paths::item_path`) and variant name, assets by bytecode path (`assets::bytecode_path`), handles by static path; functions by instance path (`paths::generic_args_suffix`), closures as `{owner}::{closure#N}` (`paths::closure_number`), screens opaque |
| `Context::fn_ref` | `Context::path`: the instance path the bytecode gives the scene, so `{path}#{ordinal}` is the step ID of the bytecode and the runtime |
| `NodeKind::Step` | `Say { step, render, next }` or `Choice { step, render, arms }`; `step` is the ordinal of the block's `Terminator::Step`; a shown arm is named by its **arm ID**, as `saves::trace::arm_id` in the engine names it: `{type path}::{variant}` for an ADT value, else `#{position in the arm list}` |
| `NodeKind::Call` | `Call { callee, exits: [(exit, target)] }` |
| `NodeKind::Rng` | `Draw { draws: [(value, target)] }` |
| the kept locals of a block (`scene_kept`) | `Scene::points`: per block some node is at, the binding names of its kept locals in frame order (`""` for a temporary) |
| `Explored::vars` | `StoryGraph::vars`: story-variable paths |
| `Explored::complete` and the `W5006`/`W5007` diagnostics | `complete`, `incomplete: [{code, message}]` |

A render that the exploration never interned is appended to `values`.

## Reusing the analysis

The exploration is the expensive part of a build. The CLI keeps its result in
`<out-dir>/analysis.kagari-cache` and offers it to the next run, so an unchanged program is not
analyzed again (`cache.rs`; the CLI side is in [Projects and the CLI](../tooling/cli.md)). The
language server never offers one: salsa already memoizes the analysis there.

- `source_fingerprint(db, program)` (salsa) is a SHA-256 over every input stage 5 reads that
  stages 1–4 do not already guard: a salt (`FINGERPRINT_SALT`), the toolchain version, every
  `CompileOptions` field, the asset root and the system-font directories, and the path, crate
  and text of every file of `def_maps` (std included), each length-prefixed. Asset *existence*
  is not part of it: a missing asset is a stage 1–4 error, and then stage 5 does not run.
- `Program::analysis_cache` is a salsa input field, `None` unless a tool calls
  `offer_analysis_cache(db, program, &cache)`, which converts the cached diagnostic records back
  to `Diagnostic`s (an unknown code or an inverted range refuses the whole offer).
- `analysis_diagnostics` (salsa) returns the offered diagnostics when the offered fingerprint
  equals `source_fingerprint`, without running `analysis`; otherwise it resolves the diagnostics
  of `analysis`. Reading the field only when it is set keeps the language server from computing
  the fingerprint.
- After `check`, `analysis_cache_use` reports `NotRun` (errors in stages 1–4), `Reused`, or
  `Fresh(cache)`: the fingerprint, the resolved stage-5 diagnostics and `story_graph`, for the tool
  to write.

The test `tests/analysis_cache.rs` observes the reuse through salsa events: with a matching offer,
`story_analysis` never executes.

## Compiler options

`kagari_lang::CompileOptions` (`input.rs`) is part of the salsa `Program` input; build it with
`RootDatabase::program_with(root, options)` or `program_with_assets(root, asset_root, font_dirs,
options)`. `kagari-project` fills it from `kagari.toml`.

| Field | Manifest key | Default | Used by |
| ----- | ------------ | ------: | ------- |
| `state_cap` | `[build] state-cap` | 200 000 (`DEFAULT_STATE_CAP`) | Exploration (`W5006` past it) |
| `keyframe_check` | `[build] keyframe-check` | `false` | Keyframe placement check (`W5011`) |
| `recursion_depth` | `[build] recursion-depth` | 2048 (`DEFAULT_RECURSION_DEPTH`) | Emission: written into the bytecode as `max_call_depth` |
| `system_fonts` | `[assets] system-fonts` | `true` | Name resolution of `system_font("…")` (`E1031` when off) |

Changing an option invalidates the queries that read it, not the whole database, and changes the
source fingerprint.

## Invariants

- Stage 5 never runs on a program with errors from stages 1–4, and the story exploration never
  runs on a program with termination errors.
- A story state is exactly (block, kept locals, story variables) within a context (scene,
  arguments, story variables at entry). Adding anything else to it breaks deduplication; leaving
  out something live makes the analysis unsound.
- "Unreachable" is claimed only when `Explored::complete` holds.
- The analysis is deterministic: LIFO worklist, interning in discovery order, and diagnostics
  sorted by `pipeline::check`. The same program gives the same diagnostics.
- Every loop in the analysis is bounded: termination by CFG size, exploration by `state_cap`,
  segments by the machine budget, screens by `SCREEN_CAP`.
- The analyses read MIR and run the same executor as stage 4, so the story they check is the
  story codegen emits.
- An offered analysis is reused only for the exact inputs it was computed from (equal
  `source_fingerprint`); the cached and the fresh diagnostics of a program are the same list.
- The persisted graph names everything as the bytecode and the engine do (paths, step IDs, arm
  IDs); it holds no `ItemId`, `ExprId` or text range.

## When you change this

- **A new story check.** Decide where it belongs: per state during exploration (`settle` in
  `explore/mod.rs`), over the finished `Explored` graph (like `screens.rs`, `keyframes.rs` or
  `report_reachability`), or per item over MIR (like `influence.rs`). Claims about *all* states
  must respect `Explored::complete`. Add a `DiagnosticCode` (`E5xxx`/`W5xxx`), a fixture under
  `tests/game_source/analysis/`, and one entry per language in the Reference's diagnostics
  appendix.
- **A new suspension or call form.** Update `liveness.rs` (what the state keeps), `segment.rs`
  (resume and stop), `termination.rs`, `explore/mod.rs` (`settle`), `screens.rs`,
  `keyframes.rs` and `graph.rs` (node kinds), and `kagari_debug::NodeKind` with its schema.
- **A change to what a state contains** changes the number of states of every story: check the
  analysis snapshots and the `state_cap` default.
- **Anything that changes the analysis's results for the same sources** (a new check, a changed
  message, a different exploration): bump `FINGERPRINT_SALT` in `cache.rs`, or caches written by
  the same toolchain version are reused with the old results. A new input the analysis reads goes
  into `source_fingerprint`.
- **A change to the stored graph** (`StoryGraph` fields or their meaning): bump
  `kagari_debug::SCHEMA` and update [`.kagari-cache` Files](../formats/cache.md).
- **A system check.** `analysis/system/` plus fixtures `analysis/001__system`, `002__system_bindings`
  and `ui/*`; the Reference's game-system chapter (en and ja).
- **An option.** `CompileOptions`, `kagari-project`'s manifest (and its tests), the Reference's
  project chapter (en and ja), the table above, and `source_fingerprint`.
- Run `cargo test -p kagari-lang` and review `tests/snapshots/game_source__*.snap`.
