# Source Files and Modules

## `.kis` and `.kix`

A `.kix` file is a `.kis` file that may also contain **forms**: `s!(…)`, `choice!{…}`, `t![…]`
and the UI forms such as `button!{…}`. Forms are fixed syntax that desugars to ordinary
expressions (see [Steps and Forms](./narrative/forms.md) and [The Game System](./system.md)), so
everything written in a `.kix` file can also be written, more verbosely, in a `.kis` file. They
are not macros: there are no macros, and the set of forms is fixed by the language.

The extension decides the syntax only. It never decides what code may do: that is the job of
[function kinds](./kinds.md). Any kind of function may appear in either file. By convention the
story is written in `.kix` files, and code without forms (types, constants, helpers, renders) in
`.kis` files.

## Modules

A game is a tree of modules, as in Rust.

```kis
// main.kix — the root module
mod characters;      // characters.kis or characters.kix
mod story;           // story.kis, story.kix, or story/mod.kis, story/mod.kix
mod ui;

use characters::{HANAKO, AZUSA};
```

- The compiler is given one **root file**, `main.kix` by convention. Every other file is reached
  through `mod` declarations.
- `mod foo;` loads `foo.kis` or `foo.kix` next to the declaring file, or `foo/mod.kis` or
  `foo/mod.kix`. More than one candidate existing is an error.
- `mod foo { … }` declares an inline module.
- Items are private to their module unless marked `pub`. `use` brings paths into scope; `::`
  separates path segments; `self`, `super` and `crate` work as in Rust.
- Constants live in the value namespace and modules in the type namespace, so a constant and a
  module may share a name: `AZUSA` is an entity id and `AZUSA::CLOSE_EYE_SMILE` one of her images.

## The standard library

The standard library is the module `std`. Its reference is the standard library book; this table
lists the modules and the book chapter of each:

| Module               | Contents | Standard library book |
| -------------------- | -------- | --------------------- |
| `std`                | `String`, `Vec`, `Range`, `Rat`, `Decimal`, `Option`, `Result`, the traits `Eq`, `Integer` and `Asset`, the asset types and `asset` | *Core Types* |
| `std::time`          | `Duration` (whole milliseconds), `ms`, `secs` | *Core Types* |
| `std::screen`        | The screen: entities, properties, layers, Hud, camera, filters; see [Presentation](./presentation.md) | *Screen* |
| `std::screen::anim`  | `Tween`, `Transition`, `Motion`, easings and the motion presets | *Animation* |
| `std::screen::audio` | Music and ambience channels, loops, one-shot sounds, audio fades | *Audio* |
| `std::stage`         | Characters, slots and side faces, written in plain `const fn` on top of `std::screen` | *Stage* |
| `std::narrative`     | The `Step` trait and the `push_step` primitive, `Say`, `Choice`, `Meta`; the `Narrative` handle for the system | *Narrative* |
| `std::narrative::text` | `Text`, `TextEffect` and its constructors, `ToText`, `TextStyle` | *Text* |
| `std::ui`            | UI elements, signals, the targets of the UI forms; `std::ui::style` for looks | *UI* |
| `std::system`        | `System`, `Settings`, `Setting`, bindings, seen sets; see [The Game System](./system.md) | *The Game System* |
| `std::system::nav`   | Navigation between modes, starting and loading the story | *The Game System* |
| `std::system::saves` | Save slots and save information | *The Game System* |

A **prelude** is imported into every module automatically. It contains:

- from `std`: `String`, `Vec`, `Range`, `Rat`, `Decimal`, `Option` with `None` and `Some`,
  `Result` with `Ok` and `Err`, `Eq`, `Integer`, the asset types and `asset`;
- from `std::time`: `Duration`, `ms`, `secs`;
- from `std::screen`: `Screen`, `Render`, `EntityId`, `Part`, `Point`, `Offset`, `Size`, `Anchor`,
  `Color`, `Plane`, `Visibility`, `point`, `offset`, `rgb`, `hex`, `BLACK`, `WHITE`,
  `TRANSPARENT`;
- from `std::screen::anim`: `Easing`, `Tween`, `Transition`, `Motion`, `tween`, `instant`, `cut`,
  `fade`, `rule`, `wipe`, `through`, `motion`, `jump`, `nod`, `shake`, `quake`;
- from `std::screen::audio`: `looped`, `sound`, `crossfade`;
- from `std::stage`: `Character`, `character`;
- from `std::narrative`: `Text`, `TextEffect`, `Say`, `Choice`, `ChoiceArm`, `push_step`, `Meta`,
  `meta`, `Narrative`;
- from `std::narrative::text`: `emphasis`, `dots`, `color_text`, `bold`, `pause`, `click_wait`,
  `mark`;
- from `std::system`: `nav`, `Setting`, `setting`, `System`, `Settings`, `Volume`, `volume`,
  `RootPage`, `Overlay`, `GameOverlay`, `Dialog`;
- from `std::ui`: `signal`, `Element`.

Everything else is imported with `use`, for example `use std::screen::size;`.

## Assets

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
```

`asset` binds a file to a constant:

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

- There are five asset types, the only implementations of the trait `Asset`: `ImageAsset`,
  `AudioAsset`, `VoiceAsset`, `VideoAsset` and `FontAsset`.
- The constant's type chooses `A`. The argument must be a string literal, and the path, relative
  to the declaring file, must not be empty.
- The file is checked to exist at compile time, so a missing file is a compile error, not a
  runtime failure.

An asset type is a [finite type](./kinds.md#finite-types): its values are exactly the asset
constants of the program. So the story may choose between assets at runtime, for example a voice
line by route.
