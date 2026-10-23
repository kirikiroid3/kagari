# Stage

`std::stage` is the character layer studios otherwise build by hand, written as ordinary
`const fn` helpers over the screen. It adds no components: it is the reference of how a studio
extends the library with helpers and constant data (design rule 6).

```kis
use crate::screen::{Anchor, EntityId, Offset, Plane, Point, Screen, TOP_LEFT, offset, point};
use crate::screen::anim::{Motion, Transition, Tween, cut, fade, instant};
```

## `Character`

```kis
pub struct Character { pub entity: EntityId, pub offset: Offset, pub anchor: Anchor, pub plane: Plane, pub z: i32 }
pub const fn character(entity: EntityId) -> Character {
    Character { entity, offset: offset(0, 0), anchor: TOP_LEFT, plane: Plane::Stage, z: 0 }
}
impl Character {
    pub const fn offset(mut self, offset: Offset) -> Character { … }
    pub const fn anchor(mut self, anchor: Anchor) -> Character { … }
    pub const fn plane(mut self, plane: Plane) -> Character { … }
    pub const fn z(mut self, z: i32) -> Character { … }
    pub const fn show(self, screen: &mut Screen, face: ImageAsset, at: Point, tween: Tween) { … }
    pub const fn face(self, screen: &mut Screen, face: ImageAsset, transition: Transition) { … }
    pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween) { … }
    pub const fn hide(self, screen: &mut Screen, tween: Tween) { … }
    pub const fn act(self, screen: &mut Screen, motion: Motion) { … }
}
pub const fn hide_all(screen: &mut Screen, tween: Tween) { … }
pub const fn side_face(screen: &mut Screen, face: Option<ImageAsset>, transition: Transition) { … }
```

A `Character` is constant data: the entity that draws the character, an offset added at every
slot (a per-character correction for sprites of different sizes), and its layout.

The target point is always `at + offset`.

|Method|Behaviour|
|---|---|
|`show`|If `screen.contains(entity)`: change the image to `face` with `fade(tween.duration)`, move the position to the target with `tween`, and change the opacity to 1.0 with `tween`. Else: `set_plane`, `set_z`, `set_anchor`; position to the target with `instant()`; image to `face` with `cut()`; opacity 1.0 with `tween`.|
|`face`|image change with `transition`|
|`move_to`|position to the target with `tween`|
|`hide`|opacity 0.0 with `tween`, then `despawn`|
|`act`|`apply_animation(motion)` on the entity|
|`hide_all`|`hide` for every id in `screen.entities(Plane::Stage)`|
|`side_face`|`Some(f)`: `hud().side_image().change(f)`; `None`: `.clear()`; either way with `transition`|

Each body is plain `const fn` code on the screen:

```kis
pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween) {
    let target = point(at.x + self.offset.dx, at.y + self.offset.dy);
    screen.entity(self.entity).position().change(target).apply_animation(tween);
}
```

## Characters, faces and slots

Slots are studio `Point` constants. Faces are `ImageAsset` constants in a module named after the
character, so a misspelled face is a resolve error and a missing file is an asset error.

```kix
use std::narrative::Mark;
use std::narrative::text::{speed, text_style};
use std::screen::{BOTTOM_CENTER, BlendMode, NO_FILTER};
use std::stage::{hide_all, side_face};

const HANAKO: Character = character(EntityId(1)).offset(offset(0, 40)).anchor(BOTTOM_CENTER);

mod HANAKO {
    pub const NORMAL: ImageAsset = asset("hanako_normal.png");
    pub const SMILE: ImageAsset = asset("hanako_smile.png");
    pub const ANGRY: ImageAsset = asset("hanako_angry.png");
    pub const SURPRISED: ImageAsset = asset("hanako_surprised.png");
}

const LEFT: Point = point(320, 720);
const CENTER: Point = point(640, 720);
const RIGHT: Point = point(960, 720);
```

Renders then read as stage directions:

