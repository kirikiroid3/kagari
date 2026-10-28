# 種別 (E3xxx) {#kinds-e3xxx}

これらの検査は、各関数とアイテムの*種別*を対象とします。素の `fn` コード、`const` コード、`pushdown fn` の物語コード、`#[scene]` のシーンには、それぞれ何を呼び出し、読み、保持できるかの規則があります。物語変数、第二級の参照とクロージャ、`choice!` とテキストフォームの形もここで検査されます。

## E3001 `call-rule` {#e3001}

**エラー。** 関数が、呼び出しを許されない種別の関数を呼んでいます。たとえば `fn` から物語への呼び出し、`pushdown fn` から `fn` の呼び出し、`const fn` から `fn` の呼び出しです。

```kis
fn start() -> Int<0, 3> { 0 }

pushdown fn visit() {
    start();
}
```

修正方法: 呼び出しを、それが許される種別の関数に移します。`fn` コードから物語に入る唯一の方法は、エントリーシーンの `nav::start` です。

## E3002 `step-outside-scene` {#e3002}

**エラー。** シーンではない関数が `s!` や `choice!` でステップを実行しています。クロージャと即時手続きも含みます。

```kix
pushdown fn visit() {
    s!(t!["no"]);
}
```

修正方法: 関数に `#[scene]` を付けるか、ステップをシーンに移します。

## E3003 `scene-call-outside-scene` {#e3003}

**エラー。** 即時手続き（`#[scene]` のない `pushdown fn`）がシーンを呼んでいます。

```kis
#[scene]
pushdown fn Other() {}

pushdown fn visit() {
    Other();
}
```

修正方法: 別の `#[scene]` 関数からシーンを呼ぶか、呼び出し元に `#[scene]` を付けます。

## E3004 `unbounded-loop` {#e3004}

**エラー。** 即時手続きが `while` などの上限のないループを使っています。即時手続きで使えるのは、配列か定数の境界を持つ範囲に対する `for` だけです。

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    while ROUTE < 3 { ROUTE += 1; }
}
```

修正方法: `for i in [1, 2] { ROUTE = i; }` のような有界ループを使うか、関数に `#[scene]` を付けます。

## E3005 `instant-recursion` {#e3005}

**エラー。** 即時手続きどうしが、直接または関数ポインターを介して循環的に呼び合っています。

```kis
pushdown fn ping() { pong(); }
pushdown fn pong() { ping(); }
```

修正方法: どれか 1 つの関数に `#[scene]` を付け、ステップを経由させます。

## E3006 `for-not-constant` {#e3006}

**エラー。** 物語コードの `for` ループが、境界が定数式でない範囲を反復しています。

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    let n = ROUTE;
    for i in 0..n { }
}
```

修正方法: 配列か、`for i in 0..MAX { ... }` のような定数の境界を持つ範囲を反復します。

## E3007 `story-variable-outside-story` {#e3007}

**エラー。** 物語変数（`static mut`）が `pushdown fn` コードの外で読み書きされています。

```kis
static mut ROUTE: Int<0, 3> = 0;

fn system() {
    let route = ROUTE;
}
```

修正方法: 物語変数には `pushdown fn` からだけアクセスします。

## E3008 `not-finite` {#e3008}

**エラー。** 物語変数、エントリーシーンのパラメーター、または表化される `const fn` 呼び出しの結果の型が、値を持ちすぎています。これらの型は有界整数、`bool`、列挙型のような有限型でなければなりません。

```kis
static mut COUNT: u32 = 0;
```

修正方法: `static mut COUNT: Int<0, 10> = 0;` のような有界の型を使います。

## E3009 `text-in-entry` {#e3009}

**エラー。** エントリーシーンのパラメーターの型または戻り値の型が `Text` を含んでいます。`fn` コードは実行時にテキストを組み立てられるため、`Text` は物語に渡せません。

```kis
enum Ending { Normal }

#[scene]
#[entry]
pushdown fn Named(name: Text) -> Ending {
    Ending::Normal
}
```

修正方法: 列挙型のような有限の値を渡し、テキストは物語の中で選びます。

## E3010 `not-constant` {#e3010}

**エラー。** `const`（または `static mut`）の初期化子が定数式ではありません。代入や `fn` 関数の呼び出しは定数式ではありません。

```kis
const COUNTED: u32 = { let mut n = 0; n += 1; n };
```

修正方法: 初期化子を `const COUNTED: u32 = 1;` のような定数式で書きます。

## E3011 `bad-handle` {#e3011}

**エラー。** 不変の `static` が、`Setting<T>` や `Meta<T>` のハンドル以外のものを保持しています。

```kis
static LIMIT_HANDLE: u32 = 3;
```

修正方法: 定数には `const`（`const LIMIT: u32 = 3;`）、物語変数には `static mut` を使います。

## E3012 `reference-in-item` {#e3012}

**エラー。** `const` または `static` の型が参照を含んでいます。

```kis
const BORROWED: &u32 = &1;
```

修正方法: `const BORROWED: u32 = 1;` のように値そのものを保持します。

## E3013 `meta-in-const` {#e3013}

**エラー。** `const` コードが `Meta` フィールドを使っています。メタは `fn` コードが読み物語が書くもので、`const` コードからは決して見えません。

```kis
static CLEARED: Meta<bool> = meta(false);

