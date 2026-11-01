#!/usr/bin/env bash
# shellcheck disable=SC2054 # Commas separate FFmpeg's component lists, not array elements.
# Builds the FFmpeg that ships with kagari-desktop-runtime: a minimal LGPL build (no
# `--enable-gpl`) of the shared libraries libavcodec, libavformat, libavutil, libswscale and
# libswresample, with only what the runtime plays (WebM with AV1 or VP9 and Opus or Vorbis, MP4
# with H.264 and AAC) and dav1d linked in statically.
#
#   KAGARI_FFMPEG_TARGET=linux|windows|macos packaging/ffmpeg/build.sh <prefix>
#
# linux: run on the oldest glibc the runtime supports (the release workflow's ubuntu:22.04).
#   Needs a C compiler, make, meson, ninja, nasm, pkg-config, libva (VAAPI), curl, tar, xz.
#   The libraries have RUNPATH `$ORIGIN`, so they find each other next to the runtime.
# windows: run in an MSYS2 shell that inherits the MSVC environment (`vcvars64` or
#   ilammy/msvc-dev-cmd), with MSYS2's link.exe moved out of the way. Needs make, nasm, pkgconf,
#   diffutils, and meson and ninja on PATH. Writes the import libraries `lib/avcodec.lib` etc.,
#   which `FFMPEG_DIR=<prefix>` builds of the runtime link against.
# macos: run on Apple Silicon (arm64 only) with the Xcode command line tools. Needs make, meson,
#   ninja, pkg-config, curl, tar, xz. Builds for macOS 11 (MACOSX_DEPLOYMENT_TARGET) with the
#   VideoToolbox hwaccels. The install names are `@rpath/<file>`, so the runtime finds the
#   libraries through its own rpaths (next to it, or in an app bundle's Frameworks).
#
# <prefix> gets include/, lib/ (and bin/ on Windows) and ffmpeg-LICENSE.txt, FFmpeg's licence
# followed by dav1d's. The files the runtime ships, by target:
#   linux:   lib/libavcodec.so.63 lib/libavformat.so.63 lib/libavutil.so.61 lib/libswscale.so.10
#            lib/libswresample.so.7
#   windows: bin/avcodec-63.dll bin/avformat-63.dll bin/avutil-61.dll bin/swscale-10.dll
#            bin/swresample-7.dll
#   macos:   lib/libavcodec.63.dylib lib/libavformat.63.dylib lib/libavutil.61.dylib
#            lib/libswscale.10.dylib lib/libswresample.7.dylib
# Changing this file changes the release workflow's cache key, which rebuilds FFmpeg.

set -euo pipefail

FFMPEG_VERSION=9.0.2
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
DAV1D_VERSION=1.5.3
DAV1D_URL="https://code.videolan.org/videolan/dav1d/-/archive/$DAV1D_VERSION/dav1d-$DAV1D_VERSION.tar.gz"
DAV1D_SHA256=cbe212b02faf8c6eed5b6d55ef8a6e363aaab83f15112e960701a9c3df813686

LIBRARIES=(avcodec avformat avutil swscale swresample)
LINUX_FILES=(libavcodec.so.63 libavformat.so.63 libavutil.so.61 libswscale.so.10 libswresample.so.7)
WINDOWS_FILES=(avcodec-63.dll avformat-63.dll avutil-61.dll swscale-10.dll swresample-7.dll)
MACOS_FILES=(libavcodec.63.dylib libavformat.63.dylib libavutil.61.dylib libswscale.10.dylib
    libswresample.7.dylib)
# The oldest macOS the runtime supports: the first with Apple Silicon.
MACOS_DEPLOYMENT_TARGET=11.0

say() { printf 'ffmpeg-build: %s\n' "$*" >&2; }
die() {
    say "error: $*"
    exit 1
}

[ "$#" -eq 1 ] || die "usage: KAGARI_FFMPEG_TARGET=linux|windows|macos $0 <prefix>"
target="${KAGARI_FFMPEG_TARGET:-}"
case "$target" in
    linux | windows) ;;
    macos)
        [ "$(uname -m)" = arm64 ] || die "the macOS build is arm64 only (this is $(uname -m))"
        export MACOSX_DEPLOYMENT_TARGET="$MACOS_DEPLOYMENT_TARGET"
        ;;
    *) die "set KAGARI_FFMPEG_TARGET to linux, windows or macos (got '$target')" ;;
esac
mkdir -p "$1"
prefix="$(cd "$1" && pwd)"
# The prefix as native tools on Windows (meson, the MSVC tools) read it.
native_prefix="$prefix"
if [ "$target" = windows ]; then
    native_prefix="$(cygpath -m "$prefix")"
fi

jobs="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# sha256 FILE: the SHA-256 of FILE in hex (macOS has shasum, not always sha256sum).
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# fetch URL FILE SHA256: downloads URL into $work/FILE and checks it.
fetch() {
    say "downloading $1"
    curl -fsSL --retry 3 --retry-all-errors -o "$work/$2" "$1"
    local actual
    actual="$(sha256 "$work/$2")"
    [ "$actual" = "$3" ] || die "$2: SHA-256 $actual, expected $3"
}

