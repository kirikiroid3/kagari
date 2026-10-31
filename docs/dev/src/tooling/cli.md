# Projects and the CLI

The `kagari` binary is a thin driver. Every command loads a project through `kagari-project`,
builds a fresh `RootDatabase` over the real file system, calls one compiler entry point, and maps
the outcome to output and an exit code. No compiler logic lives in the CLI.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `tooling/kagari-project/src/lib.rs` | `MANIFEST_FILE` (`kagari.toml`), `TOOLCHAIN_VERSION` (the crate version) and re-exports |
| `tooling/kagari-project/src/manifest.rs` | `RawManifest` (serde) → validated `Manifest`; `parse_manifest` |
| `tooling/kagari-project/src/project.rs` | `Project::{load, find, entry_path, asset_dir, build_dir, out_dir, program}` |
| `tooling/kagari-project/src/target.rs` | `Target` (`v0`) |
| `tooling/kagari-project/src/fonts.rs` | `system_font_dirs()` per platform |
| `tooling/kagari-project/src/error.rs` | `ProjectError` |
| `tooling/kagari-project/src/tests.rs` | Manifest and discovery unit tests |
| `tooling/kagari-cli/src/cli.rs` | clap definitions (`Cli`, `Command`) |
| `tooling/kagari-cli/src/main.rs` | Dispatch and the exit-code mapping |
| `tooling/kagari-cli/src/failure.rs` | `Failure` (exit 2), `io()`, `exit(failed)` (exit 0/1) |
| `tooling/kagari-cli/src/workspace.rs` | `load(--manifest-path)`, `shown(path, base)` |
| `tooling/kagari-cli/src/analysis_cache.rs` | `path`, `offer`, `store`: the analysis cache shared by `check` and `build` |
| `tooling/kagari-cli/src/outputs.rs` | `BYTECODE_FILE`, the game files of a target directory, the release directory, removing build outputs, images and caches, removing empty directories |
| `tooling/kagari-cli/src/command/{check,clean,disasm,fmt,lsp}.rs` | One module per subcommand |
| `tooling/kagari-cli/src/command/{debug,dap}.rs`, `src/debugger.rs` | `kagari debug` (the REPL) and `kagari dap` over `kagari-debugger`'s `client` and `dap` features; `debugger.rs` opens projects for them (`Projects`, the adapter's `Launcher`); see [The Debugger](../runtime/debugger.md) |
| `tooling/kagari-cli/src/command/build/{mod,archives,compat,shaders}.rs` | `kagari build` and its profiles, archive packing, runtime-profile check, shader check |
| `tooling/kagari-cli/src/command/release/{mod,download,iso}.rs` | `kagari release`: `Platform`, the runtime download (`DOWNLOAD_URL`, checksum), the ISO 9660 writer |
| `tooling/kagari-cli/src/command/init/{mod,template,font}.rs` | `kagari init`, embedded template, default-font choice |
| `tooling/kagari-cli/template/` | The new-project template (`_gitignore` becomes `.gitignore`) |
| `tooling/kagari-cli/tests/` | Integration tests running the built binary |

