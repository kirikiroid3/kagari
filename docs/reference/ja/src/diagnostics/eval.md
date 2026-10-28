# 定数評価 (E4xxx) {#constant-evaluation-e4xxx}

コンパイラは `const` コードをコンパイル時に実行します。定数を評価し、物語から呼ばれる `const fn` の呼び出しを表化し、画面とテキストのマークを組み立てます。ここに並ぶ診断は、その評価中に見つかった誤りを報告します。レンダー関数の中の誤りは、そのレンダー関数を使うステップの位置で報告されます。

## E4001 `overflow` {#e4001}

**エラー。** 算術演算の結果がその整数型に収まりません。

```kis
const OVERFLOW: u8 = 200u8 + 100u8;
```

修正: より広い型を使うか、オペランドを小さくします。例: `const OVERFLOW: u16 = 200u16 + 100u16;`

## E4002 `division-by-zero` {#e4002}

**エラー。** 定数コードがゼロで除算しています。整数の `/` や `%`、または `Rat::new(1, 0)` のように分母がゼロの有理数を作った場合です。`Decimal / Decimal` は正確に計算してから 7 桁に切り捨てるため、割り切れないことによるエラーは報告しません。

```kis
const fn divide(a: i32, b: i32) -> i32 {
    a / b
}
const DIVISION: i32 = divide(1, 0);
```

修正: 除数がゼロにならないようにします。例: `divide(1, 2)`

## E4003 `budget-exceeded` {#e4003}

**エラー。** 定数の評価がステップ上限（1000000 ステップ）を超えました。たいていは長いループや終わらないループ、再帰が原因です。

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

修正: 計算量を減らすか、その値を `const` コードの外で計算します。

## E4004 `domain-too-large` {#e4004}

**エラー。** 物語のコードが実行時の引数で `const fn` を呼んでいるため、コンパイラは引数の値のすべての組み合わせについて呼び出しを表化する必要があります。組み合わせが 65536 を超えると、このエラーになります。

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

修正: `Int<0, 3>` のようにパラメーターの型を狭めて、定義域を小さくします。

## E4005 `index-out-of-range` {#e4005}

**エラー。** 配列または `Vec` へのインデックスが長さを超えています。

```kis
const INDEX: i32 = {
    let a = [1, 2];
    a[2]
};
```

修正: 長さより小さいインデックスを使います。例: `a[1]`

## E4006 `const-cycle` {#e4006}

**エラー。** `const` の値が、直接または他の定数を通して自分自身に依存しているため、コンパイル時に計算できません。循環に含まれるすべての定数が、それぞれの名前の位置で報告されます。循環中の定数を使うだけの定数は報告されません。

```kis
const A: u32 = B + 1;
const B: u32 = A;
```

修正: どれか 1 つの定数に、元を参照しない値を与えて循環を断ちます。例: `const B: u32 = 0;`

## E4007 `handle-outside-static` {#e4007}

**エラー。** `Setting` または `Meta` のハンドルが、`static` の初期化子以外の場所で作られています。

```kis
const HANDLE: Setting<bool> = setting(true);
```

修正: static として宣言します。例: `static HANDLE: Setting<bool> = setting(true);`

## E4008 `invalid-motion` {#e4008}

**エラー。** モーションまたはシェイクの形が不正です。たとえば、キーがない、時刻ゼロのキーがある、キーの時刻が厳密に増加していない、最後のキーが恒等変換でない、繰り返し回数がゼロ、などです。

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

修正: 恒等変換で終わるキーを少なくとも 1 つ与えます。例: `motion().key(ms(100), offset(0, 0))`

## E4009 `invalid-easing` {#e4009}

**エラー。** イージングのパラメーターが有効範囲外です。たとえば `CubicBezier` の x 座標が `0.0..=1.0` の外にある場合や、`Steps(0)` です。

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

修正: パラメーターを範囲内に収めます。例: `Easing::CubicBezier(0.5, 0.0, 0.5, 1.0)`

## E4010 `duplicate-mark` {#e4010}

**エラー。** 1 つのテキストの中に同じマークが 2 回以上現れています。

```kis
use std::narrative::text::{concat, effect, mark};

const DUPLICATE_MARKS: Text = concat([effect("a", mark(1)), effect("b", mark(1))]);
```

修正: マークごとに別の番号を付けます。例: 2 つ目を `mark(2)` にする。

## E4011 `begin-text-on-mark` {#e4011}

**エラー。** タイムラインがテキストのマークに固定されている間に、レンダーが `begin_text` を呼んでいます。

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

修正: タイムラインをマークに固定する前に `begin_text` を呼びます。

## E4012 `fraction-out-of-range` {#e4012}

**エラー。** 不透明度、音量、アンカー座標など、`0.0..=1.0` の割合でなければならない値がその範囲外です。

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

修正: 値を `0.0..=1.0` に収めます。例: `.change(1.0)`

## E4013 `duration-overflow` {#e4013}

**エラー。** `Duration` が `u32` のミリ秒に収まりません。作成時、または演算の後に起こります。

```kis
const SECS_OVERFLOW: Duration = secs(5000000);
```

修正: より短い長さを使います。例: `secs(5000)`