fetch "$DAV1D_URL" "dav1d-$DAV1D_VERSION.tar.gz" "$DAV1D_SHA256"
fetch "$FFMPEG_URL" "ffmpeg-$FFMPEG_VERSION.tar.xz" "$FFMPEG_SHA256"
tar -xzf "$work/dav1d-$DAV1D_VERSION.tar.gz" -C "$work"
tar -xJf "$work/ffmpeg-$FFMPEG_VERSION.tar.xz" -C "$work"

say "building dav1d $DAV1D_VERSION"
(
    cd "$work/dav1d-$DAV1D_VERSION"
    meson setup build-dav1d "--prefix=$native_prefix" --libdir=lib --buildtype=release \
        -Ddefault_library=static -Denable_tools=false -Denable_tests=false
    ninja -C build-dav1d install
)
if [ "$target" = windows ]; then
    # Meson names a static library libdav1d.a; FFmpeg's MSVC toolchain links `-ldav1d` as
    # dav1d.lib.
    cp "$prefix/lib/libdav1d.a" "$prefix/lib/dav1d.lib"
fi

say "building FFmpeg $FFMPEG_VERSION"
flags=(
    "--prefix=$prefix" --enable-shared --disable-static --disable-programs --disable-doc
    --disable-network --disable-autodetect --disable-everything --disable-avdevice
    --disable-avfilter --disable-debug
    --enable-avcodec --enable-avformat --enable-swscale --enable-swresample --enable-libdav1d
    --enable-decoder=h264,vp9,av1,libdav1d,aac,opus,vorbis --enable-demuxer=mov,matroska
    --enable-parser=h264,vp9,av1,aac,opus,vorbis --enable-bsf=vp9_superframe_split,av1_frame_split
)
case "$target" in
    linux)
        # `$ORIGIN` reaches the linker through a response file: configure's `eval` and make's
        # and the shell's expansion would each eat a `$` written on the command line.
        # shellcheck disable=SC2016 # The dynamic loader expands $ORIGIN.
        printf '%s\n' '-rpath=$ORIGIN' --enable-new-dtags >"$work/rpath.rsp"
        flags+=(
            --enable-pic --enable-vaapi --enable-hwaccel=h264_vaapi,vp9_vaapi,av1_vaapi
            "--extra-ldsoflags=-Wl,@$work/rpath.rsp"
        )
        ;;
    windows)
        flags+=(
            --toolchain=msvc --enable-d3d11va
            --enable-hwaccel=h264_d3d11va2,vp9_d3d11va2,av1_d3d11va2
        )
        ;;
    macos)
        # `@rpath` reaches the linker through `--install-name-dir`; the runtime's rpaths
        # (build.rs) name the directories to search.
        flags+=(
            --enable-pic --enable-videotoolbox
            --enable-hwaccel=h264_videotoolbox,vp9_videotoolbox,av1_videotoolbox
            "--install-name-dir=@rpath"
        )
        ;;
esac
(
    cd "$work/ffmpeg-$FFMPEG_VERSION"
    export PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    ./configure "${flags[@]}" || {
        tail -n 50 ffbuild/config.log >&2
        exit 1
    }
    make -j"$jobs"
    make install
)

if [ "$target" = windows ]; then
    # FFmpeg installs the import libraries next to the DLLs (bin/avcodec.lib); FFMPEG_DIR wants
    # lib/avcodec.lib.
    for lib in "${LIBRARIES[@]}"; do
        [ -f "$prefix/lib/$lib.lib" ] && continue
        found=""
        for candidate in "$prefix/bin/$lib.lib" "$prefix"/bin/"$lib"-[0-9]*.lib \
            "$prefix"/lib/"$lib"-[0-9]*.lib; do
            if [ -f "$candidate" ]; then
                found="$candidate"
                break
            fi
        done
        [ -n "$found" ] || die "no import library of $lib in $prefix"
        cp "$found" "$prefix/lib/$lib.lib"
    done
    for file in "${WINDOWS_FILES[@]}"; do
        [ -f "$prefix/bin/$file" ] || die "FFmpeg did not install bin/$file"
    done
elif [ "$target" = macos ]; then
    for file in "${MACOS_FILES[@]}"; do
        [ -f "$prefix/lib/$file" ] || die "FFmpeg did not install lib/$file"
    done
else
    for file in "${LINUX_FILES[@]}"; do
        [ -f "$prefix/lib/$file" ] || die "FFmpeg did not install lib/$file"
    done
fi

# dav1d is linked into libavcodec, so its BSD licence ships with FFmpeg's LGPL.
{
    cat "$work/ffmpeg-$FFMPEG_VERSION/COPYING.LGPLv2.1"
    printf '\n\n%s\n\n' "----- dav1d $DAV1D_VERSION (linked into libavcodec) -----"
    cat "$work/dav1d-$DAV1D_VERSION/COPYING"
} >"$prefix/ffmpeg-LICENSE.txt"

say "installed FFmpeg $FFMPEG_VERSION into $prefix"
