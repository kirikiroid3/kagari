---
title: "種別 (E3xxx)"
---

この章の検査は、関数とアイテムの*種別*に関するものです。素の `fn` コード、`const` コード、`pushdown fn` の物語コード、`#[scene]` のシーンには、それぞれ何を呼び出せるか、何を読めるか、何を保持できるかの規則があります。物語変数、第二級の参照とクロージャ、`choice!` 構文やテキスト構文の形もここで検査します。

## E3001 `call-rule` {#e3001}

**エラー。** 呼び出してはいけない種別の関数を呼んでいます。`fn` から物語に入る呼び出し、`pushdown fn` からの `fn` の呼び出し、`const fn` からの `fn` の呼び出しなどが該当します。

```kis
fn start() -> Int<0, 3> { 0 }

pushdown fn visit() {
    start();
}
```

直し方：呼び出しを、その呼び出しが許される種別の関数へ移します。`fn` コードから物語に入る方法は、開始シーンの `nav::start` しかありません。

## E3002 `step-outside-scene` {#e3002}

**エラー。** シーン以外の関数で `s!` や `choice!` によるステップを実行しています。クロージャや即時手続きの中でも同じです。

```kix
pushdown fn visit() {
    s!(t!["no"]);
}
```

直し方：関数に `#[scene]` を付けるか、ステップをシーンの中へ移します。

## E3003 `scene-call-outside-scene` {#e3003}

**エラー。** 即時手続き（`#[scene]` の付いていない `pushdown fn`）からシーンを呼んでいます。

```kis
#[scene]
pushdown fn Other() {}

pushdown fn visit() {
    Other();
}
```

直し方：シーンは別の `#[scene]` 関数から呼ぶか、呼び出し側に `#[scene]` を付けます。

## E3004 `unbounded-loop` {#e3004}

**エラー。** 即時手続きで `while` など回数に上限のないループを使っています。即時手続きで使えるループは、配列か、境界が定数の範囲に対する `for` だけです。

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    while ROUTE < 3 { ROUTE += 1; }
}
```

直し方：`for i in [1, 2] { ROUTE = i; }` のように回数の決まったループにするか、関数に `#[scene]` を付けます。

## E3005 `instant-recursion` {#e3005}

**エラー。** 即時手続きが、直接または関数ポインタ経由で互いを呼び合い、循環しています。

```kis
pushdown fn ping() { pong(); }
pushdown fn pong() { ping(); }
```

直し方：どれか一つの関数に `#[scene]` を付け、途中でステップを挟むようにします。

## E3006 `for-not-constant` {#e3006}

**エラー。** 物語コードの `for` ループで、範囲の境界が定数式になっていません。

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    let n = ROUTE;
    for i in 0..n { }
}
```

直し方：配列を回すか、`for i in 0..MAX { ... }` のように境界が定数の範囲を回します。

## E3007 `story-variable-outside-story` {#e3007}

**エラー。** `pushdown fn` のコード以外で物語変数（`static mut`）を読み書きしています。

```kis
static mut ROUTE: Int<0, 3> = 0;

fn system() {
    let route = ROUTE;
}
```

直し方：物語変数には `pushdown fn` の中からだけアクセスします。

## E3008 `not-finite` {#e3008}

**エラー。** 物語変数、`pushdown fn` のローカル変数、開始シーンの引数、表化される `const fn` 呼び出しの結果のいずれかで、型の取りうる値が多すぎます。これらの型は、有界整数、`bool`、列挙型などの有限型でなければなりません。型の制約がない整数リテラルは `i32` になるため、`pushdown fn` の中で `let mut i = 0;` と書いてもこのエラーになります。

```kis
static mut COUNT: u32 = 0;
```

直し方：`static mut COUNT: Int<0, 10> = 0;` や `let mut i: Int<0, 10> = 0;` のように有界の型を使います。

## E3009 `text-in-entry` {#e3009}

**エラー。** 開始シーンの引数の型か戻り値の型に `Text` が含まれています。`fn` コードは実行時にテキストを組み立てられるので、`Text` を物語に渡すことはできません。

```kis
enum Ending { Normal }

#[scene]
#[entry]
pushdown fn Named(name: Text) -> Ending {
    Ending::Normal
}
```

直し方：列挙型などの有限の値を渡し、表示するテキストは物語の中で選びます。

## E3010 `not-constant` {#e3010}

**エラー。** `const`（または `static mut`）の初期化式が定数式になっていません。代入や `fn` 関数の呼び出しは定数式に含まれません。

```kis
const COUNTED: u32 = { let mut n = 0; n += 1; n };
```

直し方：初期化式を `const COUNTED: u32 = 1;` のような定数式で書きます。

## E3011 `bad-handle` {#e3011}

**エラー。** 不変の `static` に、`Setting<T>` や `Meta<T>` のハンドル以外の値を入れています。

```kis
static LIMIT_HANDLE: u32 = 3;
```

直し方：定数なら `const`（`const LIMIT: u32 = 3;`）、物語変数なら `static mut` を使います。

## E3012 `reference-in-item` {#e3012}

**エラー。** `const` や `static` の型に参照が含まれています。

```kis
const BORROWED: &u32 = &1;
```

直し方：`const BORROWED: u32 = 1;` のように値そのものを保持します。

## E3013 `meta-in-const` {#e3013}

**エラー。** `const` コードで `Meta` フィールドを使っています。メタデータは `fn` コードが読み、物語が書き込むもので、`const` コードからは見えません。

```kis
static CLEARED: Meta<bool> = meta(false);

