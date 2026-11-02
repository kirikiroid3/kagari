---
title: "組み込み構文"
---

`s!(…)`、`choice!{…}`、`t![…]`、そして `column![…]` や `button!{…}` などの UI 構文をまとめて**組み込み構文**と呼びます。どれもステップ、テキスト、UI の部品を組み立てるための決まった構文です。組み込み構文は、普通の構造体式、クロージャ、標準ライブラリのアイテムの呼び出しに展開されます。`.kis` ファイルでは、展開後のコードを直接書くこともできます。

組み込み構文はマクロではありません。Kagari にはマクロがなく、組み込み構文の種類は言語で決まっています。存在しない名前を使うと構文エラー（`E0001`）になります。

ステップが物語の中でどんな意味を持つかは[ステップ](../concepts/steps.md)で、UI 構文が何を組み立てるかは標準ライブラリのリファレンスの [`std::ui`](../../std/ui.md) で説明しています。この章では、組み込み構文の厳密な構文と展開を示します。

## 使える場所 {#where-forms-are-allowed}

- 組み込み構文を使えるのは `.kix` ファイルだけです。`.kis` ファイルで使うとエラー `E0001` になります。
- 組み込み構文は式で、名前、`!`、続いて `(`、`[`、`{` のいずれかという形をとります。`a != b` が組み込み構文と解釈されることはありません。
- 展開は名前解決より前に行います。展開後のコードは、同じ名前のローカルなアイテムがあっても、常に標準ライブラリのアイテム（`std::narrative::push_step`、`std::ui::button` など）を指します。
- 展開した後は通常の規則が適用されます。たとえば、ステップはシーンの中でしか実行できず（`E3002`）、UI の関数は `fn` のコードとして扱われます（[呼び出し規則](../concepts/kinds.md#call-rules)）。

## 構文の全体像 {#general-syntax}

```text
form       = name "!" "(" entries ")"
           | name "!" "[" entries "]"
           | name "!" "{" entries "}"
           | "t" "!" "[" { text-item } "]"
entries    = [ entry { "," entry } [ "," ] ]
entry      = attribute | arm | expression
attribute  = name "=" expression
arm        = expression [ "if" expression ] "=>" expression
text-item  = string | ruby | effect | "{" expression "}"
ruby       = "[" string "]" "(" string { "|" string } ")"
effect     = "!" "[" ( string | ruby ) "]" "(" expression ")"
```

括弧の種類によって、中に書けるものが決まります。

| 括弧 | 書けるもの | 組み込み構文 |
| ---------- | -------------- | -------- |
| `name!( … )` | 属性、続いて決まった数の要素 | `s!`、`text!`、`image!`、`game_screen!` |
| `name![ … ]` | 要素のみ（属性は不可） | `t!`、`column!`、`row!`、`layer!` |
| `name!{ … }` | 属性、続いて要素 | `choice!`、`button!`、`slider!`、`toggle!`、`select!` |

組み込み構文ごとに使う括弧は一種類に決まっています。次の場合は構文エラー（`E0001`）になります。

- 括弧の種類が違う（`` `s!` is written `s!(…)` ``）
- `[…]` の構文に属性がある、要素の後に属性がある、属性が重複している、その構文にない属性を使った、必須の属性がない
- 要素の数が合わない
- `choice!` 以外でアーム（`… => …`）を書いた
- `t![…]` の項目の間にカンマを入れた（項目は並べて書きます）

`key = expression` が属性として扱われるのは、組み込み構文の直下に書いた場合だけです。それ以外の場所の `=` は代入です。各項目はカンマで区切り、末尾にカンマを付けても構いません。

文の先頭に置いた波括弧の構文（`choice!{…}`、`button!{…}` など）はブロック状の式なので、後ろに `;` は要りません。`s!(…)` などのほかの構文には、普通の式文と同じく `;` が必要です。

### 一覧 {#summary-of-the-forms}

| 組み込み構文 | 属性 | 要素 | 展開後 |
| -------- | ---- | ---- | ------ |
| `s!(…)` | `who`、`voice`、`render`、`history` | `t![…]` を最大一つ | `push_step(Say { … })` |
| `choice!{…}` | `id`（必須）、`render` | アーム `t![…] => Variant`、`t![…] if guard => Variant` | `push_step(Choice { … })` |
| `t![…]` | — | テキストの項目を並べたもの | `ruby`、`mono_ruby`、`effect`、`to_text` の呼び出しを `concat([…])` でつないだもの |
| `column![…]`、`row![…]` | — | 任意の数 | `ui::column([…])`、`ui::row([…])` |
| `layer![…]` | — | 任意の数 | `ui::layer([…])` |
| `text!(…)` | `style` | ちょうど一つ | `ui::text(TextProps { … })` |
| `image!(…)` | `fit` | ちょうど一つ | `ui::image(ImageProps { … })` |
| `game_screen!(…)` | `game`（必須） | なし | `ui::game_screen(GameScreenProps { … })` |
| `button!{…}` | `on_click`、`disabled`、`style` | 任意の数 | `ui::button(ButtonProps { … })` |
| `slider!{…}` | `value`（必須）、`on_change`、`step`、`disabled`、`style` | なし | `ui::slider(SliderProps { … })` |
| `toggle!{…}` | `value`（必須）、`on_change`、`disabled`、`style` | 任意の数 | `ui::toggle(ToggleProps { … })` |
| `select!{…}` | `value`（必須）、`on_change`、`disabled`、`style` | 任意の数 | `ui::select(SelectProps { … })` |

## `s!(…)` {#s}

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
s!(who = "Azusa", history = t!["(Azusa names the date)"], t!["Today is day " {DAY} "."]);
```

| 属性 | 型 | 意味 |
| ---- | -- | ---- |
| `who` | `Text` | 話者の名前。省略すると地の文になります |
| `voice` | `VoiceAsset` | メッセージと一緒に再生するボイス |
| `render` | `Render` | ステップに入るときの画面の変化 |
| `history` | `Text` | メッセージの代わりにバックログに残すテキスト |

属性はどれも省略できます。要素を書く場合は `t![…]` 構文で書く必要があり（それ以外はエラー `E0001`）、その値がフィールド `text` になります。定数に入れた `Text` を表示したいときは、`t![{GREETING}]` のように値を埋め込みます。

各フィールドは、対応する属性や要素があれば `Some(value)`、なければ `None` になり、構文全体は `push_step` の呼び出しになります。たとえば

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
```

は、次のコードと同じ意味です。

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: None,
    render: Some(azusa_smiles),
    text: Some("Did you forget again?"),
    history: None,
});
```

（`t!["Did you forget again?"]` は文字列そのものになります。[`t![…]`](#t) を参照してください。）この式の型は `()` です。

## `choice!{…}` {#choice}

```kix
enum Confess { Yes, No }

