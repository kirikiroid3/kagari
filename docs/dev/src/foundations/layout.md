# Repository Layout

**Where in the code**

- `Cargo.toml`: the workspace (members, shared package fields, every dependency version).
- `rust-toolchain.toml`, `flake.nix`, `.envrc`: toolchain and dev shell (see [Testing](../process/testing.md)).
- `.gitignore`, `.gitattributes`, `docs/.gitignore`: what is generated or local.

The repository is one Cargo workspace (resolver 3, edition 2024, version 0.1.0, Apache-2.0) plus
two editor packages that are not workspace members. Crates live in four directories:

| Directory | Holds |
| --------- | ----- |
| `core/` | Everything the compiler needs and nothing platform-specific: syntax, compiler, std sources, numbers, bytecode, archives |
| `tooling/` | What studios run on their machines: the `kagari` CLI, project manifests, formatter, language server, editor support; plus the books' code highlighter |
| `runtime_libs/` | The platform-independent runtime: VM, engine, text, renderer, runtime profiles, debugger |
| `runtime/` | Runtime binaries for a platform: today the desktop runtime |

Every dependency version is declared once in `[workspace.dependencies]`, and crates refer to it
with `{ workspace = true }`. Every crate root denies `clippy::unwrap_used`, `clippy::panic`,
`clippy::expect_used` and `clippy::arithmetic_side_effects`; tests opt out with
`#![allow(...)]`.

## Crate graph

<!-- diagram: layout-crates (mermaid-ish)
flowchart TD
  cli["kagari-cli (bin kagari)"] ==> lsp["kagari-lsp"]
  lsp ==> project["kagari-project"]
  lsp ==> fmt["kagari-fmt"]
  project ==> compat["kagari-compat"]
  project ==> lang["kagari-lang"]
  fmt ==> syntax["kagari-syntax"]
  lang ==> syntax
  lang ==> std["kagari-std"]
  lang ==> num["kagari-num"]
  lang ==> bytecode["kagari-bytecode"]
  lang ==> debug["kagari-debug"]
  debug ==> archive["kagari-archive"]
  compat ==> bytecode
  desktop["kagari-desktop-runtime (bin)"] ==> render["kagari-render"]
  cli ==> debugger["kagari-debugger"]
  desktop ==> debugger
  debugger ==> engine
  debugger ==> debug
  render ==> engine["kagari-engine"]
  engine ==> vm["kagari-vm"]
  engine ==> text["kagari-text"]
  engine ==> archive
  vm ==> bytecode
  vm ==> num
  zed["zed-kagari (outside workspace)"] -.->|grammar, git rev| ts["tree-sitter-kagari (not Cargo)"]
  zed -.->|runs kagari lsp| cli
  mdbook["mdbook-kagari (bin)"] -.->|compiles parser.c| ts
  mdbook -.->|reads highlights.scm| zed
  %% edges implied by other edges are omitted; the table below lists every direct dependency
