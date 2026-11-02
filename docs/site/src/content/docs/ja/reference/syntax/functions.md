---
title: "関数"
---

関数は、パラメータを受け取って結果を計算するアイテムです。関数にはそれぞれ[種別](../concepts/kinds.md)があり、本体で何ができるかは種別で決まります。構文はどの種別でも同じです。

## 宣言 {#declarations}

```text
Function   = Attribute* "pub"? Kind? "fn" Name Generics? "(" Params ")" ("->" Type)? Body
Kind       = "const" | "pushdown"
Body       = Block | ";"
```

```kis
enum Answer { Yes, No }
static mut TRUST: Int<0, 5> = 0;

fn quit_game() { nav::quit() }
pub const fn twice(x: u32) -> u32 { x * 2 }
pushdown fn reset_trust() { TRUST = 0; }
#[scene]
pub pushdown fn Ask() -> Answer { Answer::Yes }
```

- 種別のキーワードは、属性と `pub` の後、`fn` の直前に書きます（`pub const fn`、`#[scene] pub pushdown fn`）。1 つの関数に付けられる種別のキーワードは 1 つまでです。`unsafe`、`async`、`extern` はありません。
- 本体のない関数（`;` で終わるもの）には `#[builtin]` が必要です。`#[builtin]` は標準ライブラリでしか使えません（[`E1010`](../../diagnostics/modules.md#e1010)）。
- 慣習として、シーンの名前は PascalCase（`Ask`）、それ以外の関数の名前は snake_case にします。

## 種別 {#kinds}

| キーワード | 種別 | 本体で扱えるもの |
| ---------- | ---- | ---------------- |
| `fn` | 実行時のチューリング完全なコード。ゲームシステムとその UI | 物語以外のすべて |
| `pushdown fn` | 物語。`#[scene]` を付ければシーン、付けなければ即時手続き | 有限型、物語変数、（シーンでは）ステップ |
| `const fn` | 純粋なコード。必要に応じてコンパイル時に評価される | 純粋なものすべて |

種別どうしの呼び出し規則と、各種別の意味については[関数種別](../concepts/kinds.md)を参照してください。コンパイラが種別を推論することはありません。関数が `const` になるのは、`const` と書いたときだけです。

## パラメータ {#parameters}

パラメータは、論駁不可能なパターンと型の組で書きます。

```kis
const fn area((w, h): (u32, u32)) -> u32 { w * h }
const fn fade_in(mut screen: Screen) -> Screen { screen }
fn ignore(_: u32) {}
```

- 型の指定は必須です。既定値、可変長引数、名前付き引数はありません。
- パターンは必ず一致するものでなければなりません（[`E2038`](../../diagnostics/types.md#e2038)）。使えるのは、名前、`mut name`、`_`、およびそれらを組み合わせたタプルや構造体のパターンです。
- 引数は値渡しで、関数は値のコピーを受け取ります。パラメータに `mut` を付けると、関数の中でそのコピーを変更できます。レンダー関数が受け取った画面に手を加えて返せるのは、この仕組みによるものです。
- 呼び出し側の値を関数の中から変更したい場合は、参照 `&mut T` を渡します（[参照](./values.md#references)を参照）。
- 引数の数が合わない呼び出しはエラー [`E2024`](../../diagnostics/types.md#e2024) になります。

## 戻り値の型と本体 {#return-type-and-body}

結果の型は `-> R` で宣言します。省略した場合、結果は `()` になります。本体はブロックで、その最後の式が結果になります。途中で戻るには `return e;` を使います。

```kis
enum Mood { Warm, Polite }

const fn mood(affection: Int<0, 5>) -> Mood {
    if affection >= 4 {
        return Mood::Warm;
    }
    Mood::Polite
}
```

本体のどの経路を通っても、戻り値の型の値を返す必要があります。値を返さずに本体の終わりまで到達しうる経路があると、型エラーになります。`break` のない `loop` は値を生まないので、値を返す必要もありません。

`?` 演算子を使うと、`None` や `Err` のときに関数をその場で終えられます。`Option<_>` を返す関数では、`x?` は `Some` の中身を取り出すか、`None` を返します。`Result<_, E>` を返す関数では、`Ok` の中身を取り出すか、その `Err` を返します（[`E2034`](../../diagnostics/types.md#e2034)）。

## ジェネリック関数 {#generic-functions}

```kis
fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn first<T, const N: usize>(items: [T; N]) -> T {
    items[0]
}

fn demo() -> bool {
    same(first([1u8, 2, 3]), first::<u8, 2>([1, 4]))
}
```

関数には、[境界](./types.md#trait-bounds)付きの型パラメータや定数パラメータを付けられます。ジェネリクス引数は呼び出しから推論されますが、ターボフィッシュ `f::<…>` で明示することもできます。ジェネリック関数は単相化され、使われた引数の組み合わせごとに別々のコピーができます。`pushdown fn` では、型引数をすべて[有限型](./types.md#finite-types)にする必要があります。

## メソッドと関連関数 {#methods-and-associated-functions}

[`impl` ブロック](./items.md#implementations)の中の関数は、対象の型の**関連関数**です。そのうち最初のパラメータが `self` のものを**メソッド**と呼びます。

```kis
struct Gauge {
    value: Int<0, 10>,
}

impl Gauge {
    const MAX: Int<0, 10> = 10;

    pub const fn new() -> Gauge {
        Gauge { value: 0 }
    }

    pub const fn is_full(&self) -> bool {
        self.value == Self::MAX
    }

    pub const fn fill(&mut self, amount: Int<0, 10>) {
        self.value += amount;           // saturates at 10
    }
}

const fn full_gauge() -> bool {
    let mut gauge = Gauge::new();
    gauge.fill(7);
    gauge.fill(7);
    gauge.is_full()                     // true
}
```

| レシーバー | 意味 | メソッドが受け取るもの |
| ---------- | ---- | ---------------------- |
| `self` | `self: Self` | 値のコピー |
| `mut self` | `mut self: Self` | 変更してよい値のコピー |
| `&self` | `self: &Self` | 値への参照 |
| `&mut self` | `self: &mut Self` | 可変参照（変更が呼び出し側に反映される） |

- `self: Box<Self>` のようにレシーバーの型を明示する書き方には対応していません。
- メソッドは `value.method(args)` の形で呼び出します。レシーバーの借用や参照外しは自動で行われるので、`gauge.fill(7)` と書けば `&mut gauge` が渡ります。`&mut self` のメソッドを呼ぶには、レシーバーが変更可能な場所でなければなりません（[`E3017`](../../diagnostics/kinds.md#e3017)）。
- 関連関数は `Type::function(args)` の形で呼び出します。メソッドも、レシーバーを最初の引数にすれば同じ形で呼べます（`Gauge::is_full(&gauge)`）。標準ライブラリのトレイトのメソッドは、トレイト名を通して呼ぶこともできます（`Integer::into(value)`）。
- `impl` の中では、`Self` が対象の型を、`self` がレシーバーを表します。
- 関連関数と関連定数は、`pub` を付けない限り非公開です。
- 存在しないメソッドを呼ぶとエラー [`E2022`](../../diagnostics/types.md#e2022) になります。

## 引数としてのクロージャ {#closures-as-arguments}

[クロージャ型](./values.md#closure-and-pointer-types)のパラメータには、種別が同じクロージャ、名前付き関数、関数ポインタを渡せます。ゲームシステムがハンドラを渡すときも、物語がちょっとした処理を呼び出し先に渡すときも、この仕組みを使います。

```kix
fn Counter() -> Element {
    let (count, set_count) = signal(0u32);
    button!{on_click = || set_count(count() + 1), t!["Count: " {count()}]}
}

pushdown fn twice(f: pushdown Fn()) {
    f();
    f();
}
```

関数ポインタ型のパラメータ（たとえば `nav::call` の `overlay: fn() -> Overlay`）には、名前付き関数か、何もキャプチャしないクロージャを渡せます。物語の中ではクロージャは第二級の値です。渡したり呼び出したりはできますが、保存したり戻り値にしたりはできません（[クロージャ](./values.md#closures)を参照）。

## 関数ポインタ {#function-pointers}

関数名はその関数自身の型を持つ値で、種別が同じ[関数ポインタ](./values.md#closure-and-pointer-types)に変換できます。

```kis
fn apply(f: fn(u32) -> u32, x: u32) -> u32 {
    f(x)
}

fn double(x: u32) -> u32 {
    x * 2
}

fn demo() -> u32 {
    apply(double, 21)
}
```

タプル構造体やタプルバリアントの名前は、`const fn` のコンストラクターです。たとえば `Some` は `const fn(T) -> Option<T>` 型の値です。関数ポインタどうしは、同じ関数を指すかどうかで比較します。関数でないものを呼び出そうとするとエラー [`E2025`](../../diagnostics/types.md#e2025) になります。

## 再帰と停止性 {#recursion-and-termination}

関数が自分自身を呼び出せるかどうか、どんなループを書けるかは、種別によって決まります。

| 種別 | 再帰 | ループ | 停止を保証する仕組み |
| ---- | ---- | ------ | -------------------- |
| `fn` | 可 | すべて可 | ランタイムの呼び出しの深さに上限があり、超えると VM パニック |
| `const fn` | 可 | すべて可 | コンパイル時の評価は 1,000,000 ステップで打ち切り（[`E4003`](../../diagnostics/eval.md#e4003)） |
| シーン（`#[scene] pushdown fn`） | ステップを挟めば可 | ステップを挟めばすべて可 | どのループも、どの呼び出しの循環も、ステップを通る必要がある（[`E5001`](../../diagnostics/analysis.md#e5001)、[`E5002`](../../diagnostics/analysis.md#e5002)） |
| 即時手続き（`pushdown fn`） | 不可（[`E3005`](../../diagnostics/kinds.md#e3005)） | 配列か定数の範囲を回す `for` だけ（[`E3004`](../../diagnostics/kinds.md#e3004)、[`E3006`](../../diagnostics/kinds.md#e3006)） | 必ず終わる |

この規則により、プレイヤーの入力から次の入力までの間に動く物語のコードは、必ず終わります。シーンの規則については[停止性](../concepts/scenes.md#termination)で説明します。

## 入れ子の関数 {#nested-functions}

関数もほかのアイテムと同じく、ブロックの中で宣言できます（[ブロック内のアイテム](./items.md#items-in-blocks)を参照）。入れ子の関数も普通のアイテムなので、外側の関数のローカル変数はキャプチャしません。ローカル変数をキャプチャしたいときはクロージャを使ってください。

## `main` 関数 {#the-main-function}

ルートモジュールでは `fn main() -> System` を宣言します。`main` は普通の `fn` で、ゲームシステムのタイトル、解像度、モード、設定、フォントを記述します。エンジンは起動時に `main` を 1 回だけ呼び出します。ルートモジュールに `main` がないとエラー [`E7001`](../../diagnostics/emit.md#e7001) になります。詳しくは[ゲームシステム](../concepts/system.md)を参照してください。
