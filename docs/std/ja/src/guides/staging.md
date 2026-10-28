# 演出レシピ {#staging-recipes}

> **安定性:** 安定 — 現在の `std` アイテムだけで組み立てたレシピです。

## レンダーの組み立て {#composing-renders}

レンダーは `const fn(Screen) -> Screen` です。大きな演出は 2 種類の関数を組み合わせて作ります。

- **ヘルパー**は `&mut Screen` を受け取って何も返しません。[`Character`](../stage.md#character) のメソッドがその例です。どのレンダーからも、ほかのヘルパーからも呼べます。
- **レンダー**は画面を値で受け取り、ヘルパーや射影を呼んで画面を返します。ステップは `s!(render = name, …)` で使います。

レンダーの中では、[タイムラインのカーソル](../screen-anim.md#the-timeline)が各変更の開始時刻を決めます。2 回のカーソル移動の間にある文は同時に始まり、`barrier()`、`wait(d)`、`at_mark(m)` がカーソルを動かします。すべての変更はアニメーションを明示し、生成されたエンティティは不透明度 0.0 から始まるので、登場のたびに不透明度をフェードやトゥイーンで上げます。

1 つの演出の間だけ存在すべきエンティティ（幕、カード）は、フェードアウトさせるのと同じレンダーで削除します。エンティティはすぐに[論理画面](../screen.md#the-logical-screen)から消え、フェードが終わるまでは描画され続けます。

## 共通の宣言 {#shared-declarations}

以下のレシピはすべてこれらの宣言を前提とします。スタジオのエンティティはキャラクターより上にあります。

```kix
use std::narrative::Mark;
use std::narrative::text::text_style;
use std::screen::BOTTOM_CENTER;
use std::screen::shader::{ColorShader, filter};
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
/// The arguments of the flashback filter.
#[shader_arg]
struct Memory {
    strength: Decimal,
}
const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
const THEME: AudioAsset = asset("bgm_theme.ogg");
const KNOCK: AudioAsset = asset("se_knock.ogg");
```

スロット定数 `CENTER` は std のアンカー `std::screen::CENTER` を隠すので、アンカーが必要なレシピではパスで書きます。

## シーンの開始 {#opening-a-scene}

音楽が始まると同時に背景がルールで現れ、`barrier` の後に花子がサイド画像とともに登場します。

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

## 暗転 {#dip-to-black}

塗りだけを持つ `Front` エンティティが画面を覆います。塗りが `TRANSPARENT → BLACK` と変化し、画面が黒い間に背景が切り替わり、塗りが元に戻ります。幕はすぐに削除されるので論理画面には含まれませんが、フェードが終わるまでは描画されます。

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

色を経由するだけの背景変更なら、背景の差し替えに `through(BLACK, d, hold)` を使えば、エンティティなしで同じことができます。

## 回想 {#flashback}

舞台の上に `color` ミックスシェーダーで合成する塗りを重ね、型付きの引数を持つシェーダーの画面フィルターを加えます。色調は、終了用のレンダーがフェードさせて削除するまで論理画面に残ります。

```kix
/// Tints the stage with a colour mix and a shader filter, for a memory.
const fn flashback(mut screen: Screen) -> Screen {
    screen.entity(TINT).set_plane(Plane::Front);
    screen.entity(TINT).mix().change(Some(std::shaders::mix::color())).apply_animation(instant());
    screen.entity(TINT).fill().change(hex(0x382000)).apply_animation(instant());
    screen.entity(TINT).opacity().change(0.6).apply_animation(tween(ms(800)));
    screen.filter().change(Some(filter(MEMORY, Memory { strength: 0.8 }))).apply_animation(tween(ms(800)));
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

現在唯一のランタイムプロファイルである `native_v0` はビルド時に `.wgsl` シェーダーアセットを拒否するため、`MEMORY` フィルターはまだビルドできません（[ランタイムプロファイル](../appendix/profiles.md#using-an-unsupported-item) を参照）。`MEMORY` と `filter()` の行を削除してください。色調だけでも機能します。

## スチル（CG） {#still-cg}

まずテキストウィンドウが隠れ、`barrier` の後に CG がルールで現れます。次のステップでウィンドウが戻ります。

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

CG はステップの論理画面に載った時点で、ギャラリー用に既読として扱われます（[`std::system`](../system.md#seen-sets) を参照）。

## 章タイトル {#chapter-card}

テキストウィンドウの上のラベルエンティティがフェードインし、待ってからフェードアウトし、削除されます。削除は論理画面上で即座に行われるので次のステップはカードを引き継ぎませんが、フェードアウトは描画されます。

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

## アイテム表示 {#item-popup}

アイテムは小さく生成され、フェードインしながら行き過ぎるイージングで大きくなります。

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

2 つの倍率変更は同じカーソル位置で始まります。1 つ目が即座に 0.8 を設定し、2 つ目が 0.8 から 1.0 へ補間します。

## 行の途中のキュー {#mid-line-cue}

テキストが `mark(1)` で位置に名前を付け、レンダーがそこにカーソルを固定します。

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

"Wait" の後で表示が 500 ms 止まり、"—what?" が始まると表情が変わります。クリックすると行全体が表示され、表情の変化もすぐに完了します。

## 連続音とモーション {#sound-bursts-and-motion}

遅延付きのワンショットで連続音を作り、キャラクターのモーションとカメラの揺れで重みを加えます。どちらのモーションも論理画面を変えません。

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

## 無効なアーム {#disabled-arm}

ステップが表示する選択肢に、表示専用のアームを加えます。選択できず、どの解析にも見えません。このレンダーをいつ使うかは物語のコードが決めます（[`std::narrative`](../narrative.md#disabled-arms) を参照）。

```kix
const fn locked_arm(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Locked"]);
    screen
}
```
