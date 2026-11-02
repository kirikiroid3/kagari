---
title: "型 (E2xxx)"
---

型の診断は、アイテムのシグネチャ（型、定数、エイリアス、境界）の検査と、関数やシーンの本体の型推論で出ます。型の不一致、有界整数の算術、変換、フィールドとメソッド、呼び出し、パターン、`match` の網羅性を扱います。`W2039` 以外はすべてエラーです。

## E2001 `mismatched-types` {#e2001}

**エラー。** 値の型が、`let` の型注釈、戻り値の型、関数の引数など、その場所で期待される型と違います。

```kis
fn mismatch() -> bool {
    let n: u32 = true;
    5
}
```

直し方：値を期待される型に合わせる（例：`let n: u32 = 1;`）か、型注釈を変えます。

## E2002 `unsupported-const` {#e2002}

**エラー。** 型の中の定数式（配列の長さ、`Int` の境界、const ジェネリクス引数）で、リテラル、算術、定数、const パラメータ以外のものを使っているか、評価が循環しています。

```kis
const A: i64 = B;
const B: i64 = A;
fn f(n: Int<0, A>) {}
```

直し方：定数はリテラル、算術、循環しないほかの定数だけで組み立てます（例：`const A: i64 = 4;`）。

## E2003 `const-overflow` {#e2003}

**エラー。** 型の中の定数式を評価した結果、オーバーフローかゼロ除算が起きたか、式の中の整数リテラルが大きすぎます。

```kis
fn f(a: [u32; 10 / 0]) {}
```

直し方：算術の結果が範囲に収まるように式を変えます（例：`fn f(a: [u32; 10 / 2]) {}`）。

## E2004 `bad-int-bounds` {#e2004}

**エラー。** `lo > hi` のため `Int<lo, hi>` 型が空になっているか、境界が `i64` に収まりません。

```kis
fn f(n: Int<5, 0>) {}
```

直し方：境界を小さい順に `fn f(n: Int<0, 5>) {}` と書きます。

## E2005 `wrong-generic-count` {#e2005}

**エラー。** 型や関数に渡したジェネリクス引数の個数が、宣言と合いません。`Int` には境界がちょうど二つ必要で、プリミティブ型は引数を取りません。

```kis
fn f(n: Int<5>) {}
```

直し方：`fn f(n: Int<0, 5>) {}` のように、宣言どおりの個数の引数を渡します。

## E2006 `expected-type` {#e2006}

**エラー。** 型を書くべき位置のパスが、関数、列挙型のバリアント、モジュール、定数など、型ではないものを指しています。

```kis
enum Route { A, B }
fn f(r: Route::A) {}
```

直し方：`fn f(r: Route) {}` のように型を書きます。

## E2007 `expected-const` {#e2007}

**エラー。** const ジェネリクスパラメータに、定数ではなく型を渡しています。

```kis
fn f(n: Int<u32, 5>) {}
```

直し方：`fn f(n: Int<0, 5>) {}` のように定数の値を渡します。

## E2008 `expected-trait` {#e2008}

**エラー。** 境界や `impl … for` で、トレイトではないものを指定しています。

```kis
struct Point { x: u32, y: u32 }
fn same<T: Point>(a: T) {}
```

直し方：境界にはトレイトを書きます（例：`fn same<T: Eq>(a: T) {}`）。

## E2009 `expected-value` {#e2009}

**エラー。** 式の位置のパスが、モジュール、型パラメータ、名前付きフィールドのある構造体やバリアントなど、値ではないものを指しています。

```kis
enum Shape { Circle { r: u32 }, Dot }
fn f() {
    let s = Shape::Circle;
}
```

直し方：`let s = Shape::Circle { r: 1 };` のように値を組み立てます。

## E2010 `alias-cycle` {#e2010}

**エラー。** 型エイリアスが、直接またはほかのエイリアスを通じて自分自身に展開されます。

```kis
type A = B;
type B = A;
```

直し方：どれか一つのエイリアスを具体的な型にします（例：`type B = u32;`）。

## E2011 `infer-in-signature` {#e2011}

**エラー。** アイテムのシグネチャにプレースホルダー `_` があります。シグネチャでは型を省略できません。

```kis
fn f(n: _) -> u32 { 1 }
```

直し方：`fn f(n: u32) -> u32 { 1 }` のように型を書きます。

## E2012 `bad-literal` {#e2012}

**エラー。** 整数リテラルに、整数型ではないサフィックスが付いています。

```kis
fn f() {
    let n = 5u7;
}
```

これは説明のための例です。未知のサフィックスは先にパーサーが `E0001` で弾くため、型推論が `E2012` を報告するのは、検査をすり抜けて届いたリテラルだけです。

直し方：正しいサフィックスを付けるか、サフィックスを外します（例：`let n = 5u8;`）。

## E2013 `literal-out-of-range` {#e2013}

**エラー。** 式やパターンの中の整数リテラルが、照合する整数型の範囲に収まりません。

```kis
fn literals() {
    let small: u8 = 300;
    let counter: Int<0, 5> = 6;
}
```

