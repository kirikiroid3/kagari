# 関数 {#functions}

関数はパラメータを受け取って結果を計算するアイテムです。すべての関数は[種別](../concepts/kinds.md)を持ち、それが本体でできることを決めます。構文はどの種別でも同じです。

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

- 種別のキーワードは、属性と `pub` の後、`fn` の直前に書きます。`pub const fn`、`#[scene] pub pushdown fn` のようになります。種別のキーワードは 1 つまでです。`unsafe`、`async`、`extern` はありません。
- 本体のない関数（`;`）は `#[builtin]` でなければならず、それを使えるのは標準ライブラリだけです（[`E1010`](../diagnostics/modules.md#e1010)）。
- 慣習として、シーンは PascalCase（`Ask`）、それ以外の関数は snake_case で名付けます。

## 種別 {#kinds}

| キーワード | 種別 | 本体 |
| ---------- | ---- | ---- |
| `fn` | チューリング完全な実行時のコード。ゲームシステムとその UI | 物語以外のすべて |
| `pushdown fn` | 物語。`#[scene]` 付きならシーン、なければ即時手続き | 有限型、物語変数、シーンの中ではステップ |
| `const fn` | 純粋なコード。必要な所でコンパイル時に評価されます | 純粋なものすべて |

種別間の呼び出し規則と各種別の意味は[関数種別](../concepts/kinds.md)にあります。コンパイラが種別を推論することはありません。関数が `const` になるのは、そう書いた場合だけです。

## パラメータ {#parameters}

各パラメータは、論駁不可能なパターンと型の組です。

```kis
const fn area((w, h): (u32, u32)) -> u32 { w * h }
const fn fade_in(mut screen: Screen) -> Screen { screen }
fn ignore(_: u32) {}
```

- 型は必須です。既定値、可変長パラメータ、名前付き引数はありません。
- パターンは必ず一致しなければなりません（[`E2038`](../diagnostics/types.md#e2038)）。名前、`mut name`、`_`、またはこれらからなるタプルや構造体のパターンです。
- 引数は値で渡され、関数はコピーを受け取ります。`mut` パラメータにすると関数はそのコピーを変更できます。レンダー関数が受け取った画面を変更して返すのはこの仕組みです。
- 呼び出し側の値を関数に変更させるには、参照 `&mut T` を渡します。[参照](./values.md#references)を参照してください。
- 引数の数が合わない呼び出しは [`E2024`](../diagnostics/types.md#e2024) です。

## 戻り値型と本体 {#return-type-and-body}

`-> R` は結果の型を宣言します。省略すると結果は `()` です。本体はブロックで、最後の式が結果になります。`return e;` で早く戻れます。

```kis
enum Mood { Warm, Polite }

const fn mood(affection: Int<0, 5>) -> Mood {
    if affection >= 4 {
        return Mood::Warm;
    }
    Mood::Polite
}
```

本体を通るすべての経路は戻り値型の値を生み出さなければなりません。値なしで終わりに達しうる経路は型エラーです。`break` のない `loop` は値を生み出さず、値を必要としません。

`?` 演算子は `None` や `Err` で関数を早く終わらせます。`Option<_>` を返す関数では、`x?` は `Some` を取り出すか `None` を返します。`Result<_, E>` を返す関数では、`Ok` を取り出すか `Err` を返します（[`E2034`](../diagnostics/types.md#e2034)）。

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

関数は[境界](./types.md#trait-bounds)付きの型パラメータと定数パラメータを持てます。引数は呼び出しから推論するか、ターボフィッシュ `f::<…>` で与えます。ジェネリック関数は単相化され、使われる引数の組み合わせごとに複製が作られます。`pushdown fn` の中では、型引数はすべて[有限型](./types.md#finite-types)でなければなりません。

## メソッドと関連関数 {#methods-and-associated-functions}

[`impl` ブロック](./items.md#implementations)の中の関数は、対象の型の**関連関数**です。最初のパラメータが `self` であるものは**メソッド**です。

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
| `mut self` | `mut self: Self` | 変更してよいコピー |
| `&self` | `self: &Self` | 値への参照 |
| `&mut self` | `self: &mut Self` | 可変参照。変更は呼び出し側に届きます |

- レシーバーの型の明示（`self: Box<Self>`）はできません。
- `value.method(args)` でメソッドを呼びます。レシーバーは自動で借用・参照外しされるので、`gauge.fill(7)` は `&mut gauge` を渡します。`&mut self` のメソッドには可変な場所が必要です（[`E3017`](../diagnostics/kinds.md#e3017)）。
- `Type::function(args)` で関連関数を呼びます。メソッドもレシーバーを最初の引数にして同じように呼べます（`Gauge::is_full(&gauge)`）。標準トレイトのメソッドはトレイトを通しても呼べます（`Integer::into(value)`）。
- `impl` の中では、`Self` は対象の型を、`self` はレシーバーを指します。
- 関連関数と関連定数は `pub` が付いていなければプライベートです。
- 未知のメソッドは [`E2022`](../diagnostics/types.md#e2022) です。

## 引数としてのクロージャ {#closures-as-arguments}

[クロージャ型](./values.md#closure-and-pointer-types)のパラメータは、同じ種別のクロージャ、名前付き関数、関数ポインタを受け付けます。システムがハンドラを渡すのも、物語が小さなコードを下へ渡すのもこの方法です。

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

`nav::call` の `overlay: fn() -> Overlay` のような関数ポインタ型のパラメータは、名前付き関数か、何もキャプチャしないクロージャを受け付けます。物語の中ではクロージャは第二級で、渡して呼ぶことはできても、格納したり返したりはできません（[クロージャ](./values.md#closures)を参照）。

## 関数ポインタ {#function-pointers}

関数の名前はその関数型の値で、同じ種別の[関数ポインタ](./values.md#closure-and-pointer-types)に変換されます。

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

タプル構造体とタプルバリアントの名前は `const fn` のコンストラクタです。`Some` は `const fn(T) -> Option<T>` 型の値です。関数ポインタは同一性で比較されます。関数でないものを呼ぶと [`E2025`](../diagnostics/types.md#e2025) です。

## 再帰と停止性 {#recursion-and-termination}

関数が自分自身を呼べるか、どんなループを含められるかは種別によって決まります。

| 種別 | 再帰 | ループ | 何が上限を与えるか |
| ---- | ---- | ------ | ------------------ |
| `fn` | 可 | すべて | ランタイムの呼び出し深さの上限。超えると VM パニックです |
| `const fn` | 可 | すべて | コンパイル時の評価は 1,000,000 ステップで止まります（[`E4003`](../diagnostics/eval.md#e4003)） |
| シーン（`#[scene] pushdown fn`） | ステップを通るなら可 | すべて（ステップを通るなら） | すべてのループと呼び出しの循環はステップを通らなければなりません（[`E5001`](../diagnostics/analysis.md#e5001)、[`E5002`](../diagnostics/analysis.md#e5002)） |
| 即時手続き（`pushdown fn`） | 不可（[`E3005`](../diagnostics/kinds.md#e3005)） | 配列または定数の範囲に対する `for` のみ（[`E3004`](../diagnostics/kinds.md#e3004)、[`E3006`](../diagnostics/kinds.md#e3006)） | 常に終わります |

そのため、プレイヤーの 2 つの入力の間に実行される物語のコードは必ず終わります。シーンの規則は[停止性](../concepts/scenes.md#termination)で説明します。

## 入れ子の関数 {#nested-functions}

関数は他のアイテムと同じくブロックの中で宣言できます（[ブロック内のアイテム](./items.md#items-in-blocks)を参照）。入れ子の関数は普通のアイテムで、外側の関数のローカル変数をキャプチャしません。キャプチャするにはクロージャを使います。

## `main` 関数 {#the-main-function}

ルートモジュールは `fn main() -> System` を宣言します。これはゲームシステム（タイトル、解像度、モード、設定、フォント）を記述する普通の `fn` で、エンジンが起動時に 1 回呼びます。`main` のないルートモジュールは [`E7001`](../diagnostics/emit.md#e7001) です。[ゲームシステム](../concepts/system.md)を参照してください。
