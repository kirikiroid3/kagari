# 制御フローとループ {#control-flow-and-loops}

Kagari の制御フローは Rust の構造化された制御フローです。`if`、`if let`、`match`、`loop`、`while`、`while let`、`for`、`break`、`continue`、`return` があり、ループにはラベルを付けられます。`goto` はありません。ループで何ができるかは、それを含む関数の[種別](../concepts/kinds.md)によって決まります。規則は[最後の節](#rules-by-function-kind)にまとめてあります。

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

- 条件は `bool` の式で（そうでなければ `E2001`）、括弧なしで書きます。分岐はブロックで、波括弧は必須です。
- `else` があれば、`if` は実行された分岐の値を値とする式です。両方の分岐は同じ型を持ちます。`else` がなければ型は `()` で、ブロックも型 `()` でなければなりません。
- 条件に構造体式を書くときは括弧で囲みます。[式と文](./expressions.md#struct-tuple-and-array-expressions)を参照してください。

`if let pattern = value { … } else { … }` は、パターンが一致したときに最初のブロックを実行し、そこではパターンの束縛が有効です。パターンは反駁可能でもかまいません。`if let` を `&&` で他の条件と組み合わせることはできないので、2 つ目の `if` を入れ子にしてください。

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

`match value { pattern => expression, … }` は、パターンが一致し、ガード（`pattern if condition => …`）が真である最初のアームを実行します。その値はそのアームの値で、すべてのアームは同じ型を持ちます。`match` は網羅的でなければなりません（`E2037`）。パターン、ガード、網羅性については[パターン](./patterns.md#match-and-exhaustiveness)を参照してください。

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

`loop { … }` は `break` で抜けるまで本体を繰り返します。`break value` は `value` をループの値にします。1 つのループのすべての `break` は同じ型の値を与え、値なしの `break` は `()` を与えます。`break` を 1 つも持たない `loop` は終わらず、型は `!` です。そのため、エンディングを返すシーンの末尾のように、任意の型の値が期待される場所に置けます。

## `while` と `while let` {#while-and-while-let}

```kis
while count < 10 {
    count += 1;
}

while let Some(next) = queue.get(index) {
    index += 1;
}
```

`while condition { … }` は条件が真である間、本体を実行します。`while let pattern = value { … }` はパターンが一致する間、本体を実行します。どちらも型は `()` で、その中の `break` は値を取りません（`E2001`）。

## `for` ループ {#for-loops}

```kis
for day in 0..7 { … }
for i in 1..=3 { … }
for name in ["Mio", "Azusa", "Hanako"] { … }
for score in scores { … }
```

`for pattern in iterable { … }` は要素ごとに 1 回本体を実行します。パターンは反駁不能でなければなりません（`E2038`）。ループの型は `()` で、その中の `break` は値を取りません。

| 反復対象 | 要素 | 順序 |
| -------- | ---- | ---- |
| `[T; N]` | `N` 個の要素（型 `T`） | 先頭から末尾 |
| `Vec<T>` | 要素（型 `T`） | 先頭から末尾 |
| `a..b`、`a..=b`: Rust 整数型または `Int` 型の `Range<T>` | `a` から `b` までの整数（`b` を含まない、または含む） | 昇順 |

それ以外の型は `E2031` で、配列への参照も含みます。整数以外（たとえば `Decimal`）の範囲も `E2031` です。

範囲を `for` の中に直接書いた場合、ループ変数の型はちょうどその範囲の値を表します。`for day in 0..7` の `day` は `Int<0, 6>`、`for i in 1..=3` の `i` は `Int<1, 3>` です。`a` と `b` が `Int<0, 9>` 型なら、`for k in a..b` の `k` は `Int<0, 8>` です。引数など別の場所から来た `Range<Int<lo, hi>>` 型の範囲値では、ループ変数は `Int<lo, hi>` です。

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

| 式 | 効果 |
| -- | ---- |
| `break` | 最も内側のループを抜けます |
| `break value` | 最も内側の `loop` を値付きで抜けます |
| `break 'label`、`break 'label value` | ラベル `'label` のループを抜けます |
| `continue` | 最も内側のループの次の反復を始めます。`for` ループでは次の要素に進みます |
| `continue 'label` | ラベル `'label` のループの次の反復を始めます |

ラベルは `loop`、`while`、`for` の前に書く `'name:` です。ラベルはループにだけ付けられ、ブロックやその他の式に付けると構文エラー（`E0001`）です。囲むループが宣言していないラベルは `E1019`、ループの外の `break` や `continue` は `E1020` です。`break` と `continue` の型は `!` です。

## `return` {#return}

`return value` は `value` を持って関数を抜け、`return` 単独は `()` を返します。型は `!` です。`return` がなければ、関数は本体のブロックの値を返します。戻り値の型を持つ関数は、すべての経路でその型の値を生まなければなりません。値なしで本体の末尾に達する経路は型エラー（`E2001`）で、値を返す関数での `return;` も同様です。

## 関数種別ごとの規則 {#rules-by-function-kind}

上の構文はどこでも使えますが、次の制限があります。

| 構文 | `fn` | `const fn` | シーン | 即時手続き、物語の中のクロージャ |
| ---- | ---- | ---------- | ------ | -------------------------------- |
| `if`、`if let`、`match` | 可 | 可 | 可 | 可 |
| `loop`、`while`、`while let` | 可 | 可 | ループを一周するすべての経路がステップを通るなら可（`E5001`） | 不可（`E3004`） |
| 配列に対する `for` | 可 | 可 | 可 | 可 |
| 範囲に対する `for` | 可 | 可 | 両端が定数式であること（`E3006`） | 両端が定数式であること（`E3006`） |
| `Vec<T>` に対する `for` | 可 | 可 | 定数のものだけ（`E3006`） | 定数のものだけ（`E3006`） |
| 再帰 | 可 | 可 | ステップを通る場合だけ（`E5002`） | 不可（`E3005`） |

`pushdown fn` の中の `for` ループは、任意の配列、両端が[定数式](./constants.md)である範囲、またはその他の定数式を反復できます。`Vec<T>` は有限型ではないので、物語が実行時に保持することはありません。

`const fn` をコンパイル時に評価するとき、評価は 1 000 000 ステップに制限されます（`E4003`）。そのため、そこで終わらないループはビルドを止めずに報告されます。

### 物語の中の停止性 {#termination-in-the-story}

物語が 2 つのステップの間に行うことは、プレイヤーの入力なしに瞬時に実行されるので、必ず終わらなければなりません。言語はこれを検査可能にしています。

- **即時手続き**（`#[scene]` のない `pushdown fn`）と物語の中のクロージャには、非有界なループも再帰もありません。`loop` も `while` も使えず（`E3004`）、`for` は配列と定数の範囲に対してだけ使え（`E3006`）、即時手続きどうしの呼び出しの循環もありません（`E3005`）。したがって、その呼び出しは必ず終わります。
- **シーン**では `loop` と `while` を使えますが、ループ本体を一周するすべての経路がステップを通らなければなりません（`E5001`）。また、シーンが直接または他のシーンを通して自分自身を呼べるのは、ステップの後だけです（`E5002`）。ステップを通らずに戻る可能性のあるシーンの呼び出しは、ステップなしとして数えられます。

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

一周ごとにプレイヤーを待つハブは受け入れられます。

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

検査はすべての分岐を起こりうるものとして扱い、ステップのない経路が実際には通られないことを証明しようとはしません。シーンの中で 1 048 576 個を超える要素を反復する `for` ループがあると、物語の解析は途中で打ち切られます（`W5007`）。規則の全体は[シーンと即時手続き](../concepts/scenes.md#termination)にあります。

## `goto` はない {#no-goto}

`goto` はなく、コードの位置を示すラベルもありません。ハブは `loop`、途中で終わるシーンは `return`、分岐は `if` か `match` で書きます。制御が別の関数やブロック・ループの途中に飛び込むことはないので、呼び出しと戻りは常に入れ子になります。物語の解析はこの性質に依存しています。
