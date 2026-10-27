# Modules, Items and Names (E1xxx)

These errors come from building the module tree, lowering item declarations, and resolving names, imports, labels and paths. They are reported before type checking, and an unresolved name here often suppresses follow-on type errors.

## E1001 `bad-root`

**Error.** The root file of the program does not have a `.kis` or `.kix` extension. `kagari.toml` already rejects such a `project.entry`, so this reaches only programs built through the library.

```text
db.program(FilePath::disk("/game/main.txt"))
```

Fix: point the compiler at a `.kis` or `.kix` root file, e.g. `main.kix`.

## E1002 `file-not-found`

**Error.** A `mod name;` declaration has no matching file; the compiler looks for `name.kis`, `name.kix`, `name/mod.kis` and `name/mod.kix`.

```kis
mod missing;
```

Fix: create one of the expected files (e.g. `missing.kis`) or remove the declaration.

## E1003 `ambiguous-file`

**Error.** A `mod name;` declaration matches more than one file, e.g. both `twice.kis` and `twice.kix` exist.

```kis
mod twice; // both `twice.kis` and `twice.kix` exist next to this file
```

Fix: delete or rename all but one of the candidate files.

## E1004 `file-reused`

**Error.** A file that is already loaded as one module is declared again as another module, for example a root file `main.kis` that declares itself with `mod main;`.

```kis
mod main;
```

Fix: declare each file as a module exactly once and reach it elsewhere with `use`.

## E1005 `file-module-in-block`

**Error.** A file module (`mod name;` without a body) is declared inside a function or block body; file modules may only be declared at module level.

```kis
fn run() {
    mod helpers;
}
```

Fix: move `mod helpers;` to the top level of the file.

## E1006 `unknown-attribute`

**Error.** An attribute is not one of the attributes the compiler knows.

```kis
#[foo]
fn attribute() {}
```

Fix: remove the attribute or use a known one such as `#[scene]`.

## E1007 `attribute-arguments`

**Error.** A known attribute is written with arguments, but Kagari attributes take none.

```kis
#[scene(main)]
pushdown fn Scene() {}
```

Fix: drop the arguments: `#[scene]`.

## E1008 `builtin-outside-std`

**Error.** `#[builtin]` is used outside the standard library; only `std` may declare compiler-provided items.

```kis
#[builtin]
fn x();
```

Fix: remove `#[builtin]` and give the function a body: `fn x() {}`.

## E1009 `builtin-with-body`

**Error.** A `#[builtin]` function has a body, or a `#[builtin]` struct declares fields; built-in items are implemented by the compiler and must be bodiless. Outside the standard library `E1008` is reported as well.

```kis
#[builtin]
fn len(n: u64) -> u64 {
    0
}
```

Fix: remove the body: `fn len(n: u64) -> u64;`.

## E1010 `missing-body`

**Error.** A function that is not `#[builtin]` is declared without a body.

```kis
fn y();
```

Fix: add a body: `fn y() {}`.

## E1011 `missing-value`

**Error.** A `const` has no value, or a `type` alias has no type.

```kis
const LIMIT: u64;
```

Fix: supply the value: `const LIMIT: u64 = 10;`.

## E1012 `trait-outside-std`

**Error.** A trait is declared outside the standard library; user code cannot declare traits.

```kis
trait Shape {
    fn sides(&self) -> u64;
}
```

Fix: remove the trait and use inherent `impl` methods on each type instead.

## E1013 `trait-not-builtin`

**Error.** A trait declared in the standard library is missing `#[builtin]`; every standard library trait must be compiler-provided.

```kis
// in the standard library's `std/mod.kis`
pub trait Eq {}
```

This snippet is illustrative: the standard library ships inside the compiler, and user code gets `E1012` for any trait, so only a modified standard library reaches this code.

Fix: mark it built-in: `#[builtin] pub trait Eq {}`.

## E1014 `duplicate-definition`

**Error.** The same name is defined more than once in one namespace of a module; a note points to the first definition.

```kis
fn a() {}
fn a() {}
```

Fix: rename or remove one of the definitions.

## E1015 `unresolved-import`

**Error.** A `use` path does not lead to any item.

```kis
use nowhere::x;
```

Fix: correct the path to an existing module and item, or remove the import.

## E1016 `unresolved-name`

