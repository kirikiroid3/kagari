# `std::screen` {#stdscreen}

> **安定性:** 安定 — レンダーが変換する画面です。エンティティとそのコンポーネント、Hud、選択肢メニュー、カメラ、画面フィルター、オーディオチャンネルを扱います。

**レンダー**は、ステップ前の画面からステップ後の画面への `const fn` です。射影を通して画面をその場で変更し、1 文につき 1 つのチェーンを書きます。すべての変更はアニメーションを明示します。

```kis
const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

このモジュールのアイテムはすべて `const fn` か単純なデータです。そのためレンダーはコンパイル時に評価され、コンパイラは画面同士を比較できます。`Screen` のタイムラインメソッドはこのページで説明します。それらがスケジュールするアニメーション値は [`std::screen::anim`](./screen-anim.md) に、オーディオ値は [`std::screen::audio`](./screen-audio.md) にあります。このモジュールの上に作られたキャラクター用ヘルパーは [`std::stage`](./stage.md) に、完全な例は [演出レシピ](./guides/staging.md) にあります。

プレリュードは `Screen`、`Render`、`EntityId`、`Part`、`Point`、`Offset`、`Size`、`Anchor`、`Color`、`Plane`、`Visibility`、`point`、`offset`、`rgb`、`hex`、`BLACK`、`WHITE`、`TRANSPARENT` を再エクスポートします。それ以外のアイテムには `use std::screen::…` が必要です。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Render`](#render) | 型 | `const fn(Screen) -> Screen` |
| [`EntityId`](#entityid)、[`BACKGROUND`](#background) | 構造体、定数 | エンティティの識別子 |
| [`Part`](#part)、[`MAIN`](#main) | 構造体、定数 | 画像レイヤーのキー |
| [`Point`](#point)、[`Offset`](#offset)、[`Size`](#size)、[`Insets`](#insets)、[`Anchor`](#anchor)、[`Angle`](#angle) | 構造体 + コンストラクター | 幾何 |
| [`Color`](#color) と定数 | 構造体 + コンストラクター | sRGB 色 |
| [`Plane`](#plane)、[`Visibility`](#visibility)、[`Flip`](#flip)、[`VAlign`](#valign)、[`BlendMode`](#blendmode) | 列挙型 | レイアウトの選択肢 |
| [`Screen`](#screen) | ビルトイン構造体 | 画面とステップのタイムライン |
| [`Entity`](#entity) | ビルトイン構造体 | 画面上のもの |
| [`Prop`](#prop)、[`Change`](#change) | ビルトイン構造体 | 補間されるプロパティ |
| [`ImageLayer`](#imagelayer)、[`LabelLayer`](#labellayer)、[`Movie`](#movie)、[`MovieLayer`](#movielayer)、[`Swap`](#swap) | 構造体 | レイヤー |
| [`Hud`](#hud) | ビルトイン構造体 | ゲームビューのクローム |
| [`ChoiceMenu`](#choicemenu) | ビルトイン構造体 | ステップの選択肢の見せ方 |
| [`Camera`](#camera) | ビルトイン構造体 | 下位プレーンにかかるカメラ |

## 論理画面 {#the-logical-screen}

ステップの**論理画面**は、そのレンダーの後に残る永続的な状態です。

- エンティティとそのコンポーネント
- Hud、カメラ、画面フィルター
- オーディオチャンネル

スナップショット、画面の等価性、一貫性チェックは論理画面だけを使います。タイムライン、スケジュールされたアニメーション、ワンショットサウンド、`ChoiceMenu` はステップに属し、論理画面には含まれません。

アニメーションが開始状態や終了状態を変えることはありません。トゥイーンやトランジションは古い値から `change` で設定した値まで進み、モーションは恒等変換で終わります。そのためクリック、スキップ、`animations` 設定のオフが論理画面を変えることはありません。

## ステップのエントリー {#step-entry}

ステップのレンダーが実行される前に、エンジンはステップ限りの状態をリセットします。

- タイムラインのカーソルは (エントリー, 0 ms) に移ります。
- スケジュール済みのアニメーションは完了します。
- ワンショットのキューは、すでに再生中のものを除いて破棄されます。
- `ChoiceMenu` は空になり、`Middle` に設定されます。
- テキストの開始位置は (エントリー, 0) に移ります。

`render` のないステップにも同じリセットがかかります。

## 型：`Render` {#render}

```kis
pub type Render = const fn(Screen) -> Screen;
```

レンダーです。ステップ前の画面からステップ後の画面への関数です。ステップの `render =` フィールドは名前付きの `const fn` またはクロージャとして `Render` を受け取ります。

```kix
s!(render = |mut screen| {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen
});
```

出力が入力に依存しないレンダーはキーフレームです。[`Screen::empty()`](#screen-empty) を参照してください。

## 幾何 {#geometry}

位置と長さは、デザイン解像度 `System::resolution` におけるピクセルです。エンティティ ID はスタジオが定義する定数です。

### 構造体：`EntityId` {#entityid}

```kis
pub struct EntityId(pub u32);
```

エンティティの識別子です。スタジオは ID を定数として宣言します（`const HANAKO: EntityId = EntityId(1);`）。ID は同じプレーンと `z` を持つエンティティの順序も決めます。[描画順序](#drawing-order) を参照してください。

### 定数：`BACKGROUND` {#background}

```kis
pub const BACKGROUND: EntityId = EntityId(0);
```

`Screen::empty()` の背景エンティティのために予約されています。スタジオの ID は 1 から始めます。[`Screen::background()`](#screen-background) はこのエンティティのメイン画像です。

### 構造体：`Part` {#part}

```kis
pub struct Part(pub u8);
```

エンティティの画像レイヤーです。パーツは昇順に描画されるので、1 つのエンティティに体、顔、オーバーレイを重ねられます。

### 定数：`MAIN` {#main}

```kis
pub const MAIN: Part = Part(0);
```

エンティティのサイズを決めるメインパーツです。[`Entity::image()`](#entity-image) は `part(MAIN)` です。

### 構造体：`Point` {#point}

```kis
pub struct Point {
    pub x: i32,
    pub y: i32,
}
pub const fn point(x: i32, y: i32) -> Point
```

デザイン解像度のピクセルで表した点です。`point(x, y)` で作ります。種別：`const fn` · ビルトイン：いいえ。

### 構造体：`Offset` {#offset}

```kis
pub struct Offset {
    pub dx: i32,
    pub dy: i32,
}
pub const fn offset(dx: i32, dy: i32) -> Offset
```

ピクセル単位の変位です。`offset(dx, dy)` で作ります。カメラとモーションのキーで使います。種別：`const fn` · ビルトイン：いいえ。

### 構造体：`Size` {#size}

```kis
pub struct Size {
    pub w: u32,
    pub h: u32,
}
pub const fn size(w: u32, h: u32) -> Size
```

ピクセル単位のサイズです。`size(w, h)` は `w × h` です。[`Entity::set_fill_size`](#entity-set_fill_size) で使います。種別：`const fn` · ビルトイン：いいえ。

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

ボックスの 4 辺からの距離（ピクセル）です。`insets` は CSS の順序（上、右、下、左）で受け取ります。`uniform(n)` はすべての辺が `n` です。種別：`const fn` · ビルトイン：いいえ。

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

エンティティのうち位置に置かれる点を、エンティティ自身のサイズに対する割合 `0.0..=1.0` で表します。`TOP_LEFT` は左上の角（生成時の既定値）、`CENTER` は中心、`BOTTOM_CENTER` は下辺の中点（キャラクター立ち絵でよく使います）です。拡大縮小と回転はアンカーを中心に行われます。

### 構造体：`Angle` {#angle}

```kis
pub struct Angle {
    pub degrees: Decimal,
}
pub const fn degrees(d: Decimal) -> Angle
```

時計回りの回転です。`degrees(d)` は `d` 度の回転です。種別：`const fn` · ビルトイン：いいえ。

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

- `rgb` は不透明な色（アルファ 255）を返します。`rgba` はアルファを明示的に受け取ります。
- `hex(0xRRGGBB)` は不透明な色を返します。`0xFFFFFF` を超えるビットは無視されます。`hex(0x382000)` は `rgb(0x38, 0x20, 0x00)` です。
- 種別：`const fn` · ビルトイン：いいえ。

### 定数：`BLACK`、`WHITE`、`TRANSPARENT` {#color-constants}

```kis
pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
```

不透明な黒、不透明な白、完全な透明です。

## プレーン、反転、ブレンドモード {#planes-flips-and-blend-modes}

### 列挙型：`Plane` {#plane}

```kis
pub enum Plane {
    Background,
    Stage,
    Front,
    OverWindow,
}
```

描画プレーンです。テキストウィンドウはゲームビューが `Front` と `OverWindow` の間に描画します。カメラと画面フィルターは `Background`、`Stage`、`Front` にだけかかります。

### 列挙型：`Visibility` {#visibility}

```kis
pub enum Visibility {
    Shown,
    Hidden,
}
```

Hud の部品を表示するかどうかです。`Prop<Visibility>` の変更はフェードとして補間されます。

### 列挙型：`Flip` {#flip}

```kis
pub enum Flip {
    None,
    Horizontal,
    Vertical,
    Both,
}
```

エンティティの描画の鏡像反転です。[`Entity::set_flip`](#entity-set_flip) で設定します。

### 列挙型：`VAlign` {#valign}

```kis
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}
```

垂直方向の配置です。[`ChoiceMenu::set_align`](#choicemenu-set_align) で使います。

### 列挙型：`BlendMode` {#blendmode}

```kis
pub enum BlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
```

エンティティを下に描画されたものへどう合成するかです。各バリアントは CSS のブレンドモードに従い、`Add` は加算合成です。生成時の既定値は `Normal` です。

## フィルターとシェーダー {#filters-and-shaders}

> **安定性:** 暫定 — 現在唯一のランタイムプロファイルである `native_v0` はビルド時に `.wgsl` シェーダーアセットを拒否するため、フィルターとシェーダーの生成内容はまだ使えません。ネイティブレンダラーもこれらを描画しません（[ランタイムプロファイル](./appendix/profiles.md#using-an-unsupported-item) を参照）。ビルドできるのは `change(None)` だけで、論理画面に残りますが見た目には影響しません。

- フィルター、天候、パーティクルはすべて `ShaderAsset`、つまり WGSL シェーダーファイル（`.wgsl`）です。
- **フィルター**は描画されたものを後処理します。エンティティは自分のフィルター（[`Entity::filter`](#entity-filter)）を持ち、画面フィルター（[`Screen::filter`](#screen-filter)）は `Background`、`Stage`、`Front` プレーンにかかります。`None` はフィルターなしです。
- 天候とパーティクルは **`shader`**（[`Entity::shader`](#entity-shader)）を設定したエンティティです。シェーダーがエンティティの内容を生成し、その内容はほかのエンティティと同じく、エンティティのプレーンに描画され `z` で順序付けられます。
- `std` にはシェーダーが同梱されていません。スタジオが自分で用意します。

```kis
const SEPIA: ShaderAsset = asset("sepia.wgsl");

screen.filter().change(Some(SEPIA)).apply_animation(tween(ms(500)));
```

## 構造体：`Screen` {#screen}

```kis
#[builtin] pub struct Screen;
```

ステップが表示する画面と、そのステップのタイムラインです。`Screen` は不透明なビルトインハンドルで、レンダーは以下のメソッドを通してのみその状態に触れます。画面は `==` で比較できます（`Eq` を実装しています）。等価性は[論理画面](#the-logical-screen)だけを使います。

### `Screen::empty()` {#screen-empty}

```kis
#[builtin] pub const fn empty() -> Screen;
```

- 戻り値：不透明な `BACKGROUND` エンティティ（プレーン `Background`、不透明度 1.0、画像なし）だけを持つ画面。
- Hud はテキストウィンドウとクイックメニューが `Shown` で、サイド画像はありません。
- カメラはオフセット 0、ズーム 1.0、回転 0 です。
- 画面フィルターは `None` です。
- すべてのオーディオチャンネルは無音で、音量は 1.0 です。
- 種別：`const fn` · ビルトイン：はい。

出力が入力に依存しないレンダーはキーフレームです。`Screen::empty()` はキーフレームを書く通常の方法です。

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

- `id` `EntityId` — エンティティ。
- 戻り値：`&mut Entity` — そのエンティティ。存在しなければ既定のコンポーネントで生成されます。
- 種別：`const fn` · ビルトイン：はい。

#### 生成 {#spawning}

`entity(id)` はエンティティが存在しなければ、次の既定値で生成します。

| コンポーネント | 既定値 |
| --------- | ------- |
| plane, z | `Stage`, 0 |
| position, anchor | (0, 0), `TOP_LEFT` |
| scale, rotation | 1.0, 0° |
| **opacity** | **0.0** |
| filter, shader | `None`, `None` |
| fill, fill size | `TRANSPARENT`, `None`（画面全体） |
| layers | パーツなし、ラベルなし、ムービーなし |
| flip, blend, gate | `Flip::None`, `Normal`, `None` |

生成されたエンティティは見えないので、登場のたびにアニメーションを明示します。

```kis
screen.entity(HANAKO).image().change(HANAKO::SMILE).apply_animation(cut());
screen.entity(HANAKO).opacity().change(1.0).apply_animation(tween(ms(300)));
```

削除が保留中の ID に対する `entity(id)` は削除を取り消します。エンティティは現在の値を保ちます。

### `screen.contains(id)` {#screen-contains}

```kis
#[builtin] pub const fn contains(&self, id: EntityId) -> bool;
```

- 戻り値：`bool` — エンティティ `id` が論理画面上にあるかどうか。削除されてフェードアウト中のエンティティは含まれません。
- 種別：`const fn` · ビルトイン：はい。

### `screen.entities(plane)` {#screen-entities}

```kis
#[builtin] pub const fn entities(&self, plane: Plane) -> Vec<EntityId>;
```

- 戻り値：`Vec<EntityId>` — 論理画面上で `plane` にある ID の昇順リスト。
- 種別：`const fn` · ビルトイン：はい。

### `screen.despawn(id)` {#screen-despawn}

```kis
#[builtin] pub const fn despawn(&mut self, id: EntityId);
```

エンティティ `id` を論理画面から即座に取り除きます。スケジュール済みのアニメーションが終わるまでは描画され続け、その後に消えます。そのためフェードアウトの後に `despawn` を書くとフェードアウトが見え、次のステップはそのエンティティを引き継ぎません。種別：`const fn` · ビルトイン：はい。

```kis
screen.entity(CARD).opacity().change(0.0).apply_animation(tween(ms(500)));
screen.despawn(CARD);
```

### `screen.background()` {#screen-background}

```kis
pub const fn background(&mut self) -> &mut ImageLayer {
    self.entity(BACKGROUND).image()
}
```

`BACKGROUND` エンティティのメイン画像です。種別：`const fn` · ビルトイン：いいえ。

```kis
screen.background().change(BG::ROOF).apply_animation(fade(ms(500)));
```

### `screen.hud()` {#screen-hud}

```kis
#[builtin] pub const fn hud(&mut self) -> &mut Hud;
```

ゲームビューが描画する [Hud](#hud) です。種別：`const fn` · ビルトイン：はい。

### `screen.choice_menu()` {#screen-choice_menu}

```kis
#[builtin] pub const fn choice_menu(&mut self) -> &mut ChoiceMenu;
```

このステップの[選択肢メニュー](#choicemenu)です。種別：`const fn` · ビルトイン：はい。

### `screen.camera()` {#screen-camera}

```kis
#[builtin] pub const fn camera(&mut self) -> &mut Camera;
```

`Background`、`Stage`、`Front` プレーンにかかる[カメラ](#camera)です。種別：`const fn` · ビルトイン：はい。

### `screen.filter()` {#screen-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<ShaderAsset>>;
```

`Background`、`Stage`、`Front` プレーンを後処理するシェーダーです。`None` はフィルターなしです。[フィルターとシェーダー](#filters-and-shaders) を参照してください。種別：`const fn` · ビルトイン：はい · プロファイル：`native_v0` はビルド時にシェーダーアセットを拒否し、ネイティブレンダラーはフィルターを描画しません。

### `screen.music()`、`screen.ambience(channel)` {#screen-music}

```kis
#[builtin] pub const fn music(&mut self) -> &mut audio::Channel;
#[builtin] pub const fn ambience(&mut self, channel: Int<0, 3>) -> &mut audio::Channel;
```

音楽チャンネルと、4 つの環境音チャンネル `ambience(0)` 〜 `ambience(3)` です。[チャンネル](./screen-audio.md#channels) を参照してください。種別：`const fn` · ビルトイン：はい。

### `screen.play(sound)`、`screen.stop_sounds(fade_out)` {#screen-play}

```kis
#[builtin] pub const fn play(&mut self, sound: audio::Sound);
#[builtin] pub const fn stop_sounds(&mut self, fade_out: Duration);
```

`play` はワンショットサウンドをカーソル位置にその遅延を足した時刻に再生します。`stop_sounds` はカーソル位置で再生中のすべてのワンショットサウンドを `fade_out` かけてフェードアウトします。[`Sound`](./screen-audio.md#sound) を参照してください。種別：`const fn` · ビルトイン：はい。

### `screen.replay_bgm()` {#screen-replay_bgm}

```kis
#[builtin] pub const fn replay_bgm(&mut self);
```

音楽チャンネルのトラックを、カーソル位置で先頭から再開します。これがなければ、すでに再生中のトラックは再開されずに後のステップへ引き継がれます。[同一トラック規則](./screen-audio.md#the-same-track-rule) を参照してください。種別：`const fn` · ビルトイン：はい。

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
| `barrier()` | カーソルを、これまでにスケジュールしたすべての終わりへ移す |
| `at_mark(m)` | カーソルを、テキストが `m` に達する時刻へ移す |
| `at_entry()` | カーソルをステップのエントリーへ戻す |
| `begin_text()` | カーソル位置でテキスト表示とボイスを開始する |

カーソルのモデルは [タイムライン](./screen-anim.md#the-timeline) で説明します。種別：`const fn` · ビルトイン：はい。

## 構造体：`Entity` {#entity}

```kis
#[builtin] pub struct Entity;
```

画面上のものです。変換、不透明度、フィルター、シェーダー、塗り、レイヤーを持ちます。コンポーネントの集合は固定で、スタジオがコンポーネントを追加することはありません。

| メンバーの種類 | メンバー | 変更方法 |
| -------------- | ------- | ------------ |
| 補間されるプロパティ | `position`, `scale`, `rotation`, `opacity`, `filter`, `shader`, `fill` | `change(v).apply_animation(Tween)` |
| レイヤー | `part(p)`, `image()`, `label()`, `movie()` | `change(x).apply_animation(Transition)` |
| レイアウト設定 | `set_plane`, `set_z`, `set_anchor`, `set_flip`, `set_blend`, `set_fill_size`, `gate` | 直接設定。補間不可 |
| 相対モーション | `apply_animation(Motion)` | [`Motion`](./screen-anim.md#motion) を参照 |

### 補間されるプロパティ {#entity-properties}

```kis
#[builtin] pub const fn position(&mut self) -> &mut Prop<Point>;
#[builtin] pub const fn scale(&mut self) -> &mut Prop<Decimal>;
#[builtin] pub const fn rotation(&mut self) -> &mut Prop<Angle>;
#[builtin] pub const fn opacity(&mut self) -> &mut Prop<Decimal>;
```

- `position` はアンカー点が置かれる位置です。
- `scale` はアンカーを中心とした倍率、`rotation` はアンカーを中心とした時計回りの回転です。
- `opacity` は割合 `0.0..=1.0` です。生成されたエンティティは 0.0 から始まります。
- 種別：`const fn` · ビルトイン：はい。

<a id="entity-shader"></a>

### `entity.filter()`、`entity.shader()` {#entity-filter}

```kis
#[builtin] pub const fn filter(&mut self) -> &mut Prop<Option<ShaderAsset>>;
#[builtin] pub const fn shader(&mut self) -> &mut Prop<Option<ShaderAsset>>;
```

`filter` はこのエンティティを後処理するシェーダーで、`None` はフィルターなしです。`shader` はシェーダーが生成する内容（雨、雪、パーティクル）で、エンティティのプレーンと z 順序で描画されます。`None` は何も描画しません。[フィルターとシェーダー](#filters-and-shaders) を参照してください。種別：`const fn` · ビルトイン：はい · プロファイル：`native_v0` はビルド時にシェーダーアセットを拒否し、ネイティブレンダラーはフィルターもシェーダーの生成内容も描画しません。

### `entity.fill()` {#entity-fill}

```kis
#[builtin] pub const fn fill(&mut self) -> &mut Prop<Color>;
```

パーツの下に、塗りサイズの範囲で描かれる色です。既定の塗りサイズ（`None`、画面全体）では、塗りだけを持つエンティティは全画面の色板になります。種別：`const fn` · ビルトイン：はい。

### `entity.part(part)`、`entity.image()` {#entity-image}

```kis
#[builtin] pub const fn part(&mut self, part: Part) -> &mut ImageLayer;
pub const fn image(&mut self) -> &mut ImageLayer {
    self.part(MAIN)
}
```

画像レイヤー `part` です。`image()` は `MAIN` レイヤーです。パーツは `Part` の昇順に描画されます。種別：`const fn` · ビルトイン：`part` ははい、`image` はいいえ。

> **安定性:** 予定 — レイヤー構成のキャラクター立ち絵での自動まばたきと口パク（自律的に動くパーツや、ボイスに連動する口パク）はまだ設計されていません。

### `entity.label()` {#entity-label}

```kis
#[builtin] pub const fn label(&mut self) -> &mut LabelLayer;
```

[テキストレイヤー](#labellayer)です。パーツとムービーの上に描画されます。種別：`const fn` · ビルトイン：はい。

### `entity.movie()` {#entity-movie}

```kis
#[builtin] pub const fn movie(&mut self) -> &mut MovieLayer;
```

[ムービーレイヤー](#movielayer)です。パーツの上に描画されます。種別：`const fn` · ビルトイン：はい · プロファイル：`native_v0` はビルド時に動画アセットを拒否するため、ビルドできるのは `clear()` だけです。ネイティブレンダラーはムービーを描画しません。

<a id="entity-set_flip"></a><a id="entity-set_fill_size"></a>

### レイアウト設定 {#entity-setters}

```kis
#[builtin] pub const fn set_plane(&mut self, plane: Plane);
#[builtin] pub const fn set_z(&mut self, z: i32);
#[builtin] pub const fn set_anchor(&mut self, anchor: Anchor);
#[builtin] pub const fn set_flip(&mut self, flip: Flip);
#[builtin] pub const fn set_blend(&mut self, blend: BlendMode);
#[builtin] pub const fn set_fill_size(&mut self, size: Option<Size>);
```

| 設定 | 効果 |
| ------ | ------ |
| `set_plane(p)` | エンティティをプレーン `p` へ移す |
| `set_z(z)` | プレーン内の順序を設定する。大きいほど後に描画される |
| `set_anchor(a)` | エンティティのうち位置に置かれる点を設定する |
| `set_flip(f)` | 描画を鏡像反転する |
| `set_blend(b)` | 下にあるものへの合成方法を設定する |
| `set_fill_size(s)` | 塗りのサイズを設定する。`None` は画面全体 |

設定は即座に反映され、アニメーションしません。種別：`const fn` · ビルトイン：はい。

### `entity.gate(setting)` {#gate}

```kis
#[builtin] pub const fn gate(&mut self, setting: Option<Setting<bool>>);
```

`gate(Some(s))` は設定 `s` が true の間だけエンティティを描画します。`gate(None)` は常に描画します。種別：`const fn` · ビルトイン：はい。

- ハンドルは論理画面の一部で、値ではなく同一性で比較されます。レンダーは設定を読めないので、論理画面は設定に依存しません。
- エンジンは描画時に値を読みます（遅延参照）。

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

開始位置で終わる相対[モーション](./screen-anim.md#motion)を再生します。エンティティの論理状態は変わりません。種別：`const fn` · ビルトイン：はい。

### 描画順序 {#drawing-order}

- プレーンは `Background < Stage < Front <（ゲームビューが描くテキストウィンドウ）< OverWindow` の順に描画されます。プレーン内では `z` の昇順、次に `EntityId` の昇順です。`z` は CSS の `z-index` に相当します。シェーダーが生成する内容（天候、パーティクル）も同じ規則で順序付けられます。
- エンティティ内の順序は、塗り、`Part` 昇順のパーツ、ムービー、ラベルです。
- アンカー点は `position` に置かれます。エンティティのサイズは `MAIN` パーツの画像のサイズ、なければ塗りサイズ、なければラベルのボックスです。拡大縮小と回転はアンカーを中心に行われます。
- カメラは `Background`、`Stage`、`Front` にだけかかります。オフセットは平行移動し、ズームと回転は画面中心を軸にします。
- 画面フィルターも同じ 3 つのプレーンにかかります。

## 構造体：`Prop` {#prop}

```kis
#[builtin] pub struct Prop<T>;
impl<T> Prop<T> {
    #[builtin] pub const fn get(&self) -> T;
    #[builtin] pub const fn change(&mut self, value: T) -> &mut Change<T>;
}
```

補間されるプロパティです。`get()` は論理値を返します。`change(v)` は論理値を即座に設定し、保留中の [`Change`](#change) を返します。変更はトゥイーンを与えたときに起こります。種別：`const fn` · ビルトイン：はい。

`prop.change(v).apply_animation(t)` は次のように動作します。

- 論理値はすぐに `v` になるので、同じレンダー内の後続の `get` は `v` を返します。
- エンジンは、`cursor + t.delay` 時点でのプロパティの値から `v` まで、`t.duration` かけて `t.easing` で補間します。
- 同じプロパティの以前のアニメーションがまだ実行中のうちに後のアニメーションが始まると、以前のものは終点まで切り上げられます。
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

保留中のプロパティ変更です。`apply_animation` を通してのみ起こり、カーソル位置に遅延を足した時点の値から新しい値まで補間します。`Change<T>` は `#[must_use]` です。`apply_animation` のない変更は起こらず、その文は警告になります。即座に反映させたい変更は [`instant()`](./screen-anim.md#tween) でそう書きます。種別：`const fn` · ビルトイン：はい。

## レイヤーと `Swap` {#layers-and-swap}

レイヤーの差し替えはプロパティの変更と同じように動作し、`Tween` の代わりに [`Transition`](./screen-anim.md#transition) を使います。論理的な内容はすぐに変わり、トランジションが `cursor + delay` で古い内容から新しい内容へ進みます。`clear()` は何もない状態へ差し替えます。

### 構造体：`ImageLayer` {#imagelayer}

```kis
#[builtin] pub struct ImageLayer;
impl ImageLayer {
    #[builtin] pub const fn get(&self) -> Option<ImageAsset>;
    #[builtin] pub const fn change(&mut self, image: ImageAsset) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
}
```

エンティティの画像レイヤーです。`get()` は表示中の画像（あれば）です。`change(image)` は画像を `image` に差し替え、`clear()` は何もない状態に差し替えます。種別：`const fn` · ビルトイン：はい。

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

エンティティのテキストレイヤーです。独自の [`TextStyle`](./narrative-text.md) で `Text` を表示します。`change(text)` と `clear()` はテキストを差し替え、`set_style` はスタイルを設定します。`set_width(Some(w))` は `w` ピクセルで折り返し、`None` は折り返しません。種別：`const fn` · ビルトイン：はい。

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

ムービーと、ループするかどうかです。`movie(v)` は `v` を 1 回再生し、`movie(v).looping()` はループ再生します。種別：`const fn` · ビルトイン：いいえ · プロファイル：`native_v0` はビルド時に動画アセットを拒否するため、ムービーと `std::system::watched` は使えません。ネイティブレンダラーはムービーを描画せず、視聴済みムービーの集合も埋めません。

### 構造体：`MovieLayer` {#movielayer}

```kis
#[builtin] pub struct MovieLayer;
impl MovieLayer {
    #[builtin] pub const fn change(&mut self, movie: Movie) -> &mut Swap;
    #[builtin] pub const fn clear(&mut self) -> &mut Swap;
}
```

エンティティのムービーレイヤーです。`change(movie)` はムービーを `movie` に差し替え、`clear()` は何もない状態に差し替えます。種別：`const fn` · ビルトイン：はい。

### 構造体：`Swap` {#swap}

```kis
#[must_use] #[builtin] pub struct Swap;
impl Swap {
    #[builtin] pub const fn apply_animation(&mut self, transition: anim::Transition);
}
```

保留中のレイヤー差し替えです。`apply_animation` を通してのみ起こり、`transition` で古い内容から新しい内容へ差し替えます。`Swap` は `#[must_use]` です。種別：`const fn` · ビルトイン：はい。

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

物語がゲームビューのクロームに求めるものです。テキストウィンドウとクイックメニューを表示するかどうか、テキストウィンドウ横のサイド画像を表します。Hud は論理画面の一部です。ゲームビューは `Narrative::hud()` で読み取り、物語が読むことはありません。種別：`const fn` · ビルトイン：はい。

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

ステップが表示する選択肢の見せ方です。種別：`const fn` · ビルトイン：はい。

- 選択肢メニューはステップ限りです。ステップのエントリーごとに空になって `Middle` に設定され、論理画面には含まれません。
- `disabled_arm(at, text)` は表示専用のアームを、表示リストの位置 `at`（範囲内に丸められます）に無効状態で追加します。値を持たず、選択できず、`choice!` の一部でもないため、どの解析にも見えません。どのレンダーで追加するかは物語のコードが選びます。[`std::narrative`](./narrative.md#disabled-arms) を参照してください。
- `set_align(align)` はメニューの垂直位置を決めます。

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

`Background`、`Stage`、`Front` プレーンにかかるカメラです。そのためテキストウィンドウと `OverWindow` は動きません。`offset` は平行移動し、`zoom` と `rotation` は画面中心を軸にします。`apply_animation` は開始位置で終わる相対モーションを再生し、シーン全体を揺らす通常の方法です。種別：`const fn` · ビルトイン：はい。

```kis
screen.camera().apply_animation(quake(8, 4, ms(600)));
```
