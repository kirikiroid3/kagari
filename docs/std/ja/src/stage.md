# `std::stage` {#stdstage}

> **安定性:** 安定 — 舞台上のキャラクターです。画面に対する `const fn` ヘルパーとして書かれています。

`std::stage` は、スタジオがふだん手作業で組み立てるキャラクター層です。コンポーネントは追加しません。どのヘルパーも [`std::screen`](./screen.md) に対するふつうの `const fn` コードで、スタジオが独自のヘルパーと定数データでライブラリを拡張する方法の手本になっています。スロットはスタジオの `Point` 定数、表情は `ImageAsset` 定数です。

プレリュードは `Character` と `character` を再エクスポートします。`hide_all` と `side_face` には `use std::stage::{hide_all, side_face};` が必要です。完全な例は [演出レシピ](./guides/staging.md) にあります。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Character`](#character)、`character` | 構造体 + コンストラクター | キャラクターの配置方法 |
| [`hide_all`](#hide_all) | 関数 | 舞台をフェードアウトして削除する |
| [`side_face`](#side_face) | 関数 | Hud のサイド画像を設定または消去する |

## 構造体：`Character` {#character}

```kis
pub struct Character {
    pub entity: EntityId,
    pub offset: Offset,
    pub anchor: Anchor,
    pub plane: Plane,
    pub z: i32,
}
```

キャラクターの配置方法です。描画するエンティティ、すべてのスロットに加えるオフセット（サイズの異なる立ち絵に対するキャラクターごとの補正）、レイアウトを持ちます。`Character` は定数データです。メソッドがスロット `at` を受け取るとき、目標点は `at + offset` です。

### `character(entity)` {#character-fn}

```kis
pub const fn character(entity: EntityId) -> Character
```

- 戻り値：`entity` が描画する、オフセットなし、`TOP_LEFT` アンカー、`Stage` プレーン、z 0 のキャラクター。
- 種別：`const fn` · ビルトイン：いいえ。

### ビルダー：`offset`、`anchor`、`plane`、`z` {#character-builders}

```kis
pub const fn offset(mut self, offset: Offset) -> Character
pub const fn anchor(mut self, anchor: Anchor) -> Character
pub const fn plane(mut self, plane: Plane) -> Character
pub const fn z(mut self, z: i32) -> Character
```

| ビルダー | 戻り値 |
| ------- | ------- |
| `offset(o)` | すべてのスロットで `o` だけずらしたこのキャラクター |
| `anchor(a)` | アンカーを `a` にしたこのキャラクター |
| `plane(p)` | プレーン `p` 上のこのキャラクター |
| `z(z)` | プレーン内の順序を `z` にしたこのキャラクター |

レイアウトは `show` がエンティティを生成するときに適用されます。種別：`const fn` · ビルトイン：いいえ。

### `character.show(screen, face, at, tween)` {#character-show}

```kis
pub const fn show(self, screen: &mut Screen, face: ImageAsset, at: Point, tween: Tween)
```

キャラクターを表情 `face` で `at` にフェードインさせて表示します。すでに画面上にいれば、移動してクロスフェードします。

- `screen.contains(entity)` のとき：画像を `fade(tween.duration)` で `face` に変え、位置を `tween` で目標点へ動かします。
- そうでないとき：キャラクターの値で `set_plane`、`set_z`、`set_anchor` を行い、位置を `instant()` で目標点に、画像を `cut()` で `face` にします。
- どちらの場合も、不透明度を `tween` で 1.0 にします。
- 種別：`const fn` · ビルトイン：いいえ。

### `character.face(screen, face, transition)` {#character-face}

```kis
pub const fn face(self, screen: &mut Screen, face: ImageAsset, transition: Transition)
```

表情（`MAIN` 画像）を `transition` で変えます。種別：`const fn` · ビルトイン：いいえ。

### `character.move_to(screen, at, tween)` {#character-move_to}

```kis
pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween)
```

キャラクターを `tween` で `at + offset` へ動かします。種別：`const fn` · ビルトイン：いいえ。本体は画面に対するふつうの `const fn` コードです。

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

キャラクターをフェードアウト（`tween` で不透明度 0.0）させて削除します。フェードは描画され続けますが、次のステップはそのエンティティを引き継ぎません。種別：`const fn` · ビルトイン：いいえ。

### `character.act(screen, motion)` {#character-act}

```kis
pub const fn act(self, screen: &mut Screen, motion: Motion)
```

キャラクターのエンティティで `motion` を再生します。[`Motion`](./screen-anim.md#motion) を参照してください。種別：`const fn` · ビルトイン：いいえ。

## 関数：`hide_all(screen, tween)` {#hide_all}

```kis
pub const fn hide_all(screen: &mut Screen, tween: Tween)
```

`Stage` プレーン上のすべてのエンティティを、`screen.entities(Plane::Stage)` の順にフェードアウト（`tween` で不透明度 0.0）させて削除します。種別：`const fn` · ビルトイン：いいえ。

## 関数：`side_face(screen, face, transition)` {#side_face}

```kis
pub const fn side_face(screen: &mut Screen, face: Option<ImageAsset>, transition: Transition)
```

`face` を Hud のサイド画像として表示する（`Some(f)`：`hud().side_image().change(f)`）か、消去します（`None`：`.clear()`）。どちらも `transition` を使います。ゲームビューはサイド画像をテキストウィンドウの横に描画します。サイド画像は論理画面の一部なので、後のレンダーが変えるまで残ります。種別：`const fn` · ビルトイン：いいえ。

```kis
side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
side_face(&mut screen, None, fade(ms(300)));
```

## キャラクター、表情、スロット {#characters-faces-and-slots}

スロットはスタジオの `Point` 定数です。表情はキャラクター名のモジュール内の `ImageAsset` 定数にします。そうすると表情名の綴り間違いは名前解決エラーに、ファイルの欠落はアセットエラーになります。

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

> **安定性:** 予定 — 自動まばたきとボイス連動の口パクを備えたレイヤー構成の立ち絵はまだ設計されていません。現在、表情は `MAIN` パーツ上の 1 つの `ImageAsset` です。追加のパーツは [`Entity::part`](./screen.md#entity-image) で手作業で重ねられます。
