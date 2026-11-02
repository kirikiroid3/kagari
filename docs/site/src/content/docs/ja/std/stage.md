---
title: "std::stage"
---

> **安定性:** 安定 — 舞台上のキャラクター。画面を操作する `const fn` の補助関数として書かれています。

`std::stage` は、本来なら制作者が自分で組み立てることになるキャラクター周りの仕組みです。コンポーネントは追加しません。補助関数はどれも [`std::screen`](./screen.md) を操作する普通の `const fn` のコードで、制作者が独自の補助関数や定数データでライブラリを拡張するときの手本にもなります。立ち位置（スロット）は制作者が定義する `Point` 定数、表情は `ImageAsset` 定数です。

`Character` と `character` はプレリュードから再エクスポートされています。`hide_all` と `side_face` を使うには `use std::stage::{hide_all, side_face};` が必要です。まとまった例は [演出の実例](./guides/staging.md) にあります。

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| ---- | ---- | ------- |
| [`Character`](#character)、`character` | 構造体とコンストラクター | キャラクターの配置の仕方 |
| [`hide_all`](#hide_all) | 関数 | 舞台上をフェードアウトさせて消去 |
| [`side_face`](#side_face) | 関数 | Hud のサイド画像の設定・消去 |

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

キャラクターをどう配置するかを表します。描画に使うエンティティ、どのスロットにも加えるオフセット（大きさの違う立ち絵を揃えるための、キャラクターごとの補正）、レイアウトを含みます。`Character` は定数データです。メソッドにスロット `at` を渡すと、目標点は `at + offset` になります。

### `character(entity)` {#character-fn}

```kis
pub const fn character(entity: EntityId) -> Character
```

- 戻り値：`entity` で描画し、オフセットなし、アンカーは `TOP_LEFT`、描画面は `Stage`、z は 0 のキャラクター。
- 種別：`const fn` · 組み込み：いいえ。

### ビルダー：`offset`、`anchor`、`plane`、`z` {#character-builders}

```kis
pub const fn offset(mut self, offset: Offset) -> Character
pub const fn anchor(mut self, anchor: Anchor) -> Character
pub const fn plane(mut self, plane: Plane) -> Character
pub const fn z(mut self, z: i32) -> Character
```

| ビルダー | 戻り値 |
| ------- | ------- |
| `offset(o)` | どのスロットでも `o` だけずらしたキャラクター |
| `anchor(a)` | アンカーを `a` にしたキャラクター |
| `plane(p)` | 描画面 `p` に置いたキャラクター |
| `z(z)` | 描画面の中での重なり順を `z` にしたキャラクター |

レイアウトは `show` がエンティティを生成するときに反映されます。種別：`const fn` · 組み込み：いいえ。

### `character.show(screen, face, at, tween)` {#character-show}

```kis
pub const fn show(self, screen: &mut Screen, face: ImageAsset, at: Point, tween: Tween)
```

キャラクターを表情 `face` で `at` にフェードインさせます。すでに画面にいる場合は、移動しながら表情をクロスフェードします。

- `screen.contains(entity)` のとき：画像を `fade(tween.duration)` で `face` に切り替え、`tween` で目標点まで動かします。
- それ以外のとき：キャラクターの値で `set_plane`、`set_z`、`set_anchor` を呼び、`instant()` で目標点に置いて、画像を `cut()` で `face` にします。
- どちらの場合も、`tween` で不透明度を 1.0 にします。
- 種別：`const fn` · 組み込み：いいえ。

### `character.face(screen, face, transition)` {#character-face}

```kis
pub const fn face(self, screen: &mut Screen, face: ImageAsset, transition: Transition)
```

表情（`MAIN` 画像）を `transition` で切り替えます。種別：`const fn` · 組み込み：いいえ。

### `character.move_to(screen, at, tween)` {#character-move_to}

```kis
pub const fn move_to(self, screen: &mut Screen, at: Point, tween: Tween)
```

キャラクターを `tween` で `at + offset` まで動かします。種別：`const fn` · 組み込み：いいえ。中身は画面を操作する普通の `const fn` のコードです。

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

キャラクターをフェードアウト（`tween` で不透明度を 0.0 に）させて消去します。フェードアウトは最後まで描画されますが、次のステップにエンティティは引き継がれません。種別：`const fn` · 組み込み：いいえ。

### `character.act(screen, motion)` {#character-act}

```kis
pub const fn act(self, screen: &mut Screen, motion: Motion)
```

キャラクターのエンティティで `motion` を再生します（[`Motion`](./screen-anim.md#motion) を参照）。種別：`const fn` · 組み込み：いいえ。

## 関数：`hide_all(screen, tween)` {#hide_all}

```kis
pub const fn hide_all(screen: &mut Screen, tween: Tween)
```

描画面 `Stage` にあるエンティティをすべて、`screen.entities(Plane::Stage)` の順にフェードアウト（`tween` で不透明度を 0.0 に）させて消去します。種別：`const fn` · 組み込み：いいえ。

## 関数：`side_face(screen, face, transition)` {#side_face}

```kis
pub const fn side_face(screen: &mut Screen, face: Option<ImageAsset>, transition: Transition)
```

`face` を Hud のサイド画像として表示するか（`Some(f)`：`hud().side_image().change(f)`）、サイド画像を消します（`None`：`.clear()`）。どちらも `transition` で切り替わります。サイド画像はゲーム画面がテキストウィンドウの横に描きます。論理画面の一部なので、後のレンダーで変えるまで表示されたままです。種別：`const fn` · 組み込み：いいえ。

```kis
side_face(&mut screen, Some(HANAKO::NORMAL), fade(ms(300)));
side_face(&mut screen, None, fade(ms(300)));
```

## キャラクター、表情、スロット {#characters-faces-and-slots}

スロットは制作者が定義する `Point` 定数です。表情は、キャラクター名のモジュールの中に `ImageAsset` 定数として置きます。こうしておくと、表情名を打ち間違えれば名前解決のエラーに、ファイルがなければアセットのエラーになります。

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

> **安定性:** 予定 — 自動まばたきやボイスに合わせた口パクができる、パーツを重ねた立ち絵はまだ設計していません。今のところ表情は `MAIN` パーツに置く `ImageAsset` ひとつです。ほかのパーツは [`Entity::part`](./screen.md#entity-image) で自分で重ねられます。
