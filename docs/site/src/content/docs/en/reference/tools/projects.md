---
title: "Projects"
---

A Kagari game is a **project**: a directory with a `kagari.toml` manifest, the source files, and
the assets. `kagari build` compiles it into one `.kagari` file plus `.kiarchive` asset archives,
which the [runtime](./runtime.md) runs.

## Project layout

`kagari init` creates this layout (see [`kagari init`](./cli.md#kagari-init)):

```text
my-game/
├── kagari.toml                 the manifest
├── .gitignore                  ignores /target
├── src/
│   ├── main.kis                the root file (`project.entry`)
│   ├── scenario/mod.kix        the story
│   ├── scripts/mod.kis
│   ├── scripts/characters.kis
│   ├── system/mod.kix          the game system
│   └── effect/mod.kis
├── assets/                     `asset("…")` paths are relative to this directory
│   ├── bgimage/
│   ├── sound/
│   ├── music/
│   └── characters/character1/
│       ├── image/normal.png
│       └── voice/0001.wav
└── target/                     build output (`build.out-dir`)
    ├── analysis.kagari-cache   the analysis cache
    └── v0/
        ├── game.kagari
        ├── *.kiarchive
        └── game.kagari-cache   debug builds only
```

Only `kagari.toml` and the root file are required. The module tree grows from the root file; see
[Source Files and Modules](../syntax/modules.md). The directory names below `src/` and `assets/`
are conventions of the template, not rules.

`example/test_game1` in the Kagari repository is a complete project with the same shape.

## `kagari.toml`

The manifest is TOML with four tables. A complete example with every key:

```toml
[project]
name = "my-game"          # required
version = "0.1.0"         # required, semver
entry = "src/main.kis"    # the root file; default "src/main.kis"

[toolchain]
kagari = "0.1"            # required; the toolchain versions the project accepts

[assets]
dir = "assets"            # the asset directory; default "assets"
system-fonts = true       # whether `system_font("…")` may name installed fonts; default true

[build]
targets = ["v0"]          # engine major versions to build for; default ["v0"]
runtime-compatibility = ["native_v0"]  # runtime profiles whose decoders the assets must suit; default ["native_v0"]
out-dir = "target"        # default "target"
state-cap = 200000
recursion-depth = 2048
keyframe-check = false
```

General rules:

- Unknown tables and keys are errors.
- Every path is relative to the directory that holds `kagari.toml`. Absolute paths are errors.
- The `kagari` tool looks for `kagari.toml` in the current directory and then in each parent
  directory, unless `--manifest-path` names it.

### `[project]` {#project}

| Key | Type | Default | Meaning |
| --- | ---- | ------- | ------- |
| `name` | string | required | The project name. Must not be empty. |
| `version` | string | required | The project version, a [semver](https://semver.org) version such as `"0.1.0"`. |
| `entry` | string (path) | `"src/main.kis"` | The root source file. Must end in `.kis` or `.kix`. |

### `[toolchain]` {#toolchain}

| Key | Type | Default | Meaning |
| --- | ---- | ------- | ------- |
| `kagari` | string | required | A semver requirement (`"0.1"`, `">=0.1, <0.3"`) the running `kagari` version must match. |

When the requirement does not match, every command that loads the project stops with
`this project requires kagari <req>, but this is kagari <version>` and exit status 2.
`kagari init` writes the `major.minor` of the toolchain that created the project.

### `[assets]` {#assets}

| Key | Type | Default | Meaning |
| --- | ---- | ------- | ------- |
| `dir` | string (path) | `"assets"` | The asset directory. `asset("…")` paths are relative to it. |
| `system-fonts` | bool | `true` | Whether `system_font("…")` may name fonts installed on the build machine. When `false`, every `system_font` call is `E1031`. |

The directories `system-fonts/` and `std/` directly inside the asset directory are reserved
(`E1032`): system fonts and the std library's own assets, such as its shaders, are stored in the
build output under those names.

`system_font("…")` searches these directories, in order, on the machine that builds the game:

| Platform | Directories |
| -------- | ----------- |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts` |
| Linux and others | `$XDG_DATA_HOME/fonts` (default `~/.local/share/fonts`), `~/.fonts`, then `fonts` in each of `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`) |

The font file is copied into the build output, so players do not need it installed. Check the
font's license before you ship it.

### `[build]` {#build}

| Key | Type | Default | Meaning |
| --- | ---- | ------- | ------- |
| `targets` | array of strings | `["v0"]` | The targets `kagari build` builds. Supported: `v0`. Must not be empty; duplicates are ignored. |
| `runtime-compatibility` | array of strings | `["native_v0"]` | The [runtime profiles](#runtime-profiles) whose decoders every asset must suit. Supported: `native_v0`. Must not be empty; duplicates are ignored. |
| `out-dir` | string (path) | `"target"` | The output directory. |
| `state-cap` | integer | `200000` | The [compiler option](#compiler-options) `state-cap`. |
| `recursion-depth` | integer | `2048` | The compiler option `recursion-depth`. |
| `keyframe-check` | bool | `false` | The compiler option `keyframe-check`. |

## Compiler options

The compiler options are the four manifest keys that change what the compiler checks or emits.
`kagari check`, `kagari build` and the language server all use them.

| Option | Key | Default | Meaning |
| ------ | --- | ------: | ------- |
| State cap | `[build] state-cap` | 200 000 | The most story states the analysis explores. Past it the analysis stops and reports `W5006`. |
| Recursion depth | `[build] recursion-depth` | 2048 | The most nested calls at run time, counting every call frame (`fn`, closures, scenes, instant procedures). A deeper call stops the game with an error. Written into the `.kagari` file. |
| Keyframe check | `[build] keyframe-check` | off | Runs the keyframe-placement check (`W5011`): every step a player can land on directly (each choice and the first steps of each entry scene) should have a keyframe render. See [Presentation](../concepts/presentation.md). |
| System fonts | `[assets] system-fonts` | on | Lets `system_font("…")` name installed fonts (`E1031` when off). |

See [Story Analysis (E5xxx)](../../diagnostics/analysis.md) for `W5006` and `W5011`, and
[Modules, Items and Names (E1xxx)](../../diagnostics/modules.md) for `E1031`.

## Targets and output directories

A **target** is the engine major version the `.kagari` file is built for. The only target is
`v0`. `kagari build` writes each target into its own directory, `<out-dir>/<target>`:

```text
target/v0/
├── game.kagari
├── bgimage.kiarchive
├── bgm.kiarchive
├── fgimage.kiarchive
├── image.kiarchive
└── system-fonts.kiarchive
```

(This is the output of `example/test_game1`.) Before writing, `kagari build` deletes the
`game.kagari`, every `.kiarchive` and every `.kagari-cache` an earlier build left in that
directory. It leaves other files alone. A [debug build](./cli.md#build-profiles)
(`kagari build --profile debug`) also writes `game.kagari-cache` there, the debug artifact.

Directly in the output directory, `kagari check` and `kagari build` keep the
[analysis cache](./cli.md#the-analysis-cache), `analysis.kagari-cache`.
[`kagari clean`](./cli.md#kagari-clean) deletes the analysis cache and the build outputs of every
target. The `.gitignore` of a new project ignores `/target`, the default output directory.

## Assets and archives

`kagari build` packs only the assets the program references (through `asset("…")` and
`system_font("…")`), not the whole asset directory. They go into `.kiarchive` files next to
`game.kagari`:

| Asset path | Archive |
| ---------- | ------- |
| `characters/character1/image/normal.png` | `characters.kiarchive` (one archive per top-level directory) |
| `title.png` (directly in the asset directory) | `data.kiarchive` |
| a system font, e.g. `system-fonts/msgothic.ttc` | `system-fonts.kiarchive` |
| a std asset, e.g. `std/shaders/color/blur.wgsl` (a shader of `std::shaders`) | `std.kiarchive` |

Inside an archive, an entry's name is its full asset path. The runtime opens each archive the
first time the game needs one of its assets. A missing archive is not an error at start: the
runtime logs a warning and the asset is missing when used.

The runtime also reads numbered patch archives next to a base archive (`bgimage.001.kiarchive`,
`bgimage.002.kiarchive`, …, up to the first missing number). Each patch adds, replaces or deletes
entries of the archives before it. `kagari build` writes only base archives.

### Runtime profiles

A **runtime profile** names the set of decoders a runtime has. Before it writes anything,
`kagari build` checks every referenced asset against every profile in
`build.runtime-compatibility`. Each asset is identified by its content, not its file extension.
The only profile is `native_v0`, the [desktop runtime](./runtime.md):

| Asset kind | Formats `native_v0` decodes |
| ---------- | --------------------------- |
| Image | PNG (APNG shows its default image), baseline and progressive JPEG, lossy, lossless and extended (alpha or metadata) WebP, BMP (uncompressed, RLE or bitfields), still JPEG XL (RGB or greyscale) |
| Audio, voice | Ogg Vorbis, FLAC, WAV (PCM or IEEE float), MP3 |
| Font | TrueType, OpenType (CFF), TrueType/OpenType collections |
| Shader | WGSL source (UTF-8 text) |
| Video | none |

A WAV whose RIFF or `data` chunk size runs past the end of the file, such as the `0xFFFFFFFF`
placeholder that streaming encoders (ffmpeg writing to a pipe, TTS services) leave, plays to the
end of the file.

Everything else is rejected: animated WebP, JPEG with arithmetic, lossless, hierarchical or
12-bit coding, GIF, BMP with embedded JPEG or PNG or CMYK pixels, AVIF, animated or CMYK JPEG XL,
Ogg Opus, Ogg FLAC, Ogg with another codec, compressed WAV, AAC/M4A, WOFF and WOFF2. Video assets
are rejected whatever their content. Each problem prints one line per asset and profile:

```text
error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
```

### Shader check

After the profile check, `kagari build` compiles every referenced shader, the std library's
included, against the role its constant declares (a `ColorShader<…>` is checked as a colour
shader, and so on). A file used under two roles is checked once per role. Each invalid shader
prints one line, with the line number in the `.wgsl` file when the error lies in it:

```text
error: shader `shader/broken.wgsl` (color): line 4: no definition in scope for identifier: `undefined_name`
```

The roles are `color`, `mix`, `texture`, `mask`, `local motion` and `transitional motion`.

The build then exits with status 1 without creating the output directory or removing earlier
outputs. See also [Runtime Profiles](../../std/appendix/profiles.md) in the STD
Reference.

## Shipping a game

A shipped game is one directory with the runtime executable and the contents of
`<out-dir>/<target>`:

```text
MyGame/
├── kagari-desktop-runtime      (kagari-desktop-runtime.exe on Windows; you may rename it)
├── game.kagari
└── *.kiarchive
```

Started without a directory argument, the runtime looks for `game.kagari` in the directory of its
own executable. Settings and saves are not written there; they go to the per-user data directory
(see [Data directory](./runtime.md#data-directory)). Players need the system libraries listed in
[System requirements](./runtime.md#system-requirements).
