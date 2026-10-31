---
title: "std::ui"
---

> **安定性:** 安定 — UI の値、リアクティビティ、UI フォームの脱糖先となる関数。

`std::ui` はゲームシステムの UI ツールキットです。要素型、リアクティブなシグナル、コンテンツとレイアウトの関数、そして UI フォーム（`column![…]`、`button!{…}`、`text!(…)` など）の脱糖先を提供します。コントロールの見た目は [`std::ui::style`](./ui-style.md) で宣言します。

UI は `fn` のコードです。**コンポーネント** は [`Element`](#element) を返す `fn` で、その要素をモード型（[`RootPage`、`Overlay`、`GameOverlay`、`Dialog`](./system.md#mode-types)）で包むか、ゲームビューならそのまま `Element` を返します。スタイルは `const fn` のデータです。

プレリュードは `signal` と `Element` をインポートします。その他のアイテムは `use std::ui::{Fit, each, show, thumbnail};` のようにパスでインポートしてください。

```kis
pub mod style;
```

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Element`](#element)、[`Placed`](#placed) | ビルトイン構造体 | UI 要素、レイヤーに配置された要素 |
| [`Align`](#align)、[`Fit`](#fit) | 列挙型 | フレックスの整列、画像のボックスへの収め方 |
| [`signal`](#signal)、[`effect`](#effect)、[`each`](#each)、[`show`](#show) | `fn` | リアクティビティ |
| [`label`](#label)、[`spacer`](#spacer)、[`scroll`](#scroll)、[`thumbnail`](#thumbnail)、[`message`](#message) | `fn` | コンテンツ |
| [`Element` のメソッド](#layout) | `fn` | サイズ、余白、整列、塗り、フォーカス順、配置 |
| [`column`](#column)、[`row`](#row)、[`layer`](#layer) | `fn` | レイアウトフォームの脱糖先 |
| [`text`](#text)、[`image`](#image)、[`game_screen`](#game_screen) | `fn` | コンテンツフォームの脱糖先 |
| [`button`](#button)、[`slider`](#slider)、[`toggle`](#toggle)、[`select`](#select)、[`option`](#option) | `fn` | コントロールフォームの脱糖先 |

## 要素 {#elements}

### 構造体: `Element` {#element}

```kis
#[builtin] pub struct Element;
```

エンジンがレイアウトする UI 要素のツリーです。`Element` は不透明な値です。このモジュールの関数とフォームで作り、[レイアウトのメソッド](#layout) で調整します。

### 構造体: `Placed` {#placed}

```kis
#[builtin] pub struct Placed;
```

[`layer![…]`](#layer) に配置された要素です。[`Element::at`](#element-at) または [`Element::cover`](#element-cover) で作ります。

### 列挙型: `Align` {#align}

```kis
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}
```

行または列の子のフレックス整列で、[`Element::align`](#element-align) で使います。

| バリアント | 行・列に沿った方向（`main`） | 交差する方向（`cross`） |
| ---------- | ---------------------------- | ----------------------- |
| `Start` | 先頭に詰める | 先頭に置く |
| `Center` | 中央に寄せる | 中央に置く |
| `End` | 末尾に詰める | 末尾に置く |
| `Stretch` | `Start` と同じ | 固定サイズでない子を交差方向いっぱいに引き伸ばす |
| `SpaceBetween` | 空き領域を子の間に等しく分ける（子が 1 つなら `Start` と同じ） | `Start` と同じ |

### 列挙型: `Fit` {#fit}

```kis
pub enum Fit {
    Stretch,
    Contain,
    Cover,
    Natural,
}
```

画像がボックスをどう埋めるかで、[`image!`](#image) で使います。

| バリアント | 意味 |
| ---------- | ---- |
| `Stretch` | 縦横比を無視してボックスを埋めます |
| `Contain` | 縦横比を保ち、ボックスに収まる最大のサイズにして、ボックスの中央に置きます |
| `Cover` | 縦横比を保ってボックスを埋めます。はみ出す方向の両側が均等に切り取られます |
| `Natural` | 画像本来のピクセルサイズで、ボックスの左上に置きます |

## コンポーネントとリアクティビティ {#components-and-reactivity}

コンポーネントとリアクティビティは [SolidJS](https://www.solidjs.com/) に倣っています。

- コンポーネントは、そのモードが表示されるときに **一度だけ** 実行されます。
- **リアクティブなソース** は、シグナルのゲッター、`Setting::get`、`Meta::get`、`Narrative` ハンドルの読み取り、[`saves::info`](./system-saves.md#info)、[既見セット](./system.md#seen-sets) です。
- **リアクティブなクロージャ** は、読み取ったソースが変わると再実行されます。リアクティブな属性と UI フォームの要素はこのようなクロージャです（[UI フォーム](#ui-forms) を参照）。そのため `text!(t!["Music: " {music().level}])` は `music` が変わるたびに更新されますが、コンポーネントの残りの部分は再実行されません。
- `on_click` などのハンドラは普通のクロージャで、イベントが起きたときに実行されます。

シグナルは `fn` のコードにしか存在しません。物語と `const fn` はシグナルを使えません。

### 関数: `signal(init)` {#signal}

```kis
#[builtin] pub fn signal<T>(init: T) -> (Fn() -> T, Fn(T));
```

- `init` `T` — 初期値。
- 戻り値: `(Fn() -> T, Fn(T))` — ゲッターとセッター。ゲッターはリアクティブなソースです。
- 種別: `fn` · ビルトイン: はい

```kix
let (hidden, set_hidden) = signal(false);
```

### 関数: `effect(run)` {#effect}

```kis
#[builtin] pub fn effect(run: Fn());
```

- `run` `Fn()` — 実行するクロージャ。
- 種別: `fn` · ビルトイン: はい

`run` を今すぐ実行し、読み取ったソースが変わるたびに再実行します。

### 関数: `each(items, item)` {#each}

```kis
#[builtin] pub fn each<T>(items: Fn() -> Vec<T>, item: Fn(T) -> Element) -> Element;
```

- `items` `Fn() -> Vec<T>` — 項目。リアクティブです。
- `item` `Fn(T) -> Element` — 1 つの項目の要素を作ります。
- 戻り値: `Element` — 項目ごとに 1 つの要素。`items` と同期し続けます。
- 種別: `fn` · ビルトイン: はい

`column![…]` や `row![…]` の中では、項目の要素がその列や行の子になり、`each` 自身のレイアウトメソッドは無視されます。それ以外の場所では、項目は列としてレイアウトされます。[行と列の中の `each` と `show`](#flattening) を参照してください。

```kix
scroll(|| each(|| game.history(), |entry| entry_view(entry)))
```

### 関数: `show(when, then)` {#show}

```kis
#[builtin] pub fn show(when: Fn() -> bool, then: Fn() -> Element) -> Element;
```

- `when` `Fn() -> bool` — 条件。リアクティブです。
- `then` `Fn() -> Element` — 内容を作ります。
- 戻り値: `Element` — `when()` が `true` の間は `then()`、そうでなければ何もなし。
- 種別: `fn` · ビルトイン: はい

[`each`](#each) と同じく、親が `column![…]` か `row![…]` なら平坦化され、それ以外の場所では列としてレイアウトされます。[行と列の中の `each` と `show`](#flattening) を参照してください。

## コンテンツ {#content}

### 関数: `label(text)` {#label}

```kis
#[builtin] pub fn label(text: Text) -> Element;
```

- `text` `Text` — テキスト。
- 戻り値: `Element` — 静的なテキスト。
- 種別: `fn` · ビルトイン: はい

テキストには [`System::text`](./system.md#system) が使われます。ボタンやトグルの中では現在の見た目のテキストスタイルが使われます（[インタラクションの見た目](./ui-style.md#interaction-looks) を参照）。`text!(…)` は、スタイルを任意で指定できるリアクティブな形です。`column!`、`row!`、`button!`、`toggle!` の中では、裸の `t![…]` 要素は `label(t![…])` になります。

### 関数: `spacer()` {#spacer}

```kis
#[builtin] pub fn spacer() -> Element;
```

- 戻り値: `Element` — 伸びて兄弟要素を押し広げる空の要素。
- 種別: `fn` · ビルトイン: はい

### 関数: `scroll(child)` {#scroll}

```kis
#[builtin] pub fn scroll(child: Fn() -> Element) -> Element;
```

- `child` `Fn() -> Element` — 内容。
- 戻り値: `Element` — 内容を囲むスクロール可能なボックス。
- 種別: `fn` · ビルトイン: はい

### 関数: `thumbnail(slot)` {#thumbnail}

```kis
#[builtin] pub fn thumbnail(slot: SaveSlot) -> Element;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — スロット。
- 戻り値: `Element` — `slot` のセーブと一緒に保存されたスクリーンショット。
- 種別: `fn` · ビルトイン: はい