```kix
const fn opening(mut screen: Screen) -> Screen {
    screen.background().change(BG::CLASSROOM).apply_animation(rule(BLINDS_RULE, ms(1000)));
    screen.music().change(looped(THEME).region(secs(16), Some(secs(80)))).apply_animation(crossfade(ms(1000)));
    screen.barrier();
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

## Recipes

Each recipe below is a complete `const fn` using only std items. Studio entities sit above the
characters:

```kix
const CURTAIN: EntityId = EntityId(100);
const TINT: EntityId = EntityId(101);
const CARD: EntityId = EntityId(102);
const ITEM: EntityId = EntityId(103);
const STILL: EntityId = EntityId(104);

mod BG {
    pub const CLASSROOM: ImageAsset = asset("bg_classroom.png");
    pub const NIGHT: ImageAsset = asset("bg_night.png");
}

mod CG {
    pub const FIREWORKS: ImageAsset = asset("cg_fireworks.png");
}

const BLINDS_RULE: ImageAsset = asset("rule_blinds.png");
const KEY_ITEM: ImageAsset = asset("item_key.png");
const THEME: AudioAsset = asset("bgm_theme.ogg");
const KNOCK: AudioAsset = asset("se_knock.ogg");
```

### Dip to black

A `Front` entity with only a fill covers the screen. It animates `TRANSPARENT → BLACK`, the
background switches while the screen is black, and the fill animates back. The curtain is
despawned at once, so it is not part of the logical screen; it is still drawn until its fade ends.

```kix
/// Fades the screen to black, swaps the background while it is hidden, and fades back.
const fn dip_to_black(screen: &mut Screen, next: ImageAsset) {
    screen.entity(CURTAIN).set_plane(Plane::Front);
    screen.entity(CURTAIN).opacity().change(1.0).apply_animation(instant());
    screen.entity(CURTAIN).fill().change(BLACK).apply_animation(tween(ms(1000)));
    screen.barrier();
    screen.background().change(next).apply_animation(cut());
    screen.entity(CURTAIN).fill().change(TRANSPARENT).apply_animation(tween(ms(1000)));
    screen.despawn(CURTAIN);
}

const fn night_falls(mut screen: Screen) -> Screen {
    hide_all(&mut screen, tween(ms(500)));
    side_face(&mut screen, None, fade(ms(300)));
    dip_to_black(&mut screen, BG::NIGHT);
    screen
}
```

For a plain background change through a colour, `through(BLACK, d, hold)` on the background swap
does the same without an entity.

### Flashback

A colour-blend fill over the stage plus a sepia screen filter:

```kix
/// Tints the stage with a colour blend and sepia, for a memory.
const fn flashback(mut screen: Screen) -> Screen {
    screen.entity(TINT).set_plane(Plane::Front);
    screen.entity(TINT).set_blend(BlendMode::Color);
    screen.entity(TINT).fill().change(hex(0x382000)).apply_animation(instant());
    screen.entity(TINT).opacity().change(0.6).apply_animation(tween(ms(800)));
    screen.filter().change(NO_FILTER.sepia(0.5)).apply_animation(tween(ms(800)));
    screen
}

/// Ends the flashback.
const fn flashback_end(mut screen: Screen) -> Screen {
    screen.entity(TINT).opacity().change(0.0).apply_animation(tween(ms(800)));
    screen.despawn(TINT);
    screen.filter().change(NO_FILTER).apply_animation(tween(ms(800)));
    screen
}
```

### Still (CG)

The text window hides first; after the `barrier`, the CG comes in through a rule. The next step
brings the window back:

```kix
/// Hides the text window, then reveals a CG through a rule.
const fn still(mut screen: Screen) -> Screen {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(tween(ms(300)));
    screen.barrier();
    screen.entity(STILL).set_plane(Plane::Front);
    screen.entity(STILL).opacity().change(1.0).apply_animation(instant());
    screen.entity(STILL).image().change(CG::FIREWORKS).apply_animation(rule(BLINDS_RULE, ms(1000)));
    screen
}

