# Releases

**Where in the code**

- Version: `[workspace.package] version` in `Cargo.toml`; every crate uses `version.workspace = true`,
  and every workspace crate in `[workspace.dependencies]` repeats it in `version`.
- Crate registry: `[registries.Suitsu31]` in `.cargo/config.toml`, `publish` in
  `[workspace.package]` ([Crates](#crates)).
- CI: `.github/workflows/release.yml`.
- Linux and macOS installer: `installer.sh` (uploaded to the bucket root).
- Windows installer: `packaging/windows/kagari.iss` (Inno Setup).
- FFmpeg shipped with the runtime: `packaging/ffmpeg/build.sh`.
- Nix: `flake.nix` (`packages.{stable,unstable,default}`), `nix/stable.json` (written by CI).

Why there is no toolchain manager: [Installers, not a toolchain manager](../foundations/decisions.md#distribution).
How users install: the Tutorial's
[Installing Kagari](https://kagari.suitsu31.club/en/tutorial/install/).

## What a release is

A release is a git tag whose name, without a leading `v`, equals the workspace version (`v0.1.0`
or `0.1.0` for `0.1.0`). It ships two binaries from one build, `kagari` (crate `kagari-cli`) and
`kagari-desktop-runtime`. `kagari --version` prints the workspace version, which
`[toolchain] kagari` in a project's manifest is matched against. Nothing is uploaded to GitHub
Releases.

## The workflow

`release.yml` runs on every pushed tag and on manual dispatch. `linux`, `windows` and `macos`
run in parallel after `version`; `upload` waits for all three, so a failed build uploads nothing.
Every job runs on a [Blacksmith](https://docs.blacksmith.sh) runner; Blacksmith's colocated cache
backs `actions/cache` transparently, so the workflow uses the plain action.

| Job | Runs on | Does |
| --- | ------- | ---- |
| `version` | `blacksmith-2vcpu-ubuntu-2404` | Reads the workspace version. On a tag it fails unless the tag matches; on a branch the version is `<workspace>-dev.<short sha>` and `release` is false. A version below `FFMPEG_SINCE` in `installer.sh` fails a tag and warns on a branch: the installer would install that build without its FFmpeg libraries. |
| `linux` | `blacksmith-8vcpu-ubuntu-2404`, in an `ubuntu:22.04` container | Builds FFmpeg ([FFmpeg](#ffmpeg); cached), builds both binaries with GCC (`x86_64-unknown-linux-gnu`) against glibc 2.35 and that FFmpeg (`FFMPEG_DIR`), runs `--version` (the runtime with the FFmpeg libraries on `LD_LIBRARY_PATH`), packs `kagari-<v>-x86_64-linux.tar.gz` (a directory `kagari-<v>-x86_64-linux/` with both binaries, `LICENSE`, the five FFmpeg libraries and `ffmpeg-LICENSE.txt`) and its `.sha256`, and copies the runtime on its own as `kagari-desktop-runtime-<v>-x86_64-linux`, the FFmpeg libraries and `ffmpeg-LICENSE.txt`, each with its `.sha256`. |
| `windows` | `blacksmith-8vcpu-windows-2025` | Builds both binaries (MSVC, the C runtime linked dynamically: they import `VCRUNTIME140.dll`), runs `--version` (the runtime's through a pipeline: it is a GUI-subsystem program, which PowerShell otherwise does not wait for; see [The console on Windows](../runtime/desktop.md#the-console-on-windows)), stages `vc_redist.x64.exe` and `vcruntime140.dll` from the redistributable folder of the Visual Studio that linked them (found with `vswhere`; `Microsoft.VCRedistVersion.default.txt` names the folder) and fails when the runtime imports any other DLL of the redistributable (`dumpbin /dependents`), builds the Inno Setup installer `kagari-<v>-x86_64-windows-setup.exe` (with the redistributable inside), copies the runtime on its own as `kagari-desktop-runtime-<v>-x86_64-windows.exe`, and writes the `.sha256` files of the installer, the runtime and `vcruntime140.dll`. Nothing is code-signed ([Code signing](#code-signing)). |
| `macos` | `blacksmith-6vcpu-macos-15` (Apple silicon) | Builds FFmpeg (cached), builds both binaries for `aarch64-apple-darwin` with `MACOSX_DEPLOYMENT_TARGET=11.0` and `FFMPEG_DIR`, checks them, runs `--version`, packs `kagari-<v>-aarch64-macos.tar.gz` (a directory `kagari-<v>-aarch64-macos/` with both binaries, `LICENSE`, the five FFmpeg libraries and `ffmpeg-LICENSE.txt`) and its `.sha256`, and copies the runtime on its own as `kagari-desktop-runtime-<v>-aarch64-macos`, the FFmpeg libraries, `ffmpeg-LICENSE.txt` and `kagari.icns`, each with its `.sha256`. Signed ad hoc only ([Code signing](#code-signing)). |
| `upload` | `blacksmith-2vcpu-ubuntu-2404` | Needs `version`, `linux`, `windows` and `macos`. Uploads the three artifacts to R2 with the AWS CLI (content types by extension: `.tar.gz` `application/gzip`, `.sha256`/`.txt` text, `.icns` `image/icns`, else `application/octet-stream`). On a release it then writes the `latest` pointers (`linux_x86_bin`, `windows_x86_installer`, `macos_arm64_bin`) and `installer.sh`. |
| `nix-stable` | `blacksmith-2vcpu-ubuntu-2404` | Releases only: commits `nix/stable.json` (`version`, `rev` of the tag) to `main`, unless it already names this or a newer version. |

A manual run on a branch is a development build: it uploads under its `-dev.` version and moves
no pointer.

The `linux` job builds in a container rather than on the runner image so that the glibc baseline
does not move when GitHub retires an image. Raising the container raises `MIN_GLIBC` in
`installer.sh` and the requirement in the Tutorial and the runtime page.

The `windows` job additionally sets up MSVC (`ilammy/msvc-dev-cmd`), MSYS2 (`make nasm pkgconf
diffutils`) and meson/ninja, builds FFmpeg in an MSYS2 shell (MSYS2's `link.exe` is moved aside
so MSVC's is found; cached), builds with `FFMPEG_DIR` and `LIBCLANG_PATH=C:\Program Files\LLVM\bin`
(the runner's LLVM), runs the smoke test with the FFmpeg `bin\` on `PATH`, stages the five DLLs and
`ffmpeg-LICENSE.txt` for the installer and `dist\` (with `.sha256`), and runs the `dumpbin` check
over the DLLs too: the only redistributable import allowed is `VCRUNTIME140.dll`. Blacksmith's
Windows image has the Visual Studio 2022 Build Tools, not the full Visual Studio; `vswhere
-products *` finds them.

The `macos` job installs the pinned toolchain with rustup and, on an FFmpeg cache miss (key
`ffmpeg-macos-<hash of build.sh>`), `brew install meson ninja pkgconf` and
`KAGARI_FFMPEG_TARGET=macos packaging/ffmpeg/build.sh`. It builds with `FFMPEG_DIR`,
`MACOSX_DEPLOYMENT_TARGET=11.0` and `COPYFILE_DISABLE=1` (no `._` files in the tarball). "Check
the binaries" fails unless every binary and dylib is arm64 only (`lipo -archs`), passes
`codesign --verify` (the linker's ad hoc signature) and links nothing outside `/usr/lib`,
`/System/Library` and `@rpath` (`otool -L`). The smoke test runs the runtime with
`DYLD_LIBRARY_PATH` set to the FFmpeg `lib/`. The tarball is made with `tar --uid 0 --gid 0`;
`.sha256` files come from `shasum -a 256`, in `sha256sum`'s format. The artifact is
`macos-aarch64`.

## FFmpeg {#ffmpeg}

The runtime decodes movies with FFmpeg ([The Desktop Runtime](../runtime/desktop.md#kagari-video)),
linked dynamically, so releases ship its shared libraries next to the runtime.
`packaging/ffmpeg/build.sh <prefix>` (`KAGARI_FFMPEG_TARGET=linux|windows|macos`) builds them:

- Pinned sources: FFmpeg 9.0.2 and dav1d 1.5.3, each checked against a SHA-256 in the script.
- dav1d is built static and linked into libavcodec.
- FFmpeg is a minimal LGPL build (no `--enable-gpl`): shared `avcodec`, `avformat`, `avutil`,
  `swscale` and `swresample` only; decoders `h264`, `vp9`, `av1`, `libdav1d`, `aac`, `opus`,
  `vorbis`; demuxers `mov`, `matroska`, with their parsers and bitstream filters; no programs,
  network, devices or filters, and no autodetected libraries. Linux adds the VAAPI hwaccels
  (so `libva.so.2` and `libva-drm.so.2` become run-time requirements) and RUNPATH `$ORIGIN`;
  Windows builds with MSVC against the dynamic C runtime (dav1d `-Db_vscrt=md`, FFmpeg
  `--extra-cflags=-MD`: cl's default `/MT` cannot link the `/MD` dav1d, and the release workflow
  requires each DLL to import `VCRUNTIME140.dll`) and adds the D3D11VA hwaccels (the
  `*_d3d11va2` ones the runtime uses, and the `*_d3d11va` ones whose objects hold them) and
  import libraries `lib/*.lib` for `FFMPEG_DIR`; macOS (arm64 only; the script refuses other
  architectures) exports `MACOSX_DEPLOYMENT_TARGET=11.0` and adds `--enable-pic
  --enable-videotoolbox
  --enable-hwaccel=h264_videotoolbox,vp9_videotoolbox,av1_videotoolbox
  --install-name-dir=@rpath`, so each library's install name is `@rpath/<file>` and it links the
  VideoToolbox, CoreMedia, CoreVideo, CoreFoundation and CoreServices frameworks. On macOS the
  script needs the Xcode command line tools, make, meson, ninja, pkg-config, curl, tar and xz; it
  checks SHA-256s with `sha256sum` or `shasum` and counts CPUs with `nproc` or `sysctl hw.ncpu`.
- `<prefix>/ffmpeg-LICENSE.txt` is FFmpeg's LGPL followed by dav1d's BSD licence; it ships with
  every copy of the libraries.

| Platform | Shipped files |
| -------- | ------------- |
| Linux | `libavcodec.so.63`, `libavformat.so.63`, `libavutil.so.61`, `libswscale.so.10`, `libswresample.so.7`, `ffmpeg-LICENSE.txt` |
| Windows | `avcodec-63.dll`, `avformat-63.dll`, `avutil-61.dll`, `swscale-10.dll`, `swresample-7.dll`, `ffmpeg-LICENSE.txt` |
| macOS | `libavcodec.63.dylib`, `libavformat.63.dylib`, `libavutil.61.dylib`, `libswscale.10.dylib`, `libswresample.7.dylib`, `ffmpeg-LICENSE.txt` |

The workflow caches each platform's prefix under a key hashed from `build.sh`, so editing the
script rebuilds FFmpeg. The decoders and demuxers must cover every video format
`kagari-compat::supports` accepts for `native_v0`.

## The bucket

The bucket is served at `https://kagari-download.suitsu31.club`.

| Key | Contents | Cache-Control |
| --- | -------- | ------------- |
| `installer.sh` | The Linux and macOS installer | `no-cache` |
| `linux_x86_bin/latest` | The latest released version, one line | `no-cache` |
| `linux_x86_bin/<v>/kagari-<v>-x86_64-linux.tar.gz` (+ `.sha256`) | The Linux build | immutable, one year |
| `linux_x86_bin/<v>/kagari-desktop-runtime-<v>-x86_64-linux` (+ `.sha256`) | The Linux runtime alone, for `kagari release --linux` | immutable, one year |
| `windows_x86_installer/latest` | The latest released version, one line | `no-cache` |
| `windows_x86_installer/<v>/kagari-<v>-x86_64-windows-setup.exe` (+ `.sha256`) | The Windows installer | immutable, one year |
| `windows_x86_installer/<v>/kagari-desktop-runtime-<v>-x86_64-windows.exe` (+ `.sha256`) | The Windows runtime alone, for `kagari release --windows` | immutable, one year |
| `windows_x86_installer/<v>/vcruntime140.dll` (+ `.sha256`) | The MSVC C runtime the Windows runtime imports, shipped next to it by `kagari release --windows` | immutable, one year |
| `windows_x86_installer/<v>/{avcodec-63,avformat-63,avutil-61,swscale-10,swresample-7}.dll`, `ffmpeg-LICENSE.txt` (+ `.sha256` each) | FFmpeg for the Windows runtime, shipped next to it by `kagari release --windows` | immutable, one year |
| `linux_x86_bin/<v>/{libavcodec.so.63,libavformat.so.63,libavutil.so.61,libswscale.so.10,libswresample.so.7}`, `ffmpeg-LICENSE.txt` (+ `.sha256` each) | FFmpeg for the Linux runtime, shipped next to it by `kagari release --linux` | immutable, one year |
| `macos_arm64_bin/latest` | The latest released version, one line | `no-cache` |
| `macos_arm64_bin/<v>/kagari-<v>-aarch64-macos.tar.gz` (+ `.sha256`) | The macOS build | immutable, one year |
| `macos_arm64_bin/<v>/kagari-desktop-runtime-<v>-aarch64-macos` (+ `.sha256`) | The macOS runtime alone, for `kagari release --macos` | immutable, one year |
| `macos_arm64_bin/<v>/{libavcodec.63,libavformat.63,libavutil.61,libswscale.10,libswresample.7}.dylib`, `ffmpeg-LICENSE.txt`, `kagari.icns` (+ `.sha256` each) | FFmpeg and the app icon for the macOS runtime, put into the app by `kagari release --macos` | immutable, one year |

Versioned files are uploaded before the pointers, so a pointer never names a missing file. A
`.sha256` file is `sha256sum` output: `<hex>  <file name>`.

`kagari release` hard-codes the server, the three directories, the runtime file names and the
libraries shipped with the runtime (`vcruntime140.dll` and FFmpeg on Windows, FFmpeg on Linux,
FFmpeg and `kagari.icns` on macOS, each with `ffmpeg-LICENSE.txt`; `command/release/{mod,download}.rs`)
and downloads them for its own version, each checked against its `.sha256`; see
[`kagari release`](../tooling/cli.md#kagari-release). 0.1.0 was released without a standalone
runtime; `kagari release` exists from the release after it on, and `--macos` from 0.3.0 (the
first macOS build). Renaming a directory, a runtime file or a library breaks every released
`kagari release`; add, never move.

## Secrets

The job `upload` uses the GitHub environment `release`; its secrets (or repository secrets) are:

| Secret | Value |
| ------ | ----- |
| `R2_ACCOUNT_ID` | The Cloudflare account ID (the endpoint is `https://<id>.r2.cloudflarestorage.com`) |
| `R2_BUCKET` | The bucket name |
| `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` | An R2 API token with Object Read & Write on the bucket |

`nix-stable` pushes with the workflow's `GITHUB_TOKEN` (`contents: write`); a branch protection
on `main` must let GitHub Actions push.

## The Linux and macOS installer

`installer.sh` runs as `curl … | bash`; all its code is in functions and `main` is called on the
last line, so a truncated download runs nothing. It runs on Linux x86_64 and macOS arm64
(`check_platform` calls `check_linux` or `check_macos`, which set the bucket directory
`linux_x86_bin`/`macos_arm64_bin`, the archive platform `x86_64-linux`/`aarch64-macos` and the
FFmpeg library list). It refuses Windows shells and other systems. On Linux it refuses other
architectures, musl and glibc older than `MIN_GLIBC`, and on NixOS points to the flake. On macOS
it requires Apple silicon by `sysctl -n hw.optional.arm64` being 1 (a shell under Rosetta reports
`x86_64` from `uname -m`), else it stops with "macOS builds are for Apple silicon (M1 or newer)
only; Intel Macs are not supported", and refuses versions below `MACOS_SINCE` (0.3.0, the first
macOS build). It reads `<bucket dir>/latest` unless given `--version`, checks the checksum,
copies both binaries into `$KAGARI_HOME/bin` (default `~/.local/share/kagari/bin` on both
systems) through a temporary file and a rename (a running `kagari lsp` keeps its old file), and
links them into `$KAGARI_BIN_DIR` (default `~/.local/bin`). It refuses to replace a file there
that is not its own link, and prints a `PATH` line when needed, naming `~/.zprofile` on macOS and
`~/.profile` on Linux. `--uninstall` removes its links and `$KAGARI_HOME`.
`KAGARI_DOWNLOAD_URL` points it at another server (for testing). It must keep working with
macOS's `/bin/bash` 3.2.

What else it installs depends on the version, so `--version` can still install an older
release. From `FFMPEG_SINCE` (0.3.0) on, the archive must hold the five FFmpeg libraries and
`ffmpeg-LICENSE.txt` (an archive without one is refused before anything is replaced); the
libraries go next to the binaries (mode 644, the same rename) and the licence to
`$KAGARI_HOME`. An older version ships none, and installing one removes those files left by a
newer one; every macOS version has them. The comparison uses the `x.y.z` part, so a development
build `0.3.0-dev.<commit>` counts as 0.3.0: a branch that changes the shipped libraries bumps the
workspace version before its development builds are installed this way. Afterwards, on Linux
only, it warns when `libasound.so.2` or `libudev.so.1` is missing from the `ldconfig` cache, and
for versions with FFmpeg also `libva.so.2` and `libva-drm.so.2` (the runtime does not start
without libva). The runtime finds the libraries through `$ORIGIN` (Linux) or `@executable_path`
(macOS), which resolve through the link to `$KAGARI_HOME/bin`.

## The Windows installer

`packaging/windows/kagari.iss` builds a per-user installer (`PrivilegesRequired=lowest`): it
installs into `%LOCALAPPDATA%\Programs\Kagari` (`bin\` holds both executables and the FFmpeg
DLLs; `ffmpeg-LICENSE.txt` goes to the top directory) and appends
`…\Kagari\bin` to `HKCU\Environment\Path` when it is not there yet; the uninstaller removes
exactly that entry. Its `AppId` is fixed, so a newer installer replaces the installed version. The
wizard has English and Japanese. The workflow passes `AppVersion`, `NumericVersion` (the version
without a pre-release or build suffix, for the file version resource), `SourceDir` and `OutputDir`.

Both executables need the MSVC C runtime. `SourceDir` also holds `vc_redist.x64.exe`, embedded
but not installed (`dontcopy`); the preprocessor reads its file version, and compiling fails
without one. Before copying anything (`PrepareToInstall`), the installer reads the version that
the Visual C++ 2015 and later Redistributable registers under
`HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64` (`Installed`, `Major`, `Minor`,
`Bld`, `Rbld`; both registry views). When neither is at least the bundled version, it extracts
the bundled one and runs it with `/install /passive /norestart` through `ShellExec`, so Windows
asks for administrator rights (UAC). Exit code 0 or 1638 (a newer version is installed)
continues, 3010 continues and asks for a restart at the end (`NeedRestart`), anything else,
including a declined UAC prompt, stops the installation with the `VCRedistFailed` message (link
to Microsoft's download). The uninstaller leaves the redistributable installed: it is shared.

### Code signing {#code-signing}

Nothing is signed with a certificate. On Windows the executables and the installer are unsigned;
on first run Windows SmartScreen shows "Windows protected your PC" until the user picks
**More info → Run anyway**; the Tutorial says so.
Azure Artifact Signing, the CI-friendly option, issues Public Trust certificates only to
individuals in the US and Canada and to registered organizations, which the project is not.
Signing later means a signing step in the `windows` job before `Build the installer` (for the
executables) and after it (for the installer), and dropping the note from the Tutorial.
`vc_redist.x64.exe` and `vcruntime140.dll` are Microsoft-signed already.

On macOS everything is signed ad hoc only: the toolchain and FFmpeg by the linker, a game's app
by `kagari release --macos` ([`kagari release`](../tooling/cli.md#kagari-release)); there is no
Apple Developer ID and no notarization (the project is not in the Apple Developer Program). The
toolchain installed by `curl … | bash` carries no quarantine attribute, so it runs without a
prompt. A downloaded game does: Gatekeeper says "Apple could not verify…" on first launch, and
the player opens it through **System Settings → Privacy & Security → Open Anyway** (macOS 15 and
later; before that, Control-click → **Open**); the Tutorial says so. `spctl -a` rejects the apps,
as expected. Developer ID signing later means a Developer ID certificate, the hardened runtime,
submission with `notarytool` and stapling; for games, the certificate is the game author's, not
the project's.

## Nix

`packages.unstable` builds the flake's own source (only the Cargo sources and `.cargo/`, through
`lib.fileset`), version `<workspace>-unstable-<date>`. `packages.stable` exists once
`nix/stable.json` does and builds the tagged commit it names, fetched with `fetchTree` (pinned by
`rev`, so pure evaluation accepts it). `packages.default` is `stable` when present, else `unstable`.
Both use nixpkgs' `rustPlatform` (its rustc must be at least the pinned toolchain), skip the tests,
and add the libraries the runtime loads with `dlopen` (Wayland, xkbcommon, Vulkan, GL, X11) to its
`RUNPATH` after fixup.

## Crates {#crates}

The workspace crates are published to a private [Kellnr](https://kellnr.io) registry at
`https://crates.suitsu31.club`, registry name `Suitsu31`, sparse index
`sparse+https://crates.suitsu31.club/api/v1/crates/`. It requires authentication for every
request, including index reads, so a project that depends on these crates also needs a token.

- `[workspace.package] publish = ["Suitsu31"]`, inherited by every crate with
  `publish.workspace = true`: `cargo publish` defaults to Suitsu31 and refuses crates.io.
  `mdbook-kagari` keeps `publish = false`.
- Each workspace crate in `[workspace.dependencies]` has `path`, `version` and
  `registry = "Suitsu31"`. Builds use the path; the published manifest keeps the version and the
  registry. Without `registry`, a published dependency would point at crates.io, where another
  crate may own the name.
- Because the manifests name the registry, every build needs it defined, even though nothing is
  fetched from it: `.cargo/config.toml` defines it without credentials, for CI, Nix and fresh
  clones. Developers who consume the crates elsewhere define the same table in
  `~/.cargo/config.toml`.
- Credentials: `cargo login --registry Suitsu31` with a token created in the Kellnr web UI
  (stored in `~/.cargo/credentials.toml`), or `CARGO_REGISTRIES_SUITSU31_TOKEN` in CI.

Another project depends on a crate with
`kagari-lang = { version = "0.3", registry = "Suitsu31" }`.

To publish, after the version bump below: `cargo publish --workspace` uploads every publishable
crate in dependency order (cargo verifies each one by building it from its packaged sources, so
the dev shell's libraries are needed). A version can be yanked
(`cargo yank --registry Suitsu31 <crate>@<version>`) but never re-uploaded. Dry run without
uploading: `cargo publish --workspace --dry-run`.

## Making a release

1. Set `[workspace.package] version` in `Cargo.toml`, set the same `version` on every workspace
   crate in `[workspace.dependencies]` (a mismatched minor version fails the build), and run
   `cargo check` so `Cargo.lock` follows; commit.
2. Tag that commit `v<version>` and push the tag.
3. Check the workflow run: both artifacts, the `latest` pointers and the `nix/stable.json` commit
   on `main`.
4. Publish the crates: `cargo publish --workspace` ([Crates](#crates)).

**Invariants**

- A tag equals the workspace version, and released files are never overwritten by another build:
  development builds go under `-dev.` versions.
- `latest` and `installer.sh` change only on a release.
- Both binaries of an installation come from the same build and sit in the same directory.
- Every release after 0.1.0 has a standalone runtime per platform under the names
  `kagari release` of that version expects.

**When you change this**

- New platform: a build job (on a Blacksmith runner label), a bucket prefix (`<os>_<arch>_…`), the
  detection in `installer.sh`, the Tutorial's install page (en and ja), the table above, and a
  `Platform` (with its flag) in `kagari release`.
- Runner labels: Blacksmith labels name vCPUs and image (`blacksmith-<n>vcpu-<image>`); an image
  change can move compilers and SDKs (the Windows image has the VS Build Tools, which `vswhere
  -products *` finds; the macOS image's Xcode sets the SDK).
- New binary in a release: the `cargo build` lines, both packaging steps, `COMMANDS` in
  `installer.sh`, `[Files]` in `kagari.iss` and `cargoBuildFlags` in `flake.nix`.
- New runtime system library: also `check_runtime_libraries` in `installer.sh` and the runtime's
  `RUNPATH` in `flake.nix` (see [Testing](./testing.md#toolchain-and-dev-shell)); on macOS it must
  come from `/usr/lib`, `/System/Library` or `@rpath` (the `macos` job's check).
- Another FFmpeg version or component: `packaging/ffmpeg/build.sh` (versions, SHA-256s, flags);
  new sonames, dylib or DLL names also change `installer.sh` (a new version threshold next to
  `FFMPEG_SINCE` with its own list, so older versions still install theirs; the lists are
  `LINUX_FFMPEG_LIBS` and `MACOS_FFMPEG_LIBS`), `[Files]` in `kagari.iss`, the workflow's lists and `Platform::libraries` in
  `kagari release` (published names are add-only, as above).
- The logo: regenerate `runtime/kagari-desktop-runtime/resources/kagari.icns`
  ([The Desktop Runtime](../runtime/desktop.md)).
