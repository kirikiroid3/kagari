---
title: "The Runtime"
---

`kagari-desktop-runtime` runs a game built by `kagari build` on Windows, macOS and Linux. It
opens a window, draws with the GPU, plays audio, and reads the keyboard, mouse and gamepads. It
implements the runtime profile `native_v0`. With `--headless` it runs without a window, driven by
a script; see [Headless Mode](./headless.md).

## Running a game

Build the game, then point the runtime at the directory `kagari build` wrote:

```sh
kagari build --manifest-path example/test_game1/kagari.toml
kagari-desktop-runtime example/test_game1/target/v0
```

Inside the Kagari repository, without installing anything:

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

Use a release build of the runtime for playing; a debug build is much slower.

## Command line

```text
kagari-desktop-runtime [OPTIONS] [GAME_DIR]
```

| Argument / option | Meaning |
| ----------------- | ------- |
| `GAME_DIR` | The directory with `game.kagari` and the `.kiarchive` files. Default: the directory of the runtime executable. |
| `--data-dir <DIR>` | The directory for settings and saves. Default: the platform data directory (see [Data directory](#data-directory)). |
| `--backend <BACKEND>` | The window backend, `wayland` or `x11` (Linux only; ignored with a warning elsewhere). Default: `wayland` when `WAYLAND_DISPLAY` is set and not empty, else `x11`. |
| `--headless` | Run without a window. Requires `--script` and `--out`. See [Headless Mode](./headless.md). |
| `--script <FILE>` | The headless input script. Requires `--headless`. |
| `--out <DIR>` | The directory for headless captures (and saves, without `--data-dir`). Requires `--headless`. |
| `--seed <N>` | The random seed of every new story in headless mode. Default `1`. Requires `--headless`. |
| `--debug-port <PORT>` | Accept a debugger on `127.0.0.1:PORT` (`0` picks a free port and logs it). Not with `--headless`. See [The Debugger](./debugger.md#window). With a debug build, the game records what [hot swap](./debugger.md#hot-swap) needs from its start (this flag and `--debug-stdio`). |
| `--debug-stdio` | Talk to a debugger over standard input and output (how `kagari debug --window` and `kagari dap` start a game). Not with `--headless` or `--debug-port`. |
| `-h`, `--help` | Print the help. |
| `-V`, `--version` | Print the version. |

When `GAME_DIR` has no `game.kagari`, the runtime stops with status 2 and suggests passing the
directory written by `kagari build`, such as `target/v0`.

Because `GAME_DIR` defaults to the executable's directory, a shipped game puts the executable
next to `game.kagari` and the archives; players then start it without arguments. See
[Shipping a game](./projects.md#shipping-a-game).

Without `--debug-port` or `--debug-stdio` the runtime opens no debug channel. With one, a
debugger can stop the story, move it and inspect it; it reads `game.kagari-cache` from the game
directory when a debug build wrote it (see [The Debugger](./debugger.md)).

## The window

The window title and fullscreen state are the ones the game's system sets, and they follow
changes while the game runs. The window opens at the game's design resolution, scaled down when
needed to fit in 90% of the primary monitor. When the window is resized, the picture is scaled
to fit and letterboxed; mouse positions are converted back to design pixels.

Closing the window ends the game normally (the game's quit handling runs) and exits with
status 0.

## Data directory

Settings and saves are files named `<name>.kisave` in the data directory. By default it is
`Kagari/<title>` in the platform data directory, where `<title>` is the game's window title:

| Platform | Default data directory |
| -------- | ---------------------- |
| Linux | `$XDG_DATA_HOME/Kagari/<title>` (default `~/.local/share/Kagari/<title>`) |
| macOS | `~/Library/Application Support/Kagari/<title>` |
| Windows | `Kagari\<title>` in the roaming AppData folder (`%APPDATA%\Kagari\<title>`) |

In `<title>`, the characters `/ \ : * ? " < > |` and control characters become `_`, surrounding
whitespace and trailing dots are removed, and an empty title becomes `Kagari Game`. Changing the
title therefore changes where settings and saves are stored; keep it stable once you ship.

`--data-dir <DIR>` replaces the directory and makes it independent of the title. Files are
written atomically (to a temporary file, then renamed), so a crash during a save does not corrupt
the previous file. When no data directory can be found, the runtime logs a warning and settings
and saves are not kept.

## Runtime profile `native_v0`

The desktop runtime implements the runtime profile `native_v0`: the whole std except video. Its
asset formats are listed in [Runtime profiles](./projects.md#runtime-profiles); `kagari build`
rejects assets it cannot decode and shaders that do not validate, so a game that builds with
`runtime-compatibility = ["native_v0"]` has no unsupported asset at run time. Shaders need a GPU
with storage buffers, and motion shaders also compute shaders; on a GPU without them the runtime
logs a warning and draws those effects as if they were not set. See also
[Runtime Profiles](../../std/appendix/profiles.md) in the STD Reference.

The runtime keeps going when a device is missing:

- No audio output: it logs `audio output unavailable (…); running silent` and runs without sound.
- No gamepad support: it logs a warning and runs with keyboard and mouse only.
- A missing archive or an undecodable sound: it logs a warning; the asset is missing.

## Logging

The runtime logs to stderr. The default level is `kagari=info,wgpu=warn`, which includes the
GPU adapter, the window backend, and the data directory in use. Set `RUST_LOG` to change it, for
example `RUST_LOG=debug`. The progress of the story is logged under the target `kagari::story`:
one line per step the story enters, `step <step ID> say` or `step <step ID> choice`, for example
`step crate::Start#0 say`; see [Step IDs](../concepts/steps.md#step-ids).

## When the game stops with an error

A game stops with an error when its code fails at run time: an arithmetic overflow, a division
by zero, an index out of range, a call deeper than
[`recursion-depth`](./projects.md#compiler-options), or an error raised by a std function.
Then:

1. The game stops: pending navigation is dropped and all audio stops. Later errors are ignored.
2. The window shows a black screen with the error message in white at the top left.
3. The runtime prints `kagari-desktop-runtime: game stopped: <message>` to stderr.
4. The window stays open until the player closes it; the exit status is then 3.

The message gives the error, the function and its source location (`file@byte offset`), and,
when the story had reached a step, `after step <step ID>` with the
[step ID](../concepts/steps.md#step-ids) of the last step reached. Each outer call follows on its
own line as `called from <function> (<location>)`. For example:

```text
vm panic: index out of range at crate::story::FirstDate (story.kis@1234), after step crate::story::FirstDate#28
```

An error in the game's code during start-up (while `main` runs, before the window opens) prints
the same message and exits with status 3 at once. A game that cannot start for another reason
exits with status 2 and prints `kagari-desktop-runtime: cannot start the game: <reason>`:

- the `.kagari` file is invalid or fails verification;
- the `.kagari` file imports a builtin this runtime does not provide
  (``the bytecode imports `<path>`, which this runtime does not provide``), for example a file
  built for a newer runtime;
- a font fails to load: a font asset is missing or cannot be decoded, or the system's default
  font is not loaded (``the system font `<path>` is not loaded``);
- a text style of the system refers to an asset that cannot be loaded.

## Exit statuses

| Status | Meaning |
| -----: | ------- |
| 0 | The game ended normally. |
| 2 | Usage error, no `game.kagari` in the game directory, a `--debug-port` that cannot be opened, or the game cannot start: an invalid `.kagari` file, an import of a builtin this runtime lacks, or a font (or other text-style asset) that fails to load. See [When the game stops with an error](#when-the-game-stops-with-an-error). |
| 3 | The game stopped with an error. |
| 4 | A window or GPU failure: no window, no usable GPU adapter, the GPU ran out of memory or was lost. |
| 5 | A headless script error (headless mode only). |

Except for status 0, the runtime prints `kagari-desktop-runtime: <message>` to stderr.

## System requirements

At run time the runtime needs a GPU driver for Vulkan, Metal, DirectX 12 or OpenGL. When no
hardware adapter is usable it tries a software fallback adapter before giving up with status 4.
On Linux it also needs Wayland or X11 and ALSA.

Building the runtime from source needs these system packages:

| System | Packages |
| ------ | -------- |
| Debian, Ubuntu | `libasound2-dev libudev-dev pkg-config` |
| Fedora | `alsa-lib-devel systemd-devel` |
| macOS, Windows | none |
| NixOS | `direnv allow` (or `nix develop`) in the repository enters the dev shell with everything |
