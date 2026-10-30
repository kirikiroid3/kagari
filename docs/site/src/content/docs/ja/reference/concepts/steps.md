---
title: "ステップ"
---

**ステップ**とは、物語が止まってプレイヤーを待つ地点です。ステップには 2 種類あり、`s!(…)` と書く**通常のステップ**と、`choice!{…}` と書く**選択肢**です。そのテキストは `t![…]` で書きます。この 3 つは**フォーム**です。フォームは `.kix` ファイルでだけ使える固定の構文で、普通の構造体式と呼び出しに脱糖されます。フォームの正確な構文（区切り、属性、要素）は[フォーム](../syntax/forms.md)にあります。

ステップを実行できるのは[シーン](./scenes.md#scenes-and-instant-procedures)だけです（`E3002`）。

## ステップは `push_step` の呼び出し {#steps-are-calls-of-push_step}

物語を中断するプリミティブは `std::narrative` にある 1 つだけです。

```kis
#[builtin] pub trait Step { type Output; }

#[builtin]
pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`push_step` はステップを表示し、入力を待ち、入力が運んできたものを返します。物語を中断するので純粋ではありません。`Step` を実装する標準の型は 2 つです。

| 型 | 書き方 | `Output` |
| -- | ------ | -------- |
| `Say` | `s!(…)` | `()`：`NextStep` は情報を運ばない |
| `Choice<A, N>` | `choice!{…}` | `A`：プレイヤーが選んだアーム |

つまり

```kix
s!(who = "Azusa", voice = AZUSA_0072, render = hanako_jumps, t!["ぬっ"]);
```

は次の意味です。

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: Some(AZUSA_0072),
    render: Some(hanako_jumps),
    text: Some(t!["ぬっ"]),
    history: None,
});
```

構造体の形は普通のコードなので手で書くこともでき、`.kis` ファイルではこの形でステップを書きます。完全な宣言は STD リファレンスの [`std::narrative`](../../std/narrative.md) にあります。

## `s!`：通常のステップ {#s-ordinary-step}

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
```

`s!` はメッセージを 1 つ表示し、`NextStep` を待ちます。属性は `Say` のフィールドです。

| 属性 | 型 | 意味 |
| ---- | -- | ---- |
| `who` | `Text` | 表示用テキストとしての話者名。`who` がなければ地の文 |
| `voice` | `VoiceAsset` | メッセージと一緒に再生するボイス |
| `render` | `Render` | ステップに入ったときの画面の変化。[プレゼンテーション](./presentation.md)を参照 |
| `history` | `Text` | メッセージのテキストの代わりにバックログが記録するテキスト |

属性はすべて省略でき、省略した属性は `None` になります。要素はメッセージのテキストである `t![…]` を高々 1 つです。テキストのない `s!` はメッセージを表示しませんが、入力は待ちます。たとえば CG を表示するレンダーの後に使います。`render` のないステップは画面を変えません。

1 つのステップの**話者は 1 人**です。会話はステップの列です。

既定では、バックログは表示時点の埋め込み値でその行を記録します。`history = t![…]` は記録されるテキストを置き換えます。

```kix
s!(who = "Azusa", history = t!["(Azusa names the date)"], t!["Today is day " {DAY} "."]);
```

`NextStep` は情報を運ばないので、セーブのトレースには記録されません。記録されるのは選択肢だけです。

## テキストとテキスト効果 {#text-and-text-effects}

`t![…]` は、並べて書いた項目から `Text` 型の値を作ります。項目は、素の文字列、ルビ（`["数多"]("あまた")`）、**テキスト効果**付きのテキスト（`!["赤い"](color_text(rgb(200, 0, 0)))`）、埋め込み値（`{expr}`）です。

```kix
t!["これは" ["数多"]("あまた") "の" !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

文字列リテラルはそれ自体が `Text` なので、`who = "Mio"` に `t!` は要りません。テキスト効果はコンパイル時に評価される定数なので、メッセージはステップの定数のままです。効果には、スタイル（色、太字、強調など）、表示速度、そしてステップ内のキューがあります。キューは、時間指定の一時停止、クリック待ち（同じステップ内でテキストを続けるクリックで、セーブのトレースには記録されません）、レンダーがアニメーションの時刻を合わせられるマークです。物語の中では、有限な実行時の値の `{expr}` は他の `const fn` 呼び出しと同様に[表化](./kinds.md#calling-const-fn-from-the-story)されます。

項目の構文と規則は[フォーム](../syntax/forms.md)に、すべての効果は STD リファレンスの [`std::narrative::text`](../../std/narrative-text.md) にあります。

## `choice!`：選択肢ステップ {#choice-choice-step}

```kix
enum Confess { Yes, No }

let answer = choice!{
    id = "confess",
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] => Confess::No,
};

match answer {
    Confess::Yes => AzusaConfession(),
    Confess::No => s!(t!["I kept quiet."]),
}
```

`choice!` は 1 つのステップです。エンジンは提示するアームごとに選択肢を 1 つ表示し、プレイヤーが選ぶのを待ちます。選択肢はそのアームのヴァリアントに評価され、普通のコードがそれで分岐します。アームはコードのブロックではなくデータです。選択後に起きることは選択肢の後に書きます。

| 属性 | 型 | 必須 | 意味 |
| ---- | -- | ---- | ---- |
| `id` | 文字列リテラル | はい | 関数内で一意な[安定した識別子](#stable-choice-ids) |
| `render` | `Render` | いいえ | `s!` と同じく、選択肢に入ったときに実行される |

各要素は**アーム** `t![…] => Variant` で、**ガード**を付けて `t![…] if condition => Variant` とも書けます。

- テキストはプレイヤーに表示される選択肢です。
- 1 つの選択肢のアームはすべて、書き手が宣言した**同じ列挙型**のヴァリアントを使い、各ヴァリアントは高々 1 回です。ヴァリアントはフィールドを持ってはいけません（`E3020`）。選択肢はその列挙型を持つので、それに対する `match` は他の `match` と同じく網羅性が検査されます。アームのないヴァリアントも処理するか、`_` で覆う必要があります。

`choice!{…}` は `push_step(Choice { … })` です。各アームは `ChoiceArm { text, shown, value }` になり、`shown` はそのガード、ガードがなければ `true` です。

### ガード {#guards}

```kix
t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
```

ガード付きのアームは、選択肢に入った時点で条件が真でなければ表示されません。表示されるアームが 1 つもない状態で到達しうる `choice!` は、プレイヤーが進めなくなるのでコンパイラが報告します（`E5003`）。

### 無効なアーム {#disabled-arms}

まだロックされている行き先のように灰色で表示される選択肢は、`choice!` のアームではありません。値を持たず、選べません。これはプレゼンテーションであり、選択肢のレンダーが画面の `ChoiceMenu` に追加します。

```kix
match choice!{
    id = "map",
    render = if VISITED_A { show_map } else { show_map_locked },
    t!["Stall A"] => Map::A,
    t!["Stall B"] if VISITED_A => Map::B,
} { … }

const fn show_map_locked(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(1, t!["Stall B"]);
    screen
}
```

`disabled_arm(at, text)` は、表示されるリストの位置 `at`（リストの長さに切り詰められます）に表示専用の選択肢を挿入します。`ChoiceMenu` はそのステップだけのもので、次のステップに入ると空になります。無効なアームは `choice!` の一部ではないので、どの解析にも見えません。プレイヤーが何を選べるかはガードだけが決めます。STD リファレンスの [`std::screen`](../../std/screen.md) を参照してください。

### 安定した選択肢 ID {#stable-choice-ids}

セーブは各選択を、関数のパス、選択肢の `id`、ヴァリアント名として記録します。たとえば `story::Prologue/confess/Confess::Yes` です。呼び出しの文脈はリプレイが補うので、関数が多くの場所から呼ばれたりループの中で実行されたりしても、選択肢の `id` は関数内で一意であれば十分です。`id` は文字列リテラルでなければならず、1 つの関数で同じ `id` を 2 回使うと `E3019` です。

ID は行番号やアームの順序に依存しないので、スクリプトの並べ替えや書き直しをしてもセーブは壊れません。関数、選択肢の `id`、ヴァリアントの名前を変えると、そこを通るセーブは壊れます。そのようなセーブを読み込むと、その選択肢で分岐（divergence）が報告されます。[セーブ、リプレイ、決定性](./saves.md)を参照してください。

## ステップ ID {#step-ids}

すべてのステップ（`s!`、`choice!`）には ID があります。関数のパス、`#`、番号を並べたもので、たとえば `crate::story::FirstDate#28` です。番号は、その関数のステップの中でソースの順に数えたステップの位置で、0 から始まります。

ステップを含まないコードを追加しても ID は変わりません。ステップを追加・削除すると、同じ関数の中でそれより後ろのステップの番号だけが変わります。他の関数のステップの ID は変わりません。

ジェネリックなシーンの各インスタンスは、型引数をパスに含んだ固有の ID をもちます。たとえば `crate::Remember::<crate::day::Mood>#0` と `crate::Remember::<crate::night::Mood>#0` です。

ステップ ID は次の場所に現れます。

- 既読フラグ。ステップ ID ごとに記録されます（[セーブ、リプレイ、決定性](./saves.md)を参照）。
- ゲームがエラーで停止したときのランタイムのメッセージ。最後に到達したステップを示します（[ランタイム](../tools/runtime.md#when-the-game-stops-with-an-error)を参照）。
- ランタイムの進行ログ。入ったステップごとに `step <step ID> say` または `step <step ID> choice` の 1 行を出します。
- コンパイラの `W5009` と `E5003` の注記。そのステップを示します。
