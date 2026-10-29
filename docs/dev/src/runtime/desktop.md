# The Desktop Runtime

`kagari-desktop-runtime` is the binary that runs the output of `kagari build` (`game.kagari` and
the `.kiarchive` files) on Linux, macOS and Windows. It implements the runtime profile
`native_v0`. It is a thin shell around the engine: it implements `Platform`, feeds input and
time, draws `Frame`s with `kagari-render`, and plays `AudioCommand`s with kira. With `--headless`
the same engine runs without a window, driven by a script, writing PNG captures; this is the
project's end-to-end test harness.

The user-facing documentation is the Reference: [The Runtime](https://kagari.suitsu31.club/en/reference/tools/runtime/)
and [Headless Mode](https://kagari.suitsu31.club/en/reference/tools/headless/).

## Where in the code

| Path (`runtime/kagari-desktop-runtime/src/`) | Contents |
| -------------------------------------------- | -------- |
| `main.rs` | logger setup, argument parsing, dispatch to `app::run` or `headless::run` |
| `cli.rs` | the clap `Cli` |
| `exit.rs` | exit statuses and `Failure` |
| `game.rs` | the game directory, reading `game.kagari`, `boot`, thumbnail uploads |
| `app.rs` | the windowed runtime: the winit `ApplicationHandler`, the frame, surface handling |
| `gpu.rs` | adapter, device and `Renderer` creation; out-of-memory and device-lost tracking |
| `platform.rs` | `DesktopPlatform`: the engine's `Platform` |
| `archives.rs` | the `.kiarchive` files, opened on first use |
| `store.rs` | settings and saves on disk |
| `audio.rs`, `sounds.rs` | the kira backend and the decoded-sound cache |
| `input.rs`, `pads.rs` | winit keyboard and mouse mapping, gilrs gamepads |
| `headless/mod.rs`, `headless/script.rs` | headless sessions and the script parser |
| `tests.rs` | key mapping, script parsing, store naming, audio spans |

Build-time system dependencies are declared in the repository's `flake.nix`.

## Process structure

`main` initializes `env_logger` with the default filter `kagari=info,wgpu=warn` (overridden by
`RUST_LOG`), parses the arguments, resolves the game directory (`GAME_DIR`, or the executable's
directory) and reads `game.kagari` from it. Then it runs `headless::run` or `app::run`. A
`Failure { code, message }` reaching `main` is printed as `kagari-desktop-runtime: <message>` and
becomes the exit status.

### Exit statuses

| Status | Constant | Cause |
| -----: | -------- | ----- |
| 0 | | normal end: the window closed, `nav::quit`, or the headless script finished |
| 2 | `USAGE` | clap usage errors, no `game.kagari` in the game directory, any `BootError` other than a trap (invalid bytecode, unsupported import, a font that fails to load) |
| 3 | `STOPPED` | the game trapped, during boot or later (`game stopped: <message>`); in a window, the status is reported when the window closes |
| 4 | `WINDOW` | no event loop or window, no surface, no usable adapter, device creation failed, GPU out of memory, device lost |
| 5 | `SCRIPT` | headless only: the script cannot be read or parsed, `expect-crash` without a crash, a capture cannot be written or read back |

## Platform implementation

`DesktopPlatform` implements the engine's `Platform`:

| Method | Implementation |
| ------ | -------------- |
| `read_store`, `write_store`, `delete_store` | `Store` ([Stores](#stores)) |
| `read_asset` | `Archives::read` |
| `audio_duration_ms` | decode with kira (`SoundCache::decode`) and take the duration; works without an audio device |
| `render_thumbnail` | `Renderer::render_to_rgba` on the platform's `Gpu`; `None` (with a warning) before the GPU exists |
| `local_time` | `jiff::Zoned::now()`, or the fixed headless time |
| `random_seed` | system time in nanoseconds XOR the process id, or the fixed headless seed |
| `set_title` | resolves the data directory from the title, unless it is fixed |

### Archives

`Archives` maps an asset path to its archive with `kagari_archive::archive::archive_stem` (the
same grouping `kagari build` writes: one archive per top-level asset directory) and opens
`<stem>.kiarchive` with `open_chain`, which also applies patch archives. Each archive is opened
on first use and kept; a missing archive is remembered as absent and logged once. See
[Archives and Save Databases](../formats/archive.md).

### Stores

`Store` writes each engine store as one file in the data directory:

- Names must match `[a-z0-9-]+\.kisave`; anything else is `InvalidInput`, so a store name can
  never escape the directory.
- Writes go to `<name>.tmp`, are `fsync`ed, then renamed over the target, so a crash keeps either
  the old or the new file.
- The directory is fixed by `--data-dir` (and in headless mode); otherwise `set_title` resolves
  `directories::BaseDirs::data_dir()/Kagari/<title>`. The title is sanitized: `/ \ : * ? " < > |`
  and control characters become `_`, surrounding whitespace and trailing dots are removed, an empty
  result is `Kagari Game`. Without a data directory every store access fails with
  `no data directory; pass --data-dir`, and the engine logs and continues.

## Windowed mode

`app::run` boots the engine **before** opening the window, at engine time 0: boot errors exit
without a window, and the window opens with the title, design resolution and fullscreen state the
game asks for. Then it builds the winit event loop (on Linux, Wayland when `--backend wayland`, or
when `WAYLAND_DISPLAY` is set and not empty; else X11; `--backend` elsewhere is ignored with a
warning) and a `wgpu::Instance` with the loop's display handle, and runs the `App` handler.

| Event | Handling |
| ----- | -------- |
| `resumed` (first) | open the window: design size in logical pixels, scaled down to fit 90 % of the primary monitor; borderless fullscreen when requested. Create the surface, the GPU and the renderer, configure the surface, compute the letterbox viewport, and start the engine clock |
| `RedrawRequested` | run one frame (below) |
| `Resized` | reconfigure the surface and the viewport; a 0×0 window (minimized) draws nothing |
| `KeyboardInput` | `HeldInputs::key`: physical key codes to `Key`, repeats dropped, left and right Shift (and Control, Alt) count as one key |
| `MouseInput`, `MouseWheel` | `MouseButton` (left, right, middle, back, forward); wheel lines, or pixels / 40 |
| `CursorMoved`, `CursorLeft` | `Viewport::to_design`; leaving sends `(-1, -1)` |
| `Focused(false)` | forget held keys, send `Focus(false)` |
| `CloseRequested` | `Engine::quit` (autosave, write stores), exit the loop |
| `about_to_wait` | request the next redraw; after a frame that could not draw, wait 16 ms instead |

**The clock.** Engine time is milliseconds since the window and GPU became ready, so their setup
time does not eat into the first animations. The engine booted at 0, before that.

**One frame:**

1. Poll gamepads and send their events.
2. `Engine::update(now)`. When the engine has crashed, remember exit status 3 (the window stays
   open, showing the crash frame).
3. Check the GPU flags; out of memory or a lost device exits with status 4.
4. Pass `take_audio()` to the audio backend.
5. Apply the `WindowState`: exit on `quit`; update the title and fullscreen state when they
   changed.
6. `Engine::frame(now)`, upload stale save thumbnails, then present: letterbox the design target
   into the surface texture and present it (`Fifo`, so the loop runs at the display rate).
7. If nothing could be presented but the frame has `capture_still`, render it offscreen anyway: the
   engine draws `Node::Still` from the next frame on and needs the still to exist.

**Surface errors.** `Timeout` and `Occluded` skip the frame and idle; `Outdated` reconfigures;
`Suboptimal` presents, then reconfigures; `Lost` drops the old surface first (a window can hold
only one Vulkan surface) and creates a new one; `Validation` is logged and the frame skipped.

### GPU

`gpu::create(instance, surface)` requests a low-power adapter compatible with the surface, and
falls back to a software adapter (`force_fallback_adapter`) when none is found. The surface format
is the first sRGB format the surface offers, else its first format; headless uses
`Rgba8UnormSrgb`. The device uses `Limits::downlevel_defaults()` raised to the adapter's texture
resolution limits. An uncaptured out-of-memory error and the device-lost callback set an atomic
flag that every frame checks; the device is never recreated.

### Audio

`AudioOut` opens kira's default backend (cpal) with three sub-tracks for the `Music`, `Sound` and
`Voice` buses. When no output device or track is available it logs
`audio output unavailable (…); running silent` and drops every command.

- Gains are linear amplitude; kira volume is `20·log10(gain)` dB, silence at 0.001 or below.
- Tweens and fades are linear.
- `Play` resolves the decoded sound through `DesktopPlatform::sound` and clamps its times with
  `clamp_span`: a start or end past the audio is clamped to its end (with a warning), an end before
  the start is ignored, a loop region (in file time) that is empty after clamping disables
  looping. So no engine command can make kira index past the data or spin on an empty loop.
- `Restart` replays a sound with its original parameters, without fade-in or delay.
- `SoundCache` keeps decoded sounds by asset path, evicting the least recently used beyond
  256 MiB (counted as 8 bytes per frame). Playing sounds hold their own reference, so eviction
  never cuts one off. A path that fails to decode is remembered and logged once.

### Gamepads

`Pads` wraps gilrs. Buttons map by position (`South`, `East`, `West`, `North`, shoulders, triggers,
`Start`, `Select`, sticks, D-pad); `C`, `Z`, `Mode` and unknown buttons are ignored. A button is
pressed while any connected pad holds it; a disconnect releases that pad's buttons. When gilrs is
unavailable on the platform it runs without gamepads.

## Headless mode

`--headless --script FILE --out DIR [--seed N] [--data-dir DIR]` runs the engine without a window,
an audio device or gamepads.

**Setup.** The script is read and fully parsed first (a parse error exits 5 before boot). `OUT` is
created. The platform uses a fixed store in `--data-dir` or `OUT/data`, the fixed save time
2026-01-01 00:00:00, and the fixed seed `--seed` (default 1). The GPU is created from a
`wgpu::Instance` without a display handle and no surface (a software adapter is acceptable).
Then the engine boots at time 0.

**Script.** One command per line; blank lines and lines starting with `#` are skipped; tokens are
separated by whitespace. Key names are the `Debug` names of `kagari_engine::Key` (`Enter`,
`Space`, `Escape`, `A`, `Digit1`, `F5`, …).

| Command | Effect |
| ------- | ------ |
| `wait MS` | advance engine time by `MS` in ticks of at most 16 ms; each tick runs `update`, drops the audio commands, builds the frame, and renders it offscreen when it captures a still |
| `key K` | press and release `K` |
| `down K`, `up K` | press / release `K` |
| `click X Y` | move to design pixel (`X`, `Y`), press and release the left button |
| `move X Y` | move the pointer |
| `wheel D` | wheel by `D` notches (positive is up) |
| `capture NAME` | render the current frame at the design resolution and write `OUT/NAME.png`; prints `wrote <path>`. `NAME` is 1 to 64 characters of `[A-Za-z0-9_-]` |
| `expect-crash` | the game must have crashed by now; from here on a crash is expected |
| `assert-no-crash` | the game must not have crashed |

Input commands are followed by one `update` at the same time, so their effects (navigation, story
input) are visible to an immediate `capture`. Time advances only through `wait`.

**Crash handling.** Before each command the session checks the engine: an unexpected crash ends
the run with status 3 (`game stopped: …`); after `expect-crash`, later commands other than
`capture` are skipped. `capture` works on a crashed engine (it shows the crash frame). The run
stops early when the game quits.

**End.** After the last command, an unexpected crash exits 3; a running game is quit through
`Engine::quit`, which autosaves at a step and writes the stores to the data directory. Exit 0.

**Determinism.** Given the same `game.kagari`, script, seed and stores, the engine's behaviour is
identical on every run ([Saves](./saves.md#determinism)). Captured pixels can differ slightly
between GPU adapters and drivers (rasterization and floating-point blending are the adapter's);
compare captures made on the same machine.

## Platform notes

| Platform | Notes |
| -------- | ----- |
| Linux | Wayland or X11 (`--backend`); Vulkan or OpenGL (EGL) through wgpu; audio through ALSA (cpal); gamepads through udev (gilrs). Data directory `$XDG_DATA_HOME/Kagari/<title>` |
| macOS | Metal; data directory `~/Library/Application Support/Kagari/<title>` |
| Windows | DirectX 12 (or Vulkan); data directory `%APPDATA%\Kagari\<title>` |

**System dependencies.** `flake.nix` provides the dev shell on NixOS (`direnv allow` or
`nix develop`); Rust itself comes from rustup (`rust-toolchain.toml`).

| Library | Why |
| ------- | --- |
| `alsa-lib` | linked at build time by `alsa-sys` (cpal) |
| `udev` | linked at build time by `libudev-sys` (gilrs) |
| `wayland`, `libxkbcommon`, `libX11`, `libXcursor`, `libXi`, `libXrandr` | loaded at run time by winit |
| `vulkan-loader`, `libGL` | loaded at run time by wgpu |
| `pkg-config` | finds the build-time libraries |

The shell puts the run-time libraries on `LD_LIBRARY_PATH`, since they are `dlopen`ed. Other Linux
distributions need the equivalent development packages (Debian: `libasound2-dev libudev-dev
pkg-config`); macOS and Windows need none.

**Logging.** Besides module targets, the engine logs progress under fixed targets that are useful
when reading headless runs: `kagari::system` (`modes: [...]` whenever the mode stack changes),
`kagari::story` (`step <function>#<ordinal> say|choice`, `chose <choice ID>`, `story returned`),
`kagari::saves` (`saved`, `loaded … (n choices replayed)`, divergence warnings) and
`kagari::desktop` (window backend, data directory).

## Invariants

- The runtime owns no game logic: everything it shows or plays comes from `Engine::frame` and
  `take_audio`, everything it knows of the game from `Engine::window` and `crash`.
- Engine time is monotonic and starts at 0 at boot; headless time advances only through `wait`.
- Headless runs use a fixed seed, a fixed save time and a data directory under `OUT` by default,
  so they never touch the player's saves.
- Exit statuses are part of the user contract (scripts and CI depend on them).
- A missing audio device or gamepad support never stops the game.
- Store names are validated before touching the file system.

## When you change this

- Command-line options, exit statuses, data directories, platform requirements: the Reference
  [The Runtime](https://kagari.suitsu31.club/en/reference/tools/runtime/) (en and ja).
- Script commands or headless behaviour: the Reference
  [Headless Mode](https://kagari.suitsu31.club/en/reference/tools/headless/) (en and ja), `headless/script.rs`
  tests in `tests.rs`.
- A new key, mouse or pad button: `kagari_engine::Key`/`PadButton` and `std::system` in
  `core/kagari-std`, `input.rs` (`map_key` and the test inverse `key_code`, which is exhaustive so
  a missing mapping fails to compile), the STD page `system.md`.
- New system libraries: `flake.nix` and the Reference's build requirements.
- Verify with `cargo test -p kagari-desktop-runtime` and a headless run of `example/test_game1`.
