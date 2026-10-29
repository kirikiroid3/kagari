# Name Resolution and HIR

This chapter covers the part of the front end between the syntax tree and type inference: the
per-file item trees, the HIR (signatures and bodies with forms desugared), the module trees of the
game and the standard library, imports and the prelude, and the resolution of every path, local,
loop label and asset path. Stage 1 ends with HIR; stage 2 is name resolution.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/item_tree.rs` | `ItemTree`, `ItemSourceMap`, `ItemId`; item-level checks (`E1005`–`E1013`) |
| `core/kagari-lang/src/hir/mod.rs` | HIR types: `ExprStore`, `Expr`, `Pat`, `TypeRef`, `Path`, `LangItem`, `Signature`, `Body` |
| `core/kagari-lang/src/hir/lower.rs` | AST → HIR lowering, the `file_hir_raw` query and its projections |
| `core/kagari-lang/src/hir/forms.rs` | Form desugaring (see [Syntax](./syntax.md#forms)) |
| `core/kagari-lang/src/hir/source_map.rs` | `StoreSourceMap`: ranges of HIR nodes and path segments |
| `core/kagari-lang/src/module_files.rs` | File lookup for `mod foo;` |
| `core/kagari-lang/src/def_map.rs` | `def_maps`: crates, modules, scopes, imports, impls, prelude; module-level path resolution |
| `core/kagari-lang/src/resolve.rs` | `signature_resolution`, `body_resolution`: paths, locals, generics, `Self`, labels, asset paths |
| `core/kagari-lang/src/assets.rs` | Asset and system-font path rules, bytecode asset paths |
| `core/kagari-lang/src/paths.rs` | Human-readable paths (`crate::a::B`, `std::screen::Screen`) for messages |
| `core/kagari-lang/src/debug.rs` | `resolution_dump` (`kagari check --resolutions`, fixture snapshots) |
| `core/kagari-std/src/lib.rs`, `kis/std/mod.kis`, `kis/std/prelude.kis` | The embedded std files, its root and its prelude |

## Item trees

`item_tree(file)` lists every item of one file in pre-order: name, visibility, attribute names,
kind and parent (`File`, an inline module/impl/trait, or a block). Items are addressed by
`ItemId { file, index }`, stable as long as the items before them do not change. The tree holds
no text ranges; `item_source_map(file)` holds them (name, attributes, variants, imports and their
segments, a `SyntaxNodePtr` per item and per item-holding block).

`ItemKind` records only what later stages need before HIR: function kind and whether there is a
body or `self`, struct shape, enum variants with shapes, whether a `const`/alias has a value,
whether a `static` is `mut`, the children of traits, impls and inline modules, and the flattened
imports of a `use` (`Import { path, alias, glob }`).

Blocks that directly contain items become `BlockScope`s (owner item, parent block, items); they turn
into anonymous modules in the def map. Items are found only as direct block statements.

Item-level checks happen while the tree is built, since they need no names:

| Code | Check |
| ---- | ----- |
| `E1005` | `mod foo;` inside a block |
| `E1006`, `E1007` | Attributes other than `scene`, `entry`, `must_use`, `builtin` (`KNOWN_ATTRIBUTES`); arguments on them |
| `E1008`–`E1010` | `#[builtin]` outside std; a `#[builtin]` function with a body or a `#[builtin]` struct with fields; a function without a body that is neither `#[builtin]` nor a trait item |
| `E1011` | A `const` or type alias without a value (outside traits) |
| `E1012`, `E1013` | A `trait` outside std; a std trait that is not `#[builtin]` |

Placement rules for attributes that need the item kind in a wider sense (`#[scene]` only on a
`pushdown fn`, `#[entry]` only with `#[scene]`, `#[must_use]` only on structs and enums, `#[shader_arg]` only on structs) are kind
checks (`E3018`), not item-tree checks.

## HIR

> **Decision.** HIR defines *what something is*; MIR defines *what happens along a path*.
>
> **Why.** Checks about a construct (its type, kind, pattern coverage, the rules of a form) need
> the program as written, with names and desugaring resolved but without control flow. Checks
> about executions (termination, reachability, liveness, consistency) need control flow and run on
> MIR in stage 5. Keeping the two apart keeps each check in one place, and HIR plus the inference
> result is enough static meaning for every stage 3 check, so there is no extra typed tree for
> checking (THIR exists only as the input of MIR building).
>
> **Consequences.** HIR is a faithful, desugared image of the source: it has no types, no
> resolved names and no implicit operations (no autoref, no coercions). Those live in side tables
> (`StoreResolution`, `InferenceResult`) keyed by HIR ids.

