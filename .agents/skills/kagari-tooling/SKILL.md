---
name: kagari-tooling
description: Load when changing tooling/* (kagari-cli, kagari-project, kagari-fmt, kagari-lsp, tree-sitter-kagari, zed-kagari) or core/kagari-lang/src/ide, or when adding CLI subcommands/flags, kagari.toml keys, formatter rules, LSP features, or grammar rules.
---

# Kagari tooling

## Scope

| Path | What |
| ---- | ---- |
| `tooling/kagari-project` | `kagari.toml` parsing/validation, project discovery, `Project::program`, targets, system font dirs |
| `tooling/kagari-cli` | The `kagari` binary: `init`, `check`, `build`, `fmt`, `disasm`, `lsp`; new-project `template/` |
| `tooling/kagari-fmt` | Width-based formatter (`MAX_WIDTH` 100, `INDENT` 4, not configurable) |
| `tooling/kagari-lsp` | Language server (lsp-server, crossbeam, salsa snapshots) |
| `core/kagari-lang/src/ide` | Go to definition, hover, file symbols over salsa queries |
| `tooling/tree-sitter-kagari` | Editor grammar; `src/parser.c`, `src/grammar.json`, `src/node-types.json` are generated |
| `tooling/zed-kagari` | Zed extension; excluded from the Cargo workspace |

## Read first

- `docs/dev/src/tooling/cli.md` — projects, manifest keys, subcommands, exit codes, archives.
- `docs/dev/src/tooling/fmt.md` — Doc model, printer, comments, verify, idempotence.
- `docs/dev/src/tooling/lsp.md` — server threads, sync, diagnostics, grammar, Zed.
- `docs/dev/src/compiler/ide.md` — `reference_at`, definition, hover, symbols.

## Invariants

- **fmt idempotence**: `format(format(x)) == format(x)`. **fmt preservation**: output reparses to
  the same tokens (commas aside) and comments (`verify.rs`). Files with syntax errors are refused.
- Every significant token is emitted once, via `Formatter::tok`/`tok_bare`/`hoisted`/
  `comments_only`; never push token text directly.
- **Grammar parity**: `grammar.js` accepts everything `kagari-syntax` accepts; `PREC` mirrors
  `core/kagari-syntax/src/cst/parser/exprs.rs`, `KEYWORDS` the lexer. Never hand-edit the
  generated files (`.gitattributes` marks them).
- **CLI exit codes**: 0 ok, 1 problems found (`failure::exit(true)`), 2 cannot run (`Failure`).
- `kagari build` writes nothing if compilation or the runtime-profile check fails; archive entry
  names = bytecode asset paths, grouped by `archive_stem`.
- The CLI and the LSP build programs only through `kagari_project::Project::program`.
- Manifest parsing stays `deny_unknown_fields`, kebab-case, paths relative to the project root.
- LSP: only the main thread writes the db; each write → `diagnostics::refresh`; background work
  inside `salsa::Cancelled::catch`; handlers must not panic the server. Columns are UTF-16.
- `kagari-lang/src/ide` has no LSP types; it returns byte ranges and `FilePath`s.
- `kagari init` output passes `check`, `build`, `fmt --check`.

## Workflows

### Add a manifest key
1. Field in the `Raw*` struct (`tooling/kagari-project/src/manifest.rs`), `Option` unless required.
2. Validate in `parse_manifest` (error names the key in backticks); store in `Manifest` or
   `CompileOptions` (`core/kagari-lang/src/input.rs`, with a default constant).
3. Tests in `tooling/kagari-project/src/tests.rs` (default + each rejection).
4. Template `tooling/kagari-cli/template/kagari.toml` if new projects should set it.
5. Docs: `docs/reference/{en,ja}/src/tools/projects.md` and the table in `docs/dev/src/tooling/cli.md`.

### Add a subcommand or flag
1. `Command` variant/field in `tooling/kagari-cli/src/cli.rs` (doc comment = `--help` text).
2. `command/<name>.rs` with `pub fn run(…) -> Result<ExitCode, Failure>`; register in
   `command/mod.rs`; dispatch in `main.rs`. Use `workspace::load`, `failure::io`, `exit(failed)`.
3. Integration test `tooling/kagari-cli/tests/<name>.rs` using `tests/common`.
4. Docs: `docs/reference/{en,ja}/src/tools/cli.md` (synopsis, flags, exit codes) and
   `docs/dev/src/tooling/cli.md`.

### Change formatter layout / support new syntax
1. Rule in `tooling/kagari-fmt/src/rules/*.rs`; new node kinds get an arm in `Formatter::node`
   (else `verbatim`).
2. Fixture `tests/fixtures/NNN__topic.k{is,ix}` with short, >100-column and commented variants.
3. `cargo test -p kagari-fmt`; review snapshots (`cargo insta review` or
   `INSTA_UPDATE=always` then `git diff tests/snapshots`).
4. Re-format `tooling/kagari-cli/template/**` if the layout changed.

### Add an LSP feature
1. Query in `core/kagari-lang/src/ide/` + unit test in `ide/tests.rs`.
2. Handler in `tooling/kagari-lsp/src/server/requests.rs`, capability in `server/mod.rs`,
   conversions in `convert.rs`/`line_index`.
3. Step in `tooling/kagari-lsp/tests/lsp.rs`.
4. Docs: `docs/reference/{en,ja}/src/tools/editors.md`, `docs/dev/src/tooling/lsp.md`,
   `docs/dev/src/compiler/ide.md`.

### Change the grammar (after any `kagari-syntax` change)
1. Edit `tooling/tree-sitter-kagari/grammar.js` (and `PREC`/`KEYWORDS`).
2. `cd tooling/tree-sitter-kagari && tree-sitter generate && tree-sitter test` (CLI emitting ABI
   15; not in the Nix shell). Add/adjust `test/corpus/*.txt`.
3. Update `tooling/zed-kagari/languages/kagari/*.scm` if node names/fields changed.
4. Commit grammar + generated `src/` together; then set `rev` in
   `tooling/zed-kagari/extension.toml` to that commit.

## Tests

```sh
cargo test -p kagari-project -p kagari-cli      # manifest + CLI integration (runs the binary)
cargo test -p kagari-fmt                        # fixtures, idempotence, corpus stability
cargo test -p kagari-lsp                        # end-to-end server test, line index
cargo test -p kagari-lang ide::                 # IDE queries
(cd tooling/tree-sitter-kagari && tree-sitter test)
```

CLI tests set `HOME`/`XDG_DATA_HOME`/`XDG_DATA_DIRS` to `tests/font_home` (one system font).
The `ide` fixture must check without diagnostics (`fixture_program_checks_cleanly`).

## Docs to update

- User-facing CLI/manifest/editor behaviour: `docs/reference/en/src/tools/{cli,projects,editors}.md`
  **and** the `ja` mirrors (same headings with `{#id}`, identical code blocks).
- Internals: `docs/dev/src/tooling/{cli,fmt,lsp}.md`, `docs/dev/src/compiler/ide.md`.