**Error.** A value, type, path segment or associated item cannot be found in scope.

```kis
fn unresolved() {
    let _ = nothing;
    let _value: Nothing = 1;
}
```

Fix: fix the spelling, import the item with `use`, or define it.

## E1017 `private-item`

**Error.** A path names an item (or associated item) that exists but is not `pub` and is not visible from the use site.

```kis
mod shapes {
    pub struct Dot {}

    impl Dot {
        const SECRET: u64 = 1;
    }
}

fn assoc_uses() -> u64 {
    shapes::Dot::SECRET
}
```

Fix: mark the item `pub` (`pub const SECRET: u64 = 1;`) or stop using it from outside.

## E1018 `ambiguous-name`

**Error.** A name is brought into scope by more than one glob import and the uses refer to different items; notes point at each import.

```kis
mod a {
    pub const X: u64 = 1;
}
mod b {
    pub const X: u64 = 2;
}
use a::*;
use b::*;

fn uses() -> u64 {
    X
}
```

Fix: import the intended item explicitly, e.g. `use a::X;`, or qualify the path: `a::X`.

## E1019 `undeclared-label`

**Error.** `break` or `continue` refers to a loop label that is not declared on any enclosing loop.

```kis
fn labels() {
    loop {
        break 'nope;
    }
}
```

Fix: label the loop (`'nope: loop { ... }`) or drop the label: `break;`.

## E1020 `outside-loop`

**Error.** `break` or `continue` appears outside of any loop.

```kis
fn labels() {
    break;
    continue;
}
```

Fix: move the statement inside a `loop`, `while` or `for`, or use `return`.

## E1021 `self-outside-impl`

**Error.** The `Self` type is used outside of an `impl` or trait.

```kis
fn self_outside() -> Self {}
```

Fix: write the concrete type instead of `Self`, or move the function into an `impl`.

## E1022 `bad-impl-target`

**Error.** An inherent `impl` targets a type it may not extend, such as a primitive type outside the standard library.

```kis
impl bool {}
```

Fix: wrap the primitive in your own struct and implement methods on that.

## E1023 `trait-impl-outside-std`

**Error.** User code implements a trait; only the standard library implements traits. For `Eq`, the compiler derives it automatically for structs and enums whose fields are all `Eq`.

```kis
struct Square {
    side: u64,
}

impl Eq for Square {}
```

Fix: remove the trait impl; use an inherent `impl Square { ... }` for methods.

## E1024 `duplicate-binding`

**Error.** The same identifier is bound more than once within a single pattern.

```kis
fn patterns(p: (u64, u64)) {
    let (x, x) = p;
}
```

Fix: give each binding a distinct name: `let (x, y) = p;`.

## E1025 `inconsistent-binding`

**Error.** In an or-pattern, a variable is bound in some alternatives but not in all of them.

```kis
fn patterns(o: Option<u64>) {
    match o {
        Some(a) | None => {}
    }
}
```

Fix: split the alternatives into separate arms, or bind the variable in every alternative.

## E1026 `missing-std-item`

**Error.** An item the compiler relies on (such as a lang type or function under `std::`) is missing from the standard library being compiled; this indicates a broken or mismatched standard library rather than a user error.

```kis
// in the standard library's `std/mod.kis`, with this line deleted:
// #[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
```

This snippet is illustrative: the standard library ships inside the compiler, so only a modified standard library reaches this code.

Fix: restore the missing `std::` item, or use the standard library that ships with the compiler.

## E1027 `asset-not-found`

**Error.** An `asset("...")` call names a file that does not exist in the project's assets.

```kis
const MISSING: ImageAsset = asset("missing.png");
```

Fix: add the file or correct the path, e.g. `asset("cg_001.png")`.

## E1028 `empty-asset-path`

**Error.** An `asset("...")` call is given an empty path.

```kis
const EMPTY: ImageAsset = asset("");
```

Fix: pass a non-empty asset path, e.g. `asset("cg_001.png")`.

## E1029 `asset-outside-asset-dir`

**Error.** An `asset("...")` path leaves the asset directory (`[assets] dir` in `kagari.toml`, or the root file's directory without a manifest).

```kis
const OUTSIDE: ImageAsset = asset("../secret.png");
```

Fix: move the file into the asset directory and name it relative to that directory, e.g. `asset("bgimage/room.png")`.
