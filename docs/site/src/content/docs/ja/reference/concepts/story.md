---
title: "物語機械"
---

**物語**とは、ゲームに含まれる `pushdown fn` 全体のことです。この章では、物語の実行モデルと物語変数を説明します。プレイヤーに何が見えるかは[ステップ](./steps.md)で、物語がどう進むかは[シーンと即時手続き](./scenes.md)で扱います。

## 実行モデル {#execution-model}

エンジンは物語をプッシュダウン機械として実行します。

- **位置**：どのシーンの、本体のどの地点にいるか。
- **コールスタック**：実行中の `pushdown fn` の呼び出しごとにフレームを 1 つ積んだもの。フレームには戻り先と、その呼び出しのローカル変数が入ります。
- **変数**：各フレームのローカル変数と[物語変数](#story-variables)。どれも[有限型](./kinds.md#finite-types)です。

これらをまとめて σ（シグマ）、つまり**物語状態**と呼びます。コードは次の 2 種類に分かれます。

| 種類 | 例 | 入力を待つか |
| ---- | -- | ------------ |
| ステップ | `s!(…)`、`choice!{…}` | 待つ |
| それ以外 | 呼び出し、`if`、`match`、ループ、代入、`return` | 待たない |

物語はステップに達すると、そのステップの[レンダー](./presentation.md)を実行してステップを表示し、いったん止まります。入力があると再開します。`s!` なら `NextStep`、`choice!` ならアームの選択が入力です。ステップとステップの間のコードは、すべて一瞬で実行されます。データは有限で、ループや呼び出しの循環は必ずステップを通るので（[停止性](./scenes.md#termination)）、入力と入力の間のコードは必ず終わります。そのため、どのステップにどの状態で到達できるかを、コンパイラが判定できます。

演出は一方通行です。物語はレンダーを選びますが、物語のコードから画面を読むことはできません。

## 物語変数 {#story-variables}

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
static mut MET_MIO: bool = false;
```

`static mut` で宣言した変数が**物語変数**です。物語を 1 回実行する間だけ存在する、大域的な物語状態です。

- 型は[有限](./kinds.md#finite-types)でなければならず（`E3008`）、初期化子は[定数式](../syntax/constants.md)でなければなりません（`E3010`）。実行は毎回初期化子から始まるので、値が未設定の変数を読むことはありません。
- 読み書きできるのは `pushdown fn` だけです。`fn` や `const fn` のコードで物語変数を参照するとエラー `E3007` になります。この規則によって、ゲームシステムは物語の中身を読めません。
- 型に参照を含めることはできません（`E3012`）。
- 物語はシングルスレッドなので、書き込みに `unsafe` は要りません。

`pushdown fn` のローカル変数は、普通の `let` や `let mut` の束縛です。呼び出しのフレームに置かれ、再帰呼び出しではそれぞれが自分の値を持ちます。

```kis
enum Route { None, Hanako, Azusa, Both }
```

列挙型は Rust の列挙型と同じです。データが有限であれば、ヴァリアントにデータを持たせることもできます。

不変の `static` は物語変数ではなく、`Setting<T>` か `Meta<T>` のハンドルを保持しなければなりません（`E3011`）。詳しくは[ゲームシステム](./system.md)を参照してください。

## 例：学園祭のハブ {#example-the-festival-hub}

地図から屋台を回り、各屋台は訪問済みかどうかを覚えている、という例です。シーン `StallB`、`StallC`、`DormNight` は省略しています。

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

- `choice!` は 1 つのステップです。値はプレイヤーが選んだアームのヴァリアントになり、普通の `match` でそれを分岐します。
- `Leave` は、3 つの屋台をすべて回った後にだけ選択肢に出ます（`if` ガード）。
- 各屋台から戻るとハブのループに戻り、地図をもう一度表示します。`break` でループを抜けるとハブから戻り、`Day1` は `DormNight()` へ進みます。
- ループを一周するどの経路も `choice!` ステップを通るので、このループは許可されます（[停止性](./scenes.md#termination)）。

戻り先はスタックが覚えているので、`FestivalHub` を複製しなくても `Day1` と `Day2` の両方から呼べ、毎回正しい場所に戻ります。

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
  subgraph stack["2 日目、StallA 実行中のスタック（先頭が上）"]
    s1["StallA: 現在のシーン"]
    s2["FestivalHub: StallA() の直後から再開"]
    s3["Day2: FestivalHub() の直後から再開"]
  end
-->
{{#include ../diagrams/story-call-stack.svg}}
