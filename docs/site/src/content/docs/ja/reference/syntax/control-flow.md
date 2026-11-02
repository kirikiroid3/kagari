---
title: "制御フローとループ"
---

Kagari の制御フローは、Rust と同じ構造化された制御フローです。`if`、`if let`、`match`、`loop`、`while`、`while let`、`for`、`break`、`continue`、`return` があり、ループにはラベルを付けられます。`goto` はありません。ループの中で何ができるかは、そのループを含む関数の[種別](../concepts/kinds.md)によって変わります。規則は[最後の節](#rules-by-function-kind)にまとめてあります。

## `if` と `if let` {#if-and-if-let}

```kis
if AFFECTION_MIO >= 3 {
    MioSmiles();
} else if AFFECTION_MIO == 0 {
    MioIgnores();
} else {
    MioShrugs();
}

let label = if cleared { "Clear" } else { "New" };
```

- 条件には `bool` 型の式を、括弧で囲まずに書きます（`bool` 以外だとエラー `E2001`）。分岐はブロックで、波括弧は省略できません。
- `else` がある `if` は式になり、実行された分岐の値がそのまま `if` 全体の値になります。両方の分岐は同じ型でなければなりません。`else` がない場合、`if` の型は `()` で、ブロックの型も `()` でなければなりません。
- 条件の中に構造体式を書くときは、括弧で囲む必要があります（[式と文](./expressions.md#struct-tuple-and-array-expressions)を参照）。

`if let pattern = value { … } else { … }` は、パターンが一致したときに最初のブロックを実行します。そのブロックの中では、パターンで束縛した変数を使えます。パターンは論駁可能でもかまいません。`if let` は `&&` でほかの条件とつなげられないので、その場合は `if` をもう 1 つ入れ子にしてください。

```kis
if let Some(slot) = last_slot {
    show_slot(slot);
}
```

## `match` {#match}

```kix
match choice!{
    id = "confess",
    t!["Tell her the truth"] => Confess::Yes,
    t!["Say nothing"] => Confess::No,
} {
    Confess::Yes => AzusaConfession(),
    Confess::No => s!(t!["I kept quiet."]),
}
```

`match value { pattern => expression, … }` は、上から順にアームを調べ、パターンが一致してガード（`pattern if condition => …`）も真になった最初のアームを実行します。`match` 全体の値は、実行したアームの値です。アームはすべて同じ型でなければなりません。また、`match` はすべての場合を網羅している必要があります（`E2037`）。パターン、ガード、網羅性については[パターン](./patterns.md#match-and-exhaustiveness)を参照してください。

## `loop` {#loop}

```kis
let mut n = 0;
let first_big = loop {
    n += 1;
    if n * n > 50 {
        break n;
    }
};
```

`loop { … }` は、`break` で抜けるまで本体を繰り返します。`break value` と書くと、`value` がループの値になります。同じループの `break` はすべて同じ型の値を返す必要があり、値を付けない `break` は `()` を返します。`break` を 1 つも含まない `loop` は終わらないので、型は `!` です。そのため、エンディングを返すシーンの末尾など、どんな型の値を書くべき位置にも置けます。

## `while` と `while let` {#while-and-while-let}

```kis
while count < 10 {
    count += 1;
}

while let Some(next) = queue.get(index) {
    index += 1;
}
```

`while condition { … }` は条件が真の間、`while let pattern = value { … }` はパターンが一致する間、本体を繰り返します。どちらも型は `()` で、中の `break` に値は付けられません（`E2001`）。

## `for` ループ {#for-loops}

```kis
for day in 0..7 { … }
for i in 1..=3 { … }
for name in ["Mio", "Azusa", "Hanako"] { … }
for score in scores { … }
```

`for pattern in iterable { … }` は、要素ごとに本体を 1 回ずつ実行します。パターンは論駁不可能でなければなりません（`E2038`）。ループの型は `()` で、中の `break` に値は付けられません。

| 反復の対象 | 取り出される要素 | 順序 |
| ---------- | ---------------- | ---- |
| `[T; N]` | `T` 型の要素 `N` 個 | 先頭から末尾へ |
| `Vec<T>` | `T` 型の各要素 | 先頭から末尾へ |
| `a..b`、`a..=b`（Rust の整数型か `Int` 型の `Range<T>`） | `a` から `b` までの整数（`b` を含まない／含む） | 小さい順 |

これ以外の型を反復しようとするとエラー `E2031` になります。配列への参照も同じです。整数以外の範囲（`Decimal` の範囲など）も `E2031` になります。

範囲を `for` に直接書いた場合、ループ変数の型は、その範囲の値をちょうど表す型になります。たとえば `a` と `b` が `Int<0, 9>` 型なら、`for k in a..b` の `k` は `Int<0, 8>` 型です。両端がリテラルのときは、ループ変数はその範囲の値をとる[整数リテラル](./numbers.md#integer-literals)として扱われます。`for day in 0..7` の `day` は、使われ方に合わせて `Int<0, 6>`（またはもっと広い `Int`）や `u32` などの型になり、型を決める手がかりが何もなければ `i32` になります。パラメータなどほかの場所から受け取った `Range<Int<lo, hi>>` 型の範囲を回す場合、ループ変数は `Int<lo, hi>` 型です。

## ラベル、`break`、`continue` {#labels-break-and-continue}

```kis
'outer: for row in 0..4 {
    for col in 0..4 {
        if blocked(row, col) {
            continue 'outer;
        }
        if done(row, col) {
            break 'outer;
        }
    }
}
```

| 式 | 動作 |
| -- | ---- |
| `break` | いちばん内側のループを抜ける |
| `break value` | いちばん内側の `loop` を、値を返して抜ける |
| `break 'label`、`break 'label value` | ラベル `'label` の付いたループを抜ける |
| `continue` | いちばん内側のループの次の回に進む（`for` ループなら次の要素へ） |
| `continue 'label` | ラベル `'label` の付いたループの次の回に進む |

ラベルは `loop`、`while`、`for` の前に `'name:` の形で書きます。ラベルを付けられるのはループだけで、ブロックやほかの式に付けると構文エラー（`E0001`）になります。外側のどのループにも付いていないラベルを指定するとエラー `E1019`、ループの外で `break` や `continue` を使うとエラー `E1020` になります。`break` と `continue` の型は `!` です。

## `return` {#return}

`return value` は、`value` を返して関数を抜けます。`return` だけを書いた場合は `()` を返します。`return` の型は `!` です。`return` を書かなければ、関数は本体のブロックの値を返します。戻り値の型を宣言した関数は、どの経路でもその型の値を返さなければなりません。値を返さずに本体の末尾まで到達する経路があると型エラー（`E2001`）になり、値を返す関数で `return;` と書いた場合も同じエラーになります。

## 関数種別ごとの規則 {#rules-by-function-kind}

ここまでの構文はどこでも使えますが、種別によって次の制限があります。

| 構文 | `fn` | `const fn` | シーン | 即時手続き、物語の中のクロージャ |
| ---- | ---- | ---------- | ------ | -------------------------------- |
| `if`、`if let`、`match` | 可 | 可 | 可 | 可 |
| `loop`、`while`、`while let` | 可 | 可 | ループを一周するどの経路もステップを通るなら可（`E5001`） | 不可（`E3004`） |
| 配列を回す `for` | 可 | 可 | 可 | 可 |
| 範囲を回す `for` | 可 | 可 | 両端が定数式の場合のみ（`E3006`） | 両端が定数式の場合のみ（`E3006`） |
| `Vec<T>` を回す `for` | 可 | 可 | 定数の場合のみ（`E3006`） | 定数の場合のみ（`E3006`） |
| 再帰 | 可 | 可 | ステップを挟む場合のみ（`E5002`） | 不可（`E3005`） |

`pushdown fn` の中の `for` ループで回せるのは、任意の配列、両端が[定数式](./constants.md)の範囲、そのほかの定数式です。`Vec<T>` は有限型ではないので、物語が実行時に `Vec<T>` を保持することはありません。

`const fn` をコンパイル時に評価する場合、評価は 1 000 000 ステップまでに制限されています（`E4003`）。そのため、終わらないループがあってもビルドが止まったままにはならず、エラーとして報告されます。

### 物語の停止性 {#termination-in-the-story}

物語がステップとステップの間に行う処理は、プレイヤーの入力を待たずに一瞬で実行されます。そのため、この処理は必ず終わらなければなりません。Kagari では、これをコンパイラが検査できるように言語を設計しています。

- **即時手続き**（`#[scene]` を付けない `pushdown fn`）と物語の中のクロージャには、上限のないループも再帰も書けません。`loop` と `while` は使えず（`E3004`）、`for` で回せるのは配列と定数の範囲だけで（`E3006`）、即時手続きどうしで呼び出しが循環することもありません（`E3005`）。したがって、即時手続きの呼び出しは必ず終わります。
- **シーン**では `loop` と `while` を使えますが、ループ本体を一周するどの経路も、ステップを通らなければなりません（`E5001`）。また、シーンが自分自身を呼び出せるのは、直接でもほかのシーンを経由する場合でも、ステップを通った後だけです（`E5002`）。ステップを通らずに戻ってくる可能性があるシーンの呼び出しは、ステップを通らないものとして扱います。

```kix
static mut READY: bool = false;

#[scene]
pushdown fn Wait() {
    loop {
        if READY {
            s!(t!["Ready."]);
        }
        // E5001: when READY is false, the loop goes around without a step
    }
}
```

一周するたびにプレイヤーの入力を待つハブなら、問題なく受け付けられます。

```kix
enum Map { A, Leave }

#[scene]
pushdown fn FestivalHub() {
    loop {
        match choice!{
            id = "map",
            t!["Stall A"] => Map::A,
            t!["Leave"] => Map::Leave,
        } {
            Map::A => StallA(),
            Map::Leave => break,
        }
    }
}
```

この検査では、どの分岐も実行されうるものとして扱います。ステップを通らない経路が実際には通らないことまでは証明しようとしません。シーンの中に 1 048 576 個を超える要素を回す `for` ループがあると、物語の解析は途中で打ち切られます（`W5007`）。規則の詳細は[シーンと即時手続き](../concepts/scenes.md#termination)を参照してください。

## `goto` がない理由 {#no-goto}

`goto` はなく、コード中の位置を示すラベルもありません。ハブは `loop` で、途中で終わるシーンは `return` で、分岐は `if` か `match` で書きます。制御がほかの関数や、ブロックやループの途中に飛び込むことはないので、呼び出しと戻りは常に入れ子の関係になります。物語の解析はこの性質を前提にしています。
