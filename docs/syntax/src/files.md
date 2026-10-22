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

The standard library is the module `std`:

| Module          | Contents |
| --------------- | -------- |
| `std::screen`   | The screen (an ECS world), components, animations; see [Presentation](./presentation.md) |
| `std::narrative`| The `push_step` primitive, `Say`, `Choice`, `Text`, `TextEffect`, `Meta`; the `Narrative` interface for the system |
| `std::ui`       | UI forms, signals; see [The Game System](./system.md) |
| `std::system`   | Navigation, saves, settings; see [The Game System](./system.md) |

A **prelude** is imported into every module automatically. It contains the types and functions
the examples of this book use without a `use`: `Screen`, `Render`, `Text`, `Int`, `Option`,
`jump`, `fade`, `instant_switch` and the asset types.

## Assets

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
```

*Tentative.* `asset` is a built-in `const fn` that binds a file to a constant. The constant's type
must be `ImageAsset`, `AudioAsset` or `VoiceAsset`, and the path, relative to the declaring file,
must not be empty. The file is checked to exist at compile time.

An asset type is a [finite type](./kinds.md#finite-types): its values are exactly the asset
constants of the program. So the story may choose between assets at runtime, for example a voice
line by route.
