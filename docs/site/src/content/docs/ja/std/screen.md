---
title: "std::screen"
---

> **安定性:** 安定 — レンダーで書き換える画面です。エンティティとそのコンポーネント、Hud、選択肢メニュー、カメラ、画面フィルター、オーディオチャンネルを扱います。

**レンダー**とは、ステップ前の画面を受け取ってステップ後の画面を返す `const fn` です。射影を通して画面をその場で書き換え、1 文に 1 つのチェーンを書きます。変更には必ずアニメーションを指定します。

```kis
const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

このモジュールのアイテムは、すべて `const fn` か単純なデータです。そのためレンダーはコンパイル時に評価でき、コンパイラは画面どうしを比較できます。このページでは `Screen` のタイムラインメソッドも扱います。タイムラインに予約するアニメーション値は [`std::screen::anim`](./screen-anim.md)、オーディオ値は [`std::screen::audio`](./screen-audio.md)、シェーダー効果（フィルター、合成、マスク、塗り、シェーダーモーション）は [`std::screen::shader`](./screen-shader.md) にあります。既製のシェーダーは [`std::shaders`](./shaders.md)、このモジュールを使ったキャラクター用の補助関数は [`std::stage`](./stage.md)、完全な例は [演出の実例](./guides/staging.md) を見てください。

プレリュードには `Screen`、`Render`、`EntityId`、`Part`、`Point`、`Offset`、`Size`、`Anchor`、`Color`、`Plane`、`Visibility`、`point`、`offset`、`rgb`、`hex`、`BLACK`、`WHITE`、`TRANSPARENT` が含まれます。それ以外のアイテムは `use std::screen::…` で取り込んでください。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Render`](#render) | 型 | `const fn(Screen) -> Screen` |
| [`EntityId`](#entityid)、[`BACKGROUND`](#background) | 構造体、定数 | エンティティの識別子 |
| [`Part`](#part)、[`MAIN`](#main) | 構造体、定数 | 画像レイヤーのキー |
| [`Point`](#point)、[`Offset`](#offset)、[`Size`](#size)、[`Insets`](#insets)、[`Anchor`](#anchor)、[`Angle`](#angle)、[`Pose`](#pose) | 構造体とコンストラクター | 幾何 |
| [`Color`](#color) と定数 | 構造体とコンストラクター | sRGB の色 |
| [`Plane`](#plane)、[`Visibility`](#visibility)、[`Flip`](#flip)、[`VAlign`](#valign) | 列挙型 | 配置に関する選択 |
| [`Screen`](#screen) | 組み込みの構造体 | 画面とステップのタイムライン |
| [`Entity`](#entity) | 組み込みの構造体 | 画面上の物体 |
| [`Prop`](#prop)、[`Change`](#change) | 組み込みの構造体 | 補間できるプロパティ |
| [`Resize`](#resize) | 列挙型 | 画像レイヤーでの内容の大きさの決め方 |
| [`ImageLayer`](#imagelayer)、[`LabelLayer`](#labellayer)、[`Movie`](#movie)、[`MovieLayer`](#movielayer)、[`Swap`](#swap) | 構造体 | レイヤー |
| [`Hud`](#hud) | 組み込みの構造体 | ゲーム画面の枠（UI 部分） |
| [`ChoiceMenu`](#choicemenu) | 組み込みの構造体 | そのステップでの選択肢の見せ方 |
| [`Camera`](#camera) | 組み込みの構造体 | 下側の描画面にかかるカメラ |

## 論理画面 {#the-logical-screen}

ステップの**論理画面**とは、レンダーを実行した後に残る永続的な状態です。

- エンティティとそのコンポーネント
- Hud、カメラ、画面フィルター
- オーディオチャンネル

スナップショット、画面の等価判定、一貫性の検査は論理画面だけを見ます。タイムライン、予約したアニメーション、単発のサウンド、`ChoiceMenu` はステップに属するもので、論理画面には含まれません。

アニメーションによって開始状態や終了状態が変わることはありません。補間やトランジションは古い値から `change` で設定した値へ進むだけで、モーションは恒等変換で終わり、シェーダーモーションはエンティティの描き方しか変えません。そのため、クリックやスキップをしても、`animations` 設定をオフにしても、論理画面は変わりません。

## ステップの開始 {#step-entry}

ステップのレンダーを実行する前に、エンジンはそのステップ限りの状態をリセットします。

- タイムラインのカーソルを（開始, 0 ms）に移す。
- 予約済みのアニメーションを完了させる。
- 単発サウンドのキューを破棄する（すでに再生中のものは除く）。
- `ChoiceMenu` を空にし、`Middle` に設定する。
- テキストの開始位置を（開始, 0）に移す。

`render` のないステップでも同じリセットを行います。

## 型：`Render` {#render}

```kis
pub type Render = const fn(Screen) -> Screen;
```

レンダーの型で、ステップ前の画面からステップ後の画面への関数です。ステップの `render =` フィールドには、名前付きの `const fn` またはクロージャを `Render` として渡します。

```kix
s!(render = |mut screen| {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen
});
```

入力に依存しない出力を返すレンダーはキーフレームです。[`Screen::empty()`](#screen-empty) を参照してください。

## 幾何 {#geometry}

位置と長さは、デザイン解像度（`System::resolution`）でのピクセル数です。エンティティ ID は制作者が定数として定義します。

### 構造体：`EntityId` {#entityid}

```kis
pub struct EntityId(pub u32);
```

エンティティの識別子です。ID は定数として宣言します（`const HANAKO: EntityId = EntityId(1);`）。描画面と `z` が同じエンティティどうしの順序も ID で決まります。[描画順序](#drawing-order) を参照してください。

### 定数：`BACKGROUND` {#background}

```kis
pub const BACKGROUND: EntityId = EntityId(0);
```

`Screen::empty()` の背景エンティティ用に予約された ID です。制作者が使う ID は 1 から始めます。[`Screen::background()`](#screen-background) はこのエンティティのメイン画像です。

### 構造体：`Part` {#part}

```kis
pub struct Part(pub u8);
```

エンティティの画像レイヤーです。パーツは番号の小さい順に描画されるので、1 体のエンティティに体、顔、オーバーレイを重ねられます。

### 定数：`MAIN` {#main}

```kis
pub const MAIN: Part = Part(0);
```

メインのパーツです。このパーツの矩形がエンティティの大きさになります（[`Resize`](#resize) を参照）。[`Entity::image()`](#entity-image) は `part(MAIN)` と同じです。

### 構造体：`Point` {#point}

```kis
pub struct Point {
    pub x: i32,
    pub y: i32,
}
pub const fn point(x: i32, y: i32) -> Point
```

デザイン解像度のピクセルで表した点です。`point(x, y)` で作ります。種別：`const fn` · 組み込み：いいえ。

### 構造体：`Offset` {#offset}

```kis
pub struct Offset {
    pub dx: i32,
    pub dy: i32,
}
pub const fn offset(dx: i32, dy: i32) -> Offset
```

ピクセル単位の変位です。`offset(dx, dy)` で作ります。カメラやモーションのキーで使います。種別：`const fn` · 組み込み：いいえ。

### 構造体：`Size` {#size}

```kis
pub struct Size {
    pub w: u32,
    pub h: u32,
}
pub const fn size(w: u32, h: u32) -> Size
```

ピクセル単位の大きさです。`size(w, h)` は `w × h` を表します。[`Entity::set_fill_size`](#entity-set_fill_size) や [`Resize`](#resize) で使います。種別：`const fn` · 組み込み：いいえ。

### 構造体：`Insets` {#insets}

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

矩形の 4 辺からの距離（ピクセル）です。`insets` は CSS と同じ順序（上、右、下、左）で指定します。`uniform(n)` は 4 辺とも `n` になります。種別：`const fn` · 組み込み：いいえ。

### 構造体：`Anchor` {#anchor}

```kis
pub struct Anchor {
    pub x: Decimal,
    pub y: Decimal,
}
pub const TOP_LEFT: Anchor = Anchor { x: 0.0, y: 0.0 };
pub const CENTER: Anchor = Anchor { x: 0.5, y: 0.5 };
pub const BOTTOM_CENTER: Anchor = Anchor { x: 0.5, y: 1.0 };
```

エンティティのどの点を位置に合わせるかを、エンティティ自身の大きさ（矩形。[描画順序](#drawing-order) を参照）に対する割合 `0.0..=1.0` で表します。`TOP_LEFT` は左上の角（生成時の既定値）、`CENTER` は中心、`BOTTOM_CENTER` は下辺の中点です（キャラクターの立ち絵でよく使います）。拡大縮小と回転はアンカーを中心に行います。

### 構造体：`Angle` {#angle}

```kis
pub struct Angle {
    pub degrees: Decimal,
}
pub const fn degrees(d: Decimal) -> Angle
```

時計回りの回転です。`degrees(d)` は `d` 度の回転を表します。種別：`const fn` · 組み込み：いいえ。

### 構造体：`Pose` {#pose}

```kis
pub struct Pose {
    pub position: Point,
    pub scale: Decimal,
    pub rotation: Angle,
}
pub const fn pose(position: Point, scale: Decimal, rotation: Angle) -> Pose
```

エンティティの姿勢、つまりエンティティの[プロパティ](#entity-properties)のうち位置、拡大率、回転の組です。`pose(position, scale, rotation)` で作り、[`Entity::travel`](#entity-travel) で使います。種別：`const fn` · 組み込み：いいえ。

## 色 {#colour}

### 構造体：`Color` {#color}

```kis
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
```

アルファ付きの sRGB 色です。

### `rgb(r, g, b)`、`rgba(r, g, b, a)`、`hex(rgb)` {#color-constructors}

```kis
pub const fn rgb(r: u8, g: u8, b: u8) -> Color
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color
pub const fn hex(rgb: u32) -> Color
```

- `rgb` は不透明な色（アルファ 255）を返します。`rgba` はアルファも指定します。
- `hex(0xRRGGBB)` は不透明な色を返します。`0xFFFFFF` を超えるビットは無視します。たとえば `hex(0x382000)` は `rgb(0x38, 0x20, 0x00)` と同じです。
- 種別：`const fn` · 組み込み：いいえ。

### 定数：`BLACK`、`WHITE`、`TRANSPARENT` {#color-constants}

```kis
pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
```

不透明な黒、不透明な白、完全な透明です。

## 描画面と反転 {#planes-and-flips}

### 列挙型：`Plane` {#plane}

```kis
pub enum Plane {
    Background,
    Stage,
    Front,
    OverWindow,
}
```

描画面です。ゲーム画面はテキストウィンドウを `Front` と `OverWindow` の間に描画します。カメラと画面フィルターがかかるのは `Background`、`Stage`、`Front` だけです。

### 列挙型：`Visibility` {#visibility}

```kis
pub enum Visibility {
    Shown,
    Hidden,
}
```

Hud の部品の表示・非表示です。`Prop<Visibility>` を変更すると、フェードとして補間されます。

### 列挙型：`Flip` {#flip}

```kis
pub enum Flip {
    None,
    Horizontal,
    Vertical,
    Both,
}
```

エンティティの描画を左右・上下に反転します。[`Entity::set_flip`](#entity-set_flip) で設定します。

### 列挙型：`VAlign` {#valign}

```kis
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}
```

縦方向の配置です。[`ChoiceMenu::set_align`](#choicemenu-set_align) で使います。

## シェーダー効果 {#shader-effects}

シェーダーとは、レンダラーが実行する WGSL ファイルです。画面はシェーダーを、シェーダーと引数を組にした**効果値**として保持します。効果値は [`std::screen::shader`](./screen-shader.md) で宣言し、既製のものは [`std::shaders`](./shaders.md) にあります。効果のスロットはどれも省略可能で、`None` は効果なしを表します。

| スロット | 値 | 効果 |
| ---- | ----- | ------ |
| [`Screen::filter`](#screen-filter) | `Prop<Option<Filter>>` | `Background`、`Stage`、`Front` の描画面の色を変える |
| [`Entity::filter`](#entity-filter) | `Prop<Option<Filter>>` | エンティティの色を変える |
| [`Entity::mix`](#entity-mix) | `Prop<Option<Mix>>` | エンティティを下にあるものと合成する。`None` は通常のソースオーバー |
| [`Entity::mask`](#entity-mask) | `Prop<Option<Mask>>` | エンティティの矩形の範囲でアルファを乗算する |
| [`Entity::local_motion`](#entity-local_motion) | `Prop<Option<LocalMotion>>` | エンティティを姿勢の周りで動かす（見た目のみ） |
| [`Entity::travel`](#entity-travel) | 呼び出し | シェーダーの経路に沿ってエンティティを新しい姿勢へ移動する |
| [`ImageLayer::paint`](#imagelayer) | レイヤーの差し替え | 画像の代わりに生成した内容を表示する |

**効果の補間。** 効果のスロットは `Prop` なので、変更には補間を指定します。古い値と新しい値が同じシェーダーを使う場合（ローカルモーションでは振る舞いも同じ場合）は、引数が補間されます。小数、色、点、大きさ、角度、長さはイージングに従って変化し、整数、`bool`、列挙型は補間の開始時に切り替わります。`None` との間の変更を含め、それ以外の変更はすべて補間の開始時に切り替わります。

```kis
use std::shaders::color::blur;

screen.filter().change(Some(blur(0.0))).apply_animation(instant());
screen.wait(ms(500));
screen.filter().change(Some(blur(6.0))).apply_animation(tween(ms(1500)));
```

**効果の時刻。** シェーダーには、効果が始まってからの秒数が渡されます。スロットが同じシェーダーを使い続ける限り、時刻はステップをまたいで進みます。[効果の時刻](./guides/shaders.md#effect-time) を参照してください。`animations` 設定がオフのときは、効果の時刻は常に 0 です。

**シェーダーを実行できない場合。** `kagari build` はゲームで使うシェーダーをすべて検査するため、壊れたシェーダーがあるとビルドできません。ある役割のシェーダーを GPU で実行できない場合は、そのスロットを `None` として描画し（移動は直線になり、マスク付きトランジションはクロスフェードになり、塗りは何も描きません）、警告をログに出します。どちらの場合も論理画面は同じです。

## 構造体：`Screen` {#screen}

```kis
#[builtin] pub struct Screen;
```

ステップが表示する画面と、そのステップのタイムラインです。`Screen` は中身の見えない組み込みのハンドルで、レンダーは以下のメソッドを通してしか状態に触れられません。画面は `==` で比較できます（`Eq` を実装しています）。比較するのは[論理画面](#the-logical-screen)だけです。

### `Screen::empty()` {#screen-empty}

```kis
#[builtin] pub const fn empty() -> Screen;
```

- 戻り値：不透明な `BACKGROUND` エンティティ（描画面 `Background`、不透明度 1.0、画像なし）だけがある画面。`MAIN` レイヤーのリサイズはデザイン解像度（`System::resolution`）での [`Resize::Cover`](#resize) なので、背景画像は解像度に関係なく画面全体を覆います。
- Hud はテキストウィンドウとクイックメニューが `Shown` で、サイド画像はなし。
- カメラはオフセット 0、ズーム 1.0、回転 0。
- 画面フィルターは `None`。
- オーディオチャンネルはすべて無音で、音量 1.0。
- 種別：`const fn` · 組み込み：はい。

入力に依存しない出力を返すレンダーはキーフレームです。キーフレームは通常 `Screen::empty()` を使って書きます。

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

- `id` `EntityId` — 対象のエンティティ。
- 戻り値：`&mut Entity` — そのエンティティ。存在しない場合は既定のコンポーネントで生成します。
- 種別：`const fn` · 組み込み：はい。

#### 生成 {#spawning}

`entity(id)` は、エンティティが存在しなければ次の既定値で生成します。

| コンポーネント | 既定値 |
| --------- | ------- |
| plane, z | `Stage`, 0 |
| position, anchor | (0, 0), `TOP_LEFT` |
| scale, rotation | 1.0, 0° |
| **opacity** | **0.0** |
| filter, mix, mask, local motion | `None` |
| fill, fill size | `TRANSPARENT`, `None`（画面全体） |
| layers | パーツ、ラベル、ムービーなし。パーツは `Resize::Natural` から始まる |
| flip, gate | `Flip::None`, `None` |

生成したばかりのエンティティは見えないので、登場させるときは毎回アニメーションを指定します。

```kis
screen.entity(HANAKO).image().change(HANAKO::SMILE).apply_animation(cut());
screen.entity(HANAKO).opacity().change(1.0).apply_animation(tween(ms(300)));
```

消去が保留中の ID に `entity(id)` を呼ぶと、消去は取り消されます。エンティティは現在の値のまま残ります。

この既定値はどの ID にも当てはまります。`BACKGROUND` エンティティを消去した後で生成し直すと、`MAIN` レイヤーは `Resize::Natural` になり、[`Screen::empty()`](#screen-empty) が設定する `Cover` にはなりません。

### `screen.contains(id)` {#screen-contains}

```kis
#[builtin] pub const fn contains(&self, id: EntityId) -> bool;
```

- 戻り値：`bool` — エンティティ `id` が論理画面上にあるかどうか。消去済みでフェードアウト中のエンティティは含みません。
- 種別：`const fn` · 組み込み：はい。

### `screen.entities(plane)` {#screen-entities}

```kis
#[builtin] pub const fn entities(&self, plane: Plane) -> Vec<EntityId>;
```

- 戻り値：`Vec<EntityId>` — 論理画面上で描画面 `plane` にある ID を昇順に並べたもの。
- 種別：`const fn` · 組み込み：はい。

### `screen.despawn(id)` {#screen-despawn}

```kis
#[builtin] pub const fn despawn(&mut self, id: EntityId);
```

エンティティ `id` を論理画面からただちに取り除きます。そのため次のステップにはこのエンティティは引き継がれません。消去そのものは[カーソル](./screen-anim.md#the-timeline)の位置に予約されます。タイムラインがカーソルに達するまで、または予約済みのアニメーションが終わるまでのうち遅いほうまで描画が続き、その後で消えます。`barrier()` はこの消去の後までカーソルを進め、クリックするとタイムラインの残りと一緒に消去も完了します。種別：`const fn` · 組み込み：はい。

フェードアウトの後に `despawn` を書けば、フェードアウトが見えます。

```kis
screen.entity(CARD).opacity().change(0.0).apply_animation(tween(ms(500)));
screen.despawn(CARD);
```

`wait` の後で消去したエンティティは、カーソルの位置まで残ります。次の例では、プレートがフェードインして 3 秒間表示されます。

```kis
screen.entity(PLATE).opacity().change(1.0).apply_animation(tween(ms(500)));
screen.wait(ms(3000));
screen.despawn(PLATE);
```

### `screen.background()` {#screen-background}

```kis
pub const fn background(&mut self) -> &mut ImageLayer {
    self.entity(BACKGROUND).image()
}
```

`BACKGROUND` エンティティのメイン画像です。`Screen::empty()` はこのレイヤーにデザイン解像度での [`Resize::Cover`](#resize) を設定するので、背景は解像度に関係なく画面全体を覆い、解像度の異なる背景どうしのトランジションも全画面のままになります。種別：`const fn` · 組み込み：いいえ。

```kis
screen.background().change(BG::ROOF).apply_animation(fade(ms(500)));
```

**移行について。** `Resize` が入る前は、背景は画像のピクセルサイズのまま描画されていました。現在は画面全体を覆います。デザイン解像度で作った背景の見た目は変わりません。背景エンティティを拡大縮小して（`screen.entity(BACKGROUND).scale()`）背景を画面に合わせていた場合は、その拡大率を外すか、次のように書いて以前の描画を保ってください。

```kis
use std::screen::Resize;

screen.background().set_resize(Resize::Natural);
```

### `screen.hud()` {#screen-hud}

```kis
#[builtin] pub const fn hud(&mut self) -> &mut Hud;
```

ゲーム画面が描画する [Hud](#hud) です。種別：`const fn` · 組み込み：はい。

### `screen.choice_menu()` {#screen-choice_menu}

```kis
#[builtin] pub const fn choice_menu(&mut self) -> &mut ChoiceMenu;
```

このステップの[選択肢メニュー](#choicemenu)です。種別：`const fn` · 組み込み：はい。

### `screen.camera()` {#screen-camera}

```kis
#[builtin] pub const fn camera(&mut self) -> &mut Camera;
```

`Background`、`Stage`、`Front` の描画面にかかる[カメラ](#camera)です。種別：`const fn` · 組み込み：はい。

### `screen.filter()` {#screen-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<shader::Filter>>;
```

`Background`、`Stage`、`Front` の描画面に、カメラの後でかかる色フィルターです。`None` はフィルターなしを表します。[シェーダー効果](#shader-effects) を参照してください。種別：`const fn` · 組み込み：はい。

### `screen.music()`、`screen.ambience(channel)` {#screen-music}

```kis
#[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
#[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
```

音楽チャンネルと、4 つの環境音チャンネル `ambience(0)`〜`ambience(3)` です。[チャンネル](./screen-audio.md#channels) を参照してください。種別：`const fn` · 組み込み：はい。

### `screen.play(sound)`、`screen.stop_sounds(fade_out)` {#screen-play}

```kis
#[builtin] pub const fn play(&mut self, sound: audio::Sound);
#[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
```

`play` は単発のサウンドを、カーソル位置にサウンド自身の遅延を足した時刻に再生します。`stop_sounds` は、カーソル位置で再生中の単発サウンドをすべて `fade_out` の時間をかけてフェードアウトさせます。[`Sound`](./screen-audio.md#sound) を参照してください。種別：`const fn` · 組み込み：はい。

### `screen.replay_bgm()` {#screen-replay_bgm}

```kis
#[builtin] pub const fn replay_bgm(&mut self);
```

音楽チャンネルの曲を、カーソル位置で頭から再生し直します。これを呼ばない限り、再生中の曲は頭に戻らずに後のステップへ引き継がれます。[同一トラック規則](./screen-audio.md#the-same-track-rule) を参照してください。種別：`const fn` · 組み込み：はい。

### タイムラインメソッド {#timeline-methods}

```kis
#[builtin] pub const fn wait(&mut self, duration: Duration);
#[builtin] pub const fn barrier(&mut self);
#[builtin] pub const fn at_mark(&mut self, mark: Mark);
#[builtin] pub const fn at_entry(&mut self);
#[builtin] pub const fn begin_text(&mut self);
```

| メソッド | 効果 |
| ------ | ------ |
| `wait(d)` | カーソルを `d` だけ進める |
| `barrier()` | カーソルを、それまでに予約したものがすべて終わる時刻へ移す |
| `at_mark(m)` | カーソルを、テキストが `m` に達する時刻へ移す |
| `at_entry()` | カーソルをステップの開始時刻へ戻す |
| `begin_text()` | カーソル位置でテキスト表示とボイスを開始する |

カーソルの考え方は [タイムライン](./screen-anim.md#the-timeline) で説明しています。種別：`const fn` · 組み込み：はい。

## 構造体：`Entity` {#entity}

```kis
#[builtin] pub struct Entity;
```

画面上の物体です。変換、不透明度、色フィルター、合成、マスク、ローカルモーション、塗り、レイヤーを持ちます。コンポーネントの種類は固定で、制作者が追加することはできません。

| メンバーの種類 | メンバー | 変更方法 |
| -------------- | ------- | ------------ |
| 補間できるプロパティ | `position`, `scale`, `rotation`, `opacity`, `filter`, `mix`, `mask`, `local_motion`, `fill` | `change(v).apply_animation(Tween)` |
| レイヤー | `part(p)`, `image()`, `label()`, `movie()` | `change(x)` または `paint(p)` の後に `apply_animation(Transition)` |
| 配置の設定 | `set_plane`, `set_z`, `set_anchor`, `set_flip`, `set_fill_size`, `gate`、画像レイヤーの [`set_resize`](#imagelayer-set_resize) | 直接設定する。補間はできない |
| 相対モーション | `apply_animation(Motion)` | [`Motion`](./screen-anim.md#motion) を参照 |
| シェーダーの経路 | `travel(to, motion)` | [`travel`](#entity-travel) を参照 |

### 補間できるプロパティ {#entity-properties}

```kis
#[builtin] pub const fn position(&mut self) -> &mut Prop<Point>;
#[builtin] pub const fn scale(&mut self) -> &mut Prop<Decimal>;
#[builtin] pub const fn rotation(&mut self) -> &mut Prop<Angle>;
#[builtin] pub const fn opacity(&mut self) -> &mut Prop<Decimal>;
```

- `position` は、アンカー点を置く位置です。
- `scale` はアンカーを中心とした拡大率、`rotation` はアンカーを中心とした時計回りの回転です。
- `opacity` は `0.0..=1.0` の割合です。生成したエンティティは 0.0 から始まります。
- 種別：`const fn` · 組み込み：はい。

### `entity.filter()` {#entity-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<shader::Filter>>;
```

このエンティティにかける色フィルターです。`None` はフィルターなしを表します。フィルターは、マスクとモーションを適用した後、エンティティが描くものすべてに画面空間でかかります。[シェーダー効果](#shader-effects) を参照してください。種別：`const fn` · 組み込み：はい。

### `entity.mix()` {#entity-mix}

```kis
#[builtin] pub const fn mix(&mut self) -> &mut Prop<Option<shader::Mix>>;
```

エンティティを下にあるものとどう合成するかを指定します。`None` は通常のソースオーバーです。乗算やスクリーンなどの描画モードは合成として書きます（[`std::shaders::mix`](./shaders-mix.md)）。エンティティの不透明度は合成の前に適用します。種別：`const fn` · 組み込み：はい。

```kis
screen.entity(TINT).mix().change(Some(std::shaders::mix::color())).apply_animation(instant());
```

### `entity.mask()` {#entity-mask}

```kis
#[builtin] pub const fn mask(&mut self) -> &mut Prop<Option<shader::Mask>>;
```

エンティティの矩形にかけるアルファです。エンティティが矩形内に描くものすべてにマスクを乗算します。マスクはエンティティと一緒に移動、拡大縮小、回転します。塗りサイズを指定していない塗りは画面全体を覆い、マスクはかかりません。`None` はマスクなしを表します。種別：`const fn` · 組み込み：はい。

### `entity.local_motion()` {#entity-local_motion}

```kis
#[builtin] pub const fn local_motion(&mut self) -> &mut Prop<Option<shader::LocalMotion>>;
```

エンティティの姿勢を基準に、見た目だけを動かすモーションです。`None` はモーションなしを表します。シェーダーは毎フレーム、補間と [`Motion`](./screen-anim.md#motion) を反映した姿勢から計算するので、上下に揺れながら歩くキャラクターも作れます。論理的な位置、拡大率、回転は変わりません。塗りサイズを指定していない塗りは動きません。種別：`const fn` · 組み込み：はい。

```kis
screen.entity(HANAKO).local_motion().change(Some(std::shaders::motion::bob(20.0, ms(1000)))).apply_animation(instant());
```

### `entity.travel(to, motion)` {#entity-travel}

```kis
#[builtin] pub const fn travel(&mut self, to: Pose, motion: shader::TransitionalMotion);
```

`motion` の経路に沿って、エンティティを `to` へ移動します。種別：`const fn` · 組み込み：はい。

- 論理的な姿勢はすぐに `to` になります。同じレンダーでこの後に `position().get()` を呼ぶと `to.position` が返ります。
- `travel` は `position`、`scale`、`rotation` のそれぞれに、モーションの長さ、イージング、遅延を使った補間を予約し、あわせてシェーダーの経路も予約します。移動中は、開始時の姿勢、`to`、イージング適用後の進み具合から、シェーダーが描画する姿勢を決めます。移動が終わると、エンティティは `to` にあります。
- シェーダーを実行できない環境では、通常の補間どおり直線で移動します。
- エンティティのローカルモーションは、移動に重ねて適用されます。
- イージングが不正な場合は、補間と同じく定数評価エラーになります。

```kis
use std::screen::{degrees, pose};

screen.entity(BIRD).travel(pose(point(900, 400), 1.0, degrees(0.0)), std::shaders::motion::arc(200.0, ms(1200)));
```

### `entity.fill()` {#entity-fill}

```kis
#[builtin] pub const fn fill(&mut self) -> &mut Prop<Color>;
```

パーツの下に、塗りサイズの範囲で描く色です。塗りサイズが既定（`None`、画面全体）のままなら、塗りだけのエンティティは全画面の単色板になります。種別：`const fn` · 組み込み：はい。

### `entity.part(part)`、`entity.image()` {#entity-image}

```kis
#[builtin] pub const fn part(&mut self, part: Part) -> &mut ImageLayer;
pub const fn image(&mut self) -> &mut ImageLayer {
    self.part(MAIN)
}
```

画像レイヤー `part` です。`image()` は `MAIN` レイヤーを返します。パーツは `Part` の小さい順に描画されます。種別：`const fn` · 組み込み：`part` ははい、`image` はいいえ。

> **安定性:** 予定 — レイヤー構成の立ち絵での自動まばたきや口パク（自律的に動くパーツ、ボイスに合わせた口パク）は未設計です。

### `entity.label()` {#entity-label}

```kis
#[builtin] pub const fn label(&mut self) -> &mut LabelLayer;
```

[テキストレイヤー](#labellayer)です。パーツとムービーより上に描画されます。種別：`const fn` · 組み込み：はい。

### `entity.movie()` {#entity-movie}

```kis
#[builtin] pub const fn movie(&mut self) -> &mut MovieLayer;
```

[ムービーレイヤー](#movielayer)です。パーツより上、ラベルより下に描画されます。種別：`const fn` · 組み込み：はい。

<a id="entity-set_flip"></a><a id="entity-set_fill_size"></a>

### 配置の設定 {#entity-setters}

```kis
#[builtin] pub const fn set_plane(&mut self, plane: Plane);
#[builtin] pub const fn set_z(&mut self, z: i32);
#[builtin] pub const fn set_anchor(&mut self, anchor: Anchor);
#[builtin] pub const fn set_flip(&mut self, flip: Flip);
#[builtin] pub const fn set_fill_size(&mut self, size: Option<Size>);
```

| 設定 | 効果 |
| ------ | ------ |
| `set_plane(p)` | エンティティを描画面 `p` へ移す |
| `set_z(z)` | 描画面内での順序を設定する。値が大きいほど後（手前）に描画される |
| `set_anchor(a)` | エンティティのどの点を位置に合わせるかを設定する |
| `set_flip(f)` | 描画を反転する |
| `set_fill_size(s)` | 塗りの大きさを設定する。`None` は画面全体 |

これらの設定はすぐに反映され、アニメーションはしません。種別：`const fn` · 組み込み：はい。画像レイヤーごとの内容の大きさの決め方は、レイヤー側の [`ImageLayer::set_resize`](#imagelayer-set_resize) で設定します。

### `entity.gate(setting)` {#gate}

```kis
#[builtin] pub const fn gate(&mut self, setting: Option<Setting<bool>>);
```

`gate(Some(s))` を指定すると、設定 `s` が true の間だけエンティティを描画します。`gate(None)` なら常に描画します。種別：`const fn` · 組み込み：はい。

- ハンドルは論理画面の一部で、値ではなく同一性で比較します。レンダーからは設定を読めないので、論理画面が設定に左右されることはありません。
- エンジンは描画のたびに値を読みます（遅延参照）。

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

開始位置に戻って終わる相対[モーション](./screen-anim.md#motion)を再生します。エンティティの論理的な状態は変わりません。種別：`const fn` · 組み込み：はい。

### 描画順序 {#drawing-order}

- 描画面は `Background < Stage < Front <（ゲーム画面が描くテキストウィンドウ）< OverWindow` の順に描画します。同じ描画面の中では `z` の昇順、次に `EntityId` の昇順です。`z` は CSS の `z-index` にあたります。
- エンティティの中では、塗り、パーツ（画像と塗り。`Part` の昇順）、ムービー、ラベルの順に描画します。
- アンカー点は `position` に置かれます。エンティティの矩形（大きさ）は、`MAIN` レイヤーの[矩形](#resize)（差し替え中は新しい内容のもの）です。それがなければ塗りサイズ、それもなければラベルの矩形、それもなくムービーを表示している場合は画面の大きさになります。`MAIN` レイヤーはアンカーに従ってエンティティの矩形内に配置します。ムービーは矩形に収まるよう縮尺して（contain）中央に置き、ほかのパーツは矩形の左上の角に置きます。いずれも自身のリサイズで大きさが決まります。拡大縮小と回転は、リサイズの後でエンティティ全体にアンカーを中心としてかかります。
- エンティティの効果は、内側から順にエンティティの矩形にかかるマスク、ローカルモーション（と移動）、フィルター、不透明度込みの合成という入れ子になります。塗りサイズを指定していない塗りはフィルターと合成の内側に描きますが、動きもマスクもかかりません。
- カメラがかかるのは `Background`、`Stage`、`Front` だけです。オフセットで平行移動し、ズームと回転は画面中心を軸にします。
- 画面フィルターも同じ 3 つの描画面に、カメラの後でかかります。

## 構造体：`Prop` {#prop}

```kis
#[builtin] pub struct Prop<T>;
impl<T> Prop<T> {
    #[builtin] pub const fn get(&self) -> T;
    #[builtin] pub const fn change(&mut self, value: T) -> &mut Change<T>;
}
```

補間できるプロパティです。`get()` は論理値を返します。`change(v)` は論理値をすぐに設定し、保留中の [`Change`](#change) を返します。補間を指定した時点で変更が実際に起こります。種別：`const fn` · 組み込み：はい。

`prop.change(v).apply_animation(t)` の動作は次のとおりです。

- 論理値はすぐに `v` になるので、同じレンダーでこの後に `get` を呼ぶと `v` が返ります。
- エンジンは、`cursor + t.delay` 時点のプロパティの値から `v` まで、`t.easing` に従って `t.duration` かけて補間します。
- 同じプロパティの前のアニメーションが実行中のまま次のアニメーションが始まると、前のアニメーションは終点まで飛ばされます。
- `Prop<Visibility>` はフェードとして補間されます。

```kis
screen.entity(HANAKO).position().change(point(400, 80)).apply_animation(tween(ms(1000)).ease(Easing::InOutQuad));
```

## 構造体：`Change` {#change}

```kis
#[must_use] #[builtin] pub struct Change<T>;
impl<T> Change<T> {
    #[builtin] pub const fn apply_animation(&mut self, tween: anim::Tween);
}
```

保留中のプロパティ変更です。変更は `apply_animation` を呼んだときにだけ起こり、カーソル位置に遅延を足した時点の値から新しい値まで補間します。`Change<T>` には `#[must_use]` が付いています。`apply_animation` を呼ばない変更は起こらず、その文は警告になります。すぐに反映させたい変更は [`instant()`](./screen-anim.md#tween) と明示します。種別：`const fn` · 組み込み：はい。

## レイヤーと `Swap` {#layers-and-swap}

レイヤーの差し替えはプロパティの変更と同じ仕組みで、`Tween` の代わりに [`Transition`](./screen-anim.md#transition) を使います。論理的な内容はすぐに変わり、`cursor + delay` の時点からトランジションで古い内容から新しい内容へ切り替わります。`clear()` は空の状態へ差し替えます。

### 列挙型：`Resize` {#resize}

```kis
pub enum Resize {
    Natural,
    Factor(Decimal),
    Width(u32),
    Height(u32),
    Stretch(Size),
    Contain(Size),
    Cover(Size),
}
```

[画像レイヤー](#imagelayer)で内容をどの大きさにするかを、エンティティの拡大率を適用する前の段階で決めます。リサイズは、内容自身の大きさ（画像のピクセルサイズ、または塗りの `size`）から、2 つの大きさを求めます。1 つはレイヤーの**矩形**で、エンティティの中でレイヤーが占める大きさです。もう 1 つは内容を描く大きさです。内容は矩形の中央に置きます。

| 種類 | 矩形 | 内容 |
| ------- | --- | ------- |
| `Natural` | 元の大きさ | 矩形と同じ |
| `Factor(f)` | 元の大きさ × `f`。`f` ≤ 0.0 なら何も描かない | 矩形と同じ |
| `Width(w)` | `w` ×（元の高さ · `w` / 元の幅） | 矩形と同じ |
| `Height(h)` | （元の幅 · `h` / 元の高さ）× `h` | 矩形と同じ |
| `Stretch(s)` | `s`（縦横比は無視） | 矩形と同じ |
| `Contain(s)` | `s` | 縦横比を保ったまま `s` に収まる最大の大きさ。余白は透明 |
| `Cover(s)` | `s` | 縦横比を保ったまま `s` を覆う最小の大きさ。はみ出た部分は切り取る |

- 内容がレイヤーの矩形の外に描かれることはありません。はみ出すのは `Cover` だけです。切り取りは画像や塗りそのものを切り抜くので、拡大縮小、回転、反転、モーションを適用しても正確です。
- `MAIN` レイヤーの矩形がエンティティの矩形になり、アンカー、マスク、ほかのパーツはこの矩形を基準にします。[描画順序](#drawing-order) を参照してください。
- エンティティの `scale` は、リサイズの後でエンティティ全体にかかります。リサイズは配置上の大きさ、`scale` はアニメーションで動かすズームと考えてください。
- 画像レイヤーは、Hud のサイド画像も含めてすべて `Natural` から始まります。例外は [`Screen::empty()`](#screen-empty) が作る `BACKGROUND` エンティティの `MAIN` レイヤーで、デザイン解像度（`System::resolution`）での `Cover` から始まります。
- 設定には [`ImageLayer::set_resize`](#imagelayer-set_resize) を使います。

1280×720 のゲームで、画像の解像度に関係なく CG 全体を表示する例です。

```kis
use std::screen::{Resize, size};

const fn cg(mut screen: Screen) -> Screen {
    screen.entity(CG).set_plane(Plane::Front);
    screen.entity(CG).image().set_resize(Resize::Contain(size(1280, 720)));
    screen.entity(CG).image().change(CG_001).apply_animation(fade(ms(500)));
    screen.entity(CG).opacity().change(1.0).apply_animation(tween(ms(500)));
    screen
}
```

### 構造体：`ImageLayer` {#imagelayer}

```kis
#[builtin] pub struct ImageLayer;
impl ImageLayer {
    #[builtin] pub const fn get(&self) -> Option<ImageAsset>;
    #[builtin] pub const fn change(&mut self, image: ImageAsset) -> &mut Swap;
    #[builtin] pub const fn paint(&mut self, paint: shader::Paint) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
    #[builtin] pub const fn set_resize(&mut self, resize: Resize);
}
```

エンティティの画像レイヤーです。種別：`const fn` · 組み込み：はい。

- `get()` は表示中の画像を返します（なければ `None`）。塗りを表示しているときも `None` です。
- `change(image)` は内容を `image` に差し替えます。
- `paint(p)` は内容を、生成した [`Paint`](./screen-shader.md#paint) `p` に差し替えます。グラデーションや雨のように、テクスチャーシェーダーが `p` の大きさで描く内容です。同じ大きさの画像と同じように配置、反転、差し替え、[リサイズ](#resize)ができます。内容は `p` の大きさで生成してから描画サイズへ引き伸ばすので、シェーダーから見える大きさは `p` のピクセルサイズのままです。`Cover` では切り取られます。
- `clear()` は空の状態に差し替えます。
- `set_resize(r)` は、レイヤーで内容をどの大きさにするかを設定します。[`set_resize`](#imagelayer-set_resize) を参照してください。

```kis
use std::screen::{degrees, rgba, size};

screen.entity(RAIN).image().paint(std::shaders::texture::rain(rgba(200, 210, 255, 160), 30, 900.0, degrees(10.0), size(1920, 1080))).apply_animation(fade(ms(500)));
```

#### `layer.set_resize(resize)` {#imagelayer-set_resize}

```kis
#[builtin] pub const fn set_resize(&mut self, resize: Resize);
```

- `resize` [`Resize`](#resize) — 内容の大きさの決め方。
- レイヤーが表示するすべての内容について、大きさの決め方を設定します。[配置の設定](#entity-setters)と同じく、すぐに反映され、アニメーションはせず、論理画面の一部になります。
- リサイズは内容ではなくレイヤーに属するので、`change`、`paint`、`clear` の後も変わりません。差し替えの前後両方に適用され、トランジション中は古い内容も新しい内容もこのリサイズで大きさが決まります。
- 種別：`const fn` · 組み込み：はい。

### 構造体：`LabelLayer` {#labellayer}

```kis
#[builtin] pub struct LabelLayer;
impl LabelLayer {
    #[builtin] pub const fn change(&mut self, text: Text) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
    #[builtin] pub const fn set_style(&mut self, style: TextStyle);
    #[builtin] pub const fn set_width(&mut self, width: Option<u32>);
}
```

エンティティのテキストレイヤーです。独自の [`TextStyle`](./narrative-text.md) で `Text` を表示します。`change(text)` と `clear()` でテキストを差し替え、`set_style` でスタイルを設定します。`set_width(Some(w))` は幅 `w` ピクセルで折り返し、`None` なら折り返しません。種別：`const fn` · 組み込み：はい。

### 構造体：`Movie` {#movie}

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

ムービーと、ループするかどうかの組です。`movie(v)` は `v` を 1 回だけ再生し、`movie(v).looping()` はループ再生します。種別：`const fn` · 組み込み：いいえ。

### 構造体：`MovieLayer` {#movielayer}

```kis
#[builtin] pub struct MovieLayer;
impl MovieLayer {
    #[builtin] pub const fn change(&mut self, movie: Movie) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
}
```

エンティティのムービーレイヤーです。`change(movie)` でムービーを `movie` に差し替え、`clear()` で空の状態に差し替えます。種別：`const fn` · 組み込み：はい。

ムービーは差し替えと同時に始まります。ステップのタイムラインは、[`screen.play(sound)`](#screen-play) と同じように、ループしないムービーの終わりまで待ちます。ループするムービーの場合、タイムラインに加わるのはトランジションの長さだけです。プレイヤーがアニメーションをオフにしていても、ムービーの時間は進みます。同じ動画が同じエンティティにある間は、ステップをまたいで再生が続きます。`V` の再生中に `change(movie(V))` しても頭からにはなりません。頭から再生し直すには、いったん `clear()` し、後のステップで設定し直してください。

ムービーの音声は Music バスで再生します。音声付きのムービーが始まると、効果音とボイスは止まります。ゲームの上にゲームメニューやオーバーレイを開くと、ムービーと音声は一時停止します。読み込めないムービーは描画されず、タイムラインにも何も加えません（ランタイムは警告をログに出します）。

### 構造体：`Swap` {#swap}

```kis
#[must_use] #[builtin] pub struct Swap;
impl Swap {
    #[builtin] pub const fn apply_animation(&mut self, transition: anim::Transition);
}
```

保留中のレイヤー差し替えです。差し替えは `apply_animation` を呼んだときにだけ起こり、`transition` で古い内容から新しい内容へ切り替わります。`Swap` には `#[must_use]` が付いています。種別：`const fn` · 組み込み：はい。

```kis
screen.background().change(BG::ROOF).apply_animation(fade(ms(500)));
```

## 構造体：`Hud` {#hud}

```kis
#[builtin] pub struct Hud;
impl Hud {
    #[builtin] pub const fn text_window(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn quick_menu(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn side_image(&mut self) -> &mut ImageLayer;
}
```

物語がゲーム画面の枠（UI 部分）に求める内容です。テキストウィンドウとクイックメニューの表示・非表示と、テキストウィンドウ横のサイド画像を表します。Hud は論理画面の一部です。ゲーム画面は `Narrative::hud()` で Hud を読みますが、物語側から読むことはありません。種別：`const fn` · 組み込み：はい。

サイド画像は[画像レイヤー](#imagelayer)で、自身の[リサイズ](#resize)で大きさが決まります。ゲーム画面は、その矩形の左下の角を画面の左下の角に合わせて描画します。

```kix
s!(render = |mut screen| {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen
});
```

## 構造体：`ChoiceMenu` {#choicemenu}

<a id="choicemenu-set_align"></a>

```kis
#[builtin] pub struct ChoiceMenu;
impl ChoiceMenu {
    #[builtin] pub const fn disabled_arm(&mut self, at: u32, text: Text);
    #[builtin] pub const fn set_align(&mut self, align: VAlign);
}
```

ステップでの選択肢の見せ方です。種別：`const fn` · 組み込み：はい。

- 選択肢メニューはステップ限りのものです。ステップが始まるたびに空になって `Middle` に設定され、論理画面には含まれません。
- `disabled_arm(at, text)` は、表示専用のアームを無効状態で表示リストの位置 `at`（範囲外なら範囲内に丸めます）に追加します。値がなく、選択できず、`choice!` の一部でもないので、どの解析からも見えません。どのレンダーで追加するかは物語のコード次第です。[`std::narrative`](./narrative.md#disabled-arms) を参照してください。
- `set_align(align)` はメニューの縦方向の位置を決めます。

## 構造体：`Camera` {#camera}

```kis
#[builtin] pub struct Camera;
impl Camera {
    #[builtin] pub const fn offset(&mut self) -> &mut Prop<Offset>;
    #[builtin] pub const fn zoom(&mut self) -> &mut Prop<Decimal>;
    #[builtin] pub const fn rotation(&mut self) -> &mut Prop<Angle>;
    #[builtin] pub const fn apply_animation(&mut self, motion: anim::Motion);
}
```

`Background`、`Stage`、`Front` の描画面にかかるカメラです。そのため、テキストウィンドウと `OverWindow` は動きません。`offset` で平行移動し、`zoom` と `rotation` は画面中心を軸にします。`apply_animation` は開始位置に戻って終わる相対モーションを再生します。シーン全体を揺らすときは通常これを使います。種別：`const fn` · 組み込み：はい。

```kis
screen.camera().apply_animation(quake(8, 4, ms(600)));
```
