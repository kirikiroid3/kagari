# Builtins and the Host ABI

`std` is written in Kagari. Most of it is ordinary code a studio could have written; the rest is
`#[builtin]`: declarations without a body that the compiler, the VM or the engine implement.
The `#[builtin]` items are the ABI between a compiled game and the engine that runs it.

Where in the code:

- `core/kagari-std/kis/std/**/*.kis`: the library, one file per module.
- `core/kagari-std/src/lib.rs`: `ROOT` (`mod.kis`) and `file(path)`, which embeds every file
  with `include_str!`. The compiler (`core/kagari-lang/src/db.rs`, `def_map.rs`) loads std
  through it as `FilePath::Std(..)`; the language server materializes the same text to a temp
  directory for go-to-definition (`tooling/kagari-lsp/src/std_files.rs`).
- `core/kagari-lang/src/item_tree.rs`: the `#[builtin]` attribute rules.
- `core/kagari-lang/src/signatures/collect.rs` (`std_items`) and `signatures/mod.rs`
  (`StdItems`): std items the checker treats specially, found by std path.
- `core/kagari-lang/src/eval/std_items.rs`: `Builtin`, the `BUILTINS` table, `StdTable`.
  `eval/machine/builtins.rs`: the compile-time implementation of every const builtin.
- `core/kagari-lang/src/emit/codegen/call.rs` (`FnCodegen::builtin`) and `emit/mod.rs`
  (`Emitter::import`): builtins as intrinsic instructions or imports.
- `runtime_libs/kagari-engine/src/host/mod.rs`: `StdImport`, `Bindings`, the `Host` impl.
  `src/lib.rs`: the `import_table!` macro. One import table per std module:
  `screen/mod.rs`, `audio/mod.rs`, `story/narrative.rs`, `system/imports.rs`,
  `system/nav.rs`, `saves/mod.rs`, `ui/imports.rs`.
- `runtime_libs/kagari-engine/src/values/table.rs`: `STD_ADTS`, the std types the engine
  reads and builds, with field order.

## What `#[builtin]` means

| Item | Rule (checked in `item_tree.rs`) | Meaning |
| ---- | -------------------------------- | ------- |
| `#[builtin] fn` | has no body (`E1009` otherwise) | implemented outside Kagari |
| `#[builtin] struct` | unit-shaped, no fields (`E1009`) | an opaque type: a host object, a scalar the VM knows (`Rat`, `Decimal`), or a handle (`Setting<T>`, `Meta<T>`) |
| `#[builtin] trait` | every std trait must be builtin (`E1013`); traits outside std are `E1012` | the checker decides which types implement it |
| any | only in std (`E1008`) | |

A non-builtin function without a body is `E1010`, except trait methods. Body-less trait
methods (`Integer::into`, `Integer::saturate`) are builtins too.

**Decision.** Builtins are declared in Kagari source, with real signatures and kinds.

**Why.** The checker, the IDE, the formatter and the STD Reference see one declaration per
item; kinds and call rules apply to builtins exactly as to user code; the std documentation
lives next to the declaration. Opaque `#[builtin]` types keep engine layouts out of bytecode:
a `Screen` is a host object reached only through builtin calls.

## The three implementations of a builtin

Every builtin function is identified by its std item. What implements it depends on its kind:

| Builtin kind | Compile time | Run time |
| ------------ | ------------ | -------- |
| `const fn` | `eval/machine/builtins.rs` (const evaluation folds renders, constants, tables) | an intrinsic instruction, or an engine import (when `fn` code calls it at run time) |
| `fn` | never evaluated (`Builtin::Runtime`) | an engine import |
| `pushdown fn` | `push_step` and `rng_choose` are handled by the story analysis; `Meta::write` by const evaluation in story mode (it changes nothing the analysis tracks) | STEP, RAND, or an engine import |

So a `const fn` builtin such as `Entity::position` exists **twice**: in the compiler's evaluator
(for folding) and in the engine (`ScreenImport::EntityPosition`), because `fn` code may build
screens at run time. Both must agree.

## Identification {#identification}

There are no numeric builtin ids or hashes; every layer identifies std items by **name**:

| Layer | Key | Table |
| ----- | --- | ----- |
| checker | std path, e.g. `["narrative", "text", "Text"]` | `StdItems` in `signatures/collect.rs` |
| const evaluation | `(std file, owner type or trait, fn name)`, e.g. `("screen.kis", "Entity", "position")` | `BUILTINS` in `eval/std_items.rs` |
| evaluator and emitter std types | `(std file, item name)` | `std_table` in `eval/std_items.rs` |
| bytecode | import path string, e.g. `std::screen::Entity::position` (`paths::assoc_path`: type path + method, generic arguments dropped) | Imports section |
| engine imports | the same path string | `import_table!` in each engine module |
| engine std values | type path + variant and field order | `STD_ADTS` in `values/table.rs` |

`std_table` walks every body-less function of every std file. A function found in `BUILTINS`
*and* declared `const fn` gets its `Builtin` variant; every other one is `Builtin::Runtime`.
The test `every_const_builtin_is_known` (`eval/tests.rs`) fails when a `const fn` builtin has
no evaluator implementation, or a table entry matches nothing.

At run time the engine binds each `ImportId` once at boot (`Bindings::new`): the first engine
module whose table contains the path wins. A path no module provides fails the boot with
`BootError::UnsupportedImport` ("the bytecode imports `…`, which this runtime does not
provide"). That is the run-time compatibility check between a game and a runtime.

`values/tests.rs::std_table_matches_the_compiled_std` compiles a program using the std types and
checks `STD_ADTS` against the module's Types section (paths, variant names and field names in
order). Field order in `.kis` files is therefore ABI.

## Lowering a builtin call

