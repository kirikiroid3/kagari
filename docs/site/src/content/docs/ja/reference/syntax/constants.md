---
title: "定数式"
---

**定数式**は、コンパイラーがコンパイル時に計算する式です。物語が実行時に保持できないもの（時間、色、レンダーなど）は定数が運びます。また定数によって、コンパイラーはテキスト効果、モーション、アセットのパスといった値をゲームの実行前に検査できます。

## 定義 {#definition}

式は、次のいずれかであれば定数です。

- リテラル
- `const` アイテム、ユニット構造体やユニットバリアント、名前付き関数、タプル構造体やタプルバリアントのコンストラクター、const ジェネリクスパラメーター、不変の `static`（`Setting` または `Meta` のハンドルで、値はハンドルそのもの）、定数ローカル（後述）へのパス
- オペランドがすべて定数である、構造体・enum・タプル・配列・範囲の式、演算子（単項、二項、添字、共有借用 `&`）、フィールドアクセス、ブロック、`if`、`if let`、`match`。このようなブロックの文は、不変の変数だけを束縛し初期化子が定数である `let` 文、アイテム、定数の式文に限られます
- 引数がすべて定数である `const fn` またはタプルコンストラクターの呼び出し。メソッド呼び出しではレシーバーも引数として数え、可変に借用してはなりません。`&mut self` のメソッドの呼び出しは定数ではありません
- `asset::<A>("…")`。これもそのような呼び出しです
- 何もキャプチャしないクロージャ

**定数ローカル**とは、初期化子が定数である `let` の不変の束縛、または検査対象が定数である `if let` や `match` のアームのパターンの不変の束縛です。

次のものは決して定数になりません: 引数、物語変数（`static mut`）、`let mut` の束縛、`fn` と `pushdown fn` の呼び出し結果、ループ、`break`、`continue`、`return`、`?`、代入、`&mut` 借用、変数をキャプチャするクロージャ。

```kis
const FADE: Transition = fade(ms(500));       // a const fn call with constant arguments
const CENTER: Point = point(1280 / 2, 720);   // an operator on literals
const DIM: Color = {
    let base = rgb(40, 40, 60);               // a constant local
    Color { r: base.r, g: base.g, b: base.b, a: 160 }
};
```

## `const fn` の呼び出し {#const-fn-calls}

引数が定数である `const fn` の呼び出しは定数式です。コンパイラーは関数を実行し、その結果を使います。関数の本体そのものは定数式である必要はありません。`let mut`、ループ、代入を使えます。必要なのは純粋な `const fn` であることだけです（[関数種別](../concepts/kinds.md#const-fn)を参照）。

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

コンパイル時の評価は最大 1 000 000 ステップまでです（`E4003`）。

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

物語が実行時に保持するのは[有限型](../concepts/kinds.md#finite-types)の値だけですが、`pushdown fn` の中には任意の型の定数式を書けます。コンパイラーがそれを計算するからです。`render = fade_to_room`、`fade(ms(500))`、`rgb(200, 0, 0)` はどれもシーンの中で使えますが、物語変数から計算した `Duration` は使えません（`E3008`）。

`pushdown fn` の中では、極大の定数部分式はすべてコンパイル時に計算されます。そのため、そのエラー（オーバーフローや不正なモーションなど）は実行時パニックではなくコンパイルエラーになります。引数がすべて定数ではないがすべて有限型である `const fn` の呼び出しは、代わりに可能なすべての引数値についてコンパイル時に計算されます。[物語からの `const fn` の呼び出し](../concepts/kinds.md#calling-const-fn-from-the-story)を参照してください。

## 型レベル定数 {#type-level-constants}

配列の長さ、`Int` の境界、const ジェネリクス引数は型の一部なので、コンパイラーは型を検査する間にその値を必要とします。これらには、より小さな言語を使います。

- 整数リテラル
- 単項 `-` と演算子 `+`、`-`、`*`、`/`、`%`
- 最後の式だけを含むブロック `{ … }`
- 単独の const ジェネリクスパラメーター（`N` は可、`N + 1` は不可）
- 初期化子がそれ自体型レベル定数である `const` アイテムへのパス

`const fn` の呼び出しを含め、それ以外はすべて `E2002` です。自分自身に依存する `const` も `E2002` です。オーバーフロー、ゼロ除算、大きすぎる整数リテラルは `E2003` です。リテラル以外の const ジェネリクス引数は波括弧で囲んで書きます。

```kis
const SLOTS: usize = 4;
const MAX_LOVE: i64 = 10;

static mut FLAGS: [bool; SLOTS] = [false; SLOTS];
static mut LOVE: Int<0, { MAX_LOVE * 2 }> = 0;
```

## 定数評価エラー {#const-evaluation-errors}

コンパイラーが定数を計算して失敗すると、`E4xxx` のコードを持つコンパイルエラーになります。例と修正方法は[定数評価 (E4xxx)](../../diagnostics/eval.md) を参照してください。

| コード | エラー |
| ------ | ------ |
| [`E4001`](../../diagnostics/eval.md#e4001) | 整数の結果が型に収まらない |
| [`E4002`](../../diagnostics/eval.md#e4002) | ゼロ除算（`Rat::new(n, 0)` を含む） |
| [`E4003`](../../diagnostics/eval.md#e4003) | 評価が 1 000 000 ステップを超えた |
| [`E4004`](../../diagnostics/eval.md#e4004) | 物語からの `const fn` 呼び出しで、計算すべき引数の組み合わせが 65 536 を超えた |
| [`E4005`](../../diagnostics/eval.md#e4005) | 配列や `Vec` の末尾を越える添字 |
| [`E4006`](../../diagnostics/eval.md#e4006) | 値が自分自身に依存する `const` |
| [`E4007`](../../diagnostics/eval.md#e4007) | `static` の初期化子以外で作られた `Setting` または `Meta` のハンドル |
| [`E4008`](../../diagnostics/eval.md#e4008) | 不正なモーションやシェイク |
| [`E4009`](../../diagnostics/eval.md#e4009) | パラメーターが範囲外のイージング |
| [`E4010`](../../diagnostics/eval.md#e4010) | 1 つのテキストに同じテキストマークが 2 回 |
| [`E4011`](../../diagnostics/eval.md#e4011) | タイムラインがテキストマークに固定されている間の `begin_text` |
| [`E4012`](../../diagnostics/eval.md#e4012) | 不透明度、音量、アンカーなど、`0.0..=1.0` の外にある割合 |
| [`E4013`](../../diagnostics/eval.md#e4013) | `u32` のミリ秒に収まらない `Duration` |
