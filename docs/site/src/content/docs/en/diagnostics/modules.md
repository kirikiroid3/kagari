---
title: "Modules, Items and Names (E1xxx)"
---

These errors come from building the module tree, lowering item declarations, and resolving names, imports, labels and paths. They are reported before type checking, and an unresolved name here often suppresses follow-on type errors.

## E1001 `bad-root` {#e1001}

**Error.** The root file of the program does not have a `.kis` or `.kix` extension. `kagari check` and `kagari build` read the root file from `project.entry` in `kagari.toml`, which already rejects other extensions, so a project built with the CLI does not normally see this code.

```toml
[project]
entry = "src/main.txt"
```

Fix: point `project.entry` at a `.kis` or `.kix` root file, e.g. `src/main.kix`.

## E1002 `file-not-found` {#e1002}

**Error.** A `mod name;` declaration has no matching file; the compiler looks for `name.kis`, `name.kix`, `name/mod.kis` and `name/mod.kix`.

```kis
mod missing;
```

Fix: create one of the expected files (e.g. `missing.kis`) or remove the declaration.

## E1003 `ambiguous-file` {#e1003}

**Error.** A `mod name;` declaration matches more than one file, e.g. both `twice.kis` and `twice.kix` exist.

```kis
mod twice; // both `twice.kis` and `twice.kix` exist next to this file
```

Fix: delete or rename all but one of the candidate files.

## E1004 `file-reused` {#e1004}

**Error.** A file that is already loaded as one module is declared again as another module, for example a root file `main.kis` that declares itself with `mod main;`.

```kis
mod main;
```

Fix: declare each file as a module exactly once and reach it elsewhere with `use`.

## E1005 `file-module-in-block` {#e1005}

**Error.** A file module (`mod name;` without a body) is declared inside a function or block body; file modules may only be declared at module level.

```kis
fn run() {
    mod helpers;
}
```

Fix: move `mod helpers;` to the top level of the file.

## E1006 `unknown-attribute` {#e1006}

**Error.** An attribute is not one of the attributes the compiler knows.

```kis
#[foo]
fn attribute() {}
```

Fix: remove the attribute or use a known one such as `#[scene]`.

## E1007 `attribute-arguments` {#e1007}

**Error.** A known attribute is written with arguments, but Kagari attributes take none.

```kis
#[scene(main)]
pushdown fn Scene() {}
```

Fix: drop the arguments: `#[scene]`.

## E1008 `builtin-outside-std` {#e1008}

**Error.** `#[builtin]` is used outside the standard library; only `std` may declare compiler-provided items.

```kis
#[builtin]
fn x();
```

Fix: remove `#[builtin]` and give the function a body: `fn x() {}`.

## E1009 `builtin-with-body` {#e1009}

**Error.** A `#[builtin]` function has a body, or a `#[builtin]` struct declares fields; built-in items are implemented by the compiler and must be bodiless. Outside the standard library `E1008` is reported as well.

```kis
#[builtin]
fn len(n: u64) -> u64 {
    0
}
```

Fix: remove the body: `fn len(n: u64) -> u64;`.

## E1010 `missing-body` {#e1010}

**Error.** A function that is not `#[builtin]` is declared without a body.

```kis
fn y();
```

Fix: add a body: `fn y() {}`.

## E1011 `missing-value` {#e1011}

**Error.** A `const` has no value, or a `type` alias has no type.

```kis
const LIMIT: u64;
```

Fix: supply the value: `const LIMIT: u64 = 10;`.

## E1012 `trait-outside-std` {#e1012}

**Error.** A trait is declared outside the standard library; user code cannot declare traits.

```kis
trait Shape {
    fn sides(&self) -> u64;
}
```

Fix: remove the trait and use inherent `impl` methods on each type instead.

## E1013 `trait-not-builtin` {#e1013}

**Error.** A trait declared in the standard library is missing `#[builtin]`; every standard library trait must be compiler-provided.

```kis
// in the standard library's `std/mod.kis`
pub trait Eq {}
```

This snippet is illustrative: the standard library ships inside the compiler, and user code gets `E1012` for any trait, so only a modified standard library reaches this code.

Fix: mark it built-in: `#[builtin] pub trait Eq {}`.

## E1014 `duplicate-definition` {#e1014}

**Error.** The same name is defined more than once in one namespace of a module; a note points to the first definition.

```kis
fn a() {}
fn a() {}
```

Fix: rename or remove one of the definitions.

## E1015 `unresolved-import` {#e1015}

**Error.** A `use` path does not lead to any item.

```kis
use nowhere::x;
```

Fix: correct the path to an existing module and item, or remove the import.

## E1016 `unresolved-name` {#e1016}

**Error.** A value, type, path segment or associated item cannot be found in scope.

```kis
fn unresolved() {
    let _ = nothing;
    let _value: Nothing = 1;
}
```

Fix: fix the spelling, import the item with `use`, or define it.

## E1017 `private-item` {#e1017}

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

## E1018 `ambiguous-name` {#e1018}

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

## E1019 `undeclared-label` {#e1019}

**Error.** `break` or `continue` refers to a loop label that is not declared on any enclosing loop.

```kis
fn labels() {
    loop {
        break 'nope;
    }
}
```

Fix: label the loop (`'nope: loop { ... }`) or drop the label: `break;`.

