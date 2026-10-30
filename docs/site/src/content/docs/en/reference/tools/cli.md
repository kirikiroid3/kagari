---
title: "The kagari CLI"
---

`kagari` is the Kagari toolchain: it creates, checks, builds, cleans and formats projects,
disassembles `.kagari` files, runs the language server, and debugs stories.

```text
kagari <COMMAND>

Commands:
  init    Create a new project in PATH
  check   Check the project and print its diagnostics
  build   Build the project's bytecode and asset archives
  clean   Delete the build outputs and the analysis cache
  fmt     Format the project's source files, or the given files
  disasm  Disassemble a `.kagari` bytecode file
  lsp     Run the language server over stdio
  debug   Debug the story: the query engine's REPL on the headless game (by default), a game in a window (`--window`), or a running game (`--attach`)
  dap     Run the debug adapter (Debug Adapter Protocol) over stdio
  help    Print this message or the help of the given subcommand(s)
```

`kagari --version` prints the toolchain version, which `[toolchain] kagari` in `kagari.toml` must
match. `kagari help <COMMAND>` or `kagari <COMMAND> --help` prints the help of one command.

## Installing

The installers in [Installing Kagari](../../tutorial/install.md) put `kagari` and
`kagari-desktop-runtime` on your `PATH` on Linux and Windows; on NixOS, use the flake described
there.

To build both from the Kagari repository instead (needs Rust and Cargo):

```sh
cargo install --path tooling/kagari-cli
cargo install --path runtime/kagari-desktop-runtime
```

This installs them into Cargo's `bin` directory. Inside the repository you can also run `kagari`
without installing: `cargo run -p kagari-cli -- <COMMAND>`.

## Finding the project

`check`, `build`, `clean` and `fmt` (without files) work on a project. They use the `kagari.toml`
given by `--manifest-path <PATH>`, or else the first `kagari.toml` found in the current directory
or one of its parents. See [Projects](./projects.md) for the manifest.

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

## The analysis cache

The story analysis is the slowest part of compiling a project: it explores every story state to
check termination, reachability, stuck choices, screens and their consistency, keyframes and the
game system ([Story Analysis](../../diagnostics/analysis.md),
[Game System](../../diagnostics/system.md)). `kagari check` and `kagari build` keep its result in
`<out-dir>/analysis.kagari-cache` (`target/analysis.kagari-cache` by default) and reuse it while
nothing it depends on has changed:

- the text of every source file of the program, the standard library included;
- the [compiler options](#compiler-options) (`state-cap`, `recursion-depth`, `keyframe-check`,
  `system-fonts`);
- the location of the asset directory and of the system font directories;
- the toolchain version.

When all of these are unchanged since the file was written, the analysis does not run again and
its diagnostics are taken from the file; the output is the same as with a fresh analysis.
Otherwise the analysis runs and the file is rewritten. The analysis, and therefore the file, is
skipped while an earlier check reports an error. So `kagari check` writes into the output directory
too.