-->
{{#include ../diagrams/layout-crates.svg}}

The graph omits edges that other edges imply (for example `kagari-cli` also depends on
`kagari-lang` directly). The direct dependencies:

| Crate | Kagari dependencies | Notable external dependencies |
| ----- | ------------------- | ----------------------------- |
| `kagari-syntax` | — | `logos`, `rowan` |
| `kagari-std` | — | — |
| `kagari-num` | — | — |
| `kagari-bytecode` | — | — |
| `kagari-archive` | — | `lz4_flex`, `crc32fast` |
| `kagari-debug` | `kagari-archive` | — |
| `kagari-lang` | `kagari-bytecode`, `kagari-debug`, `kagari-num`, `kagari-syntax`, `kagari-std` | `salsa`, `rowan`, `la-arena`, `imbl`, `triomphe`, `smallvec`, `stacker`, `sha2` |
| `kagari-fmt` | `kagari-syntax` | `unicode-width` |
| `kagari-project` | `kagari-compat`, `kagari-lang` | `toml`, `serde`, `semver` |
| `kagari-lsp` | `kagari-lang`, `kagari-syntax`, `kagari-std`, `kagari-fmt`, `kagari-project` | `lsp-server`, `lsp-types`, `salsa` |
| `kagari-cli` | `kagari-archive`, `kagari-bytecode`, `kagari-compat`, `kagari-debugger`, `kagari-fmt`, `kagari-lang`, `kagari-lsp`, `kagari-project`, `kagari-syntax` | `clap`, `serde`, `serde_json`, `sha2` |
| `mdbook-kagari` | — | `mdbook-preprocessor`, `mdbook-markdown`, `tree-sitter`, `cc` (build) |
| `kagari-compat` | `kagari-bytecode` | — |
| `kagari-vm` | `kagari-bytecode`, `kagari-num` (dev: `kagari-lang`) | — |
| `kagari-text` | — | `harfrust`, `skrifa`, `zeno`, `unicode-linebreak`, `unicode-segmentation` |
| `kagari-engine` | `kagari-bytecode`, `kagari-vm`, `kagari-text`, `kagari-archive` (dev: `kagari-lang`) | `image`, `png`, `sha2` |
| `kagari-render` | `kagari-bytecode`, `kagari-engine`, `kagari-text` | `wgpu`, `bytemuck` |
| `kagari-debugger` | `kagari-archive`, `kagari-bytecode`, `kagari-debug`, `kagari-engine`, `kagari-vm` (dev: `kagari-lang`) | `serde`, `serde_json`, `log` |
| `kagari-desktop-runtime` | `kagari-archive`, `kagari-debugger`, `kagari-engine`, `kagari-render` | `winit`, `wgpu`, `kira`, `gilrs`, `directories`, `clap`, `jiff` |

Two rules follow from the graph and must stay true:

- **The runtime does not depend on the compiler.** `kagari-vm`, `kagari-engine`, `kagari-render`,
  `kagari-debugger` and the desktop runtime reach `kagari-lang` only as a dev-dependency, to
  compile test fixtures. A shipped runtime contains no compiler, and the debugger reads the
  compiler's output (the debug artifact) instead of calling it.
- **The compiler does not depend on the runtime.** `kagari-lang` shares only `kagari-bytecode`
  (the format), `kagari-num` (exact numbers, so const evaluation and the VM compute the same
  values) and `kagari-debug` (the story graph and debug information a debugger reads) with it.

## `core/`

**`kagari-syntax`** — Lossless syntax trees for `.kis` and `.kix`. A `logos` lexer, a hand-written
parser that builds a `rowan` CST containing every byte of the input (including trivia and errors),
a typed AST layer over it, and a syntactic validator (`SourceKind` decides whether forms are
allowed). It is the authority on what the language accepts; the tree-sitter grammar follows it.
See [Syntax: Lexer, CST, AST and Forms](../compiler/syntax.md).

**`kagari-std`** — The standard library as Kagari source, in `kis/std/**/*.kis`, one file per
module, embedded with `include_str!` and served by `kagari_std::file(path)`. `#[builtin]` items
here are the host ABI. Adding a std file means adding it to the `match` in `src/lib.rs`. See
[Builtins and the Host ABI](../std/builtins.md) and [The std–Checker Contract](../std/contract.md).

**`kagari-num`** — `Decimal` (an `i128` mantissa and a scale, normalized) and `Rat` (an `i128`
fraction in lowest terms). Every operation is checked and returns `None` when the result is not
representable. Shared by the compiler and the VM.

**`kagari-lang`** — The compiler: a salsa database (`RootDatabase`) running parse, item trees,
HIR lowering with forms desugared, module trees and name resolution, signatures and type
inference, pattern and kind checks, const evaluation, whole-program story and system analysis,
and bytecode emission. Bodies go HIR + inference → THIR → MIR; const evaluation and the story
exploration interpret MIR, and codegen reads MIR. It also has the `ide` module behind the language
server and `debug` dumps for tests. Entry points: `check` (diagnostics) and `build` (diagnostics
plus verified bytecode). See the Compiler part, starting at [Pipeline and
Queries](../compiler/pipeline.md).

**`kagari-bytecode`** — The `.kagari` format: the in-memory `Module`, `encode`, `decode`, the
load-time `verify`, and `disassemble` (behind `kagari disasm`). No dependencies, so any runtime
can use it. See [The `.kagari` Format](../formats/kagari.md).

**`kagari-archive`** — `.kiarchive` asset archives (streamable front index, Store/LZ4 blocks,
solid blocks, CRC32, numbered patch chains) and `.kisave` save databases (16 KiB pages, in-place
edits, `compact()`). `archive::ArchiveDir` opens a game directory's archives on demand (used by
the desktop runtime and the headless query engine). Used by `kagari build` to pack assets and by
the runtime to read assets and store saves and settings. See
[Archives and Save Databases](../formats/archive.md).

**`kagari-debug`** — `.kagari-cache` files over the `.kisave` container: the explored story graph
(`StoryGraph`, with a walk from the roots to every reached step ID), the debug information of a
debug build (`DebugInfo`), and the two file kinds, the analysis cache and the debug artifact.
The compiler writes them; a debugger or query engine reads them without the compiler. See
[`.kagari-cache` Files](../formats/cache.md).

## `tooling/`

**`kagari-project`** — The `kagari.toml` manifest (`parse_manifest`, unknown keys are errors),
`Project` loading into a compiler `Program`, build targets, and `system_font_dirs`.
`TOOLCHAIN_VERSION` (the crate version) is checked against `[toolchain] kagari`. See
[Projects and the CLI](../tooling/cli.md).

**`kagari-cli`** — The `kagari` binary: `init`, `check`, `build`, `fmt`, `disasm`, `lsp`, `debug`,
`dap` (the story debugger, over `kagari-debugger`). `build` checks assets against the runtime
profiles in `kagari-compat`, then writes
`<out-dir>/<target>/game.kagari` and the `.kiarchive` files. The project template for `kagari init`
lives in `tooling/kagari-cli/template/` and is embedded file by file in
`src/command/init/template.rs`; a new template file must be listed there. Usage, IO and manifest
errors exit with 2.

**`kagari-fmt`** — A width-based formatter for `.kis`/`.kix` (maximum width 100, 4-space indent,
not configurable). It refuses files with syntax errors. Backs `kagari fmt` and LSP formatting. See
[Formatter](../tooling/fmt.md).

**`kagari-lsp`** — The language server over stdio: diagnostics, formatting, go to definition,
hover and document symbols, on top of the compiler's salsa database. `kagari lsp` runs it. See
[Language Server and Editors](../tooling/lsp.md).

**`mdbook-kagari`** — The mdBook preprocessor that the Develop Guide runs (`[preprocessor.kagari]`
in `docs/dev/book.toml`). It replaces `kis`/`kix` code blocks with HTML highlighted by the
tree-sitter grammar, whose `src/parser.c` its build script compiles, and the Zed extension's
`highlights.scm`. Studios never run it. See [Documentation](../process/docs.md#code-highlighting).

**`tooling/tree-sitter-kagari`** — Not a Cargo crate. A tree-sitter grammar (`grammar.js`,
derived from tree-sitter-rust) used only for highlighting (in editors and in the Develop Guide),
brackets, indentation and outlines, so it is more permissive than `kagari-syntax`.
`test/corpus/*.txt` are its tests. Its bindings are all disabled in `tree-sitter.json`;
`mdbook-kagari` compiles `src/parser.c` itself.

**`tooling/textmate-kagari`** — Not a Cargo crate. The canonical TextMate grammar
(`kagari.tmLanguage.json`) for Shiki and TextMate-based editors. It approximates the tree-sitter
grammar with regular expressions and follows the captures of `highlights.scm`. The documentation
site highlights with its own copy, `docs/site/grammars/kagari.tmLanguage.json`, which may lag
behind. See [Documentation](../process/docs.md#code-highlighting).

**`tooling/zed-kagari`** — The Zed extension, excluded from the workspace (`exclude` in
`Cargo.toml`): it builds to WebAssembly (`wasm32-wasip2`) with its own `Cargo.lock` against
`zed_extension_api`. `extension.toml` pins the tree-sitter grammar to a git revision of this
repository, and `languages/kagari/*.scm` hold the queries. The extension launches `kagari lsp`
from `PATH` or from the `lsp.kagari.binary.path` setting.

## `runtime_libs/`

**`kagari-compat`** — Runtime profiles (today only `native_v0`) and the asset formats each can
decode. `sniff` identifies an asset by content, not by extension; `kagari build` rejects assets a
listed profile cannot decode.

**`kagari-vm`** — The bytecode interpreter: `Program::load` (decode, verify, reject required
features), tagged values with copy-on-write sharing, references, the instruction loop, story
fibers with the PRNG, traps, the instruction budget and the call-depth cap. The host is a trait
(`Host`); the VM knows no std import. See [The VM](../runtime/vm.md).

**`kagari-text`** — Fonts, shaping (`harfrust`), line breaking with kinsoku, ruby, vertical text,
emphasis dots, outline and shadow, and glyph rasterization (`skrifa`, `zeno`). No platform code.

**`kagari-engine`** — The std host imports over `kagari-vm`: the screen model with its timeline,
animation, the audio model, the story driver and reveal timing, play modes, the mode stack and
navigation, saves (trace replay), settings and meta stores, and the reactive UI with layout and
focus. It emits a `Frame` draw list and audio commands. It has no platform dependencies: a
`Platform` implementation supplies storage, assets, time and thumbnails. See
[The Engine](../runtime/engine.md).

**`kagari-render`** — The `wgpu` compositor for the engine's `Frame`: draws at design size into a
linear-light premultiplied `Rgba16Float` target, then letterboxes onto a surface, renders
offscreen, or reads back sRGB RGBA8 (thumbnails, headless captures). See
[Text and Rendering](../runtime/text-render.md).

**`kagari-debugger`** — The story debugger without a compiler: the query engine (a headless
runtime on a headless `Platform` that runs the debugger's story commands against a debug build),
the runtime side of the debug protocol, and the instruction hook it installs in the VM. The
desktop runtime serves it with `--debug-port`/`--debug-stdio`; `kagari debug` and `kagari dap`
drive it. It never depends on the compiler. See [The Debugger](../runtime/debugger.md).

## `runtime/`

**`kagari-desktop-runtime`** — The desktop binary (profile `native_v0`): a `winit` + `wgpu` window,
`kira` audio, `gilrs` gamepads, file-backed stores in the platform data directory, and a headless
mode driven by an input script. See [The Desktop Runtime](../runtime/desktop.md).

## Other directories

| Path | Contents |
| ---- | -------- |
| `docs/site` | The documentation site (Astro + Lotus): Tutorial, Language Reference, STD Reference and Diagnostics in ja and en; see [Documentation](../process/docs.md) |
| `docs/dev` | This guide, an mdBook |
| `example/` | Local example projects, ignored by git (see below) |
| `*/tests/` | Integration tests and fixtures; see [Testing](../process/testing.md) |

## Generated and local files

| Path | What | Tracked |
| ---- | ---- | ------- |
| `target/` | Cargo output | No (`.gitignore`) |
| `tooling/tree-sitter-kagari/src/parser.c`, `src/grammar.json`, `src/node-types.json` | Generated by `tree-sitter generate` from `grammar.js` | Yes, marked `linguist-generated` in `.gitattributes` (hidden from diffs and language stats). Regenerate after every `grammar.js` change, never edit by hand |
| `tooling/tree-sitter-kagari/src/tree_sitter/**` | Runtime headers copied verbatim from tree-sitter | Yes, marked `linguist-vendored` |
| `tooling/tree-sitter-kagari/{node_modules,build}/`, `*.wasm`, `*.so` | tree-sitter CLI build output | No |
| `docs/site/{node_modules,dist,.astro}/` | pnpm packages, the built site and Astro's cache | No (`docs/site/.gitignore`) |
| `tooling/zed-kagari/{target,grammars}/`, `extension.wasm` | Built by Zed when the extension is installed as a dev extension | No |
| `**/tests/snapshots/*.snap` | `insta` snapshots | Yes; review every change (see [Testing](../process/testing.md)) |
| `docs/_site/` | The built Develop Guide | No (`docs/.gitignore`) |
| `example/test_game1` | A local port of the TyranoScript tutorial game with its third-party assets, used for runtime smoke tests | No; the whole directory is ignored |
| `.direnv/`, `.idea/`, `omp-session-*`, `rustc-ice-*.txt`, `**/mutants.out*/` | Local tool state | No |

**When you change this**

- A new crate: add it to `members` and to `[workspace.dependencies]` in `Cargo.toml`, deny the four
  clippy lints in its root, add a paragraph and a table row here, and update the SVG and its
  mermaid-ish comment.
- A new dependency between crates: check the two rules above, update the table and, unless an
  existing edge already implies it, the SVG.
- A grammar change: regenerate the tree-sitter sources, update the TextMate grammar
  (`tooling/textmate-kagari`), and bump the `rev` in `tooling/zed-kagari/extension.toml` once the
  grammar commit exists. `mdbook-kagari` recompiles the regenerated parser on the next guide
  build.