const fn render(mut screen: Screen) -> Screen {
    let cleared = CLEARED;
    screen
}
```

修正方法: メタの値を `fn` コードで読むか、パラメーターとして `const fn` に渡します。

## E3014 `second-class-reference` {#e3014}

**エラー。** 物語コードが参照をローカルに保存するか返しています。物語では参照は第二級で、引数として渡すことしかできません。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn alias(p: &mut Point) {
    let q = &p.x;
}
```

修正方法: 参照を直接引数として渡すか、`let q = p.x;` で値をコピーします。

## E3015 `second-class-closure` {#e3015}

**エラー。** 物語コードがクロージャをローカルに保存しています。物語ではクロージャは第二級で、引数として渡すことと呼び出すことしかできません。

```kis
pushdown fn twice(f: pushdown Fn()) {
    f();
    let g = f;
}
```

修正方法: `f` をローカルに束縛せず、直接呼び出します。

## E3016 `aliased-mut-argument` {#e3016}

**エラー。** 物語コードで、ある場所が `&mut` 引数として渡され、同じ呼び出しの別の引数としても渡されています。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn shift(a: &mut Point, b: &Point) {
    a.x = b.y;
}

pushdown fn alias(p: &mut Point) {
    shift(p, p);
}
```

修正方法: `shift(&mut a, &b);` のように別々の場所を渡します。

## E3017 `immutable-place` {#e3017}

**エラー。** ルートの束縛が可変として宣言されていない場所に代入しています。

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

#[scene]
pushdown fn Moves() {
    let b = Point { x: 1, y: 2 };
    b.x = 3;
}
```

修正方法: `let mut b = Point { x: 1, y: 2 };` のように束縛を可変にします。

## E3018 `misplaced-attribute` {#e3018}

**エラー。** 属性が、適用できないアイテムに使われています。たとえば `fn` への `#[scene]`、`#[scene]` のない `#[entry]`、関数への `#[must_use]`、構造体以外への `#[shader_arg]` です。

```kis
#[scene] fn not_a_story() {}
```

修正方法: `#[scene] pushdown fn not_a_story() {}` のように、適用できる場所にだけ属性を付けます。

## E3019 `choice-id` {#e3019}

**エラー。** 同じ関数の 2 つの `choice!` フォームが同じ `id` を使っています。セーブは選択肢の id を記録するため、関数内で一意でなければなりません。

```kix
enum Map { A, B }

#[scene]
pushdown fn Main() {
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
}
```

修正方法: `id = "map2"` のように、選択肢ごとに別の id を付けます。

## E3020 `choice-arms` {#e3020}

**エラー。** `choice!` が同じバリアントを複数のアームに並べています。各バリアントは最大 1 回しか選べません。

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

修正方法: 各アームを別々のバリアントに対応させます。

## E3021 `ruby-readings` {#e3021}

**エラー。** モノルビが、親文字 1 文字につきちょうど 1 つの読みを与えていません。

```kix
#[scene]
pushdown fn Main() {
    s!(t![["漢字"]("か"|"ん"|"じ")]);
}
```

修正方法: `s!(t![["漢字"]("かん"|"じ")]);` のように 1 文字につき 1 つの読みを与えます。

## E3022 `effect-not-constant` {#e3022}

**エラー。** テキストの `effect(base, e)` の効果引数が定数式ではありません。

```kis
use std::narrative::text::{effect, mark};

fn tagged(n: u8) -> Text {
    effect("a", mark(n))
}
```

修正方法: `effect("a", mark(1))` のように定数の効果を渡します。

## W3023 `unused-must-use` {#w3023}

**警告。** `Swap` のような `#[must_use]` 型の値が作られ、そのまま捨てられています。それが表す変更は、使われるまで起こりません。

```kis
const fn render(mut screen: Screen) -> Screen {
    screen.background().clear();
    screen
}
```

修正方法: `screen.background().clear().apply_animation(cut());` のように変更を適用します。

## E3024 `rng-outside-scene` {#e3024}

**エラー。** シーンではない関数が `rng_choose` を呼んでいます。乱数を引けるのはシーンだけです。

```kis
enum Coin { Heads, Tails }

pushdown fn flip() -> Coin {
    rng_choose::<Coin>()
}
```

修正方法: 関数に `#[scene]` を付けるか、シーンで値を引いてから渡します。

## E3025 `shader-arg-shape` {#e3025}

**エラー。** `#[shader_arg]` の構造体が、名前付きフィールドを 1 つ以上持つレコード構造体ではないか、型パラメータを持っています。シェーダーは引数を、型の決まった名前付きメンバーを持つ WGSL の構造体として読みます。配列の長さのような const パラメータは使えます。

```kis
#[shader_arg]
struct Pair(Decimal, Decimal);
```

修正方法: フィールドに名前を付け、型パラメータをなくします。例: `#[shader_arg] struct Pair { a: Decimal, b: Decimal }`。

## E3026 `shader-arg-field` {#e3026}

**エラー。** `#[shader_arg]` の構造体のフィールドが、シェーダーが読めない型であるか、長さ 0 の配列です。使えるフィールドの型は `bool`、`i32`、`u32`、`Decimal`、`Rat`、`Color`、`Point`、`Offset`、`Size`、`Anchor`、`Angle`、`Duration`、フィールドのない enum、他の `#[shader_arg]` 構造体、およびこれらの空でない配列です。

```kis
#[shader_arg]
struct Caption {
    text: String,
}
```

修正方法: シェーダーには数値のデータだけを渡します。例: `#[shader_arg] struct Caption { strength: Decimal }`。
