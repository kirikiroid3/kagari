---
title: "言語トレイト"
---

Kagari のトレイトは、ゲームを書くための道具ではありません。トレイトは標準ライブラリが決まった数だけ宣言しており、どれにも [`#[builtin]`](./attributes.md#builtin) が付いています。実装するのも標準ライブラリだけです。トレイトの目的は二つあります。オーバーロードや暗黙の型変換に頼らずに `std` のシグネチャを型検査できるようにすること、そしてコンパイラが「`==` には `Eq` が必要」「`rng_choose` は `RandChoose` の値を引く」といった規則を表現できるようにすることです。

ゲームのコードでトレイトを使うのは、自分で定義したジェネリックパラメータに**境界**として付けるときだけです。なお `Eq` と `RandChoose` は、ゲームで宣言した型でも条件を満たせば自動的に実装されます。`ShaderArg` も、ゲームで [`#[shader_arg]`](./attributes.md#shader_arg) を付けた構造体には実装されます。

## 概要 {#overview}

| トレイト | 宣言場所 | プレリュード | 実装する型 | 使われる場所 |
| -------- | -------- | ------------ | ---------- | ------------ |
| [`Eq`](#eq) | `std` | あり | プリミティブ型、関数ポインタ、構成要素がすべて `Eq` の型（構造的に判定）、一部の組み込み型 | `==`、`!=`、`ui::select` |
| [`Integer`](#integer) | `std` | あり | `Int<lo, hi>` と Rust の整数型 | `into`、`saturate` |
| [`FromInteger`](#frominteger) | `std` | なし | `Int<lo, hi>`、Rust の整数型、`Decimal` | `into` の変換先 |
| [`ToText`](#totext) | `std::narrative::text` | なし | `Text`、`String`、整数型、`Rat`、`Decimal` | `t![…]` 内の `{expr}`、`to_text` |
| [`RandChoose`](#randchoose) | `std` | あり | `Int<lo, hi>` と、フィールドのない列挙型すべて | `rng_choose` |
| [`Asset`](#asset) | `std` | なし | ファイルを指す 5 つのアセット型と 6 つのシェーダー型 | `asset` |
| [`Step`](#step) | `std::narrative` | なし | `Say`、`Choice<A, N>` | `push_step`、`s!`、`choice!` |
| [`ShaderArg`](#shaderarg) | `std::screen::shader` | なし | `()` と `#[shader_arg]` 構造体 | `filter` などのシェーダー用コンストラクター |
| [`Shader`](#shader) | `std::screen::shader` | なし | 6 つのシェーダー型 | `std` の内部 |

プレリュードにないトレイトは、`std::narrative::text::ToText` のようにパスで指定するか、`use` でインポートして使います。

## ゲーム側の規則 {#rules-for-games}

- ゲームのコードで `trait` アイテムを書くとエラー `E1012` になります。
- ゲームのコードで `impl Trait for Type` を書くとエラー `E1023` になります。`Eq` の場合は、自動的に実装されることがエラーメッセージで示されます。
- `derive`、ユーザー定義の関連型、`<T as Trait>::f` のような修飾パスはありません。
- 自分で定義した構造体や列挙型には、トレイトを伴わない通常の `impl` ブロックを書けます。メソッドや関連関数を定義するためのもので、トレイトとは関係ありません。

`std` の中では、トレイトにはすべて `#[builtin]` が必要です（`E1013`）。

## 境界 {#bounds}

関数、構造体、列挙型、`impl` ブロックは、型や定数をパラメータに取るジェネリックな定義にできます。型パラメータには境界を付けられ、複数の境界は `+` でつなぎます。

```kis
use std::narrative::text::{ToText, to_text};

const fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

const fn describe<T: Eq + ToText>(a: T, b: T) -> Text {
    if a == b { to_text(a) } else { "different" }
}
```

- 境界はパラメータリストにしか書けません。`where` 節はありません。
- 境界にはトレイトを指定します（`E2008`）。型引数が境界のトレイトを実装していないとエラー `E2026` になります。
- ジェネリックなコードの中では、型パラメータが実装しているとみなされるのは境界に書いたトレイトだけです。たとえば `T: Eq` のない `T` で `a == b` と書くとエラー `E2017` になります。
- `pushdown fn` では、型引数はすべて[有限型](../concepts/kinds.md#finite-types)でなければなりません。ジェネリックなコードは型引数の組み合わせごとに具体化されるため、物語は有限のプログラムのまま保たれます。

## `Eq` {#eq}

```kis
#[builtin] pub trait Eq {}
```

`Eq` は、`==` と `!=` で値を比較できる型を表します。両辺は同じ型で、その型が `Eq` を実装している必要があります（`E2017`）。`std::ui` の関数 `select` も、値の型に `T: Eq` を求めます。

`Eq` は自動的に実装されるので、何も書く必要はありません。

| 型 | `Eq` になる条件 |
| -- | --------------- |
| `bool`、`()`、`Int<lo, hi>`、Rust の整数型 | 常に |
| 関数ポインタ、名前付き関数 | 常に。同一の関数かどうかで比較 |
| タプル、配列 | 要素の型がすべて `Eq` のとき |
| Kagari で宣言した構造体と列挙型（ゲーム側でも `std` 側でも） | すべてのバリアントのすべてのフィールドが `Eq` のとき。同じ形に組み立てた値どうしが等しくなる |
| `&T`、`&mut T` | `T` が `Eq` のとき。参照先の値を比較 |
| `#[builtin]` 型 | `std` に `impl Eq` があるものだけ。`String`、`T` が `Eq` の `Vec<T>`、`Rat`、`Decimal`、`Text`、`Screen`、6 つのアセット型、6 つのシェーダー型、`ShaderArgs` |
| クロージャ | なし |

そのため、`Setting<T>`、`Meta<T>`、`Narrative`、`Element`、`Placed`、`Arm`、`Divergence`、画面の部品（`Entity`、`Prop<T>`、各レイヤー、`Hud`、`ChoiceMenu`、`Camera`、`Channel`）、保留中の変更（`Change<T>`、`Swap`、`ChannelChange`）は `Eq` ではありません。これらを含む構造体や列挙型も同様です。Kagari に浮動小数点数がないのは、数値型をすべて `Eq` にしたいという理由もあります。

## `Integer` {#integer}

```kis
#[builtin] pub trait Integer {
    const fn into<T: FromInteger>(self) -> T;
    const fn saturate<T: Integer>(self) -> T;
}
```

`Integer` を実装するのは、`Int<lo, hi>` と `u8`、`u16`、`u32`、`u64`、`u128`、`i8`、`i16`、`i32`、`i64`、`i128`、`usize`、`isize` です。整数型どうしの変換は、この二つのメソッドでしか行えません。変換先の型は文脈から推論されます。

| メソッド | 結果 |
| -------- | ---- |
| `x.into()` | 同じ値を、整数型または `Decimal`（[`FromInteger`](#frominteger) を実装する型）として返す。変換元の型の値がすべて変換先の型に収まる場合に限り使え、そうでなければコンパイルエラー（`E2018`） |
| `x.saturate()` | 変換先の整数型の範囲に収まるよう切り詰めた値 |

```kis
static MUSIC: Setting<Volume> = setting(volume(60));

fn set_music_level(level: u32) {
    let level: Int<0, 100> = level.saturate();
    MUSIC.set(volume(level));
}

const fn label_width(chars: Int<0, 40>) -> u32 {
    let n: u32 = chars.into();
    n * 16
}

const fn half_width(chars: Int<0, 40>) -> Decimal {
    let n: Decimal = chars.into();
    n * 0.5
}
```

`as` によるキャストはありません（`E2019`）。詳しくは[数値](../syntax/numbers.md)を参照してください。

## `FromInteger` {#frominteger}

```kis
#[builtin] pub trait FromInteger {}
```

`FromInteger` は `Integer::into` の変換先になれる型を表します。`Integer` を実装する型すべてと `Decimal` が該当します。整数はどれも `Decimal` に収まるため、`Decimal` への `into` は必ずコンパイルが通ります。これは `Decimal::from_int` と同じ変換です。一方、`saturate` の境界は `Integer` のままなので、`Decimal` への `saturate` はエラー `E2026` になります。

## `ToText` {#totext}

```kis
#[builtin] pub trait ToText {}
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

`ToText` は、`t![…]` の `{expr}` で埋め込める型を表します。`{expr}` は `to_text(expr)` の呼び出しと同じ意味です。実装するのは `Text`、`String`、`Int<lo, hi>`、Rust の整数型すべて、`Rat`、`Decimal` です。`bool` や列挙型は `ToText` を実装していないので、`match` で分岐して文面を書いてください。それ以外の型を埋め込むとエラー `E2026` になります。

`pushdown fn` の中で、`Int<0, 5>` の物語変数のような有限の実行時の値に `to_text` を使うと、他の `const fn` 呼び出しと同じく表化されます。そのため台詞は、物語の状態から決まる有限の関数のままです。

## `RandChoose` {#randchoose}

```kis
#[builtin] pub trait RandChoose {}
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

`RandChoose` は、シーンが `rng_choose::<T>()` で無作為に引ける型を表します。実装するのは `Int<lo, hi>` と、バリアントが 1 つ以上あり、どのバリアントにもフィールドがない列挙型です。列挙型には自動的に実装されます。`bool` やデータを持つ列挙型は `RandChoose` ではありません。`rng_choose` を呼べるのはシーンだけです（`E3024`）。

```kix
enum Weather { Sunny, Rainy, Snowy }

#[scene]
pushdown fn Morning() {
    match rng_choose::<Weather>() {
        Weather::Sunny => s!(t!["The sun is out."]),
        Weather::Rainy => s!(t!["It is raining."]),
        Weather::Snowy => s!(t!["Snow covers the yard."]),
    }
}
```

詳しくは[乱数](../concepts/scenes.md#randomness)を参照してください。

## `Asset` {#asset}

```kis
#[builtin] pub trait Asset {}
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

`Asset` は、値がプログラム中のアセット定数だけからなる型を表します。`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` と、6 つのシェーダー型 `ColorShader<A>`、`MixShader<A>`、`TextureShader<A>`、`MaskShader<A>`、`LocalMotionShader<A>`、`TransitionalMotionShader<A>` が該当します。アセット定数は `asset` で宣言し、`Asset` はその `asset` の境界になっています。[アセット型](./builtin-types.md#asset-types)を参照してください。

## `Step` {#step}

```kis
#[builtin] pub trait Step {
    type Output;
}
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`Step` は、物語を中断させる値を表します。実装するのは、`s!(…)` で書くステップ `Say`（`Output = ()`）と、`choice!{…}` で書くステップ `Choice<A, N>`（`Output = A`、つまりアームの値の型）です。`push_step` はステップを実行し、その結果を受け取って処理を再開します。関連型を持つトレイトは `Step` だけです。ステップを実行できるのはシーンだけです（`E3002`）。詳しくは[ステップ](../concepts/steps.md)を参照してください。

## `ShaderArg` {#shaderarg}

```kis
#[builtin] pub trait ShaderArg {}
```

`ShaderArg` は、シェーダーが引数として受け取れる型を表します。ユニット型 `()` と、[`#[shader_arg]`](./attributes.md#shader_arg) を付けた構造体が該当します。`std::screen::shader` のコンストラクターは、`filter<A: ShaderArg>(shader: ColorShader<A>, args: A)` のように、このトレイトで引数の型を制約しています。[`std::screen::shader`](../../std/screen-shader.md) を参照してください。

## `Shader` {#shader}

```kis
#[builtin] pub trait Shader {}
```

`Shader` は 6 つのシェーダー型を表します。標準ライブラリが役割を問わずシェーダーを受け取るために使うもので、ゲームのコードで使うことはありません。

## クロージャ型はトレイトではない {#closure-types-are-not-traits}

`Fn(A) -> B`、`const Fn(A) -> B`、`pushdown Fn(A) -> B` は Rust の `Fn` トレイトに似ていますが、Kagari では型です。[種別](../concepts/kinds.md)の情報を含むクロージャ型であり、境界には使えません。[関数ポインタ型とクロージャ型](./builtin-types.md#function-types)を参照してください。
