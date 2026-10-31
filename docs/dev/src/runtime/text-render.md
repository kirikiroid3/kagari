# Text and Rendering

The engine describes each frame as a platform-neutral draw list, `Frame`. Text is shaped and laid
out on the CPU by `kagari-text` while the engine builds that list; `kagari-render` draws the list
with wgpu. This chapter follows a line of text from its `Text` value to pixels, then describes the
compositor and how the design resolution maps to a window.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `runtime_libs/kagari-text/src/lib.rs` | the public types: `Paragraph`, `Span`, `RubyText`, `ParagraphStyle`, `Layout`, `Glyph`, `GlyphKey`, `Mask`, `units` |
| `runtime_libs/kagari-text/src/fonts.rs` | `Fonts`: loaded faces, bold instances, line metrics, coverage, the fallback chain |
| `runtime_libs/kagari-text/src/shape.rs` | shaping through harfrust, grapheme segmentation, vertical orientation |
| `runtime_libs/kagari-text/src/breaking.rs` | UAX #14 opportunities, kinsoku, greedy line filling, hanging indent |
| `runtime_libs/kagari-text/src/layout.rs` | lines and columns, ruby, emphasis dots |
| `runtime_libs/kagari-text/src/raster.rs` | glyph coverage masks: skrifa outlines filled or stroked by zeno |
| `runtime_libs/kagari-engine/src/ui/text.rs` | `TextStyle`/`TextEffect` decoding, flattening a `Text` into spans, the layout cache |
| `runtime_libs/kagari-engine/src/story/reveal.rs` | reveal timing: units, speeds, pauses, marks |
| `runtime_libs/kagari-engine/src/frame.rs` | `Frame`, `Node`, `Reveal`, `ShaderEffect`, `Pose`, `MotionStage`, `TransitionDraw` |
| `runtime_libs/kagari-engine/src/screen/draw.rs`, `ui/draw.rs` | the screen and the mounted UI as `Frame` nodes |
| `runtime_libs/kagari-render/src/lib.rs` | `Renderer`: `render`, `render_offscreen`, `render_to_rgba`, thumbnails |
| `runtime_libs/kagari-render/src/plan.rs`, `text.rs`, `encode.rs` | the planning walk, text drawing, command encoding |
| `runtime_libs/kagari-render/src/pipelines.rs`, `shaders/*.wgsl` | built-in pipelines, bind group layouts, uniforms, shaders |
| `runtime_libs/kagari-render/src/shaders.rs` | user shader pipelines (`ShaderCache`), built from `kagari_shader::compose` |
| `runtime_libs/kagari-render/src/atlas.rs`, `textures.rs`, `layers.rs`, `readback.rs`, `viewport.rs` | glyph atlas, image and thumbnail textures, offscreen layers, readback, letterboxing |

## Overview

<!-- diagram: text-render-pipeline (mermaid-ish)
flowchart LR
  text["Text: value from bytecode"] ==> flatten["flatten: spans, rubies"] ==> layout["layout(): shape, break, place"] ==> lay["Layout: glyphs, units"] ==> node["Node::Text plus Reveal"]
  node ==>|Frame| plan["plan: one walk, verts, passes"] ==> encode["encode: one command encoder"] ==> design["design target: Rgba16Float, linear"]
  atlas["glyph atlas: R8 masks, skrifa + zeno"] ==> plan
  design ==> present["present: letterbox to surface"]
  design ==> resolve["resolve: sRGB RGBA8 readback"]
  design ==> still["still copy: dialog backdrop"]