Each item has up to two HIR stores, produced by one lowering pass over its file
(`file_hir_raw`) and read through the projections `signature(file, index)` and `body(file,
index)` (convenience: `hir::signature_of(db, item)`, `hir::body_of(db, item)`):

- **`Signature`**: generics (`GenericParam { name, kind: Type { bounds } | Const { ty } }`) and a
  `SignatureKind` (`Fn { params, self_param, ret }`, `Struct { fields }`, `Enum { variants }`,
  `Const { ty }`, `Static { ty }`, `TypeAlias { bounds, value }`, `Impl { trait_ref, self_ty }`,
  `Trait { bounds }`).
- **`Body`** of a `fn`, `const` or `static`: parameter patterns, `has_self`, `self_mutable`, and the
  root expression.

Both own an `ExprStore`: `la_arena` arenas of `Expr`, `Pat` and `TypeRef` plus `source_order`.
Ranges are in a separate `StoreSourceMap` (expressions, patterns, types, the segments of each
path, generic parameter names), so semantic queries are unaffected by edits that only move text
(see [Pipeline and Queries](./pipeline.md#firewalls-raw-queries-and-projections)).

Properties of HIR that later stages rely on:

- **Errors become `Missing`.** A malformed or absent expression, pattern or type is
  `Expr::Missing`, `Pat::Missing` or `TypeRef::Missing`, never a panic or a skipped node.
- **Paths are unresolved.** `Path { kind, segments }` with `PathKind::Plain`, `Crate`,
  `SelfModule`, `Super(n)` or `Lang(LangItem)`; segments carry optional generic arguments.
- **Forms are gone.** `s!`, `choice!`, `t!` and UI forms are calls and records with `Lang` paths.
- **Parentheses are gone**; `a op= b` is `Expr::Assign { op: Some(op), … }`; `if let`/`while let`
  use `Condition::Let`.
- **Items in blocks** are `Stmt::Item(ItemId)`, and a block that holds items records its
  `BlockIndex` (`Expr::Block { scope }`), which resolution uses to enter the block's module.
- Literals are parsed: `Literal::Int { value: u128, suffix }`, `Decimal(text)`, `String(Arc<str>)`
  (unescaped), `Bool`.

## Crates and modules

`def_maps(program)` builds everything at module level in one query:

1. **std crate.** `collect_crate(CrateId::Std, FilePath::Std(kagari_std::ROOT))`: the std root is
   `mod.kis`, and its `mod` declarations are followed exactly like the game's, inside the embedded
   file set of `kagari_std::file`.
2. **game crate.** The root `SourceFile` of the `Program`. A missing root is `E1002`
   (`cannot read the root file`); a root that is not `.kis`/`.kix` is `E1001`.
3. **Blocks.** Every item-holding block of every loaded file becomes an anonymous module whose
   parent is the enclosing block's module or the module of the owning item.
4. **Prelude.** The std root's `prelude` module, if present.
5. **Imports** to a fixpoint, then **impls**.

`DefMaps` holds both `CrateDefMap`s (`modules: Vec<ModuleData>`, `root`), `files` (every loaded
file with its crate, in load order: this is the set of files of the program), the maps
`file_modules`, `inline_modules`, `block_modules`, the impl indexes (`inherent`, `trait_impls`,
`impl_keys`, `impl_traits`), `prelude` and the diagnostics. A `ModuleData` has an origin (file,
inline module or block), parent, name and two scopes (`types`, `values`: `Name → ScopeEntry { def,
vis, defined_in, source, ambiguous }`).

Queries for items in files that are not in `def_maps(program).file_modules` return empty results:
the database may hold files that are not part of the program (an open loose file in the editor).

### `mod` file lookup

`module_files::lookup_mod_file` implements Rust 2018 lookup. For `mod foo;` declared in file `F`
inside inline modules `a::b`:

| Declaring file | Directory searched |
| -------------- | ------------------ |
| The crate root, `mod.kis` or `mod.kix` | `F`'s directory, then `a/b/` |
| Any other file `name.kis`/`name.kix` | `F`'s directory + `name/`, then `a/b/` |

In that directory the candidates are `foo.kis`, `foo.kix`, `foo/mod.kis`, `foo/mod.kix`. None
exists: `E1002`, listing the candidates relative to the declaring file's directory; more than one:
`E1003`; the file is already loaded as another module: `E1004`. Existence is read through
`file_exists`, not the text. `mod foo;` inside a block is ignored here (the item tree reported
`E1005`).

### Scopes and namespaces

| Item | Namespace |
| ---- | --------- |
| `mod` | types |
| `struct` with named fields | types |
| tuple or unit `struct` | types and values (the constructor) |
| `enum`, `trait`, `type` alias | types |
| `fn`, `const`, `static` | values |
| `impl` | none (indexed by its self type) |

A constant and a module may share a name, as in Rust (`AZUSA` the entity id and
`AZUSA::CLOSE_EYE_SMILE`). Definitions and named imports clash in their namespace (`E1014`, with
a note at the first definition). Glob imports never replace a definition or named import; when two
globs supply different definitions, the entry records the others in `ambiguous`, and `E1018` is
reported where the name is *used*, not where it is imported.

### Imports

`use` trees are flattened by the item tree into `Import`s. `resolve_imports` runs a fixpoint:
each round tries every pending named import (types and values namespaces separately, so `use
a::X` imports both a type and a value `X` when they exist) and applies every glob (from a module
or an enum), until a round changes nothing. Then:

- an import that resolved to something it may not see is `E1017`;
- an import that resolved in neither namespace is `E1015` at its first failing segment.

### Impls

`collect_impls` resolves each impl's trait and self type in the impl's module. An impl's key is a
`TypeKey`: `Item(struct or enum)` or `Prim(PrimType)` (through aliases too). Rules: trait impls
are allowed only in std (`E1023`; for `Eq` the message explains that `Eq` is structural);
inherent impls on primitive types only in std, and the target must be a struct, enum or
primitive (`E1022`); two inherent items of one type with the same name are `E1014`. Inherent items
go into `inherent[key][name]`; trait impls into `trait_impls[key]` and `impl_traits[impl]`.

## Resolving paths

`signature_resolution(program, item)` and `body_resolution(program, item)` walk one HIR store with
a `Resolver` and produce a `StoreResolution`: `Resolution` per path expression, record path,
pattern path, type reference, const generic argument and range-pattern bound. The body result also
has `locals` (an arena of `Local { name, pat, mutable }`; `self` is a local with `pat: None`),
`bindings` (pattern → local) and `loop_targets` (`break`/`continue` → loop).

`Resolution` is one of `Local`, `Def(Module | Item | Variant)`, `Prim`, `GenericParam { owner,
index }`, `SelfType(impl or trait)`, `Assoc(item)`, `Deferred { base, rest }` or `Error`.

<!-- diagram: resolve-lookup (mermaid-ish)
flowchart LR
  locals["locals (single-segment value paths)"] ==> generics["generic params: item, then impl/trait"]
  generics ==> selfty["Self in impls and traits"]
  selfty ==> scopes["module scopes: block modules up to the named module"]
  scopes ==> std["std (types only)"]
  std ==> prelude["prelude: std::prelude"]
  prelude ==> prims["primitives (types only)"]
  prims -.-> rest["later segments: resolve_rest (modules, variants, associated items, or Deferred)"]
-->
{{#include ../diagrams/resolve-lookup.svg}}

The first segment of a `Plain` path is looked up in this order; the last segment uses the
requested namespace, earlier segments the types namespace:

1. **Locals**, innermost scope first, only for a value path of one segment.
2. **Generic parameters** of the item, then of its impl or trait; const parameters in the values
   namespace, type parameters in the types namespace.
3. **`Self`** inside an impl or trait (`E1021` elsewhere). `self` as a value outside a method is
   `E1016`.
4. **Module scopes**: the current block module, its parent blocks, up to the nearest named module.
   Names of enclosing *named* modules are not visible (Rust's rule).
5. **`std`** (types namespace), the std crate root. A user item named `std` shadows it.
6. **The prelude**: the scope of `std::prelude`. Its contents are whatever `prelude.kis` re-exports;
   the STD Reference page [`std::prelude`](https://kagari.suitsu31.club/en/std/prelude/) lists them.
7. **Primitive types** (`bool`, the Rust integers, `Int`), types namespace.

`crate::`, `self::` and `super::` paths start at the crate root or the named module (`E1016` for
too many `super`s). `Lang` paths start at the std root with the `LangItem::std_path` segments
prepended; failure is `E1026` (`standard library item … is missing`).

Later segments (`resolve_rest`) go through module scopes (privacy: an entry is visible when it is
`pub` or the use site is inside the defining module, else `E1017`), enum variants, associated
items (inherent first, then trait impls of the type; items of a trait impl take the trait's
visibility) and trait items. A path that continues past a generic parameter, a trait's `Self` or an
alias whose target is not a known type becomes `Resolution::Deferred { base, rest }` and is
finished by type inference (`T::Output`, `S::new`). Method names and field names are never
resolved here; inference resolves them from the receiver's type.

Identifier patterns are first tried as paths: a binding pattern whose name resolves to a unit
struct, unit variant, `const` or `static` is a path pattern (`ident_as_path`), as in Rust.

### Locals, patterns and labels

- `let` resolves its type and initializer before binding the pattern, so `let x = x;` sees the
  outer `x`. `match` arms, `if let`, `while let`, `for` and closure parameters open a scope.
- A name bound twice in one pattern is `E1024`. Or-patterns must bind the same names in every
  alternative (`E1025`); later alternatives map their bindings to the first alternative's locals.
- Loops push `(label, loop)`. `break`/`continue` with a label find the innermost loop with that
  label (`E1019` otherwise); without a label the innermost loop (`E1020` outside a loop). Closure
  bodies start with no enclosing loops.

### Asset paths

A call whose callee resolves to std's `asset` or `system_font` function (found through
`LangItem::Asset`/`SystemFont`), in a file on disk, with a string literal argument, is checked
during body resolution:

| Function | Rules |
| -------- | ----- |
| `asset("p")` | Not empty (`E1028`); joined to `Program::asset_root`, must stay strictly inside it (`E1029`); must not enter the reserved `system-fonts/` or `std/` directory (`E1032`, `assets::is_reserved` names the directory); the `AssetFile` must exist (`E1027`) |
| `system_font("p")` | Program option `system_fonts` (`E1031`); not empty (`E1028`); relative and never above its directory (`assets::stays_inside`, `E1029`); found in the first `font_dirs` entry that has it (`E1030`, listing the directories searched) |

A path that is not a string literal is `E2028`, reported by inference, since only a literal can be
checked here. Existence goes through `AssetFile` inputs, so the language server re-checks when an
asset appears or disappears (`set_asset_exists`). The bytecode path of an asset (relative to the
asset root, `system-fonts/<path in its font directory>`, or `std/<name>` for an asset std declares)
comes from `assets::bytecode_path` at
emit time.

## Debugging

`kagari check --resolutions` (and every `game_source` fixture snapshot) appends
`debug::resolution_dump`: for each game file, each resolved path in source order, `file:line:col
path -> description`, plus one line per `break`/`continue` naming its loop. Desugared `Lang` paths
are not shown. `paths::def_path` renders `crate::…`/`std::…` paths, with `{block}` for block
modules.

## Tests

The fixtures in `core/kagari-lang/tests/game_source/script/` cover this chapter:
`002__modules` (files, `mod`/`use`), `003__module_errors`, `004__resolve_errors`, `006__assets`,
`007__kis_root`, `008__impls_and_blocks`, `009__trait_errors`, `005__patterns`. Each snapshot holds
the diagnostics and the resolution dump. `core/kagari-lang/tests/asset_root.rs` covers asset roots
and system fonts with temporary directories; `assets.rs` has unit tests for the path rules.

## Invariants

- Item trees and HIR stores hold no text ranges; ranges live in `ItemSourceMap` and
  `StoreSourceMap`.
- Lowering never fails: errors become `Missing` nodes.
- Every path in HIR either resolves, or is reported exactly once and resolves to
  `Resolution::Error`; later stages skip `Error` silently.
- `def_maps` reads only item trees, `file_exists` and impl signatures, never bodies or text, so
  edits inside bodies do not re-run it.
- Desugared code resolves through `PathKind::Lang` from the std root, independent of user scopes
  and imports.
- `DefMaps::files` is the program's file set; queries on other files return empty results.

## When you change this

- **New attribute.** Add it to `KNOWN_ATTRIBUTES` in `item_tree.rs`, give it placement rules
  (`typeck/kinds.rs::check_attributes`), and document it in the Reference
  [Attributes](https://kagari.suitsu31.club/en/reference/special/attributes/) page (en and ja).
- **Module or `use` rules.** Update `module_files.rs`/`def_map.rs`, the
  `002__modules`/`003__module_errors` fixtures, and the Reference
  [Source Files and Modules](https://kagari.suitsu31.club/en/reference/syntax/modules/) page (en and ja).
- **Prelude.** Edit `core/kagari-std/kis/std/prelude.kis`; update the STD Reference
  [`std::prelude`](https://kagari.suitsu31.club/en/std/prelude/) page (en and ja).
- **New std file.** Add it to the `match` in `core/kagari-std/src/lib.rs`, or the `mod` lookup
  cannot find it.
- **New `LangItem`.** Add the variant and its `std_path`; the std item must exist at that path.
- **New resolution site or `Resolution` variant.** Update `debug::resolution_dump` and the IDE
  lookup (`ide/lookup.rs`, see [IDE Layer](./ide.md)).
- **New diagnostic.** See [Diagnostics](./diagnostics.md#adding-a-diagnostic).
- Run `cargo test -p kagari-lang`.
