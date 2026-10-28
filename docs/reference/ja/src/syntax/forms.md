# フォーム {#forms}

**フォーム**は、ステップ、テキスト、UI の部品を組み立てる固定の構文です。`s!(…)`、`choice!{…}`、`t![…]` と、`column![…]` や `button!{…}` などの UI フォームがあります。各フォームは、普通の構造体式、クロージャ、標準ライブラリのアイテムの呼び出しに脱糖されます。`.kis` ファイルでは、同じものを手で書くこともできます。

フォームはマクロではありません。Kagari にマクロはなく、フォームの集合は言語が固定しています。未知のフォーム名は構文エラー（`E0001`）です。

ステップが物語にとって何を意味するかは[ステップ](../concepts/steps.md)に、UI フォームが何を組み立てるかは STD リファレンスの [`std::ui`](../../../../std/ja/src/ui.md) にあります。この章では、フォームの正確な構文と脱糖を示します。

## フォームを書ける場所 {#where-forms-are-allowed}

- フォームは `.kix` ファイルでだけ使えます。`.kis` ファイルのフォームは `E0001` です。
- フォームは式で、名前、`!`、そして `(`、`[`、`{` のいずれかが続きます。`a != b` がフォームになることはありません。
- 脱糖は名前解決より前に行われ、脱糖後のコードは常に標準ライブラリのアイテム（`std::narrative::push_step`、`std::ui::button` など）を指します。同じ名前のローカルなアイテムがあっても変わりません。
- 脱糖の後は通常の規則が適用されます。ステップはシーンの中でだけ実行でき（`E3002`）、UI 関数は `fn` のコードです（[呼び出し規則](../concepts/kinds.md#call-rules)）。

## 一般構文 {#general-syntax}

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

区切り記号によって、フォームが受け付けるものが決まります。

| 区切り記号 | 受け付けるもの | フォーム |
| ---------- | -------------- | -------- |
| `name!( … )` | 属性、続いて決まった数の要素 | `s!`、`text!`、`image!`、`game_screen!` |
| `name![ … ]` | 要素だけ。属性は書けません | `t!`、`column!`、`row!`、`layer!` |
| `name!{ … }` | 属性、続いて要素 | `choice!`、`button!`、`slider!`、`toggle!`、`select!` |

各フォームの区切り記号はちょうど 1 つに決まっています。次は構文エラー（`E0001`）です。

- 区切り記号の誤り（`` `s!` is written `s!(…)` ``）
- `[…]` フォームの属性、要素の後の属性、重複した属性、フォームが知らない属性、必須属性の欠落
- 要素の数の誤り
- `choice!` 以外でのアーム（`… => …`）
- `t![…]` の項目の間のカンマ。項目は並べて書きます

`key = expression` が属性になるのはフォームの直下だけで、それ以外の場所の `=` は代入です。エントリーはカンマで区切り、末尾のカンマも書けます。

文の先頭にある波括弧のフォーム（`choice!{…}`、`button!{…}` など）はブロック様で、後に `;` は不要です。`s!(…)` などの他のフォームには、通常の式文と同じく `;` が必要です。

### フォームの一覧 {#summary-of-the-forms}

| フォーム | 属性 | 要素 | 脱糖先 |
| -------- | ---- | ---- | ------ |
| `s!(…)` | `who`、`voice`、`render`、`history` | 高々 1 つの `t![…]` | `push_step(Say { … })` |
| `choice!{…}` | `id`（必須）、`render` | アーム `t![…] => Variant`、`t![…] if guard => Variant` | `push_step(Choice { … })` |
| `t![…]` | — | 並べて書いたテキスト項目 | `ruby`、`mono_ruby`、`effect`、`to_text` の呼び出しの `concat([…])` |
| `column![…]`、`row![…]` | — | 任意の数 | `ui::column([…])`、`ui::row([…])` |
| `layer![…]` | — | 任意の数 | `ui::layer([…])` |
| `text!(…)` | `style` | ちょうど 1 つ | `ui::text(TextProps { … })` |
| `image!(…)` | `fit` | ちょうど 1 つ | `ui::image(ImageProps { … })` |
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
| `who` | `Text` | 話者の名前。なければ地の文になります |
| `voice` | `VoiceAsset` | メッセージとともに再生するボイス |
| `render` | `Render` | ステップに入るときに画面がどう変わるか |
| `history` | `Text` | メッセージの代わりにバックログに記録するテキスト |

属性はすべて省略できます。要素を書く場合は `t![…]` フォームとして書かなければならず（そうでなければ `E0001`）、それがフィールド `text` になります。定数に入った `Text` を表示するには、補間を使います: `t![{GREETING}]`。

各フィールドは、属性や要素があれば `Some(value)`、なければ `None` になり、フォーム全体は `push_step` の呼び出しになります。つまり

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
```

は次の意味です。

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: None,
    render: Some(azusa_smiles),
    text: Some("Did you forget again?"),
    history: None,
});
```

（`t!["Did you forget again?"]` は文字列そのものです。[`t![…]`](#t) を参照してください。）この式の型は `()` です。

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
| `id` | 文字列リテラル | はい | ない場合や文字列リテラルでない場合は `E0001`。1 つの関数で 2 回使うと `E3019` |
| `render` | `Render` | いいえ | `s!` と同じ |

要素はすべて**アーム**です: `t![…] => Variant` または `t![…] if guard => Variant`。

- テキストは `t![…]` フォームとして書かなければなりません。素の文字列リテラルは `E0001` です。
- ガードは `bool` の式で、選択肢に入るときに評価されます。ガードが偽のアームは提示されません。
- 値はフィールドを持たない enum バリアントでなければなりません（`E3020`）。すべてのアームは 1 つの enum のバリアントを使い（そうでなければ `E2001`）、各バリアントは高々 1 回です（`E3020`）。`choice!` 式の型はその enum です。

このフォームは、アームごとに 1 つの `ChoiceArm` をソース順に並べた `Choice` に脱糖されます。ガードのないアームは `shown: true` になります。上の例は次の意味です。

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

すべてのアームが隠れた状態で到達しうる選択肢は `E5003` です。ガード、無効なアーム、安定した選択肢 ID については[ステップ](../concepts/steps.md#choice-choice-step)で説明しています。

## `t![…]` {#t}

```kix
t!["これは" ["数多"]("あまた") "の" ["漢字"]("かん"|"じ") !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

`t![…]` は `Text` を作ります。項目はカンマなしで並べて書き、そのまま連結されます。項目の間に空白が足されたり除かれたりすることはありません。

| 項目 | 意味 | 脱糖先 |
| ---- | ---- | ------ |
| `"…"` | 普通のテキスト | その文字列リテラル |
| `["数多"]("あまた")` | ルビ: 親文字全体に 1 つの読み（グループルビ） | `ruby("数多", "あまた")` |
| `["漢字"]("かん"\|"じ")` | ルビ: 親文字 1 文字ごとに 1 つの読み（モノルビ） | `mono_ruby("漢字", ["かん", "じ"])` |
| `!["赤い"](e)` | テキスト効果 `e` を付けて親文字を表示 | `effect("赤い", e)` |
| `![["数多"]("あまた")](e)` | テキスト効果付きのルビ | `effect(ruby("数多", "あまた"), e)` |
| `{expr}` | `expr` の値をテキストにしたもの | `to_text(expr)` |

フォーム全体は次のとおりです。

- 項目がなければ `""`
- 項目が 1 つなら、その項目の式。`t!["Mio"]` は `"Mio"`、`t![{DAY}]` は `to_text(DAY)` です
- それ以外は項目の `concat([…])`

```kix
t!["Day " {DAY} ": " !["the festival"](emphasis())]
```

は次の意味です。

```kis
concat(["Day ", to_text(DAY), ": ", effect("the festival", emphasis())])
```

`ruby`、`mono_ruby`、`effect`、`concat`、`to_text` は [`std::narrative::text`](../../../../std/ja/src/narrative-text.md) の `const fn` ビルトインです。

### 文字列 {#strings}

文字列項目は文字列リテラルで、エスケープ `\n`（改行）、`\r`、`\t`、`\\`、`\"`、`\0`、`\u{…}` を使えます。引用符の中に直接入力した改行も、そのままテキストの一部になります。文字列リテラルはどこでも型 `Text` を持つので、`who = "Mio"` のような属性に `t!` は不要です。

### ルビ {#ruby}

`[base](readings)` は、1 つの文字列リテラルである `base` にルビを振ります。

- 読みが 1 つ: 親文字全体へのグループルビです。
- `|` で区切った複数の読み: モノルビで、親文字 1 文字ごとに 1 つの読みです。読みの数は文字数と等しくなければなりません（`E3021`）。

### テキスト効果 {#text-effects}

`![base](effect)` は、文字列リテラルまたはルビ項目である `base` を**テキスト効果**付きで表示します。`effect` は型 `TextEffect` の式で、[定数式](./constants.md)でなければなりません（`E3022`）。そのため、メッセージは依然としてそのステップの定数です。効果は `.and(…)` で組み合わせ、右側で設定したフィールドが優先されます。親文字の中で補間は使えません。

```kix
t!["Tell me about " !["you"](emphasis()) "."]
t!["It was " !["here"](dots().and(color_text(rgb(200, 0, 0)))) "."]
```

効果（`color_text`、`bold`、`italic`、`size`、`font`、`outline`、`shadow`、`emphasis`、`dots`、`speed`、`speed_scale`、`reveal_instant`、`pause`、`click_wait`、`mark`）の一覧は STD リファレンスの [`std::narrative::text`](../../../../std/ja/src/narrative-text.md) にあります。

### ポーズとマーク {#pauses-and-marks}

**キュー**は、テキストの見た目ではなく表示の進行に作用する効果です。親文字は通常、空文字列です。

```kix
s!(who = "Mio", render = mio_startled, t!["Wait" ![""](pause(ms(500)).and(mark(1))) "—what?"]);
s!(t!["Read slowly." ![""](click_wait()) "\nThen go on."]);
```

| キュー | 効果 |
| ------ | ---- |
| `pause(d)` | 親文字の前で表示を `d` だけ止めます |
| `click_wait()` | クリックまで表示を止めます。クリックすると同じステップの中でテキストが続きます |
| `mark(n)` | 表示が親文字に達した瞬間に名前を付け、レンダーがそこでアニメーションを予定できるようにします |

1 つのテキストで同じマーク番号を 2 回使うと `E4010` です。表示、ポーズ、マークの正確な規則は STD リファレンスの [`std::narrative::text`](../../../../std/ja/src/narrative-text.md) にあります。

### 補間 {#interpolation}

`{expr}` は `expr` の値をテキストとして挿入します。`expr` の型は標準トレイト `ToText` を実装していなければなりません（そうでなければ `E2026`）。実装しているのは `Text`、`String`、`Int<lo, hi>`、Rust の整数型、`Rat`、`Decimal` です。`pushdown fn` の中で物語変数のような有限の実行時値を補間すると、有限の引数による `const fn` `to_text` の呼び出しになり、コンパイラーがそれを[表化](../concepts/kinds.md#calling-const-fn-from-the-story)します。

## UI フォーム {#ui-forms}

```kix
column![
    text!(t!["Text speed: " {speed()}]),
    slider!{value = speed(), on_change = |v| set_speed(v)},
    button!{on_click = || nav::back(), t!["Back"]},
]
```

UI フォームは、`fn` のコードで [`std::ui`](../../../../std/ja/src/ui.md) の `Element` 値を組み立てます。脱糖の規則は次のとおりです。

- **要素。** 各要素 `e` はクロージャ `|| e` になります。`column!`、`row!`、`button!`、`toggle!` では、`t![…]` フォームとして書いた要素は `|| ui::label(t![…])` になります。
- **リアクティブな属性。** 名前が `on_` で始まらない属性はすべてクロージャになります。必須の属性なら `key: || expr`、省略可能な属性なら `key: Some(|| expr)` です。UI は、クロージャが読んだシグナルが変わるとそのクロージャを再実行します。
- **ハンドラー属性。** `on_*` 属性は書いたまま渡されます: `key: Some(expr)`。
- **書かれなかった省略可能な属性**は `None` になります。
- `column!`、`row!`、`layer!` は要素のクロージャの配列を直接渡します。他のフォームは、属性を `std::ui` で宣言された順のフィールドとし、要素用のフィールドを加えた props 構造体を渡します。

| フォーム | 脱糖先 | 属性とその型 | 要素 |
| -------- | ------ | ------------ | ---- |
| `column![…]`、`row![…]` | `ui::column(children)`、`ui::row(children)` | なし | `Element`。任意の数 |
| `layer![…]` | `ui::layer(children)` | なし | `Placed`。任意の数。`t![…]` は包まれません |
| `text!(…)` | `ui::text(TextProps { … })` | `style`: `TextStyle` | ちょうど 1 つの `Text` → `content` |
| `image!(…)` | `ui::image(ImageProps { … })` | `fit`: `Fit` | ちょうど 1 つの `ImageAsset` → `source` |
| `game_screen!(…)` | `ui::game_screen(GameScreenProps { … })` | `game`: `Narrative`（必須） | なし |
| `button!{…}` | `ui::button(ButtonProps { … })` | `on_click`: `Fn()`、`disabled`: `bool`、`style`: `ButtonStyle` | `Element` → `children` |
| `slider!{…}` | `ui::slider(SliderProps { … })` | `value`: `Int<LO, HI>`（必須）、`on_change`: `Fn(Int<LO, HI>)`、`step`: `u32`、`disabled`: `bool`、`style`: `SliderStyle` | なし |
| `toggle!{…}` | `ui::toggle(ToggleProps { … })` | `value`: `bool`（必須）、`on_change`: `Fn(bool)`、`disabled`: `bool`、`style`: `ButtonStyle` | `Element` → `children` |
| `select!{…}` | `ui::select(SelectProps { … })` | `value`: `T`（必須）、`on_change`: `Fn(T)`、`disabled`: `bool`、`style`: `ButtonStyle` | `ui::option(value, label)` の項目 → `options`。`t![…]` は包まれません |

したがって

```kix
button!{on_click = || nav::back(), t!["Back"]}
```

は次の意味になり、

```kis
std::ui::button(std::ui::ButtonProps {
    on_click: Some(|| nav::back()),
    disabled: None,
    style: None,
    children: [|| std::ui::label("Back")],
})
```

また

```kix
slider!{value = speed(), on_change = |v| set_speed(v)}
```

は次の意味になります。

```kis
std::ui::slider(std::ui::SliderProps {
    value: || speed(),
    on_change: Some(|v| set_speed(v)),
    step: None,
    disabled: None,
    style: None,
})
```

スライダーの範囲はその値の `Int` 型の範囲です。`min` や `max` の属性はありません。

## フォームを手で書く {#writing-forms-by-hand}

`.kis` ファイル、またはそうしたい `.kix` ファイルでは、脱糖後のコードを直接書きます。脱糖先はすべて普通の公開アイテムです。`push_step`、`Say`、`Choice`、`ChoiceArm` はプレリュードに、テキストのビルトインは `std::narrative::text` に、UI 関数と props 構造体は `std::ui` にあります。手書きのコードもフォームと同じ検査を受けます。手書きの `Choice` は文字列リテラルの `id`（`E3019`）と、フィールドのないバリアントを持つ `ChoiceArm { … }` レコードの配列リテラル（`E3020`）を使わなければなりません。
