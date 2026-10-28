# Runtime Profiles

A **runtime profile** names what a runtime provides: which std items it implements and which asset
kinds and file formats it decodes. A project lists the profiles it must run on in `kagari.toml`,
and `kagari build` checks every asset the program uses against each of them:

```toml
[build]
runtime-compatibility = ["native_v0"]
```

`runtime-compatibility` defaults to `["native_v0"]`; the list must not be empty, and an unknown name
is a manifest error. See [Projects](../../../../reference/en/src/tools/projects.md).

## Profiles

|Profile|Runtime|
|---|---|
|`native_v0`|The desktop runtime (`kagari-desktop-runtime`), version 0|

`native_v0` is currently the only profile.

## `native_v0` support matrix

### By module

|Module|Support in `native_v0`|
|---|---|
|`std` (types, traits, `asset`, `system_font`)|Supported, except that `VideoAsset` files cannot be used (see below)|
|`std::time`|Supported|
|`std::narrative`|Supported: `Say`, `Choice` with disabled arms, `rng_choose`, `Meta`, every `Narrative` method. Timed input is Planned and exists in no profile|
|`std::narrative::text`|Supported: ruby and mono ruby, every text effect (styles, emphasis dots, reveal speeds, pauses, marks), every `TextStyle` field including `hanging_indent` and `vertical`|
|`std::screen`|Supported, except movie layers (`Entity::movie`, `MovieLayer`), which need video assets|
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
|`ImageAsset`|PNG; baseline and progressive JPEG; lossy, lossless and extended (alpha or metadata) WebP|JPEG with arithmetic, lossless, hierarchical or 12-bit coding; animated WebP; GIF; BMP; AVIF; JPEG XL|
|`AudioAsset`, `VoiceAsset`|Ogg Vorbis; FLAC; WAV (PCM); WAV (IEEE float); MP3|Ogg Opus; Ogg FLAC; other Ogg codecs; compressed WAV; AAC/M4A|
|`FontAsset`|TrueType; OpenType (CFF); TrueType/OpenType collections (`.ttc`)|WOFF; WOFF2|
|`VideoAsset`|—|every format: video assets are not supported|
|`ColorShader`, `MixShader`, `TextureShader`, `MaskShader`, `LocalMotionShader`, `TransitionalMotionShader`|WGSL source (UTF-8 text) that validates as its role|non-text data; WGSL with errors|

## Using an unsupported item

**Unsupported assets fail the build.** For every asset the program uses that a listed profile
cannot decode, `kagari build` prints one error and produces no output:

```text
error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
error: asset `music/theme.ogg` (audio): Ogg Opus is not supported by runtime profile `native_v0`
error: asset `bgimage/bad.jxl` (image): JPEG XL is not supported by runtime profile `native_v0`
```

So a game that targets `native_v0` cannot use a `VideoAsset` constant anywhere. Convert other
assets to a supported format. For the items that take video assets, this means:

|Item|At `kagari build`|At runtime in `native_v0`|
|---|---|---|
|`MovieLayer::change(…)` (a `Movie` holds a `VideoAsset`)|rejected (video asset)|—|
|`system::watched(VIDEO)`|rejected (video asset)|—|
|`Entity::movie()`, `MovieLayer::clear(…)`|accepted|kept in the logical screen (saves, equality, consistency) but never drawn; there is nothing to draw|

The renderer of the desktop runtime has no movie playback, so movie layers never produce pixels
in `native_v0`, and the `watched` set is never filled.

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