直し方：型の範囲に収まる値を使うか、範囲の広い型にします（例：`let small: u16 = 300;`）。

## E2014 `int-mix` {#e2014}

**エラー。** 算術演算子の両辺で整数型が異なります。暗黙の型変換はありません。

```kis
fn int_ranges(a: Int<0, 5>) {
    let mixed = a + 1u32;
}
```

直し方：どちらかを `.into()` か `.saturate()` で明示的に変換します（例：`let mixed = a.into() + 1u32;`）。

## E2015 `bad-operands` {#e2015}

**エラー。** 演算子が対応していない型に使われています。`bool`・整数以外への `!`、符号なしの値の符号反転、結果の範囲がオーバーフローする有界整数の算術などが該当します。

```kis
struct Point { x: u32, y: u32 }
fn f(p: Point) -> bool {
    !p
}
```

直し方：演算子は対応する型に使います（例：`bool` に対する `!flag`）。

## E2016 `divisor-may-be-zero` {#e2016}

**エラー。** `/` や `%` の除数が、範囲に 0 を含む有界の `Int` 型です。

```kis
fn int_ranges(a: Int<0, 5>) {
    let quotient = a / a;
}
```

直し方：先に除数の型を 0 を含まない範囲に絞ります。たとえば `b: Int<1, 5>` を受け取って `a / b` と書きます。

## E2017 `not-eq` {#e2017}

**エラー。** クロージャなど、`Eq` になりえない型に `==` や `!=` を使っています。

```kis
fn equality() -> bool {
    let f = || 1u32;
    f == f
}
```

直し方：`Eq` を実装した値どうしを比べます（例：`f() == f()`）。

## E2018 `lossy-into` {#e2018}

**エラー。** `.into()` で、変換元の値が変換先に収まるとは限らない整数型どうしを変換しています。

```kis
fn conversions(wide: u32) {
    let byte: u8 = wide.into();
}
```

直し方：`let byte: u8 = wide.saturate();` のように明示的に飽和させます。

## E2019 `as-cast` {#e2019}

**エラー。** Kagari には `as` によるキャストはありません。

```kis
fn conversions(wide: u32) {
    let cast = wide as u8;
}
```

直し方：値を失わない変換には `.into()`、飽和させる変換には `.saturate()` を使います（例：`let cast: u8 = wide.saturate();`）。

## E2020 `unknown-field` {#e2020}

**エラー。** 構造体の初期化式やフィールドアクセスで、型にないフィールドを指定しています。

```kis
struct Point { x: u32, y: u32 }
fn fields(p: Point) -> u32 {
    let unknown = Point { x: 1, y: 2, z: 3 };
    p.z
}
```

直し方：宣言されたフィールドだけを使います（例：`p.x`）。

## E2021 `missing-field` {#e2021}

**エラー。** 構造体の初期化式で、一部のフィールドが抜けています。

```kis
struct Point { x: u32, y: u32 }
fn fields() {
    let missing = Point { x: 1 };
}
```

直し方：`let p = Point { x: 1, y: 2 };` のように、すべてのフィールドを初期化します。

## E2022 `unknown-method` {#e2022}

**エラー。** 呼び出したメソッドが、型にもスコープ内のどのトレイトにもありません。

```kis
struct Point { x: u32, y: u32 }
fn methods(p: Point) {
    p.frobnicate();
}
```

直し方：存在するメソッドを呼ぶか、`impl Point` ブロックに `frobnicate` を定義します。

## E2023 `not-a-method` {#e2023}

**エラー。** `self` パラメータのない関連関数を、メソッド呼び出しの形で呼んでいます。

```kis
struct Point { x: u32, y: u32 }
impl Point {
    fn origin() -> Point { Point { x: 0, y: 0 } }
}
fn f(p: Point) {
    p.origin();
}
```

直し方：`Point::origin()` のように型を通して呼び出します。

## E2024 `wrong-arity` {#e2024}

**エラー。** 呼び出しの引数の個数が、関数の受け取る個数と合いません。

```kis
fn arity(x: u32) -> u32 {
    arity(x, x)
}
```

直し方：`arity(x)` のように、宣言どおりの引数を渡します。

## E2025 `not-callable` {#e2025}

**エラー。** 呼び出し式の呼び出し先が、関数、関数ポインタ、クロージャのどれでもありません。

```kis
fn f(n: u32) -> u32 {
    n(1)
}
```

直し方：関数値を呼び出すか、`n` を値として使いたいのであれば括弧を外します。

## E2026 `unsatisfied-bound` {#e2026}

**エラー。** ジェネリクス引数の型が、呼び出し先の境界で求められているトレイトを実装していません。

```kis
fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn bounds() -> bool {
    same(|| 1u32, || 2u32)
}
```

直し方：そのトレイトを実装した型を渡します（例：`same(1u32, 2u32)`）。

## E2027 `bad-nav-start` {#e2027}

**エラー。** `nav::start` の第 1 引数が `#[entry]` シーンではありません。

