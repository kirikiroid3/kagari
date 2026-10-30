#!/usr/bin/env bash
# Installs the Kagari toolchain (`kagari` and `kagari-desktop-runtime`) for the current user.
#
#   curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
#   curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash -s -- --version 0.1.0
#   curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash -s -- --uninstall
#
# Environment:
#   KAGARI_HOME          where the files go (default: $XDG_DATA_HOME/kagari or ~/.local/share/kagari)
#   KAGARI_BIN_DIR       where the commands are linked (default: ~/.local/bin)
#   KAGARI_DOWNLOAD_URL  the download server (default: https://kagari-download.suitsu31.club)
#
# Release layout on the download server (written by .github/workflows/release.yml):
#   linux_x86_bin/latest                                       the latest version, one line
#   linux_x86_bin/<version>/kagari-<version>-x86_64-linux.tar.gz
#   linux_x86_bin/<version>/kagari-<version>-x86_64-linux.tar.gz.sha256
#   windows_x86_installer/<version>/kagari-<version>-x86_64-windows-setup.exe

# Everything is inside functions and `main` runs on the last line, so a download cut short by
# `curl | bash` runs nothing.

set -euo pipefail

DOWNLOAD_URL="${KAGARI_DOWNLOAD_URL:-https://kagari-download.suitsu31.club}"
DOWNLOAD_URL="${DOWNLOAD_URL%/}"
INSTALL_HOME="${KAGARI_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/kagari}"
BIN_DIR="${KAGARI_BIN_DIR:-$HOME/.local/bin}"
TUTORIAL_URL="https://kagari.suitsu31.club/en/tutorial/install/"
COMMANDS=(kagari kagari-desktop-runtime)
# The Linux binaries are built on Ubuntu 22.04.
MIN_GLIBC="2.35"

say() { printf 'kagari-installer: %s\n' "$*" >&2; }
die() {
    say "error: $*"
    exit 1
}

usage() {
    cat <<'EOF'
Installs the Kagari toolchain (kagari, kagari-desktop-runtime) for the current user.

Usage: installer.sh [--version <VERSION>] [--uninstall]

Options:
  --version <VERSION>  Install this version instead of the latest one
  --uninstall          Remove an installation made by this script
  -h, --help           Print this help

Environment:
  KAGARI_HOME          Where the files go (default: ~/.local/share/kagari)
  KAGARI_BIN_DIR       Where the commands are linked (default: ~/.local/bin)
  KAGARI_DOWNLOAD_URL  The download server
EOF
}

check_platform() {
    case "$(uname -s)" in
        Linux) ;;
        Darwin) die "macOS is not supported yet. See $TUTORIAL_URL" ;;
        MINGW* | MSYS* | CYGWIN*) die "on Windows, use the Windows installer. See $TUTORIAL_URL" ;;
        *) die "unsupported operating system: $(uname -s)" ;;
    esac

    if [ -e /etc/NIXOS ] || grep -qs '^ID=nixos' /etc/os-release; then
        say "NixOS cannot run these prebuilt binaries; use the flake instead:"
        say "  nix profile install github:kirikiroid3/kagari#stable"
        exit 1
    fi

    case "$(uname -m)" in
        x86_64 | amd64) ;;
        *) die "unsupported CPU architecture: $(uname -m) (only x86_64 is built)" ;;
    esac

    local glibc
    glibc="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
    glibc="${glibc#glibc }"
    if [ -z "$glibc" ]; then
        die "the Linux build needs glibc $MIN_GLIBC or newer (musl-based systems are not supported)"
    fi
    if [ "$(printf '%s\n%s\n' "$MIN_GLIBC" "$glibc" | sort -V | head -n 1)" != "$MIN_GLIBC" ]; then
        die "the Linux build needs glibc $MIN_GLIBC or newer; this system has $glibc"
    fi
}

# fetch URL FILE
fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1" || die "cannot download $1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1" || die "cannot download $1"
    else
        die "needs curl or wget"
    fi
}

# sha256 FILE: prints the hex digest
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        die "needs sha256sum or shasum"
    fi
}

# Warns about the shared libraries the runtime links that are not installed. The CLI needs none.
check_runtime_libraries() {
    local ldconfig
    if command -v ldconfig >/dev/null 2>&1; then
        ldconfig=ldconfig
    elif [ -x /sbin/ldconfig ]; then
        ldconfig=/sbin/ldconfig
    else
        return 0
    fi
    local cache missing=() lib
    cache="$("$ldconfig" -p 2>/dev/null || true)"
    for lib in libasound.so.2 libudev.so.1; do
        case "$cache" in
            *"$lib "*) ;;
            *) missing+=("$lib") ;;
        esac
    done
    if [ "${#missing[@]}" -gt 0 ]; then
        say "warning: kagari-desktop-runtime needs ${missing[*]}, which this system does not have."
        say "  Install ALSA and libudev with your package manager (Debian/Ubuntu: libasound2 libudev1;"
        say "  Fedora: alsa-lib systemd-libs). The kagari command works without them."
    fi
}

