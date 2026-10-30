# Pipeline and Queries

The compiler front end (`kagari-lang`) is a [salsa](https://github.com/salsa-rs/salsa) database.
Every stage is a set of memoized queries over a few inputs, and `check` is the query that pulls
them all. Bytecode emission is a plain function, `build`, on top of `check`. This chapter describes
the stages, the database, how the tools drive it, and the two rules that shape the pipeline:
checks run as independent queries before lowering, and the whole-program analysis runs only on
programs without errors.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/lib.rs` | Crate docs, public API (`check`, `build`, `parse`, `RootDatabase`, `Diagnostic`, …) |
| `core/kagari-lang/src/db.rs` | `Db` trait, `RootDatabase`, `FileSystem` (`OsFileSystem`, `MemoryFileSystem`) |
| `core/kagari-lang/src/input.rs` | Salsa inputs `SourceFile`, `AssetFile`, `Program`; `FilePath`; `CompileOptions`; `file_exists` |
| `core/kagari-lang/src/pipeline.rs` | `check`: every diagnostic of stages 1–5 (`front_diagnostics` for 1–4), and the analysis gate (`analysis_runs`) |
| `core/kagari-lang/src/cache.rs` | `source_fingerprint`, `analysis_diagnostics`, `offer_analysis_cache`, `analysis_cache_use`: reusing stage 5 across runs |
| `core/kagari-lang/src/emit/mod.rs` | `build`: `check`, then stage 6 (see [Emitting Bytecode](./emit.md)) |
| `core/kagari-lang/src/parse_query.rs` | `parse`, the bridge to `kagari-syntax` |
| `core/kagari-lang/src/item_tree.rs`, `hir/` | Stage 1 queries after parsing (see [Name Resolution and HIR](./resolve.md)) |
| `core/kagari-lang/src/def_map.rs`, `resolve.rs` | Stage 2 |
| `core/kagari-lang/src/signatures/`, `infer/`, `typeck/` | Stage 3 (see [Type Inference and Checks](./typeck.md)) |
| `core/kagari-lang/src/eval/` | Stage 4 (see [Constant Evaluation](./eval.md)) |
| `core/kagari-lang/src/analysis/` | Stage 5 (see [Story and System Analysis](./analysis.md)) |
| `core/kagari-lang/src/thir/`, `mir/` | Lowering (see [THIR and MIR](./mir.md)) |
| `core/kagari-lang/src/intern.rs`, `ty.rs` | The hash-consing interner and the interned semantic types |
| `core/kagari-lang/tests/incremental.rs` | The incrementality test (see [below](#anchors-instead-of-ranges)) |
| `tooling/kagari-project/src/project.rs` | `Project::program`: manifest → `Program` input |
| `tooling/kagari-cli/src/command/check.rs`, `build/mod.rs` | `kagari check`, `kagari build` |
| `tooling/kagari-lsp/src/diagnostics.rs`, `server/notifications.rs` | The language server's use of the database |

## The six stages

<!-- diagram: pipeline-stages (mermaid-ish)
flowchart LR
  inputs["salsa inputs: Program, SourceFile, AssetFile"] ==> s1["1 Parse, HIR: parse, item_tree, signature/body"]
  inputs ==> s2
  s1 ==> s2["2 Resolve: def_maps, signature_resolution, body_resolution"]
  s2 ==> s3["3 Check: program_signatures, infer, pattern_checks, kind_checks, instant_recursion"]
  s3 ==> s4["4 Const eval: const_eval, const_value, static_value"]
  s4 ==> gate1{"gate: no errors from 1-4"}
  gate1 ==> s5["5 Analysis: analysis (story, influence, system)"]
  s5 ==> gate2{"gate: build emits after a clean check"}
  gate2 ==> s6["6 Emit: emit::build, verify, encode"]
  s1 -.-> lower["lowering: thir, item_mir (no diagnostics)"]
  lower -.-> s3
  lower -.-> s4
  lower -.-> s5
  lower -.-> s6
-->
{{#include ../diagrams/pipeline-stages.svg}}

The stage numbers are the ones the Reference uses and the first digit of the
[diagnostic codes](./diagnostics.md) (stage 3 owns `E2xxx` and `E3xxx`, stage 5 `E5xxx` and
`E6xxx`).

| Stage | What happens | Queries | Codes |
| ----- | ------------ | ------- | ----- |
| 1 · Parse and lower | Each file is lexed, parsed and validated (`kagari-syntax`). Its items are collected into an item tree, with the item-level checks. Signatures and bodies are lowered to HIR, and **forms are desugared** here, so later stages see ordinary calls and struct expressions. | `parse`, `item_tree_raw` → `item_tree`/`item_source_map`/`item_tree_diagnostics`, `file_hir_raw` → `signature`/`body`/`*_source_map` | `E0001`, `E1005`–`E1013` |
| 2 · Resolve | The module trees of the game and std crates are built by following `mod` declarations from the root file and `std`'s root; imports reach a fixpoint; impls are indexed. Then every path, local, label and asset path of each signature and body is resolved. | `def_maps`, `signature_resolution`, `body_resolution` | `E1001`–`E1004`, `E1014`–`E1032` |
| 3 · Check | Item signatures become semantic types once for the program. Each body is inferred. Exhaustiveness, refutability, the kind and story rules and the instant-recursion check read the inference results. | `program_signatures`, `infer`, `pattern_checks`, `kind_checks` (with `item_const_exprs`), `instant_edges`, `instant_recursion` | `E2xxx`, `E3xxx` |
| 4 · Const evaluation | Constants and statics are evaluated by the MIR interpreter; story calls of `const fn` are tabulated; renders are evaluated. | `const_eval`, `const_value`, `static_value`, `promoted_value`, `std_table` | `E4xxx` |
| 5 · Analysis | Termination, the story's state exploration, screens, liveness and the system checks. | `analysis` (plus internal per-item queries such as `scene_kept`, `influence_summary`, `system_summary`) | `E5xxx`, `E6xxx` |
| 6 · Emit | MIR to bytecode, verification, encoding. Not a query: `build` calls `check` and emits only when it has no errors. | `emit::build` | `E7xxx` |

Module *discovery* (following `mod foo;` to files) is part of `def_maps` in stage 2, because it
needs the item trees of the files it has already loaded; the old user documentation counted it as
part of "parse". Stages 1–4 are cheap per item; stage 5 is where the expensive whole-program
work happens.

## The query database

### Inputs

| Input | Fields | Created by |
| ----- | ------ | ---------- |
| `SourceFile` | `path: FilePath`, `text: Option<Arc<str>>` (`None`: missing or unreadable) | `Db::source_file(path)`, on first use |
| `AssetFile` | `path: FilePath`, `exists: bool` | `Db::asset_file(path)`, on first use |
| `Program` | `root: SourceFile`, `asset_root: FilePath`, `font_dirs: Arc<[FilePath]>`, `options: CompileOptions`, `analysis_cache: Option<Arc<CachedAnalysis>>` (`None` at creation) | `RootDatabase::program`, `program_with`, `program_with_assets` |

- `FilePath` is either `Std(name)` (a file embedded in `kagari-std`, `/`-separated relative to the
  std directory, read with `kagari_std::file`) or `Disk(path)` (absolute, lexically normalized).
  Std files never touch the file system; a std path never names an asset.
- `RootDatabase` keeps a `FilePath → SourceFile` and a `FilePath → AssetFile` table behind
  `Arc<Mutex<…>>`, so one path always maps to one input. The first request for a path reads the
  file (or checks existence) through the `FileSystem` the database was created with:
  `OsFileSystem` for the tools, `MemoryFileSystem` for tests.
- After creation an input changes only through a setter: `set_file_text(path, text)`,
  `set_asset_exists(path, exists)` and `cache::offer_analysis_cache(db, program, cache)`. The
  database never re-reads the disk by itself.
- `file_exists(file)` is a tracked query that reads only whether `text` is `Some`. `mod` lookup uses
  it, so editing a module's text does not re-run module discovery of its parent.
- `program_with` uses the root file's directory as the asset root and no font directories;
  `kagari_project::Project::program` calls `program_with_assets` with the manifest's asset
  directory, the system font directories (`kagari_project::system_font_dirs`, unless
  `[assets] system-fonts = false`) and the manifest's options.

`CompileOptions` (all fields `Copy`, part of the `Program` input):

| Field | Default | Manifest key | Read by |
| ----- | ------: | ------------ | ------- |
| `state_cap` | `DEFAULT_STATE_CAP` = 200 000 | `[build] state-cap` | Story exploration (`W5006` past it) |
| `recursion_depth` | `DEFAULT_RECURSION_DEPTH` = 2048 | `[build] recursion-depth` | Emit: written into the bytecode for the VM's call-depth limit |
| `keyframe_check` | `false` | `[build] keyframe-check` | Analysis: the keyframe-placement check (`W5011`) |
| `system_fonts` | `true` | `[assets] system-fonts` | Resolve: `system_font("…")` is `E1031` when off |

### Tracked queries

All queries take `&dyn Db` first. Item-level queries are keyed by `ItemId { file: SourceFile,
index: ItemIndex }`, a plain `Copy` struct; salsa interns multi-argument keys itself (their debug
names carry `::interned_arguments`).

| Query | Key | Result | Notes |
| ----- | --- | ------ | ----- |
| `parse` | `SourceFile` | `Parse` | `.kis` parses as `SourceKind::Kis`, every other extension as `Kix`; missing text parses as empty |
| `item_tree_raw` | `SourceFile` | `ItemTreeRaw` | `no_eq`; read only by the three projections below |
| `item_tree`, `item_source_map`, `item_tree_diagnostics` | `SourceFile` | projections | The tree holds no text ranges; the source map does |
| `file_hir_raw` | `SourceFile` | `FileHirRaw` | `no_eq`; HIR and source maps of every item of the file |
| `signature`, `body`, `signature_source_map`, `body_source_map` | `(SourceFile, ItemIndex)` | projections | Use `hir::signature_of(db, item)` / `body_of` |
| `def_maps` | `Program` | `DefMaps` | Both crates, every loaded file, scopes, imports, impls, prelude |
| `signature_resolution`, `body_resolution` | `(Program, ItemId)` | resolutions + `Diag`s | Empty for files outside the program |
| `program_signatures` | `Program` | `ProgramSignatures` | All item signatures as `Ty`, plus `StdItems` |
| `infer` | `(Program, ItemId)` | `Option<InferOutput>` | `Some` for `fn`/`const`/`static` with a body in the program |
| `item_const_exprs` | `(Program, ItemId)` | `Option<ConstExprs>` | Which expressions are constant; read by kind checks, THIR and eval |
| `pattern_checks`, `kind_checks` | `(Program, ItemId)` | `Diag`s | Stage 3 checks |
| `instant_edges` | `(Program, ItemId)` | `InstantEdges` | Reads the item's MIR |
| `instant_recursion` | `Program` | `Diag`s | Program-wide cycle check |
| `thir`, `item_mir` | `(Program, ItemId)` | `Option<…>` | Lowering; no diagnostics |
| `std_table`, `const_value`, `static_value`, `promoted_value`, `const_eval` | `Program` (+ item) | values, `Diag`s | `const_value`/`static_value` have `cycle_result` (`E4006`) |
| `story_analysis` | `Program` | `StoryAnalysis` | `no_eq`; stage 5's story part, keeps the explored graph |
| `analysis` | `Program` | `Analysis` | Stage 5 |
| `front_diagnostics` | `Program` | `Vec<Diagnostic>` | Stages 1–4, resolved, unsorted |
| `source_fingerprint` | `Program` | `String` | SHA-256 of every input stage 5 reads; computed only when an analysis cache is offered |
| `analysis_diagnostics` | `Program` | `Vec<Diagnostic>` | Stage 5, resolved: the offered cache's when its fingerprint matches, else `analysis`'s |
| `check` | `Program` | `Vec<Diagnostic>` | Resolved, sorted, deduplicated |

### Firewalls: raw queries and projections

`item_tree_raw` and `file_hir_raw` compute everything about one file in one pass, including text
ranges, and are declared `no_eq`. Nothing semantic reads them. Each is read by small projection
queries (`item_tree`, `item_source_map`, `signature`, `body`, `body_source_map`, …) that compare
their result with the previous revision. When a keystroke only shifts text, the raw query re-runs,
the source-map projections change, and the semantic projections compare equal and are
*backdated*: no query that depends only on them re-runs.

The same idea runs through the HIR: an `ExprStore` (the arenas of one signature or body) holds no
ranges; its `StoreSourceMap` does. Its `source_order` gives each expression its pre-order position,
an order that edits elsewhere do not change (desugared nodes share their form's position).

### Anchors instead of ranges

Query results never hold text ranges. A diagnostic inside a query is a `Diag` whose location is an
`Anchor` (`ItemName(item)`, `Node(Owner::Body(item), NodeId::Expr(id))`, `Segment(…)`, …). Only
`check` resolves anchors through the source maps (`Diag::resolve`) into `Diagnostic`s with a
`FilePath` and a `TextRange`. `Anchor::Text(file, range)` is the one raw form, used by parsing and
item-tree-level checks, whose queries depend on the text anyway. See
[Diagnostics](./diagnostics.md).

The effect is tested by `core/kagari-lang/tests/incremental.rs`: after `check` and lowering every
item, it changes a string inside one scene, runs both again, and asserts from salsa's
`WillExecute` events that `infer` and `item_mir` ran once (for that scene) and `def_maps`,
`program_signatures` and `const_value` did not run at all.

### Whole-program queries

`def_maps`, `program_signatures`, `instant_recursion`, `const_eval` and `analysis` are keyed by
`Program`. `program_signatures` is deliberately one query for the whole program: type aliases and
constants used in types reach across files, and computing them per item would create query
cycles. The only queries that may cycle are `const_value` and `static_value`, which declare a
`cycle_result` that reports `E4006`.

### Interning

There are two kinds of interning:

- **Salsa keys.** `SourceFile`, `AssetFile` and `Program` are salsa input entities; `ItemId` and
  other plain keys are interned by salsa when a tracked function takes more than one argument.
- **Semantic types.** `crate::intern` is a global hash-consing interner outside salsa. Each
  `Internable` type has a static table sharded 16 ways by content hash; a value is stored once
  behind a `triomphe::Arc` with its hash, so equality is a pointer comparison and hashing writes
  the cached hash. Tables are never garbage-collected. `Ty` (an interned `TyData`: a `TyKind` plus
  `TyFlags`) and `ty::GenericArgs` use it. `TyFlags` (`HAS_ERROR`, `HAS_INFER`, `HAS_PARAM`) are
  computed once at interning, so `references_error()`, `has_infer()` and `has_param()` are O(1).

### What is cached, and for how long

Every tracked query is memoized for the life of the database. The CLI creates a fresh
`RootDatabase` per command, so salsa only shares work inside one run (`build` reuses everything
`check` computed). Across runs, the CLI keeps the one expensive result, stage 5, on disk and offers
it back (see [Reusing the analysis](./analysis.md#reusing-the-analysis)). The language server
keeps one database for the whole session, so the firewalls above are what makes typing cheap.

## How the tools drive the pipeline

| Driver | What it does |
| ------ | ------------ |
| `kagari check` | Loads the project, `RootDatabase::new(Arc::new(OsFileSystem))`, `project.program(&db)`, offers `<out-dir>/analysis.kagari-cache`, `check`, writes the cache when the analysis ran, prints `render_diagnostics` relative to the project root. `--resolutions` adds `debug::resolution_dump`. Exits 1 when there is an error. Emit errors (`E7xxx`) never appear here: only `build` emits. |
| `kagari build` | Same setup, then `kagari_lang::build`: `check`; if there is an error, no module; otherwise emit, verify (`E7003` on failure), report `E7001` without `fn main`, encode. The CLI then checks asset compatibility and writes archives and, for a debug build, the debug artifact (see [Projects and the CLI](../tooling/cli.md)). |
| Language server | One `RootDatabase` (`OsFileSystem`) per session. `didOpen`/`didChange` call `set_file_text` with the editor's text, `didClose` with the disk text; watched-file events call `set_file_text` for `.kis`/`.kix` files that are not open, `set_asset_exists` for files under a project's asset directory, and reload the projects when a `kagari.toml` changes. Every change spawns a thread with a clone of the database that runs `check` for each loaded project; a later write cancels it (`salsa::Cancelled::catch`). Open files outside every project get only `kagari_syntax::parse` errors. See [Language Server and Editors](../tooling/lsp.md) and [IDE Layer](./ide.md). |
| Tests | `RootDatabase::new` over `OsFileSystem` (fixture directories) or `MemoryFileSystem`; `RootDatabase::with_event_callback` to observe salsa events. |

## Checks before lowering

> **Decision.** Validation checks are independent queries that read HIR, name resolution and the
> inference result. Lowering to THIR and MIR reports no diagnostics and runs only on bodies whose
> resolution and inference reported no error.
>
> **Why.** Lowering code can assume well-formed, well-typed input instead of threading error
> cases through every construct. Diagnostics stay in one place per rule and do not depend on how
> the code is later compiled. The language server gets every error of stages 1–3 without building
> any MIR. HIR plus `InferenceResult` already carries all the static meaning the checks need, so
> there is no separate typed tree for checking.
>
> **Consequences.** `thir(program, item)` returns `None` when the item's `body_resolution` or
> `infer` has an error-severity diagnostic (or std failed to load), and `item_mir` is `None` with
> it. A check that needs control flow reads MIR and must tolerate `None`: `instant_edges` (for
> `E3005`) returns no edges, and const evaluation skips the item; the missing information is
> covered by the errors already reported. Pattern and kind errors do not block lowering, but
> they block emission through the gate below. No `Diag` is ever created in `thir/` or `mir/`;
> keep it that way.

The rule "HIR defines what something is; MIR defines what happens along a path" decides where a
new check goes: a property of a construct (a type, a kind, a pattern) is checked on HIR and
inference; a property of executions (termination, reachability, liveness) is checked on MIR in
stage 5. See [Name Resolution and HIR](./resolve.md#hir) and [THIR and MIR](./mir.md).

## The analysis gate

`check` collects, in this order: def-map diagnostics, `program_signatures` diagnostics, then per
loaded file its parse errors (as `E0001`), item-tree diagnostics, signature and body resolution,
inference, pattern checks and kind checks of every item; then `instant_recursion` and
`const_eval` (`front_diagnostics`). If none of these has `Severity::Error` (`analysis_runs`), it
adds `analysis_diagnostics(program)`. Finally it sorts by `(file, start, end, code, message)` and
removes exact duplicates.

> **Decision.** Stage 5 runs only on a program with no errors from stages 1–4.
>
> **Why.** The analyses *execute* the story with the MIR interpreter and explore its states; they
> assume every body is well-typed and well-kinded and that every constant evaluated.
>
> **Consequences.** The gate is program-wide: one error anywhere (std included) hides every stage 5
> diagnostic, and a studio sees analysis results only after fixing the earlier errors. Warnings do
> not close the gate. `build` applies the same test again before emitting.

Diagnostics of the standard library are part of `check` (their file is `FilePath::Std`); a correct
std produces none, and `kagari-lang`'s fixtures would show them if it did.

## Invariants

- Query results hold no `TextRange`s; locations are `Anchor`s, resolved only in `check` (or by
  `Diag::resolve` in a caller that needs ranges).
- Semantic queries never read `item_tree_raw` or `file_hir_raw`; they read the projections.
- No diagnostics from lowering (`thir/`, `mir/`). Lowering returns `None` instead of guessing.
- `check` runs `analysis` only when stages 1–4 have no error; `build` emits only when `check` has no
  error.
- Inputs change only through `set_file_text` / `set_asset_exists` / `offer_analysis_cache`;
  queries never do I/O except through `Db::source_file` / `Db::asset_file`, which create inputs.
- `crate::intern` tables are global and never freed: intern only small, structural values (types,
  generic argument lists), never per-revision data.
- `incremental.rs` keeps passing: an edit inside one body re-runs only that body's queries and
  their users.

## When you change this

- **New query.** Key item-level queries by `(Program, ItemId)` and return anchored `Diag`s. If it
  reports diagnostics, add it to `check` in `pipeline.rs` (before the gate for stages 1–4, inside
  the `analysis` query for stage 5). Consider whether an edit inside one body must re-run it; if
  not, extend `tests/incremental.rs`.
- **New input or option.** `CompileOptions` fields need a manifest key in `tooling/kagari-project`
  (`manifest.rs`), the Reference [Projects](https://kagari.suitsu31.club/en/reference/tools/projects/) page (en
  and ja), and the table above.
- **New stage-level rule** (what runs before or after the gate): update the Reference
  [Architecture](https://kagari.suitsu31.club/en/reference/overview/architecture/) page in en and ja, which
  describes the stages to studios, and this chapter.
- Run `cargo test -p kagari-lang` (fixtures, `incremental`, `mir`, `bytecode`) and, when the LSP
  path changes, `cargo test -p kagari-lsp`.
