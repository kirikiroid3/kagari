---
name: kagari-frontend
description: Load when changing core/kagari-syntax (lexer, parser, AST, form validation) or the parse/item-tree/HIR/def-map/resolve/signatures/infer/typeck/diagnostic parts of core/kagari-lang, or when adding syntax, a form, a type/kind check or a diagnostic code.
---

# Kagari front end (stages 1–3, diagnostics)

## Scope

- `core/kagari-syntax/` — logos lexer (`lex/`), `SyntaxKind` + recursive-descent parser (`cst/`),
  `validate.rs` (forms, literals, forms-in-`.kis`), typed AST (`ast/`, `UI_FORMS` in `ast/forms.rs`).
- `core/kagari-lang/src/`: `db.rs`, `input.rs`, `pipeline.rs` (`check`), `parse_query.rs`,
  `module_files.rs`, `item_tree.rs`, `hir/` (incl. `forms.rs` desugaring), `def_map.rs`,
  `resolve.rs`, `paths.rs`, `assets.rs`, `intern.rs`, `ty.rs`, `callee.rs`, `signatures/`,
  `infer/`, `typeck/` (`exhaustive.rs`, `kinds.rs`, `kinds/`), `diagnostic.rs`.
- Not here: THIR/MIR, eval, analysis, emit (other chapters of the Develop Guide).

## Read first (docs/dev/src/compiler/)

- `pipeline.md` — stages, salsa queries, firewalls, anchors, the analysis gate.
- `syntax.md` — lexer, parser, recovery, forms (how-to lists for tokens, nodes, forms).
- `resolve.md` — item trees, HIR, def maps, imports, prelude, path lookup, asset paths.
- `typeck.md` — signatures, inference (`Int` ranges, coercions, bounds), exhaustiveness, kind rules.
- `diagnostics.md` — `Diag`/`Anchor`, code ranges, stability, "Adding a diagnostic".

## Invariants (do not break)

- Syntax trees are lossless for every input; the parser never panics or loops; `.kis` and `.kix`
  share one grammar and only `validate.rs` reads `SourceKind`.
- Query results hold no `TextRange`s: diagnostics are `Diag` with an `Anchor`; ranges live in
  `ItemSourceMap`/`StoreSourceMap`. Semantic queries never read `item_tree_raw`/`file_hir_raw`.
- HIR defines what something is (no types, no resolved names, errors as `Missing`); MIR defines
  what happens along a path. Checks read HIR + resolution + `InferenceResult`; THIR/MIR emit no
  diagnostics and return `None` for bodies with resolution/inference errors.
- Stage 5 runs only when stages 1–4 report no error; `build` emits only after an error-free `check`.
- `Ty::Error` unifies with everything and checks skip `references_error()` types: one root cause,
  one diagnostic. No inference variable leaves `infer`.
- Desugared code uses `PathKind::Lang(LangItem)` resolved from the std root; `UI_FORMS` attribute
  order = std props struct field order.
- Std items are found by role (`StdItems`), never by comparing names in checks.
- Diagnostic codes are never renumbered or reused; `W` ⇔ warning; severity is per code.
- Crates `#![deny(clippy::unwrap_used, panic, expect_used, arithmetic_side_effects)]`: use
  `checked_*`/`saturating_*`/`Option` in non-test code.
- `tests/incremental.rs` must keep passing: editing inside one body re-runs only that body's
  queries.

## Workflows

### Add syntax (token, keyword, node)
1. `cst/mod.rs` `kinds!` (+ `from_keyword`/`composite_parts`/`describe`); lexer `Tok` only for
   single characters; composites are glued by the parser.
2. Parser in `cst/parser/*.rs` with recovery; `ast_node!` + `Option` accessors in `ast/`.
3. Fixture `core/kagari-syntax/tests/parse/{kis,kix}_NNN__topic.k{is,ix}`; accessor test in
   `tests/ast.rs`.
4. Lower in `hir/lower.rs` (and `item_tree.rs` for items); resolve, infer, check.
5. `tooling/kagari-fmt` rule + fixture; `tooling/tree-sitter-kagari/grammar.js` + `test/corpus`
   (regenerate with `tree-sitter generate`, `tree-sitter test`); Zed queries/`rev` in
   `tooling/zed-kagari` when highlighting changes.

### Add a form
`validate.rs` `FORMS` (or `ast::UI_FORMS`) → std declaration in `core/kagari-std/kis/std/` →
`LangItem` + `std_path` → desugaring in `hir/forms.rs` → type-dependent rules in
`typeck/kinds/forms.rs` → parse fixture, `game_source` fixture, fmt fixture.

### Add a check + diagnostic
1. Pick group and next free number; add the variant to `codes!` in `diagnostic.rs`.
2. Emit a `Diag` from the owning query (narrow anchor, house-style message: lower case, no final
   period, fact then reason/fix).
3. Add a case to `core/kagari-lang/tests/game_source/<area>/NNN__topic/main.kix` (e.g.
   `script/010__type_errors`, `script/011__kind_errors`) and accept the snapshot.
4. Add the entry ``## E3025 `name` {#e3025}`` (Error/Warning line, minimal program, Fix) to
   `docs/site/src/content/docs/en/diagnostics/<group>.md` **and** `docs/site/src/content/docs/ja/diagnostics/<group>.md`
   (same heading and id, byte-identical code block).
5. `cargo test -p kagari-lang --test diagnostics_book`.

## Tests

- `cargo test -p kagari-syntax` — CST snapshots (`tests/snapshots/snapshots__parse_fixtures@*.snap`),
  every-prefix losslessness, AST accessors.
- `cargo test -p kagari-lang` — `game_source` (diagnostics + resolution dump + render dump
  snapshots; error-free fixtures must also `build`), `mir`, `bytecode`, `incremental`,
  `diagnostics_book`, `asset_root`, unit tests (`typeck::exhaustive::tests`, `ide::tests`, …).
- `cargo test -p kagari-fmt` and `cargo test -p kagari-lsp` after syntax or diagnostic-shape changes.
- Snapshots are insta (`insta::glob!`). A changed or new snapshot fails the test and writes
  `*.snap.new`; accept with `cargo insta review` (or `cargo insta test -p <crate> --accept`) if
  cargo-insta is installed, otherwise `INSTA_UPDATE=always cargo test -p <crate>`; then review the
  `.snap` diff with `git diff` before committing. Never commit `.snap.new` files.

## Docs to update in the same change

- Develop Guide chapter(s) above for any internal change.
- User-visible syntax: `docs/site/src/content/docs/{en,ja}/reference/syntax/*.md` (both languages).
- New/changed rule or code: Reference diagnostics pages (en + ja) and the page stating the rule;
  std-item rules: `docs/site/src/content/docs/{en,ja}/std/appendix/rules.md`; std–checker contract:
  `docs/dev/src/std/contract.md`.
- Prelude/std module changes: STD Reference pages (en + ja).