`FnCodegen::builtin` (`emit/codegen/call.rs`) lowers a call of a builtin in this order:

1. `push_step` → STEP; `rng_choose::<T>` → RAND; `std::ui::slider::<LO, HI>` → CALLIMP with
   `LO` and `HI` appended as `Int` arguments.
2. Intrinsics by `Builtin`: `asset`/`system_font` whose argument is a constant string → CONST
   of an Asset constant (otherwise a CALLIMP); `Vec` methods → VECNEW/LEN/VECGET/VECSET/VECPUSH; `Integer::into` → MOVE;
   `Integer::saturate` → ICLAMP; text builders → RUBY/MRUBY/EFFECT/CONCAT/TOTEXT;
   `Rat::new`/`to_decimal` → RATNEW/RATDEC.
3. Everything else → `CALLIMP` of the import for that item, created on first use by
   `Emitter::import` with the path, the kind (`fn` → 0, `const fn` → 1, `pushdown fn` → 2) and
   the parameter count including `self`.

The full mapping is in [The `.kagari` Format](../formats/kagari.md#intrinsics-and-imports).
Arguments are passed raw: a `&self` or `&mut self` receiver arrives at the engine as a
`Value::Ref`. A builtin used as a function value gets a synthetic wrapper function
(`Instance::Builtin`, `call::builtin_body`).

## Implementing an import in the engine

Each engine module declares its imports with `import_table!`:

```rust
import_table!(
    /// The `std::system` imports.
    SystemImport {
        SettingGet = "std::system::Setting::get",
        SettingSet = "std::system::Setting::set",
        // …
    }
);
```

The macro generates the enum with `from_path` and `path`. The module's `call(self, ctx, vm,
args)` matches on the variant, decodes the arguments with the typed decoders in
`values/decode.rs` (they deref references and check the std type), does the work on
`EngineState`, and returns a `Value`. Errors are `Trap::host(message)`; the convention is to
name the import path in the message. Reactive reads register a dependency
(`ctx.state.ui.track(..)`), writes invalidate (`ctx.state.ui.invalidate(..)`).

To return a std ADT, build it with `StdTypes::make(StdTy::…, variant, fields)`; the type must be
in `STD_ADTS`.

## Adding or changing a std item {#adding-a-std-item}

End-to-end checklist. Not every step applies to every item; skip the ones that do not.

1. **Declare it** in the module's `.kis` file under `core/kagari-std/kis/std/`, with a doc
   comment, the right kind, and `#[builtin]` only if Kagari code cannot implement it. Keep the
   [design rules](./contract.md#design-rules). A new file needs a `pub mod` in its parent and an
   arm in `core/kagari-std/src/lib.rs::file`. If it belongs in the prelude, add it to
   `prelude.kis`.
2. **Checker special cases.** If the type or kind rules need to know the item (a trait, a
   finite type, a special signature such as `nav::start`), add a field to `StdItems`, fill it in
   `signatures/collect.rs::std_items`, and implement the rule in `infer/` or `typeck/`. Give the
   rule a diagnostic code (see [Diagnostics](../compiler/diagnostics.md)) and record it in
   [The std–Checker Contract](./contract.md).
3. **Const builtin.** Add a `Builtin` variant and a `BUILTINS` row in `eval/std_items.rs`,
   implement it in `eval/machine/builtins.rs`, and test it in `eval/tests.rs`. If the evaluator
   needs a std type, add it to `StdTable`.
4. **Emission.** Default: nothing to do, the call becomes a CALLIMP. Add an intrinsic only for
   hot, engine-independent operations; a new opcode is a format change (load the
   `kagari-bytecode` skill).
5. **Engine.** Add the path to the right `import_table!` and implement the arm in `call`. If the
   engine reads or builds a new std ADT, add it to `STD_ADTS`. For a new std module, add a
   `StdImport` variant and its `from_path` link in `host/mod.rs`. Every `fn` and `pushdown fn`
   builtin, and every `const fn` builtin without an intrinsic, needs an import; without one,
   any game that calls it fails to boot. No test enumerates all builtins against the engine,
   so add an engine test that calls the new item (`test_support::bound_engine` panics on an
   unbound import).
6. **Analysis.** A new `pushdown fn` builtin must be handled by the story analysis and the
   influence analysis (`analysis/influence.rs` treats `pushdown` builtins as sinks); a new step
   type needs the story explorer (`analysis/story/explore`).
7. **Assets and profiles.** A new asset kind touches `AssetKind` in the bytecode,
   `kagari-compat` (`RuntimeProfile` support) and the STD Reference appendix on runtime
   profiles.
8. **Documentation.** Update the module page of the STD Reference in **both**
   `docs/std/en/src/` and `docs/std/ja/src/` (same headings and ids, identical code blocks,
   Stability line), the `appendix/rules.md` page if a compiler rule changed, and the
   Reference's diagnostics pages for new codes.
9. **Tooling.** New keywords or attributes are out of scope for std; a new prelude name or
   form target may need tree-sitter and formatter tests (see
   [Formatter](../tooling/fmt.md)).

Renaming or reordering is an ABI change: import paths, `BUILTINS` rows, `STD_ADTS` field order
and save data (std values stored in `.kisave` files by type path) all depend on names.

## Invariants

- Every `const fn` builtin has an evaluator implementation (`every_const_builtin_is_known`).
- Every builtin that can be emitted as an import has an engine implementation with the same
  semantics as the evaluator's.
- `STD_ADTS` matches the `.kis` declarations (`std_table_matches_the_compiled_std`).
- `#[builtin]` never appears outside std; std traits are all `#[builtin]`.

## When you change this

Load the `kagari-std` skill (`.agents/skills/kagari-std/SKILL.md`) and follow the checklist
above.
