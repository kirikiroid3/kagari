---
title: "std::ui"
---

> **安定性:** 安定 — UI の値、リアクティビティ、UI 構文の展開先となる関数。

`std::ui` はゲームシステム用の UI ツールキットです。要素の型、リアクティブなシグナル、内容とレイアウトを扱う関数のほか、UI 構文（`column![…]`、`button!{…}`、`text!(…)` など）の展開先をまとめています。コントロールの見た目は [`std::ui::style`](./ui-style.md) で宣言します。

UI は `fn` のコードで書きます。**コンポーネント** とは [`Element`](#element) を返す `fn` のことです。モードの画面ではその要素をモード型（[`RootPage`、`Overlay`、`GameOverlay`、`Dialog`](./system.md#mode-types)）で包み、ゲーム画面では `Element` をそのまま返します。スタイルは `const fn` で作るデータです。

プレリュードには `signal` と `Element` が含まれます。それ以外のアイテムは `use std::ui::{Fit, each, show, thumbnail};` のようにパスを指定してインポートしてください。

```kis
pub mod style;
```

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Element`](#element)、[`Placed`](#placed) | 組み込みの構造体 | UI 要素、レイヤーに配置した要素 |
| [`Align`](#align)、[`Fit`](#fit) | 列挙型 | フレックスの整列、枠への画像の収め方 |
| [`signal`](#signal)、[`effect`](#effect)、[`each`](#each)、[`show`](#show) | `fn` | リアクティビティ |
| [`label`](#label)、[`spacer`](#spacer)、[`scroll`](#scroll)、[`thumbnail`](#thumbnail)、[`message`](#message) | `fn` | 内容 |
| [`Element` のメソッド](#layout) | `fn` | サイズ、余白、整列、塗り、フォーカス順、配置 |
| [`column`](#column)、[`row`](#row)、[`layer`](#layer) | `fn` | レイアウト構文の展開先 |
| [`text`](#text)、[`image`](#image)、[`game_screen`](#game_screen) | `fn` | 内容を表す構文の展開先 |
| [`button`](#button)、[`slider`](#slider)、[`toggle`](#toggle)、[`select`](#select)、[`option`](#option) | `fn` | コントロール構文の展開先 |

## 要素 {#elements}

### 構造体：`Element` {#element}

```kis
#[builtin] pub struct Element;
```

エンジンがレイアウトする UI 要素のツリーです。中身を直接見ることはできません。このモジュールの関数や UI 構文で作り、[レイアウトのメソッド](#layout) で調整します。

### 構造体：`Placed` {#placed}

```kis
#[builtin] pub struct Placed;
```

[`layer![…]`](#layer) に配置した要素です。[`Element::at`](#element-at) か [`Element::cover`](#element-cover) で作ります。

### 列挙型：`Align` {#align}

```kis
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}
```

行や列の子をフレックスでどう整列するかを表し、[`Element::align`](#element-align) に渡します。

| バリアント | 行・列に沿った方向（`main`） | 交差する方向（`cross`） |
| ---------- | ---------------------------- | ----------------------- |
| `Start` | 先頭に詰める | 先頭に置く |
| `Center` | 中央に寄せる | 中央に置く |
| `End` | 末尾に詰める | 末尾に置く |
| `Stretch` | `Start` と同じ | 固定サイズでない子を交差方向いっぱいに引き伸ばす |
| `SpaceBetween` | 空き領域を子の間に等しく分ける（子が 1 つなら `Start` と同じ） | `Start` と同じ |

### 列挙型：`Fit` {#fit}

```kis
pub enum Fit {
    Stretch,
    Contain,
    Cover,
    Natural,
}
```

画像で枠をどう埋めるかを表し、[`image!`](#image) で使います。

| バリアント | 意味 |
| ---------- | ---- |
| `Stretch` | 縦横比を無視して枠いっぱいに広げます |
| `Contain` | 縦横比を保ったまま枠に収まる最大の大きさにし、枠の中央に置きます |
| `Cover` | 縦横比を保ったまま枠を埋めます。はみ出した方向は両端を均等に切り取ります |
| `Natural` | 画像本来のピクセルサイズのまま、枠の左上に置きます |

## コンポーネントとリアクティビティ {#components-and-reactivity}

コンポーネントとリアクティビティの仕組みは [SolidJS](https://www.solidjs.com/) を手本にしています。

- コンポーネントは、モードが表示されるときに **一度だけ** 実行されます。
- **リアクティブなソース** には、シグナルのゲッター、`Setting::get`、`Meta::get`、`Narrative` ハンドルからの読み取り、[`saves::info`](./system-saves.md#info)、[既見セット](./system.md#seen-sets) があります。
- **リアクティブなクロージャ** は、読み取ったソースが変わると実行し直されます。リアクティブな属性や UI 構文の要素がこれに当たります（[UI 構文](#ui-forms) を参照）。たとえば `text!(t!["Music: " {music().level}])` は `music` が変わるたびに更新されますが、コンポーネントのほかの部分は実行し直されません。
- `on_click` などのハンドラは普通のクロージャで、イベントが起きたときに実行されます。

シグナルは `fn` のコードの中にしかありません。物語や `const fn` からは使えません。

### 関数：`signal(init)` {#signal}

```kis
#[builtin] pub fn signal<T>(init: T) -> (Fn() -> T, Fn(T));
```

- `init` `T` — 初期値。
- 戻り値：`(Fn() -> T, Fn(T))` — ゲッターとセッター。ゲッターはリアクティブなソースです。
- 種別：`fn` · 組み込み：はい

```kix
let (hidden, set_hidden) = signal(false);
```

### 関数：`effect(run)` {#effect}

```kis
#[builtin] pub fn effect(run: Fn());
```

- `run` `Fn()` — 実行するクロージャ。
- 種別：`fn` · 組み込み：はい

`run` をすぐに一度実行し、その後は読み取ったソースが変わるたびに実行し直します。

### 関数：`each(items, item)` {#each}

```kis
#[builtin] pub fn each<T>(items: Fn() -> Vec<T>, item: Fn(T) -> Element) -> Element;
```

- `items` `Fn() -> Vec<T>` — 項目の一覧。リアクティブです。
- `item` `Fn(T) -> Element` — 項目ひとつ分の要素を作ります。
- 戻り値：`Element` — 項目ごとの要素。常に `items` と同じ内容に保たれます。
- 種別：`fn` · 組み込み：はい

`column![…]` や `row![…]` の中に置くと、各項目の要素がその列や行の子になり、`each` 自体に付けたレイアウトメソッドは無視されます。それ以外の場所では、項目を縦に並べた列になります。詳しくは [行・列の中の `each` と `show`](#flattening) を参照してください。

```kix
scroll(|| each(|| game.history(), |entry| entry_view(entry)))
```

### 関数：`show(when, then)` {#show}

```kis
#[builtin] pub fn show(when: Fn() -> bool, then: Fn() -> Element) -> Element;
```

- `when` `Fn() -> bool` — 表示する条件。リアクティブです。
- `then` `Fn() -> Element` — 表示する内容を作ります。
- 戻り値：`Element` — `when()` が `true` の間は `then()`、それ以外は何も表示しません。
- 種別：`fn` · 組み込み：はい

[`each`](#each) と同じく、親が `column![…]` か `row![…]` なら平坦化され、それ以外の場所では列になります。詳しくは [行・列の中の `each` と `show`](#flattening) を参照してください。

## 内容 {#content}

### 関数：`label(text)` {#label}

```kis
#[builtin] pub fn label(text: Text) -> Element;
```

- `text` `Text` — 表示するテキスト。
- 戻り値：`Element` — 変化しないテキスト。
- 種別：`fn` · 組み込み：はい

文字の体裁には [`System::text`](./system.md#system) を使います。ただしボタンやトグルの中では、そのときの見た目に設定されたテキストスタイルを使います（[操作に応じた見た目](./ui-style.md#interaction-looks) を参照）。リアクティブにしたい場合やスタイルを指定したい場合は `text!(…)` を使います。`column!`、`row!`、`button!`、`toggle!` の中に `t![…]` をそのまま書くと、`label(t![…])` として扱われます。

### 関数：`spacer()` {#spacer}

```kis
#[builtin] pub fn spacer() -> Element;
```

- 戻り値：`Element` — 伸びて隣の要素を押し広げる、空の要素。
- 種別：`fn` · 組み込み：はい

### 関数：`scroll(child)` {#scroll}

```kis
#[builtin] pub fn scroll(child: Fn() -> Element) -> Element;
```

- `child` `Fn() -> Element` — 中身。
- 戻り値：`Element` — 中身を収めたスクロールできる枠。
- 種別：`fn` · 組み込み：はい

### 関数：`thumbnail(slot)` {#thumbnail}

```kis
#[builtin] pub fn thumbnail(slot: SaveSlot) -> Element;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — 対象のスロット。
- 戻り値：`Element` — `slot` にセーブしたときに一緒に保存したスクリーンショット。
- 種別：`fn` · 組み込み：はい

