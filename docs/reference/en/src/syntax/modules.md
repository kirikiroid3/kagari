# Source Files and Modules

A game is a tree of **modules**, as in Rust. Each module is a source file, an inline `mod { … }`
block, or a block with items in a function body. This chapter covers files, the module tree, paths,
visibility, `use`, the standard library and its prelude, and assets.

## `.kis` and `.kix` {#kis-and-kix}

| Extension | Contains |
| --------- | -------- |
| `.kis` | The core language |
| `.kix` | The core language plus [forms](./forms.md): `s!(…)`, `choice!{…}`, `t![…]` and the UI forms such as `button!{…}` |

Forms are fixed syntax that stands for ordinary expressions, so everything written in a `.kix`
file can also be written, more verbosely, in a `.kis` file. They are not macros: Kagari has no
macros, and the set of forms is fixed by the language. A form in a `.kis` file is a syntax error
([`E0001`](../diagnostics/syntax.md#e0001)).

The extension decides the syntax only. It never decides what code may do: that is the job of
[function kinds](../concepts/kinds.md). Any kind of function may appear in either file. By
convention the story and UI are written in `.kix` files, and code without forms (types, constants,
helpers, renders) in `.kis` files.

## The root file

The compiler starts from one **root file**: `project.entry` in `kagari.toml`, by default
`src/main.kis` (see [Projects](../tools/projects.md)). It must be a `.kis` or `.kix` file
([`E1001`](../diagnostics/modules.md#e1001)). The root file is the module `crate`, and it must
declare `fn main()`, which describes the game system
([`E7001`](../diagnostics/emit.md#e7001)). Every other file is reached through `mod` declarations;
a file that no `mod` declares is not part of the game.

All files of a game are compiled together into one `.kagari` file.

## Modules

```kis
// src/main.kix: the root module
mod characters;      // a file module
mod story;
mod ui;
mod helpers {        // an inline module
    pub const fn twice(x: u32) -> u32 { x * 2 }
}

use characters::{HANAKO, AZUSA};
```

- `mod name;` declares a **file module**. Its file is looked up as described below.
- `mod name { … }` declares an **inline module** with the items between the braces.
- A function body or block that contains items is a nameless **block scope**; see
  [Items in blocks](./items.md#items-in-blocks). A file module cannot be declared in a block
  ([`E1005`](../diagnostics/modules.md#e1005)).
- Items may be used before their declaration.

### Where the file of a module is {#where-the-file-of-a-module-is}

For `mod name;` the compiler looks in the **child directory** of the declaring module:

- in the root file, or in a file named `mod.kis` or `mod.kix`, the child directory is the
  directory of that file;
- in any other file `foo.kis` or `foo.kix`, it is the directory `foo/` next to the file;
- inside an inline module `mod a { … }`, the name of each enclosing inline module is appended:
  `a/`.

In the child directory, these four candidates are tried: `name.kis`, `name.kix`, `name/mod.kis`
and `name/mod.kix`. Exactly one must exist. None is
[`E1002`](../diagnostics/modules.md#e1002), more than one is
[`E1003`](../diagnostics/modules.md#e1003), and a file that is already loaded as another module is
[`E1004`](../diagnostics/modules.md#e1004).

<!-- diagram: modules-tree (mermaid-ish)
flowchart TD
  root["crate: src/main.kix (root)"] ==>|"mod characters;"| ch["crate::characters: src/characters.kis"]
  root ==>|"mod story;"| st["crate::story: src/story.kix"]
  root ==>|"mod ui;"| ui["crate::ui: src/ui/mod.kis"]
  st ==>|"mod day1;"| d1["crate::story::day1: src/story/day1.kix"]
  ui ==>|"mod menu;"| me["crate::ui::menu: src/ui/menu.kis"]
-->
{{#include ../diagrams/modules-tree.svg}}

Here `story.kix` declares `mod day1;`, so its child directory is `src/story/`. `ui/mod.kis` is the
file of `ui` itself, so its child directory is `src/ui/`.

## Paths

A **path** names an item: segments separated by `::`, such as `std::screen::size` or
`Route::Hanako`. A path may start with one of these keywords:

| Start | Refers to |
| ----- | --------- |
| `crate` | the root module of the game |
| `self` | the current module |
| `super` | the parent module; `super::super` goes up two levels. Going above the root is [`E1016`](../diagnostics/modules.md#e1016) |
| `Self` | the type of the current `impl` (see [Methods](./functions.md#methods-and-associated-functions)) |

A path cannot start with `::`. Generic arguments are written in types as `Vec<u8>` and in
expressions with the turbofish `Vec::<u8>::new()`.

The first segment of a plain path is looked up in this order:

1. the enclosing block scopes, innermost first;
2. the items and imports of the current module;
3. `std`, the standard library, unless the module defines or imports its own `std`;
4. the [prelude](#the-prelude);
5. the primitive types `bool`, `Int` and the Rust integer types.

The following segments are looked up in the module, enum or type that the path names so far:
items of a module, variants of an enum, and associated functions and constants of a type. A name
that is not found is [`E1016`](../diagnostics/modules.md#e1016).

### Namespaces

Each module has two namespaces:

| Namespace | Holds |
| --------- | ----- |
| types | modules, structs, enums, type aliases, traits |
| values | functions, constants, statics, and the constructors of tuple and unit structs |

A name may be used once in each namespace. A second definition in the same namespace is
[`E1014`](../diagnostics/modules.md#e1014). Because the namespaces are separate, a constant and a
module may share a name: `AZUSA` is an entity id and `AZUSA::CLOSE_EYE_SMILE` one of her images.

```kis
pub const AZUSA: EntityId = EntityId(217);

pub mod AZUSA {
    pub const CLOSE_EYE_SMILE: ImageAsset = asset("azusa/close_eye_smile.png");
}
```

## Visibility

An item is **private** unless it is marked `pub`. A private item is visible in the module that
defines it and in all modules nested in that module. Naming a private item from anywhere else is
[`E1017`](../diagnostics/modules.md#e1017). There are no restricted forms such as `pub(crate)`.

- Associated functions and constants of an `impl` follow the same rule: they are private unless
  marked `pub`.
- Enum variants are as visible as their enum.
- Struct fields may be marked `pub`, as in the standard library. The compiler does not check field
  visibility yet: every field can be read and written wherever the struct is visible.

## `use` declarations {#use-declarations}

`use` brings names into the current module or block:

| Form | Effect |
| ---- | ------ |
| `use std::screen::size;` | imports `size` |
| `use characters::{HANAKO, AZUSA};` | imports several names; groups may nest |
| `use crate::widen as w;` | imports under another name |
| `use crate::widen as _;` | imports without binding a name |
| `use Route::*;` | glob import: every visible name of a module, or every variant of an enum |
| `pub use self::text::Text;` | re-exports: the name becomes part of this module for others |

- An import takes the item from every namespace that has the name: `use characters::AZUSA;`
  imports both the constant and the module.
- Use paths have no generic arguments.
- A name defined in the module, or imported by name, takes precedence over a glob import.
- When two glob imports supply different items for one name, using that name is
  [`E1018`](../diagnostics/modules.md#e1018).
- An import that leads nowhere is [`E1015`](../diagnostics/modules.md#e1015). Importing a private
  item is [`E1017`](../diagnostics/modules.md#e1017).

## The standard library

The standard library is the module `std`. Its items are described in the
[STD Reference](../../../../std/en/src/std.md):

| Module | Contents | STD Reference |
| ------ | -------- | ------------- |
| `std` | `String`, `Vec`, `Range`, `Rat`, `Decimal`, `Option`, `Result`; the traits `Eq`, `Integer`, `RandChoose` and `Asset`; the asset types, `asset` and `system_font` | [std](../../../../std/en/src/std.md) |
| `std::prelude` | the names imported into every module | [std::prelude](../../../../std/en/src/prelude.md) |
| `std::time` | `Duration` (whole milliseconds), `ms`, `secs` | [std::time](../../../../std/en/src/time.md) |
| `std::narrative` | steps (`Step`, `push_step`, `Say`, `Choice`), `rng_choose`, meta fields, the `Narrative` handle | [std::narrative](../../../../std/en/src/narrative.md) |
| `std::narrative::text` | `Text`, text effects, `ToText`, text styles | [std::narrative::text](../../../../std/en/src/narrative-text.md) |
| `std::screen` | the screen: entities, properties, layers, Hud, camera, filters | [std::screen](../../../../std/en/src/screen.md) |
| `std::screen::anim` | tweens, transitions, motions, easings | [std::screen::anim](../../../../std/en/src/screen-anim.md) |
| `std::screen::audio` | music and ambience channels, sounds, audio fades | [std::screen::audio](../../../../std/en/src/screen-audio.md) |
| `std::stage` | characters, slots and side faces on top of `std::screen` | [std::stage](../../../../std/en/src/stage.md) |
| `std::system` | `System`, settings, seen sets, bindings | [std::system](../../../../std/en/src/system.md) |
| `std::system::nav` | navigation between modes, starting and loading the story | [std::system::nav](../../../../std/en/src/system-nav.md) |
| `std::system::saves` | save slots and save information | [std::system::saves](../../../../std/en/src/system-saves.md) |
| `std::ui` | UI elements, signals, the functions behind the UI forms | [std::ui](../../../../std/en/src/ui.md) |
| `std::ui::style` | the looks of UI controls | [std::ui::style](../../../../std/en/src/ui-style.md) |

## The prelude

The **prelude** is imported into every module. A module's own items and imports take precedence
over it. It contains:

| From | Names |
| ---- | ----- |
| `std` | `String`, `Vec`, `Rat`, `Decimal`, `Range`, `Option`, `Result`, `Eq`, `Integer`, `RandChoose`, `ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`, `ShaderAsset`, `asset`, `system_font`; the variants `None`, `Some`, `Ok`, `Err` |
| `std::time` | `Duration`, `ms`, `secs` |
| `std::screen` | `Screen`, `Render`, `EntityId`, `Part`, `Point`, `Offset`, `Size`, `Anchor`, `Color`, `Plane`, `Visibility`, `point`, `offset`, `rgb`, `hex`, `BLACK`, `WHITE`, `TRANSPARENT` |
| `std::screen::anim` | `Easing`, `Tween`, `Transition`, `Motion`, `tween`, `instant`, `cut`, `fade`, `rule`, `wipe`, `through`, `motion`, `jump`, `nod`, `shake`, `quake` |
| `std::screen::audio` | `looped`, `sound`, `crossfade` |
| `std::stage` | `Character`, `character` |
| `std::narrative` | `Text`, `TextEffect`, `Say`, `Choice`, `ChoiceArm`, `push_step`, `rng_choose`, `Meta`, `meta`, `Narrative` |
| `std::narrative::text` | `emphasis`, `dots`, `color_text`, `bold`, `pause`, `click_wait`, `mark` |
| `std::system` | `nav`, `Setting`, `setting`, `System`, `Settings`, `Volume`, `volume`, `RootPage`, `Overlay`, `GameOverlay`, `Dialog` |
| `std::ui` | `signal`, `Element` |

Everything else is imported with `use`, for example `use std::screen::size;`.

## Assets

An **asset** is a file of the game, bound to a constant:

```kis
const CG_001: ImageAsset = asset("cg/cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
```

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

There are six asset types, the only types that implement the trait `Asset`:

| Type | File |
| ---- | ---- |
| `ImageAsset` | an image: PNG, JPEG or WebP |
| `AudioAsset` | music or a sound |
| `VoiceAsset` | a voice file, bound to a line |
| `VideoAsset` | a movie |
| `FontAsset` | a font |
| `ShaderAsset` | a WGSL shader (`.wgsl`): a filter, or generated content such as weather or particles |

The type of the constant chooses `A`; `asset::<ImageAsset>("…")` names it explicitly. The path
follows these rules:

- It is a string literal ([`E2028`](../diagnostics/types.md#e2028)) and not empty
  ([`E1028`](../diagnostics/modules.md#e1028)).
- It is relative to the **asset directory**: `[assets] dir` in `kagari.toml`, by default `assets`
  in the project directory. Both `/` and `\` separate directories.
- It stays inside the asset directory: no absolute path, no drive (`C:`), and no `..` that
  leaves it ([`E1029`](../diagnostics/modules.md#e1029)).
- It does not enter the top-level directory `system-fonts`, which is reserved for system fonts
  ([`E1032`](../diagnostics/modules.md#e1032)).
- The file exists when the game is compiled
  ([`E1027`](../diagnostics/modules.md#e1027)), so a missing file is a compile error, not a
  failure during play.

An asset type is a [finite type](../concepts/kinds.md#finite-types): its values are exactly the
asset constants of the program. The story may therefore choose between assets at runtime, for
example a voice line by route. Two asset values are equal when they name the same file.

### System fonts

A font installed on the building machine can be bound without copying it into the asset
directory:

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

The path is a non-empty string literal relative to a system font directory and must stay inside
it. The directories are searched in this order, and the first one that has the file wins:

| Platform | Directories |
| -------- | ----------- |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts` |
| Linux and others | `$XDG_DATA_HOME/fonts` (default `~/.local/share/fonts`), `~/.fonts`, then `fonts` in each directory of `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`) |

A font that is not found is [`E1030`](../diagnostics/modules.md#e1030).
`[assets] system-fonts = false` in `kagari.toml` forbids system fonts
([`E1031`](../diagnostics/modules.md#e1031)). The standard library ships no fonts, and
`System::font`, the default font, is required.

### Assets in the build {#assets-in-the-build}

`kagari build` packs every asset file the program uses into archives (`.kiarchive`) next to the
`.kagari` file. Each file is stored under its path relative to the asset directory, with `/`
separators, and goes into the archive named after the first directory of that path:

| Asset path | Archive |
| ---------- | ------- |
| `bg/classroom.png` | `bg.kiarchive` |
| `cg/cg_001.png` | `cg.kiarchive` |
| `logo.png` (directly in the asset directory) | `data.kiarchive` |
| a system font `msgothic.ttc` | `system-fonts.kiarchive`, as `system-fonts/msgothic.ttc` |

Grouping files by top-level directory therefore decides how the game's data is split into
archives. See [The `kagari` CLI](../tools/cli.md) for the build output.