Users see this as the Reference chapters [Projects](https://kagari.suitsu31.club/en/reference/tools/projects/)
and [The `kagari` CLI](https://kagari.suitsu31.club/en/reference/tools/cli/).

## `kagari-project`

### Manifest

`parse_manifest(text) -> Result<Manifest, String>` deserializes into private `Raw*` structs with
`#[serde(deny_unknown_fields, rename_all = "kebab-case")]`, so unknown keys and misspellings are
errors and TOML keys are kebab-case. It then validates and fills defaults. The error string does
not name the file; `Project::load` wraps it as `ProjectError::Manifest { path, message }`.

| Key | Type | Default | Validation | `Manifest` field |
| --- | ---- | ------- | ---------- | ---------------- |
| `project.name` | string | required | non-empty | `name` |
| `project.version` | string | required | `semver::Version` | `version` |
| `project.entry` | string | `src/main.kis` | relative; extension `kis` or `kix` | `entry` |
| `toolchain.kagari` | string | required | `semver::VersionReq` | `toolchain` |
| `assets.dir` | string | `assets` | relative | `asset_dir` |
| `assets.system-fonts` | bool | `true` | — | `options.system_fonts` |
| `build.targets` | string array | `["v0"]` | non-empty; each a `Target`; duplicates dropped | `targets` |
| `build.runtime-compatibility` | string array | `["native_v0"]` | non-empty; each a `kagari_compat::RuntimeProfile`; duplicates dropped | `runtime_compatibility` |
| `build.out-dir` | string | `target` | relative | `out_dir` |
| `build.state-cap` | u32 | `DEFAULT_STATE_CAP` (200 000) | — | `options.state_cap` |
| `build.recursion-depth` | u32 | `DEFAULT_RECURSION_DEPTH` (2048) | — | `options.recursion_depth` |
| `build.keyframe-check` | bool | `false` | — | `options.keyframe_check` |

"Relative" means neither absolute nor rooted (`relative()` rejects both, so `\x` on Windows is
rejected too). The compile options end up in `kagari_lang::CompileOptions`, which is part of the
`Program` salsa input; see [Pipeline and Queries](../compiler/pipeline.md).

### Loading and discovery

- `Project::load(manifest_path)` reads and parses the manifest, canonicalizes its directory into
  `root` (a bare `kagari.toml` means `.`), and checks `manifest.toolchain` against
  `TOOLCHAIN_VERSION` (`CARGO_PKG_VERSION` of `kagari-project`, i.e. the workspace version). A
  mismatch is `ProjectError::Toolchain`.
- `Project::find(start)` walks `start.ancestors()` and loads the first `kagari.toml` it finds;
  none is `ProjectError::NotFound`. The CLI starts at the current directory; the language server
  starts at an opened file's directory.
- Every manifest path is resolved against `root`: `entry_path()`, `asset_dir()`,
  `build_dir() = root/<out-dir>` and `out_dir(target) = root/<out-dir>/<target>` (e.g. `target/v0`).

### The project's program

`Project::program(&RootDatabase) -> Program` calls `db.program_with_assets(entry, asset_dir,
font_dirs, options)`. `asset("…")` paths resolve against the asset directory, and
`system_font("…")` searches `system_font_dirs()` in order, unless `assets.system-fonts = false`
(then the list is empty and the compiler reports `E1031`).

`system_font_dirs()` returns the existing directories among, user's first:

| Platform | Directories |
| -------- | ----------- |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%WINDIR%\Fonts` (fallback `%SystemRoot%`, then `C:\Windows`) |
| macOS | `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts` |
| Others | `$XDG_DATA_HOME/fonts` (default `~/.local/share/fonts`), `~/.fonts`, `fonts` in each of `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`) |

Relative environment values are ignored, as the XDG specification requires. The CLI tests rely on
this: they point `HOME`, `XDG_DATA_HOME` and `XDG_DATA_DIRS` at `tests/font_home` so that exactly
one system font (`TestSans.ttf`) exists.

### Targets

`Target` is the engine major version a build is for. It has one variant, `V0` (`"v0"`). `ALL`,
`as_str` and `parse` are the only places that list names; manifest validation and error messages
use `ALL`. Today every target gets the same bytes; the type exists so that `build.targets` and the
output layout (`<out-dir>/<target>/`) are stable when a second engine version appears.

## `kagari-cli`

### Structure

`main.rs` parses `Cli` with clap and calls `command::<name>::run(…)`, which returns
`Result<ExitCode, Failure>`. Each `run` is a function, not a trait object; arguments are passed
as plain values.

```rust
match result {
    Ok(code) => code,
    Err(Failure(message)) => { eprintln!("kagari: {message}"); ExitCode::from(2) }
}
```

`workspace::load(manifest_path)` is the one way commands find a project: `Project::load` for
`--manifest-path`, else `Project::find(current_dir)`. `workspace::shown` prints paths relative to
the project root with `/` separators (it reuses `FilePath::display`).

### Exit codes

| Code | Meaning | Produced by |
| ---: | ------- | ----------- |
| 0 | Success (warnings allowed) | `ExitCode::SUCCESS`, `exit(false)` |
| 1 | The command ran and found problems | `exit(true)`: error diagnostics (`check`, `build`), an asset a runtime profile cannot decode (`build`), files that would change or cannot be formatted (`fmt`), a file that fails to decode or verify (`disasm`) |
| 2 | The command could not run | `Failure`: no or invalid manifest, toolchain mismatch, I/O error, `--target` not in `build.targets`, `init` over existing files, `fmt` on a non-`.kis`/`.kix` path, LSP protocol error, `debug` when the debug build fails or the game cannot be reached, `release` when the runtime cannot be downloaded or fails its checksum or a file name does not fit the image; also clap's own usage errors |

Keep this split: 1 means "your game has a problem", 2 means "the tool could not do its job".
Scripts and CI rely on it.

### `kagari check`

```text
kagari check [--manifest-path PATH] [--resolutions]
```

Loads the project, offers the analysis cache (see [The analysis cache](#the-analysis-cache)), runs
`kagari_lang::check(db, program)` (every stage, including analysis when stages 1–4 had no error),
stores the cache, and prints `render_diagnostics(…, Some(&project.root))` to **stdout**
(`no diagnostics` when empty). `--resolutions` appends `debug::resolution_dump`, which prints what
every name resolves to; it is a compiler debugging aid. Exit 1 iff any diagnostic has
`Severity::Error`.

### The analysis cache

`check` and `build` share the story analysis of the previous run through
`<out-dir>/analysis.kagari-cache` (`analysis_cache.rs`; format in
[`.kagari-cache` Files](../formats/cache.md)):

1. `offer`: read the file and `kagari_lang::cache::offer_analysis_cache`. A missing, unreadable,
   invalid or refused file is ignored.
2. The command runs `check` or `build` as usual; the compiler reuses the offered diagnostics when
   the program's source fingerprint equals the file's, and otherwise analyzes
   ([Reusing the analysis](../compiler/analysis.md#reusing-the-analysis)).
3. `store`: `kagari_lang::cache::analysis_cache_use` says `NotRun` (nothing to do), `Reused` (the
   offered cache is this run's analysis) or `Fresh(cache)`: write it, creating the output
   directory. A file that cannot be encoded or written prints
   `kagari: warning: cannot write the analysis cache: …` to stderr; the exit code does not change.

The cache is written whenever the analysis runs, also by a `build` that later fails (for
example in emission or the runtime-profile check): it is a cache, not a build output.

### `kagari build`

```text
kagari build [--manifest-path PATH] [--target TARGET]... [--profile debug|release]
```

1. Load the project; choose targets: every `--target` must parse **and** be listed in
   `build.targets` (else exit 2); without `--target`, all of `build.targets`. `--profile`
   (`cli::Profile`) defaults to `release`.
2. Offer the analysis cache; `kagari_lang::build(db, program)` returns `Build { diagnostics, module, bytes, asset_files, shader_roles, debug }`:
   the diagnostics of `check` plus emission, the verified `kagari_bytecode::Module` and its
   encoding (both `None` on any error), the `FilePath` behind each `module.assets` entry, and
   the `kagari_debug::DebugInfo` of the module. Store the analysis cache. Non-empty diagnostics
   are printed to **stderr**. No module → exit 1.
3. **Runtime-profile check** (`build/compat.rs`), before anything is written: for each asset and
   each profile in `build.runtime-compatibility`, read at most 1 MiB (for an ISO BMFF file,
   `ftyp` first, instead `kagari_compat::read_iso_bmff_head`: the top-level boxes except `mdat`
   and free space, up to 16 MiB, since encoders write `moov` after the media data),
   `kagari_compat::sniff` the format by content (never by extension), and print one line per
   unsupported kind (`supports_kind`) or format (`supports`):

   ```text
   error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
   error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
   ```

   Any such line → exit 1, and the output directory is neither created nor cleaned. The format
   tables per profile live in `runtime_libs/kagari-compat`; the user-facing list is in the
   Reference's [Runtime profiles](https://kagari.suitsu31.club/en/reference/tools/projects/#runtime-profiles).
   `native_v0` accepts shaders whose content sniffs as UTF-8 text (`AssetFormat::Wgsl`).
4. **Shader check** (`build/shaders.rs`): `shader_roles` is parallel to `asset_files`; for each
   asset with `Some(role)`, read the whole source (disk, or `kagari_std::file` for
   `FilePath::Std`) and run `kagari_shader::validate(role, &src)` (feature `validate`, naga). Each
   error prints `error: shader `{path}` ({role.name()}): {ShaderError}` (`line N: message`, line
   relative to the user file). Any error → exit 1, nothing written. A file used under two roles
   has two assets, so it is validated once per role.
5. **Debug profile only:** encode the debug artifact once (`DebugArtifact`): the toolchain
   version, the source fingerprint and story graph of this run's analysis (fresh or reused), the
   script hash (SHA-256 of `bytes`, lowercase hex, as the engine computes it) and `Build::debug`.
6. For each target: create `out_dir(target)`, delete the previous `game.kagari`, every
   `*.kiarchive` and every `*.kagari-cache` in it (`outputs::remove_target_outputs`; other files
   are left alone), write `game.kagari`, the archives, then (debug profile) `game.kagari-cache`.

The profile changes nothing in the compilation: `game.kagari` is byte-identical in both, and a
release build removes the artifact an earlier debug build left.

Archive packing (`build/archives.rs`) groups the referenced assets by
`kagari_archive::archive::archive_stem(path)`: the first `/`-separated segment of the asset path
**as stored in the bytecode**, or `data` for a path without `/`. Each group becomes
`<stem>.kiarchive`, built with `ArchiveBuilder::base(ArchiveOptions::default())`; entry names are
the bytecode asset paths, so the runtime looks assets up by exactly the string the program uses.
Consequences:

- Only assets the bytecode references are packed. An asset directory with no referenced file gets
  no archive.
- System fonts have bytecode paths `system-fonts/<path under the font dir>`, so they land in
  `system-fonts.kiarchive`. This is why the compiler reserves a top-level `system-fonts`
  directory in the asset directory (`E1032`).
- Std assets (the `std::shaders` WGSL files) have `FilePath::Std(name)` and bytecode paths
  `std/<name>`, so they land in `std.kiarchive`; `archives::source` returns `Source::Std(text)`
  from `kagari_std::file`, added with `ArchiveBuilder::add`. The compiler reserves a top-level
  `std` asset directory too (`E1032`, `STD_ASSETS_DIR`).
- Every other asset file must be on disk (`Source::Disk`); a missing std file is a `Failure`.

The archive format itself is described in [Archives and Save Databases](../formats/archive.md),
the bytecode in [The `.kagari` Format](../formats/kagari.md).

### `kagari release`

```text
kagari release [--manifest-path PATH] (--windows | --linux)
```

Exactly one platform flag (a required clap `ArgGroup`); `release::Platform` names the download
directory, the runtime's file name there and in the image, and the libraries shipped next to the
runtime.

1. Load the project and take the first of `build.targets`. `build::build_project` builds it in
   the release profile, the same function `kagari build` calls; problems → exit 1, before any
   download.
2. `download::runtime`, then `download::file` for each of `Platform::libraries`:
   `GET <base>/<bucket dir>/<TOOLCHAIN_VERSION>/<file>` and its `.sha256` with `ureq` (rustls,
   webpki roots), where `<base>` is `DOWNLOAD_URL` (`https://kagari-download.suitsu31.club`)
   unless `KAGARI_DOWNLOAD_URL` is set (as for `installer.sh`; the tests point it at a local
   server). The bucket layout and the names are the release workflow's
   ([Releases](../process/releases.md#the-bucket)); the files are always those of this
   toolchain's version. HTTP 404 says the platform's runtime is not published; a checksum
   mismatch, any other HTTP or network error → `Failure` (exit 2), and no image is written. The
   body limit is 1 GiB. The Windows runtime imports `VCRUNTIME140.dll`, which a clean Windows
   lacks; Windows loads DLLs from the executable's directory first, so the image carries
   `vcruntime140.dll` (the library list is `["vcruntime140.dll"]` on Windows, empty on Linux).
3. Create `<out-dir>/release/<platform>/`, delete the `*.iso` files in it, and write
   `<name>.iso.part`, renamed to `<name>.iso` when complete (removed on failure). `<name>` is
   `project.name` with `/ \ : * ? " < > |` and controls replaced by `_`, surrounding whitespace
   and trailing dots removed (`Kagari Game` when empty): the rule of the runtime's data
   directory, so the name is valid on every platform. The image holds the runtime (`run.exe` on
   Windows, `kagari-desktop-runtime` on Linux; the runtime finds the game in its own directory
   whatever its name), the libraries, and `outputs::game_files` of the target directory
   (`game.kagari`, `*.kiarchive`; not the debug artifact).

The image (`release/iso.rs`) is ISO 9660 with one flat root directory, written in one pass
after a sizing pass (directory sizes depend only on record lengths):

| Sectors | Contents |
| ------- | -------- |
| 0–15 | System area, zeros |
| 16, 17, 18 | Primary volume descriptor, Joliet supplementary descriptor (escape `%/E`, UCS-2 level 3), terminator |
| 19–22 | Path tables (one root record each): primary L and M, Joliet L and M |
| 23 | Rock Ridge continuation area: the `ER` entry (`RRIP_1991A`) |
| 24… | Primary root directory, then the Joliet root directory, then each file from a sector boundary |

- **Primary tree**: level 1 identifiers (`NAME.EXT;1`, d-characters, made unique with `_N`),
  for readers without extensions. Its `.` record carries `SP`, `CE` (to the `ER`) and `PX`;
  every file carries `PX` (mode `0555` for the runtime, `0444` otherwise, owner 0) and `NM`
  (the real name, UTF-8, at most 165 bytes). Linux reads this tree: Rock Ridge permissions
  override the `fmode=` that udisks passes, so the runtime stays executable on a mounted image.
- **Joliet tree**: the real names, UTF-16 big-endian with `;1`, at most 64 code units; no
  control characters or `* / : ; ? \`. Windows reads this tree.
- Volume label: the project name (Joliet, 16 code units, with the characters Joliet forbids in
  names replaced by `_`) and its d-characters (primary).
  Timestamps are the time of writing, UTC.

### `kagari clean`

```text
kagari clean [--manifest-path PATH]
```

Deletes what `check`, `build` and `release` write and nothing else: the `*.kagari-cache` files
directly in `build_dir()` (the analysis cache), in `out_dir(target)` for **every** `Target::ALL`
(listed in `build.targets` or not) the `game.kagari`, `*.kiarchive` and `*.kagari-cache` files,
and the `*.iso` files in `release/<platform>/` for every `Platform::ALL`. Then it removes each
target and release directory and the output directory if they are empty. It prints
`removed N files from <out-dir>` and exits 0; an I/O error is a `Failure` (exit 2). It never
deletes the output directory wholesale, because `build.out-dir` may name a directory that holds
other files.

### `kagari fmt`

```text
kagari fmt [--check] [--manifest-path PATH] [FILES]...
```

Without `FILES`, it loads the project and collects every `.kis`/`.kix` file under the **entry
file's directory** (not the project root), skipping directories whose names start with `.`,
sorted. With `FILES`, no project is loaded. For each file it calls `kagari_fmt::format(text,
kind)`:

| Result | Without `--check` | With `--check` |
| ------ | ----------------- | -------------- |
| Unchanged | nothing | nothing |
| Changed | rewrite, print `formatted PATH` | print `would reformat PATH`, mark failed |
| `FormatError::Syntax` | print the error count and the first error's `line:col`, mark failed | same |
| `FormatError::Internal` | print the message, mark failed | same |

A path without a `.kis`/`.kix` extension or an unreadable file is a `Failure` (exit 2) and stops
the run. Otherwise exit 1 iff any file was marked failed. The formatter is described in
[Formatter](./fmt.md).

### `kagari disasm`

```text
kagari disasm FILE [--all]
```

Reads the file, `kagari_bytecode::decode`s and `verify`s it (either failure prints to stderr,
exit 1), and prints `kagari_bytecode::disassemble(&module, &filter)`. The filter keeps functions
whose path starts with `crate::` unless `--all` is given, which also prints std functions. It
needs no project.

### `kagari lsp`

Calls `kagari_lsp::run_stdio()`; see [Language Server and Editors](./lsp.md). A protocol or I/O
error becomes a `Failure`.

### `kagari debug` and `kagari dap`

```text
kagari debug [--manifest-path PATH] [--target TARGET] [--entry PATH] [--seed N] [--no-build] [--watch]
kagari debug --window [--runtime PATH] … | --attach ADDRESS …
kagari dap
```

Both are front ends of the story debugger ([The Debugger](../runtime/debugger.md)). `debug`
resolves the project and the target directory (`debugger::open`), runs `kagari build
--profile debug` as a child process of the same executable (its output is printed; `dap` sends it
as `output` events, since its stdout is the protocol), then connects a backend: the headless
query engine on a thread of this process (`kagari_debugger::headless::serve` over an in-process
`Link`), `kagari-desktop-runtime DIR --debug-stdio` (next to `kagari`, else on `PATH`), or a TCP
connection to `--debug-port`. The source root is the root file's directory, which the
bytecode's source file names are relative to; `Projects` offers `open` to the adapter as its
`Launcher`. Locations, line breakpoints and the commands as text come from
`kagari_debugger::client`. The REPL exits 0 at the end of its input or on `quit`; `dap` exits
when the editor disconnects. The REPL's `swap` and `--watch` (and the adapter's `kagari/swap` and
`watch`) rebuild through the same `debugger::open` and hot swap the game; `Opened` names the
project root to watch and its output directory to skip.

### `kagari init`

```text
kagari init [PATH] [--name NAME]
```

1. Create `PATH` (default `.`) and canonicalize it. The name defaults to the directory name; an
   empty or non-UTF-8 name is a `Failure`.
2. Refuse (exit 2) if **any** template file already exists; `init` never overwrites.
3. Write every entry of `template::FILES` (embedded with `include_str!`/`include_bytes!`).
   `kagari.toml` and `src/main.kis` are rendered:
   - `"{{name}}"` becomes a TOML basic string in the manifest and a Kagari string literal in
     `main.kis` (both escaped; see `toml_string`, `kagari_string`);
   - `{{toolchain}}` becomes `major.minor` of `TOOLCHAIN_VERSION` (`0.1`);
   - `{{font}}` becomes `system_font("<path>")` for the chosen default font, or
     `asset("fonts/default.ttf")` with a warning on stderr when no font is installed.

The default font (`init/font.rs`) is the first `PREFERRED` file name found (case-insensitive;
Japanese gothic faces first, then common sans-serif faces) under `system_font_dirs()`, searched up
to 6 directory levels deep, else the first font file (`ttf`, `otf`, `ttc`, `otc`) in path order.
The resulting project must check, build and format cleanly; the integration tests assert exactly
that.

## Tests

| Test | What it covers |
| ---- | -------------- |
| `tooling/kagari-project/src/tests.rs` | Defaults, unknown keys, unknown/duplicate/empty targets and profiles, absolute paths, toolchain check, parent-directory discovery |
| `tooling/kagari-cli/tests/init.rs` | Template files, refusal to overwrite, name escaping, system-font choice and the no-font fallback |
| `tooling/kagari-cli/tests/check.rs` | Fresh project has no diagnostics (also from a subdirectory), assets outside the asset dir, `system-fonts = false`, missing manifest (2), toolchain mismatch (2) |
| `tooling/kagari-cli/tests/build.rs` | Bytecode decodes and verifies, archive names and contents, stale archive removal, unlisted target (2), rejected image/audio/video formats (1, no bytecode written), invalid WGSL (1, `error: shader … line`), std shaders packed into `std.kiarchive`, `disasm` of the result |
| `tooling/kagari-cli/tests/debug.rs` | A debug build writes `game.kagari-cache` (script hash, one debug entry per function, every reached step is a bytecode step) and the same bytecode as a release build, which removes the artifact; `check` and `build` reuse a cached analysis (a diagnostic only the cache holds is printed) until a source edit, and replace an invalid cache; `clean` removes outputs and caches only |
| `tooling/kagari-cli/tests/debugger.rs` | `kagari debug` on a story with scene calls, a guarded choice and a draw: `ls`, `nav` and the `push` that follows it, `goto`, `stack`, `pop`, `detail`, `seed`; breakpoints by step ID and by `file:line`; `swap` after a source edit rebuilds and moves the story onto the new sources |
| `tooling/kagari-cli/tests/dap.rs` | `kagari dap` end to end over DAP: launch headless, breakpoints by line, stack trace, `kagari/push` and `kagari/nav`, `evaluate`, continue to a breakpoint; `kagari/swap` rebuilds, calls the changed scene again and sets the breakpoints again |
| `tooling/kagari-cli/tests/fmt.rs` | Fresh project is formatted, rewriting, syntax errors |
| `tooling/kagari-cli/tests/release.rs` | `kagari release` against a local download server (`KAGARI_DOWNLOAD_URL`): the image's Joliet and Rock Ridge trees hold the runtime (executable) and the game files byte for byte, for Linux, and `run.exe` with `vcruntime140.dll` for Windows; `clean` removes the image; an unpublished runtime or C runtime DLL and a checksum mismatch (2, no image); build errors (1, no download) |

The integration tests run the real binary (`env!("CARGO_BIN_EXE_kagari")`) through
`tests/common/mod.rs`, inside a temp dir created by `kagari init`. Run them with
`cargo test -p kagari-project -p kagari-cli`.

## Invariants

- The CLI holds no compiler logic: diagnostics come from `kagari_lang::check`/`build`, formatting
  from `kagari_fmt`, bytecode handling from `kagari_bytecode`. The language server must see the
  same project as the CLI, so both go through `Project::find`/`load` and `Project::program`.
- Exit codes: 0 success, 1 problems found, 2 cannot run.
- `kagari build` writes no build output when compilation or the runtime-profile check fails (the
  analysis cache is written whenever the analysis ran).
- The bytecode does not depend on the build profile.
- An analysis cache is used only for the inputs it was computed from (the compiler compares
  source fingerprints); the CLI never interprets it.
- `kagari clean` deletes only files the CLI writes.
- Archive entry names equal bytecode asset paths, and archives are grouped by `archive_stem`. The
  runtime depends on both.
- Manifest parsing stays strict (`deny_unknown_fields`) and every path key stays relative to the
  project root.
- `kagari init` output passes `check`, `build` and `fmt --check` with exit 0.

## When you change this

### Adding a manifest key

1. Add the field to the right `Raw*` struct in `manifest.rs` (kebab-case in TOML comes from
   `rename_all`; make it `Option` unless it is required).
2. Validate it in `parse_manifest` with an error that names the key in backticks
   (`` `build.out-dir` must be a relative path ``), and store it in `Manifest` (or in
   `CompileOptions` when the compiler needs it; then also add the field and its default constant
   in `core/kagari-lang/src/input.rs`).
3. Add unit tests in `kagari-project/src/tests.rs` for the default and for each rejection.
4. If new projects should set it, add it to `tooling/kagari-cli/template/kagari.toml`.
5. Document it in the Reference: `docs/site/src/content/docs/en/reference/tools/projects.md` **and**
   `docs/site/src/content/docs/ja/reference/tools/projects.md` (same table rows, same code blocks).

### Adding a subcommand or flag

1. Add the variant/field to `Command` in `cli.rs`. Its doc comments are the `--help` text: one
   sentence, imperative, ending with a period.
2. Add `command/<name>.rs` (or a directory for several files) with
   `pub fn run(…) -> Result<ExitCode, Failure>`, register it in `command/mod.rs`, and dispatch it
   in `main.rs`.
3. Use `workspace::load` if it needs a project, `failure::io` for I/O errors, and `exit(failed)`
   for problem-found results. Print user-facing paths with `workspace::shown`.
4. Add an integration test in `tooling/kagari-cli/tests/<name>.rs` using `tests/common`.
5. Document it in `docs/site/src/content/docs/en/reference/tools/cli.md` and `docs/site/src/content/docs/ja/reference/tools/cli.md`
   (synopsis, flags, exit codes), and update the command list in this chapter.

### Other changes

- New `Target` or `RuntimeProfile`: extend `ALL`, `as_str`, `parse`; for a profile also its
  `supports*` tables in `kagari-compat`. Update the Reference project page (en+ja).
- Changing the output layout (`game.kagari`, archive naming): the desktop runtime reads it; update
  `runtime/kagari-desktop-runtime`, [The Desktop Runtime](../runtime/desktop.md), and the
  Reference [The Runtime](https://kagari.suitsu31.club/en/reference/tools/runtime/) page (en+ja).
- Changing the template: run `kagari init` in a temp dir and make sure `check`, `build` and
  `fmt --check` exit 0 (the integration tests do this).
