---
title: "演出の実例"
---

> **安定性:** 安定 — 現在の `std` のアイテムだけで組んだ実例です。

## レンダーの組み立て方 {#composing-renders}

レンダーは `const fn(Screen) -> Screen` 型の関数です。大きな演出は、次の 2 種類の関数を組み合わせて作ります。

- **補助関数**：`&mut Screen` を受け取り、何も返しません。[`Character`](../stage.md#character) のメソッドがこの形です。どのレンダーからも、ほかの補助関数からも呼べます。
- **レンダー**：画面を値で受け取り、補助関数や射影を呼んでから画面を返します。ステップからは `s!(render = name, …)` で指定します。

レンダーの中で各変更がいつ始まるかは、[タイムラインのカーソル](../screen-anim.md#the-timeline)で決まります。カーソルを動かすのは `barrier()`、`wait(d)`、`at_mark(m)` で、その間に書いた文は同時に始まります。変更にはすべてアニメーションを指定します。また、生成したエンティティは不透明度 0.0 から始まるので、登場させるときは毎回フェードか補間で不透明度を上げます。

幕やカードのように一つの演出の間だけ使うエンティティは、フェードアウトさせるのと同じレンダーで消去します。消去したエンティティはすぐに[論理画面](../screen.md#the-logical-screen)から外れますが、フェードが終わるまでは描画されます。

## 共通の宣言 {#shared-declarations}

以下の実例は、どれも次の宣言があるものとします。制作者が用意するエンティティはキャラクターより手前に置きます。

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

立ち位置の定数 `CENTER` が std のアンカー `std::screen::CENTER` を隠してしまうため、アンカーを使う実例ではパス付きで書いています。

## シーンの開始 {#opening-a-scene}

音楽が流れ始めると同時に、背景がルール画像で切り替わります。`barrier` の後、花子がサイド画像付きで登場します。

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

塗りだけを持つ `Front` のエンティティで画面を覆います。塗りを `TRANSPARENT → BLACK` と変化させ、画面が真っ黒な間に背景を切り替えてから、塗りを元に戻します。幕はすぐに消去するので論理画面には残りませんが、フェードが終わるまでは描画されます。

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

単に色を挟んで背景を変えたいだけなら、背景の切り替えに `through(BLACK, d, hold)` を使えば、エンティティなしで同じ効果が得られます。

## 回想 {#flashback}

舞台の上に塗りを重ねて `color` 合成シェーダーで合成し、さらに型付きの引数をとるシェーダーで画面フィルターをかけます。色調は、終了用のレンダーでフェードさせて消去するまで論理画面に残ります。

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

`MEMORY` は「シェーダーの書き方」の [最初のシェーダー](./shaders.md#a-first-shader) で作る色シェーダーです。ソースを `assets/memory.wgsl` に置いてください。

## スチル（CG） {#still-cg}

まずテキストウィンドウを隠し、`barrier` の後で CG をルール画像で表示します。次のステップでウィンドウを戻します。

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

CG は、いずれかのステップの論理画面に載った時点でギャラリー上は既読になります（[`std::system`](../system.md#seen-sets) を参照）。

## 章タイトル {#chapter-card}

テキストウィンドウより手前にラベルのエンティティを置き、フェードインさせて少し待ち、フェードアウトさせてから消去します。論理画面からはすぐに消えるので次のステップにカードは引き継がれませんが、フェードアウトはきちんと描画されます。

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

## アイテムの表示 {#item-popup}

アイテムを小さめに生成し、フェードインさせながら、一度行き過ぎて戻るイージングで拡大します。

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

2 つの拡大率の変更は、同じカーソル位置から始まります。1 つ目で即座に 0.8 にし、2 つ目で 0.8 から 1.0 へ補間します。

## 台詞の途中での演出 {#mid-line-cue}

テキスト側で `mark(1)` によって位置に名前を付け、レンダー側でカーソルをその位置に合わせます。

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

"Wait" を表示したところで文字送りが 500 ms 止まり、"—what?" が出始めると同時に表情が変わります。クリックすると台詞全体が表示され、表情の変化もその場で完了します。

## 連続する効果音とモーション {#sound-bursts-and-motion}

遅延を付けた単発の効果音を重ねて連打音を作り、キャラクターのモーションとカメラの揺れで迫力を出します。どちらのモーションも論理画面は変えません。

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

## 選べないアーム {#disabled-arm}

ステップが表示する選択肢に、表示するだけのアームを加えます。このアームは選択できず、どの解析からも見えません。このレンダーをいつ使うかは物語のコードで決めます（[`std::narrative`](../narrative.md#disabled-arms) を参照）。

```kix
const fn locked_arm(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Locked"]);
    screen
}
```