/// Brings the text window back over the CG.
const fn still_text(mut screen: Screen) -> Screen {
    screen.hud().text_window().change(Visibility::Shown).apply_animation(tween(ms(300)));
    screen
}
```

```kix
s!(render = still);
s!(render = still_text, t!["The fireworks lit the sky."]);
```

The CG counts as seen for the gallery once it is on a step's logical screen (see
[The Game System](./system.md#seen-sets)).

### Chapter card

A label entity over the text window fades in, waits, fades out and is despawned. The despawn is
immediate on the logical screen, so the next step does not inherit the card, while the fade-out
is still drawn:

```kix
/// Shows a chapter title over the text window, then removes it once it has faded out.
const fn chapter_card(mut screen: Screen) -> Screen {
    screen.entity(CARD).set_plane(Plane::OverWindow);
    screen.entity(CARD).set_anchor(std::screen::CENTER);
    screen.entity(CARD).position().change(point(640, 360)).apply_animation(instant());
    screen.entity(CARD).label().set_style(text_style(48, WHITE));
    screen.entity(CARD).label().change(t!["Chapter 1"]).apply_animation(cut());
    screen.entity(CARD).opacity().change(1.0).apply_animation(tween(ms(500)));
    screen.wait(ms(2500));
    screen.entity(CARD).opacity().change(0.0).apply_animation(tween(ms(500)));
    screen.despawn(CARD);
    screen
}
```

`std::screen::CENTER` is written by path because the studio's slot constant `CENTER` shadows the
std anchor in this module.

### Item popup

```kix
/// Pops an item up in the middle of the screen.
const fn item_popup(mut screen: Screen) -> Screen {
    screen.entity(ITEM).set_plane(Plane::Front);
    screen.entity(ITEM).set_anchor(std::screen::CENTER);
    screen.entity(ITEM).position().change(point(640, 300)).apply_animation(instant());
    screen.entity(ITEM).image().change(KEY_ITEM).apply_animation(cut());
    screen.entity(ITEM).scale().change(0.8).apply_animation(instant());
    screen.entity(ITEM).scale().change(1.0).apply_animation(tween(ms(300)).ease(Easing::OutBack));
    screen.entity(ITEM).opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}

/// Removes the item popup.
const fn item_popup_end(mut screen: Screen) -> Screen {
    screen.entity(ITEM).opacity().change(0.0).apply_animation(tween(ms(300)));
    screen.despawn(ITEM);
    screen
}
```

The two scale changes start at the same cursor: the first sets 0.8 instantly, and the second
interpolates from 0.8 to 1.0.

### Mid-line cue

The text names a point with `mark(1)`; the render anchors the cursor there:

```kix
/// Changes Hanako's face when the text reaches mark 1.
const fn surprised_at_mark(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(1));
    HANAKO.face(&mut screen, HANAKO::SURPRISED, fade(ms(300)));
    screen
}
```

```kix
s!(who = "Hanako", render = surprised_at_mark, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
```

The reveal stops for 500 ms after "Wait", and the face changes as "—what?" begins. A click shows
the whole line and completes the face change at once.

### Sound bursts and motion

```kix
const fn knocking(mut screen: Screen) -> Screen {
    screen.play(sound(KNOCK));
    screen.play(sound(KNOCK).delay(ms(250)));
    screen.play(sound(KNOCK).volume(0.6).delay(ms(500)));
    HANAKO.face(&mut screen, HANAKO::ANGRY, fade(ms(300)));
    HANAKO.act(&mut screen, shake(10, ms(600), 3));
    screen
}

const fn earthquake(mut screen: Screen) -> Screen {
    screen.camera().apply_animation(quake(8, 4, ms(500)));
    HANAKO.move_to(&mut screen, RIGHT, tween(ms(1000)));
    screen
}
```

### Disabled arm

```kix
const fn locked_arm(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Locked"]);
    screen
}
```

## Side face

`side_face` sets the Hud's side image, which the game view draws next to the text window. It is
part of the logical screen, so it stays until a later render changes it:

```kix
side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
side_face(&mut screen, None, fade(ms(300)));
```