install_kagari() {
    local version="$1"
    if [ -z "$version" ]; then
        local tmp_latest
        tmp_latest="$TMP_DIR/latest"
        fetch "$DOWNLOAD_URL/linux_x86_bin/latest" "$tmp_latest"
        version="$(tr -d '[:space:]' <"$tmp_latest")"
    fi
    case "$version" in
        "" | *[!0-9A-Za-z.+-]*) die "invalid version: '$version'" ;;
    esac

    local name="kagari-$version-x86_64-linux"
    local url="$DOWNLOAD_URL/linux_x86_bin/$version/$name.tar.gz"
    say "downloading Kagari $version"
    fetch "$url" "$TMP_DIR/$name.tar.gz"
    fetch "$url.sha256" "$TMP_DIR/$name.tar.gz.sha256"

    local expected actual
    expected="$(cut -d ' ' -f 1 <"$TMP_DIR/$name.tar.gz.sha256")"
    actual="$(sha256 "$TMP_DIR/$name.tar.gz")"
    [ "$expected" = "$actual" ] || die "checksum mismatch for $name.tar.gz"

    tar -xzf "$TMP_DIR/$name.tar.gz" -C "$TMP_DIR"
    local unpacked="$TMP_DIR/$name"
    local cmd
    for cmd in "${COMMANDS[@]}"; do
        [ -f "$unpacked/$cmd" ] || die "the archive has no $cmd"
    done

    mkdir -p "$INSTALL_HOME/bin" "$BIN_DIR"
    for cmd in "${COMMANDS[@]}"; do
        local link="$BIN_DIR/$cmd"
        if [ -e "$link" ] && [ ! -L "$link" ]; then
            die "$link exists and is not a link made by this installer; remove it first"
        fi
    done
    # Copy next to the target, then rename: a running `kagari lsp` keeps its old file.
    for cmd in "${COMMANDS[@]}"; do
        cp "$unpacked/$cmd" "$INSTALL_HOME/bin/.$cmd.new"
        chmod 755 "$INSTALL_HOME/bin/.$cmd.new"
        mv -f "$INSTALL_HOME/bin/.$cmd.new" "$INSTALL_HOME/bin/$cmd"
        ln -sfn "$INSTALL_HOME/bin/$cmd" "$BIN_DIR/$cmd"
    done
    if [ -f "$unpacked/LICENSE" ]; then
        cp "$unpacked/LICENSE" "$INSTALL_HOME/LICENSE"
    fi

    local installed
    installed="$("$INSTALL_HOME/bin/kagari" --version 2>/dev/null)" ||
        die "the installed kagari does not run on this system"
    say "installed $installed into $INSTALL_HOME"
    say "linked ${COMMANDS[*]} into $BIN_DIR"

    check_runtime_libraries
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            say "$BIN_DIR is not on your PATH. Add this line to ~/.profile (or your shell's rc file):"
            say "  export PATH=\"$BIN_DIR:\$PATH\""
            ;;
    esac
}

uninstall_kagari() {
    local cmd
    for cmd in "${COMMANDS[@]}"; do
        local link="$BIN_DIR/$cmd"
        if [ -L "$link" ] && [ "$(readlink "$link")" = "$INSTALL_HOME/bin/$cmd" ]; then
            rm -f "$link"
        fi
    done
    if [ -d "$INSTALL_HOME" ]; then
        rm -rf "$INSTALL_HOME"
        say "removed $INSTALL_HOME"
    else
        say "nothing installed in $INSTALL_HOME"
    fi
}

main() {
    local version="" uninstall=0
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || die "--version needs a value"
                version="$2"
                shift 2
                ;;
            --version=*)
                version="${1#--version=}"
                shift
                ;;
            --uninstall)
                uninstall=1
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            *) die "unknown argument: $1 (see --help)" ;;
        esac
    done

    if [ "$uninstall" -eq 1 ]; then
        uninstall_kagari
        return
    fi

    check_platform
    TMP_DIR="$(mktemp -d)"
    trap 'rm -rf "$TMP_DIR"' EXIT
    install_kagari "$version"
}

main "$@"
