---
title: "std::prelude"
---

> **Stability:** Stable — the names imported into every module.

Every module of a game imports `std::prelude` implicitly, as if it started with
`use std::prelude::*;`. A name declared in the module, or imported by it, takes precedence over a
prelude name, so the prelude never makes a game's own name ambiguous.

Everything not listed here is imported by path, for example `use std::screen::anim::rule_with;`
or `use std::narrative::text::speed;`.

## Declaration

```kis
pub use crate::{String, Vec, Rat, Decimal, Range, Option, Result, Eq, Integer, RandChoose, ImageAsset, AudioAsset, VoiceAsset, VideoAsset, FontAsset, asset, system_font};
pub use crate::Option::{None, Some};
pub use crate::Result::{Err, Ok};
pub use crate::time::{Duration, ms, secs};
pub use crate::screen::{Screen, Render, EntityId, Part, Point, Offset, Size, Anchor, Color, Plane, Visibility, point, offset, rgb, hex, BLACK, WHITE, TRANSPARENT};
pub use crate::screen::anim::{Easing, Tween, Transition, Motion, tween, instant, cut, fade, rule, wipe, through, motion, jump, nod, shake, quake};
pub use crate::screen::audio::{looped, sound, crossfade};
pub use crate::stage::{Character, character};
pub use crate::narrative::{Text, TextEffect, Say, Choice, ChoiceArm, push_step, rng_choose, Meta, meta, Narrative};
pub use crate::narrative::text::{emphasis, dots, color_text, bold, pause, click_wait, mark};
pub use crate::system::{nav, Setting, setting, System, Settings, Volume, volume, RootPage, Overlay, GameOverlay, Dialog};
pub use crate::ui::{signal, Element};
```

## Names in scope

|From|Names|Page|
|---|---|---|
|`std`|`String`, `Vec`, `Rat`, `Decimal`, `Range`, `Option`, `Result`, `Eq`, `Integer`, `RandChoose`, `ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`, `asset`, `system_font`|[std](./std.md)|
|`std::Option`|`None`, `Some`|[std](./std.md#enum-optiont)|
|`std::Result`|`Err`, `Ok`|[std](./std.md#enum-resultt-e)|
|`std::time`|`Duration`, `ms`, `secs`|[std::time](./time.md)|
|`std::screen`|`Screen`, `Render`, `EntityId`, `Part`, `Point`, `Offset`, `Size`, `Anchor`, `Color`, `Plane`, `Visibility`, `point`, `offset`, `rgb`, `hex`, `BLACK`, `WHITE`, `TRANSPARENT`|[std::screen](./screen.md)|
|`std::screen::anim`|`Easing`, `Tween`, `Transition`, `Motion`, `tween`, `instant`, `cut`, `fade`, `rule`, `wipe`, `through`, `motion`, `jump`, `nod`, `shake`, `quake`|[std::screen::anim](./screen-anim.md)|
|`std::screen::audio`|`looped`, `sound`, `crossfade`|[std::screen::audio](./screen-audio.md)|
|`std::stage`|`Character`, `character`|[std::stage](./stage.md)|
|`std::narrative`|`Text`, `TextEffect`, `Say`, `Choice`, `ChoiceArm`, `push_step`, `rng_choose`, `Meta`, `meta`, `Narrative`|[std::narrative](./narrative.md)|
|`std::narrative::text`|`emphasis`, `dots`, `color_text`, `bold`, `pause`, `click_wait`, `mark`|[std::narrative::text](./narrative-text.md)|
|`std::system`|`nav` (the module), `Setting`, `setting`, `System`, `Settings`, `Volume`, `volume`, `RootPage`, `Overlay`, `GameOverlay`, `Dialog`|[std::system](./system.md)|
|`std::ui`|`signal`, `Element`|[std::ui](./ui.md)|

Because `nav` is the module itself, its functions are called as `nav::start(…)`, `nav::back()`
and so on without a `use`.

## Not in the prelude

Common items that need a `use`:

|Item|`use`|
|---|---|
|`ZERO`|`use std::time::ZERO;`|
|`Step`, `Line`, `Menu`, `MenuArm`, `Arm`, `HudState`, `Entry`, `PlayMode`, `Mark`|`use std::narrative::{…};`|
|`ruby`, `mono_ruby`, `effect`, `concat`, `to_text`, `ToText`|`use std::narrative::text::{…};` (usually written through `t![…]`)|
|`size`, `italic`, `font`, `outline`, `shadow`, `speed`, `speed_scale`, `reveal_instant`|`use std::narrative::text::{…};`|
|`TextStyle`, `text_style`, `Outline`, `Shadow`, `TextAlign`, `EmphasisStyle`|`use std::narrative::text::{…};`|
|`Asset`|`use std::Asset;`|
