# `std::ui::style` {#stduistyle}

> **安定性:** 安定 — UI コントロールの見た目。

`std::ui::style` は背景とコントロールの見た目を記述します。[`Paint`](#paint) はボックスを塗り、[`Look`](#look) は背景とテキストスタイルの組、[`ButtonStyle`](#buttonstyle) はボタン・トグル・セレクトのインタラクション状態ごとの見た目、[`SliderStyle`](#sliderstyle) はスライダーの塗りです。

スタイルは `const fn` で作る普通のデータです。スタジオはスタイルを `const` アイテムとして宣言し、コントロールフォームの `style` 属性や [`Element::background`](./ui.md#element-background) に渡します。

```kis
use std::ui::style::{ButtonStyle, Look, Paint, SliderStyle, button_style};
```

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Paint`](#paint) | 列挙型 | ボックスを埋めるもの |
| [`Look`](#look) | 構造体 | 背景とテキストスタイル |
| [`ButtonStyle`](#buttonstyle) | 構造体 | ボタン型コントロールのインタラクション状態ごとの見た目 |
| [`button_style(idle)`](#button_style) | `const fn` | 通常時の見た目だけを持つスタイル |
| [`ButtonStyle` のビルダーメソッド](#buttonstyle-hover) | `const fn` | 見た目、トゥイーン、サウンドを追加する |
| [`SliderStyle`](#sliderstyle) | 構造体 | スライダーの塗り |

## 列挙型: `Paint` {#paint}

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
| `None` | 何もなし。ボックスは透明です |
| `Color(c)` | 単色。例: `rgba(0, 0, 0, 200)` |
| `Image(i)` | ボックス全体に引き伸ばした画像 |
| `NineSlice { image, insets }` | 9 スライス画像。`insets` で指定した大きさの四隅は拡大縮小されず、辺と中央が伸びます。サイズが変わるウィンドウ枠やボタンに使います |

## 構造体: `Look` {#look}

```kis
pub struct Look {
    pub background: Paint,
    pub text: TextStyle,
}
```

背景とテキストスタイルです。`text` のスタイルはコントロール内のラベルに適用されます（[インタラクションの見た目](#interaction-looks) を参照）。

## 構造体: `ButtonStyle` {#buttonstyle}

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

ボタン型のコントロール（`button!`、`toggle!`、`select!`）の、インタラクション状態ごとの見た目、その間の変化のアニメーション、再生するサウンドです。UI のサウンドはサウンドバスで再生されます。

### インタラクションの見た目 {#interaction-looks}

- コントロールが無効な間は、`disabled` の見た目が他のすべての見た目を置き換えます。`disabled` の見た目がなければ、無効なコントロールは `idle` を半分の不透明度で表示します。
- そうでなければ、`pressed`（コントロールの上でポインタを押している間）、`focus`（コントロールにフォーカスがあり、フォーカスが表示されている間）、`hover` のうち当てはまる最初の `Some` が使われ、`idle` が既定です。
- 値が `true` のトグルが `idle` を表示するはずのときは、代わりに **オンの見た目** を表示します。`pressed` の見た目があればそれ、なければ `hover` の見た目、それもなければ `idle` に、通常時のテキスト色で 2 px の枠線を付けたものです。
- 見た目の変化は `change` でアニメーションします。
- ボタンやトグルの中のラベルは、現在の見た目の `text` を使います。それ以外では、`text!` に `style` を指定しない限り、`label` と `text!` は [`System::text`](./system.md#system) を使います。これ以外の継承はありません。

### 関数: `button_style(idle)` {#button_style}

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
- 戻り値: `ButtonStyle` — 通常時の見た目だけを持ち、変化は即時で、サウンドのないスタイル。
- 種別: `const fn` · ビルトイン: いいえ

その他の部分は以下のビルダーメソッドで追加します。どれもスタイルを値で受け取り、変更して返すので、連鎖できます。

### `ButtonStyle::hover(look)` {#buttonstyle-hover}

```kis
pub const fn hover(mut self, look: Look) -> ButtonStyle
```

- 戻り値: ホバー時の見た目を加えたこのスタイル。

### `ButtonStyle::focus(look)` {#buttonstyle-focus}

```kis
pub const fn focus(mut self, look: Look) -> ButtonStyle
```

- 戻り値: フォーカス時の見た目を加えたこのスタイル。コントロールがキーボードやパッドのフォーカスを持つ間に表示されます。

### `ButtonStyle::pressed(look)` {#buttonstyle-pressed}

```kis
pub const fn pressed(mut self, look: Look) -> ButtonStyle
```

- 戻り値: 押下時の見た目を加えたこのスタイル。

### `ButtonStyle::disabled(look)` {#buttonstyle-disabled}

```kis
pub const fn disabled(mut self, look: Look) -> ButtonStyle
```

- 戻り値: 無効時の見た目を加えたこのスタイル。

### `ButtonStyle::change(tween)` {#buttonstyle-change}

```kis
pub const fn change(mut self, tween: Tween) -> ButtonStyle
```

- `tween` `Tween` — 見た目の変化の長さ、イージング、遅延（[`std::screen::anim`](./screen-anim.md) を参照）。
- 戻り値: 見た目の変化を `tween` でアニメーションするこのスタイル。

### `ButtonStyle::hover_sound(sound)` {#buttonstyle-hover_sound}

```kis
pub const fn hover_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — ワンショットのサウンド（[`std::screen::audio`](./screen-audio.md) を参照）。
- 戻り値: ホバー時に `sound` を再生するこのスタイル。

### `ButtonStyle::click_sound(sound)` {#buttonstyle-click_sound}

```kis
pub const fn click_sound(mut self, sound: Sound) -> ButtonStyle
```

- `sound` `Sound` — ワンショットのサウンド。
- 戻り値: クリック時に `sound` を再生するこのスタイル。

ビルダーメソッドはすべて `const fn` · ビルトイン: いいえ です。

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

画像ボタンは、ホバー時とフォーカス時に背景画像を差し替えます。

```kis
const fn image_style(idle: ImageAsset, hover: ImageAsset) -> ButtonStyle {
    let text = text_style(1, WHITE);
    button_style(Look { background: Paint::Image(idle), text })
        .hover(Look { background: Paint::Image(hover), text })
        .focus(Look { background: Paint::Image(hover), text })
}
```

## 構造体: `SliderStyle` {#sliderstyle}

```kis
pub struct SliderStyle {
    pub track: Paint,
    pub fill: Paint,
    pub thumb: Paint,
    pub thumb_size: Size,
}
```

[`slider!`](./ui.md#slider) の塗りです。

- `track` — トラック。最小値でのつまみの中心から、最大値でのつまみの中心まで。
- `fill` — トラックの始点からつまみの中心までの部分。`track` の上に描かれます。
- `thumb` — つまみ。大きさは `thumb_size` で、値の位置を中心に置かれます。
- `thumb_size` — つまみのピクセル単位のサイズ。
