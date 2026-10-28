# `std::screen`

> **Stability:** Stable — the screen a render transforms: entities and their components, the Hud,
> the choice menu, the camera, the screen filter and the audio channels.

A **render** is a `const fn` from the screen before a step to the screen after it. It changes the
screen in place through projections, one chain per statement, and every change states its
animation:

```kis
const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

Every item of this module is `const fn` or plain data, so renders are evaluated at compile time
and the compiler can compare screens. The timeline methods of `Screen` are described here; the
animation values they schedule are in [`std::screen::anim`](./screen-anim.md), and the audio
values in [`std::screen::audio`](./screen-audio.md). Character helpers built on this module are in
[`std::stage`](./stage.md); complete examples are in [Staging Recipes](./guides/staging.md).

The prelude re-exports `Screen`, `Render`, `EntityId`, `Part`, `Point`, `Offset`, `Size`,
`Anchor`, `Color`, `Plane`, `Visibility`, `point`, `offset`, `rgb`, `hex`, `BLACK`, `WHITE` and
`TRANSPARENT`. Every other item needs a `use std::screen::…`.

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Render`](#render) | type | `const fn(Screen) -> Screen` |
| [`EntityId`](#entityid), [`BACKGROUND`](#background) | struct, constant | entity identity |
| [`Part`](#part), [`MAIN`](#main) | struct, constant | image layer key |
| [`Point`](#point), [`Offset`](#offset), [`Size`](#size), [`Insets`](#insets), [`Anchor`](#anchor), [`Angle`](#angle) | structs + constructors | geometry |
| [`Color`](#color) and constants | struct + constructors | sRGB colour |
| [`Plane`](#plane), [`Visibility`](#visibility), [`Flip`](#flip), [`VAlign`](#valign), [`BlendMode`](#blendmode) | enums | layout options |
| [`Screen`](#screen) | builtin struct | the screen and the step's timeline |
| [`Entity`](#entity) | builtin struct | a thing on the screen |
| [`Prop`](#prop), [`Change`](#change) | builtin structs | interpolated properties |
| [`ImageLayer`](#imagelayer), [`LabelLayer`](#labellayer), [`Movie`](#movie), [`MovieLayer`](#movielayer), [`Swap`](#swap) | structs | layers |
| [`Hud`](#hud) | builtin struct | the game view's chrome |
| [`ChoiceMenu`](#choicemenu) | builtin struct | presentation of the step's choice |
| [`Camera`](#camera) | builtin struct | the camera over the lower planes |

## The logical screen {#the-logical-screen}

The **logical screen** of a step is the persistent state after its render:

- the entities with their components;
- the Hud, the camera and the screen filter;
- the audio channels.

Snapshots, screen equality and the consistency check use the logical screen only. The timeline,
scheduled animations, one-shot sounds and the `ChoiceMenu` belong to the step and are not part of
it.

Animations never change the start or end state: a tween or transition runs from the old value to
the value set by `change`, and a motion ends at identity. So a click, skip, or the `animations`
setting being off can never change the logical screen.

## Step entry {#step-entry}

Before the step's render runs, the engine resets the step-only state:

- the timeline cursor goes to (entry, 0 ms);
- scheduled animations are completed;
- one-shot cues are dropped unless they are already playing;
- the `ChoiceMenu` is emptied and set to `Middle`;
- the text start goes to (entry, 0).

A step without `render` gets the same reset.

## Type: `Render` {#render}

```kis
pub type Render = const fn(Screen) -> Screen;
```

A render: the screen before a step to the screen after it. A step's `render =` field takes a
`Render`, either a named `const fn` or a closure:

```kix
s!(render = |mut screen| {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen
});
```

A render whose output does not depend on its input is a keyframe; see
[`Screen::empty()`](#screen-empty).

## Geometry {#geometry}

Positions and lengths are pixels in the design resolution `System::resolution`. Entity ids are
studio constants.

### Struct: `EntityId` {#entityid}

```kis
pub struct EntityId(pub u32);
```

The identity of an entity. Studios declare ids as constants:
`const HANAKO: EntityId = EntityId(1);`. Ids also order entities that share a plane and `z`; see
[Drawing order](#drawing-order).

### Constant: `BACKGROUND` {#background}

```kis
pub const BACKGROUND: EntityId = EntityId(0);
```

Reserved for the background entity of `Screen::empty()`; studio ids start at 1.
[`Screen::background()`](#screen-background) is its main image.

### Struct: `Part` {#part}

```kis
pub struct Part(pub u8);
```

An image layer of an entity. Parts draw in ascending order, so one entity can stack a body, a face
and an overlay.

### Constant: `MAIN` {#main}

```kis
pub const MAIN: Part = Part(0);
```

The main part, which gives the entity its size. [`Entity::image()`](#entity-image) is
`part(MAIN)`.

### Struct: `Point` {#point}

```kis
pub struct Point {
    pub x: i32,
    pub y: i32,
}
pub const fn point(x: i32, y: i32) -> Point
```

A point in design-resolution pixels; `point(x, y)` builds one. Kind: `const fn` · Builtin: no.

### Struct: `Offset` {#offset}

```kis
pub struct Offset {
    pub dx: i32,
    pub dy: i32,
}
pub const fn offset(dx: i32, dy: i32) -> Offset
```

A displacement in pixels; `offset(dx, dy)` builds one. Used by the camera and by motion keys.
Kind: `const fn` · Builtin: no.

### Struct: `Size` {#size}

```kis
pub struct Size {
    pub w: u32,
    pub h: u32,
}
pub const fn size(w: u32, h: u32) -> Size
```

A size in pixels; `size(w, h)` is `w × h`. Used by [`Entity::set_fill_size`](#entity-set_fill_size).
Kind: `const fn` · Builtin: no.

### Struct: `Insets` {#insets}

```kis
pub struct Insets {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}
pub const fn insets(top: i32, right: i32, bottom: i32, left: i32) -> Insets
pub const fn uniform(n: i32) -> Insets
```

Distances from the four edges of a box, in pixels. `insets` takes CSS order: top, right, bottom,
left. `uniform(n)` is `n` on every edge. Kind: `const fn` · Builtin: no.

### Struct: `Anchor` {#anchor}

```kis
pub struct Anchor {
    pub x: Decimal,
    pub y: Decimal,
}
pub const TOP_LEFT: Anchor = Anchor { x: 0.0, y: 0.0 };
pub const CENTER: Anchor = Anchor { x: 0.5, y: 0.5 };
pub const BOTTOM_CENTER: Anchor = Anchor { x: 0.5, y: 1.0 };
```

The point of an entity that sits at its position, as fractions of the entity's own size,
`0.0..=1.0`. `TOP_LEFT` is the top-left corner (the spawn default), `CENTER` the centre,
`BOTTOM_CENTER` the middle of the bottom edge (usual for character sprites). Scale and rotation
pivot on the anchor.

### Struct: `Angle` {#angle}

```kis
pub struct Angle {
    pub degrees: Decimal,
}
pub const fn degrees(d: Decimal) -> Angle
```

A rotation, clockwise; `degrees(d)` is a rotation of `d` degrees. Kind: `const fn` · Builtin: no.

## Colour {#colour}

### Struct: `Color` {#color}

```kis
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
```

An sRGB colour with alpha.

### `rgb(r, g, b)`, `rgba(r, g, b, a)`, `hex(rgb)` {#color-constructors}

```kis
pub const fn rgb(r: u8, g: u8, b: u8) -> Color
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color
pub const fn hex(rgb: u32) -> Color
```

- `rgb` returns an opaque colour (alpha 255); `rgba` takes alpha explicitly.
- `hex(0xRRGGBB)` returns the opaque colour; bits above `0xFFFFFF` are ignored. `hex(0x382000)`
  is `rgb(0x38, 0x20, 0x00)`.
- Kind: `const fn` · Builtin: no.

### Constants: `BLACK`, `WHITE`, `TRANSPARENT` {#color-constants}

```kis
pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
```

Opaque black, opaque white, and fully transparent.

## Planes, flips and blend modes {#planes-flips-and-blend-modes}

### Enum: `Plane` {#plane}

```kis
pub enum Plane {
    Background,
    Stage,
    Front,
    OverWindow,
}
```

A drawing plane. The text window is drawn by the game view between `Front` and `OverWindow`. The
camera and the screen filter apply to `Background`, `Stage` and `Front` only.

### Enum: `Visibility` {#visibility}

```kis
pub enum Visibility {
    Shown,
    Hidden,
}
```

Whether a Hud part is shown. A `Prop<Visibility>` change interpolates as a fade.

### Enum: `Flip` {#flip}

```kis
pub enum Flip {
    None,
    Horizontal,
    Vertical,
    Both,
}
```

A mirror of an entity's drawing; set with [`Entity::set_flip`](#entity-set_flip).

### Enum: `VAlign` {#valign}

```kis
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}
```

Vertical placement; used by [`ChoiceMenu::set_align`](#choicemenu-set_align).

### Enum: `BlendMode` {#blendmode}

```kis
pub enum BlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
```

How an entity is composited over what is drawn below it. The variants follow the CSS blend modes;
`Add` is additive blending. The spawn default is `Normal`.

## Filters and shaders {#filters-and-shaders}

> **Stability:** Tentative — `native_v0`, today the only runtime profile, rejects `.wgsl` shader
> assets at build, so filters and shader content cannot be used yet; the native renderer draws
> none of them (see [Runtime Profiles](./appendix/profiles.md#using-an-unsupported-item)). Only `change(None)` builds; it stays on the
> logical screen and has no visible effect.

- Filters, weather and particles are all `ShaderAsset`s: WGSL shader files (`.wgsl`).
- A **filter** post-processes what is drawn. An entity has its own filter
  ([`Entity::filter`](#entity-filter)); the screen filter ([`Screen::filter`](#screen-filter))
  applies to the `Background`, `Stage` and `Front` planes. `None` is no filter.
- Weather and particles are entities whose **`shader`** ([`Entity::shader`](#entity-shader)) is
  set: the shader generates the entity's content, which is drawn in the entity's plane and ordered
  by its `z`, like any other entity.
- No shader ships with `std`; a studio provides its own.

```kis
const SEPIA: ShaderAsset = asset("sepia.wgsl");

screen.filter().change(Some(SEPIA)).apply_animation(tween(ms(500)));
```

## Struct: `Screen` {#screen}

```kis
#[builtin] pub struct Screen;
```

The screen a step shows, and the step's timeline. `Screen` is an opaque builtin handle: a render
reaches its state only through the methods below. Screens compare with `==` (they implement
`Eq`); equality uses the [logical screen](#the-logical-screen) only.

### `Screen::empty()` {#screen-empty}

```kis
#[builtin] pub const fn empty() -> Screen;
```

- Returns: a screen with only the opaque `BACKGROUND` entity (plane `Background`, opacity 1.0, no
  image).
- The Hud has the text window and quick menu `Shown` and no side image.
- The camera has offset 0, zoom 1.0 and rotation 0.
- The screen filter is `None`.
- Every audio channel is silent with volume 1.0.
- Kind: `const fn` · Builtin: yes.

A render whose output does not depend on its input is a keyframe; `Screen::empty()` is the usual
way to write one:

```kis
const fn roof_evening(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(BG::ROOF_EVENING).apply_animation(fade(ms(500)));
    screen
}
```

### `screen.entity(id)` {#screen-entity}

```kis
#[builtin] pub const fn entity(&mut self, id: EntityId) -> &mut Entity;
```

- `id` `EntityId` — the entity.
- Returns: `&mut Entity` — the entity, spawned with default components if it is absent.
- Kind: `const fn` · Builtin: yes.

#### Spawning {#spawning}

`entity(id)` creates the entity if it is absent, with these defaults:

| Component | Default |
| --------- | ------- |
| plane, z | `Stage`, 0 |
| position, anchor | (0, 0), `TOP_LEFT` |
| scale, rotation | 1.0, 0° |
| **opacity** | **0.0** |
| filter, shader | `None`, `None` |
| fill, fill size | `TRANSPARENT`, `None` (whole screen) |
| layers | no parts, no label, no movie |
| flip, blend, gate | `Flip::None`, `Normal`, `None` |

A spawned entity is invisible, so every appearance states its animation:

```kis
screen.entity(HANAKO).image().change(HANAKO::SMILE).apply_animation(cut());
screen.entity(HANAKO).opacity().change(1.0).apply_animation(tween(ms(300)));
```

`entity(id)` on an id with a pending despawn cancels the despawn; the entity keeps its current
values.

### `screen.contains(id)` {#screen-contains}

```kis
#[builtin] pub const fn contains(&self, id: EntityId) -> bool;
```

- Returns: `bool` — whether entity `id` is on the logical screen. A despawned entity that is
  still fading out is not.
- Kind: `const fn` · Builtin: yes.

### `screen.entities(plane)` {#screen-entities}

```kis
#[builtin] pub const fn entities(&self, plane: Plane) -> Vec<EntityId>;
```

- Returns: `Vec<EntityId>` — the ids of the logical screen on `plane`, ascending.
- Kind: `const fn` · Builtin: yes.

### `screen.despawn(id)` {#screen-despawn}

```kis
#[builtin] pub const fn despawn(&mut self, id: EntityId);
```

Removes entity `id` from the logical screen immediately. It is still drawn until its scheduled
animations end, and then removed. So a fade-out followed by `despawn` fades out, and the next
step does not inherit the entity. Kind: `const fn` · Builtin: yes.

```kis
screen.entity(CARD).opacity().change(0.0).apply_animation(tween(ms(500)));
screen.despawn(CARD);
```

### `screen.background()` {#screen-background}

```kis
pub const fn background(&mut self) -> &mut ImageLayer {
    self.entity(BACKGROUND).image()
}
```

The main image of the `BACKGROUND` entity. Kind: `const fn` · Builtin: no.

```kis
screen.background().change(BG::ROOF).apply_animation(fade(ms(500)));
```

### `screen.hud()` {#screen-hud}

```kis
#[builtin] pub const fn hud(&mut self) -> &mut Hud;
```

The [Hud](#hud) the game view draws. Kind: `const fn` · Builtin: yes.

### `screen.choice_menu()` {#screen-choice_menu}

```kis
#[builtin] pub const fn choice_menu(&mut self) -> &mut ChoiceMenu;
```

The [choice menu](#choicemenu) of this step. Kind: `const fn` · Builtin: yes.

### `screen.camera()` {#screen-camera}

```kis
#[builtin] pub const fn camera(&mut self) -> &mut Camera;
```

The [camera](#camera) over the `Background`, `Stage` and `Front` planes. Kind: `const fn` ·
Builtin: yes.

### `screen.filter()` {#screen-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<ShaderAsset>>;
```

The shader that post-processes the `Background`, `Stage` and `Front` planes; `None` is no filter.
See [Filters and shaders](#filters-and-shaders). Kind: `const fn` · Builtin: yes · Profile: `native_v0`
rejects shader assets at build; the native renderer does not draw filters.

### `screen.music()`, `screen.ambience(channel)` {#screen-music}

```kis
#[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
#[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
```

The music channel and the four ambience channels `ambience(0)` to `ambience(3)`. See
[Channels](./screen-audio.md#channels). Kind: `const fn` · Builtin: yes.

### `screen.play(sound)`, `screen.stop_sounds(fade_out)` {#screen-play}

```kis
#[builtin] pub const fn play(&mut self, sound: audio::Sound);
#[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
```

`play` plays a one-shot sound at the cursor plus its delay. `stop_sounds` fades out, over
`fade_out`, every one-shot sound playing at the cursor. See
[`Sound`](./screen-audio.md#sound). Kind: `const fn` · Builtin: yes.

### `screen.replay_bgm()` {#screen-replay_bgm}

```kis
#[builtin] pub const fn replay_bgm(&mut self);
```

Restarts the music channel's track from its start at the cursor. Without it, a track that is
already playing carries over to later steps without restarting. See
[The same-track rule](./screen-audio.md#the-same-track-rule). Kind: `const fn` · Builtin: yes.

### Timeline methods {#timeline-methods}

```kis
#[builtin] pub const fn wait(&mut self, duration: Duration);
#[builtin] pub const fn barrier(&mut self);
#[builtin] pub const fn at_mark(&mut self, mark: Mark);
#[builtin] pub const fn at_entry(&mut self);
#[builtin] pub const fn begin_text(&mut self);
```

| Method | Effect |
| ------ | ------ |
| `wait(d)` | advances the cursor by `d` |
| `barrier()` | moves the cursor to the end of everything scheduled so far |
| `at_mark(m)` | moves the cursor to the time the text reaches `m` |
| `at_entry()` | moves the cursor back to the step entry |
| `begin_text()` | starts the text reveal and the voice at the cursor |

The cursor model is described in [The timeline](./screen-anim.md#the-timeline). Kind: `const fn` ·
Builtin: yes.

## Struct: `Entity` {#entity}

```kis
#[builtin] pub struct Entity;
```

A thing on the screen: a transform, an opacity, a filter, a shader, a fill and layers. The
component set is fixed; studios add no components.

| Kind of member | Members | Changed with |
| -------------- | ------- | ------------ |
| Interpolated properties | `position`, `scale`, `rotation`, `opacity`, `filter`, `shader`, `fill` | `change(v).apply_animation(Tween)` |
| Layers | `part(p)`, `image()`, `label()`, `movie()` | `change(x).apply_animation(Transition)` |
| Layout settings | `set_plane`, `set_z`, `set_anchor`, `set_flip`, `set_blend`, `set_fill_size`, `gate` | direct setters, not interpolable |
| Relative motion | `apply_animation(Motion)` | see [`Motion`](./screen-anim.md#motion) |

### Interpolated properties {#entity-properties}

```kis
#[builtin] pub const fn position(&mut self) -> &mut Prop<Point>;
#[builtin] pub const fn scale(&mut self) -> &mut Prop<Decimal>;
#[builtin] pub const fn rotation(&mut self) -> &mut Prop<Angle>;
#[builtin] pub const fn opacity(&mut self) -> &mut Prop<Decimal>;
```

- `position` is where the anchor point sits.
- `scale` is a factor around the anchor; `rotation` is clockwise around the anchor.
- `opacity` is a fraction `0.0..=1.0`; spawned entities start at 0.0.
- Kind: `const fn` · Builtin: yes.

<a id="entity-shader"></a>

### `entity.filter()`, `entity.shader()` {#entity-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<ShaderAsset>>;
#[builtin] pub const fn shader(&mut self) -> &mut Prop<Option<ShaderAsset>>;
```

`filter` is the shader that post-processes this entity; `None` is no filter. `shader` is content
generated by a shader (rain, snow, particles), drawn in the entity's plane and z order; `None`
draws nothing. See [Filters and shaders](#filters-and-shaders). Kind: `const fn` · Builtin: yes ·
Profile: `native_v0` rejects shader assets at build; the native renderer draws neither filters
nor shader content.

### `entity.fill()` {#entity-fill}

```kis
#[builtin] pub const fn fill(&mut self) -> &mut Prop<Color>;
```

A colour drawn below the parts, over the fill size. With the default fill size (`None`, the whole
screen), an entity with only a fill is a full-screen colour plate. Kind: `const fn` · Builtin: yes.

### `entity.part(part)`, `entity.image()` {#entity-image}

```kis
#[builtin] pub const fn part(&mut self, part: Part) -> &mut ImageLayer;
pub const fn image(&mut self) -> &mut ImageLayer {
    self.part(MAIN)
}
```

The image layer `part`; `image()` is the `MAIN` layer. Parts draw in ascending `Part` order.
Kind: `const fn` · Builtin: `part` yes, `image` no.

> **Stability:** Planned — auto-blink and lip sync on layered character sprites (parts that
> animate on their own, and lip sync driven by the voice) are not designed yet.

### `entity.label()` {#entity-label}

```kis
#[builtin] pub const fn label(&mut self) -> &mut LabelLayer;
```

The [text layer](#labellayer), drawn above the parts and the movie. Kind: `const fn` · Builtin: yes.

### `entity.movie()` {#entity-movie}

```kis
#[builtin] pub const fn movie(&mut self) -> &mut MovieLayer;
```

The [movie layer](#movielayer), drawn above the parts. Kind: `const fn` · Builtin: yes · Profile:
`native_v0` rejects video assets at build, so only `clear()` builds; the native renderer draws no
movies.

<a id="entity-set_flip"></a><a id="entity-set_fill_size"></a>

### Layout setters {#entity-setters}

```kis
#[builtin] pub const fn set_plane(&mut self, plane: Plane);
#[builtin] pub const fn set_z(&mut self, z: i32);
#[builtin] pub const fn set_anchor(&mut self, anchor: Anchor);
#[builtin] pub const fn set_flip(&mut self, flip: Flip);
#[builtin] pub const fn set_blend(&mut self, blend: BlendMode);
#[builtin] pub const fn set_fill_size(&mut self, size: Option<Size>);
```

| Setter | Effect |
| ------ | ------ |
| `set_plane(p)` | moves the entity to plane `p` |
| `set_z(z)` | sets the order inside the plane; higher draws later |
| `set_anchor(a)` | sets the point of the entity that sits at its position |
| `set_flip(f)` | mirrors the drawing |
| `set_blend(b)` | sets how the entity composites over what is below it |
| `set_fill_size(s)` | sets the fill's size; `None` covers the whole screen |

Setters take effect at once; they are not animated. Kind: `const fn` · Builtin: yes.

### `entity.gate(setting)` {#gate}

```kis
#[builtin] pub const fn gate(&mut self, setting: Option<Setting<bool>>);
```

`gate(Some(s))` draws the entity only while setting `s` is true; `gate(None)` always draws it.
Kind: `const fn` · Builtin: yes.

- The handle is part of the logical screen and compares by identity, not by value. A render cannot
  read the setting, so the logical screen does not depend on it.
- The engine reads the value at draw time (a deferred reference).

```kis
static SHOW_CLOCK: Setting<bool> = setting(true);
const CLOCK: EntityId = EntityId(40);
const CLOCK_FACE: ImageAsset = asset("clock.png");

const fn clock_overlay(mut screen: Screen) -> Screen {
    screen.entity(CLOCK).set_plane(Plane::OverWindow);
    screen.entity(CLOCK).gate(Some(SHOW_CLOCK));
    screen.entity(CLOCK).image().change(CLOCK_FACE).apply_animation(cut());
    screen.entity(CLOCK).opacity().change(1.0).apply_animation(tween(ms(200)));
    screen
}
```

### `entity.apply_animation(motion)` {#entity-apply_animation}

```kis
#[builtin] pub const fn apply_animation(&mut self, motion: anim::Motion);
```

Plays a relative [motion](./screen-anim.md#motion) that ends where it started. The entity's
logical state does not change. Kind: `const fn` · Builtin: yes.

### Drawing order {#drawing-order}

- Planes are drawn in the order `Background < Stage < Front < (text window, drawn by the game
  view) < OverWindow`. Inside a plane, `z` ascends, then `EntityId` ascends: `z` is the analog of
  CSS `z-index`. Shader-generated content (weather, particles) is ordered the same way.
- Inside an entity, the order is fill, parts by ascending `Part`, movie, label.
- The anchor point sits at `position`. The entity size is the size of the `MAIN` part's image,
  else the fill size, else the label box. Scale and rotation pivot on the anchor.
- The camera applies to `Background`, `Stage` and `Front` only: offset translates, zoom and
  rotation pivot on the screen centre.
- The screen filter applies to the same three planes.

## Struct: `Prop` {#prop}

```kis
#[builtin] pub struct Prop<T>;
impl<T> Prop<T> {
    #[builtin] pub const fn get(&self) -> T;
    #[builtin] pub const fn change(&mut self, value: T) -> &mut Change<T>;
}
```

An interpolated property. `get()` returns the logical value. `change(v)` sets the logical value at
once and returns a pending [`Change`](#change); the change happens once it is given a tween.
Kind: `const fn` · Builtin: yes.

`prop.change(v).apply_animation(t)`:

- The logical value becomes `v` at once, so a later `get` in the same render sees `v`.
- The engine interpolates from the value the property has at `cursor + t.delay` to `v`, over
  `t.duration` with `t.easing`.
- A later animation of the same property that starts while an earlier one is still running cuts
  the earlier one to its end.
- `Prop<Visibility>` interpolates as a fade.

```kis
screen.entity(HANAKO).position().change(point(400, 80)).apply_animation(tween(ms(1000)).ease(Easing::InOutQuad));
```

## Struct: `Change` {#change}

```kis
#[must_use] #[builtin] pub struct Change<T>;
impl<T> Change<T> {
    #[builtin] pub const fn apply_animation(&mut self, tween: anim::Tween);
}
```

A pending property change; it happens only through `apply_animation`, which interpolates from the
value at the cursor plus the delay to the new value. `Change<T>` is `#[must_use]`: a change with no
`apply_animation` does not happen, and the statement is a warning. A change that should be
immediate says so with [`instant()`](./screen-anim.md#tween). Kind: `const fn` · Builtin: yes.

## Layers and `Swap` {#layers-and-swap}

A layer swap behaves like a property change, with a [`Transition`](./screen-anim.md#transition)
instead of a `Tween`: the logical content changes at once, and the transition runs from the old
content to the new one at `cursor + delay`. `clear()` swaps to nothing.

### Struct: `ImageLayer` {#imagelayer}

```kis
#[builtin] pub struct ImageLayer;
impl ImageLayer {
    #[builtin] pub const fn get(&self) -> Option<ImageAsset>;
    #[builtin] pub const fn change(&mut self, image: ImageAsset) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
}
```

An image layer of an entity. `get()` is the image shown, if any. `change(image)` swaps the image
for `image`; `clear()` swaps it for nothing. Kind: `const fn` · Builtin: yes.

### Struct: `LabelLayer` {#labellayer}

```kis
#[builtin] pub struct LabelLayer;
impl LabelLayer {
    #[builtin] pub const fn change(&mut self, text: Text) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
    #[builtin] pub const fn set_style(&mut self, style: TextStyle);
    #[builtin] pub const fn set_width(&mut self, width: Option<u32>);
}
```

The text layer of an entity. It shows a `Text` with its own
[`TextStyle`](./narrative-text.md). `change(text)` and `clear()` swap the text; `set_style` sets
the style; `set_width(Some(w))` wraps at `w` pixels, `None` does not wrap. Kind: `const fn` ·
Builtin: yes.

### Struct: `Movie` {#movie}

```kis
pub struct Movie {
    pub video: VideoAsset,
    pub looping: bool,
}
pub const fn movie(video: VideoAsset) -> Movie
impl Movie {
    pub const fn looping(mut self) -> Movie
}
```

A movie and whether it loops. `movie(v)` plays `v` once; `movie(v).looping()` loops it.
Kind: `const fn` · Builtin: no · Profile: `native_v0` rejects video assets at build, so movies
and `std::system::watched` cannot be used; the native renderer draws no movies and never fills
the seen-movie set.

### Struct: `MovieLayer` {#movielayer}

```kis
#[builtin] pub struct MovieLayer;
impl MovieLayer {
    #[builtin] pub const fn change(&mut self, movie: Movie) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
}
```

The movie layer of an entity. `change(movie)` swaps the movie for `movie`; `clear()` swaps it for
nothing. Kind: `const fn` · Builtin: yes.

### Struct: `Swap` {#swap}

```kis
#[must_use] #[builtin] pub struct Swap;
impl Swap {
    #[builtin] pub const fn apply_animation(&mut self, transition: anim::Transition);
}
```

A pending layer swap; it happens only through `apply_animation`, which swaps from the old content
to the new one with `transition`. `Swap` is `#[must_use]`. Kind: `const fn` · Builtin: yes.

```kis
screen.background().change(BG::ROOF).apply_animation(fade(ms(500)));
```

## Struct: `Hud` {#hud}

```kis
#[builtin] pub struct Hud;
impl Hud {
    #[builtin] pub const fn text_window(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn quick_menu(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn side_image(&mut self) -> &mut ImageLayer;
}
```

What the story asks of the game view's chrome: whether the text window and the quick menu are
shown, and the side image next to the text window. The Hud is part of the logical screen. The game
view reads it through `Narrative::hud()`; the story never reads it. Kind: `const fn` · Builtin: yes.

```kix
s!(render = |mut screen| {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen
});
```

## Struct: `ChoiceMenu` {#choicemenu}

<a id="choicemenu-set_align"></a>

```kis
#[builtin] pub struct ChoiceMenu;
impl ChoiceMenu {
    #[builtin] pub const fn disabled_arm(&mut self, at: u32, text: Text);
    #[builtin] pub const fn set_align(&mut self, align: VAlign);
}
```

The presentation of the choice a step shows. Kind: `const fn` · Builtin: yes.

- The choice menu is step-only: it is emptied and set to `Middle` at every step entry, and is not
  part of the logical screen.
- `disabled_arm(at, text)` adds a display-only arm, shown disabled at position `at` of the shown
  list (clamped). It has no value, cannot be selected and is not part of `choice!`, so no analysis
  sees it. Story code picks the render that adds it; see
  [`std::narrative`](./narrative.md#disabled-arms).
- `set_align(align)` places the menu vertically.

## Struct: `Camera` {#camera}

```kis
#[builtin] pub struct Camera;
impl Camera {
    #[builtin] pub const fn offset(&mut self) -> &mut Prop<Offset>;
    #[builtin] pub const fn zoom(&mut self) -> &mut Prop<Decimal>;
    #[builtin] pub const fn rotation(&mut self) -> &mut Prop<Angle>;
    #[builtin] pub const fn apply_animation(&mut self, motion: anim::Motion);
}
```

The camera over the `Background`, `Stage` and `Front` planes, so the text window and `OverWindow`
stay still. `offset` translates; `zoom` and `rotation` pivot on the screen centre.
`apply_animation` plays a relative motion that ends where it started; it is the usual way to shake
the whole scene. Kind: `const fn` · Builtin: yes.

```kis
screen.camera().apply_animation(quake(8, 4, ms(600)));
```
