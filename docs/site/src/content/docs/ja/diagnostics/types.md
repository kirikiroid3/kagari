---
title: "型 (E2xxx)"
---

型の診断は、アイテムのシグネチャ（型、定数、エイリアス、境界）の検査と、関数やシーンの本体の型推論から報告されます。型の不一致、有界整数の算術、変換、フィールドとメソッド、呼び出し、パターン、`match` の網羅性が対象です。`W2039` 以外はすべてエラーです。

## E2001 `mismatched-types` {#e2001}

**エラー。** 値の型が、文脈が期待する型（`let` の型注釈、戻り値の型、関数の引数など）と異なります。

```kis
fn mismatch() -> bool {
    let n: u32 = true;
    5
}
```

修正方法: 値を期待される型に合わせる（例: `let n: u32 = 1;`）か、型注釈を変えます。

## E2002 `unsupported-const` {#e2002}

**エラー。** 型の中の定数式（配列の長さ、`Int` の境界、const ジェネリクス引数）が、リテラル・算術・定数・const パラメーター以外のものを使っているか、評価が循環しています。

```kis
const A: i64 = B;
const B: i64 = A;
fn f(n: Int<0, A>) {}
```

修正方法: 定数をリテラル、算術、循環のない他の定数だけで組み立てます。例: `const A: i64 = 4;`。

## E2003 `const-overflow` {#e2003}

**エラー。** 型の中の定数式の評価がオーバーフローしたかゼロ除算になったか、その中の整数リテラルが大きすぎます。

```kis
fn f(a: [u32; 10 / 0]) {}
```

修正方法: 算術が範囲内に収まるように式を変えます。例: `fn f(a: [u32; 10 / 2]) {}`。

## E2004 `bad-int-bounds` {#e2004}

**エラー。** `lo > hi` のため `Int<lo, hi>` 型が空であるか、境界が `i64` に収まりません。

```kis
fn f(n: Int<5, 0>) {}
```

修正方法: 境界を昇順に書きます: `fn f(n: Int<0, 5>) {}`。

## E2005 `wrong-generic-count` {#e2005}

**エラー。** 型または関数が、宣言と異なる個数のジェネリクス引数を受け取っています。`Int` はちょうど 2 つの境界を必要とし、プリミティブ型は引数を取りません。

```kis
fn f(n: Int<5>) {}
```

修正方法: 宣言どおりの個数の引数を与えます: `fn f(n: Int<0, 5>) {}`。

## E2006 `expected-type` {#e2006}

**エラー。** 型の位置にあるパスが、型ではないもの（関数、列挙型のバリアント、モジュール、定数など）に解決されました。

```kis
enum Route { A, B }
fn f(r: Route::A) {}
```

修正方法: 代わりに型を指定します: `fn f(r: Route) {}`。

## E2007 `expected-const` {#e2007}

**エラー。** const ジェネリクスパラメーターに定数ではなく型が与えられています。

```kis
fn f(n: Int<u32, 5>) {}
```

修正方法: 定数値を渡します: `fn f(n: Int<0, 5>) {}`。

## E2008 `expected-trait` {#e2008}

**エラー。** 境界や `impl … for` が、トレイトではないものを指しています。

```kis
struct Point { x: u32, y: u32 }
fn same<T: Point>(a: T) {}
```

修正方法: 境界にはトレイトを使います。例: `fn same<T: Eq>(a: T) {}`。

## E2009 `expected-value` {#e2009}

**エラー。** 式の位置にあるパスが、値ではないもの（モジュール、型パラメーター、名前付きフィールドを持つ構造体やバリアントなど）に解決されました。

```kis
enum Shape { Circle { r: u32 }, Dot }
fn f() {
    let s = Shape::Circle;
}
```

修正方法: 値を構築します: `let s = Shape::Circle { r: 1 };`。

## E2010 `alias-cycle` {#e2010}

**エラー。** 型エイリアスが、直接または他のエイリアスを介して自分自身に展開されます。

```kis
type A = B;
type B = A;
```

修正方法: どれか 1 つのエイリアスを具体的な型にします。例: `type B = u32;`。

## E2011 `infer-in-signature` {#e2011}

**エラー。** アイテムのシグネチャにプレースホルダー `_` が現れています。シグネチャでは型を明記する必要があります。

```kis
fn f(n: _) -> u32 { 1 }
```

修正方法: 型を書きます: `fn f(n: u32) -> u32 { 1 }`。

## E2012 `bad-literal` {#e2012}

**エラー。** 整数リテラルに、整数型ではないサフィックスが付いています。

```kis
fn f() {
    let n = 5u7;
}
```

この例は説明用です。未知のサフィックスはパーサーが先に `E0001` で拒否するため、`E2012` が報告されるのは検査を経ずに型推論まで届いたリテラルだけです。

修正方法: 有効なサフィックスを使うか、付けません: `let n = 5u8;`。

## E2013 `literal-out-of-range` {#e2013}

**エラー。** 式またはパターンの中の整数リテラルが、照合される整数型に収まりません。

```kis
fn literals() {
    let small: u8 = 300;
    let counter: Int<0, 5> = 6;
}
```