let answer = choice!{
    id = "confess",
    render = azusa_waits,
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] if !MET_MIO => Confess::No,
};
```

| 属性 | 型 | 必須 | 規則 |
| ---- | -- | ---- | ---- |
| `id` | 文字列リテラル | はい | ない、または文字列リテラルでなければ `E0001`。同じ関数で二度使うと `E3019` |
| `render` | `Render` | いいえ | `s!` と同じ |

要素はすべて**アーム**で、`t![…] => Variant` か `t![…] if guard => Variant` の形で書きます。

- テキストは `t![…]` 構文で書きます。文字列リテラルをそのまま書くとエラー `E0001` になります。
- ガードは `bool` の式で、選択肢に入った時点で評価します。ガードが偽のアームは表示されません。
- 値はフィールドのない enum バリアントでなければなりません（`E3020`）。すべてのアームで同じ enum のバリアントを使い（違えば `E2001`）、同じバリアントは一度しか使えません（`E3020`）。`choice!` 式の型はその enum になります。

この構文は、アームごとに `ChoiceArm` を一つずつソースの順に並べた `Choice` に展開されます。ガードのないアームは `shown: true` です。上の例は次のコードと同じ意味です。

```kis
let answer = push_step(Choice {
    id: "confess",
    render: Some(azusa_waits),
    arms: [
        ChoiceArm { text: "Tell her the truth", shown: true, value: Confess::Yes },
        ChoiceArm { text: "Say nothing", shown: !MET_MIO, value: Confess::No },
    ],
});
```

すべてのアームが隠れた状態で到達しうる選択肢はエラー `E5003` です。ガード、無効なアーム、選択肢 ID を安定させる方法は[ステップ](../concepts/steps.md#choice-choice-step)で説明しています。

## `t![…]` {#t}

```kix
t!["これは" ["数多"]("あまた") "の" ["漢字"]("かん"|"じ") !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

`t![…]` は `Text` を作ります。項目はカンマを入れずに並べて書き、書いたとおりに連結されます。項目の間に空白を足したり削ったりはしません。

| 項目 | 意味 | 展開後 |
| ---- | ---- | ------ |
| `"…"` | 普通のテキスト | 文字列リテラルそのもの |
| `["数多"]("あまた")` | 親文字全体に読みを一つ振るルビ（グループルビ） | `ruby("数多", "あまた")` |
| `["漢字"]("かん"\|"じ")` | 親文字の一文字ごとに読みを振るルビ（モノルビ） | `mono_ruby("漢字", ["かん", "じ"])` |
| `!["赤い"](e)` | テキスト効果 `e` を付けた親文字 | `effect("赤い", e)` |
| `![["数多"]("あまた")](e)` | テキスト効果付きのルビ | `effect(ruby("数多", "あまた"), e)` |
| `{expr}` | `expr` の値をテキストにしたもの | `to_text(expr)` |

構文全体は次の式になります。

- 項目がなければ `""`
- 項目が一つなら、その項目の式（`t!["Mio"]` は `"Mio"`、`t![{DAY}]` は `to_text(DAY)`）
- それ以外は、項目を `concat([…])` でつないだもの

たとえば

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
```

は、次のコードと同じ意味です。

```kis
concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

`ruby`、`mono_ruby`、`effect`、`concat`、`to_text` は、[`std::narrative::text`](../../std/narrative-text.md) にある組み込みの `const fn` です。

### 文字列 {#strings}

文字列の項目は文字列リテラルで、エスケープ `\n`（改行）、`\r`、`\t`、`\\`、`\"`、`\0`、`\u{…}` が使えます。引用符の中で直接改行した場合も、その改行はテキストに含まれます。文字列リテラルはどこでも `Text` 型なので、`who = "Mio"` のような属性に `t!` を付ける必要はありません。

### ルビ {#ruby}

`[base](readings)` は、文字列リテラル一つからなる `base` にルビを振ります。

- 読みが一つの場合：親文字全体に振るグループルビになります。
- `|` で区切って読みを複数書いた場合：親文字の一文字ごとに読みを振るモノルビになります。読みの数は文字数と一致しなければなりません（`E3021`）。

### テキスト効果 {#text-effects}

`![base](effect)` は、`base`（文字列リテラルかルビの項目）を**テキスト効果**付きで表示します。`effect` は `TextEffect` 型の式で、[定数式](./constants.md)でなければなりません（`E3022`）。これにより、メッセージは引き続きステップの定数として扱えます。効果は `.and(…)` で組み合わせられ、同じフィールドを設定した場合は右側が優先されます。親文字の中には値を埋め込めません。

```kix
t!["Tell me about " !["you"](emphasis()) "."]
t!["It was " !["here"](dots().and(color_text(rgb(200, 0, 0)))) "."]
```

使える効果（`color_text`、`bold`、`italic`、`size`、`font`、`outline`、`shadow`、`emphasis`、`dots`、`speed`、`speed_scale`、`reveal_instant`、`pause`、`click_wait`、`mark`）の一覧は、標準ライブラリのリファレンスの [`std::narrative::text`](../../std/narrative-text.md) にあります。

### 一時停止とマーク {#pauses-and-marks}

**キュー**とは、テキストの見た目ではなく、文字を表示していく進み方に作用する効果です。親文字には普通、空文字列を使います。

```kix
s!(who = "Mio", render = mio_startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
s!(t!["Read slowly." ![""](click_wait()) "\nThen go on."]);
```

| キュー | 働き |
| ------ | ---- |
| `pause(d)` | 親文字の手前で表示を `d` の間止める |
| `click_wait()` | クリックされるまで表示を止める。クリックすると同じステップのままテキストの続きを表示 |
| `mark(n)` | 表示が親文字に届いた瞬間に名前を付ける。レンダーはその時点にアニメーションを予約できる |

一つのテキストで同じマーク番号を二度使うとエラー `E4010` になります。表示、一時停止、マークの厳密な規則は、標準ライブラリのリファレンスの [`std::narrative::text`](../../std/narrative-text.md) を参照してください。

### 値の埋め込み {#interpolation}

`{expr}` は、`expr` の値をテキストとして埋め込みます。`expr` の型は標準トレイト `ToText` を実装している必要があります（実装していなければ `E2026`）。実装しているのは `Text`、`String`、`Int<lo, hi>`、Rust の整数型、`Rat`、`Decimal` です。`pushdown fn` の中で物語変数のような有限の実行時の値を埋め込むと、有限の引数で `const fn` の `to_text` を呼ぶことになり、コンパイラがその呼び出しを[表化](../concepts/kinds.md#calling-const-fn-from-the-story)します。

## UI 構文 {#ui-forms}

```kix
column![
    text!(t!["Text speed: " {speed()}]),
    slider!{value = speed(), on_change = |v| set_speed(v)},
    button!{on_click = || nav::back(), t!["Back"]},
]
```

UI 構文は、`fn` のコードの中で [`std::ui`](../../std/ui.md) の `Element` 値を組み立てます。展開の規則は次のとおりです。

- **要素：** 各要素 `e` はクロージャ `|| e` になります。`column!`、`row!`、`button!`、`toggle!` では、`t![…]` 構文で書いた要素は `|| ui::label(t![…])` になります。
- **リアクティブな属性：** 名前が `on_` で始まらない属性は、すべてクロージャになります。必須の属性なら `key: || expr`、省略可能な属性なら `key: Some(|| expr)` です。UI は、クロージャが読んだシグナルが変わるたびにそのクロージャを実行し直します。
- **ハンドラの属性：** `on_*` の属性は、書いた式をそのまま `key: Some(expr)` として渡します。
- **省略した属性：** 省略可能な属性を書かなかった場合は `None` になります。
- `column!`、`row!`、`layer!` は、要素のクロージャの配列をそのまま渡します。それ以外の構文は props 構造体を渡します。props 構造体のフィールドは、`std::ui` で宣言した順の属性と、要素用のフィールドです。

| 組み込み構文 | 展開後 | 属性とその型 | 要素 |
| -------- | ------ | ------------ | ---- |
| `column![…]`、`row![…]` | `ui::column(children)`、`ui::row(children)` | なし | `Element`。任意の数 |
| `layer![…]` | `ui::layer(children)` | なし | `Placed`。任意の数。`t![…]` はラベルに変換しない |
| `text!(…)` | `ui::text(TextProps { … })` | `style`: `TextStyle` | `Text` をちょうど一つ → `content` |
| `image!(…)` | `ui::image(ImageProps { … })` | `fit`: `Fit` | `ImageAsset` をちょうど一つ → `source` |
| `game_screen!(…)` | `ui::game_screen(GameScreenProps { … })` | `game`: `Narrative`（必須） | なし |
| `button!{…}` | `ui::button(ButtonProps { … })` | `on_click`: `Fn()`、`disabled`: `bool`、`style`: `ButtonStyle` | `Element` → `children` |
| `slider!{…}` | `ui::slider(SliderProps { … })` | `value`: `Int<LO, HI>`（必須）、`on_change`: `Fn(Int<LO, HI>)`、`step`: `u32`、`disabled`: `bool`、`style`: `SliderStyle` | なし |
| `toggle!{…}` | `ui::toggle(ToggleProps { … })` | `value`: `bool`（必須）、`on_change`: `Fn(bool)`、`disabled`: `bool`、`style`: `ButtonStyle` | `Element` → `children` |
| `select!{…}` | `ui::select(SelectProps { … })` | `value`: `T`（必須）、`on_change`: `Fn(T)`、`disabled`: `bool`、`style`: `ButtonStyle` | `ui::option(value, label)` の項目 → `options`。`t![…]` はラベルに変換しない |

したがって、

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

は次のコードと同じ意味です。

```kis
std::ui::button(std::ui::ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| std::ui::label("Back")],
})
```

同様に、

```kix
slider!{value = speed(), on_change = |v| set_speed(v)}
```

は次のコードと同じ意味です。

```kis
std::ui::slider(std::ui::SliderProps {
    value: || speed(),
    on_change: Some(|v| set_speed(v)),
    step: None,
    disabled: None,
    style: None,
})
```

スライダーの範囲は、値の `Int` 型の範囲そのものです。`min` や `max` という属性はありません。

## 展開後のコードを直接書く {#writing-forms-by-hand}

`.kis` ファイルでは（`.kix` ファイルでも、そのほうがよければ）、展開後のコードを直接書けます。展開後に使うものはすべて普通の公開アイテムです。`push_step`、`Say`、`Choice`、`ChoiceArm` はプレリュードに、テキスト用の組み込み関数は `std::narrative::text` に、UI の関数と props 構造体は `std::ui` にあります。手で書いたコードにも、組み込み構文と同じ検査がかかります。たとえば手書きの `Choice` では、`id` に文字列リテラルを使い（`E3019`）、`arms` にはフィールドのないバリアントを値とする `ChoiceArm { … }` の配列リテラルを書く必要があります（`E3020`）。