### 関数: `message(game, style)` {#message}

```kis
#[builtin] pub fn message(game: Narrative, style: TextStyle) -> Element;
```

- `game` `Narrative` — 実行中のゲーム。
- `style` `TextStyle` — ウィンドウのテキストスタイル（[`std::narrative::text`](./narrative-text.md) を参照）。
- 戻り値: `Element` — 現在の行。文字送り、テキスト効果、ポーズ、マーク、クリック待ちグリフを含みます。
- 種別: `fn` · ビルトイン: はい

`message` はメッセージウィンドウの本体です。`Narrative` ハンドルが存在する場所、つまりゲームビューとゲームオーバーレイでだけ使えます。

> **安定性:** 予定 — NVL モード（改ページまで複数の行が画面に残る全画面テキストモード）はまだ設計されていません。テキストウィンドウの表示内容と、バックログや `Narrative::line` がステップを表す方法が変わる予定です。

## レイアウト {#layout}

```kis
impl Element {
    #[builtin] pub fn width(self, px: u32) -> Element;
    #[builtin] pub fn height(self, px: u32) -> Element;
    #[builtin] pub fn padding(self, insets: Insets) -> Element;
    #[builtin] pub fn gap(self, px: u32) -> Element;
    #[builtin] pub fn align(self, main: Align, cross: Align) -> Element;
    #[builtin] pub fn grow(self, factor: u32) -> Element;
    #[builtin] pub fn background(self, paint: Paint) -> Element;
    #[builtin] pub fn opacity(self, opacity: Decimal) -> Element;
    #[builtin] pub fn focus_order(self, order: u32) -> Element;
    #[builtin] pub fn at(self, position: Point, anchor: Anchor) -> Placed;
    #[builtin] pub fn cover(self) -> Placed;
}
```

