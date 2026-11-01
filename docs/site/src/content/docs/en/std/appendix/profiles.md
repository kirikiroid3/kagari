---
title: "Runtime Profiles"
---

A **runtime profile** names what a runtime provides: which std items it implements and which asset
kinds and file formats it decodes. A project lists the profiles it must run on in `kagari.toml`,
and `kagari build` checks every asset the program uses against each of them:

```toml
[build]
runtime-compatibility = ["native_v0"]
```

`runtime-compatibility` defaults to `["native_v0"]`; the list must not be empty, and an unknown name
is a manifest error. See [Projects](../../reference/tools/projects.md).

## Profiles

|Profile|Runtime|
|---|---|
|`native_v0`|The desktop runtime (`kagari-desktop-runtime`), version 0|

`native_v0` is currently the only profile.

## `native_v0` support matrix

### By module

|Module|Support in `native_v0`|
|---|---|
|`std` (types, traits, `asset`, `system_font`)|Supported|
|`std::time`|Supported|
|`std::narrative`|Supported: `Say`, `Choice` with disabled arms, `rng_choose`, `Meta`, every `Narrative` method. Timed input is Planned and exists in no profile|
|`std::narrative::text`|Supported: ruby and mono ruby, every text effect (styles, emphasis dots, reveal speeds, pauses, marks), every `TextStyle` field including `hanging_indent` and `vertical`|
|`std::screen`|Supported, including movie layers (`Entity::movie`, `MovieLayer`)|
|`std::screen::shader`, `std::shaders`|Supported. Every shader is validated at `kagari build`; on a GPU without storage buffers (or without compute, for the motion roles) the runtime draws the content without the effect|
|`std::screen::anim`|Supported|
|`std::screen::audio`|Supported|
|`std::stage`|Supported|
|`std::system`, `std::system::nav`, `std::system::saves`|Supported|
|`std::ui`, `std::ui::style`|Supported|

### Asset kinds and formats

Formats are recognized by file content, not by extension.

|Asset type|Supported formats|Not supported|
|---|---|---|
|`ImageAsset`|PNG; baseline and progressive JPEG; lossy, lossless and extended (alpha or metadata) WebP; BMP (uncompressed, RLE or bitfields); still JPEG XL (RGB or greyscale)|JPEG with arithmetic, lossless, hierarchical or 12-bit coding; animated WebP; GIF; BMP with embedded JPEG or PNG, or CMYK; AVIF; animated or CMYK JPEG XL|
|`AudioAsset`, `VoiceAsset`|Ogg Vorbis; Ogg FLAC; FLAC; WAV (PCM); WAV (IEEE float); MP3; mono or stereo AAC-LC (ADTS or MP4/M4A); mono or stereo ALAC (MP4/M4A)|Ogg Opus; other Ogg codecs; compressed WAV; HE-AAC and other AAC profiles; AAC or ALAC with more than two channels; MP4/M4A with another codec|
|`FontAsset`|TrueType; OpenType (CFF); TrueType/OpenType collections (`.ttc`)|WOFF; WOFF2|
|`VideoAsset`|WebM with VP9 or AV1 video and Opus or Vorbis audio, or no audio; MP4 with H.264 video and AAC-LC audio, or no audio|VP8 and other video codecs; several video tracks; other audio codecs; WebM with other tracks; other containers|
|`ColorShader`, `MixShader`, `TextureShader`, `MaskShader`, `LocalMotionShader`, `TransitionalMotionShader`|WGSL source (UTF-8 text) that validates as its role|non-text data; WGSL with errors|

## Using an unsupported item

**Unsupported assets fail the build.** For every asset the program uses that a listed profile
cannot decode, `kagari build` prints one error and produces no output:

```text
error: asset `movie/intro.webm` (video): WebM/Matroska with another codec, several video tracks or an unreadable header is not supported by runtime profile `native_v0`
error: asset `music/theme.ogg` (audio): Ogg Opus is not supported by runtime profile `native_v0`
error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
```

Convert such assets to a supported format. For a movie, for example:

```sh
ffmpeg -i in.mov -c:v libvpx-vp9 -c:a libopus out.webm
```

Use `-c:v libsvtav1` for AV1, or `-c:v libx264 -c:a aac` with an `.mp4` output for H.264.

**Invalid shaders fail the build.** `kagari build` compiles every shader the program uses, the
`std::shaders` ones included, as the role its type names, and prints one error per invalid shader
and role, with the line in the `.wgsl` file when the error lies there:

```text
error: shader `shader/broken.wgsl` (color): line 4: no definition in scope for identifier: `undefined_name`
```

Because `native_v0` is the only profile and `runtime-compatibility` cannot be empty, every
`kagari build` checks against `native_v0`: a project cannot opt out of this check today.

**Missing std items fail at startup.** A `.kagari` file built against a newer `std` may call a
builtin the installed runtime does not provide. The runtime then refuses to start the game with
"the bytecode imports `…`, which this runtime does not provide"; a `.kagari` file that requires an
engine feature the runtime does not know is refused the same way. Build the game with the
toolchain that matches the runtime you ship.