修正方法: 型の範囲内の値を使うか、型を広げます。例: `let small: u16 = 300;`。

## E2014 `int-mix` {#e2014}

**エラー。** 算術演算子が 2 つの異なる整数型を組み合わせています。暗黙の変換はありません。

```kis
fn int_ranges(a: Int<0, 5>) {
    let mixed = a + 1u32;
}
```

修正方法: 片方を `.into()` か `.saturate()` で明示的に変換します。例: `let mixed = a.into() + 1u32;`。

## E2015 `bad-operands` {#e2015}

**エラー。** 演算子が、対応していない型のオペランドに適用されています。たとえば `bool`・整数以外への `!`、符号なし値の符号反転、結果の範囲がオーバーフローする有界算術です。

```kis
struct Point { x: u32, y: u32 }
fn f(p: Point) -> bool {
    !p
}
```

修正方法: 対応する型に演算子を適用します。例: `bool` に対する `!flag`。

## E2016 `divisor-may-be-zero` {#e2016}

**エラー。** `/` または `%` の除数が、範囲に 0 を含む有界 `Int` 型です。

```kis
fn int_ranges(a: Int<0, 5>) {
    let quotient = a / a;
}
```

修正方法: 先に除数の型を 0 を含まないように狭めます。例: `b: Int<1, 5>` を受け取り `a / b` と書きます。

## E2017 `not-eq` {#e2017}

**エラー。** `==` または `!=` が、決して `Eq` にならない型（クロージャなど）に使われています。

```kis
fn equality() -> bool {
    let f = || 1u32;
    f == f
}
```

修正方法: `Eq` を実装する値どうしを比較します。例: `f() == f()`。

## E2018 `lossy-into` {#e2018}

**エラー。** `.into()` が、変換元の値が変換先に常に収まるとは限らない整数型どうしを変換しています。

```kis
fn conversions(wide: u32) {
    let byte: u8 = wide.into();
}
```

修正方法: 明示的に飽和させます: `let byte: u8 = wide.saturate();`。

## E2019 `as-cast` {#e2019}

**エラー。** Kagari には `as` キャストはありません。

```kis
fn conversions(wide: u32) {
    let cast = wide as u8;
}
```

修正方法: 損失のない変換には `.into()`、飽和変換には `.saturate()` を使います: `let cast: u8 = wide.saturate();`。

## E2020 `unknown-field` {#e2020}

**エラー。** 構造体の初期化子またはフィールドアクセスが、型にないフィールドを指定しています。

```kis
struct Point { x: u32, y: u32 }
fn fields(p: Point) -> u32 {
    let unknown = Point { x: 1, y: 2, z: 3 };
    p.z
}
```

修正方法: 宣言されたフィールドだけを使います。例: `p.x`。

## E2021 `missing-field` {#e2021}

**エラー。** 構造体の初期化子で、構造体のフィールドが省略されています。

```kis
struct Point { x: u32, y: u32 }
fn fields() {
    let missing = Point { x: 1 };
}
```

修正方法: すべてのフィールドを初期化します: `let p = Point { x: 1, y: 2 };`。

## E2022 `unknown-method` {#e2022}

**エラー。** メソッド呼び出しが、型にもスコープ内のどのトレイトにもないメソッドを指定しています。

```kis
struct Point { x: u32, y: u32 }
fn methods(p: Point) {
    p.frobnicate();
}
```

修正方法: 既存のメソッドを呼ぶか、`impl Point` ブロックに `frobnicate` を定義します。

## E2023 `not-a-method` {#e2023}

**エラー。** `self` パラメーターを持たない関連関数に、メソッド呼び出し構文が使われています。

```kis
struct Point { x: u32, y: u32 }
impl Point {
    fn origin() -> Point { Point { x: 0, y: 0 } }
}
fn f(p: Point) {
    p.origin();
}
```

修正方法: 型を通して呼び出します: `Point::origin()`。

## E2024 `wrong-arity` {#e2024}

**エラー。** 呼び出しの引数の個数が、関数の受け取る個数と異なります。

```kis
fn arity(x: u32) -> u32 {
    arity(x, x)
}
```

修正方法: 宣言どおりの引数を渡します: `arity(x)`。

## E2025 `not-callable` {#e2025}

**エラー。** 呼び出し式の呼び出し先が、関数、関数ポインター、クロージャのいずれでもありません。

```kis
fn f(n: u32) -> u32 {
    n(1)
}
```

修正方法: 関数値を呼び出すか、`n` を値として使うつもりなら括弧を外します。

## E2026 `unsatisfied-bound` {#e2026}

**エラー。** ジェネリクス引数が、呼び出し先の境界が要求するトレイトを実装していません。

```kis
fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn bounds() -> bool {
    same(|| 1u32, || 2u32)
}
```

修正方法: そのトレイトを実装する型を渡します。例: `same(1u32, 2u32)`。

## E2027 `bad-nav-start` {#e2027}

**エラー。** `nav::start` の第 1 引数が `#[entry]` シーンを指していません。

