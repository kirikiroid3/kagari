# `std::narrative::text` {#stdnarrativetext}

> **安定性:** 安定 — テキスト値、`t![…]` の展開先のビルトイン、テキスト効果、テキストスタイル。

`std::narrative::text` は、すべてのメッセージ・名前・ラベルの型である `Text`、フォーム `t![…]` が展開されるビルトイン関数、埋め込みの境界である `ToText`、テキストの一部を装飾し表示のきっかけを与えるテキスト効果（`TextEffect` とそのコンストラクタ）、そしてテキストボックス全体のスタイル `TextStyle` を宣言します。

関数はすべて `const fn` です。`Text`、`TextEffect`、`Mark` は [`std::narrative`](./narrative.md) から再エクスポートされます。プレリュードは `Text`、`TextEffect` と、コンストラクタ `emphasis`、`dots`、`color_text`、`bold`、`pause`、`click_wait`、`mark` をインポートします。それ以外は `std::narrative::text` からインポートします。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`Text`](#struct-text)|ビルトイン構造体|定数テキスト（プレーンまたはリッチ）|
|[`ruby`](#function-rubybase-reading)、[`mono_ruby`](#function-mono_rubybase-readings)、[`effect`](#function-effectbase-effect)、[`concat`](#function-concatitems)|ビルトイン `const fn`|`t![…]` の要素の展開先|
|[`ToText`](#trait-totext)、[`to_text`](#function-to_textvalue)|ビルトイン|埋め込み|
|[`Mark`](#struct-mark)、[`Weight`](#enum-weight)、[`Emphasis`](#enum-emphasis)、[`Reveal`](#enum-reveal)、[`Pause`](#enum-pause)、[`Outline`](#struct-outline)、[`Shadow`](#struct-shadow)|データ|テキスト効果の部品|
|[`TextEffect`](#struct-texteffect)、[`PLAIN`](#constant-plain)|構造体、定数|テキスト効果|
|[`color_text` … `mark`](#effect-constructors)|`const fn`|テキスト効果のコンストラクタ|
|[`TextAlign`](#enum-textalign)、[`EmphasisStyle`](#enum-emphasisstyle)、[`TextStyle`](#struct-textstyle)、[`text_style`](#function-text_stylesize-color)|データ、`const fn`|テキストボックスのスタイル|

## Struct: `Text` {#struct-text}

```kis
#[builtin] pub struct Text;
```

定数テキストです：プレーン（`"Mio"`）またはリッチ（`t![…]`）。

- Kind: — · Builtin: yes

- 文字列リテラルの型は `Text` です：`who = "Mio"` に `t!` は要りません。
- `Text` は `Eq` かつ有限型です：物語の中では、`Text` の値はプログラムの定数テキストです。`fn` のコードは [`to_text`](#function-to_textvalue) で実行時に別のテキストを作れます。
- そのため、エントリーシーンの引数と戻り値の型は `Text` を含んではなりません（[`E3009`](../../../reference/ja/src/diagnostics/kinds.md#e3009)）。

## `t![…]` の展開 {#t-desugaring}

`t![…]` はフォームで、各要素が 1 つの呼び出しになります。

|要素|展開先|
|---|---|
|`"…"`|文字列リテラル|
|`["数多"]("あまた")`|`ruby("数多", "あまた")`|
|`["漢字"]("かん"\|"じ")`|`mono_ruby("漢字", ["かん", "じ"])`|
|`!["赤い"](e)`|`effect("赤い", e)`。ルビ要素を基にする場合は `effect(ruby(…), e)`|
|`{expr}`|`to_text(expr)`|

フォーム全体は次のようになります。

- 要素がなければ `""`
- 要素が 1 つならその要素の式。つまり `t![{DAY}]` は `to_text(DAY)`
- それ以外は要素の `concat([…])`

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
// concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

フォームの構文は [フォーム](../../../reference/ja/src/syntax/forms.md) で説明しています。

## Function: `ruby(base, reading)` {#function-rubybase-reading}

```kis
#[builtin] pub const fn ruby(base: Text, reading: Text) -> Text;
```

- `base` `Text` — 親文字。文字列リテラルでなければなりません（[`E3021`](../../../reference/ja/src/diagnostics/kinds.md#e3021)）。
- `reading` `Text` — 読み。
- Returns: `Text` — `base` の上に `reading` をルビとして付けたもの（グループルビ：親文字全体に 1 つの読み）。
- Kind: `const fn` · Builtin: yes

別のルビの親文字の中に入れ子にしたルビは、読みが捨てられます。ルビ辞書（繰り返し出る語のルビを一度だけ宣言するもの）は予定していません：ルビは使う箇所ごとに書きます。

## Function: `mono_ruby(base, readings)` {#function-mono_rubybase-readings}

```kis
#[builtin] pub const fn mono_ruby<const N: usize>(base: Text, readings: [Text; N]) -> Text;
```

- `base` `Text` — 親文字。文字列リテラルでなければなりません。
- `readings` `[Text; N]` — `base` の 1 文字ごとの読み。配列リテラルでなければなりません。
- Returns: `Text` — 1 文字ごとにルビを付けた `base`。
- Kind: `const fn` · Builtin: yes

`N` は親文字の文字数と等しくなければなりません（[`E3021`](../../../reference/ja/src/diagnostics/kinds.md#e3021)）。

## Function: `effect(base, effect)` {#function-effectbase-effect}

```kis
#[builtin] pub const fn effect(base: Text, effect: TextEffect) -> Text;
```

- `base` `Text` — 装飾するテキスト。
- `effect` `TextEffect` — 効果。定数式でなければなりません（[`E3022`](../../../reference/ja/src/diagnostics/kinds.md#e3022)）。
- Returns: `Text` — `effect` を適用した `base`。
- Kind: `const fn` · Builtin: yes

## Function: `concat(items)` {#function-concatitems}

```kis
#[builtin] pub const fn concat<const N: usize>(items: [Text; N]) -> Text;
```

- `items` `[Text; N]` — 部品。
- Returns: `Text` — 要素を順に並べたもの。空白の追加や畳み込みはしません。
- Kind: `const fn` · Builtin: yes

## Trait: `ToText` {#trait-totext}

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

`t![…]` の `{expr}` 要素で埋め込める型です：`Text`、`String`、すべての整数型、`Rat`、`Decimal`。

- Kind: — · Builtin: yes

## Function: `to_text(value)` {#function-to_textvalue}

```kis
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

- `value` `T` — 表示する値。
- Returns: `Text` — テキストにした `value`。`t![…]` の `{expr}` の展開先です。
- Kind: `const fn` · Builtin: yes

- `{expr}` には `T: ToText` である `expr: T` が必要です。
- 整数は十進で、`Rat` と `Decimal` は正確に書き出されます。
- 物語の中では、有限型の値に対する `to_text` は他の `const fn` 呼び出しと同じく表化されるので、`{DAY}` を含む行も物語の状態の有限な関数のままです。
- バックログでは、ステップが `history` を設定していない限り、埋め込んだ値は表示時のまま記録されます。

```kix
static mut DAY: Int<1, 7> = 1;

#[scene]
pushdown fn Morning() {
    s!(t!["Day " {DAY}]);
}
```

## Struct: `Mark` {#struct-mark}

```kis
pub struct Mark(pub u8);
```

テキストの表示の中の名前付きの点で、レンダーはタイムラインをここに合わせられます（`Screen::at_mark`、[`std::screen`](./screen.md) を参照）。[`mark(n)`](#marks) で設定します。`Mark` は Rust の整数を保持するので有限型ではありません。物語は定数式の中で使います。

- Kind: — · Builtin: no

## Enum: `Weight` {#enum-weight}

```kis
pub enum Weight {
    Regular,
    Bold,
}
```

フォントの太さです。

- Kind: — · Builtin: no

## Enum: `Emphasis` {#enum-emphasis}

```kis
pub enum Emphasis {
    Styled,
    Dots,
}
```

強調したテキストの描き方です。

- `Styled` — ウィンドウの `TextStyle::emphasis` で描きます。
- `Dots` — ウィンドウのスタイルにかかわらず、全文字に圏点（傍点）を付けます。
- Kind: — · Builtin: no

## Enum: `Reveal` {#enum-reveal}

```kis
pub enum Reveal {
    Player,
    PerChar(Duration),
    Scaled(Decimal),
    Instant,
}
```

テキストが現れる速さです。

|バリアント|速さ|
|---|---|
|`Player`|`text_speed` 設定|
|`PerChar(d)`|設定にかかわらず 1 文字あたり `d`|
|`Scaled(x)`|プレイヤーの速さ × `x`（`x` > 0）|
|`Instant`|一度に全部|

- Kind: — · Builtin: no

## Enum: `Pause` {#enum-pause}

```kis
pub enum Pause {
    Time(Duration),
    Click,
}
```

効果の基になるテキストの前で、表示を止めます。

- `Time(d)` — `d` の間止めます。
- `Click` — クリックまで止め、そのクリックでテキストの続きを表示します。
- Kind: — · Builtin: no

## Struct: `Outline` {#struct-outline}

```kis
pub struct Outline {
    pub color: Color,
    pub width: u32,
}
```

テキストの縁取りです：`color` と、ピクセル単位の `width`。

- Kind: — · Builtin: no

## Struct: `Shadow` {#struct-shadow}

```kis
pub struct Shadow {
    pub color: Color,
    pub offset: Offset,
    pub blur: u32,
}
```

テキストの影です：`color`、ピクセル単位の `offset`、ピクセル単位のぼかし半径 `blur`。

- Kind: — · Builtin: no

## Struct: `TextEffect` {#struct-texteffect}

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

テキストの一部に適用するスタイルときっかけです。`None` のフィールドは継承します（ウィンドウのスタイルをそのまま使います）。`size` はウィンドウの文字サイズに対する倍率です。

- Kind: — · Builtin: no

テキスト効果は関数ではなく値です。`!["…"](…)` の効果は定数式でなければならないので、メッセージはそのステップの定数のままです。効果は [`and`](#texteffectandself-other) で組み合わせます。

```kix
const RED: Color = rgb(220, 40, 40);

const LINE: Text = t!["It was " !["right there"](bold().and(color_text(RED))) "."];
```

### `TextEffect::and(self, other)` {#texteffectandself-other}

```kis
pub const fn and(self, other: TextEffect) -> TextEffect
```

- `other` `TextEffect` — この効果の上に重ねる効果。
- Returns: `TextEffect` — 右優先のマージ：`other` で設定されたフィールドが勝ち、それ以外は `self` から取ります。
- Kind: `const fn` · Builtin: no

## Constant: `PLAIN` {#constant-plain}

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

何も変えない効果です：すべてのフィールドが `None`。

## 効果のコンストラクタ {#effect-constructors}

各コンストラクタは、`PLAIN` のフィールドを 1 つだけ設定したものです。すべて `const fn` で、Builtin: no です。

|コンストラクタ|設定するもの|プレリュード|
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

スタイルのフィールド（`color`、`size`、`weight`、`italic`、`font`、`outline`、`shadow`、`emphasis`）は、効果の基になるテキストに適用されます。

- `emphasis()` はウィンドウの `TextStyle::emphasis` で描きます。
- `dots()` はウィンドウのスタイルにかかわらず、常に圏点を付けます。

```kix
const LINE: Text = t!["これは" ["数多"]("あまた") "の" !["赤い"](emphasis()) "花。"];
```

### 表示速度 {#reveal}

`speed(per_char)`、`speed_scale(scale)`、`reveal_instant()`。

`reveal` は基になるテキストが現れる速さを設定します。[`Reveal`](#enum-reveal) を参照してください。`speed_scale` には 0 より大きい倍率が必要です。

```kix
use std::narrative::text::speed;

const LINE: Text = t![!["Slowly, slowly…"](speed(ms(80))) " then all at once."];
```

### ポーズ {#pauses}

`pause(duration)`、`click_wait()`。

ポーズは、効果の基になるテキストの前で表示を止めます。基になるテキストは通常 `""` です。

- `pause(d)` は表示を `d` の間止めます。
- `click_wait()` はクリックまで表示を止めます。そのクリックはテキストの続きを表示するもので、テキストを完了させず、`NextStep` でもありません。セーブにも記録されません。
- オートモードでは、クリック待ちのポーズは `auto_delay` 設定の後に続きます。

```kix
const LINE: Text = t!["Wait" ![""](pause(ms(500))) "… " ![""](click_wait()) "there it is."];
```

### マーク {#marks}

`mark(n)`。

マークは表示の中の点に名前を付け、レンダーがそこにアニメーションを予約できるようにします（[`std::screen::anim`](./screen-anim.md) を参照）。

- `mark(n)` は表示がその点に達した時刻を記録します。
- 1 つのテキスト内でマークが重複すると定数評価エラーです（[`E4010`](../../../reference/ja/src/diagnostics/eval.md#e4010)）。
- ステップのテキストに含まれないマークは、表示の終わりとして扱われます。

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

この例では、レンダー `startled` が `screen.at_mark(Mark(1))` を呼び、表示がダッシュに達した瞬間に表情を変えられます。

> **安定性:** 予定 — マークを超えたテキストとアニメーションの同期：表示される 1 文字ごとに追従するアニメーションや、アニメーションを待つ表示など。まだ設計されていません。

### クリックとバックログ {#clicks-and-the-backlog}

- 表示中のクリックは全文を一度に表示します。ただし `click_wait()` のポーズは、それ自身のクリックを引き続き待ちます。
- バックログのテキストはポーズや表示演出なしで表示されます。ステップが `history` を設定していない限り、埋め込んだ値は表示時のまま記録されます。

## Enum: `TextAlign` {#enum-textalign}

```kis
pub enum TextAlign {
    Start,
    Center,
    End,
}
```

テキストの水平方向の揃え方です。

- Kind: — · Builtin: no

## Enum: `EmphasisStyle` {#enum-emphasisstyle}

```kis
pub enum EmphasisStyle {
    Dots,
    Bold,
    Italic,
}
```

テキストボックス内で `Emphasis::Styled` のテキスト（`emphasis()` によるもの）をどう描くかです。

- Kind: — · Builtin: no

## Struct: `TextStyle` {#struct-textstyle}

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

テキストボックスのスタイルです：メッセージウィンドウ（`ui::message`）、UI のラベルと `text!`、画面上のラベルレイヤーに使います。

|フィールド|意味|
|---|---|
|`font`|`None`：`System::font`|
|`size`、`ruby_size`|ピクセル|
|`color`|文字色|
|`line_height`|`size` の倍数|
|`letter_spacing`|文字間のピクセル|
|`align`|水平方向の揃え方|
|`outline`、`shadow`|`None`：なし|
|`emphasis`|`Emphasis::Styled` のテキストの描き方|
|`hanging_indent`|開き括弧（`「『（`）で始まる行は、折り返した行を括弧の幅だけ字下げします|
|`vertical`|縦書き|

- Kind: — · Builtin: no

## Function: `text_style(size, color)` {#function-text_stylesize-color}

```kis
pub const fn text_style(size: u32, color: Color) -> TextStyle
```

- `size` `u32` — ピクセル単位の文字サイズ。
- `color` `Color` — 文字色。
- Returns: `TextStyle` — フォント `None`、`line_height` 1.5、`letter_spacing` 0、揃え `Start`、縁取りと影なし、`ruby_size` = `size / 2`、強調 `Dots`、`hanging_indent` false、`vertical` false のスタイル。
- Kind: `const fn` · Builtin: no

### `TextStyle` のビルダー {#textstyle-builders}

各ビルダーは、フィールドを 1 つ置き換えたスタイルを返します。`Option` のフィールドのビルダーは値を `Some` で包みます。すべて `const fn` で、Builtin: no です。

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

|ビルダー|設定するもの|
|---|---|
|`font(font)`|`font: Some(font)`|
|`line_height(value)`|`line_height`：サイズの `value` 倍|
|`letter_spacing(value)`|`letter_spacing`：文字間のピクセル|
|`align(value)`|`align`|
|`outline(value)`|`outline: Some(value)`|
|`shadow(value)`|`shadow: Some(value)`|
|`ruby_size(value)`|`ruby_size`：ピクセル|
|`emphasis(value)`|`emphasis`：`Emphasis::Styled` のテキストの描き方|
|`hanging_indent(value)`|`hanging_indent`|
|`vertical(value)`|`vertical`：縦書きか横書きか|

```kis
use std::narrative::text::{Outline, TextStyle, text_style};

const WINDOW_TEXT: TextStyle = text_style(26, WHITE)
    .outline(Outline { color: BLACK, width: 2 })
    .hanging_indent(true);
```
