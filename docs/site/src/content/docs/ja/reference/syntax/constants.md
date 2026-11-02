---
title: "定数式"
---

**定数式**とは、コンパイラがコンパイル時に計算する式のことです。時間、色、レンダーなど、物語が実行時に保持できない値は定数として扱います。また、定数であればテキスト効果、モーション、アセットのパスといった値を、ゲームの実行前にコンパイラが検査できます。

## 定義 {#definition}

次のいずれかに当たる式は定数です。

- リテラル
- 次のものへのパス：`const` アイテム、ユニット構造体やユニットバリアント、名前付き関数、タプル構造体やタプルバリアントのコンストラクター、const ジェネリクスパラメータ、不変の `static`（`Setting` または `Meta` のハンドルで、値はハンドルそのもの）、定数ローカル（後述）
- オペランドがすべて定数である、構造体・enum・タプル・配列・範囲の式、演算子（単項、二項、添字、共有借用 `&`）、フィールドアクセス、ブロック、`if`、`if let`、`match`。ブロック内の文は、初期化子が定数で不変の変数だけを束縛する `let` 文、アイテム、定数の式文に限ります
- 引数がすべて定数である `const fn` またはタプルコンストラクターの呼び出し。メソッド呼び出しではレシーバーも引数に数え、レシーバーを可変に借用してはいけません。`&mut self` のメソッドの呼び出しは定数になりません
- `asset::<A>("…")`（これも上の呼び出しの一種です）
- 何もキャプチャしないクロージャ

**定数ローカル**とは、初期化子が定数である `let` による不変の束縛、または検査対象が定数である `if let` や `match` のアームで、パターンが行う不変の束縛のことです。

次のものは定数になりません：引数、物語変数（`static mut`）、`let mut` の束縛、`fn` や `pushdown fn` の呼び出し結果、ループ、`break`、`continue`、`return`、`?`、代入、`&mut` 借用、変数をキャプチャするクロージャ。

```kis
const FADE: Transition = fade(ms(500));       // a const fn call with constant arguments
const CENTER: Point = point(1280 / 2, 720);   // an operator on literals
const DIM: Color = {
    let base = rgb(40, 40, 60);               // a constant local
    Color { r: base.r, g: base.g, b: base.b, a: 160 }
};
```

## `const fn` の呼び出し {#const-fn-calls}

