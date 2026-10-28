# IDE Layer

The IDE layer answers editor questions about a program: what is at this offset, where is it
defined, what does it look like, and what does this file contain. It lives in `kagari-lang`, next
to the queries it reads, and has no LSP types in it. The language server
([Language Server and Editors](../tooling/lsp.md)) converts its results to the protocol.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/ide/mod.rs` | Public surface: `definition`, `hover`/`Hover`, `file_symbols`/`FileSymbol`/`SymbolKind`, `NavTarget` |
| `core/kagari-lang/src/ide/lookup.rs` | `reference_at`: offset → owning item, highlight range and what the site denotes (`Found`) |
| `core/kagari-lang/src/ide/definition.rs` | Go to definition: `Found` → `NavTarget` |
| `core/kagari-lang/src/ide/hover.rs` | Hover text: header or type plus doc comment, as Markdown |
| `core/kagari-lang/src/ide/header.rs` | `item_header`, `doc_comment`, `collapse_whitespace` (CST text helpers) |
| `core/kagari-lang/src/ide/symbols.rs` | `file_symbols`: the nested outline of one file |
| `core/kagari-lang/src/ide/tests.rs` | Unit tests over an in-memory two-file program |
| `core/kagari-lang/src/debug.rs` | `resolution_dump` (`kagari check --resolutions`), which enumerates the same reference sites as `lookup.rs` |
| `tooling/kagari-lsp/src/diagnostics.rs` | Diagnostics conversion (`kagari_lang::Diagnostic` → LSP); see [below](#diagnostics) |

## Inputs and the database

Every entry point takes `&dyn Db` plus salsa handles, never paths or text:

```rust
pub fn definition(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<NavTarget>;
pub fn hover(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<Hover>;
pub fn file_symbols(db: &dyn Db, file: SourceFile) -> Vec<FileSymbol>;
```

- `SourceFile` and `Program` are salsa inputs (`core/kagari-lang/src/input.rs`). The caller gets a
  `SourceFile` from `Db::source_file(FilePath)` and a `Program` from
  `RootDatabase::program*` (in practice `kagari_project::Project::program`).
- `offset` is a byte offset (`TextSize`) into the file's current text. Converting editor
  positions (line, UTF-16 column) is the client's job; `kagari-lsp` does it with its `LineIndex`.
- `file_symbols` needs no `Program`: an outline is per file and does not depend on name
  resolution.
- Ranges in results are byte ranges into the target file's text as stored in the database.

The IDE functions are plain functions, not `#[salsa::tracked]`. They are cheap compositions of
tracked queries, so all the expensive work is memoized and shared with `check`:

| Query | Used for |
| ----- | -------- |
| `parse(file)` | CST root, to find method-name and variant nodes and to print headers |
| `item_tree(file)`, `item_source_map(file)` | Items of the file, their name ranges, variant ranges and node pointers |
| `signature_of(item)`, `signature_source_map_of(item)` | Paths in signatures (types, bounds, const args) |
| `body_of(item)`, `body_source_map_of(item)` | Paths, patterns and bindings in bodies |
| `signature_resolution(program, item)`, `body_resolution(program, item)` | What each path resolves to; local variables |
| `inference(program, item)` | Method-call targets and local variable types |
| `def_maps(program)` | Module origins and def paths |

Because salsa keeps the HIR (`body`, `signature`) apart from its source maps
(`body_source_map`, `signature_source_map`), an edit that only shifts text re-runs the parser,
item tree and source maps, while resolution and inference results are backdated and reused (see
[Pipeline and Queries](./pipeline.md)). In the language server, after a keystroke the
first hover or definition request typically re-executes only the queries of the edited file;
`check`, which the server runs in the background, has usually already brought them up to date.

## Finding what is at an offset

`lookup::reference_at(db, program, file, offset)` returns `(owner, range, Found)`:

```rust
pub(crate) enum Found {
    Res(Resolution),   // a resolved path, a binding, a local use
    Method(ItemId),    // the method a method call calls
    DefSite(Def),      // the name of a definition itself
}
```

The algorithm:

1. **Definition sites first.** Walk the file's item tree. If `offset` lies in an item's name
   range, return `DefSite(Def::Item)`. For an `enum`, if it lies in a variant's name range, return
   `DefSite(Def::Variant)`.
2. **Owner.** Otherwise the owner is the smallest item whose syntax range contains `offset`
   (nested items, `impl` methods and inline modules are separate items).
3. **Candidates.** Collect `(range, Found)` pairs from the owner:
   - signature store: every resolved path expression, record path, type path, const argument,
     pattern path and range-pattern bound, with the range of the **last path segment**
     (`std::screen::Color` highlights `Color`);
   - body store: the same, plus every binding pattern (`Pat::Bind`) as `Res(Resolution::Local)`;
   - method calls: for each `inference.method_calls` entry whose callee is an item
     (`FnDefId::Item`), the `NAME_REF` of the `METHOD_CALL_EXPR` in the CST.
   Language paths (`PathKind::Lang`, produced by desugaring) are skipped.
4. **Pick.** Among candidates whose range contains `offset` (inclusive at both ends, so a cursor
   right after a name still finds it), choose the smallest range.

Not navigable, by design of this lookup: field accesses (`a.b`), `use` paths, and anything with
`Resolution::Deferred` or `Resolution::Error`. `Resolution::Prim` has a hover but no definition.

`lookup.rs` mirrors the sites that `debug::resolution_dump` enumerates. If a resolution site is
added to `StoreResolution` (a new expression, type or pattern kind that holds a path), both must
learn it.

## Go to definition

`definition` maps `Found` to a `NavTarget { file: FilePath, range }`:

| Found | Target |
| ----- | ------ |
| `Res(Local)` | The binding pattern of the local (`locals[local].pat` through the body source map) |
| `Res(Def(Item))`, `Res(Assoc)`, `Res(SelfType)`, `Method`, `DefSite(Item)` | The item's name range |
| `Res(Def(Variant))`, `DefSite(Variant)` | The variant's name range |
| `Res(Def(Module))`, `DefSite(Module)` | File module: the file, range `0..0`; inline module: its name; block module: none |
| `Res(GenericParam)` | The generic parameter's name in the owner's signature |
| `Res(Prim / Deferred / Error)` | None |

`NavTarget.file` can be `FilePath::Std(…)`: std declarations are compiled from text embedded in
`kagari-std`. The language server writes such files to disk before returning a location.

## Hover

`hover` returns `Hover { range, markdown }`. The Markdown is a fenced code block tagged
`kagari`, then a blank line and the doc comment when there is one:

| Found | Code block |
| ----- | ---------- |
| Item (any of the item cases above) | `item_header(node)`: the item's text up to its body, value or `;` |
| Local | `let [mut ]name: Ty`, with the type from `inference.local_types` printed by `signatures::display_ty` |
| Variant | The `VARIANT` node's text, whitespace collapsed |
| Module | `mod <def path>` |
| Primitive | Its name (`u32`, `bool`) |
| Generic parameter | Its name |

`item_header` walks the item node's children and stops at the first `BLOCK_EXPR`,
`RECORD_FIELD_LIST`, `VARIANT_LIST`, `ASSOC_ITEM_LIST`, `ITEM_LIST` or `;` (and at `=` for
`const`/`static`), skipping attributes and comments, then collapses every whitespace run to one
space: `pub fn helper(x: u32) -> u32`. An `impl` header doubles as its outline name
(`impl Counter`).

`doc_comment` collects the `///` lines directly above a node (not `////`), walking previous
sibling tokens; a blank line (a whitespace token with two or more newlines) ends the run. Each
line loses `///` and one following space. Doc comments are therefore read from the CST, not
stored in the item tree: editing a doc comment does not invalidate any semantic query.

## Document symbols

`file_symbols(db, file)` returns the items outside function bodies, nested as written:

- children: inline `mod` items, `trait` and `impl` members, and enum variants (`SymbolKind::Variant`);
- `use` items are left out;
- a `fn` in an `impl` or `trait` with a `self` parameter is a `Method`, otherwise a `Function`;
- `full_range` is the whole item; `focus_range` is the item source map's name range, which is the
  whole item for nameless items. An `impl` is named by its header text (`impl Counter`), and its
  `focus_range` is the whole `impl`.

## Diagnostics

The IDE layer does not convert diagnostics; `kagari_lang::check(db, program)` already returns
resolved `Diagnostic`s (`file`, byte `range`, `severity`, `code`, `message`, `related`). The
conversion happens in `kagari-lsp/src/diagnostics.rs`:

| `kagari_lang` | LSP |
| ------------- | --- |
| `file: FilePath::Disk` | grouped by path, one `publishDiagnostics` per file; `FilePath::Std` diagnostics are dropped |
| `range` | `Range` via `LineIndex` (line, UTF-16 column), one index per file per run |
| `severity` | `ERROR` / `WARNING` |
| `code` | `NumberOrString::String("E2001")` |
| `related` (disk only) | `relatedInformation` with location and message |
| — | `source: "kagari"` |

Open files that belong to no loaded program get parser errors only (`kagari_syntax::parse`),
without a code. `render_diagnostics` (`core/kagari-lang/src/diagnostic.rs`) is the CLI's
counterpart: `path:line:col: severity[CODE]: message`, 1-based, columns in chars.

## Tests

`core/kagari-lang/src/ide/tests.rs` builds a `RootDatabase` over a `MemoryFileSystem` with
`/game/main.kix` (a complete game, so the program checks cleanly) and `/game/other.kis`. It
locates offsets by searching for substrings (`offset_of(MAIN_SOURCE, "+ x", 0, 2)`). Covered:
local → `let` pattern, call → other file, method call → method name, std type → `FilePath::Std`,
hover on a local and on a `fn` with doc comment, and nesting/kinds of `file_symbols`. The first
test asserts the fixture has no diagnostics; keep it passing when the std or the language changes,
or every other test silently tests a broken program.

End-to-end behaviour through the protocol is covered by `tooling/kagari-lsp/tests/lsp.rs`.

## Invariants

- No LSP or editor types in `kagari-lang`. Results are byte offsets and `FilePath`s.
- IDE functions only read tracked queries; they never write inputs and never run `check`
  themselves. A request on an erroneous program must still answer what it can (all lookups return
  `Option`).
- Every range returned lies inside the text of the file it names, so position conversion cannot
  fail on a well-formed target.
- `lookup.rs` and `debug::resolution_dump` cover the same reference sites.
- Hover Markdown uses the `kagari` fence tag; the LSP test matches on the header text.

## When you change this

- New item kind: update `SymbolKind` and its mapping in `kagari-lsp/src/convert.rs`
  (`symbol_kind`), `item_header`'s stop list if the item has a new body node kind, and the
  `definition`/`hover` matches.
- New `Resolution` variant or resolution site: update `lookup::store_candidates`,
  `definition::target`, `hover`, and `debug::resolution_dump`.
- New request (references, rename, completion): add a function here that returns compiler types,
  a handler in `kagari-lsp/src/server/requests.rs`, the capability in `server/mod.rs`, a unit test
  in `ide/tests.rs` and a step in `kagari-lsp/tests/lsp.rs`. Update the feature list in the
  Reference's [Editor Support](../../../reference/en/src/tools/editors.md) page (en and ja).
- Run `cargo test -p kagari-lang ide::` and `cargo test -p kagari-lsp`.
