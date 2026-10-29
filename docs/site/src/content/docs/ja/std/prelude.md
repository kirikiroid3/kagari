---
title: "std::prelude"
---

> **安定性:** 安定 — すべてのモジュールにインポートされる名前。

ゲームのすべてのモジュールは、先頭に `use std::prelude::*;` があるかのように `std::prelude` を暗黙にインポートします。モジュール内で宣言した名前やインポートした名前はプレリュードの名前より優先されるため、プレリュードがゲーム自身の名前を曖昧にすることはありません。

ここにないものはパスを指定してインポートします。例：`use std::screen::anim::rule_with;`、`use std::narrative::text::speed;`。

## 宣言 {#declaration}

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

## スコープに入る名前 {#names-in-scope}

|元|名前|ページ|
|---|---|---|
|`std`|`String`、`Vec`、`Rat`、`Decimal`、`Range`、`Option`、`Result`、`Eq`、`Integer`、`RandChoose`、`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset`、`asset`、`system_font`|[std](./std.md)|
|`std::Option`|`None`、`Some`|[std](./std.md#enum-optiont)|
|`std::Result`|`Err`、`Ok`|[std](./std.md#enum-resultt-e)|
|`std::time`|`Duration`、`ms`、`secs`|[std::time](./time.md)|
|`std::screen`|`Screen`、`Render`、`EntityId`、`Part`、`Point`、`Offset`、`Size`、`Anchor`、`Color`、`Plane`、`Visibility`、`point`、`offset`、`rgb`、`hex`、`BLACK`、`WHITE`、`TRANSPARENT`|[std::screen](./screen.md)|
|`std::screen::anim`|`Easing`、`Tween`、`Transition`、`Motion`、`tween`、`instant`、`cut`、`fade`、`rule`、`wipe`、`through`、`motion`、`jump`、`nod`、`shake`、`quake`|[std::screen::anim](./screen-anim.md)|
|`std::screen::audio`|`looped`、`sound`、`crossfade`|[std::screen::audio](./screen-audio.md)|
|`std::stage`|`Character`、`character`|[std::stage](./stage.md)|
|`std::narrative`|`Text`、`TextEffect`、`Say`、`Choice`、`ChoiceArm`、`push_step`、`rng_choose`、`Meta`、`meta`、`Narrative`|[std::narrative](./narrative.md)|
|`std::narrative::text`|`emphasis`、`dots`、`color_text`、`bold`、`pause`、`click_wait`、`mark`|[std::narrative::text](./narrative-text.md)|
|`std::system`|`nav`（モジュール）、`Setting`、`setting`、`System`、`Settings`、`Volume`、`volume`、`RootPage`、`Overlay`、`GameOverlay`、`Dialog`|[std::system](./system.md)|
|`std::ui`|`signal`、`Element`|[std::ui](./ui.md)|

`nav` はモジュールそのものなので、その関数は `use` なしで `nav::start(…)`、`nav::back()` のように呼べます。

## プレリュードにないもの {#not-in-the-prelude}

`use` が必要なよく使うアイテム：

|アイテム|`use`|
|---|---|
|`ZERO`|`use std::time::ZERO;`|
|`Step`、`Line`、`Menu`、`MenuArm`、`Arm`、`HudState`、`Entry`、`PlayMode`、`Mark`|`use std::narrative::{…};`|
|`ruby`、`mono_ruby`、`effect`、`concat`、`to_text`、`ToText`|`use std::narrative::text::{…};`（通常は `t![…]` を通して書きます）|
|`size`、`italic`、`font`、`outline`、`shadow`、`speed`、`speed_scale`、`reveal_instant`|`use std::narrative::text::{…};`|
|`TextStyle`、`text_style`、`Outline`、`Shadow`、`TextAlign`、`EmphasisStyle`|`use std::narrative::text::{…};`|
|`Asset`|`use std::Asset;`|
