---
title: "std::ui::style"
---

> **安定性:** 安定 — UI コントロールの見た目。

`std::ui::style` では背景とコントロールの見た目を記述します。[`Paint`](#paint) は枠の塗り、[`Look`](#look) は背景とテキストスタイルの組です。[`ButtonStyle`](#buttonstyle) はボタン・トグル・セレクトの操作状態ごとの見た目を、[`SliderStyle`](#sliderstyle) はスライダーの塗りを表します。

スタイルは `const fn` で作る普通のデータです。制作者はスタイルを `const` アイテムとして宣言し、コントロール構文の `style` 属性や [`Element::background`](./ui.md#element-background) に渡します。

```kis
use std::ui::style::{ButtonStyle, Look, Paint, SliderStyle, button_style};
```

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Paint`](#paint) | 列挙型 | 枠を埋める塗り |
| [`Look`](#look) | 構造体 | 背景とテキストスタイル |
| [`ButtonStyle`](#buttonstyle) | 構造体 | ボタン型コントロールの操作状態ごとの見た目 |
| [`button_style(idle)`](#button_style) | `const fn` | 通常時の見た目だけを指定したスタイル |
| [`ButtonStyle` のビルダーメソッド](#buttonstyle-hover) | `const fn` | 見た目、補間、サウンドの追加 |
| [`SliderStyle`](#sliderstyle) | 構造体 | スライダーの塗り |

## 列挙型：`Paint` {#paint}

```kis
pub enum Paint {
    None,
    Color(Color),
    Image(ImageAsset),
    /// An image whose corners stay unscaled; `insets` are the corner sizes.
    NineSlice { image: ImageAsset, insets: Insets },
}
```

| バリアント | 意味 |
| ---------- | ---- |
| `None` | 塗りなし。枠は透明になります |
| `Color(c)` | 単色。例：`rgba(0, 0, 0, 200)` |
| `Image(i)` | 枠全体に引き伸ばした画像 |
| `NineSlice { image, insets }` | 9 スライス画像。`insets` で指定した大きさの四隅は拡大縮小せず、辺と中央だけを伸ばします。大きさが変わるウィンドウ枠やボタンに使います |

## 構造体：`Look` {#look}

```kis
pub struct Look {
    pub background: Paint,
    pub text: TextStyle,
}
```

背景とテキストスタイルの組です。`text` のスタイルは、コントロールの中のラベルに使われます（[操作に応じた見た目](#interaction-looks) を参照）。

## 構造体：`ButtonStyle` {#buttonstyle}

```kis
pub struct ButtonStyle {
    pub idle: Look,
    pub hover: Option<Look>,
    pub focus: Option<Look>,
    pub pressed: Option<Look>,
    /// Replaces every other look while the control is disabled.
    pub disabled: Option<Look>,
    /// How look changes animate.
    pub change: Tween,
    pub hover_sound: Option<Sound>,
    pub click_sound: Option<Sound>,
}
```

ボタン型のコントロール（`button!`、`toggle!`、`select!`）について、操作状態ごとの見た目、見た目が切り替わるときのアニメーション、再生するサウンドをまとめたものです。UI のサウンドはサウンドバスで再生します。

### 操作に応じた見た目 {#interaction-looks}

- コントロールが無効な間は、ほかのどの見た目よりも `disabled` が優先されます。`disabled` を指定していなければ、無効なコントロールは `idle` を半透明（不透明度は半分）で表示します。
- 無効でなければ、`pressed`（コントロール上でポインタを押している間）、`focus`（フォーカスがあり、かつフォーカスが表示されている間）、`hover` の順に調べ、条件に当てはまり `Some` である最初のものを使います。どれも当てはまらなければ `idle` です。
- 値が `true` のトグルは、本来 `idle` を表示する場面で **オンの見た目** を表示します。オンの見た目は `pressed` があればそれ、なければ `hover`、それもなければ `idle` に通常時のテキスト色で 2 px の枠線を付けたものです。
- 見た目の切り替わりは `change` に従ってアニメーションします。
- ボタンやトグルの中のラベルは、そのときの見た目の `text` を使います。それ以外の場所では、`text!` で `style` を指定しない限り、`label` も `text!` も [`System::text`](./system.md#system) を使います。これ以外にスタイルが引き継がれることはありません。

### 関数：`button_style(idle)` {#button_style}

```kis
pub const fn button_style(idle: Look) -> ButtonStyle {
    ButtonStyle {
        idle,
        hover: None,
        focus: None,
        pressed: None,
        disabled: None,
        change: instant(),
        hover_sound: None,
        click_sound: None,
    }
}
```

- `idle` `Look` — 通常時の見た目。
- 戻り値：`ButtonStyle` — 通常時の見た目だけを指定し、切り替わりは即時、サウンドなしのスタイル。
- 種別：`const fn` · 組み込み：いいえ

残りは以下のビルダーメソッドで追加します。どのメソッドもスタイルを値で受け取り、変更したものを返すので、つなげて書けます。

### `ButtonStyle::hover(look)` {#buttonstyle-hover}

```kis
pub const fn hover(mut self, look: Look) -> ButtonStyle
```

- 戻り値：ホバー時の見た目を加えたスタイル。

### `ButtonStyle::focus(look)` {#buttonstyle-focus}

```kis
pub const fn focus(mut self, look: Look) -> ButtonStyle
```

- 戻り値：フォーカス時の見た目を加えたスタイル。キーボードやパッドでフォーカスしている間に表示します。

### `ButtonStyle::pressed(look)` {#buttonstyle-pressed}

```kis
pub const fn pressed(mut self, look: Look) -> ButtonStyle
```

- 戻り値：押している間の見た目を加えたスタイル。

### `ButtonStyle::disabled(look)` {#buttonstyle-disabled}

```kis
pub const fn disabled(mut self, look: Look) -> ButtonStyle
```

- 戻り値：無効時の見た目を加えたスタイル。

### `ButtonStyle::change(tween)` {#buttonstyle-change}

```kis
pub const fn change(mut self, tween: Tween) -> ButtonStyle
```

- `tween` `Tween` — 見た目の切り替わりにかける時間、イージング、遅延（[`std::screen::anim`](./screen-anim.md) を参照）。
- 戻り値：見た目の切り替わりを `tween` でアニメーションさせるスタイル。

### `ButtonStyle::hover_sound(sound)` {#buttonstyle-hover_sound}

```kis
pub const fn hover_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — 単発のサウンド（[`std::screen::audio`](./screen-audio.md) を参照）。
- 戻り値：ホバー時に `sound` を鳴らすスタイル。

### `ButtonStyle::click_sound(sound)` {#buttonstyle-click_sound}

```kis
pub const fn click_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — 単発のサウンド。
- 戻り値：クリック時に `sound` を鳴らすスタイル。

ビルダーメソッドはいずれも、種別：`const fn` · 組み込み：いいえ です。

```kis
use std::narrative::text::text_style;
use std::screen::rgba;
use std::ui::style::{ButtonStyle, Look, Paint, button_style};

const CLICK: AudioAsset = asset("sound/click.ogg");

const MENU_BUTTON: ButtonStyle = button_style(Look { background: Paint::None, text: text_style(28, WHITE) })
    .hover(Look { background: Paint::Color(rgba(255, 255, 255, 40)), text: text_style(28, WHITE) })
    .change(tween(ms(150)))
    .click_sound(sound(CLICK));
```

画像ボタンでは、ホバー時とフォーカス時に背景画像を差し替えます。

```kis
const fn image_style(idle: ImageAsset, hover: ImageAsset) -> ButtonStyle {
    let text = text_style(1, WHITE);
    button_style(Look { background: Paint::Image(idle), text })
        .hover(Look { background: Paint::Image(hover), text })
        .focus(Look { background: Paint::Image(hover), text })
}
```

## 構造体：`SliderStyle` {#sliderstyle}

```kis
pub struct SliderStyle {
    pub track: Paint,
    pub fill: Paint,
    pub thumb: Paint,
    pub thumb_size: Size,
}
```

[`slider!`](./ui.md#slider) の塗りを指定します。

- `track` — 溝の部分。最小値でのつまみの中心から、最大値でのつまみの中心までです。
- `fill` — 溝の始点からつまみの中心までの部分。`track` の上に描きます。
- `thumb` — つまみ。大きさは `thumb_size` で、値の位置を中心に置きます。
- `thumb_size` — つまみの大きさ（ピクセル）。
