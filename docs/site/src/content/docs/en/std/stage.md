---
title: "std::stage"
---

> **Stability:** Stable — characters on the stage, written as `const fn` helpers over the screen.

`std::stage` is the character layer studios otherwise build by hand. It adds no components: every
helper is ordinary `const fn` code over [`std::screen`](./screen.md), and is the model for how a
studio extends the library with its own helpers and constant data. Slots are studio `Point`
constants; faces are `ImageAsset` constants.

The prelude re-exports `Character` and `character`. `hide_all` and `side_face` need a
`use std::stage::{hide_all, side_face};`. Complete examples are in
[Staging Recipes](./guides/staging.md).

## Overview

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`Character`](#character), `character` | struct + constructor | how a character is placed |
| [`hide_all`](#hide_all) | function | fade out and despawn the stage |
| [`side_face`](#side_face) | function | set or clear the Hud's side image |

## Struct: `Character` {#character}

```kis
pub struct Character {
    pub entity: EntityId,
    pub offset: Offset,
    pub anchor: Anchor,
    pub plane: Plane,
    pub z: i32,
}
```

How a character is placed: the entity that draws it, an offset added to every slot (a
per-character correction for sprites of different sizes), and its layout. A `Character` is
constant data. Wherever a method takes a slot `at`, the target point is `at + offset`.

### `character(entity)` {#character-fn}

```kis
pub const fn character(entity: EntityId) -> Character
```

- Returns: a character drawn by `entity`, without offset, anchored `TOP_LEFT` on the `Stage`
  plane at z 0.
- Kind: `const fn` · Builtin: no.

### Builders: `offset`, `anchor`, `plane`, `z` {#character-builders}

```kis
pub const fn offset(mut self, offset: Offset) -> Character
pub const fn anchor(mut self, anchor: Anchor) -> Character
pub const fn plane(mut self, plane: Plane) -> Character
pub const fn z(mut self, z: i32) -> Character
```

| Builder | Returns |
| ------- | ------- |
| `offset(o)` | this character shifted by `o` at every slot |
| `anchor(a)` | this character anchored at `a` |
| `plane(p)` | this character on plane `p` |
| `z(z)` | this character at order `z` in its plane |

The layout is applied when `show` spawns the entity. Kind: `const fn` · Builtin: no.

### `character.show(screen, face, at, tween)` {#character-show}

```kis
pub const fn show(self, screen: &mut Screen, face: ImageAsset, at: Point, tween: Tween)
```

Shows the character with `face` at `at`, fading in, or moves and crossfades it if it is already on
the screen.

- If `screen.contains(entity)`: the image changes to `face` with `fade(tween.duration)` and the
  position moves to the target with `tween`.
- Else: `set_plane`, `set_z` and `set_anchor` from the character; the position is set to the
  target with `instant()`; the image to `face` with `cut()`.
- Either way, the opacity changes to 1.0 with `tween`.
- Kind: `const fn` · Builtin: no.

### `character.face(screen, face, transition)` {#character-face}

```kis
pub const fn face(self, screen: &mut Screen, face: ImageAsset, transition: Transition)
```

Changes the face (the `MAIN` image) with `transition`. Kind: `const fn` · Builtin: no.

### `character.move_to(screen, at, tween)` {#character-move_to}

```kis
pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween)
```

Moves the character to `at + offset` with `tween`. Kind: `const fn` · Builtin: no. The body is
plain `const fn` code on the screen:

```kis
pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween) {
    let target = point(at.x + self.offset.dx, at.y + self.offset.dy);
    screen.entity(self.entity).position().change(target).apply_animation(tween);
}
```

### `character.hide(screen, tween)` {#character-hide}

```kis
pub const fn hide(self, screen: &mut Screen, tween: Tween)
```

Fades the character out (opacity 0.0 with `tween`) and despawns it. The fade is still drawn; the
next step does not inherit the entity. Kind: `const fn` · Builtin: no.

### `character.act(screen, motion)` {#character-act}

```kis
pub const fn act(self, screen: &mut Screen, motion: Motion)
```

Plays `motion` on the character's entity. See [`Motion`](./screen-anim.md#motion). Kind:
`const fn` · Builtin: no.

## Function: `hide_all(screen, tween)` {#hide_all}

```kis
pub const fn hide_all(screen: &mut Screen, tween: Tween)
```

Fades out (opacity 0.0 with `tween`) and despawns every entity on the `Stage` plane, in
`screen.entities(Plane::Stage)` order. Kind: `const fn` · Builtin: no.

## Function: `side_face(screen, face, transition)` {#side_face}

```kis
pub const fn side_face(screen: &mut Screen, face: Option<ImageAsset>, transition: Transition)
```

Shows `face` as the Hud's side image (`Some(f)`: `hud().side_image().change(f)`), or clears it
(`None`: `.clear()`), with `transition`. The game view draws the side image next to the text
window. It is part of the logical screen, so it stays until a later render changes it. Kind:
`const fn` · Builtin: no.

```kis
side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
side_face(&mut screen, None, fade(ms(300)));
```

## Characters, faces and slots {#characters-faces-and-slots}

Slots are studio `Point` constants. Faces are `ImageAsset` constants in a module named after the
character, so a misspelled face is a resolve error and a missing file is an asset error.

```kis
use std::screen::BOTTOM_CENTER;
use std::stage::side_face;

const HANAKO: Character = character(EntityId(1)).offset(offset(0, 40)).anchor(BOTTOM_CENTER);

mod HANAKO {
    pub const NORMAL: ImageAsset = asset("hanako_normal.png");
    pub const SMILE: ImageAsset = asset("hanako_smile.png");
}

const LEFT: Point = point(320, 720);
const CENTER: Point = point(640, 720);

const fn hanako_enters(mut screen: Screen) -> Screen {
    HANAKO.show(&mut screen, HANAKO::NORMAL, CENTER, tween(ms(300)));
    side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
    screen
}

const fn hanako_smiles(mut screen: Screen) -> Screen {
    HANAKO.face(&mut screen, HANAKO::SMILE, fade(ms(300)));
    HANAKO.act(&mut screen, jump(30, ms(400)));
    screen
}

const fn hanako_moves_left(mut screen: Screen) -> Screen {
    HANAKO.move_to(&mut screen, LEFT, tween(ms(1000)).ease(Easing::InOutSine));
    screen
}
```

> **Stability:** Planned — layered character sprites with auto-blink and voice-driven lip sync
> are not designed yet. Today a face is one `ImageAsset` on the `MAIN` part; further parts can be
> stacked by hand with [`Entity::part`](./screen.md#entity-image).
