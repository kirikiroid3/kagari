# AGENTS.md

Kagari is a visual novel engine: a Rust-like language (`.kis`/`.kix`), its compiler, a bytecode
VM and a desktop runtime. This file holds only the rules that apply to every change. For design
details, see the **Develop Guide** (`docs/dev/src/`); read the chapter for the code you touch
before changing it.

## Always

- **Update the Develop Guide when the code updates**, in the same change: the chapter that
  describes the code you changed (`docs/dev/src/`, see its `SUMMARY.md`). See the Develop Guide to
  understand the design details.
- **User-visible changes update the user books in English and Japanese**, in the same change:
  The Kagari Reference (`docs/reference/{en,ja}/src/`) for the language, CLI, runtime and
  diagnostics; the STD Reference (`docs/std/{en,ja}/src/`) for `std`. The `ja` tree mirrors
  `en`: same files, same headings with shared `{#id}` anchors, byte-identical code blocks.
  Rules: `docs/dev/src/process/docs.md`.
- **The code is the truth.** When docs and code disagree, fix the docs (or the code, if the docs
  describe the intended behaviour and the user agrees).
- Load the matching skill from `.agents/skills/` before working in its area: `kagari-frontend`
  (syntax, resolve, types, kinds, diagnostics), `kagari-analysis` (THIR/MIR, const eval,
  analysis, emit), `kagari-bytecode` (format, VM, archives), `kagari-std` (std items),
  `kagari-runtime` (engine, text, render, desktop runtime), `kagari-tooling` (CLI, project,
  fmt, LSP, tree-sitter, Zed), `kagari-docs` (any book).

## Commands

```sh
cargo test --workspace                   # all tests; no GPU, window, audio or network needed
cargo clippy --workspace --all-targets   # must stay clean
cargo fmt --all                          # rustfmt defaults
mdbook build docs/reference/en           # likewise docs/reference/ja, docs/std/en, docs/std/ja, docs/dev
python3 docs/check-books.py              # book links, anchors, en/ja mirroring, diagram comments
```

- Toolchain: `rust-toolchain.toml`. On NixOS, `direnv allow` (or `nix develop`) provides the
  system libraries; Debian/Ubuntu need `libasound2-dev libudev-dev pkg-config`.
- Snapshot tests use `insta`: after an intentional output change, review every changed `.snap`
  (`cargo insta test --review`, or `INSTA_UPDATE=always cargo test -p <crate>` and read the diff);
  never accept snapshots blindly, never commit `.snap.new`, and delete snapshots of removed
  fixtures.
- Kagari sources (std, fixtures, examples) are formatted with `kagari fmt` and must stay
  idempotent under it.
- Commit messages are conventional commits: `feat(lang): …`, `fix(runtime): …`, `docs: …`.

## Code rules

- Crate roots deny `clippy::unwrap_used`, `expect_used`, `panic` and `arithmetic_side_effects`;
  use checked operations and `get`. Test files opt out with `#![allow(...)]`.
- Every `DiagnosticCode` has exactly one ``## Exxxx `slug` {#exxxx}`` heading in
  `docs/reference/en/src/diagnostics/*.md` and one in the `ja` mirror
  (`cargo test -p kagari-lang --test diagnostics_book`). Codes are never renumbered or reused.
- Bytecode opcodes are frozen: add, never renumber or repurpose. Std item names, paths and field
  order are the host ABI.
- Runtime crates never depend on `kagari-lang` (dev-dependency only).
- Syntax changes also update `tooling/tree-sitter-kagari` (regenerate; never hand-edit its
  generated `src/` files), the Zed grammar `rev`, and `kagari-fmt`.
- CLI exit codes are a contract: 0 success, 1 problems found, 2 cannot run.
