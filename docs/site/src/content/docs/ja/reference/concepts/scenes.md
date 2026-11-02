---
title: "シーンと即時手続き"
---

物語は、`#[scene]` を付けた `pushdown fn` である**シーン**と、それ以外の `pushdown fn` である**即時手続き**でできています。シーンは互いに呼び出し、呼び出し元へ戻ります。シーン内の制御フローは Rust と同じで、`goto` はありません。

## 呼び出し {#calls}

```kix
FestivalHub();                // call a scene
let answer = Confession();    // a scene that returns a value
visit(Map::A);                // call an instant procedure
```

どの呼び出しも普通の関数呼び出しの構文で書きます。呼び出すと、戻り先と呼び出し先のローカル変数を入れたフレームを積んでから、呼び出し先を実行します。呼び出し先が戻るとフレームを取り除き、呼び出し元の続きを実行します（[物語機械](./story.md#execution-model)を参照）。

## シーンと即時手続き {#scenes-and-instant-procedures}

```kix
#[scene]
pushdown fn StallA() {
    if VISITED_A {
        s!(t!["The stall is already packing up."]);
        return;
    }
    visit(Map::A);
    s!(who = "Hanako", t!["Welcome to stall A!"]);
}

pushdown fn visit(stall: Map) {
    match stall {
        Map::A => VISITED_A = true,
        Map::B => VISITED_B = true,
        Map::C => VISITED_C = true,
        Map::Leave => {}
    }
}
```

`pushdown fn` に `#[scene]` を付けると**シーン**になります。物語の検査はシーン単位で行います。

- [`push_step`](./steps.md#steps-are-calls-of-push_step) を呼べる（`s!` や `choice!` を実行できる）のはシーンだけです。ステップは物語を中断するため、純粋な処理ではありません。シーン以外で使うと `E3002` になります。
- 別のシーンを呼べるのもシーンだけです（`E3003`）。シーンは再帰できます。
- [乱数](#randomness)を引けるのもシーンだけです（`E3024`）。
- コンパイラは各シーンを物語全体の構成要素として 1 回だけ解析します。多くの場所から呼ばれるシーンでも、呼び出し元ごとに解析し直すことはありません。

`#[scene]` のない `pushdown fn` は**即時手続き**です。

- 有限型の値を保持し、物語変数を読み書きし、ほかの即時手続きや `const fn` を呼べます。
- ステップの実行、シーンの呼び出し、乱数の取得はできません。
- 再帰はできず、即時手続き同士の呼び出しが循環するとエラー `E3005` になります。ループは定数の範囲か配列に対する `for` だけで、`loop` や `while` を使うと `E3004` になります。

そのため即時手続きは入力を待たずに必ず終わり、コンパイラはその呼び出しを、呼び出し元のシーン内の 1 回の遷移として扱います。

| | シーン（`#[scene] pushdown fn`） | 即時手続き（`pushdown fn`） |
| - | -------------------------------- | --------------------------- |
| ステップの実行（`s!`、`choice!`、`push_step`） | 可 | 不可（`E3002`） |
| シーンの呼び出し | 可 | 不可（`E3003`） |
| 即時手続きと `const fn` の呼び出し | 可 | 可 |
| 乱数（`rng_choose`） | 可 | 不可（`E3024`） |
| 物語変数の読み書き、メタデータの書き込み | 可 | 可 |
| 再帰 | 可（ステップを通ること） | 不可（`E3005`） |
| `loop`、`while` | 可（毎周ステップを通ること） | 不可（`E3004`） |
| 定数の範囲か配列に対する `for` | 可 | 可 |

`pushdown fn` 以外の関数に `#[scene]` を付けるとエラー `E3018` になります。シーン名は PascalCase で付けるので、呼び出し箇所を見れば即時手続きと区別できます。snake_case のリントはシーンには適用されません。

## 制御フロー {#control-flow}

物語では Rust の制御フローをそのまま使います。`if`、`if let`、`match`、`loop`、`while`、`for`、`break`、`continue`、`return` に加え、入れ子のループを抜けるためのループラベルもあります。構文は[制御フローとループ](../syntax/control-flow.md)で説明します。

```kix
#[scene]
pushdown fn Week() {
    'week: for day in 0..7 {
        match Day(day) {
            DayEnd::Normal => continue,
            DayEnd::Hospital => break 'week,
        }
    }
}
```

`pushdown fn` の中の `for` で回せるのは、両端が定数式の範囲か配列だけです（`E3006`）。ループ変数は有限型でなければなりません。`for day in 0..7` の `day` は、`Day` の引数が求める `Int` 型になります。型を決める手がかりのないループ変数は `i32` になり、有限ではないのでエラー `E3008` になります。その場合は `const LAPS: Int<0, 3> = 3;` と宣言して `for lap in 0..LAPS` のように、範囲の端に型を付けてください。

`goto` も、ジャンプ先となるラベル付きの位置もありません。ハブは `loop` で、途中で終わるシーンは `return` で、分岐は `if` か `match` で書きます。この結果、次の 2 つが成り立ちます。

- 制御が別の関数やブロックの途中へ飛び込むことはありません。解析の前提である呼び出しと戻りの入れ子構造は、言語の構成上必ず保たれます。
- ループの途中へ飛び込むような既約でない制御フローは書けません。物語でそれが必要になることはありません。

条件は普通の真偽値の式です。複数の変数はタプルにまとめて照合します。

```kix
match (VISITED_A, VISITED_B, VISITED_C) {
    (true, true, true) => FestivalEnd(),
    _ => {}
}
```

まとめると次のとおりです。

| したいこと | 書き方 |
| ---------- | ------ |
| シーンを呼ぶ | `Name(x)` |
| シーンを抜ける | `return;` / `return value;` |
| 分岐する | `if`、`match`（網羅的。範囲とタプルも使える） |
| ハブ、繰り返し出す選択肢 | `loop { match choice!{…} { … } }` と `break` |
| 回数の決まった繰り返し | `for i in 0..n` |
| カウンタを増減する | `x += 1;` |
| メタデータを書き込む | `META_FIELD.write(v);`（[開始シーンとエンディング](./entry.md#writing-meta)） |
| 乱数を引く（シーンのみ） | `rng_choose::<T>()` |
| 物語を終える | 開始シーンから戻る（[開始シーンとエンディング](./entry.md#the-ending)） |

## 値を返す {#returning-a-value}

```kix
enum Answer { Accepted, Rejected }

#[scene]
pushdown fn Confession() -> Answer {
    match choice!{
        id = "confess",
        t!["Tell her the truth"] => Confess::Yes,
        t!["Say nothing"] => Confess::No,
    } {
        Confess::Yes => Answer::Accepted,
        Confess::No => Answer::Rejected,
    }
}

#[scene]
pushdown fn Day3() {
    match Confession() {
        Answer::Accepted => AzusaRoute(),
        Answer::Rejected => s!(t!["The wind stopped."]),
    }
}
```

どう終わったかを呼び出し元に伝えたいシーンは、有限型の戻り値を宣言して結果を返します。Rust の型検査により、どの経路も必ず値を返します。値を返さずに本体の終わりに達しうる経路があれば型エラーです。終わらないループには値は要りません。

## 停止性 {#termination}

ステップとステップの間のコードは一瞬で実行されるので、必ず終わらなければなりません。コンパイラは次のものを拒否します。

- シーン内の `loop` や `while` で、本体を一周する経路のどれかがステップを通らないもの（`E5001`）。
- ステップを通らない呼び出しの循環。たとえば、ステップより前に自分自身を呼ぶシーン（`E5002`）。

定数の範囲か配列に対する `for` は回数が決まっているので、ステップは要りません。分岐はどれも実際に通りうるものとして扱います。ステップのない経路が実際には通らないことを、検査で証明しようとはしません。

検査は呼び出し先の中まで見ます。*ステップを通らずに戻る可能性がある*シーンの呼び出しは、ステップなしとして数えます。即時手続きの呼び出しも、すべてステップなしです。したがって、ループ内の唯一のステップが呼び出し先のシーンにある場合、そのシーンがどの経路でも必ずステップを実行するときだけ受け入れられます。

日ごとのループのように繰り返す物語は、1 つの進行役のシーンに `loop` を書き、その中で各日を順に呼び出します。シーン同士が輪になって呼び合い、戻らない書き方をすると、一周するたびにコールスタックが伸びていきます。

## 物語の中のクロージャ {#closures-in-the-story}

`pushdown fn` はクロージャを引数に取り、呼び出せます。

```kix
pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}

twice(|| AFFECTION_MIO += 1);
```

物語の中のクロージャは[第二級](../syntax/values.md)です。渡したり呼んだりはできますが、保存、返却、キャプチャはできません（`E3015`）。クロージャは即時手続きなので、ステップの実行やシーンの呼び出しもできません。

## 乱数 {#randomness}

シーンでは `rng_choose::<T>()` で乱数を引きます。`T` は `RandChoose` を実装する型、つまりフィールドのない列挙型か `Int<lo, hi>` です。どの値も等確率で選ばれます。乱数を引けるのはシーンだけで、即時手続きから呼ぶと `E3024`、`RandChoose` でない `T` を指定すると `E2026` になります。

```kix
enum Coin { Heads, Tails }

match rng_choose::<Coin>() {
    Coin::Heads => s!(t!["Heads."]),
    Coin::Tails => s!(t!["Tails."]),
}
```

生成器は xoroshiro128++ です。生成器の状態は物語状態の一部で、シードはセーブに含まれるため、再生するとすべての乱数がそのまま再現されます（[セーブ、再生、決定性](./saves.md)を参照）。コンパイラは乱数を、プレイヤーの選択と同じく `T` のどの値も選ばれうる自由な選択として扱います。
