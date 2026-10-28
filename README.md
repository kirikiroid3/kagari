# Kagari Visual Novel Engine

## Progress

Compiler stages follow [Compilation](docs/syntax/src/compilation.md#pipeline).

| Stage                    | Status         | Notes                                                                                          |
| ------------------------ | -------------- | ---------------------------------------------------------------------------------------------- |
| 1. Parse + desugar forms | ✅ Done        | Lossless CST, typed AST; `s!`/`choice!`/`t!` and the UI forms desugared.                       |
| 2. Resolve               | ✅ Done        | Module tree, imports, impls, paths, locals, labels, asset paths.                               |
| 3. Type and kind check   | ✅ Done        | Item signatures and type inference of every body (`Int` ranges, coercions, generics and std trait bounds, `nav::start`, `asset`); `match` exhaustiveness; kind and story rules (call rules, scenes and instant procedures, story variables and handles, finite types, second-class references and closures, mutability, `choice!`/`t!` rules, `#[must_use]`). |
| 4. Const evaluation      | ✅ Done        | Constants, statics, folding and tabulation of story bodies, every const-eval error, keyframe detection (`src/eval`). Evaluating renders on the screens that reach their steps is part of stage 5. |
| 5. Analysis              | ✅ Done        | Termination; the reachable states of the story from every entry scene and argument, summarized per scene (`src/analysis/story`): stuck choices, errors in story code, unreachable scenes and steps, renders on the screens that reach their steps, bounded screens, consistency; story variables that never influence the story; mode-graph checks of the system. |
| 6. Emit bytecode         | ✅ Done        | `kagari-bytecode`: format, encoder, decoder, verifier, disassembler; emitter in `src/emit`.     |

| Component     | Status                          | Notes                                                                                                                                                   |
| ------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kagari-std`  | 🚧 Designed; declarations only  | `time`, `screen` (+`anim`, `audio`), `stage`, `narrative` (+`text`), `system` (+`nav`, `saves`), `ui` (+`style`), `prelude`; see [`docs/std`](docs/std) and the diagnostic codes in [`docs/diagnostics`](docs/diagnostics). |
| `kagari-archive` | ✅ Done                      | `.kiarchive` asset packages (Store/LZ4 blocks, solid blocks, streamable front index, CRC32, numbered patch chains) and `.kisave` paged save databases (in-place edits, `compact()`). |
| `kagari` CLI  | ✅ Done                         | `kagari init\|check\|build\|fmt\|disasm\|lsp` on a `kagari.toml` project; `build` writes bytecode plus `.kiarchive` asset archives.                      |
| `kagari-fmt`  | ✅ Done                         | Width-based formatter (max width 100, 4-space indent), backs `kagari fmt` and LSP formatting.                                                           |
| `kagari-lsp`  | ✅ Done                         | Run by `kagari lsp`: diagnostics, formatting, go to definition, hover, document symbols.                                                                |
| Editor support | ✅ Done                        | `tooling/tree-sitter-kagari` grammar and the `tooling/zed-kagari` Zed extension.                                                                        |
| Runtime | ✅ Done | `kagari-vm`, `kagari-engine`, `kagari-text`, `kagari-render` and the `kagari-desktop-runtime` binary (winit + wgpu, kira audio, gilrs gamepads, headless mode). Profile `native_v0`: the whole std except video and shaders. |

## Running a game

Build the game, then point the runtime at the directory `kagari build` wrote:

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

Without `GAME_DIR`, the runtime uses its own directory, so a shipped game puts the executable next to `game.kagari` and the `.kiarchive` files.

Options:

- `--data-dir DIR`: where settings and saves go. By default they go to the platform data directory under `Kagari/<title>`: `~/.local/share/Kagari/<title>` on Linux, `~/Library/Application Support/Kagari/<title>` on macOS, and `Kagari\<title>` in the roaming AppData folder on Windows.
- `--backend wayland|x11` (Linux only): the window backend. The default is Wayland when `WAYLAND_DISPLAY` is set, else X11.

Building needs these system packages:

- Debian/Ubuntu: `libasound2-dev libudev-dev pkg-config`.
- Fedora: `alsa-lib-devel systemd-devel`.
- macOS and Windows: nothing.
- NixOS: `direnv allow` (or `nix develop`) enters the flake's dev shell.

At run time the runtime needs a Vulkan, Metal, DX12 or GL driver, plus Wayland or X11 and ALSA on Linux.

### Headless mode

`--headless --script FILE --out DIR [--seed N]` runs the game without a window, audio device or gamepads. The clock only moves on `wait`, saves record the fixed time 2026-01-01 00:00:00, every new story uses seed `N` (default 1), and saves go to `DIR/data` unless `--data-dir` is given. The script holds one command per line; blank lines and lines starting with `#` are skipped.

| Command | Action |
| ------- | ------ |
| `wait MS` | Advance the clock by `MS` ms, in steps of at most 16 ms, updating the game at each step |
| `key KEY` | Press and release a key; `KEY` is a `std::system::Key` variant name (`Space`, `Enter`, `A`, `Digit1`, …) |
| `down KEY` / `up KEY` | Press / release a key |
| `click X Y` | Move the mouse to design pixel (`X`, `Y`), then press and release the left button |
| `move X Y` | Move the mouse |
| `wheel DY` | Scroll by `DY` notches (`> 0` is up) |
| `capture NAME` | Write the current frame to `DIR/NAME.png` (`NAME`: 1–64 of `A-Z a-z 0-9 _ -`) |
| `expect-crash` | The game must have stopped with an error. Without it, any command but `capture` after a crash exits 3; with it, they are ignored |
| `assert-no-crash` | The game must still be running |

| Exit status | Meaning |
| ----------- | ------- |
| 0 | Success |
| 2 | Usage error, no `game.kagari` in the game directory, or the game cannot start |
| 3 | The game stopped with an error (a VM panic) |
| 4 | Window or GPU failure |
| 5 | Headless script error |

Example, for `example/test_game1`:

```sh
cat > smoke.txt <<'EOF'
wait 2000
capture title
# the title's Start button
click 285 265
wait 1000
capture first-line
assert-no-crash
EOF
kagari-desktop-runtime example/test_game1/target/v0 --headless --script smoke.txt --out shots --seed 1
```

It writes `shots/title.png` and `shots/first-line.png`; its saves go to `shots/data`.
