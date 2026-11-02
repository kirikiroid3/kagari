---
title: "定数評価 (E4xxx)"
---

コンパイラは `const` コードをコンパイル時に実行します。定数の評価、物語から呼ばれる `const fn` の表化、画面やテキストのマークの組み立てなどです。ここで扱う診断は、この評価の途中で見つかった誤りを知らせます。レンダー関数の中の誤りは、そのレンダー関数を使うステップの位置に報告されます。

## E4001 `overflow` {#e4001}

**エラー。** 算術演算の結果が整数型の範囲に収まりません。

```kis
const OVERFLOW: u8 = 200u8 + 100u8;
```

直し方：より大きな型を使うか、オペランドを小さくします（例：`const OVERFLOW: u16 = 200u16 + 100u16;`）。

## E4002 `division-by-zero` {#e4002}

**エラー。** 定数コードの中でゼロ除算が起きています。整数の `/` や `%` で 0 で割った場合と、`Rat::new(1, 0)` のように分母が 0 の有理数を作った場合が該当します。`Decimal / Decimal` は正確に計算してから 7 桁に切り捨てるので、割り切れないことを理由にエラーになることはありません。

```kis
const fn divide(a: i32, b: i32) -> i32 {
    a / b
}
const DIVISION: i32 = divide(1, 0);
```

直し方：除数が 0 にならないようにします（例：`divide(1, 2)`）。

## E4003 `budget-exceeded` {#e4003}

**エラー。** 定数の評価がステップ数の上限（1000000 ステップ）を超えました。原因の多くは、長いループや終わらないループ、再帰です。

```kis
const fn spin(n: u64) -> u64 {
    let mut i = 0u64;
    while i < n {
        i = i + 1;
    }
    i
}
const BUDGET: u64 = spin(100000000);
```

直し方：計算量を減らすか、その値を `const` コードの外で計算します。

## E4004 `domain-too-large` {#e4004}

**エラー。** 物語のコードが実行時の値を引数にして `const fn` を呼んでいます。この場合、コンパイラは引数の値の組み合わせをすべて表化する必要があります。組み合わせが 65536 通りを超えるとエラーになります。

```kis
static mut BIG: Int<0, 100000> = 0;

const fn same(x: Int<0, 100000>) -> Int<0, 100000> {
    x
}

#[scene]
pushdown fn Start() {
    let big = same(BIG);
}
```

直し方：引数の型を `Int<0, 3>` のように狭めて、定義域を小さくします。

## E4005 `index-out-of-range` {#e4005}

**エラー。** 配列や `Vec` の添字が長さを超えています。

```kis
const INDEX: i32 = {
    let a = [1, 2];
    a[2]
};
```

直し方：長さより小さい添字を使います（例：`a[1]`）。

## E4006 `const-cycle` {#e4006}

**エラー。** `const` の値が、直接またはほかの定数を経由して自分自身に依存しているため、コンパイル時に計算できません。循環に含まれる定数はすべて、それぞれの名前の位置に報告されます。循環中の定数を使っているだけの定数は報告されません。

```kis
const A: u32 = B + 1;
const B: u32 = A;
```

直し方：どれか一つの定数に、循環元を参照しない値を与えて循環を断ち切ります（例：`const B: u32 = 0;`）。

## E4007 `handle-outside-static` {#e4007}

**エラー。** `Setting` や `Meta` のハンドルを、`static` の初期化子以外の場所で作っています。

```kis
const HANDLE: Setting<bool> = setting(true);
```

直し方：`static` として宣言します（例：`static HANDLE: Setting<bool> = setting(true);`）。

## E4008 `invalid-motion` {#e4008}

**エラー。** モーションやシェイクの形が正しくありません。たとえば、キーが一つもない、時刻 0 のキーがある、キーの時刻が厳密に増えていない、最後のキーが恒等変換でない、繰り返し回数が 0 である、などの場合です。

```kix
const A: EntityId = EntityId(3);

const fn no_keys(mut screen: Screen) -> Screen {
    screen.entity(A).apply_animation(motion());
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = no_keys, t!["1"]);
}
```

直し方：恒等変換で終わるキーを少なくとも一つ指定します（例：`motion().key(ms(100), offset(0, 0))`）。

## E4009 `invalid-easing` {#e4009}

**エラー。** イージングの引数が有効な範囲外です。たとえば `CubicBezier` の x 座標が `0.0..=1.0` の外にある場合や、`Steps(0)` の場合です。

```kix
const A: EntityId = EntityId(3);

const fn bezier(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(1.0).apply_animation(tween(ms(100)).ease(Easing::CubicBezier(1.5, 0.0, 0.5, 1.0)));
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = bezier, t!["1"]);
}
```

直し方：引数を範囲内に収めます（例：`Easing::CubicBezier(0.5, 0.0, 0.5, 1.0)`）。

## E4010 `duplicate-mark` {#e4010}

**エラー。** 一つのテキストの中に、同じマークが複数回出てきます。

```kis
use std::narrative::text::{concat, effect, mark};

const DUPLICATE_MARKS: Text = concat([effect("a", mark(1)), effect("b", mark(1))]);
```

直し方：マークごとに別の番号を付けます（例：2 つ目を `mark(2)` にする）。

## E4011 `begin-text-on-mark` {#e4011}

**エラー。** タイムラインをテキストのマークに合わせている間に、レンダーが `begin_text` を呼んでいます。

```kix
use std::narrative::Mark;

const fn text_on_mark(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(2));
    screen.begin_text();
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = text_on_mark, t!["1"]);
}
```

直し方：タイムラインをマークに合わせる前に `begin_text` を呼びます。

## E4012 `fraction-out-of-range` {#e4012}

**エラー。** `0.0..=1.0` の割合で指定すべき値（不透明度、音量、アンカー座標など）が範囲外です。

```kix
const A: EntityId = EntityId(3);

const fn opacity(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(1.5).apply_animation(instant());
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = opacity, t!["1"]);
}
```

直し方：値を `0.0..=1.0` に収めます（例：`.change(1.0)`）。

## E4013 `duration-overflow` {#e4013}

**エラー。** `Duration` が `u32` のミリ秒で表せる範囲を超えています。作成時にも、演算の結果としても起こります。

```kis
const SECS_OVERFLOW: Duration = secs(5000000);
```

直し方：もっと短い時間にします（例：`secs(5000)`）。
