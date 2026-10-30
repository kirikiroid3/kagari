# Releases

**Where in the code**

- Version: `[workspace.package] version` in `Cargo.toml`; every crate uses `version.workspace = true`.
- CI: `.github/workflows/release.yml`.
- Linux installer: `installer.sh` (uploaded to the bucket root).
- Windows installer: `packaging/windows/kagari.iss` (Inno Setup).
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

`release.yml` runs on every pushed tag and on manual dispatch. `linux` and `windows` run in
parallel after `version`; `upload` waits for both, so a failed build uploads nothing.

| Job | Runs on | Does |
| --- | ------- | ---- |
| `version` | ubuntu | Reads the workspace version. On a tag it fails unless the tag matches; on a branch the version is `<workspace>-dev.<short sha>` and `release` is false. |
| `linux` | `ubuntu:22.04` container | Builds both binaries with GCC (`x86_64-unknown-linux-gnu`) against glibc 2.35, runs `--version`, packs `kagari-<v>-x86_64-linux.tar.gz` (a directory `kagari-<v>-x86_64-linux/` with both binaries and `LICENSE`) and its `.sha256`. |
| `windows` | windows-2025 | Builds both binaries (MSVC), builds the Inno Setup installer `kagari-<v>-x86_64-windows-setup.exe` from them, writes its `.sha256`. Nothing is code-signed ([Code signing](#code-signing)). |
| `upload` | ubuntu | Uploads both artifacts to R2 with the AWS CLI. On a release it then writes the `latest` pointers and `installer.sh`. |
| `nix-stable` | ubuntu | Releases only: commits `nix/stable.json` (`version`, `rev` of the tag) to `main`, unless it already names this or a newer version. |

A manual run on a branch is a development build: it uploads under its `-dev.` version and moves
no pointer.

The `linux` job builds in a container rather than on the runner image so that the glibc baseline
does not move when GitHub retires an image. Raising the container raises `MIN_GLIBC` in
`installer.sh` and the requirement in the Tutorial and the runtime page.

## The bucket

The bucket is served at `https://kagari-download.suitsu31.club`.

| Key | Contents | Cache-Control |
| --- | -------- | ------------- |
| `installer.sh` | The Linux installer | `no-cache` |
| `linux_x86_bin/latest` | The latest released version, one line | `no-cache` |
| `linux_x86_bin/<v>/kagari-<v>-x86_64-linux.tar.gz` (+ `.sha256`) | The Linux build | immutable, one year |
| `windows_x86_installer/latest` | The latest released version, one line | `no-cache` |
| `windows_x86_installer/<v>/kagari-<v>-x86_64-windows-setup.exe` (+ `.sha256`) | The Windows installer | immutable, one year |

Versioned files are uploaded before the pointers, so a pointer never names a missing file. A
`.sha256` file is `sha256sum` output: `<hex>  <file name>`.

## Secrets

The job `upload` uses the GitHub environment `release`; its secrets (or repository secrets) are:

| Secret | Value |
| ------ | ----- |
| `R2_ACCOUNT_ID` | The Cloudflare account ID (the endpoint is `https://<id>.r2.cloudflarestorage.com`) |
| `R2_BUCKET` | The bucket name |
| `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` | An R2 API token with Object Read & Write on the bucket |

`nix-stable` pushes with the workflow's `GITHUB_TOKEN` (`contents: write`); a branch protection
on `main` must let GitHub Actions push.

## The Linux installer

`installer.sh` runs as `curl … | bash`; all its code is in functions and `main` is called on the
last line, so a truncated download runs nothing. It refuses macOS, Windows shells, other
architectures, musl and glibc older than `MIN_GLIBC`, and on NixOS points to the flake. It reads
`linux_x86_bin/latest` unless given `--version`, checks the checksum, copies both binaries into
`$KAGARI_HOME/bin` (default `~/.local/share/kagari/bin`) through a temporary file and a rename (a
running `kagari lsp` keeps its old file), and links them into `$KAGARI_BIN_DIR` (default
`~/.local/bin`). It refuses to replace a file there that is not its own link, warns when
`libasound.so.2` or `libudev.so.1` is missing from the `ldconfig` cache, and prints a `PATH` line
when needed. `--uninstall` removes its links and `$KAGARI_HOME`. `KAGARI_DOWNLOAD_URL` points it
at another server (for testing).

## The Windows installer

`packaging/windows/kagari.iss` builds a per-user installer (`PrivilegesRequired=lowest`): it
installs into `%LOCALAPPDATA%\Programs\Kagari` (`bin\` holds both executables) and appends
`…\Kagari\bin` to `HKCU\Environment\Path` when it is not there yet; the uninstaller removes
exactly that entry. Its `AppId` is fixed, so a newer installer replaces the installed version. The
wizard has English and Japanese. The workflow passes `AppVersion`, `NumericVersion` (the version
without a pre-release or build suffix, for the file version resource), `SourceDir` and `OutputDir`.

### Code signing {#code-signing}

The executables and the installer are not code-signed. On first run Windows SmartScreen shows
"Windows protected your PC" until the user picks **More info → Run anyway**; the Tutorial says so.
Azure Artifact Signing, the CI-friendly option, issues Public Trust certificates only to
individuals in the US and Canada and to registered organizations, which the project is not.
Signing later means a signing step in the `windows` job before `Build the installer` (for the
executables) and after it (for the installer), and dropping the note from the Tutorial.

## Nix

`packages.unstable` builds the flake's own source (only the Cargo sources, through
`lib.fileset`), version `<workspace>-unstable-<date>`. `packages.stable` exists once
`nix/stable.json` does and builds the tagged commit it names, fetched with `fetchTree` (pinned by
`rev`, so pure evaluation accepts it). `packages.default` is `stable` when present, else `unstable`.
Both use nixpkgs' `rustPlatform` (its rustc must be at least the pinned toolchain), skip the tests,
and add the libraries the runtime loads with `dlopen` (Wayland, xkbcommon, Vulkan, GL, X11) to its
`RUNPATH` after fixup.

## Making a release

1. Set `[workspace.package] version` in `Cargo.toml` and run `cargo check` so `Cargo.lock`
   follows; commit.
2. Tag that commit `v<version>` and push the tag.
3. Check the workflow run: both artifacts, the `latest` pointers and the `nix/stable.json` commit
   on `main`.

**Invariants**

- A tag equals the workspace version, and released files are never overwritten by another build:
  development builds go under `-dev.` versions.
- `latest` and `installer.sh` change only on a release.
- Both binaries of an installation come from the same build and sit in the same directory.

**When you change this**

- New platform: a build job, a bucket prefix (`<os>_<arch>_…`), the detection in `installer.sh`,
  the Tutorial's install page (en and ja) and the table above.
- New binary in a release: the `cargo build` lines, both packaging steps, `COMMANDS` in
  `installer.sh`, `[Files]` in `kagari.iss` and `cargoBuildFlags` in `flake.nix`.
- New runtime system library: also `check_runtime_libraries` in `installer.sh` and the runtime's
  `RUNPATH` in `flake.nix` (see [Testing](./testing.md#toolchain-and-dev-shell)).