const fn render(mut screen: Screen) -> Screen {
    let cleared = CLEARED;
    screen
}
```

直し方：メタデータの値は `fn` コードで読むか、引数として `const fn` に渡します。

## E3014 `second-class-reference` {#e3014}

**エラー。** 物語コードで参照をローカル変数に入れるか、戻り値として返しています。物語の中では参照は第二級で、引数として渡すことしかできません。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn alias(p: &mut Point) {
    let q = &p.x;
}
```

直し方：参照を直接引数として渡すか、`let q = p.x;` で値をコピーします。

## E3015 `second-class-closure` {#e3015}

**エラー。** 物語コードでクロージャをローカル変数に入れています。物語の中ではクロージャは第二級で、引数として渡すことと呼び出すことしかできません。

```kis
pushdown fn twice(f: pushdown Fn()) {
    f();
    let g = f;
}
```

直し方：`f` をローカル変数に束縛せず、そのまま呼び出します。

## E3016 `aliased-mut-argument` {#e3016}

**エラー。** 物語コードの同じ呼び出しで、ある場所を `&mut` 引数として渡し、さらに別の引数としても渡しています。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn shift(a: &mut Point, b: &Point) {
    a.x = b.y;
}

pushdown fn alias(p: &mut Point) {
    shift(p, p);
}
```

直し方：`shift(&mut a, &b);` のように別々の場所を渡します。

## E3017 `immutable-place` {#e3017}

**エラー。** 根元の束縛が可変として宣言されていない場所に代入しています。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

#[scene]
pushdown fn Moves() {
    let b = Point { x: 1, y: 2 };
    b.x = 3;
}
```

直し方：`let mut b = Point { x: 1, y: 2 };` のように束縛を可変にします。

## E3018 `misplaced-attribute` {#e3018}

**エラー。** 付けられないアイテムに属性を付けています。`fn` に付けた `#[scene]`、`#[scene]` のない `#[entry]`、関数に付けた `#[must_use]`、構造体以外に付けた `#[shader_arg]` などが該当します。

```kis
#[scene] fn not_a_story() {}
```

直し方：`#[scene] pushdown fn not_a_story() {}` のように、属性は付けられる場所にだけ付けます。

## E3019 `choice-id` {#e3019}

**エラー。** 同じ関数の中にある二つの `choice!` 構文が同じ `id` を使っています。セーブには選択肢の id が記録されるため、id は関数の中で重複してはいけません。

```kix
enum Map { A, B }

#[scene]
pushdown fn Main() {
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
}
```

直し方：`id = "map2"` のように、選択肢ごとに別の id を付けます。

## E3020 `choice-arms` {#e3020}

**エラー。** `choice!` で、同じバリアントを複数のアームに書いています。一つのバリアントを選べるのは一度だけです。

```kix
enum Map { A, B }

#[scene]
pushdown fn Main() {
    match choice!{ id = "map", t!["A"] => Map::A, t!["A again"] => Map::A } {
        Map::A => {}
        _ => {}
    }
}
```

直し方：アームごとに異なるバリアントを対応させます。

## E3021 `ruby-readings` {#e3021}

**エラー。** モノルビの読みが、親文字 1 文字につき一つになっていません。

```kix
#[scene]
pushdown fn Main() {
    s!(t![["漢字"]("か"|"ん"|"じ")]);
}
```

直し方：`s!(t![["漢字"]("かん"|"じ")]);` のように、1 文字につき一つの読みを付けます。

## E3022 `effect-not-constant` {#e3022}

**エラー。** テキストの `effect(base, e)` で、効果の引数が定数式になっていません。

```kis
use std::narrative::text::{effect, mark};

fn tagged(n: u8) -> Text {
    effect("a", mark(n))
}
```

直し方：`effect("a", mark(1))` のように定数の効果を渡します。

## W3023 `unused-must-use` {#w3023}

**警告。** `Swap` などの `#[must_use]` 型の値を作ったまま捨てています。この値が表す変更は、使うまで反映されません。

```kis
const fn render(mut screen: Screen) -> Screen {
    screen.background().clear();
    screen
}
```

直し方：`screen.background().clear().apply_animation(cut());` のように変更を適用します。

## E3024 `rng-outside-scene` {#e3024}

**エラー。** シーン以外の関数で `rng_choose` を呼んでいます。乱数を引けるのはシーンだけです。

```kis
enum Coin { Heads, Tails }

pushdown fn flip() -> Coin {
    rng_choose::<Coin>()
}
```

直し方：関数に `#[scene]` を付けるか、シーンで値を引いてから引数で渡します。

## E3025 `shader-arg-shape` {#e3025}

**エラー。** `#[shader_arg]` の構造体が、名前付きフィールドを一つ以上含むレコード構造体になっていないか、型パラメータを含んでいます。シェーダーは引数を、名前と型の決まったメンバーからなる WGSL の構造体として読むためです。配列の長さなどの const パラメータは使えます。

```kis
#[shader_arg]
struct Pair(Decimal, Decimal);
```

直し方：フィールドに名前を付け、型パラメータを外します（例：`#[shader_arg] struct Pair { a: Decimal, b: Decimal }`）。

## E3026 `shader-arg-field` {#e3026}

**エラー。** `#[shader_arg]` の構造体に、シェーダーが読めない型のフィールドか、長さ 0 の配列のフィールドがあります。フィールドに使える型は `bool`、`i32`、`u32`、`Decimal`、`Rat`、`Color`、`Point`、`Offset`、`Size`、`Anchor`、`Angle`、`Duration`、フィールドのない列挙型、ほかの `#[shader_arg]` 構造体、およびこれらの空でない配列です。

```kis
#[shader_arg]
struct Caption {
    text: String,
}
```

直し方：シェーダーには数値のデータだけを渡します（例：`#[shader_arg] struct Caption { strength: Decimal }`）。