A missing, unreadable or invalid file is ignored and replaced. When the file cannot be written,
`kagari: warning: cannot write the analysis cache: <reason>` goes to stderr and the command
continues with the same exit status. [`kagari clean`](#kagari-clean) deletes the file. If you work
on the compiler itself and change it without changing its version, run `kagari clean`, because the
cache cannot tell the two compilers apart.

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

Compiles the project and prints every diagnostic to stdout. It writes no build output, only the
[analysis cache](#the-analysis-cache).

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`. |
| `--resolutions` | Also print what every name in the program resolves to. Useful for debugging imports. |

Exits with 1 when there is at least one error diagnostic. Warnings alone exit with 0. Diagnostic
codes are listed in [Diagnostics](../../diagnostics/index.md).

## `kagari build`

```text
kagari build [--manifest-path <PATH>] [--target <TARGET>]... [--profile <PROFILE>]
```

Compiles the project and writes, for each target, `<out-dir>/<target>/game.kagari` and the asset
archives; with `--profile debug` also `game.kagari-cache`. See
[Targets and output directories](./projects.md#targets-and-output-directories) and
[Assets and archives](./projects.md#assets-and-archives). Like `kagari check`, it reuses and
updates the [analysis cache](#the-analysis-cache).

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`. |
| `--target <TARGET>` | Build only this target. Repeatable. Each must be listed in `build.targets`, else status 2. Default: every target of `build.targets`. |
| `--profile <PROFILE>` | `release` or `debug`; see [Build profiles](#build-profiles). Default `release`. |

The steps, in order:

1. Compile. Diagnostics are printed to stderr. On an error diagnostic, stop with status 1.
2. Check every referenced asset against `build.runtime-compatibility`
   ([Runtime profiles](./projects.md#runtime-profiles)). On a problem, stop with status 1. No
   build output has been written or deleted yet; only the analysis cache may have been updated.
3. For each target: create the output directory, delete the old `game.kagari`, `.kiarchive` and
   `.kagari-cache` files there, write `game.kagari`, then the archives, then (debug profile only)
   `game.kagari-cache`. Each written file is printed:

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
```

```sh
kagari build
kagari build --manifest-path example/test_game1/kagari.toml
kagari build --target v0
kagari build --profile debug
```

### Build profiles

The profile decides what `kagari build` writes besides the bytecode and the archives. The
`game.kagari` file is byte for byte the same in both profiles, so a debug build plays exactly like
a release build.

| Profile | Writes |
| ------- | ------ |
| `release` (default) | `game.kagari` and the archives. |
| `debug` | The same, plus `game.kagari-cache`, the debug artifact. |

The debug artifact is for the [debugger](./debugger.md) and other tools, and holds:

- the explored story graph: every reachable story state the analysis found, per scene and calling
  context, with the steps (by [step ID](../concepts/steps.md#step-ids)), scene calls and random
  draws, the locals and story variables kept in each state, the successors of each state for
  every choice arm, drawn value and scene exit, and which renders are keyframes;
- debug information for every function: the names of its local variables, where they live over
  the function's code, and its lexical scopes;
- the hash of the matching `game.kagari`, so that a stale artifact is rejected rather than read
  against another build.

It is printed after the archives:

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
wrote target/v0/game.kagari-cache
```

A release build deletes a `game.kagari-cache` an earlier debug build left in the target directory.
The runtime reads `game.kagari-cache` only when a debugger attaches to it, so a shipped game does
not need it.

## `kagari clean`

```text
kagari clean [--manifest-path <PATH>]
```

Deletes what `kagari check` and `kagari build` wrote into the output directory (`build.out-dir`):

- the [analysis cache](#the-analysis-cache): every `.kagari-cache` file directly in the output
  directory;
- in the directory of every target (`v0`), listed in `build.targets` or not: `game.kagari`, every
  `.kiarchive` file and every `.kagari-cache` file.

Then it removes the target directories and the output directory if they are left empty. Other
files stay, and so do the directories that hold them.

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`. |

It prints how many files it removed, with the output directory relative to the project directory:

```text
removed 4 files from target
```

Exits with 0, or with 2 when it cannot run (no `kagari.toml`, an IO error).

```sh
kagari clean
kagari clean --manifest-path example/test_game1/kagari.toml
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

## `kagari debug`

```text
kagari debug [--manifest-path <PATH>] [--target <TARGET>] [--entry <PATH>] [--seed <N>] [--no-build] [--watch]
kagari debug --window [--runtime <PATH>] [--manifest-path <PATH>] [--target <TARGET>] [--no-build] [--watch]
kagari debug --attach <ADDRESS> [--manifest-path <PATH>] [--target <TARGET>] [--watch]
```

Builds the project in the debug profile and debugs its story: by default on the query engine
(the game without a window), with `--window` in a window, with `--attach` in a game already
running with `--debug-port`. It reads one command per line from stdin; `--watch` rebuilds and hot
swaps the game whenever a project file changes. The options and the
commands are in [The Debugger](./debugger.md#kagari-debug). The status is 0 after the input ends
or `quit`, and 2 when the project cannot be built or the game cannot be reached.

## `kagari dap`

```text
kagari dap
```

Runs the debug adapter, speaking the Debug Adapter Protocol over stdin and stdout. Editors start
it; you do not run it by hand. Its launch and attach settings are in
[The Debugger](./debugger.md#editors).