## E1020 `outside-loop` {#e1020}

**Error.** `break` or `continue` appears outside of any loop.

```kis
fn labels() {
    break;
    continue;
}
```

Fix: move the statement inside a `loop`, `while` or `for`, or use `return`.

## E1021 `self-outside-impl` {#e1021}

**Error.** The `Self` type is used outside of an `impl` or trait.

```kis
fn self_outside() -> Self {}
```

Fix: write the concrete type instead of `Self`, or move the function into an `impl`.

## E1022 `bad-impl-target` {#e1022}

**Error.** An inherent `impl` targets a type it may not extend, such as a primitive type outside the standard library.

```kis
impl bool {}
```

Fix: wrap the primitive in your own struct and implement methods on that.

## E1023 `trait-impl-outside-std` {#e1023}

**Error.** User code implements a trait; only the standard library implements traits. For `Eq`, the compiler derives it automatically for structs and enums whose fields are all `Eq`.

```kis
struct Square {
    side: u64,
}

impl Eq for Square {}
```

Fix: remove the trait impl; use an inherent `impl Square { ... }` for methods.

## E1024 `duplicate-binding` {#e1024}

**Error.** The same identifier is bound more than once within a single pattern.

```kis
fn patterns(p: (u64, u64)) {
    let (x, x) = p;
}
```

Fix: give each binding a distinct name: `let (x, y) = p;`.

## E1025 `inconsistent-binding` {#e1025}

**Error.** In an or-pattern, a variable is bound in some alternatives but not in all of them.

```kis
fn patterns(o: Option<u64>) {
    match o {
        Some(a) | None => {}
    }
}
```

Fix: split the alternatives into separate arms, or bind the variable in every alternative.

## E1026 `missing-std-item` {#e1026}

**Error.** An item the compiler relies on (such as a lang type or function under `std::`) is missing from the standard library being compiled; this indicates a broken or mismatched standard library rather than a user error.

```kis
// in the standard library's `std/mod.kis`, with this line deleted:
// #[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
```

This snippet is illustrative: the standard library ships inside the compiler, so only a modified standard library reaches this code.

Fix: restore the missing `std::` item, or use the standard library that ships with the compiler.

## E1027 `asset-not-found` {#e1027}

**Error.** An `asset("...")` call names a file that does not exist in the project's assets.

```kis
const MISSING: ImageAsset = asset("missing.png");
```

Fix: add the file or correct the path, e.g. `asset("cg_001.png")`.

## E1028 `empty-asset-path` {#e1028}

**Error.** An `asset("...")` or `system_font("...")` call is given an empty path.

```kis
const EMPTY: ImageAsset = asset("");
```

Fix: pass a non-empty asset path, e.g. `asset("cg_001.png")`.

## E1029 `asset-outside-asset-dir` {#e1029}

**Error.** An `asset("...")` path leaves the asset directory (`[assets] dir` in `kagari.toml`, by default `assets` next to the manifest), or a `system_font("...")` path is absolute or leaves the system font directory it is joined to. Asset paths are checked only inside a project: `kagari check` and `kagari build` need a `kagari.toml`, and the language server reports only syntax errors for a file outside every project.

```kis
const OUTSIDE: ImageAsset = asset("../secret.png");
```

Fix: move the file into the asset directory and name it relative to that directory, e.g. `asset("bgimage/room.png")`.
For a system font, name the file relative to a system font directory, e.g. `system_font("msgothic.ttc")` rather than `system_font("C:/Windows/Fonts/msgothic.ttc")`.

## E1030 `system-font-not-found` {#e1030}

**Error.** A `system_font("...")` call names a file that is in none of the system font directories. The message lists the directories searched, in order: on Windows `%LOCALAPPDATA%\Microsoft\Windows\Fonts` and `%WINDIR%\Fonts`; on macOS `~/Library/Fonts`, `/Library/Fonts` and `/System/Library/Fonts`; elsewhere `$XDG_DATA_HOME/fonts` (by default `~/.local/share/fonts`), `~/.fonts` and `fonts` in each of `$XDG_DATA_DIRS` (by default `/usr/local/share:/usr/share`). Directories that do not exist are skipped.

```kis
const MINCHO: FontAsset = system_font("missing-mincho.ttc");
```

Fix: install the font, correct the path relative to its font directory (e.g. `system_font("truetype/dejavu/DejaVuSans.ttf")` for `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`), or copy the file into the asset directory and use `asset("…")`.

## E1031 `system-fonts-disabled` {#e1031}

**Error.** A `system_font("...")` call in a project whose `kagari.toml` sets `system-fonts = false` under `[assets]`.

```toml
[assets]
system-fonts = false
```

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

Fix: copy the font into the asset directory and use `asset("fonts/msgothic.ttc")`, or allow system fonts by removing the setting.

## E1032 `reserved-asset-dir` {#e1032}

**Error.** An `asset("...")` path enters the top-level `system-fonts` or `std` directory of the asset directory. These names are where the bytecode and the archives keep system fonts and the assets of the standard library (its shaders), so assets may not use them.

```kis
const GOTHIC: FontAsset = asset("system-fonts/msgothic.ttc");
```

Fix: rename the directory, e.g. `asset("fonts/msgothic.ttc")`.
