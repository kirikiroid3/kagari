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
| `main.rs` | the Windows GUI subsystem, logger setup, argument parsing, dispatch to `app::run` or `headless::run` |
| `console.rs` | Windows only: attaching the parent's console for unset stdout and stderr |
| `cli.rs` | the clap `Cli` |
| `exit.rs` | exit statuses and `Failure` |
| `game.rs` | the game directory (`game_dir`, `bundle_resources` for a macOS app), reading `game.kagari`, `boot`, thumbnail uploads |
| `app.rs` | the windowed runtime: the winit `ApplicationHandler`, the frame, surface handling |
| `debug.rs` | `DebugPort`: the debug channel (`--debug-port`, `--debug-stdio`) |
| `gpu.rs` | adapter, device and `Renderer` creation; out-of-memory and device-lost tracking |
| `platform.rs` | `DesktopPlatform`: the engine's `Platform` |
| `movies.rs` | `Movies`: movie bytes and facts, one player per `VideoKey`, frame uploads ([Movies](#movies)) |
| `store.rs` | settings and saves on disk |
| `audio.rs`, `sounds.rs` | the kira backend (movie audio included) and the sound cache (short sounds decoded, long ones streamed) |
| `input.rs`, `pads.rs` | winit keyboard and mouse mapping, gilrs gamepads |
| `headless/mod.rs`, `headless/script.rs` | headless sessions and the script parser |
| `tests.rs` | key mapping, script parsing, store naming, audio spans, WAV sizes past the end of the file, streaming of long sounds |

| Path (`runtime/kagari-desktop-runtime/`) | Contents |
| ---------------------------------------- | -------- |
| `build.rs`, `kagari-desktop-runtime.rc` | Windows: compiles the resource script with `embed-resource` (`rc.exe`; `llvm-rc` when cross-compiling) into the executable; a build that cannot compile it fails. Linux: links the binaries with RUNPATH `$ORIGIN`; macOS: with the rpaths `@executable_path` and `@executable_path/../Frameworks`; so the FFmpeg libraries shipped next to them (or in an app's `Frameworks/`) are found |
| `resources/kagari.ico` | The Kagari logo as icon resource 1: the `.exe`'s icon in Explorer, and the window's title bar and taskbar icon (`app.rs`, `with_logo`, which loads it with winit's `Icon::from_resource`; other platforms leave the icon to the desktop) |
| `resources/kagari.icns` | The Kagari logo for macOS app bundles. Not embedded in the binary: the release workflow uploads it next to the macOS runtime, and `kagari release --macos` puts it into the app's `Resources/` ([`kagari release`](../tooling/cli.md#kagari-release)) |

`kagari.ico` is rendered from the site's `docs/site/public/logo.svg` (its light colours) at 16, 20,
24, 32, 40, 48, 64, 128 and 256 pixels with Inkscape and packed as PNG entries. `kagari.icns` is
rendered from the same SVG with `rsvg-convert` into an iconset (`icon_NxN.png` and
`icon_NxN@2x.png` for 16, 32, 64, 128, 256, 512 and 1024 pixels) and packed with
`iconutil -c icns` on a Mac. Regenerate both when the logo changes; nothing checks that they agree.

Build-time system dependencies are declared in the repository's `flake.nix`.

## Process structure

`main` attaches the console (below), initializes `env_logger` with the default filter
`kagari=info,wgpu=warn` (overridden by `RUST_LOG`), parses the arguments, resolves the game
directory and reads `game.kagari` from it. Then it runs
`headless::run` or `app::run`. A `Failure { code, message }` reaching `main` is printed as
`kagari-desktop-runtime: <message>` and becomes the exit status.

The game directory (`game::game_dir`) is `GAME_DIR` when set, else the executable's directory.
On macOS, when that directory has no `game.kagari` and is `<x>.app/Contents/MacOS`, it is
`Contents/Resources` if that holds `game.kagari` (`bundle_resources`): the layout
`kagari release --macos` writes, which keeps data out of `Contents/MacOS` so the code signature
covers it as resources.

### The console on Windows

On Windows the executable is a GUI-subsystem program
(`#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]`): a player starting the
game (`run.exe` in the image of `kagari release`) gets no console window next to the game. Unit
tests keep the console subsystem so the test harness prints.

A GUI-subsystem process has no console, so started from a terminal its output would go nowhere.
`console::attach` runs first in `main`: when stdout or stderr is unset (null or invalid handle),
it calls `AttachConsole(ATTACH_PARENT_PROCESS)`, opens `CONOUT$` and sets it as each unset handle;
handles the parent set (pipes and files, such as `kagari debug --window`'s stdout pipe or
`> log.txt`) are put back as they were. Started from Explorer, there is no parent console and
nothing happens. Stdin is never attached: the shell keeps reading it.

Shells do not wait for a GUI program. In an interactive `cmd` or PowerShell the prompt returns at
once, the runtime's output appears after it and the exit status is lost. Whatever needs the status
waits explicitly: `start /wait` in `cmd`, a pipeline in PowerShell
(`kagari-desktop-runtime.exe --version | Write-Output`, as the release workflow's smoke test
does), or `Start-Process -Wait`. Programs that spawn it (`kagari debug`, CI) wait as for any other
process.

### Exit statuses

| Status | Constant | Cause |
| -----: | -------- | ----- |
| 0 | | normal end: the window closed, `nav::quit`, or the headless script finished |
| 2 | `USAGE` | clap usage errors, no `game.kagari` in the game directory, any `BootError` other than a trap (invalid bytecode, unsupported import, a font that fails to load), a `--debug-port` that cannot be bound |
| 3 | `STOPPED` | the game trapped, during boot or later (`game stopped: <message>`); in a window, the status is reported when the window closes |
| 4 | `WINDOW` | no event loop or window, no surface, no usable adapter, device creation failed, GPU out of memory, device lost |
| 5 | `SCRIPT` | headless only: the script cannot be read or parsed, `expect-crash` without a crash, a capture cannot be written or read back |

## Platform implementation

`DesktopPlatform` implements the engine's `Platform`:

| Method | Implementation |
| ------ | -------------- |
| `read_store`, `write_store`, `delete_store` | `Store` ([Stores](#stores)) |
| `read_asset` | `kagari_archive::archive::ArchiveDir::read` |
| `audio_duration_ms` | load through `SoundCache::load` (the container's length for a streamed sound) and take the duration; works without an audio device |
| `video_info` | `Movies::info`: `kagari_video::probe` on the movie's bytes, cached per path; an unreadable movie is warned once |
| `render_thumbnail` | `Renderer::render_to_rgba` on the platform's `Gpu`; `None` (with a warning) before the GPU exists |
| `local_time` | `jiff::Zoned::now()`, or the fixed headless time |
| `random_seed` | system time in nanoseconds XOR the process id, or the fixed headless seed |
| `set_title` | resolves the data directory from the title, unless it is fixed |
| `program_changed` | after a hot swap: opens the archives again, drops the cached sounds and movies (`Movies::reset`) and creates the `Renderer` again (its textures and pipelines are keyed by the old build's asset ids) |

### Archives

`ArchiveDir` (in `kagari-archive`) maps an asset path to its archive with `archive_stem` (the
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
6. `Engine::frame(now)`, upload stale save thumbnails, decode and upload the frame's movies
   (`DesktopPlatform::update_movies`, [Movies](#movies)), then present: letterbox the design target
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
resolution limits, and `MemoryHints::MemoryUsage`: with the default (`Performance`) the allocator
maps 64–128 MiB host-visible blocks, which stay resident in the process (about 60 MB more on the
example game). An uncaptured out-of-memory error and the device-lost callback set an atomic
flag that every frame checks; the device is never recreated.

### Audio

`AudioOut` opens kira's default backend (cpal) with three sub-tracks for the `Music`, `Sound` and
`Voice` buses. When no output device or track is available it logs
`audio output unavailable (…); running silent` and drops every command.

- Gains are linear amplitude; kira volume is `20·log10(gain)` dB, silence at 0.001 or below.
- Tweens and fades are linear.
- `Play` resolves the sound through `DesktopPlatform::sound` and clamps its times with
  `clamp_span`: a start or end past the audio is clamped to its end (with a warning), an end before
  the start is ignored, a loop region (in file time) that is empty after clamping disables
  looping. So no engine command can make kira index past the data or spin on an empty loop.
- `Restart` replays a sound with its original parameters, without fade-in or delay; on movie
  audio it does nothing.
- `PlayMovie { id, path, looping, gain }` reads the movie through `DesktopPlatform::movie_bytes`,
  opens `kagari_video::AudioDecoder` (a movie without audio plays nothing; an error is warned as
  ``cannot play movie audio `path`: …``), and plays it as a kira `StreamingSoundData` through
  `MovieAudio` on the `Music` bus (`Handle::Movie`; looping loops the whole file). Decode errors
  while playing are logged like those of streamed sounds. `Pause { id }` / `Resume { id }` pause
  and resume any handle without a fade.
- `SoundCache::load` keeps a sound by asset path in one of two forms. A sound whose decoded frames
  (8 bytes each: two `f32` channels) would exceed 2 MiB (about 6 s at 44.1 kHz) by its container's
  frame count is `Sound::Streamed`: the encoded bytes, shared by every play, each play a kira
  `StreamingSoundData` decoding on its own thread (decode errors while playing are logged). Other
  sounds, and files whose container gives no length, are decoded whole (`Sound::Decoded`,
  `StaticSoundData`). Decoding whole music would cost about 21 MB per minute.
  The length and sample rate of a stream come from the container; kira exposes the rate only
  through the duration, from which `stream_length` recovers it exactly. For WAV, Ogg Vorbis,
  FLAC, MP3 (with or without a Xing header), ADTS AAC and ALAC the container's length equals the
  decoded one; for AAC in MP4 it leaves out the encoder's trailing padding (344 frames on a 10 s
  ffmpeg encode), so a long AAC-in-MP4 sound reports and plays a few milliseconds less than the
  same file decoded whole. The priming at the start is kept either way. Lengths only time
  presentation (voice and `play` waits); they are never saved.
- The cache evicts the least recently used sounds beyond 32 MiB (decoded frames plus encoded
  bytes). Playing sounds hold their own reference, so eviction never cuts one off. A path that
  fails to decode is remembered and logged once.
- Before decoding, `fit_wav_sizes` lowers a WAV's RIFF and `data` chunk sizes that run past the
  end of the file to the bytes present. Streaming encoders write `0xFFFFFFFF` there; symphonia
  then reads the `data` chunk until it runs out of bytes and fails the whole decode, while the
  `kagari-compat` sniffer (which reads only the `fmt ` chunk) accepts the file, so `kagari build`
  and the runtime would disagree.

### Gamepads

`Pads` wraps gilrs. Buttons map by position (`South`, `East`, `West`, `North`, shoulders, triggers,
`Start`, `Select`, sticks, D-pad); `C`, `Z`, `Mode` and unknown buttons are ignored. A button is
pressed while any connected pad holds it; a disconnect releases that pad's buttons. When gilrs is
unavailable on the platform it runs without gamepads.

## Movies

`Movies` (`movies.rs`) turns the frame's `VideoRequest`s into textures
([Text and Rendering](./text-render.md#how-the-engine-builds-a-frame)). It keeps the encoded
bytes per asset path (read from the archives on first use; the least recently used bytes no live
player holds are evicted beyond 256 MiB), the probe result per path, and one player per
`VideoKey`. `DesktopPlatform::update_movies(frame, assets)` runs `Movies::update` with
`frame.video_requests()`:

- The target time is `position_ms`, modulo the movie length when it loops.
- A new player opens `kagari_video::VideoDecoder` and logs `` movie `path`: <backend> decoding ``
  (info, target `kagari::video`). An open or decode error is warned once; the player then shows
  nothing (or keeps its last frame).
- A frame is uploaded with `Renderer::set_video_frame` only when it differs from the last one
  uploaded for that key (`Arc::ptr_eq`).
- A player whose key the frame does not request is dropped with its texture (`remove_video`).

| `Pace` | Used by | Decoding |
| ------ | ------- | -------- |
| `Live` | the windowed runtime | `Accel::Hardware`; the first frame is decoded synchronously, then the decoder moves to a `kagari-video` thread that decodes the latest target; the main thread never waits, and a late frame keeps the previous one on screen |
| `Exact` | headless | `Accel::Software`, the exact frame decoded synchronously at each capture, so captures are deterministic |

The hooks: the windowed frame (after the thumbnails), the headless `capture`, and the
`capture_still` render inside `wait`. A headless run decodes nothing between captures. Headless
runs drop audio commands, so movie audio is never decoded there.

## `kagari-video`

`runtime_libs/kagari-video` wraps FFmpeg (`ffmpeg-next` 9, linked dynamically) and depends on no
Kagari crate, so other runtimes can reuse it. It reads movies from memory (`StreamIo` over a
`Cursor<Arc<[u8]>>`). Its API: `init` (once per process; FFmpeg's own log level is `Error`, and
its messages go to stderr), `probe` → `MediaInfo { width, height, duration_ms, has_audio }`,
`VideoDecoder { open(bytes, Accel), backend, frame_at }`, `AudioDecoder { open, sample_rate,
num_frames, decode, seek }`, `RgbaFrame { pts_ms, width, height, rgba }`, `VideoError`. Both
decoders are `Send`.

**Decoder choice.** Software: AV1 through `libdav1d` (else FFmpeg's native `av1`), H.264 and VP9
through the native decoders, with automatic frame threading. Hardware: VAAPI on Linux, D3D11VA on
Windows, VideoToolbox on macOS, with the native decoders; the hardware pixel format comes from
`avcodec_get_hw_config`, and `get_format` returns it when offered, else the first format that is
not a hwaccel one. Hardware decoding falls back to software, once per decoder, when no config
matches, the device cannot be created, the decoder does not open, a decode fails, or FFmpeg
silently decodes in software; the decoder reopens in software and seeks to the current target.
The fallback logs `video: hardware decoding unavailable (<reason>); using software` (info,
`kagari::video`); FFmpeg may print its own lines (such as "Hardware does not support image
size") before it. VideoToolbox decodes AV1 in hardware only on M3 and newer Apple chips; on M1
and M2 the AV1 decoder falls back to software (dav1d) this way. H.264 and VP9 use VideoToolbox on
every Apple silicon Mac.

**`frame_at(target)`** returns the frame with the largest pts ≤ `target` (the first frame when
`target` precedes it), converting it to RGBA only when it changed. It seeks backwards when
`target` is before the current frame, and forwards when `target` is more than 2000 ms past the
next frame (or past 2000 ms on a fresh decoder); at the end of the stream it holds the last frame.
Timestamps are relative to the stream's start time.

**Colour.** swscale converts to sRGB RGBA8 with alpha 255 and packed rows, using the stream's
matrix (BT.709, BT.601 or BT.2020; unspecified is BT.709 from 720 rows up, else BT.601) and its
range (limited or full).

**Audio.** The best audio stream, resampled to packed stereo `f32` at its own rate. `num_frames`
comes from the stream duration, else the container's. After the end of the stream `decode`
returns silent chunks of 1024 frames rather than none (kira loops forever on an empty chunk);
`seek(index)` is sample exact.

**Building.** `ffmpeg-sys-next` finds FFmpeg (up to 9.0) through `FFMPEG_DIR`, vcpkg or
pkg-config and runs bindgen, which needs libclang (`LIBCLANG_PATH`). The dev shell provides
`ffmpeg-headless` and `LIBCLANG_PATH`; Debian/Ubuntu need `libavcodec-dev libavformat-dev
libavutil-dev libswscale-dev libswresample-dev libclang-dev`. Releases ship a minimal LGPL FFmpeg
built by `packaging/ffmpeg/build.sh` ([Releases](../process/releases.md#ffmpeg)).

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

## The debug channel

`--debug-port PORT` and `--debug-stdio` (windowed only; they conflict with `--headless` and with
each other) open a channel for a debugger ([The Debugger](./debugger.md)). Without them nothing
listens.

- `--debug-port PORT` listens on `127.0.0.1:PORT` (0 picks a free port) on a background thread,
  logs `debugger: listening on <address>` under `kagari::debug`, and hands each accepted client to
  the frame loop; one debugger is attached at a time, and the next waits until it detaches. A
  port that cannot be bound exits with status 2.
- `--debug-stdio` talks to one debugger over standard input and output (how `kagari dap` and
  `kagari debug --window` launch a game); the runtime writes nothing else to stdout.

`DebugPort::open` runs right after the boot. When `game.kagari-cache` in the game directory
belongs to this `game.kagari` (a debug build), it turns on the engine's call recording
(`Engine::debug_record_calls`), so games started from then on can be hot swapped
([The Debugger](./debugger.md#hot-swap)); the artifact is kept for the first debugger.

`DebugPort::poll` runs at the start of every frame, before `Engine::update`: it attaches a waiting
debugger (with that artifact, or `game.kagari-cache` read again when its script hash matches, else
with a warning that `nav`, local names and hot swap are unavailable) and polls the live
`kagari_debugger::Server`, which applies the requests and reports the engine's own stops; the
server reloads the game directory for `kagari/swap`. A stop inside code blocks the frame loop
until the debugger goes on, so the window does not redraw meanwhile.

## Platform notes

| Platform | Notes |
| -------- | ----- |
| Linux | Wayland or X11 (`--backend`); Vulkan or OpenGL (EGL) through wgpu; audio through ALSA (cpal); gamepads through udev (gilrs). Data directory `$XDG_DATA_HOME/Kagari/<title>` |
| macOS | Apple silicon (arm64) only, macOS 11.0 or newer (`MACOSX_DEPLOYMENT_TARGET`); Metal; data directory `~/Library/Application Support/Kagari/<title>` |
| Windows | DirectX 12 (or Vulkan); data directory `%APPDATA%\Kagari\<title>` |

**System dependencies.** `flake.nix` provides the dev shell on NixOS (`direnv allow` or
`nix develop`); Rust itself comes from rustup (`rust-toolchain.toml`).

| Library | Why |
| ------- | --- |
| `alsa-lib` | linked at build time by `alsa-sys` (cpal) |
| `udev` | linked at build time by `libudev-sys` (gilrs) |
| `wayland`, `libxkbcommon`, `libX11`, `libXcursor`, `libXi`, `libXrandr` | loaded at run time by winit |
| `vulkan-loader`, `libGL` | loaded at run time by wgpu |
| `ffmpeg-headless` | linked by `ffmpeg-sys-next` (`kagari-video`) |
| `libclang` (`LIBCLANG_PATH`), `bindgenHook` in the package | bindgen in `ffmpeg-sys-next` |
| `pkg-config` | finds the build-time libraries |

The shell puts the run-time libraries and FFmpeg on `LD_LIBRARY_PATH`. Other Linux distributions
need the equivalent development packages (Debian: `libasound2-dev libudev-dev pkg-config` and the
FFmpeg and libclang packages above). The released Linux FFmpeg links VAAPI, so the runtime needs
`libva.so.2` and `libva-drm.so.2` at run time (Debian/Ubuntu `libva2 libva-drm2`, Fedora `libva`)
and does not start without them. Windows and macOS builds need FFmpeg (`FFMPEG_DIR`) and libclang.
On macOS, build FFmpeg with `KAGARI_FFMPEG_TARGET=macos packaging/ffmpeg/build.sh <prefix>` and
point `FFMPEG_DIR` at the prefix ([Releases](../process/releases.md#ffmpeg)); libclang comes from
the Xcode command line tools and is found without `LIBCLANG_PATH`.

**Logging.** Besides module targets, the engine logs progress under fixed targets that are useful
when reading headless runs: `kagari::system` (`modes: [...]` whenever the mode stack changes),
`kagari::story` (`step <step id> say|choice`, e.g. `step crate::Start#0 say`; `chose <choice ID>`, `story returned`),
`kagari::saves` (`saved`, `loaded … (n choices replayed)`, divergence warnings),
`kagari::video` (movie decoding backend, hardware fallback) and
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
- A new video codec or container: `kagari-compat` (`sniff`, `supports`), the decoders, demuxers,
  parsers and hwaccels in `packaging/ffmpeg/build.sh`, `kagari-video`'s decoder choice and the
  format tables (en and ja). A different FFmpeg major version changes the shipped sonames, dylib
  and DLL names: `Platform::libraries` (every platform's list, macOS included), `installer.sh`
  (`LINUX_FFMPEG_LIBS` and `MACOS_FFMPEG_LIBS`), `kagari.iss` and the release workflow.
- The logo: regenerate `resources/kagari.ico` and `resources/kagari.icns` (above).
- A new import from the MSVC redistributable (`VCRUNTIME140_1.dll`, `MSVCP140.dll`, …) on
  Windows: `Platform::libraries` in `kagari-cli` and the `dumpbin` check in the release workflow
  ([Releases](../process/releases.md#the-workflow)).
- Verify with `cargo test -p kagari-desktop-runtime` and a headless run of `example/test_game1`.