-->
{{#include ../diagrams/text-render-pipeline.svg}}

`kagari-text` has no platform or GPU dependency: the engine calls `kagari_text::layout` and stores
the resulting `Layout` (`Rc`) in `Node::Text`; the renderer rasterizes glyph masks through
`Fonts::rasterize`, reading the fonts from the engine's `AssetStore`. Colours everywhere in a
`Frame` are premultiplied linear light; coordinates are design pixels, origin top left, y down.

## Fonts

All font assets are loaded at boot, in asset order, into one `Fonts` (`AssetStore::load_fonts`);
`FontId` is the load index. `Fonts::add` accepts TrueType, OpenType (CFF) and face 0 of a
collection. For each face it keeps:

- an unhinted skrifa scaler, and a second one for the `wght = 700` instance when the font has a
  `wght` axis (used for bold);
- the line ascender and descender from the font's horizontal metrics, or 0.88 / 0.12 em when the
  font has none.

A grapheme takes the first font that maps every character of it, trying the span's font, then the
paragraph's fallback (the system font, `System::font`), then every other loaded font in load
order; when none covers it, the span's font draws `.notdef`.

**Synthesis.** Bold without a `wght` axis strokes the outline by `size / 24` px under the fill.
Italic shears the outline by 0.2 px per px above the baseline. Both are part of the `GlyphKey`, so
they are cached like real glyphs.

## From `Text` to spans

A `Text` value is a tree of `Plain`, `Concat`, `Effect(base, TextEffect)`, `Ruby(base, reading)`
and `MonoRuby(base, readings)`. `ui/text.rs` decodes the `TextStyle` (12 fields: font, size,
colour, line height, letter spacing, alignment, outline, shadow, ruby size, emphasis style,
hanging indent, vertical) into a hashable `TextStyleSpec`, then flattens the tree:

- `Effect` decodes fields 0 to 7 of the `TextEffect` (colour, size multiplier, weight, italic,
  font, outline, shadow, emphasis) and combines them with the enclosing effect; the inner value
  wins per field (`TextEffect::and`). Fields 8 to 10 (reveal speed, pause, mark) are read by the
  reveal plan, not by layout.
- `Ruby`/`MonoRuby` open a ruby group; a ruby nested inside another ruby's base keeps its base and
  drops its reading.
- Consecutive pieces with the same style merge into one `Span`.
- Emphasis `Styled` maps to the style's emphasis (bold, italic or dots); `Dots` always draws
  emphasis dots.

Layouts are cached by (hash of the text, style, wrap width × 64); the cache is cleared when it
holds more than 1024 entries.

## Shaping, line breaking, ruby

`kagari_text::layout(fonts, paragraph)`:

1. **Shape.** Each span is segmented into extended grapheme clusters (`unicode-segmentation`) and
   shaped with harfrust, one run per font. In vertical text, Han, kana, CJK symbols and
   punctuation, full-width forms and Hangul stay upright; other graphemes are rotated 90°
   (`GlyphKey::rotate90`). Bold shapes with the bold instance when the font has one.
2. **Ruby.** Spans with the same ruby index form one base. A `Group` reading is shaped over the
   whole base, a `Mono` reading per base grapheme, at `ruby_size_px` in the base's font. A base
   narrower than its reading is padded by half the difference on each side.
3. **Break.** Break opportunities come from UAX #14 (`unicode-linebreak`), restricted by kinsoku
   rules: characters such as `、。）」ー` and small kana never start a line, `「『（` and other
   openers never end one; there is no opportunity inside a ruby base. Lines are filled greedily up
   to the limit (the width, or the column length in vertical text); a line without an opportunity
   breaks before the overflowing grapheme (one earlier when that grapheme may not start a line).
   U+0020 and U+3000 at a line end never cause an overflow. With `hanging_indent`, continuation
   lines of a line opening with `「`, `『` or `（` are indented past the opener. `\n` ends a line.
4. **Place.** Lines (or columns, top to bottom, right to left) are stacked at `line_height` × the
   line's largest font size and aligned (start, centre, end) within the widest line. Ruby glyphs
   are placed over (or beside) their bases, emphasis dots (U+FE45, or U+2022 when no font has it)
   over each emphasized grapheme.

The `Layout` holds the text box size, the number of reveal units and the glyphs. Each `Glyph` has
its `GlyphKey` (font, glyph id, size × 64, embolden, skew, rotate90), its baseline position, its
colour, outline and shadow, and its **unit**: the index of its base grapheme. Ruby and emphasis
glyphs carry their base's first unit, so they appear with it. `kagari_text::units(text)` counts
grapheme clusters without line breaks; the reveal plan uses the same function, so unit numbers in
the reveal and in the layout always agree.

## Reveal

`story/reveal.rs` turns the current line into timing. `RevealPlan::build` walks the `Text` once:

| Effect field | Effect on the plan |
| ------------ | ------------------ |
| `reveal` (8): `Player` | each unit takes the `text_speed` setting |
| `PerChar(d)` | each unit takes `d` |
| `Scaled(x)` | each unit takes `text_speed / x` (a factor ≤ 0 is ignored with a warning) |
| `Instant` | units take 0 ms |
| `pause` (9): `Time(d)` | the clock advances by `d` before the base |
| `pause`: click | closes the current segment: the reveal waits for a click |
| `mark` (10): `Mark(n)` | records the time of the first `Mark(n)` |

A `RevealRun` starts at the text start (`ScreenRuntime::text_start_ms`, after step entry) and is in
one of three states: `Revealing`, `WaitingClick { since }` (a segment finished and more follow) or
`Done { at }`. A click while revealing completes the current segment; a click at a pause
releases it; skip completes everything. `RevealRun` also implements `MarkTimes`, which resolves
the timeline items anchored on marks.

`RevealRun::reveal(t)` produces the `Reveal` of `Node::Text`: `All`, `Units(n)` (units below `n`
visible) or `UnitsWithFade { full, partial }` (unit `full` fades in at `partial`). The renderer
multiplies each glyph's alpha by `reveal_alpha(reveal, glyph.unit)`.

## Rasterization and the glyph atlas

`Fonts::rasterize(key, outline_px)` draws a skrifa outline into a zeno coverage mask at
`size_q / 64` px. `outline_px > 0` gives the stroked outline ring of width `2 × outline_px` instead
of the fill. Masks are uploaded on first use to the glyph atlas: `R8Unorm` pages of 2048×2048, at
most 4, packed by shelves. Page 0 also holds a 4×4 white block that `Fill` nodes sample, so fills
and glyphs share one pipeline. Glyph quads are snapped to whole pixels at the glyph origin.

## The `Frame` draw list

| `Node` | Meaning |
| ------ | ------- |
| `Group { transform, opacity, clip, children }` | children drawn into an isolated layer, then composited source-over; the renderer skips isolation with at most one child |
| `Filter { effect, children }` | children drawn into a layer, then run through a colour shader in screen space |
| `Mix { effect, opacity, children }` | children drawn into a layer, then composited over the backdrop by a mixing shader, which returns the final pixel |
| `Masked { effect, rect, children }` | children drawn into a layer whose alpha is multiplied by a mask shader evaluated over `rect` |
| `Motion { stages, pose, children }` | GPU motion: the stages (a transitional, then a local motion shader) compute a pose from `pose`; the children are displaced by `P·M(result)·M(pose)⁻¹·P⁻¹` |
| `Paint { rect, size, uv, texture, mask, flip_x, flip_y }` | generated content: a texture shader of `size` px (its `kg_size()`), optionally times a mask shader; `rect` shows its part `uv` (`0..1` for all of it), so a resized paint is stretched and a `Cover` paint cropped |
| `Image { rect, image, uv, flip_x, flip_y, tint }` | an image asset |
| `Fill { rect, color }` | a solid rectangle |
| `NineSlice { rect, image, insets, tint }` | a nine-slice image; insets in source pixels, CSS order |
| `Text { origin, layout, reveal, opacity }` | a laid-out paragraph |
| `Transition { bounds, kind, progress, from, to }` | two node lists blended by a transition; `TransitionDraw::Masked` switches pixels by a mask shader with the rule math |
| `Still { rect }` | the still captured by the last frame with `capture_still` |
| `Thumbnail { rect, slot, generation }` | a save thumbnail |

A `ShaderEffect` is a shader asset, its arguments packed in WGSL storage layout and its effect
time in seconds ([The Engine](./engine.md#shader-effects)). A `Pose` is an anchor position in
design pixels, a scale and a rotation in radians (clockwise, y down).

`Affine` is `[a b c d e f]` with `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.

### How the engine builds a frame

- **Mounts.** `ui::draw::frame_nodes` lays out the mounted trees, then draws the mounts bottom to
  top. Under the topmost `Dialog`, the modes below are drawn live once, in a frame with
  `capture_still = true`; later frames draw a `Still` and only the dialog and what is above it.
- **The game screen.** A `game_screen!` element draws the sampled `VisualScreen`, scaled from the
  design resolution into the element's rect and clipped to it. Entities are sorted by
  `(plane, z, id)`. `Background`, `Stage` and `Front` form one group under the camera transform
  `T(c)·S(zoom)·R(−rotation)·T(−c)·T(−offset)` (`c` the screen centre), wrapped in a `Filter` when
  the screen has a filter. The `OverWindow` plane and the HUD side image (sized by its layer's
  `Resize`, with its box's bottom-left corner at the screen's bottom-left corner, faded with the
  text window, hidden with it) are deferred and drawn after the mount's whole tree, so they sit
  above the text window.
- **An entity**, outermost first: a `Mix` with the entity's opacity when it has a mix, else a
  `Group` with its opacity; a `Filter` when it has a filter; inside, an unsized fill covering the
  screen (never moved), then a `Motion` when a travel plays or it has a local motion, holding a
  group with `T(position)·R(rotation)·S(scale)·T(−anchor × size)`, holding a `Masked` over the
  entity's box when it has a mask, holding the sized fill, the parts in part order (images or
  paints; part 0 placed by the anchor in the entity's box, the others at the origin; flips mirror
  them) and the label. Each part is sized by its layer's `Resize` (`screen/draw.rs`, `resized`):
  a box, and the content's rect centred in it, larger than the box only under `Cover`. The content
  is cropped to the box by cutting its uv range (`content_node`), not by a scissor, so the clip
  stays exact under the entity's transform, flips and motions; a paint is generated at its own
  `size` and stretched to the content rect, so `kg_size()` stays that size. The entity size is
  part 0's box (the new side's during a transition, `layer_box`), else the fill size, else the
  label size. `S(scale)` applies on top of the resize. Under a `Motion` a zero scale is drawn at
  1, so that the pose stays invertible and the stages decide the scale. A gated entity is drawn
  only while its `Setting<bool>` is true; this is how settings reach the screen without reaching
  the story.
- **Transitions** of image and label layers become `Node::Transition` over the union of both
  sides; `Cut` just shows the new side.
- **The text window.** The subtree that holds the `message` element (and no game screen) is the
  window. Its fills and image tints are multiplied by the `window_opacity` setting (text is kept),
  and the whole window fades with the HUD's text-window alpha.
- **Not drawn by `native_v0`**: movies. They are part of the logical screen (the consistency
  analysis sees them) but the compositor has no pipeline for them; `kagari build` rejects video
  assets for this profile.
- **Crash.** A crashed engine returns a black frame with the trap message
  ([The Engine](./engine.md#errors-traps-and-the-crashed-state)).

## The renderer

`Renderer::new(device, queue, surface_format)` builds the built-in pipelines once. Every frame is
drawn in two phases into the **design target**, an `Rgba16Float` texture at the frame size holding
linear, premultiplied colour:

1. **Plan** (`plan.rs`, `text.rs`): one walk over the node tree produces the vertices (six per
   quad, in target pixels), one uniform slot per parameterized draw (aligned to the device's
   dynamic-offset alignment, at least 256 bytes), the packed shader arguments (each at the
   device's storage-offset alignment), the motion compute dispatches and an ordered list of
   operations: passes (target, clear, draws) and copies. Consecutive draws with the same pipeline,
   bind group, scissor, uniforms and arguments merge.
2. **Encode** (`encode.rs`): the buffers are uploaded; one compute pass runs the motion stages,
   then every pass and copy is recorded into the same command encoder, which is submitted.

| Pipeline | Use |
| -------- | --- |
| `Image` | an image texel decoded to linear, times the vertex colour; premultiplied over |
| `Linear` | a linear layer or the still composited over |
| `Mask`, `MaskMax` | glyph and fill coverage times colour; `MaskMax` keeps the per-channel maximum (shadows) |
| `MaskComposite` | a layer times the red channel of a mask layer (`Node::Masked`) |
| `Blur` | one direction of a separable Gaussian blur |
| `Transition` | two layers combined by a transition |
| `Present`, `Resolve` | the design target onto the surface, or into the readback texture |
| user shaders | one per (shader asset, role), see below |

**Group 0.** Every raster pipeline binds the per-draw `Uniforms` (dynamic offset; its first 64
bytes are the user prelude's `KgFrame`: target size, `xf_slot`, `a`, `b`, `c`) and the motion
displacement rows `transforms` (`kagari_shader::TRANSFORMS_BYTES`, 256 slots × 2 `vec4`). The
vertex stage moves every vertex by the affine of slot `xf_slot`; slot 0 is the identity, written
once in `Renderer::new`.

**Colour.** Images are decoded by the engine to straight-alpha sRGB RGBA8 and uploaded
premultiplied in sRGB encoding (`c·a/255`, rounded) to `Rgba8Unorm` textures; the shader
un-premultiplies, converts to linear and premultiplies again (`decode_image`). Everything after
that, including compositing and every user shader, happens in linear light. `Present` writes
linear light into an sRGB surface (the hardware encodes) or encodes by hand for a non-sRGB
surface; `Resolve` writes into an `Rgba8UnormSrgb` texture. Output alpha is always 1.

**Groups.** A group with more than one child is drawn into an offscreen layer (a pooled
`Rgba16Float` texture) at alpha 1, then composited with `Linear` at the group's alpha; otherwise its
child is drawn directly with the alpha multiplied in. A `clip` becomes a scissor rectangle (the
transformed bounds). There are no blend modes: an entity's compositing is a mix shader.

**User shaders** (`shaders.rs`, [Shaders](./shaders.md)). `ShaderCache::prepare(asset, role)` reads
`AssetStore::shader_source`, runs `kagari_shader::validate`, then creates the module from
`kagari_shader::compose(role, source)` and the pipeline inside a validation error scope. A shader
that fails is warned about once (``shader `path` (role): error``) and cached as unavailable; one
whose source is not loaded is warned about once and retried. Layouts follow the preludes: raster
group 0 as above; group 1 is `tex1` (Color, Texture), `tex2` (Mix) or empty (Mask); group 2 is the
arguments (read-only storage, dynamic offset, `MAX_ARGS_BYTES` bound). Color and Texture blend
premultiplied over; Mix and Mask replace. Motion shaders are compute pipelines: group 0 the
`KgMotionIn` uniform (`MotionUniform`, 112 bytes, in the uniform buffer), group 1 the motion
states and the displacement rows (read-write storage), group 2 the arguments. Without storage
buffers (`max_storage_buffers_per_shader_stage == 0`) no user shader is drawn; without compute
(`max_compute_workgroups_per_dimension == 0`) no motion shader is; both are logged once.

| Node | Plan | When the shader is unavailable |
| ---- | ---- | ------------------------------ |
| `Filter` | children into layer L at alpha 1; a full-target quad with the colour shader sampling L, at the context alpha, `a = (time, …)`, `b = (0, 0, target size)` | L composited with `Linear` |
| `Mix` | children into layer L; the target under the scissor copied into a backdrop layer; a full-target quad with the mix shader over (L, backdrop), `a.z` = alpha × opacity; it replaces the target's pixels | L composited at alpha × opacity |
| `Masked` | children into layer C; the mask shader over `rect` (current transform and motion slot) into layer M; a full-target `MaskComposite` of (C, M) | C composited |
| `Paint` | the mask (if any) into layer M as for `Masked`; the texture shader over `rect` with corners mirrored for flips, sampling M (or the white texture), `b = (0, 0, rect size)` | nothing; without the mask if only the mask fails |
| `Motion` | a new motion slot k (1..255); per available stage a compute dispatch with P = the current transform, the pose, `from`/`to`, `(time, progress)`, slot k and `chained` after the first; children planned in slot k | children planned unchanged (also when the slots run out, warned once per frame) |
| `Transition` + `Masked` | the mask shader over `bounds` into layer M, then the transition with kind 5 sampling M in screen space | crossfade |

**Motion slots.** A compute dispatch writes the pose its stage computed into `states[k]` (the next
stage reads it when chained) and the displacement `P·M(state)·M(pose)⁻¹·P⁻¹` into rows `2k`,
`2k+1`. Every draw below the `Motion` uses a uniform slot with `xf_slot = k` (`Planner::base_for`
caches one per slot for plain geometry); full-target composites use slot 0, since their layers
already hold displaced content. Below a `Motion` the geometry moves on the GPU, so clips,
transition bounds and shadow blur bounds no longer narrow the scissor. Motions do not nest: an
inner `Motion` replaces the outer displacement for its children.

**Text.** Glyphs are drawn in three passes: all shadows, then all outlines, then all fills, so an
outline never covers a neighbouring glyph's fill. Shadows are grouped by blur radius: each bucket is
drawn with `MaskMax` into a layer, blurred horizontally and vertically with σ = blur / 2 (at most
32, a 64 px blur; radius ⌈3σ⌉ taps), and composited. A decorated text drawn at opacity below 1 goes
through a layer first, so overlapping outlines and fills do not darken.

**Transitions** draw `from` and `to` into two layers and combine them over `bounds`:

| Kind | Rule |
| ---- | ---- |
| `Crossfade` | linear mix by progress |
| `Rule { mask, softness, reverse }` | the mask image's luma (0 to 255, inverted when `reverse`) against a threshold that sweeps `0..255 + softness`; `softness` widens the edge |
| `Wipe { dir }` | a hard edge moving across the bounds |
| `Blinds { dir, count }` | the wipe repeated in `count` stripes |
| `ThroughColor { color, hold_fraction }` | fade to the colour, hold, fade to `to` |
| `Masked { mask, softness, reverse }` | the rule math on the mask shader's value × 255, drawn into a layer over `bounds` |

**The still.** When `frame.capture_still` is set, the design target is copied into the still
texture after the frame; `Node::Still` draws it with `Linear`. The runtime must render such a
frame even when it cannot present it ([The Desktop Runtime](./desktop.md#windowed-mode)).

**Thumbnails.** The engine holds save thumbnails as RGBA8 with a generation number;
`stale_thumbnails(frame)` lists the `Thumbnail` nodes whose generation the renderer lacks and
`set_thumbnail` uploads them. A `Thumbnail` node whose generation is not uploaded draws nothing.

**Caches and limits.** Image textures are evicted least recently used beyond 512 MiB (`w·h·4`
summed). Layers are pooled per size. User pipelines live as long as the renderer. A frame larger
than the device's maximum texture dimension is clipped with a warning. A readback larger than the
limit fails and returns an empty buffer.

## Design resolution and the window

`System::resolution` is the design resolution. Everything the engine computes (layout, the screen,
input coordinates) is in design pixels, and the renderer always draws at that size. Mapping to the
output happens only at the end:

- **Present** (`Viewport::letterbox`): the largest rectangle with the design aspect ratio that
  fits the surface, rounded and centred; the rest is cleared black. The design target is sampled
  bilinearly, so a window larger or smaller than the design size is scaled, never reflowed.
- **Input**: `Viewport::to_design` maps a surface pixel back to design pixels (not clamped; an
  empty viewport gives `(-1, -1)`, which means "no hover").
- **Readback** (`render_to_rgba(frame, assets, w, h)`): the design target stretched to `w × h`,
  sRGB RGBA8, alpha 255. Headless captures use the design size; thumbnails use 320×180.
- **Window size**: the desktop runtime opens the window at the design size in logical pixels,
  scaled down to fit 90 % of the monitor; on a HiDPI display the surface therefore has more
  physical pixels than the design size, and the design target is upscaled.

## Invariants

- The renderer draws exactly what the `Frame` says; all policy (what is visible, reveal, window
  opacity, gating) is decided in the engine. A renderer for another platform must give the same
  picture from the same `Frame`.
- Colours in a `Frame` and a `Layout` are premultiplied linear; images and thumbnails cross the
  engine/renderer boundary as straight-alpha sRGB RGBA8.
- Layout units and reveal units come from `kagari_text::units`; a ruby or emphasis glyph carries
  its base's first unit.
- `kagari-text` stays free of platform and GPU dependencies; it is shared with any future runtime.
- Text layout does not depend on the window size: the same `Frame` lays out identically at any
  output size.

## When you change this

- Text styles, effects, ruby or reveal: the std declarations in
  `core/kagari-std/kis/std/narrative/text.kis`, the decoders in `ui/text.rs` and
  `story/reveal.rs`, `values/table.rs` when field order changes, the STD page
  [`std::narrative::text`](https://kagari.suitsu31.club/en/std/narrative-text/) (en and ja), and
  `kagari-text` tests (they use the bundled test font `tests/fonts/kagari-test-cjk.otf`).
- Transitions or a new `Node`: `frame.rs`, `screen/draw.rs` or `ui/draw.rs`, the planner, the
  shaders, `kagari-render` tests, and the STD pages `screen.md` / `screen-anim.md`.
- Shader bindings or `Uniforms`: the preludes in `core/kagari-shader/src/wgsl/`, the layouts in
  `pipelines.rs` and `kagari-render`'s layout tests (`uniforms_start_with_the_user_frame`,
  `motion_uniform_matches_the_prelude`) together.
- A new drawable (movies, shaders, filters): the compositor, the profile table in
  `kagari-compat` (a new profile if `native_v0`'s promise changes), and the STD appendix
  [Runtime Profiles](https://kagari.suitsu31.club/en/std/appendix/profiles/).
- Check visual changes with headless captures before and after
  ([The Desktop Runtime](./desktop.md#headless-mode)).
