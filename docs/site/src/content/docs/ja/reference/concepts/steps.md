---
title: "ステップ"
---

**ステップ**とは、物語が止まってプレイヤーの操作を待つ地点です。ステップには、`s!(…)` と書く**通常のステップ**と、`choice!{…}` と書く**選択肢**の 2 種類があります。どちらもテキストは `t![…]` で書きます。この 3 つは**組み込み構文**です。組み込み構文は `.kix` ファイルでだけ使える固定の構文で、普通の構造体式と関数呼び出しに展開されます。区切り記号、属性、要素といった正確な書き方は[組み込み構文](../syntax/forms.md)で説明します。

ステップを実行できるのは[シーン](./scenes.md#scenes-and-instant-procedures)だけです（それ以外では `E3002`）。

## ステップの正体は `push_step` の呼び出し {#steps-are-calls-of-push_step}

物語を中断する基本操作は、`std::narrative` にある次の 1 つだけです。

```kis
#[builtin] pub trait Step { type Output; }

#[builtin]
pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`push_step` はステップを表示して入力を待ち、入力で得た値を返します。物語を中断するため、純粋な処理ではありません。標準で `Step` を実装している型は 2 つです。

| 型 | 書き方 | `Output` |
| -- | ------ | -------- |
| `Say` | `s!(…)` | `()`（`NextStep` は何の情報も持たない） |
| `Choice<A, N>` | `choice!{…}` | `A`（プレイヤーが選んだアーム） |

たとえば

```kix
s!(who = "Azusa", voice = AZUSA_0072, render = hanako_jumps, t!["ぬっ"]);
```

は、次のコードと同じ意味です。

```kis
push_step(Say {
    who: Some("Azusa"),
    voice: Some(AZUSA_0072),
    render: Some(hanako_jumps),
    text: Some(t!["ぬっ"]),
    history: None,
});
```

構造体の形は普通のコードなので、手で書いてもかまいません。`.kis` ファイルではこの形でステップを書きます。宣言の全体は、標準ライブラリのリファレンスの [`std::narrative`](../../std/narrative.md) にあります。

## `s!`：通常のステップ {#s-ordinary-step}

```kix
s!(t!["It was the last day of summer."]);
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);
s!(render = cg_fireworks);
```

`s!` はメッセージを 1 つ表示して、`NextStep` を待ちます。属性は `Say` のフィールドに対応します。

| 属性 | 型 | 意味 |
| ---- | -- | ---- |
| `who` | `Text` | 表示する話者名。`who` がなければ地の文 |
| `voice` | `VoiceAsset` | メッセージと一緒に再生するボイス |
| `render` | `Render` | ステップに入ったときの画面の変化。[演出](./presentation.md)を参照 |
| `history` | `Text` | メッセージ本文の代わりにバックログに残すテキスト |

属性はどれも省略でき、省略すると `None` になります。要素はメッセージ本文の `t![…]` で、1 つまで書けます。テキストのない `s!` はメッセージを表示しませんが、入力は待ちます。CG を表示するレンダーの後などに使います。`render` のないステップでは画面は変わりません。

1 つのステップの**話者は 1 人**です。会話はステップを並べて書きます。

バックログには、既定では表示した時点の埋め込み値でその行が残ります。`history = t![…]` を指定すると、残るテキストを差し替えられます。

```kix
s!(who = "Azusa", history = t!["(Azusa names the date)"], t!["Today is day " {DAY} "."]);
```

`NextStep` は何の情報も持たないので、セーブのトレースには残りません。記録されるのは選択肢だけです。

## テキストとテキスト効果 {#text-and-text-effects}

`t![…]` は、並べて書いた項目から `Text` 型の値を作ります。項目には、ただの文字列、ルビ（`["数多"]("あまた")`）、**テキスト効果**付きのテキスト（`!["赤い"](color_text(rgb(200, 0, 0)))`）、埋め込み値（`{expr}`）があります。

```kix
t!["これは" ["数多"]("あまた") "の" !["赤い"](color_text(rgb(200, 0, 0))) "花。"]
```

文字列リテラルはそのままで `Text` なので、`who = "Mio"` には `t!` が要りません。テキスト効果はコンパイル時に評価される定数なので、メッセージもステップの定数のままです。効果には、スタイル（色、太字、強調など）、表示速度、ステップ内の合図があります。合図とは、時間を指定した一時停止、クリック待ち（同じステップの中でテキストを続けるクリックで、セーブのトレースには残らない）、レンダーがアニメーションのタイミングを合わせるためのマークです。物語の中では、有限型の実行時の値を使う `{expr}` は、ほかの `const fn` 呼び出しと同じく[表化](./kinds.md#calling-const-fn-from-the-story)されます。

項目の書き方と規則は[組み込み構文](../syntax/forms.md)に、効果の一覧は標準ライブラリのリファレンスの [`std::narrative::text`](../../std/narrative-text.md) にあります。

## `choice!`：選択肢 {#choice-choice-step}

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

`choice!` は 1 つのステップです。エンジンは表示対象のアームごとに選択肢を 1 つ並べ、プレイヤーが選ぶのを待ちます。選ばれると、`choice!` の値はそのアームのヴァリアントになり、後は普通のコードで分岐します。アームはコードのブロックではなくデータです。選んだ後の処理は、選択肢の後ろに書きます。

| 属性 | 型 | 必須 | 意味 |
| ---- | -- | ---- | ---- |
| `id` | 文字列リテラル | はい | 関数内で一意な[安定した識別子](#stable-choice-ids) |
| `render` | `Render` | いいえ | `s!` と同じく、選択肢に入ったときに実行する |

各要素は `t![…] => Variant` という**アーム**で、ガードを付けて `t![…] if condition => Variant` とも書けます。

- テキストは、プレイヤーに見せる選択肢の文言です。
- 1 つの選択肢のアームはすべて、書き手が宣言した**同じ列挙型**のヴァリアントを使い、同じヴァリアントは 1 回までです。ヴァリアントはフィールドを持てません（`E3020`）。選択肢の型はその列挙型なので、それに対する `match` もほかの `match` と同じく網羅性が検査されます。アームに使っていないヴァリアントも処理するか、`_` で受ける必要があります。

`choice!{…}` の中身は `push_step(Choice { … })` です。各アームは `ChoiceArm { text, shown, value }` になり、`shown` にはガードの条件が、ガードがなければ `true` が入ります。

### ガード {#guards}

```kix
t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
```

**ガード**付きのアームは、選択肢に入った時点で条件が真のときだけ表示されます。表示されるアームが 1 つもない状態で到達しうる `choice!` があると、プレイヤーが先に進めなくなるため、コンパイラが報告します（`E5003`）。

### 選べないアーム {#disabled-arms}

まだ解放されていない行き先のように灰色で表示される選択肢は、`choice!` のアームではありません。値を持たず、選ぶこともできません。これは演出の一部で、選択肢のレンダーが画面の `ChoiceMenu` に追加するものです。

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

`disabled_arm(at, text)` は、表示中の一覧の位置 `at` に、表示専用の選択肢を挿入します（`at` は一覧の長さまでに切り詰められます）。`ChoiceMenu` はそのステップ限りのもので、次のステップに入ると空になります。選べないアームは `choice!` の一部ではないので、どの解析からも見えません。プレイヤーが何を選べるかは、ガードだけで決まります。標準ライブラリのリファレンスの [`std::screen`](../../std/screen.md) を参照してください。

### 安定した選択肢 ID {#stable-choice-ids}

セーブには、各選択が関数のパス、選択肢の `id`、ヴァリアント名の組で記録されます（例：`story::Prologue/confess/Confess::Yes`）。呼び出しの文脈は再生時に補われるので、関数がさまざまな場所から呼ばれたりループの中で実行されたりしても、選択肢の `id` は関数内で一意であれば十分です。`id` は文字列リテラルで書く必要があり、同じ関数で同じ `id` を 2 回使うとエラー `E3019` になります。

この ID は行番号やアームの順序に依存しないので、スクリプトを並べ替えたり文言を書き直したりしてもセーブは壊れません。一方、関数、選択肢の `id`、ヴァリアントのいずれかの名前を変えると、そこを通るセーブは使えなくなります。そうしたセーブを読み込むと、その選択肢で食い違い（divergence）が報告されます。[セーブ、再生、決定性](./saves.md)を参照してください。

## ステップ ID {#step-ids}

すべてのステップ（`s!`、`choice!`）には ID があります。関数のパス、`#`、番号をつなげたもので、たとえば `crate::story::FirstDate#28` のようになります。番号は、その関数の中のステップをソースの順に 0 から数えたものです。

ステップを含まないコードを追加しても、ID は変わりません。ステップを追加・削除すると、同じ関数の中でそれより後ろにあるステップの番号だけが変わります。ほかの関数のステップの ID は変わりません。

ジェネリックなシーンでは、インスタンスごとに別の ID になり、パスに型引数が入ります。たとえば `crate::Remember::<crate::day::Mood>#0` と `crate::Remember::<crate::night::Mood>#0` です。

ステップ ID は次の場所に現れます。

- 既読フラグ。ステップ ID ごとに記録します（[セーブ、再生、決定性](./saves.md)を参照）。
- ゲームがエラーで止まったときのランタイムのメッセージ。最後に到達したステップを示します（[ランタイム](../tools/runtime.md#when-the-game-stops-with-an-error)を参照）。
- ランタイムの進行ログ。ステップに入るたびに `step <step ID> say` または `step <step ID> choice` を 1 行出力します。
- コンパイラの `W5009` と `E5003` に付く注記。該当するステップを示します。
