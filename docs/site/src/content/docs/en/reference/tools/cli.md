---
title: "The kagari CLI"
---

`kagari` is the Kagari toolchain: it creates, checks, builds and formats projects, disassembles
`.kagari` files, and runs the language server.

```text
kagari <COMMAND>

Commands:
  init    Create a new project in PATH
  check   Check the project and print its diagnostics
  build   Build the project's bytecode and asset archives
  fmt     Format the project's source files, or the given files
  disasm  Disassemble a `.kagari` bytecode file
  lsp     Run the language server over stdio
  help    Print this message or the help of the given subcommand(s)
```

`kagari --version` prints the toolchain version, which `[toolchain] kagari` in `kagari.toml` must
match. `kagari help <COMMAND>` or `kagari <COMMAND> --help` prints the help of one command.

## Installing

Build the tool from the Kagari repository:

```sh
cargo install --path tooling/kagari-cli
```

This installs the `kagari` executable into Cargo's `bin` directory. Inside the repository you can
also run it without installing: `cargo run -p kagari-cli -- <COMMAND>`.

## Finding the project

`check`, `build` and `fmt` (without files) work on a project. They use the `kagari.toml` given by
`--manifest-path <PATH>`, or else the first `kagari.toml` found in the current directory or one of
its parents. See [Projects](./projects.md) for the manifest.

## Exit statuses

All commands use the same exit statuses:

| Status | Meaning |
| -----: | ------- |
| 0 | Success. |
| 1 | The command ran and found problems: an error diagnostic, an unsupported asset format, a file that is not formatted, an invalid `.kagari` file. |
| 2 | The command could not run: a usage error, an IO error, no `kagari.toml` found, an invalid manifest, a toolchain version mismatch. The message is printed to stderr as `kagari: <message>`. |

## Compiler options

The compiler has no command-line flags. Its options are keys in `kagari.toml`, so `check`,
`build` and the language server always agree:

| Key | Default | Effect |
| --- | ------: | ------ |
| `[build] state-cap` | 200 000 | The most story states the analysis explores (`W5006` past it). |
| `[build] recursion-depth` | 2048 | The most nested calls at run time. |
| `[build] keyframe-check` | `false` | Runs the keyframe-placement check (`W5011`). |
| `[assets] system-fonts` | `true` | Lets `system_font("…")` name installed fonts (`E1031` when off). |

See [Compiler options](./projects.md#compiler-options) for details.

## `kagari init`

```text
kagari init [PATH] [--name <NAME>]
```

Creates a new project in `PATH` (default: the current directory). The directory is created when
missing.

| Option | Meaning |
| ------ | ------- |
| `PATH` | The project directory. Default `.`. |
| `--name <NAME>` | The project name. Default: the directory name. |

`kagari init` only creates new files: when any template file already exists in `PATH`, it stops
with status 2 and changes nothing. It writes the layout shown in
[Project layout](./projects.md#project-layout), with `[toolchain] kagari` set to the running
toolchain's `major.minor`.

The template's `src/main.kis` declares the game's default font (`FONT`). `kagari init` picks an
installed font from the [system font directories](./projects.md#assets) and writes
`system_font("…")`. It prefers Japanese gothic faces (Yu Gothic, Meiryo, MS Gothic, Hiragino
Sans, Noto Sans CJK/JP), then common sans-serif faces (Segoe UI, Arial, Helvetica, Noto Sans,
DejaVu Sans, Liberation Sans), and else takes the first font file it finds. With no font
installed, it writes `asset("fonts/default.ttf")` and warns that you must put a font at
`assets/fonts/default.ttf` or change `FONT`.

```sh
kagari init my-game
kagari init --name "My Game"
```

## `kagari check`

```text
kagari check [--manifest-path <PATH>] [--resolutions]
```

Compiles the project without writing anything and prints every diagnostic to stdout.

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`. |
| `--resolutions` | Also print what every name in the program resolves to. Useful for debugging imports. |

Exits with 1 when there is at least one error diagnostic. Warnings alone exit with 0. Diagnostic
codes are listed in [Diagnostics](../../diagnostics/index.md).

## `kagari build`

```text
kagari build [--manifest-path <PATH>] [--target <TARGET>]...
```

Compiles the project and writes, for each target, `<out-dir>/<target>/game.kagari` and the asset
archives. See [Targets and output directories](./projects.md#targets-and-output-directories) and
[Assets and archives](./projects.md#assets-and-archives).

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`. |
| `--target <TARGET>` | Build only this target. Repeatable. Each must be listed in `build.targets`, else status 2. Default: every target of `build.targets`. |

The steps, in order:

1. Compile. Diagnostics are printed to stderr. On an error diagnostic, stop with status 1.
2. Check every referenced asset against `build.runtime-compatibility`
   ([Runtime profiles](./projects.md#runtime-profiles)). On a problem, stop with status 1. Nothing
   has been written or deleted yet.
3. For each target: create the output directory, delete the old `game.kagari` and `.kiarchive`
   files there, write `game.kagari`, then the archives. Each written file is printed:

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
```

```sh
kagari build
kagari build --manifest-path example/test_game1/kagari.toml
kagari build --target v0
```

## `kagari fmt`

```text
kagari fmt [--check] [--manifest-path <PATH>] [FILES]...
```

Formats source files in place. Without `FILES`, it formats every `.kis` and `.kix` file in the
directory of the project's root file and below it (normally `src/`), skipping directories whose
name starts with `.`. With `FILES`, it formats exactly those files and needs no project.

| Option | Meaning |
| ------ | ------- |
| `FILES` | Files to format instead of the project's. Each must end in `.kis` or `.kix` (else status 2). |
| `--check` | Do not write; print `would reformat <file>` for each file that would change. |
| `--manifest-path <PATH>` | The project's `kagari.toml` (ignored when `FILES` are given). |

For each changed file it prints `formatted <file>`. A file with syntax errors is left alone and
reported as `<file>: cannot format: N syntax error(s)`, followed by the position and message of
the first error. Exits with 1 when `--check` found a file to reformat or a file could not be
formatted. The formatting style is described in [Editor Support](./editors.md#the-formatter).

```sh
kagari fmt
kagari fmt --check
kagari fmt src/scenario/mod.kix
```

## `kagari disasm`

```text
kagari disasm <FILE> [--all]
```

Decodes and verifies a `.kagari` file and prints its functions as text. Useful to see what the
compiler produced, or to inspect a file you received.

| Option | Meaning |
| ------ | ------- |
| `FILE` | The `.kagari` file. |
| `--all` | Also print the std functions. Default: only the game's own functions (paths starting with `crate::`). |

When the file is not a valid `.kagari` file, the reason is printed to stderr and the status is 1.
The text format is not stable.

```sh
kagari disasm target/v0/game.kagari
```

## `kagari lsp`

```text
kagari lsp
```

Runs the language server, speaking the Language Server Protocol over stdin and stdout. Editors
start it; you do not run it by hand. See [Editor Support](./editors.md#the-language-server).
