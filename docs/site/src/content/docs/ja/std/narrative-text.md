---
title: "std::narrative::text"
---

> **安定性:** 安定 — テキスト値、`t![…]` の展開先となる組み込み関数、テキスト効果、テキストのスタイル。

`std::narrative::text` では次のものを宣言しています。

- メッセージ、名前、ラベルに使う型 `Text`
- `t![…]` 構文の展開先となる組み込み関数
- 埋め込める値を表すトレイト `ToText`
- テキストの一部を装飾したり、表示のタイミングを指定したりするテキスト効果（`TextEffect` とそのコンストラクター）
- テキストボックス全体のスタイル `TextStyle`

関数はすべて `const fn` です。`Text`、`TextEffect`、`Mark` は [`std::narrative`](./narrative.md) からも再エクスポートしています。プレリュードに入っているのは `Text`、`TextEffect` と、コンストラクター `emphasis`、`dots`、`color_text`、`bold`、`pause`、`click_wait`、`mark` です。それ以外は `std::narrative::text` からインポートしてください。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`Text`](#struct-text)|組み込みの構造体|定数のテキスト（装飾なし、または装飾付き）|
|[`ruby`](#function-rubybase-reading)、[`mono_ruby`](#function-mono_rubybase-readings)、[`effect`](#function-effectbase-effect)、[`concat`](#function-concatitems)|組み込みの `const fn`|`t![…]` の各要素の展開先|
|[`ToText`](#trait-totext)、[`to_text`](#function-to_textvalue)|組み込み|値の埋め込み|
|[`Mark`](#struct-mark)、[`Weight`](#enum-weight)、[`Emphasis`](#enum-emphasis)、[`Reveal`](#enum-reveal)、[`Pause`](#enum-pause)、[`Outline`](#struct-outline)、[`Shadow`](#struct-shadow)|データ|テキスト効果を構成する値|
|[`TextEffect`](#struct-texteffect)、[`PLAIN`](#constant-plain)|構造体、定数|テキスト効果|
|[`color_text` … `mark`](#effect-constructors)|`const fn`|テキスト効果のコンストラクター|
|[`TextAlign`](#enum-textalign)、[`EmphasisStyle`](#enum-emphasisstyle)、[`TextStyle`](#struct-textstyle)、[`text_style`](#function-text_stylesize-color)|データ、`const fn`|テキストボックスのスタイル|

## 構造体：`Text` {#struct-text}

```kis
#[builtin] pub struct Text;
```

定数のテキストです。装飾のないもの（`"Mio"`）と装飾付きのもの（`t![…]`）があります。

- 種別：— · 組み込み：はい

- 文字列リテラルの型は `Text` なので、`who = "Mio"` のように `t!` を付けずに書けます。
- `Text` は `Eq` を実装した有限型です。物語の中で扱う `Text` の値は、プログラム中の定数テキストに限られます。`fn` のコードでは、[`to_text`](#function-to_textvalue) を使って実行時に新しいテキストを作れます。
- このため、開始シーンの引数と戻り値の型に `Text` を含めることはできません（[`E3009`](../diagnostics/kinds.md#e3009)）。

## `t![…]` の展開 {#t-desugaring}

`t![…]` は組み込み構文で、要素ごとに 1 つの関数呼び出しに展開されます。

|要素|展開先|
|---|---|
|`"…"`|文字列リテラル|
|`["数多"]("あまた")`|`ruby("数多", "あまた")`|
|`["漢字"]("かん"\|"じ")`|`mono_ruby("漢字", ["かん", "じ"])`|
|`!["赤い"](e)`|`effect("赤い", e)`。ルビ要素に効果を付けた場合は `effect(ruby(…), e)`|
|`{expr}`|`to_text(expr)`|

構文全体は次のように展開されます。

- 要素がなければ `""`
- 要素が 1 つなら、その要素の式そのもの（`t![{DAY}]` は `to_text(DAY)`）
- 2 つ以上なら、要素を並べた `concat([…])`

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
// concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

構文の書き方は [組み込み構文](../reference/syntax/forms.md) を参照してください。

## 関数：`ruby(base, reading)` {#function-rubybase-reading}

```kis
#[builtin] pub const fn ruby(base: Text, reading: Text) -> Text;
```

- `base` `Text` — 親文字。文字列リテラルでなければなりません（[`E3021`](../diagnostics/kinds.md#e3021)）。
- `reading` `Text` — 読み。
- 戻り値：`Text` — `base` の上に `reading` をルビとして振ったテキスト（親文字全体に 1 つの読みを振るグループルビ）。
- 種別：`const fn` · 組み込み：はい

ルビの親文字の中にさらにルビを入れ子にすると、内側の読みは捨てられます。よく出る語のルビを一度だけ宣言しておくルビ辞書は、予定していません。ルビは使う箇所ごとに書いてください。

## 関数：`mono_ruby(base, readings)` {#function-mono_rubybase-readings}

```kis
#[builtin] pub const fn mono_ruby<const N: usize>(base: Text, readings: [Text; N]) -> Text;
```

- `base` `Text` — 親文字。文字列リテラルでなければなりません。
- `readings` `[Text; N]` — `base` の 1 文字ずつの読み。配列リテラルでなければなりません。
- 戻り値：`Text` — 1 文字ずつルビを振った `base`。
- 種別：`const fn` · 組み込み：はい

`N` が親文字の文字数と一致しないとエラーになります（[`E3021`](../diagnostics/kinds.md#e3021)）。

## 関数：`effect(base, effect)` {#function-effectbase-effect}

```kis
#[builtin] pub const fn effect(base: Text, effect: TextEffect) -> Text;
```

- `base` `Text` — 装飾するテキスト。
- `effect` `TextEffect` — 効果。定数式でなければなりません（[`E3022`](../diagnostics/kinds.md#e3022)）。
- 戻り値：`Text` — `effect` を適用した `base`。
- 種別：`const fn` · 組み込み：はい

## 関数：`concat(items)` {#function-concatitems}

```kis
#[builtin] pub const fn concat<const N: usize>(items: [Text; N]) -> Text;
```

- `items` `[Text; N]` — つなげるテキスト。
- 戻り値：`Text` — 要素を順につなげたテキスト。空白を足したり詰めたりはしません。
- 種別：`const fn` · 組み込み：はい

## トレイト：`ToText` {#trait-totext}

```kis
#[builtin] pub trait ToText {}
impl ToText for Text {}
impl ToText for String {}
impl<const LO: i64, const HI: i64> ToText for Int<LO, HI> {}
impl ToText for u8 {}
impl ToText for u16 {}
impl ToText for u32 {}
impl ToText for u64 {}
impl ToText for u128 {}
impl ToText for i8 {}
impl ToText for i16 {}
impl ToText for i32 {}
impl ToText for i64 {}
impl ToText for i128 {}
impl ToText for usize {}
impl ToText for isize {}
impl ToText for Rat {}
impl ToText for Decimal {}
```

`t![…]` の `{expr}` 要素に埋め込める型です。`Text`、`String`、すべての整数型、`Rat`、`Decimal` が該当します。

- 種別：— · 組み込み：はい

## 関数：`to_text(value)` {#function-to_textvalue}

```kis
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

- `value` `T` — 表示する値。
- 戻り値：`Text` — `value` をテキストにしたもの。`t![…]` の `{expr}` はこの関数に展開されます。
- 種別：`const fn` · 組み込み：はい

- `{expr}` の `expr: T` は `T: ToText` を満たす必要があります。
- 整数は十進表記になり、`Rat` と `Decimal` は値を丸めずにそのまま書き出します。
- 物語の中では、有限型の値に対する `to_text` も他の `const fn` 呼び出しと同様に表化されます。そのため、`{DAY}` を含む行も、物語の状態から有限に決まる値のままです。
- バックログには、ステップが `history` を指定していない限り、埋め込んだ値が表示したときのまま記録されます。

```kix
static mut DAY: Int<1, 7> = 1;

#[scene]
pushdown fn Morning() {
    s!(t!["Day " {DAY}]);
}
```

## 構造体：`Mark` {#struct-mark}

```kis
pub struct Mark(pub u8);
```

テキストの表示中の一点に付ける名前です。レンダーはこの点にタイムラインを合わせられます（`Screen::at_mark`。[`std::screen`](./screen.md) を参照）。設定には [`mark(n)`](#marks) を使います。`Mark` は Rust の整数を持つので有限型ではなく、物語では定数式の中で使います。

- 種別：— · 組み込み：いいえ

## 列挙型：`Weight` {#enum-weight}

```kis
pub enum Weight {
    Regular,
    Bold,
}
```

フォントの太さです。

- 種別：— · 組み込み：いいえ

## 列挙型：`Emphasis` {#enum-emphasis}

```kis
pub enum Emphasis {
    Styled,
    Dots,
}
```

強調するテキストの描き方です。

- `Styled` — ウィンドウの `TextStyle::emphasis` に従って描きます。
- `Dots` — ウィンドウのスタイルに関係なく、すべての文字に圏点（傍点）を付けます。
- 種別：— · 組み込み：いいえ

## 列挙型：`Reveal` {#enum-reveal}

```kis
pub enum Reveal {
    Player,
    PerChar(Duration),
    Scaled(Decimal),
    Instant,
}
```

テキストを表示していく速さです。

|バリアント|速さ|
|---|---|
|`Player`|`text_speed` 設定に従う|
|`PerChar(d)`|設定に関係なく 1 文字あたり `d`|
|`Scaled(x)`|プレイヤーの設定した速さの `x` 倍（`x` > 0）|
|`Instant`|一度にすべて表示|

- 種別：— · 組み込み：いいえ

## 列挙型：`Pause` {#enum-pause}

```kis
pub enum Pause {
    Time(Duration),
    Click,
}
```

効果を付けたテキストの手前で、表示を一時停止します。

- `Time(d)` — `d` の間止めます。
- `Click` — クリックされるまで止めます。そのクリックで続きのテキストを表示します。
- 種別：— · 組み込み：いいえ

## 構造体：`Outline` {#struct-outline}

```kis
pub struct Outline {
    pub color: Color,
    pub width: u32,
}
```

文字の縁取りです。色 `color` と、ピクセル単位の太さ `width` を指定します。

- 種別：— · 組み込み：いいえ

## 構造体：`Shadow` {#struct-shadow}

```kis
pub struct Shadow {
    pub color: Color,
    pub offset: Offset,
    pub blur: u32,
}
```

文字の影です。色 `color`、ピクセル単位のずらし幅 `offset`、ピクセル単位のぼかし半径 `blur` を指定します。

- 種別：— · 組み込み：いいえ

## 構造体：`TextEffect` {#struct-texteffect}

```kis
pub struct TextEffect {
    pub color: Option<Color>,
    pub size: Option<Decimal>,
    pub weight: Option<Weight>,
    pub italic: Option<bool>,
    pub font: Option<FontAsset>,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
    pub emphasis: Option<Emphasis>,
    pub reveal: Option<Reveal>,
    pub pause: Option<Pause>,
    pub mark: Option<Mark>,
}
```

テキストの一部に適用するスタイルと表示のタイミングです。`None` のフィールドは外側の設定（ウィンドウのスタイル）を引き継ぎます。`size` はウィンドウの文字サイズに対する倍率です。

- 種別：— · 組み込み：いいえ

テキスト効果は関数ではなく値です。`!["…"](…)` に渡す効果は定数式でなければならないので、メッセージはステップごとに決まった定数のままです。複数の効果は [`and`](#texteffectandself-other) で組み合わせます。

```kix
const RED: Color = rgb(220, 40, 40);

const LINE: Text = t!["It was " !["right there"](bold().and(color_text(RED))) "."];
```

### `TextEffect::and(self, other)` {#texteffectandself-other}

```kis
pub const fn and(self, other: TextEffect) -> TextEffect
```

- `other` `TextEffect` — この効果に重ねる効果。
- 戻り値：`TextEffect` — 右側を優先して合わせた効果。`other` で設定したフィールドが優先され、それ以外は `self` の値を使います。
- 種別：`const fn` · 組み込み：いいえ

## 定数：`PLAIN` {#constant-plain}

```kis
pub const PLAIN: TextEffect = TextEffect {
    color: None,
    size: None,
    weight: None,
    italic: None,
    font: None,
    outline: None,
    shadow: None,
    emphasis: None,
    reveal: None,
    pause: None,
    mark: None,
};
```

何も変えない効果です。すべてのフィールドが `None` です。

## 効果のコンストラクター {#effect-constructors}

どのコンストラクターも、`PLAIN` のフィールドを 1 つだけ設定した効果を返します。すべて `const fn` で、組み込みではありません。

|コンストラクター|設定するフィールド|プレリュード|
|---|---|---|
|`pub const fn color_text(color: Color) -> TextEffect`|`color`|あり|
|`pub const fn size(scale: Decimal) -> TextEffect`|`size`：ウィンドウの文字サイズの `scale` 倍|なし|
|`pub const fn bold() -> TextEffect`|`weight: Some(Bold)`|あり|
|`pub const fn italic() -> TextEffect`|`italic: Some(true)`|なし|
|`pub const fn font(font: FontAsset) -> TextEffect`|`font`|なし|
|`pub const fn outline(color: Color, width: u32) -> TextEffect`|`outline`|なし|
|`pub const fn shadow(color: Color, offset: Offset, blur: u32) -> TextEffect`|`shadow`|なし|
|`pub const fn emphasis() -> TextEffect`|`emphasis: Some(Styled)`|あり|
|`pub const fn dots() -> TextEffect`|`emphasis: Some(Dots)`|あり|
|`pub const fn speed(per_char: Duration) -> TextEffect`|`reveal: Some(PerChar(per_char))`|なし|
|`pub const fn speed_scale(scale: Decimal) -> TextEffect`|`reveal: Some(Scaled(scale))`|なし|
|`pub const fn reveal_instant() -> TextEffect`|`reveal: Some(Instant)`|なし|
|`pub const fn pause(duration: Duration) -> TextEffect`|`pause: Some(Time(duration))`|あり|
|`pub const fn click_wait() -> TextEffect`|`pause: Some(Click)`|あり|
|`pub const fn mark(n: u8) -> TextEffect`|`mark: Some(Mark(n))`|あり|

### スタイル {#styles}

`color_text(color)`、`size(scale)`、`bold()`、`italic()`、`font(font)`、`outline(color, width)`、`shadow(color, offset, blur)`、`emphasis()`、`dots()`。

スタイルのフィールド（`color`、`size`、`weight`、`italic`、`font`、`outline`、`shadow`、`emphasis`）は、効果を付けたテキストに適用されます。

- `emphasis()` はウィンドウの `TextStyle::emphasis` に従って描きます。
- `dots()` はウィンドウのスタイルに関係なく、常に圏点を付けます。

```kix
const LINE: Text = t!["これは" ["数多"]("あまた") "の" !["赤い"](emphasis()) "花。"];
```

### 表示速度 {#reveal}

`speed(per_char)`、`speed_scale(scale)`、`reveal_instant()`。

`reveal` は、効果を付けたテキストを表示していく速さを指定します。詳しくは [`Reveal`](#enum-reveal) を参照してください。`speed_scale` の倍率は 0 より大きくなければなりません。

```kix
use std::narrative::text::speed;

const LINE: Text = t![!["Slowly, slowly…"](speed(ms(80))) " then all at once."];
```

### 一時停止 {#pauses}

`pause(duration)`、`click_wait()`。

効果を付けたテキストの手前で表示を一時停止します。効果を付けるテキストは、ふつうは `""` です。

- `pause(d)` は表示を `d` の間止めます。
- `click_wait()` はクリックされるまで表示を止めます。このクリックは続きを表示するためのもので、テキストを最後まで表示するわけではなく、`NextStep` にもなりません。セーブにも記録されません。
- オートモードでは、クリック待ちの一時停止は `auto_delay` 設定の時間が過ぎると先へ進みます。

```kix
const LINE: Text = t!["Wait" ![""](pause(ms(500))) "… " ![""](click_wait()) "there it is."];
```

### マーク {#marks}

`mark(n)`。

マークは表示中の一点に名前を付けるもので、レンダーはその時点にアニメーションを予約できます（[`std::screen::anim`](./screen-anim.md) を参照）。

- `mark(n)` は、表示がその位置に達した時刻を記録します。
- 1 つのテキストの中で同じマークを 2 回使うと、定数評価のエラーになります（[`E4010`](../diagnostics/eval.md#e4010)）。
- ステップのテキストに含まれないマークは、表示の終わりとして扱います。

```kix
use std::narrative::Mark;

const fn startled(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(1));
    // changes scheduled here start when the reveal reaches mark 1
    screen
}

#[scene]
pushdown fn Surprise() {
    s!(render = startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
}
```

この例では、レンダー `startled` が `screen.at_mark(Mark(1))` を呼ぶことで、表示がダッシュに達した瞬間に表情を変えられます。

> **安定性:** 予定 — マークより細かいテキストとアニメーションの同期。表示される 1 文字ごとに追従するアニメーションや、アニメーションの終了を待つ表示などで、まだ設計していません。

### クリックとバックログ {#clicks-and-the-backlog}

- 表示の途中でクリックすると、全文を一度に表示します。ただし `click_wait()` による一時停止では、それとは別にクリックを待ちます。
- バックログのテキストは、一時停止や表示の演出なしで表示します。ステップが `history` を指定していない限り、埋め込んだ値は表示したときのまま記録されます。

## 列挙型：`TextAlign` {#enum-textalign}

```kis
pub enum TextAlign {
    Start,
    Center,
    End,
}
```

テキストの水平方向の揃え方です。

- 種別：— · 組み込み：いいえ

## 列挙型：`EmphasisStyle` {#enum-emphasisstyle}

```kis
pub enum EmphasisStyle {
    Dots,
    Bold,
    Italic,
}
```

テキストボックスの中で、`Emphasis::Styled` のテキスト（`emphasis()` を付けたもの）をどう描くかを指定します。

- 種別：— · 組み込み：いいえ

## 構造体：`TextStyle` {#struct-textstyle}

```kis
pub struct TextStyle {
    pub font: Option<FontAsset>,
    pub size: u32,
    pub color: Color,
    pub line_height: Decimal,
    pub letter_spacing: i32,
    pub align: TextAlign,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
    pub ruby_size: u32,
    pub emphasis: EmphasisStyle,
    pub hanging_indent: bool,
    pub vertical: bool,
}
```

テキストボックスのスタイルです。メッセージウィンドウ（`ui::message`）、UI のラベルと `text!`、画面上のラベルレイヤーで使います。

|フィールド|意味|
|---|---|
|`font`|`None` なら `System::font`|
|`size`、`ruby_size`|ピクセル数|
|`color`|文字色|
|`line_height`|`size` に対する倍率|
|`letter_spacing`|文字間隔（ピクセル）|
|`align`|水平方向の揃え方|
|`outline`、`shadow`|`None` ならなし|
|`emphasis`|`Emphasis::Styled` のテキストの描き方|
|`hanging_indent`|開き括弧（`「『（`）で始まる行では、折り返した行を括弧 1 文字分下げる|
|`vertical`|縦書き|

- 種別：— · 組み込み：いいえ

## 関数：`text_style(size, color)` {#function-text_stylesize-color}

```kis
pub const fn text_style(size: u32, color: Color) -> TextStyle
```

- `size` `u32` — 文字サイズ（ピクセル）。
- `color` `Color` — 文字色。
- 戻り値：`TextStyle` — 次の設定のスタイル。フォント `None`、`line_height` 1.5、`letter_spacing` 0、揃え `Start`、縁取りと影なし、`ruby_size` = `size / 2`、強調 `Dots`、`hanging_indent` false、`vertical` false。
- 種別：`const fn` · 組み込み：いいえ

### `TextStyle` のビルダー {#textstyle-builders}

各ビルダーは、フィールドを 1 つだけ置き換えたスタイルを返します。`Option` のフィールドでは、値を `Some` で包んで設定します。すべて `const fn` で、組み込みではありません。

```kis
pub const fn font(mut self, font: FontAsset) -> TextStyle
pub const fn line_height(mut self, value: Decimal) -> TextStyle
pub const fn letter_spacing(mut self, value: i32) -> TextStyle
pub const fn align(mut self, value: TextAlign) -> TextStyle
pub const fn outline(mut self, value: Outline) -> TextStyle
pub const fn shadow(mut self, value: Shadow) -> TextStyle
pub const fn ruby_size(mut self, value: u32) -> TextStyle
pub const fn emphasis(mut self, value: EmphasisStyle) -> TextStyle
pub const fn hanging_indent(mut self, value: bool) -> TextStyle
pub const fn vertical(mut self, value: bool) -> TextStyle
```

|ビルダー|設定するフィールド|
|---|---|
|`font(font)`|`font: Some(font)`|
|`line_height(value)`|`line_height`：サイズの `value` 倍|
|`letter_spacing(value)`|`letter_spacing`：文字間隔（ピクセル）|
|`align(value)`|`align`|
|`outline(value)`|`outline: Some(value)`|
|`shadow(value)`|`shadow: Some(value)`|
|`ruby_size(value)`|`ruby_size`：ピクセル数|
|`emphasis(value)`|`emphasis`：`Emphasis::Styled` のテキストの描き方|
|`hanging_indent(value)`|`hanging_indent`|
|`vertical(value)`|`vertical`：縦書きか横書きか|

```kis
use std::narrative::text::{Outline, TextStyle, text_style};

const WINDOW_TEXT: TextStyle = text_style(26, WHITE)
    .outline(Outline { color: BLACK, width: 2 })
    .hanging_indent(true);
```
