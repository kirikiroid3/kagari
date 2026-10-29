---
name: kagari-std
description: Load when adding, removing, renaming or changing anything in core/kagari-std/kis (std declarations, #[builtin] items, prelude), its compiler special cases (StdItems, eval/std_items.rs BUILTINS, eval/machine/builtins.rs), or its engine implementation (import_table! imports, STD_ADTS in runtime_libs/kagari-engine).
---

# Kagari std

## Scope

| Path | What |
| ---- | ---- |
| `core/kagari-std/kis/std/**/*.kis` | the library, one file per module; `prelude.kis` |
| `core/kagari-std/src/lib.rs` | `file(path)`: `include_str!` of every `.kis` file (add new files here) |
| `core/kagari-lang/src/signatures/{mod,collect}.rs` | `StdItems`: std items the checker knows by path |
| `core/kagari-lang/src/eval/std_items.rs` | `Builtin`, `BUILTINS` `(file, owner, fn)`, `StdTable` |
| `core/kagari-lang/src/eval/machine/builtins.rs` | compile-time implementation of const builtins |
| `core/kagari-lang/src/emit/codegen/call.rs` | intrinsic vs CALLIMP lowering |
| `runtime_libs/kagari-engine/src/*/` | `import_table!` per module (screen, audio, story/narrative, system, system/nav, saves, ui) |
| `runtime_libs/kagari-engine/src/host/mod.rs` | `StdImport`, `Bindings` (boot fails on an unknown import path) |
| `runtime_libs/kagari-engine/src/values/table.rs` | `STD_ADTS`: std types the engine reads/builds, field order |
| `docs/site/src/content/docs/{en,ja}/std/` | STD Reference, one page per module |

## Read first

- `docs/dev/src/std/builtins.md` — identification by name, lowering, engine imports, the full
  add/change checklist.
- `docs/dev/src/std/contract.md` — design rules and every checker special case.
- `docs/site/src/content/docs/en/std/index.md` and an existing module page — the STD page template.

## Invariants

- `#[builtin]` only in std; builtin fns have no body; builtin structs have no fields; every std
  trait is `#[builtin]`.
- Design rules (contract.md): render/geometry/animation API is `const fn`; system view, settings,
  meta reads, nav, saves, UI are `fn`; only `push_step`, `rng_choose`, `Meta::write` are
  `pushdown fn`; visible state changes go through `Prop`/layers + `apply_animation`; explicit
  durations; pixels `i32`/`u32`, fractions `Decimal`, times `Duration`.
- Names are ABI: import paths (`std::screen::Entity::position`), `BUILTINS` rows, `StdItems`
  paths, `STD_ADTS` paths and **field order**, and save data (ADTs stored by type path +
  variant name). Renames/reorders need all of them updated.
- Every `const fn` builtin has an evaluator implementation (`every_const_builtin_is_known`).
- Every builtin that can be emitted as CALLIMP (all `fn`/`pushdown fn` builtins except
  `push_step`/`rng_choose`, and `const fn` builtins without an intrinsic) has an engine import
  with the same semantics as the evaluator. No test enumerates this: add one per new item.

## Workflow: add or change a std item

1. Declare in the module's `.kis` (doc comment, kind, `#[builtin]` only if Kagari cannot
   implement it). New file: `pub mod` in the parent + arm in `core/kagari-std/src/lib.rs`.
   Prelude export in `prelude.kis` if studios use it everywhere.
2. Checker special case? Field in `StdItems` + `signatures/collect.rs::std_items`, rule in
   `infer/` or `typeck/`, new diagnostic code (`core/kagari-lang/src/diagnostic.rs`), tests.
3. Const builtin? `Builtin` variant + `BUILTINS` row (`eval/std_items.rs`), implementation in
   `eval/machine/builtins.rs`, test in `eval/tests.rs`; `StdTable` entry if the evaluator needs
   the type.
4. Emission: default CALLIMP needs nothing. Intrinsic only for engine-independent hot ops; a
   new opcode → load the `kagari-bytecode` skill.
5. Engine: path in the module's `import_table!`, arm in its `call` (decode args with
   `values/decode.rs`, errors via `Trap::host` naming the path, `ui.track`/`invalidate` for
   reactive state). New std ADT the engine touches → `STD_ADTS` row. New std module →
   `StdImport` variant in `host/mod.rs`. Test with `test_support::bound_engine`.
6. Story semantics: new `pushdown fn` builtin or step type → story analysis
   (`analysis/story/explore`) and influence analysis (`analysis/influence.rs`).
7. New asset kind → `AssetKind` (bytecode), `kagari-compat` profiles, STD appendix
   `profiles.md`.
8. Docs: STD Reference module page in **en and ja** (`docs/site/src/content/docs/{en,ja}/std/<module>.md`, same
   headings with `{#id}` in ja, byte-identical code blocks, Stability line, every public item in
   declaration order); `appendix/rules.md` (en + ja) for compiler rules; Reference
   `diagnostics/*.md` (en + ja) for new codes; `docs/dev/src/std/contract.md` for special cases.

## Tests

```sh
cargo test -p kagari-lang eval::           # const builtins, every_const_builtin_is_known
cargo test -p kagari-lang                  # checker rules, diagnostics
cargo test -p kagari-engine                # imports, std_table_matches_the_compiled_std
cargo test -p kagari-cli                   # build of the template project
```

Then `target/debug/kagari check` / `build` in `example/test_game1` and run it with
`target/debug/kagari-desktop-runtime`.

## Docs to update

- `docs/site/src/content/docs/en/std/*.md` **and** `docs/site/src/content/docs/ja/std/*.md` (mirrored).
- `docs/dev/src/std/builtins.md`, `docs/dev/src/std/contract.md`.
- Reference (en + ja) when a language-level rule or diagnostic changes.
