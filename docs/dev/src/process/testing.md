# Testing

**Where in the code**

- Toolchain and dev shell: `rust-toolchain.toml`, `flake.nix`, `.envrc`.
- Integration tests: `<crate>/tests/*.rs`, fixtures next to them, snapshots in `<crate>/tests/snapshots/`.
- Unit tests: `src/tests.rs` or `src/**/tests.rs` modules behind `#[cfg(test)]`.
- tree-sitter corpus: `tooling/tree-sitter-kagari/test/corpus/*.txt`.

## Toolchain and dev shell

`rust-toolchain.toml` pins Rust **1.99**; rustup installs it on first use. Nothing else in the
repository needs a specific Rust version.

The desktop runtime links system libraries: ALSA (through `cpal`, used by `kira`) and libudev
(through `gilrs`), and it loads Wayland, X11, xkbcommon, Vulkan and GL at run time. On NixOS the
flake's dev shell provides them:

```sh
direnv allow        # with direnv: .envrc runs `use flake`
nix develop         # without direnv
```

The dev shell provides only system libraries and sets `LD_LIBRARY_PATH` for the libraries loaded
with `dlopen`; Rust itself still comes from rustup and `rust-toolchain.toml`. On Debian/Ubuntu
install `libasound2-dev libudev-dev pkg-config`, on Fedora `alsa-lib-devel systemd-devel`;
macOS and Windows need nothing. The compiler and tooling crates need none of these.

## Running the tests

```sh
cargo test --workspace                 # everything in the Cargo workspace
cargo test -p kagari-lang              # one crate
cargo test -p kagari-lang --test game_source   # one integration test target
cargo clippy --workspace --all-targets # lints (each crate root denies four clippy lints)
cargo fmt --all --check                # Rust formatting (rustfmt defaults; no rustfmt.toml)
```

`cargo test --workspace` does not cover `tooling/zed-kagari` (excluded from the workspace) or the
tree-sitter grammar. Test those when you change them:

```sh
cd tooling/tree-sitter-kagari && tree-sitter generate && tree-sitter test
cd tooling/zed-kagari && cargo build --target wasm32-wasip2
```

No test needs a GPU, a window, an audio device or the network. The `kagari-cli` tests run the
real `kagari` binary; Cargo builds it for them.

## Snapshot tests