### 関数：`message(game, style)` {#message}

```kis
#[builtin] pub fn message(game: Narrative, style: TextStyle) -> Element;
```

- `game` `Narrative` — 実行中のゲーム。
- `style` `TextStyle` — メッセージウィンドウのテキストスタイル（[`std::narrative::text`](./narrative-text.md) を参照）。
- 戻り値：`Element` — 現在の行。文字送り、テキスト効果、一時停止、マーク、クリック待ち記号まで含みます。
- 種別：`fn` · 組み込み：はい

`message` はメッセージウィンドウの本体です。`Narrative` ハンドルがある場所、すなわちゲーム画面とゲームオーバーレイの中でだけ使えます。

> **安定性:** 予定 — NVL モード（改ページまで複数の行を画面に残す全画面表示のテキストモード）はまだ設計していません。導入時には、テキストウィンドウに表示する内容と、バックログや `Narrative::line` でのステップの表し方が変わる予定です。

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

レイアウトの考え方はフレックスボックスに近いものです。

- [`column![…]`](#column) と [`row![…]`](#row) はフレックスコンテナです。子の間隔は `gap`、整列は `align` で決まり、`grow` は子ごとの伸長係数です。子にある [`each` と `show`](#flattening) は平坦化されます。
- [`layer![…]`](#layer) は [`Placed`](#placed) の子を重ねて表示します。`at` は子のアンカーを指定した点に合わせ、`cover` はレイヤー全体を覆います。
- サイズと位置の単位は、デザイン解像度（[`System::resolution`](./system.md#system)）でのピクセルです。
- レイアウトはエンジンが要素ツリーに対して行う処理です。リサイズのたびに計算し直し、セーブには含まれません。

どのメソッドも要素を値で受け取り、調整後の要素を返します。そのため `column![…].gap(16).align(Align::Center, Align::Center)` のようにつなげて書けます。いずれも組み込みの `fn` です。

### 行・列の中の `each` と `show` {#flattening}

[`each`](#each) と [`show`](#show) のレイアウトは、親が何かによって変わります。

- **`column![…]` か `row![…]` の子のとき：平坦化されます。** `each` の要素（`show` では `then()`）は、`each` が置かれていた位置に、列や行の子として直接入ります。間隔は列や行の `gap`、整列は列や行の `align` に従い、各要素の `grow` も効きます。平坦化された `each` や `show` の中にさらに `each` や `show` があれば、それも平坦化されます。そのため `column![show(…, || each(…))]` では項目が列に直接入ります。平坦化された `each` や `show` は自分の枠を持たないので、そこに呼んだ `width`、`height`、`padding`、`gap`、`align`、`grow`、`background`、`opacity` は無視されます。
- **それ以外の場所：列になります。** `layer![…]` に配置した場合、`scroll(…)`、`button!`、`toggle!` の中身にした場合、平坦化されない `each` や `show` の中に置いた場合、モードのルート要素にした場合は、要素を縦に並べた列としてレイアウトされ、`each` や `show` 自体に付けたメソッドが効きます。

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

- `px` `u32` — 固定の幅（ピクセル）。

### `Element::height(px)` {#element-height}

```kis
#[builtin] pub fn height(self, px: u32) -> Element;
```

- `px` `u32` — 固定の高さ（ピクセル）。

### `Element::padding(insets)` {#element-padding}

```kis
#[builtin] pub fn padding(self, insets: Insets) -> Element;
```

- `insets` `Insets` — 内側の余白。`std::screen::insets(top, right, bottom, left)` で作ります（[`std::screen`](./screen.md) を参照）。

### `Element::gap(px)` {#element-gap}

```kis
#[builtin] pub fn gap(self, px: u32) -> Element;
```

- `px` `u32` — 行や列の子どうしの間隔。平坦化された `each` や `show` では無視されます（[行・列の中の `each` と `show`](#flattening) を参照）。

### `Element::align(main, cross)` {#element-align}

```kis
#[builtin] pub fn align(self, main: Align, cross: Align) -> Element;
```

- `main` `Align` — 行や列の並ぶ方向での子の整列。
- `cross` `Align` — それと直交する方向での整列。

### `Element::grow(factor)` {#element-grow}

```kis
#[builtin] pub fn grow(self, factor: u32) -> Element;
```

- `factor` `u32` — フレックスの伸長係数。行や列の空き領域をどれだけの割合で受け取るかを表します。

### `Element::background(paint)` {#element-background}

```kis
#[builtin] pub fn background(self, paint: Paint) -> Element;
```

- `paint` [`Paint`](./ui-style.md#paint) — 要素の背面に描く塗り。

### `Element::opacity(opacity)` {#element-opacity}

```kis
#[builtin] pub fn opacity(self, opacity: Decimal) -> Element;
```

- `opacity` `Decimal` — 要素の不透明度。範囲は `0.0..=1.0` です。

### `Element::focus_order(order)` {#element-focus_order}

```kis
#[builtin] pub fn focus_order(self, order: u32) -> Element;
```

- `order` `u32` — フォーカスが移動する順番での位置（[フォーカス](#focus) を参照）。

### `Element::at(position, anchor)` {#element-at}

```kis
#[builtin] pub fn at(self, position: Point, anchor: Anchor) -> Placed;
```

- `position` `Point` — レイヤー上の点。
- `anchor` `Anchor` — 要素のうち、`position` に合わせる点。`TOP_LEFT` や `CENTER` など（[`std::screen`](./screen.md) を参照）。
- 戻り値：`Placed` — `layer![…]` に渡す値。

### `Element::cover()` {#element-cover}

```kis
#[builtin] pub fn cover(self) -> Placed;
```

- 戻り値：`Placed` — レイヤー全体を覆う要素。

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

- フォーカスできる要素は、ボタン、スライダー、トグル、セレクトです。
- [`focus_order`](#element-focus_order) を指定しなければ、フォーカスは記述順に移動します。
- モードが開くと、最初のフォーカス可能な要素にフォーカスが当たります。ただしフォーカスが表示されるのは、フォーカス移動のキーを初めて押したときからです。Enter や `PadButton::South` でフォーカス中のコントロールを実行できるのも、フォーカスが表示されている間だけです。
- 上下左右キー、十字キー、Tab、Shift+Tab でフォーカスが移動します。ただしスライダーやセレクトにフォーカスがあるときは、左右キーで値が変わります。スライダーは `step` ずつ増減し、セレクトは前後の選択肢に切り替わります。
- これらのキーは組み込みのもので、割り当てを変えることはできません（[バインディング](./system.md#bindings) を参照）。

## UI 構文 {#ui-forms}

UI 構文は `.kix` ファイルに書きます。展開すると、props 構造体を渡す以下の関数の呼び出しになります。`fn` や `const fn` のコードで値を作るための構文であり、ステップにはなりません。区切り文字の規則は言語全体と同じです（リファレンスの [組み込み構文](../reference/syntax/forms.md) を参照）。

- `name![…]` は要素だけを受け取ります。
- `name!(…)` は属性と、決まった数の要素を受け取ります。
- `name!{…}` は属性と、任意の数の要素を受け取ります。

### 展開 {#desugaring}

- **リアクティブな属性** は `key: Some(|| expr)` になります。必須の属性なら `key: || expr` です。
- **ハンドラ属性**（`on_*`）は `key: Some(expr)` になります。
- **省略した任意の属性** は `None` になります。必須の属性がない場合、未知の属性がある場合、要素の数が合わない場合（`text!` と `image!` はちょうど 1 つ、`game_screen!` と `slider!` は 0 個）は構文エラーです。
- **要素** は、それぞれ `e` が `|| e` になります。`column!`、`row!`、`button!`、`toggle!` の中で、書かれた形が `t![…]` である要素は `|| ui::label(t![…])` になります。

| 構文 | 展開先 | 属性 | 要素 |
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

たとえば次のコードは、

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

次のように展開されます。

```kis
ui::button(ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| ui::label(t!["Back"])],
})
```

同様に、

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

### 関数：`column(children)` {#column}

```kis
#[builtin] pub fn column<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

`column![…]` の展開先です。子を上から下へ積み重ねます。子にある `each` や `show` は平坦化され、その要素が列の子として積まれます（[行・列の中の `each` と `show`](#flattening) を参照）。

### 関数：`row(children)` {#row}

```kis
#[builtin] pub fn row<const N: usize>(children: [Fn() -> Element; N]) -> Element;
```

`row![…]` の展開先です。子を左から右へ並べます。子にある `each` や `show` は平坦化され、その要素が行の子として並びます（[行・列の中の `each` と `show`](#flattening) を参照）。

### 関数：`layer(children)` {#layer}

```kis
#[builtin] pub fn layer<const N: usize>(children: [Fn() -> Placed; N]) -> Element;
```

`layer![…]` の展開先です。[`Placed`](#placed) の子を重ね、後に書いた子ほど手前に描きます。

### 構造体：`TextProps` {#textprops}

```kis
pub struct TextProps {
    pub style: Option<Fn() -> TextStyle>,
    pub content: Fn() -> Text,
}
```

`text!(…)` の属性と内容をまとめた構造体です。

### 関数：`text(props)` {#text}

```kis
#[builtin] pub fn text(props: TextProps) -> Element;
```

`text!(…)` の展開先で、リアクティブなテキストを作ります。`style` を省略すると [`System::text`](./system.md#system) を使います。ボタンやトグルの中では、そのときの見た目に設定されたテキストスタイルを使います。

```kix
text!(style = NAME_STYLE, who_text(game))
```

### 構造体：`ImageProps` {#imageprops}

```kis
pub struct ImageProps {
    pub fit: Option<Fn() -> Fit>,
    pub source: Fn() -> ImageAsset,
}
```

`image!(…)` の属性と画像のソースをまとめた構造体です。

### 関数：`image(props)` {#image}

```kis
#[builtin] pub fn image(props: ImageProps) -> Element;
```

`image!(…)` の展開先で、画像を表示します。ソースもリアクティブなので、`image!(if seen_image(cg) { cg } else { LOCKED })` は条件が変わるとすぐに画像が切り替わります。

### 構造体：`GameScreenProps` {#gamescreenprops}

```kis
pub struct GameScreenProps {
    pub game: Fn() -> Narrative,
}
```

`game_screen!(…)` の属性をまとめた構造体です。

### 関数：`game_screen(props)` {#game_screen}

```kis
#[builtin] pub fn game_screen(props: GameScreenProps) -> Element;
```

`game_screen!(…)` の展開先で、物語の画面（実行中のゲームでレンダーが描いたもの）を表示します。ゲーム画面（`System::game`）では普通、`layer![…]` の先頭に `.cover()` を付けて置きます。

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

### 構造体：`ButtonProps` {#buttonprops}

```kis
pub struct ButtonProps<const N: usize> {
    pub on_click: Option<Fn()>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

`button!{…}` の属性と子をまとめた構造体です。

### 関数：`button(props)` {#button}

```kis
#[builtin] pub fn button<const N: usize>(props: ButtonProps<N>) -> Element;
```

`button!{…}` の展開先で、フォーカスできるボタンを作ります。クリックするか実行操作をすると `on_click` が呼ばれます。無効にしたボタンは入力を受け付けず、無効時の見た目になります（[操作に応じた見た目](./ui-style.md#interaction-looks) を参照）。ボタンはそのときの見た目の背景を枠全体に塗り、その上に子を中央揃えで置きます。子のないボタンは背景だけになるので、画像だけのボタンも作れます。

```kix
button!{style = image_style(START, START_HOVER), on_click = || start_game()}.width(300).height(70)
```

### 構造体：`SliderProps` {#sliderprops}

```kis
pub struct SliderProps<const LO: i64, const HI: i64> {
    pub value: Fn() -> Int<LO, HI>,
    pub on_change: Option<Fn(Int<LO, HI>)>,
    pub step: Option<Fn() -> u32>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> SliderStyle>,
}
```

`slider!{…}` の属性をまとめた構造体です。値の範囲は `Int` 型の範囲で決まるので、`min` や `max` の属性はありません。

### 関数：`slider(props)` {#slider}

```kis
#[builtin] pub fn slider<const LO: i64, const HI: i64>(props: SliderProps<LO, HI>) -> Element;
```

`slider!{…}` の展開先で、`LO` から `HI` まで `step` 刻みで動く、フォーカスできるスライダーを作ります。`Volume::level` は `Int<0, 100>` なので、音量のスライダーでは範囲を指定する必要がありません。

```kix
slider!{
    value = MUSIC.get().level,
    on_change = |v| MUSIC.set(Volume { level: v, muted: MUSIC.get().muted }),
}
```

### 構造体：`ToggleProps` {#toggleprops}

```kis
pub struct ToggleProps<const N: usize> {
    pub value: Fn() -> bool,
    pub on_change: Option<Fn(bool)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub children: [Fn() -> Element; N],
}
```

`toggle!{…}` の属性と子をまとめた構造体です。

### 関数：`toggle(props)` {#toggle}

```kis
#[builtin] pub fn toggle<const N: usize>(props: ToggleProps<N>) -> Element;
```

`toggle!{…}` の展開先で、オンとオフを切り替える、フォーカスできるコントロールを作ります。実行（クリック、Enter、`PadButton::South`）すると、`!value()` を引数に `on_change` を呼びます。値が `true` の間はオンの見た目になります（[操作に応じた見た目](./ui-style.md#interaction-looks) を参照）。

```kix
toggle!{value = SKIP_UNREAD.get(), on_change = |v| SKIP_UNREAD.set(v), t!["Skip unread text"]}
```

### 構造体：`SelectOption<T>` {#selectoption}

```kis
pub struct SelectOption<T> {
    pub value: T,
    pub label: Element,
}
```

`select!{…}` の選択肢ひとつ分です。

### 関数：`option(value, label)` {#option}

```kis
pub fn option<T>(value: T, label: Element) -> SelectOption<T> {
    SelectOption { value, label }
}
```

- `value` `T` — この選択肢が表す値。
- `label` `Element` — 選択肢として表示するもの。
- 戻り値：`SelectOption<T>` — `label` を表示し、値 `value` を表す選択肢。
- 種別：`fn` · 組み込み：いいえ

### 構造体：`SelectProps<T, N>` {#selectprops}

```kis
pub struct SelectProps<T, const N: usize> {
    pub value: Fn() -> T,
    pub on_change: Option<Fn(T)>,
    pub disabled: Option<Fn() -> bool>,
    pub style: Option<Fn() -> ButtonStyle>,
    pub options: [Fn() -> SelectOption<T>; N],
}
```

`select!{…}` の属性と選択肢をまとめた構造体です。

### 関数：`select(props)` {#select}

```kis
#[builtin] pub fn select<T: Eq, const N: usize>(props: SelectProps<T, N>) -> Element;
```

`select!{…}` の展開先で、選択肢を順に切り替える、フォーカスできるコントロールを作ります。値が `value()` と等しい選択肢のラベルを表示するので、`T` は `Eq` を満たす型でなければなりません。実行すると次の選択肢の値で `on_change` を呼び、最後の選択肢の次は最初に戻ります。フォーカス中は右キーで次の選択肢、左キーで前の選択肢に切り替わります。`value()` と等しい選択肢がない場合は、最初の選択肢の値が次の値になります。

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

> **安定性:** 予定 — UI 要素のフェードやトランジションにはまだ対応していません。アニメーションするのは、コントロールの見た目が切り替わるときだけです（[`ButtonStyle::change`](./ui-style.md#buttonstyle-change) を参照）。