レイアウトはフレックスボックスに似ています。

- [`column![…]`](#column) と [`row![…]`](#row) はフレックスコンテナです。`gap` と `align` が子の間隔と整列を決め、`grow` は子のフレックス伸長係数です。子の [`each` と `show`](#flattening) は平坦化されます。
- [`layer![…]`](#layer) は [`Placed`](#placed) の子を重ねます。`at` は子のアンカーを点に置き、`cover` はレイヤー全体を埋めます。
- サイズと位置はデザイン解像度（[`System::resolution`](./system.md#system)）のピクセルです。
- レイアウトは要素ツリーに対するエンジンの処理です。リサイズ時に再計算され、セーブされることはありません。

すべてのメソッドは要素を値で受け取り、調整した要素を返すので、`column![…].gap(16).align(Align::Center, Align::Center)` のように連鎖できます。いずれもビルトインの `fn` です。

### 行と列の中の `each` と `show` {#flattening}

[`each`](#each) と [`show`](#show) のレイアウトは親によって変わります。

- **`column![…]` か `row![…]` の子: 平坦化されます。** `each` の要素（`show` なら `then()`）は、兄弟の中で `each` があった位置に、その列や行の子として入ります。間隔は列や行の `gap`、整列はその `align` で決まり、要素自身の `grow` が効きます。平坦化された `each` や `show` の中の `each` や `show` も平坦化されるので、`column![show(…, || each(…))]` は項目を直接列に入れます。平坦化された `each` や `show` は自身の箱を持たず、それに対して呼んだ `width`、`height`、`padding`、`gap`、`align`、`grow`、`background`、`opacity` は無視されます。
- **それ以外の場所: 列になります。** `layer![…]` に配置したとき、`scroll(…)`、`button!`、`toggle!` の内容としたとき、平坦化されていない `each` や `show` の中、モードのルート要素としたときは、`each` や `show` は要素を並べた列としてレイアウトされ、自身のメソッドが効きます。

```kix
// 40 px between all the children, items included: the gap belongs to the column.
column![header(), each(|| items(), |i| tile(i))].gap(40)
// 40 px between the items, 8 px between the other children: wrap the each in a column.
column![header(), column![each(|| items(), |i| tile(i))].gap(40), footer()].gap(8)
// 40 px between the items: in a layer, the each is a column with its own gap.
layer![each(|| items(), |i| tile(i)).gap(40).at(point(100, 100), TOP_LEFT)]
```

### `Element::width(px)` {#element-width}

```kis
#[builtin] pub fn width(self, px: u32) -> Element;
```

- `px` `u32` — ピクセル単位の固定幅。

### `Element::height(px)` {#element-height}

```kis
#[builtin] pub fn height(self, px: u32) -> Element;
```

- `px` `u32` — ピクセル単位の固定高さ。

### `Element::padding(insets)` {#element-padding}

```kis
#[builtin] pub fn padding(self, insets: Insets) -> Element;
```

- `insets` `Insets` — 内側の余白。`std::screen::insets(top, right, bottom, left)` で作ります（[`std::screen`](./screen.md) を参照）。

### `Element::gap(px)` {#element-gap}

```kis
#[builtin] pub fn gap(self, px: u32) -> Element;
```

- `px` `u32` — 行または列の子の間隔。平坦化された `each` や `show` では無視されます（[行と列の中の `each` と `show`](#flattening) を参照）。

### `Element::align(main, cross)` {#element-align}

```kis
#[builtin] pub fn align(self, main: Align, cross: Align) -> Element;
```

- `main` `Align` — 行または列に沿った子の整列。
- `cross` `Align` — それと交差する方向の整列。

### `Element::grow(factor)` {#element-grow}

```kis
#[builtin] pub fn grow(self, factor: u32) -> Element;
```

- `factor` `u32` — フレックス伸長係数。行または列の空き領域のうち、この要素が取る割合です。

### `Element::background(paint)` {#element-background}

```kis
#[builtin] pub fn background(self, paint: Paint) -> Element;
```

- `paint` [`Paint`](./ui-style.md#paint) — 要素の背後に描くもの。

### `Element::opacity(opacity)` {#element-opacity}

```kis
#[builtin] pub fn opacity(self, opacity: Decimal) -> Element;
```

- `opacity` `Decimal` — 要素の不透明度。`0.0..=1.0`。

### `Element::focus_order(order)` {#element-focus_order}

```kis
#[builtin] pub fn focus_order(self, order: u32) -> Element;
```

- `order` `u32` — フォーカス順における要素の位置（[フォーカス](#focus) を参照）。

### `Element::at(position, anchor)` {#element-at}

```kis
#[builtin] pub fn at(self, position: Point, anchor: Anchor) -> Placed;
```

- `position` `Point` — レイヤー上の点。
- `anchor` `Anchor` — そこに置く要素側の点。`TOP_LEFT` や `CENTER` など（[`std::screen`](./screen.md) を参照）。
- 戻り値: `Placed` — `layer![…]` 用。

### `Element::cover()` {#element-cover}

```kis
#[builtin] pub fn cover(self) -> Placed;
```

- 戻り値: `Placed` — レイヤー全体を埋める要素。

```kix
use std::screen::CENTER;
use std::ui::Fit;

fn Title() -> RootPage {
    RootPage(layer![
        image!(fit = Fit::Cover, TITLE_BG).cover(),
        column![
            button!{on_click = || nav::start(story::Main, (), |_end| nav::goto(Title)), t!["Start"]},
            button!{on_click = || nav::call(Config), t!["Config"]},
            button!{on_click = || nav::quit(), t!["Quit"]},
        ].gap(16).at(point(640, 520), CENTER),
    ])
}
```

## フォーカス {#focus}

- フォーカス可能な要素は、ボタン、スライダー、トグル、セレクトです。
- `focus_order`（[`focus_order`](#element-focus_order)）が設定されていなければ、フォーカスは文書順に従います。
- モードが開くと、最初のフォーカス可能な要素にフォーカスが当たります。フォーカスは最初のフォーカスキーから表示され、Enter と `PadButton::South` がフォーカス中のコントロールを実行するのは、フォーカスが表示されている間だけです。
- 上下左右キー、十字キー、Tab と Shift+Tab はフォーカスを移動します。フォーカス中のスライダーとセレクトでは、左右キーは代わりに値を変えます。スライダーは `step` ずつ、セレクトは前後の選択肢に切り替わります。
- これらのキーは組み込みで、バインドできません（[バインディング](./system.md#bindings) を参照）。

## UI フォーム {#ui-forms}

UI フォームは `.kix` ファイルに書き、props 構造体を伴う以下の関数の呼び出しに脱糖されます。`fn` と `const fn` のコードでは値を作るもので、ステップになることはありません。言語の区切り文字の規則に従います（リファレンスの [フォーム](../reference/syntax/forms.md) の章を参照）。

- `name![…]` は要素だけを取ります。
- `name!(…)` は属性と、決まった数の要素を取ります。
- `name!{…}` は属性と、任意の数の要素を取ります。

### 脱糖 {#desugaring}

- **リアクティブな属性** は `key: Some(|| expr)` になります。必須の属性なら `key: || expr` です。
- **ハンドラ属性**（`on_*`）は `key: Some(expr)` になります。
- **省略された任意の属性** は `None` になります。必須の属性の欠落、未知の属性、要素の数の誤り（`text!` と `image!` はちょうど 1 つ、`game_screen!` と `slider!` は 0 個）は構文エラーです。
- **要素。** 各要素 `e` は `|| e` になります。`column!`、`row!`、`button!`、`toggle!` の中では、構文上 `t![…]` である要素は `|| ui::label(t![…])` になります。

| フォーム | 脱糖先 | 属性 | 要素 |
| -------- | ------ | ---- | ---- |
| `column![…]`、`row![…]` | `ui::column([…])`、`ui::row([…])` | なし | `Element` |
| `layer![…]` | `ui::layer([…])` | なし | `Placed` |
| `text!(…)` | `ui::text(TextProps { … })` | `style`: リアクティブな `TextStyle` | ちょうど 1 つの `Text` → `content` |
| `image!(…)` | `ui::image(ImageProps { … })` | `fit`: リアクティブな `Fit` | ちょうど 1 つの `ImageAsset` → `source` |
| `game_screen!(…)` | `ui::game_screen(GameScreenProps { … })` | `game`: リアクティブな `Narrative`、必須 | なし |
| `button!{…}` | `ui::button(ButtonProps { … })` | `on_click`: ハンドラ `Fn()`、`disabled`: リアクティブな `bool`、`style`: リアクティブな `ButtonStyle` | `Element` → `children` |
| `slider!{…}` | `ui::slider(SliderProps { … })` | `value`: リアクティブな `Int<LO, HI>`、必須、`on_change`: ハンドラ `Fn(Int<LO, HI>)`、`step`: リアクティブな `u32`、`disabled`: リアクティブな `bool`、`style`: リアクティブな `SliderStyle` | なし |
| `toggle!{…}` | `ui::toggle(ToggleProps { … })` | `value`: リアクティブな `bool`、必須、`on_change`: ハンドラ `Fn(bool)`、`disabled`: リアクティブな `bool`、`style`: リアクティブな `ButtonStyle` | `Element` → `children` |
| `select!{…}` | `ui::select(SelectProps { … })` | `value`: リアクティブな `T`、必須、`on_change`: ハンドラ `Fn(T)`、`disabled`: リアクティブな `bool`、`style`: リアクティブな `ButtonStyle` | `ui::option(v, label)` → `options` |

例えば

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

は次のように脱糖されます。

```kis
ui::button(ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| ui::label(t!["Back"])],
})
```

また

```kix
slider!{value = music().level, on_change = |v| set_level(v)}
```

は次のようになります。

```kis
ui::slider(SliderProps {
    value: || music().level,
    on_change: Some(|v| set_level(v)),
    step: None,
    disabled: None,
    style: None,
})
```

### 関数: `column(children)` {#column}

```kis
#[builtin] pub fn column<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

`column![…]` の脱糖先です。子を上から下へ積みます。子の `each` や `show` は平坦化され、その要素が列の子として積まれます（[行と列の中の `each` と `show`](#flattening) を参照）。

### 関数: `row(children)` {#row}

```kis
#[builtin] pub fn row<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

`row![…]` の脱糖先です。子を左から右へ並べます。子の `each` や `show` は平坦化され、その要素が行の子として並びます（[行と列の中の `each` と `show`](#flattening) を参照）。

### 関数: `layer(children)` {#layer}

```kis
#[builtin] pub fn layer<const N: usize>(children: [Fn() -> Placed; N]) -> Element;
```

`layer![…]` の脱糖先です。[`Placed`](#placed) の子を重ね、後の子ほど上に描かれます。

### 構造体: `TextProps` {#textprops}

```kis
pub struct TextProps {
    pub style: Option<Fn() -> TextStyle>,
    pub content: Fn() -> Text,
}
```

`text!(…)` の属性と内容です。

### 関数: `text(props)` {#text}

```kis
#[builtin] pub fn text(props: TextProps) -> Element;
```

`text!(…)` の脱糖先で、リアクティブなテキストです。`style` がなければ [`System::text`](./system.md#system) が使われ、ボタンやトグルの中では現在の見た目のテキストスタイルが使われます。

```kix
text!(style = NAME_STYLE, who_text(game))
```

### 構造体: `ImageProps` {#imageprops}

```kis
pub struct ImageProps {
    pub fit: Option<Fn() -> Fit>,
    pub source: Fn() -> ImageAsset,
}
```

`image!(…)` の属性とソースです。

### 関数: `image(props)` {#image}

```kis
#[builtin] pub fn image(props: ImageProps) -> Element;
```

`image!(…)` の脱糖先で、画像です。ソースはリアクティブなので、`image!(if seen_image(cg) { cg } else { LOCKED })` は条件が変わるとすぐに切り替わります。

### 構造体: `GameScreenProps` {#gamescreenprops}

```kis
pub struct GameScreenProps {
    pub game: Fn() -> Narrative,
}
```

`game_screen!(…)` の属性です。

### 関数: `game_screen(props)` {#game_screen}

```kis
#[builtin] pub fn game_screen(props: GameScreenProps) -> Element;
```

`game_screen!(…)` の脱糖先で、物語のレンダーが描く実行中のゲームの画面です。ゲームビューでは通常、`layer![…]` の最初に `.cover()` 付きで置きます。

```kix
use std::narrative::text::text_style;
use std::screen::BOTTOM_CENTER;
use std::ui::message;

fn GameView(game: Narrative) -> Element {
    layer![
        game_screen!(game = game).cover(),
        message(game, text_style(26, WHITE)).width(1000).at(point(640, 690), BOTTOM_CENTER),
    ]
}
```

### 構造体: `ButtonProps` {#buttonprops}

```kis
pub struct ButtonProps<const N: usize> {
    pub on_click: Option<Fn()>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

`button!{…}` の属性と子です。

### 関数: `button(props)` {#button}

```kis
#[builtin] pub fn button<const N: usize>(props: ButtonProps<N>) -> Element;
```

`button!{…}` の脱糖先で、フォーカス可能なボタンです。`on_click` はクリックまたは実行時に走ります。無効なボタンは入力を無視し、無効時の見た目を表示します（[インタラクションの見た目](./ui-style.md#interaction-looks) を参照）。ボタンは現在の見た目の背景をボックス全体に塗り、その上に子を中央揃えで配置します。そのため子のないボタンは背景だけで描かれ、画像ボタンを作れます。

```kix
button!{style = image_style(START, START_HOVER), on_click = || start_game()}.width(300).height(70)
```

### 構造体: `SliderProps` {#sliderprops}

```kis
pub struct SliderProps<const LO: i64, const HI: i64> {
    pub value: Fn() -> Int<LO, HI>,
    pub on_change: Option<Fn(Int<LO, HI>)>,
    pub step: Option<Fn() -> u32>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> SliderStyle>,
}
```

`slider!{…}` の属性です。範囲は値の `Int` 型の範囲で、`min` や `max` の属性はありません。

### 関数: `slider(props)` {#slider}

```kis
#[builtin] pub fn slider<const LO: i64, const HI: i64>(props: SliderProps<LO, HI>) -> Element;
```

`slider!{…}` の脱糖先で、`LO` から `HI` まで `step` 刻みで動く、フォーカス可能なスライダーです。`Volume::level` は `Int<0, 100>` なので、音量スライダーに範囲の指定は要りません。

```kix
slider!{
    value = MUSIC.get().level,
    on_change = |v| MUSIC.set(Volume { level: v, muted: MUSIC.get().muted }),
}
```

### 構造体: `ToggleProps` {#toggleprops}

```kis
pub struct ToggleProps<const N: usize> {
    pub value: Fn() -> bool,
    pub on_change: Option<Fn(bool)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

`toggle!{…}` の属性と子です。

### 関数: `toggle(props)` {#toggle}

```kis
#[builtin] pub fn toggle<const N: usize>(props: ToggleProps<N>) -> Element;
```

`toggle!{…}` の脱糖先で、フォーカス可能なオン・オフのコントロールです。実行する（クリック、Enter、`PadButton::South`）と、`!value()` で `on_change` を呼びます。値が `true` の間、トグルはオンの見た目を表示します（[インタラクションの見た目](./ui-style.md#interaction-looks) を参照）。

```kix
toggle!{value = SKIP_UNREAD.get(), on_change = |v| SKIP_UNREAD.set(v), t!["Skip unread text"]}
```

### 構造体: `SelectOption<T>` {#selectoption}

```kis
pub struct SelectOption<T> {
    pub value: T,
    pub label: Element,
}
```

`select!{…}` の選択肢です。

### 関数: `option(value, label)` {#option}

```kis
pub fn option<T>(value: T, label: Element) -> SelectOption<T> {
    SelectOption { value, label }
}
```

- `value` `T` — 選択肢が表す値。
- `label` `Element` — 選択肢の表示。
- 戻り値: `SelectOption<T>` — `label` として表示される選択肢 `value`。
- 種別: `fn` · ビルトイン: いいえ

### 構造体: `SelectProps<T, N>` {#selectprops}

```kis
pub struct SelectProps<T, const N: usize> {
    pub value: Fn() -> T,
    pub on_change: Option<Fn(T)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub options: [Fn() -> SelectOption<T>; N],
}
```

`select!{…}` の属性と選択肢です。

### 関数: `select(props)` {#select}

```kis
#[builtin] pub fn select<T: Eq, const N: usize>(props: SelectProps<T, N>) -> Element;
```

`select!{…}` の脱糖先で、選択肢を順に切り替えるフォーカス可能なコントロールです。値が `value()` と等しい選択肢のラベルを表示するため、`T` は `Eq` でなければなりません。実行すると次の選択肢の値で `on_change` を呼び、最後の次は最初に戻ります。フォーカス中は、右キーと左キーで次と前の選択肢に切り替わります。`value()` と等しい選択肢がないときは、最初の選択肢の値が次の値になります。

```kix
use std::ui::{label, option};

select!{
    value = WINDOW_OPACITY.get(),
    on_change = |v| WINDOW_OPACITY.set(v),
    option(50, label("50%")),
    option(80, label("80%")),
    option(100, label("100%")),
}
```

> **安定性:** 予定 — UI 要素のフェードとトランジションはまだサポートされていません。アニメーションするのはコントロールの見た目の変化だけです（[`ButtonStyle::change`](./ui-style.md#buttonstyle-change) を参照）。