```kis
#[scene]
pushdown fn Side() {}

fn navigation() {
    nav::start(Side, (), |_end| ());
}
```

修正方法: `#[entry]` シーンを開始します。例: `Side` に `#[entry]` を付けます。

## E2028 `asset-path-not-literal` {#e2028}

**エラー。** `asset(…)` または `system_font(…)` に渡したパスが文字列リテラルではないため、コンパイル時に検査できません。

```kis
const PATH: Text = "missing.png";
const IMAGE: ImageAsset = asset(PATH);
```

修正方法: パスを直接書きます: `const IMAGE: ImageAsset = asset("missing.png");`。

## E2029 `annotations-needed` {#e2029}

**エラー。** 推論で型を決定できませんでした。たとえば、後で制約されない空のコレクションや、型の分からない値への呼び出し・フィールドアクセス・反復です。

```kis
fn annotations() {
    let list = Vec::new();
}
```

修正方法: 型注釈を付けます: `let list: Vec<u32> = Vec::new();`。

## E2030 `open-range` {#e2030}

**エラー。** 範囲式に境界がありません。値としての範囲には両端（`a..b` または `a..=b`）が必要で、`a..`、`..b`、`..=b`、`..` はいずれも報告されます。開いた範囲は[範囲パターン](../reference/syntax/patterns.md#range-patterns)としてだけ存在し、範囲パターンは `a..`、`..=b`、`a..b`、`a..=b` です。`..b` というパターンはなく、パターン中の単独の `..` は範囲ではなく残余パターンです。コンパイラのメッセージは「`a..`, `..b` and `..` exist only in patterns」と述べますが、上の一覧のとおりに読んでください。

```kis
fn f() {
    for i in 0.. {}
}
```

修正方法: 両方の境界を与えます: `for i in 0..10 {}`。

## E2031 `not-iterable` {#e2031}

**エラー。** `for` ループが、反復できない型の値を反復しています。

```kis
fn f(flag: bool) {
    for x in flag {}
}
```

修正方法: 範囲やコレクションを反復します。例: `for x in 0..5 {}`。

## E2032 `not-indexable` {#e2032}

**エラー。** インデックス `[…]` が、それに対応しない型の値に適用されています。

```kis
fn f(n: u32) -> u32 {
    n[0]
}
```

修正方法: 代わりに配列、`Vec`、その他のインデックス可能なコレクションを使います。

## E2033 `bad-deref` {#e2033}

**エラー。** 参照外し演算子 `*` が、参照ではない値に適用されています。

```kis
fn f(n: u32) -> u32 {
    *n
}
```

修正方法: `*` を外すか、`r: &u32` のような実際の参照を参照外しします。

## E2034 `bad-try` {#e2034}

**エラー。** `?` 演算子が `Option`/`Result` ではない値に使われているか、関数の戻り値の型がオペランドの種類と一致しません。

```kis
fn f(o: Option<u32>) -> u32 {
    o?
}
```

修正方法: 一致する型を返します。例: `fn f(o: Option<u32>) -> Option<u32> { Some(o?) }`。

## E2035 `bad-pattern` {#e2035}

**エラー。** パターンの形が、指す対象と合いません。たとえばタプルでないバリアントへのタプルパターン、フィールド数の誤り、バリアントが期待される場所での列挙型、パターン中の static です。

```kis
fn f(r: Option<u32>) -> u32 {
    match r {
        Some(a, b) => a,
        None => 0,
    }
}
```

修正方法: 宣言された形に合わせます: `Some(a) => a,`。

## E2036 `capturing-closure` {#e2036}

**エラー。** ローカル変数をキャプチャするクロージャが、関数ポインターが必要な場所で使われています。

```kis
fn f(n: u32) {
    let g: fn() -> u32 = || n;
}
```

修正方法: 値をキャプチャせずパラメーターとして渡します。例: `let g: fn(u32) -> u32 = |n| n;`。

## E2037 `non-exhaustive-match` {#e2037}

**エラー。** `match` が検査対象のすべての値を網羅していません。メッセージに網羅されていないパターンが表示されます。

```kis
fn bounded(n: Int<0, 10>) -> u32 {
    match n {
        0..=5 => 0,
    }
}
```

修正方法: 不足している値のアームかワイルドカードを追加します: `6..=10 => 1,`。

## E2038 `refutable-pattern` {#e2038}

**エラー。** 照合に失敗しうるパターンが、必ず成功しなければならない場所（`let`、関数やクロージャのパラメーター、`for` の束縛）で使われています。

```kis
enum Route { A, B, C }
fn refutable(r: Option<Route>) {
    let Some(x) = r;
}
```

修正方法: `match` か `if let Some(x) = r { … }` を使って他のケースを扱います。

## W2039 `unreachable-pattern` {#w2039}

**警告。** それより前のアームがすでにすべての値を網羅しているため、この `match` アームには決して到達しません。

```kis
enum Route { A, B, C }
fn unreachable(r: Route) -> u32 {
    match r {
        Route::A | Route::B => 0,
        Route::B => 1,
        _ => 2,
    }
}
```

修正方法: 到達しないアームを削除するか、それを網羅するアームより前に移します。