Most compiler and tooling tests are [`insta`](https://insta.rs) snapshot tests over fixture files,
driven by `insta::glob!`. Snapshots are committed in `tests/snapshots/` and named after the test
and the fixture path, for example
`game_source__game_source@analysis__003__influence__main.kix.snap`.

When a change alters output on purpose:

```sh
cargo insta test -p kagari-lang --review   # needs `cargo install cargo-insta`
# or, without cargo-insta:
INSTA_UPDATE=always cargo test -p kagari-lang
git diff -- '*.snap'                       # review every changed snapshot
```

Read every changed snapshot before committing it. A snapshot that changed for a reason you cannot
explain is a bug. Delete the snapshot of a fixture you delete or rename; insta does not.

## What each crate tests

| Crate | Tests | Fixtures |
| ----- | ----- | -------- |
| `kagari-syntax` | `tests/snapshots.rs`: the CST debug dump and the errors of every fixture, asserting the tree is lossless; `every_prefix_is_lossless`: every char-boundary prefix of every fixture parses to a tree with exactly that text. `tests/ast.rs`: the typed AST | `tests/parse/{kis,kix}_NNN__name.{kis,kix}` |
| `kagari-lang` | `tests/game_source.rs`: per fixture, the rendered diagnostics, the resolution dump and the render dump; every fixture without errors must also build to verified bytecode (or fail only with `E7001`, no `main`) | `tests/game_source/<area>/NNN__name/main.{kis,kix}`, areas `script`, `analysis`, `style`, `ui`; a fixture may have more modules and asset files next to `main` |
| | `tests/bytecode.rs`: each fixture builds without errors, `decode(encode(module)) == module`, and the snapshot is the warnings plus the disassembly of the game's functions. `tests/mir.rs`: the MIR of every game item of the same fixtures | `tests/bytecode/NNN__name/main.{kis,kix}` (with the assets they use) |
| | `tests/incremental.rs`: editing one scene re-runs only that scene's queries (salsa events). `tests/asset_root.rs`: asset and system font path resolution, in a `MemoryFileSystem` | Inline sources |
| | `tests/diagnostics_book.rs`: every `DiagnosticCode` has exactly one `## E…`/`## W…` heading in `docs/site/src/content/docs/en/diagnostics/*.md` and in `docs/site/src/content/docs/ja/diagnostics/*.md` | The site's Diagnostics pages themselves |
| | Unit tests in `src/eval/tests.rs`, `src/emit/tests.rs`, `src/ide/tests.rs` | Inline sources |
| `kagari-fmt` | `tests/format.rs`: each fixture's formatted output (snapshot) and idempotence; `corpus_is_stable`: every parseable `.kis`/`.kix` under `core/kagari-std/kis`, `core/kagari-lang/tests/{bytecode,game_source}` and `core/kagari-syntax/tests/parse` formats idempotently; syntax errors are refused | `tests/fixtures/NNN__name.{kis,kix}` |
| `kagari-bytecode`, `kagari-num`, `kagari-archive`, `kagari-compat`, `kagari-project` | Unit tests (`src/tests.rs`, `src/archive/tests.rs`, `src/save/tests.rs`): round trips, verifier rejections, corruption, arithmetic edge cases, manifest parsing | Built in the tests (`tempfile` for file systems) |
| `kagari-vm` | `tests/vm.rs`: hand-assembled modules (`common::hand`) for instruction semantics and traps; `tests/story.rs`: story fibers on programs compiled from Kagari source (`common::compile`, a dev-dependency on `kagari-lang`) | `tests/common/mod.rs` |
| `kagari-text` | `src/tests.rs`: ruby, kinsoku, vertical text, rasterization | `tests/fonts/kagari-test-cjk.otf` (OFL, see `OFL.txt`) |
| `kagari-engine` | `src/tests/{story,saves,settings,reveal,reactive,focus}.rs` on a booted engine with `FakePlatform`; `src/values/tests.rs` | Sources compiled by `test_support::compile`; `tests/fixtures/std_types/main.kix` interns every std type of `values/table.rs` |
| `kagari-render` | `src/tests.rs`: viewport letterboxing, blur weights, thumbnail limits; no GPU | — |
| `kagari-cli` | `tests/{init,check,build,fmt}.rs` run the `kagari` binary in a temp dir with `HOME` and the XDG directories pointed at `tests/font_home`, whose only system font is `TestSans.ttf` | `tests/common/mod.rs`, `tests/font_home/` |
| `kagari-lsp` | `tests/lsp.rs` drives the real server loop over an in-memory connection | Temp dirs |
| `mdbook-kagari` | `src/tests.rs`: Kagari blocks become one-line `<pre>` blocks that stay inside list items and block quotes and show exactly the code; other blocks and text are untouched; Zed's capture precedence; snippets mixing statements and items | Inline Markdown |
| `kagari-desktop-runtime` | `src/tests.rs`: key mapping, headless script parsing and exit codes, store names | — |

## Adding tests

- **Parser change** (`kagari-syntax`): add `tests/parse/kis_NNN__what.kis` or `kix_NNN__what.kix`
  with the next free number; the prefix test covers it automatically. Mirror the change in the
  tree-sitter corpus.
- **A new diagnostic or check** (`kagari-lang`): add a fixture under `tests/game_source/<area>/`
  that triggers it (errors and warnings both), add the code's entry to the site's Diagnostics
  pages in en and ja (the `diagnostics_book` test fails otherwise), and accept the new
  snapshot.
- **Codegen or a new instruction**: add or extend a fixture under `tests/bytecode/`; it yields a
  bytecode snapshot and a MIR snapshot. Add a VM test in `runtime_libs/kagari-vm/tests/vm.rs` for
  the instruction's semantics and traps.
- **Std or engine behaviour**: write the program inline and compile it with
  `test_support::compile` in a `kagari-engine` test; add a type to `tests/fixtures/std_types` when
  `values/table.rs` gains one.
- **Formatter**: add `tests/fixtures/NNN__what.{kis,kix}`. New Kagari sources under the corpus
  directories are format-checked automatically, so they must format idempotently.
- **CLI behaviour**: a test in `tooling/kagari-cli/tests/` using `common::init_project` and
  `common::kagari`.
- Test files start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`,
  since the crates deny them.
- `core/kagari-lang/tests/game_source/script/001__book_examples` collects examples of the books and
  the proposal and checks that they resolve; add a Reference example there when it is not covered
  by another fixture.

## Runtime smoke test

The desktop runtime has a headless mode for end-to-end checks without a window, audio device or
gamepad (user documentation: [Headless Mode](https://kagari.suitsu31.club/en/reference/tools/headless/)). Use it
after changes to the engine, the renderer, the runtime or the std host imports:

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cat > smoke.txt <<'EOF'
wait 2000
capture title
# the title's Start button
click 285 265
wait 1000
capture first-line
assert-no-crash
EOF
cargo run -p kagari-desktop-runtime -- example/test_game1/target/v0 \
    --headless --script smoke.txt --out shots --seed 1
```

It writes `shots/title.png` and `shots/first-line.png` and keeps its saves under `shots/data`.
Exit status 0 is success, 3 a VM panic, 5 a script error; look at the captures.

`example/test_game1` is not in git: it is a local port of the TyranoScript tutorial game whose
assets cannot be redistributed, and its root module lists what the standard library cannot
express yet. It names the system font `opentype/noto-cjk/NotoSansCJK-VF.otf.ttc`, so `kagari check`
needs Noto Sans CJK in a system font directory. Without it, any project works: `kagari init
example/smoke` creates one from the template (its Start button is at a different position).

**When you change this**

- A new test convention or fixture directory: document it in the table above and, for Kagari
  fixtures, consider adding the directory to the formatter's `corpus_is_stable` list.
- A new system dependency of the runtime: add it to `flake.nix` and to the package lists above
  and in the Reference's runtime chapter.