```kis
#[scene]
pushdown fn Side() {}

fn navigation() {
    nav::start(Side, (), |_end| ());
}
```

直し方：`#[entry]` シーンを開始します。この例なら `Side` に `#[entry]` を付けます。

## E2028 `asset-path-not-literal` {#e2028}

**エラー。** `asset(…)` や `system_font(…)` に渡したパスが文字列リテラルではないため、コンパイル時に検査できません。

```kis
const PATH: Text = "missing.png";
const IMAGE: ImageAsset = asset(PATH);
```

直し方：`const IMAGE: ImageAsset = asset("missing.png");` のように、パスをその場に直接書きます。

## E2029 `annotations-needed` {#e2029}

**エラー。** 型推論で型を決められませんでした。後から型が決まらない空のコレクションや、型の分からない値に対する呼び出し・フィールドアクセス・反復などで起こります。

```kis
fn annotations() {
    let list = Vec::new();
}
```

直し方：`let list: Vec<u32> = Vec::new();` のように型注釈を付けます。

## E2030 `open-range` {#e2030}

**エラー。** 範囲式の境界が欠けています。値として使う範囲には両端（`a..b` か `a..=b`）が必要で、`a..`、`..b`、`..=b`、`..` はどれもエラーになります。開いた範囲を書けるのは[範囲パターン](../reference/syntax/patterns.md#range-patterns)だけで、範囲パターンには `a..`、`..=b`、`a..b`、`a..=b` があります。`..b` というパターンはなく、パターン中の単独の `..` は範囲ではなく残余パターンです。コンパイラのメッセージには「`a..`, `..b` and `..` exist only in patterns」とありますが、実際には上に挙げたとおりです。

```kis
fn f() {
    for i in 0.. {}
}
```

直し方：`for i in 0..10 {}` のように両方の境界を書きます。

## E2031 `not-iterable` {#e2031}

**エラー。** `for` ループで、反復できない型の値を回そうとしています。

```kis
fn f(flag: bool) {
    for x in flag {}
}
```

直し方：範囲やコレクションを回します（例：`for x in 0..5 {}`）。

## E2032 `not-indexable` {#e2032}

**エラー。** インデックス `[…]` に対応していない型の値にインデックスを使っています。

```kis
fn f(n: u32) -> u32 {
    n[0]
}
```

直し方：配列、`Vec` など、インデックスで参照できるコレクションを使います。

## E2033 `bad-deref` {#e2033}

**エラー。** 参照ではない値に参照外し演算子 `*` を使っています。

```kis
fn f(n: u32) -> u32 {
    *n
}
```

直し方：`*` を外すか、`r: &u32` のような本当の参照に対して参照外しをします。

## E2034 `bad-try` {#e2034}

**エラー。** `Option`/`Result` 以外の値に `?` 演算子を使っているか、関数の戻り値の型がオペランドの種類と合っていません。

```kis
fn f(o: Option<u32>) -> u32 {
    o?
}
```

直し方：対応する型を返します（例：`fn f(o: Option<u32>) -> Option<u32> { Some(o?) }`）。

## E2035 `bad-pattern` {#e2035}

**エラー。** パターンの形が、指している対象と合いません。タプルではないバリアントへのタプルパターン、フィールド数の誤り、バリアントを書くべき場所への列挙型、パターン中の static などが該当します。

```kis
fn f(r: Option<u32>) -> u32 {
    match r {
        Some(a, b) => a,
        None => 0,
    }
}
```

直し方：`Some(a) => a,` のように宣言どおりの形に合わせます。

## E2036 `capturing-closure` {#e2036}

**エラー。** ローカル変数をキャプチャするクロージャを、関数ポインタが必要な場所で使っています。

```kis
fn f(n: u32) {
    let g: fn() -> u32 = || n;
}
```

直し方：値はキャプチャせず、引数として渡します（例：`let g: fn(u32) -> u32 = |n| n;`）。

## E2037 `non-exhaustive-match` {#e2037}

**エラー。** `match` が、検査対象のとりうる値をすべて網羅していません。網羅できていないパターンはメッセージに表示されます。

```kis
fn bounded(n: Int<0, 10>) -> u32 {
    match n {
        0..=5 => 0,
    }
}
```

直し方：足りない値のアームかワイルドカードを追加します（例：`6..=10 => 1,`）。

## E2038 `refutable-pattern` {#e2038}

**エラー。** 照合に失敗しうるパターンを、必ず照合に成功しなければならない場所（`let`、関数やクロージャの引数、`for` の束縛）で使っています。

```kis
enum Route { A, B, C }
fn refutable(r: Option<Route>) {
    let Some(x) = r;
}
```

直し方：`match` か `if let Some(x) = r { … }` を使い、ほかの場合も扱います。

## W2039 `unreachable-pattern` {#w2039}

**警告。** この `match` アームが照合する値は、前のアームですべて網羅されているため、このアームには到達しません。

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

直し方：到達しないアームを削除するか、そのアームを網羅しているアームより前に移します。
