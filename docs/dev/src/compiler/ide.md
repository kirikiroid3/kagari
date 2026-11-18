# IDE Layer

The IDE layer answers editor questions about a program: what is at this offset, where is it
defined, what does it look like, what does this file contain, what must change to rename it, and
what can be written here. It lives in `kagari-lang`, next to the queries it reads, and has no LSP
types in it. The language server ([Language Server and Editors](../tooling/lsp.md)) converts its
results to the protocol.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/ide/mod.rs` | Public surface: `definition`, `hover`/`Hover`, `file_symbols`/`FileSymbol`/`SymbolKind`, `prepare_rename`/`rename`/`RenameError`, `completions`/`Completions`/`CompletionItem`/`CompletionKind`, `NavTarget`, `TextEdit`, `FileEdits` |
| `core/kagari-lang/src/ide/lookup.rs` | Reference sites (`Site`, `Found`, `FieldId`): `def_sites`, `item_sites`, `reference_at`; `Symbol`, what a site denotes independent of where it is |
| `core/kagari-lang/src/ide/definition.rs` | Go to definition: `Symbol` → `NavTarget` |
| `core/kagari-lang/src/ide/hover.rs` | Hover text: header or type plus doc comment, as Markdown; `item_node`, `variant_node`, `field_node` |
| `core/kagari-lang/src/ide/header.rs` | `item_header`, `doc_comment`, `collapse_whitespace` (CST text helpers) |
| `core/kagari-lang/src/ide/symbols.rs` | `file_symbols`: the nested outline of one file |
| `core/kagari-lang/src/ide/rename.rs` | `prepare_rename`, `rename`: the symbol at an offset, the symbols that share its name, the edits |
| `core/kagari-lang/src/ide/completion.rs` | `completions`: the context before the offset and the names it allows |
| `core/kagari-lang/src/ide/tests.rs` | Unit tests over an in-memory two-file program |
| `core/kagari-lang/src/debug.rs` | `resolution_dump` (`kagari check --resolutions`), which enumerates the resolved paths that `lookup.rs` also turns into sites |
| `tooling/kagari-lsp/src/diagnostics.rs` | Diagnostics conversion (`kagari_lang::Diagnostic` → LSP); see [below](#diagnostics) |

## Inputs and the database

Every entry point takes `&dyn Db` plus salsa handles, never paths or text:

```rust
pub fn definition(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<NavTarget>;
pub fn hover(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<Hover>;
pub fn file_symbols(db: &dyn Db, file: SourceFile) -> Vec<FileSymbol>;
pub fn prepare_rename(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Result<TextRange, RenameError>;
pub fn rename(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize, new_name: &str) -> Result<Vec<FileEdits>, RenameError>;
pub fn completions(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<Completions>;
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
| `parse(file)` | CST root, to find method-name, field, record and variant nodes and to print headers |
| `item_tree(file)`, `item_source_map(file)` | Items of the file, their name ranges, variant ranges, import segment ranges and node pointers |
| `signature_of(item)`, `signature_source_map_of(item)` | Paths in signatures (types, bounds, const args), generic parameter names |
| `body_of(item)`, `body_source_map_of(item)` | Paths, patterns and bindings in bodies |
| `signature_resolution(program, item)`, `body_resolution(program, item)` | What each path and each path prefix resolves to; local variables |
| `inference(program, item)` | Method-call targets, field accesses, struct expressions and patterns, expression and local types |
| `def_maps(program)` | Module origins and scopes, impls, def paths; `resolve_module_path` for `use` paths and completion qualifiers |

Because salsa keeps the HIR (`body`, `signature`) apart from its source maps
(`body_source_map`, `signature_source_map`), an edit that only shifts text re-runs the parser,
item tree and source maps, while resolution and inference results are backdated and reused (see
[Pipeline and Queries](./pipeline.md)). In the language server, after a keystroke the
first hover or definition request typically re-executes only the queries of the edited file;
`check`, which the server runs in the background, has usually already brought them up to date.

## Finding what is at an offset

Everything starts from **sites**: places in a file that name something, each one identifier.

```rust
pub(crate) struct Site { range: TextRange, found: Found, shorthand: Option<TextRange> }

pub(crate) enum Found {
    Res(Resolution),   // a resolved path or path segment, a binding, a local use
    Method(ItemId),    // the method a method call calls
    DefSite(Def),      // the name of a definition itself
    Field(FieldId),    // a named field, declared or used: { adt, variant: Option<u32>, index }
}
```

`def_sites(file)` lists the definition names: every named item (`DefSite(Def::Item)`), every
enum variant (`DefSite(Def::Variant)`) and every named field of a struct or record variant
(`Field`; `field_decls` indexes the `RECORD_FIELD`s with a name, like `Shape::Record`).

`item_sites(program, item)` lists the references inside one item (nested items, `impl` methods and
inline modules are separate items):

- signature and body stores: every resolved path expression, record path, type path, const
  argument, pattern path and range-pattern bound, with the range of the **last path segment**
  (`std::screen::Color` gives `Color`), plus **every earlier segment** from
  `StoreResolution::prefixes` (`Mood` in `Mood::Calm`, `other` in `other::helper()`);
- the signature's generic parameter names, as `Res(GenericParam)`;
- the body's binding patterns (`Pat::Bind`) as `Res(Resolution::Local)`, with the range of the
  name (`x` of `mut x`, `binding_name`);
- method calls: for each `inference.method_calls` entry whose callee is an item
  (`FnDefId::Item`), the `NAME_REF` of the `METHOD_CALL_EXPR`;
- fields: each named field access (`inference.fields`; the struct is the base's type after its
  `derefs`), and each field of a struct expression (`inference.records`) or struct pattern
  (`inference.pat_ctors`), found in the `RECORD_EXPR`/`RECORD_PAT` node at the expression's or
  pattern's range. Records synthesized by forms have the form's range, so they have no fields
  here. A field written in shorthand (`S { x }`, `S { mut x }`) is one name that is both the
  field and the value or binding: every site with that name's range gets `shorthand = Some(the
  whole field)`;
- for a `use` item: each import path resolved again from the module of the `use` item
  (`resolve_module_path`, types and values namespaces, the types namespace only for globs),
  giving a site for each segment through `PathResolution::prefixes`.

Language paths (`PathKind::Lang`, produced by desugaring) are skipped; their prefixes are empty.

`reference_at(db, program, file, offset)` returns `(owner, range, Found)`: a definition site
containing `offset` if there is one, else, in the smallest item containing `offset`, the
smallest site containing it (inclusive at both ends, so a cursor right after a name still finds
it; the first of equal ones, so a shorthand field finds the value, not the field). Nothing is
found for tuple fields (`a.0`) or for anything with `Resolution::Deferred` or
`Resolution::Error`. `Resolution::Prim` has a hover but no definition.

`Found::symbol(owner)` turns a site into a `Symbol`, the definition independent of where it is
named: `Item`, `Variant`, `Module`, `Local { owner, local }`, `GenericParam`, `Field`.
`Res(Assoc)`, `Res(SelfType)` and `Method` are `Item` (for `SelfType`, the `impl` or `trait`);
`Prim`, `Deferred` and `Error` have none. Definition, rename and the sites they compare all go
through it.

`lookup.rs` covers every resolved path that `debug::resolution_dump` prints (the dump shows whole
paths, not their prefixes, fields or `use` paths). If a resolution site is added to
`StoreResolution` (a new expression, type or pattern kind that holds a path), both must learn
it.

## Go to definition

`definition` maps the `Symbol` of the site to a `NavTarget { file: FilePath, range }`:

| Symbol | Target |
| ------ | ------ |
| `Local` | The binding pattern of the local (`locals[local].pat` through the body source map); none for `self` |
| `Item` | The item's name range (the whole item for an `impl`) |
| `Variant` | The variant's name range |
| `Module` | File module: the file, range `0..0`; inline module: its name; block module: none |
| `GenericParam` | The generic parameter's name in the owner's signature |
| `Field` | The field's name in its struct or variant (from `def_sites`) |

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
| Field | The `RECORD_FIELD` node's text, whitespace collapsed (`pub n: u32`) |
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

## Rename

`prepare_rename` returns the range of the name at the offset, or a `RenameError` whose message
the editor shows; `rename` returns, for that name and a new name, the edits grouped by file
(`FileEdits`, sorted by path; edits in text order, deduplicated, never overlapping).

**The target.** `reference_at` gives the site and `Found::symbol` its symbol. Then:

1. The text at the site must equal the symbol's declared name (`symbol_name`); otherwise the
   name is spelled differently there (an import alias, `Self`) and the rename is refused.
   Keyword names (`self`) are refused too.
2. A `mod` item's `Symbol::Item` becomes the module's `Symbol::Module`. A file module (`mod a;`
   or a path to it) is refused: renaming it means renaming the file.
3. The symbol's file must be in the game crate (`DefMaps::files`), so std items are refused.
4. `related` adds the `mod` item of an inline module, which must keep the module's name. Trait
   items need no grouping: only std declares traits and trait impls (`E1012`, `E1023` in game
   code), and std is never renamed.

**The new name** must lex as exactly one `IDENT` (`kagari_syntax::lex::lex`), so keywords and
anything else are refused.

**The edits.** The files searched are the owner's file for a `Local` (and only the owner's
sites), the owner's file for a `GenericParam`, and every game file otherwise; a file whose text
does not contain the old name is skipped without computing its sites. In each file, every
`def_sites` and `item_sites` site whose symbol is one of the target's symbols and whose text is
the old name is replaced by the new name. Sites with other text are left alone, which is what
keeps aliases, `Self` and names synthesized by forms (whose ranges cover the whole form) intact.
A shorthand site rewrites the whole field instead:

| Renamed | `S { n }` | `S { mut n }` (pattern) |
| ------- | --------- | ----------------------- |
| The field, to `count` | `S { count: n }` | `S { count: mut n }` |
| The value or binding, to `count` | `S { n: count }` | `S { n: mut count }` |

A name can denote two definitions at once: `use m::x;` imports both a type `x` and a value `x`
(a module and a constant, say). If a range to be edited is also a site of a symbol that is not
renamed (shorthand sites aside), the rename is refused, since editing the `use` would break the
other import.

Not checked: whether the new name collides with or shadows another name in scope. Code inside
forms is renamed where it is lowered with its own ranges (form attributes, `t![…]`
interpolations, closures in UI forms).

## Completion

`completions` returns `Completions { range, items }`: `range` is the identifier (or keyword) the
cursor is in or right after, else an empty range at the offset; each `CompletionItem` has a
`label`, a `CompletionKind`, a `detail` (an item header from `item_header`, a field's or
variant's text, `let [mut ]x: Ty` for a local, `mod <path>` for a module) and the doc comment.
It returns `None` outside the program, inside comments and string literals,
and where a name is being declared (the word is in a `NAME`, or the previous token is `let`,
`mut`, `fn`, `struct`, `enum`, `trait`, `mod`, `type`, `const`, `static`), except that after
`let` it offers the keyword `mut`. Labels are unique: the first one offered wins, so locals
shadow items and inner items shadow outer ones.

The **context** is the previous non-trivia token before the word:

| Before | Completes | How |
| ------ | --------- | --- |
| `.` (in a `FIELD_EXPR` or `METHOD_CALL_EXPR`) | Fields, then methods | The receiver is the expression of the receiver node's range in the body source map, typed by `inference.expr_types`, references stripped. Named fields of a struct; methods (`fn` with `self`) of the inherent impls (visible ones), of the traits of its trait impls, of a generic parameter's bounds and of a trait's `Self` |
| `::` (a `COLON2`, or two adjacent `:` when nothing follows: the parser glues `::` only before a segment) | The qualifier's members | The qualifier is the outermost `PATH` ending at the token before `::`, lowered with `lower_path` and resolved in the types namespace from the module at the offset (`Self::` from the enclosing impl or trait). A module gives its visible scope entries, a struct, enum, alias or primitive its variants and associated items (`DefMaps::inherent`, then trait impls), a trait its items |
| `{` or `,` in a `USE_TREE_LIST` | The members of the list's path | The paths of the enclosing `USE_TREE`s, joined |
| A field position of a `RECORD_EXPR_FIELD_LIST` / `RECORD_PAT_FIELD_LIST` | The fields not written yet | The record at the list's parent range, `inference.records` / `pat_ctors`; for an expression, the scope follows |
| Anything else | The scope, then keywords | Locals in scope, generic parameters of the item and of its impl or trait, the entries of the module at the offset and of its enclosing block modules up to the named module, `std`, the prelude and the primitive types. In a `PATH_TYPE`, or after the `:` of a `let`, parameter, field, `const` or `static` or after `->`, only the types namespace (and no locals). Among items, keywords only (below) |

The module at the offset is the smallest block with items (`ItemSourceMap::blocks` whose
`block_modules` entry exists) containing it, else the inline module of a `mod` item, the module of
the smallest item, or the file's module.

A local is in scope when its pattern ends before the offset and the offset is in its scope,
found from the binding's `IDENT_PAT` upwards: after a `LET_STMT` to the end of its block; in the
`BLOCK_EXPR` guarded by a `LET_EXPR` (`if let`, `while let`); in the body of a `FOR_EXPR`; in a
`MATCH_ARM`, `CLOSURE_EXPR` or `FN`. `self` is always in scope in its body.

**Keywords** (`CompletionKind::Keyword`) follow from a `KeywordContext`. `keyword_context` starts
at the node the cursor is in (the word's parent, or the parent of the token at the offset) and
climbs out of every node the word starts and of the wrappers `NAME_REF`, `PATH_SEGMENT`, `PATH`,
`PATH_EXPR`, `EXPR_STMT` and `ERROR` (a stray word among items is an `ERROR` node). The node it
stops at decides:

| Stops at | Context | Keywords |
| -------- | ------- | -------- |
| `SOURCE_FILE`, `ITEM_LIST`; or the previous token is `pub` | `Item` | `fn struct enum type const static mod use impl pub pushdown` (no `pub` after `pub`; no `trait`, which only std declares). No names |
| `ASSOC_ITEM_LIST` (also after `pub` inside one) | `Assoc` | `fn const type pub pushdown`. No names |
| `BLOCK_EXPR` | `Statement` | `let`, the expression keywords, the item keywords without `pub`; `else` when the previous token closes the last block of an `IF_EXPR` without `else` |
| An expression node (`LET_STMT`, `BIN_EXPR`, `ARG_LIST`, `MATCH_ARM`, form attributes and elements, `TEXT_INTERP`, `CONST`/`STATIC` initializers, …) | `Expression` | `if match loop while for true false crate super`; `Self` with an impl or trait; `return` when the owner is a `fn`; `break continue` inside a `LOOP_EXPR`/`WHILE_EXPR`/`FOR_EXPR` of the same function or closure; `mut` after `&` |
| `USE`, `USE_TREE`, `USE_TREE_LIST` | `UsePath` | `crate self super` |
| Anything else (attributes, parameter, field and variant lists, generic parameter lists) | `None` | none |

A type position (`types_only`) is `Type`: `crate super`, `Self`, and `fn const pushdown scene` for
function pointer and closure types.

Typing `value.` leaves a `FIELD_EXPR` without a name; it lowers to `Expr::Field { name:
FieldName::Missing }`, which inference checks as its base only (the parser already reported the
error), so the receiver has a type for the completion.

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
path prefix and field → their definitions, hover on a local, a field and a `fn` with doc comment,
nesting/kinds of `file_symbols`; rename of a local and of a field (shorthand fields spelled out),
of an enum through a path prefix, an enum variant, a generic parameter, an inline module, a
function in the other file, a method and a function imported under an alias (the alias stays),
each followed by checking the renamed program again; rename refusals, including a name one `use`
imports twice; completion after `.`,
after `Enum::` (unglued colons) and `module::`, in a `use` list, in scope, in a struct
expression, in types only (after `:` and in a type path), none at a declared name, and the
keywords of each `KeywordContext` (items, `impl` items, after `pub`, statements, `break` in a
loop, `else` after an `if`, expressions, `mut` after `let`, `use` paths).
Completion tests edit the text with `set_file_text`.
The first test asserts the fixture has no diagnostics; keep it passing when the std or the
language changes, or every other test silently tests a broken program.

End-to-end behaviour through the protocol is covered by `tooling/kagari-lsp/tests/lsp.rs`.

## Invariants

- No LSP or editor types in `kagari-lang`. Results are byte offsets and `FilePath`s.
- IDE functions only read tracked queries; they never write inputs and never run `check`
  themselves. A request on an erroneous program must still answer what it can (all lookups return
  `Option`).
- Every range returned lies inside the text of the file it names, so position conversion cannot
  fail on a well-formed target.
- `lookup.rs` covers every resolved path `debug::resolution_dump` prints, plus path prefixes,
  fields, generic parameter names and `use` paths.
- A site's range is one name; rename only replaces sites whose text is the old name.
- Rename never edits a file outside the game crate.
- Hover Markdown uses the `kagari` fence tag; the LSP test matches on the header text.

## When you change this

- New item kind: update `SymbolKind` and its mapping in `kagari-lsp/src/convert.rs`
  (`symbol_kind`), `CompletionKind` (`completion::push_item`, `convert::completion_kind`),
  `item_header`'s stop list if the item has a new body node kind, and the `definition`/`hover`
  matches.
- New `Resolution` variant or resolution site: update `lookup::store_sites`, `Found::symbol`,
  `definition::target`, `hover`, `rename::symbol_name`/`symbol_file`, and
  `debug::resolution_dump`.
- New syntax that binds a local: teach `completion::in_scope` its scope.
- New keyword or construct that a keyword starts: add it to the keyword lists of
  `completion.rs` (`ITEM_KEYWORDS`, `EXPR_KEYWORDS`, …) and, for a new node kind that holds
  expressions, to the `Expression` arm of `keyword_context`.
- New request (for example references, which `item_sites` and `Found::symbol` already support):
  add a function here that returns compiler types, a handler in
  `kagari-lsp/src/server/requests.rs`, the capability in `server/mod.rs`, a unit test in
  `ide/tests.rs` and a step in `kagari-lsp/tests/lsp.rs`. Update the feature list in the
  Reference's [Editor Support](https://kagari.suitsu31.club/en/reference/tools/editors/) page (en and ja).
- Run `cargo test -p kagari-lang ide::` and `cargo test -p kagari-lsp`.
