# Projects and the CLI

The `kagari` binary is a thin driver. Every command loads a project through `kagari-project`,
builds a fresh `RootDatabase` over the real file system, calls one compiler entry point, and maps
the outcome to output and an exit code. No compiler logic lives in the CLI.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `tooling/kagari-project/src/lib.rs` | `MANIFEST_FILE` (`kagari.toml`), `TOOLCHAIN_VERSION` (the crate version) and re-exports |
| `tooling/kagari-project/src/manifest.rs` | `RawManifest` (serde) → validated `Manifest`; `parse_manifest` |
| `tooling/kagari-project/src/project.rs` | `Project::{load, find, entry_path, asset_dir, out_dir, program}` |
| `tooling/kagari-project/src/target.rs` | `Target` (`v0`) |
| `tooling/kagari-project/src/fonts.rs` | `system_font_dirs()` per platform |
| `tooling/kagari-project/src/error.rs` | `ProjectError` |
| `tooling/kagari-project/src/tests.rs` | Manifest and discovery unit tests |
| `tooling/kagari-cli/src/cli.rs` | clap definitions (`Cli`, `Command`) |
| `tooling/kagari-cli/src/main.rs` | Dispatch and the exit-code mapping |
| `tooling/kagari-cli/src/failure.rs` | `Failure` (exit 2), `io()`, `exit(failed)` (exit 0/1) |
| `tooling/kagari-cli/src/workspace.rs` | `load(--manifest-path)`, `shown(path, base)` |
| `tooling/kagari-cli/src/command/{check,disasm,fmt,lsp}.rs` | One module per subcommand |
| `tooling/kagari-cli/src/command/build/{mod,archives,compat}.rs` | `kagari build`, archive packing, runtime-profile check |
| `tooling/kagari-cli/src/command/init/{mod,template,font}.rs` | `kagari init`, embedded template, default-font choice |
| `tooling/kagari-cli/template/` | The new-project template (`_gitignore` becomes `.gitignore`) |
| `tooling/kagari-cli/tests/` | Integration tests running the built binary |

Users see this as the Reference chapters [Projects](../../../reference/en/src/tools/projects.md)
and [The `kagari` CLI](../../../reference/en/src/tools/cli.md).

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
- Every manifest path is resolved against `root`: `entry_path()`, `asset_dir()`, and
  `out_dir(target) = root/<out-dir>/<target>` (e.g. `target/v0`).

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
| 2 | The command could not run | `Failure`: no or invalid manifest, toolchain mismatch, I/O error, `--target` not in `build.targets`, `init` over existing files, `fmt` on a non-`.kis`/`.kix` path, LSP protocol error; also clap's own usage errors |

Keep this split: 1 means "your game has a problem", 2 means "the tool could not do its job".
Scripts and CI rely on it.

### `kagari check`

```text
kagari check [--manifest-path PATH] [--resolutions]
```

Loads the project, runs `kagari_lang::check(db, program)` (every stage, including analysis when
stages 1–4 had no error) and prints `render_diagnostics(…, Some(&project.root))` to **stdout**
(`no diagnostics` when empty). `--resolutions` appends `debug::resolution_dump`, which prints what
every name resolves to; it is a compiler debugging aid. Exit 1 iff any diagnostic has
`Severity::Error`.

### `kagari build`

```text
kagari build [--manifest-path PATH] [--target TARGET]...
```

1. Load the project; choose targets: every `--target` must parse **and** be listed in
   `build.targets` (else exit 2); without `--target`, all of `build.targets`.
2. `kagari_lang::build(db, program)` returns `Build { diagnostics, module, bytes, asset_files }`:
   the diagnostics of `check` plus emission, the verified `kagari_bytecode::Module` and its
   encoding (both `None` on any error), and the `FilePath` behind each `module.assets` entry.
   Non-empty diagnostics are printed to **stderr**. No module → exit 1.
3. **Runtime-profile check** (`build/compat.rs`), before anything is written: for each asset and
   each profile in `build.runtime-compatibility`, read at most 1 MiB, `kagari_compat::sniff` the
   format by content (never by extension), and print one line per unsupported kind
   (`supports_kind`) or format (`supports`):

   ```text
   error: asset `bgimage/bad.jxl` (image): JPEG XL is not supported by runtime profile `native_v0`
   error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
   ```

   Any such line → exit 1, and the output directory is neither created nor cleaned. The format
   tables per profile live in `runtime_libs/kagari-compat`; the user-facing list is in the
   Reference's [Runtime profiles](../../../reference/en/src/tools/projects.md#runtime-profiles).
4. For each target: create `out_dir(target)`, delete the previous `game.kagari` and every
   `*.kiarchive` in it (other files are left alone), write `game.kagari`, then the archives.

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
- Every asset file must be on disk (`FilePath::Disk`); anything else is a `Failure`.

The archive format itself is described in [Archives and Save Databases](../formats/archive.md),
the bytecode in [The `.kagari` Format](../formats/kagari.md).

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
| `tooling/kagari-cli/tests/build.rs` | Bytecode decodes and verifies, archive names and contents, stale archive removal, unlisted target (2), rejected image/audio/video formats (1, no bytecode written), `disasm` of the result |
| `tooling/kagari-cli/tests/fmt.rs` | Fresh project is formatted, rewriting, syntax errors |

The integration tests run the real binary (`env!("CARGO_BIN_EXE_kagari")`) through
`tests/common/mod.rs`, inside a temp dir created by `kagari init`. Run them with
`cargo test -p kagari-project -p kagari-cli`.

## Invariants

- The CLI holds no compiler logic: diagnostics come from `kagari_lang::check`/`build`, formatting
  from `kagari_fmt`, bytecode handling from `kagari_bytecode`. The language server must see the
  same project as the CLI, so both go through `Project::find`/`load` and `Project::program`.
- Exit codes: 0 success, 1 problems found, 2 cannot run.
- `kagari build` writes nothing when compilation or the runtime-profile check fails.
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
5. Document it in the Reference: `docs/reference/en/src/tools/projects.md` **and**
   `docs/reference/ja/src/tools/projects.md` (same table rows, same code blocks).

### Adding a subcommand or flag

1. Add the variant/field to `Command` in `cli.rs`. Its doc comments are the `--help` text: one
   sentence, imperative, ending with a period.
2. Add `command/<name>.rs` (or a directory for several files) with
   `pub fn run(…) -> Result<ExitCode, Failure>`, register it in `command/mod.rs`, and dispatch it
   in `main.rs`.
3. Use `workspace::load` if it needs a project, `failure::io` for I/O errors, and `exit(failed)`
   for problem-found results. Print user-facing paths with `workspace::shown`.
4. Add an integration test in `tooling/kagari-cli/tests/<name>.rs` using `tests/common`.
5. Document it in `docs/reference/en/src/tools/cli.md` and `docs/reference/ja/src/tools/cli.md`
   (synopsis, flags, exit codes), and update the command list in this chapter.

### Other changes

- New `Target` or `RuntimeProfile`: extend `ALL`, `as_str`, `parse`; for a profile also its
  `supports*` tables in `kagari-compat`. Update the Reference project page (en+ja).
- Changing the output layout (`game.kagari`, archive naming): the desktop runtime reads it; update
  `runtime/kagari-desktop-runtime`, [The Desktop Runtime](../runtime/desktop.md), and the
  Reference [The Runtime](../../../reference/en/src/tools/runtime.md) page (en+ja).
- Changing the template: run `kagari init` in a temp dir and make sure `check`, `build` and
  `fmt --check` exit 0 (the integration tests do this).
