# Headless Mode

Headless mode runs a game without a window, audio device or gamepads. A script drives the input
and the clock and writes screenshots. Use it for automated tests: smoke tests in CI, checking that
a route reaches its ending, comparing screenshots between builds.

## Running headless

```text
kagari-desktop-runtime [GAME_DIR] --headless --script <FILE> --out <DIR> [--seed <N>] [--data-dir <DIR>]
```

| Option | Meaning |
| ------ | ------- |
| `--headless` | Run without a window. Requires `--script` and `--out`. |
| `--script <FILE>` | The input script (below). |
| `--out <DIR>` | The output directory, created when missing. Captures are written here. |
| `--seed <N>` | The random seed of every new story. Default `1`. |
| `--data-dir <DIR>` | The directory for settings and saves. Default `<out>/data`. |

`--script`, `--out` and `--seed` are usage errors (status 2) without `--headless`. `GAME_DIR`
works as in [the runtime](./runtime.md#command-line).

Headless mode still renders with the GPU, so it needs a GPU driver, but no display server. Where
no hardware GPU is available (as on many CI machines), a software adapter such as Mesa's
llvmpipe works.

## Determinism

A headless run is reproducible: the same game, script and seed give the same captures.

| Aspect | Windowed | Headless |
| ------ | -------- | -------- |
| Clock | Real time | Starts at 0 ms; moves only on `wait` |
| Save time (`local_time`) | The local time | Always 2026-01-01 00:00:00 |
| Seed of a new story | From the system clock and process id | `--seed`, default `1` |
| Settings and saves | Platform data directory | `--data-dir`, default `<out>/data` |
| Audio | Played | Processed, not played |
| Gamepads | Read | None |

Settings and saves persist in the data directory between runs. Delete it (or use a fresh `--out`)
when each run must start from a clean state.

## Script format

A script is a text file with one command per line. Arguments are separated by whitespace. Blank
lines and lines whose first non-blank character is `#` are skipped. Commands run in order.

| Command | Action |
| ------- | ------ |
| `wait MS` | Advance the clock by `MS` milliseconds, in steps of at most 16 ms, updating the game at each step. |
| `key KEY` | Press and release a key. |
| `down KEY` | Press a key and hold it. |
| `up KEY` | Release a key. |
| `click X Y` | Move the mouse to design pixel (`X`, `Y`), then press and release the left button. |
| `move X Y` | Move the mouse to design pixel (`X`, `Y`). |
| `wheel DY` | Scroll by `DY` notches; `DY > 0` scrolls up. |
| `capture NAME` | Write the current frame to `<out>/NAME.png` and print `wrote <path>`. `NAME` is 1–64 characters of `A-Z a-z 0-9 _ -`. |
| `expect-crash` | The game must have stopped with an error by now. |
| `assert-no-crash` | The game must still be running. |

- `KEY` is a variant name of `std::system::Key`: `Enter`, `Escape`, `Space`, `Backspace`, `Tab`,
  `Up`, `Down`, `Left`, `Right`, `PageUp`, `PageDown`, `Home`, `End`, `Shift`, `Control`, `Alt`,
  `F1`–`F12`, `A`–`Z`, `Digit0`–`Digit9`. Names are case-sensitive.
- Coordinates and wheel deltas are decimal numbers (`285`, `12.5`). Coordinates are in the game's
  design resolution, independent of any window size.
- Input commands (`key`, `down`, `up`, `click`, `move`, `wheel`) do not advance the clock; the
  game handles them at the current time. Add a `wait` to let animations and text progress.
- Captures are PNG files at the design resolution.

The whole script is parsed before the game starts. A syntax error stops the run with status 5
and a message `script:<line>: <message>`, for example `` script:3: unknown key `space` ``.

### Errors and crashes

When the game stops with an error (see
[When the game stops with an error](./runtime.md#when-the-game-stops-with-an-error)):

- Without `expect-crash`, the next command other than `capture` ends the run with status 3. If
  the error happens during the last command, the run also ends with status 3.
- `expect-crash` accepts the error. All later commands except `capture` are ignored, and the run
  ends with status 0. `capture` writes the error screen.
- `expect-crash` while the game is still running is a script error (status 5).
- `assert-no-crash` after an error ends the run with status 3.

When the game quits by itself (for example through a "quit" button), the remaining commands are
skipped and the run ends with status 0. At the end of the script, a running game is quit as when
the player closes the window.

## Exit statuses

| Status | Meaning |
| -----: | ------- |
| 0 | The script ran to its end (or the game quit). |
| 2 | Usage error, no `game.kagari`, or the game cannot start: an invalid `.kagari` file, an import of a builtin this runtime lacks, or a font that fails to load (see [the runtime](./runtime.md#when-the-game-stops-with-an-error)). |
| 3 | The game stopped with an error that the script did not expect, or `assert-no-crash` failed. |
| 4 | GPU failure: no usable adapter, out of memory, device lost. |
| 5 | Script error: the script cannot be read or parsed, `expect-crash` found a running game, `--out` cannot be created, or a capture cannot be written. |

## Example

A smoke test for `example/test_game1`: wait for the title, capture it, click the title's Start
button, and capture the first line.

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

It writes `shots/title.png` and `shots/first-line.png`; its settings and saves go to
`shots/data`. The exit status is 0 when the game is still running at the end.