引数が定数である `const fn` の呼び出しは定数式です。コンパイラが関数を実行し、その結果を使います。関数の本体まで定数式である必要はなく、`let mut`、ループ、代入も使えます。条件は、純粋な `const fn` であることだけです（[関数種別](../concepts/kinds.md#const-fn)を参照）。

```kis
const fn triangle(n: u32) -> u32 {
    let mut total = 0;
    for i in 1..=n {
        total += i;
    }
    total
}

const T10: u32 = triangle(10);   // 55, computed at compile time
```

コンパイル時の評価は 1 000 000 ステップまでです（`E4003`）。

## 定数式が必要な場所 {#where-constant-expressions-are-required}

| 場所 | 要件 | 診断 |
| ---- | ---- | ---- |
| `const` アイテムの初期化子 | 定数式 | `E3010` |
| 物語変数（`static mut`）の初期化子 | 定数式 | `E3010` |
| ハンドル（不変の `static`）の初期化子 | `Setting` または `Meta` のハンドルを作る定数式 | `E3010`、`E3011`、`E4007` |
| 配列の長さ `[T; N]`、繰り返し数 `[value; N]` | [型レベル定数](#type-level-constants) | `E2002`、`E2003` |
| `Int<lo, hi>` の境界 | `i64` に収まり `lo <= hi` である型レベル定数 | `E2002`、`E2003`、`E2004` |
| const ジェネリクス引数 | 型レベル定数 | `E2002`、`E2003` |
| `t![…]` の中のテキスト効果 `![…](effect)` の効果 | 型 `TextEffect` の定数式 | `E3022` |
| `pushdown fn` の中の非有限型の値 | 定数式 | `E3008` |
| `pushdown fn` の中の `for` ループの範囲の両端 | 定数式 | `E3006` |
| `asset::<A>(…)` と `system_font(…)` のパス | 文字列リテラル | `E2028` |
| `choice!{…}` の `id` | 文字列リテラル | `E0001`、`E3019` |

### 物語の中の定数 {#constants-in-the-story}

物語が実行時に保持できるのは[有限型](../concepts/kinds.md#finite-types)の値だけですが、`pushdown fn` の中にはどんな型の定数式でも書けます。コンパイラが計算してくれるからです。`render = fade_to_room`、`fade(ms(500))`、`rgb(200, 0, 0)` はどれもシーンの中で使えます。一方、物語変数から計算した `Duration` は使えません（`E3008`）。

`pushdown fn` の中では、定数である部分式のうち最大のものがすべてコンパイル時に計算されます。そのため、オーバーフローや不正なモーションなどのエラーは、実行時のパニックではなくコンパイルエラーになります。引数がすべて定数ではないものの、すべて有限型である `const fn` の呼び出しは、とりうる引数値すべてについてコンパイル時に計算します。[物語からの `const fn` の呼び出し](../concepts/kinds.md#calling-const-fn-from-the-story)を参照してください。

## 型レベル定数 {#type-level-constants}

配列の長さ、`Int` の境界、const ジェネリクス引数は型の一部なので、コンパイラは型検査の途中でその値を必要とします。これらには、通常の定数式より小さな言語を使います。

- 整数リテラル
- 単項 `-` と演算子 `+`、`-`、`*`、`/`、`%`
- 最後の式だけを含むブロック `{ … }`
- 単独の const ジェネリクスパラメータ（`N` は可、`N + 1` は不可）
- 初期化子がそれ自体型レベル定数である `const` アイテムへのパス

これ以外は、`const fn` の呼び出しも含めてすべてエラー `E2002` になります。自分自身に依存する `const` も `E2002` です。オーバーフロー、ゼロ除算、大きすぎる整数リテラルは `E2003` になります。リテラル以外の const ジェネリクス引数は波括弧で囲んで書きます。

```kis
const SLOTS: usize = 4;
const MAX_LOVE: i64 = 10;

static mut FLAGS: [bool; SLOTS] = [false; SLOTS];
static mut LOVE: Int<0, { MAX_LOVE * 2 }> = 0;
```

## 定数評価エラー {#const-evaluation-errors}

コンパイラが定数の計算に失敗すると、`E4xxx` のコードでコンパイルエラーになります。例と直し方は[定数評価 (E4xxx)](../../diagnostics/eval.md) を参照してください。

| コード | エラー |
| ------ | ------ |
| [`E4001`](../../diagnostics/eval.md#e4001) | 整数の結果が型に収まらない |
| [`E4002`](../../diagnostics/eval.md#e4002) | ゼロ除算（`Rat::new(n, 0)` を含む） |
| [`E4003`](../../diagnostics/eval.md#e4003) | 評価が 1 000 000 ステップを超えた |
| [`E4004`](../../diagnostics/eval.md#e4004) | 物語から `const fn` を呼ぶとき、計算すべき引数の組み合わせが 65 536 を超えた |
| [`E4005`](../../diagnostics/eval.md#e4005) | 配列や `Vec` の末尾を越える添字 |
| [`E4006`](../../diagnostics/eval.md#e4006) | 値が自分自身に依存する `const` |
| [`E4007`](../../diagnostics/eval.md#e4007) | `static` の初期化子以外で作られた `Setting` または `Meta` のハンドル |
| [`E4008`](../../diagnostics/eval.md#e4008) | 不正なモーションやシェイク |
| [`E4009`](../../diagnostics/eval.md#e4009) | パラメータが範囲外のイージング |
| [`E4010`](../../diagnostics/eval.md#e4010) | 1 つのテキストで同じテキストマークを 2 回使った |
| [`E4011`](../../diagnostics/eval.md#e4011) | タイムラインがテキストマークに固定されている間に `begin_text` を使った |
| [`E4012`](../../diagnostics/eval.md#e4012) | 不透明度、音量、アンカーなどの割合が `0.0..=1.0` の外にある |
| [`E4013`](../../diagnostics/eval.md#e4013) | `u32` のミリ秒に収まらない `Duration` |
