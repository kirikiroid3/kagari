# Staging Recipes

> **Stability:** Stable — recipes built only from current `std` items.

## Composing renders {#composing-renders}

A render is a `const fn(Screen) -> Screen`. Larger effects are composed from two kinds of
function:

- **Helpers** take `&mut Screen` and return nothing, like the methods of
  [`Character`](../stage.md#character). They can be called from any render and from each other.
- **Renders** take the screen by value, call helpers and projections, and return it. A step uses
  one with `s!(render = name, …)`.

Inside a render, the [timeline cursor](../screen-anim.md#the-timeline) decides when each change
starts: statements between two cursor moves start together; `barrier()`, `wait(d)` and
`at_mark(m)` move the cursor. Every change states its animation, and a spawned entity starts at
opacity 0.0, so every appearance fades or tweens its opacity in.

An entity that should exist only during one effect (a curtain, a card) is despawned in the same
render that fades it out: it leaves the [logical screen](../screen.md#the-logical-screen) at once
and is still drawn until its fade ends.

## Shared declarations {#shared-declarations}

Every recipe below assumes these declarations. Studio entities sit above the characters:

```kix
use std::narrative::Mark;
use std::narrative::text::text_style;
use std::screen::{BOTTOM_CENTER, BlendMode};
use std::stage::{hide_all, side_face};

const HANAKO: Character = character(EntityId(1)).offset(offset(0, 40)).anchor(BOTTOM_CENTER);

mod HANAKO {
    pub const NORMAL: ImageAsset = asset("hanako_normal.png");
    pub const ANGRY: ImageAsset = asset("hanako_angry.png");
    pub const SURPRISED: ImageAsset = asset("hanako_surprised.png");
}

const CENTER: Point = point(640, 720);
const RIGHT: Point = point(960, 720);

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
const MEMORY: ShaderAsset = asset("memory.wgsl");
const THEME: AudioAsset = asset("bgm_theme.ogg");
const KNOCK: AudioAsset = asset("se_knock.ogg");
```

The slot constant `CENTER` shadows the std anchor `std::screen::CENTER`, so recipes that need the
anchor write it by path.

## Opening a scene {#opening-a-scene}

The background comes in through a rule while the music starts; after the `barrier`, Hanako
appears with her side face:

```kix
const fn opening(mut screen: Screen) -> Screen {
    screen.background().change(BG::CLASSROOM).apply_animation(rule(BLINDS_RULE, ms(1000)));
    screen.music().change(looped(THEME).region(secs(16), Some(secs(80)))).apply_animation(crossfade(ms(1000)));
    screen.barrier();
    HANAKO.show(&mut screen, HANAKO::NORMAL, CENTER, tween(ms(300)));
    side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
    screen
}
```

## Dip to black {#dip-to-black}

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

## Flashback {#flashback}

A colour-blend fill over the stage plus a shader screen filter. The tint stays on the logical
screen until the end render fades and despawns it.

```kix
/// Tints the stage with a colour blend and a shader filter, for a memory.
const fn flashback(mut screen: Screen) -> Screen {
    screen.entity(TINT).set_plane(Plane::Front);
    screen.entity(TINT).set_blend(BlendMode::Color);
    screen.entity(TINT).fill().change(hex(0x382000)).apply_animation(instant());
    screen.entity(TINT).opacity().change(0.6).apply_animation(tween(ms(800)));
    screen.filter().change(Some(MEMORY)).apply_animation(tween(ms(800)));
    screen
}

/// Ends the flashback.
const fn flashback_end(mut screen: Screen) -> Screen {
    screen.entity(TINT).opacity().change(0.0).apply_animation(tween(ms(800)));
    screen.despawn(TINT);
    screen.filter().change(None).apply_animation(tween(ms(800)));
    screen
}
```

`native_v0`, today the only runtime profile, rejects `.wgsl` shader assets at build, so the
`MEMORY` filter does not build yet (see
[Runtime Profiles](../appendix/profiles.md#using-an-unsupported-item)). Drop `MEMORY` and the
`filter()` lines; the tint alone works.

## Still (CG) {#still-cg}

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
[`std::system`](../system.md#seen-sets)).

## Chapter card {#chapter-card}

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

## Item popup {#item-popup}

The item spawns small and grows with an overshooting easing while it fades in:

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

## Mid-line cue {#mid-line-cue}

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

## Sound bursts and motion {#sound-bursts-and-motion}

One-shots with delays make a burst; a character motion and a camera quake add weight. Neither
motion changes the logical screen:

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

## Disabled arm {#disabled-arm}

A display-only arm in the choice the step shows. It cannot be selected and no analysis sees it;
story code decides when to use this render (see [`std::narrative`](../narrative.md#disabled-arms)):

```kix
const fn locked_arm(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Locked"]);
    screen
}
```
