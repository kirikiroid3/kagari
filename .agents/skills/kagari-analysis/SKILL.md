---
name: kagari-analysis
description: Load when changing THIR, MIR, const evaluation, story/system analysis or bytecode emission in core/kagari-lang (src/thir, src/mir, src/eval, src/analysis, src/emit) or their tests (tests/mir.rs, tests/bytecode.rs, tests/game_source/analysis).
---

# Kagari middle and back end (THIR → MIR → eval / analysis / emit)

## Scope

- `core/kagari-lang/src/thir/` — typed tree per item; promotion of story constants.
- `core/kagari-lang/src/mir/` — CFG per body (`item_mir`), liveness, validation, pretty dump.
- `core/kagari-lang/src/eval/` — stage 4: `const_eval`, per-item queries, MIR executor
  (`eval/machine`), tabulation, renders/keyframes, compile-time `Screen` model.
- `core/kagari-lang/src/analysis/` — stage 5: termination, story exploration, screens,
  keyframes, influence (`W5010`), system mode graph (`E6xxx`).
- `core/kagari-lang/src/emit/` — stage 6: module tables, instances, codegen, regalloc.
- Not here: parsing, resolution, inference, stage-3 checks (`kagari-syntax`, `hir`, `resolve`,
  `infer`, `typeck`), the bytecode format itself (`core/kagari-bytecode`), the VM/engine.

## Read first

Develop Guide (`docs/dev/src/`):

- `compiler/mir.md` — THIR, MIR shape, suspensions, decision trees, builder, validation.
- `compiler/eval.md` — machine, constants, promotion, tabulation, renders, `E4xxx`, limits.
- `compiler/analysis.md` — termination, kept locals, exploration state, screens, influence, system.
- `compiler/emit.md` — instances, regalloc, constants, builtins → intrinsics/imports, terminators.
- `compiler/pipeline.md` and `compiler/diagnostics.md` for salsa queries and `Diag`/`Anchor`.
- `formats/kagari.md` when touching emitted instructions or tables.

## Invariants

- **One semantics.** MIR is the only semantic model: the const evaluator, the story exploration and
  codegen all consume it. Never special-case a construct in one consumer only; a MIR change must be
  handled in `eval/machine/exec.rs`, `emit/codegen`, `mir/liveness.rs`, `mir/validate.rs`,
  `mir/pretty.rs`, `analysis/influence.rs`, `analysis/system/summary.rs`, `typeck/kinds/instant.rs`.
- **Suspensions** (`Step`, `Rng`, `SceneCall`) are terminators, legal only in `BodyKind::Scene`.
- **Determinism.** No floats, no host I/O, no iteration over hash maps whose insertion order is not
  deterministic. Same program ⇒ same diagnostics and same bytes (the engine's script hash is the
  SHA-256 of the bytes).
- **Decidability bounds.** Every loop is bounded: eval step budget (1 000 000) and call depth
  (10 000) per `Machine`; `TABLE_DOMAIN_CAP` (65 536); `state_cap` (default 200 000);
  `SCREEN_CAP` (64). Exceeding a bound is a diagnostic (`E4003`, `E4004`, `W5006`, `E5005`), never
  a hang.
- **Soundness of claims.** "Unreachable" (`W5008`/`W5009`) only when `Explored::complete`. A story
  state is exactly (block, kept locals, story variables) within a context (scene, args, globals).
- **Diagnostics.** Raise errors as `Fault`/`Diag` with an `Anchor` (never text ranges); report only
  for game-crate items; stage 5 runs only without errors from stages 1–4.
- Crate lints deny `unwrap`, `expect`, `panic` and `arithmetic_side_effects`: use `get`,
  `checked_*`/`saturating_*`, and `stacker::maybe_grow` in recursive walkers.

## Workflows

### Add an analysis check (E5xxx/W5xxx)

1. Pick the level: per state in `explore/mod.rs::settle`; over the finished `Explored` graph (like
   `screens.rs`, `keyframes.rs`, `report_reachability`); or per item over MIR (like
   `influence.rs`, cache it with `#[salsa::tracked]`).
2. Add the `DiagnosticCode` in `src/diagnostic.rs`.
3. Add a fixture `tests/game_source/analysis/NNN__name/main.kix` (copy a sibling for the required
   `System`/settings boilerplate).
4. Add exactly one `## CODE name` entry to `docs/reference/{en,ja}/src/diagnostics/*.md`
   (`tests/diagnostics_book.rs` fails otherwise).
5. Document the algorithm in `docs/dev/src/compiler/analysis.md`.

### Change lowering (THIR/MIR)

1. If inference records a new adjustment, make it an explicit THIR node in `thir/build/`.
2. Lower it in the matching MIR flavour (`place`/`operand`/`rvalue`/`into`/`control`/`call`/
   `matches`); keep `src` set via `with_src`.
3. Update every MIR consumer listed above; extend `validate.rs` if a new structural rule appears.
4. Review `tests/snapshots/mir__*.snap` and `bytecode__*.snap`.

### Change emitted code

1. Codegen lives in `emit/codegen/{rvalue,place,call,term}.rs`; tables in `emit/mod.rs`.
2. A new instruction needs `kagari-bytecode` (encode, decode, verify, disassemble), the VM, and
   possibly a format version bump; update `docs/dev/src/formats/kagari.md` and `runtime/vm.md`.
3. Any byte change alters every game's script hash: say so in the change description.
4. Review `tests/snapshots/bytecode__*.snap`; `tests/bytecode.rs` also checks the decode round trip.

### Add a const builtin

`eval/std_items.rs` (`Builtin`, `BUILTINS`, `BUILTINS_LEN` test) → `eval/machine/builtins.rs` →
codegen intrinsic or import in `emit/codegen/call.rs` → engine import → STD Reference (en, ja).

## Tests and commands

```sh
cargo test -p kagari-lang                       # unit tests + snapshot tests
cargo test -p kagari-lang --test mir            # MIR dumps of tests/bytecode/*
cargo test -p kagari-lang --test bytecode       # disassembly of tests/bytecode/*
cargo test -p kagari-lang --test game_source    # diagnostics/resolutions/renders of tests/game_source/*
cargo test -p kagari-lang --test diagnostics_book
cargo insta review                              # accept snapshot changes after reading them
cargo clippy -p kagari-lang --all-targets
```

Unit tests: `eval/tests.rs`, `analysis/story/tests.rs`, `emit/tests.rs`. Runtime agreement:
`runtime_libs/kagari-vm/tests` and `kagari-engine` tests compile Kagari through `kagari_lang::build`.

## Docs to update

- Develop Guide: the matching `docs/dev/src/compiler/*.md` chapter; `formats/kagari.md` for bytecode;
  `process/open-questions.md` when a tentative limit changes.
- Reference (en + ja): diagnostics appendix for any code change; user-visible rules (termination,
  consistency, keyframes, constant expressions, compiler options).
- STD Reference (en + ja) for builtin or std-rule changes (`appendix/rules.md`).
- Module doc comments in `src/eval`, `src/analysis`, `src/mir`, `src/emit` that cite doc paths.
