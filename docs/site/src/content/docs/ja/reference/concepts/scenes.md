---
title: "シーンと即時手続き"
---

物語は、`#[scene]` を付けた `pushdown fn` である**シーン**と、それ以外の `pushdown fn` である**即時手続き**からできています。シーンは互いに呼び合って戻り、シーンの中の制御フローは Rust と同じです。`goto` はありません。

## 呼び出し {#calls}

```kix
FestivalHub();                // call a scene
let answer = Confession();    // a scene that returns a value
visit(Map::A);                // call an instant procedure
```

呼び出しはすべて普通の呼び出し構文です。呼び出しは戻り先と呼び出し先のローカル変数を持つフレームを積み、呼び出し先を実行します。呼び出し先が戻るとフレームが取り除かれ、呼び出し元が続きを実行します（[物語機械](./story.md#execution-model)を参照）。

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

`#[scene]` は `pushdown fn` を**シーン**にします。シーンは物語を検査する単位です。

- [`push_step`](./steps.md#steps-are-calls-of-push_step) を呼べる、つまり `s!` と `choice!` を実行できるのはシーンだけです。ステップは物語を中断するので純粋ではないからです。それ以外では `E3002` です。
- 別のシーンを呼べるのはシーンだけです（`E3003`）。シーンは再帰できます。
- [乱数](#randomness)を引けるのはシーンだけです（`E3024`）。
- コンパイラは各シーンを物語全体の 1 つの構成要素として 1 回だけ解析するので、多くの場所から呼ばれるシーンでも呼び出し元ごとに解析し直すことはありません。

`#[scene]` のない `pushdown fn` は**即時手続き**です。

- 有限型を保持し、物語変数を読み書きでき、他の即時手続きと `const fn` を呼べます。
- ステップを実行せず、シーンを呼ばず、乱数も引きません。
- 再帰しません。即時手続き同士の呼び出しは循環を作れません（`E3005`）。ループは定数の範囲か配列に対する `for` だけで、`loop` と `while` は `E3004` です。

したがって即時手続きは入力なしに必ず終わり、コンパイラはその呼び出しを、呼び出し元のシーンの中の 1 つの遷移として扱います。

| | シーン（`#[scene] pushdown fn`） | 即時手続き（`pushdown fn`） |
| - | -------------------------------- | --------------------------- |
| ステップの実行（`s!`、`choice!`、`push_step`） | 可 | 不可（`E3002`） |
| シーンの呼び出し | 可 | 不可（`E3003`） |
| 即時手続きと `const fn` の呼び出し | 可 | 可 |
| 乱数（`rng_choose`） | 可 | 不可（`E3024`） |
| 物語変数の読み書き、メタの書き込み | 可 | 可 |
| 再帰 | 可。ステップを通ること | 不可（`E3005`） |
| `loop`、`while` | 可。毎周ステップを通ること | 不可（`E3004`） |
| 定数の範囲か配列に対する `for` | 可 | 可 |

`pushdown fn` でない関数に `#[scene]` を付けると `E3018` です。シーンは PascalCase で名付けるので、呼び出し箇所で即時手続きと区別できます。snake_case のリントはシーンには適用されません。

## 制御フロー {#control-flow}

物語は Rust の制御フローを使います。`if`、`if let`、`match`、`loop`、`while`、`for`、`break`、`continue`、`return` と、入れ子のループを抜けるためのループラベルです。構文は[制御フローとループ](../syntax/control-flow.md)にあります。

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

`pushdown fn` の中の `for` は、両端が定数式である範囲か配列に対してだけ回せ（`E3006`）、ループ変数は有限型でなければなりません。`for day in 0..7` の `day` は、`Day` のパラメーターが期待する `Int` 型になります。何にも制約されないループ変数は `i32` で、有限ではありません（`E3008`）。代わりに `const LAPS: Int<0, 3> = 3;` として `for lap in 0..LAPS` のように端に型を付けます。

`goto` も、ジャンプ先となるラベル付きの位置もありません。ハブは `loop`、早く終わるシーンは `return`、分岐は `if` か `match` です。その結果、次の 2 つが成り立ちます。

- 制御が別の関数やブロックの途中に飛び込むことはありません。解析が前提とする呼び出しと戻りの入れ子は、構成上成り立ちます。
- ループの途中に飛び込むような既約でない制御フローは書けません。そのような物語は必要ありません。

条件は普通の真偽値の式で、複数の変数はタプルとしてまとめて照合します。

```kix
match (VISITED_A, VISITED_B, VISITED_C) {
    (true, true, true) => FestivalEnd(),
    _ => {}
}
```

まとめ：

| したいこと | 書き方 |
| ---------- | ------ |
| シーンを呼ぶ | `Name(x)` |
| シーンを抜ける | `return;` / `return value;` |
| 分岐 | `if`、`match`（網羅的。範囲とタプルが使える） |
| ハブ、繰り返す選択肢 | `loop { match choice!{…} { … } }` と `break` |
| 有界な繰り返し | `for i in 0..n` |
| カウンタを変える | `x += 1;` |
| メタを書く | `META_FIELD.write(v);`（[エントリーシーンとエンディング](./entry.md#writing-meta)） |
| 乱数を引く（シーンのみ） | `rng_choose::<T>()` |
| 物語を終える | エントリーシーンから戻る（[エントリーシーンとエンディング](./entry.md#the-ending)） |

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

どう終わったかを伝える必要のあるシーンは、有限型の戻り値の型を宣言し、結果を返します。Rust の型付けにより、すべての経路が値を生みます。値なしで本体の終わりに達しうる経路は型エラーです。終わらないループに値は要りません。

## 停止性 {#termination}

2 つのステップの間はすべて即座に実行されるので、必ず終わらなければなりません。コンパイラは次のものを拒否します。

- 本体を一周する経路のうちステップを通らないものがある、シーン内の `loop` や `while`（`E5001`）。
- ステップを通らない呼び出しの循環。たとえば、ステップより前に自分自身を呼ぶシーン（`E5002`）。

定数の範囲か配列に対する `for` は有界なので、ステップは要りません。すべての分岐は起こりうるものとして扱われます。ステップのない経路が実際には通らないことを証明しようとはしません。

検査は呼び出しの中まで見ます。*ステップを通らずに戻りうる*シーンの呼び出しはステップなしと数え、即時手続きの呼び出しもすべてステップなしと数えます。そのため、唯一のステップが呼び出したシーンの中にあるループは、そのシーンがすべての経路でステップを実行する場合にだけ受け入れられます。

日々のループのように循環する物語は、1 つの駆動シーンの中の `loop` が各日を順に呼ぶ形で書きます。互いに輪になって呼び合い、戻らないシーンは、一周するたびにコールスタックを伸ばします。

## 物語の中のクロージャ {#closures-in-the-story}

`pushdown fn` はクロージャの引数を受け取って呼べます。

```kix
pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}

twice(|| AFFECTION_MIO += 1);
```

物語のクロージャは[第二級](../syntax/values.md)です。渡して呼ぶことはできますが、保存、返却、キャプチャはできません（`E3015`）。クロージャは即時手続きであり、ステップの実行やシーンの呼び出しはできません。

## 乱数 {#randomness}

シーンは `rng_choose::<T>()` で乱数を引きます。`T: RandChoose`、つまり `T` はフィールドのない列挙型か `Int<lo, hi>` です。各値は等確率です。乱数を引けるのはシーンだけで、即時手続きからの呼び出しは `E3024`、`RandChoose` でない `T` は `E2026` です。

```kix
enum Coin { Heads, Tails }

match rng_choose::<Coin>() {
    Coin::Heads => s!(t!["Heads."]),
    Coin::Tails => s!(t!["Tails."]),
}
```

生成器は xoroshiro128++ です。その状態は物語状態の一部で、シードはセーブにあるので、リプレイはすべての乱数を再現します（[セーブ、リプレイ、決定性](./saves.md)を参照）。コンパイラは各乱数を、プレイヤーの選択と同じく `T` のすべての値にわたる自由な選択として扱います。
