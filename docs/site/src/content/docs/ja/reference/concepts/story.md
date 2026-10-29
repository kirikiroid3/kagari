---
title: "物語機械"
---

**物語**とは、ゲームの `pushdown fn` の集まりです。この章では実行モデルと物語変数を説明します。プレイヤーに見えるものは[ステップ](./steps.md)、物語の進み方は[シーンと即時手続き](./scenes.md)で扱います。

## 実行モデル {#execution-model}

エンジンは物語をプッシュダウン機械として実行します。

- **位置**は、シーンとその本体の中の一点です。
- **コールスタック**は、実行中の `pushdown fn` の呼び出しごとに 1 つのフレームを持ちます。フレームには戻り先とその呼び出しのローカル変数が入ります。
- **変数**は、各フレームのローカル変数と[物語変数](#story-variables)です。すべて[有限型](./kinds.md#finite-types)です。

これらをまとめたものが σ（シグマ）、**物語状態**です。コードは 2 つに分かれます。

| 種類 | 例 | 入力を待つか |
| ---- | -- | ------------ |
| ステップ | `s!(…)`、`choice!{…}` | 待つ |
| それ以外 | 呼び出し、`if`、`match`、ループ、代入、`return` | 待たない |

物語はステップに達すると、そのステップの[レンダー](./presentation.md)を実行し、ステップを表示して中断します。入力で再開します。`s!` なら `NextStep`、`choice!` ならアームの選択です。2 つのステップの間はすべて即座に実行されます。データが有限で、すべてのループと呼び出しの循環がステップを通らなければならないため（[停止性](./scenes.md#termination)）、2 つの入力の間のコードは必ず終わり、どのステップにどの状態で到達できるかという問いをコンパイラが決定できます。

プレゼンテーションは一方向です。物語はレンダーを選びますが、物語のコードが画面を読むことはできません。

## 物語変数 {#story-variables}

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
static mut MET_MIO: bool = false;
```

`static mut` は**物語変数**です。物語の 1 回の実行の間だけ存在する、大域的な物語状態です。

- 型は[有限](./kinds.md#finite-types)でなければならず（`E3008`）、初期化子は[定数式](../syntax/constants.md)でなければなりません（`E3010`）。毎回の実行は初期化子から始まるので、未設定の変数を読むことはありません。
- 読み書きできるのは `pushdown fn` だけです。`fn` や `const fn` のコードが物語変数を名指すと `E3007` になります。これにより、システムが物語を読むことができなくなります。
- 型に参照を含めることはできません（`E3012`）。
- 物語はシングルスレッドなので、`unsafe` なしで書き込めます。

`pushdown fn` のローカル変数は普通の `let` と `let mut` の束縛です。呼び出しのフレームに置かれ、再帰呼び出しはそれぞれ自分のコピーを持ちます。

```kis
enum Route { None, Hanako, Azusa, Both }
```

列挙型は普通の Rust の列挙型です。データが有限であれば、ヴァリアントにデータを持たせられます。

不変の `static` は物語変数ではありません。`Setting<T>` か `Meta<T>` のハンドルを持たなければなりません（`E3011`）。[ゲームシステム](./system.md)を参照してください。

## 例：学園祭のハブ {#example-the-festival-hub}

地図から屋台を訪れ、各屋台は訪問済みかどうかを覚えています。シーン `StallB`、`StallC`、`DormNight` は省略しています。

```kix
static mut VISITED_A: bool = false;
static mut VISITED_B: bool = false;
static mut VISITED_C: bool = false;

enum Ending { Normal }
enum Map { A, B, C, Leave }

#[scene]
#[entry]
pushdown fn Day1() -> Ending {
    s!(t!["The school festival starts today."]);
    FestivalHub();
    DormNight();
    Ending::Normal
}

#[scene]
pushdown fn FestivalHub() {
    loop {
        match choice!{
            id = "map",
            t!["Go to stall A"] => Map::A,
            t!["Go to stall B"] => Map::B,
            t!["Go to stall C"] => Map::C,
            t!["Leave"] if VISITED_A && VISITED_B && VISITED_C => Map::Leave,
        } {
            Map::A => StallA(),
            Map::B => StallB(),
            Map::C => StallC(),
            Map::Leave => break,
        }
    }
}

#[scene]
pushdown fn StallA() {
    if VISITED_A {
        s!(t!["The stall is already packing up."]);
        return;
    }
    VISITED_A = true;
    s!(who = "Hanako", render = hanako_jumps, t!["Welcome to stall A!"]);
}

const fn hanako_jumps(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen
}
```

- `choice!` は 1 つのステップです。プレイヤーが選んだアームのヴァリアントに評価され、普通の `match` でそれに応じて分岐します。
- `Leave` は 3 つの屋台をすべて訪れた後にだけ提示されます（`if` ガード）。
- 各屋台はハブのループに戻り、ループが再び地図を表示します。`break` でループを抜けるとハブが戻り、`Day1` は `DormNight()` に進みます。
- ループを一周するすべての経路が `choice!` ステップを通るので、このループは許されます（[停止性](./scenes.md#termination)）。

戻り先はスタックが覚えているので、`FestivalHub` は複製することなく `Day1` からも `Day2` からも呼べ、そのたびに正しい場所へ戻ります。

<!-- diagram: story-call-stack (mermaid-ish)
flowchart LR
  subgraph day1["Day1"]
    d1hub["FestivalHub()"] ==> dorm["DormNight()"]
  end
  subgraph day2["Day2"]
    d2hub["FestivalHub()"] ==> check["EndingCheck()"]
  end
  subgraph hub["#[scene] pushdown fn FestivalHub"]
    start(["開始"]) ==> loop["loop: 地図と屋台"] ==> ret(["return"])
  end
  d1hub -.->|"呼び出し: Day1 を push"| start
  d2hub -.->|"呼び出し: Day2 を push"| start
  ret -.->|"return: pop"| dorm
  ret -.->|"return: pop"| check
  subgraph stack["2 日目の StallA 内でのスタック（先頭が上）"]
    s1["StallA: 現在のシーン"]
    s2["FestivalHub: StallA() の後から再開"]
    s3["Day2: FestivalHub() の後から再開"]
  end
-->
{{#include ../diagrams/story-call-stack.svg}}
