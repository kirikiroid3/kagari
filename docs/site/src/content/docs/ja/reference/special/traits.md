---
title: "言語トレイト"
---

Kagari のトレイトはゲームのための道具ではありません。標準ライブラリが固定のトレイト群を宣言し、どれも [`#[builtin]`](./attributes.md#builtin) が付いています。それらを実装するのも標準ライブラリだけです。トレイトがあるのは、オーバーロードや暗黙の変換なしに `std` のシグネチャを型検査するため、そしてコンパイラが規則を述べられるようにするため（「`==` には `Eq` が必要」「`rng_choose` は `RandChoose` を引く」）です。

ゲームがこれらのトレイトを使う方法は一つだけです。自分のジェネリックパラメータへの**境界**です。そのうち `Eq` と `RandChoose` の二つは、ゲームが宣言した適切な型に対して自動的にも成り立ち、`ShaderArg` はゲームが [`#[shader_arg]`](./attributes.md#shader_arg) を付けた構造体に成り立ちます。

## 概要 {#overview}

| トレイト | 宣言場所 | プレリュード | 実装する型 | 使う場所 |
| -------- | -------- | ------------ | ---------- | -------- |
| [`Eq`](#eq) | `std` | あり | プリミティブ、関数ポインタ、構成要素がすべて `Eq` の型（構造的に）、一部のビルトイン型 | `==`、`!=`、`ui::select` |
| [`Integer`](#integer) | `std` | あり | `Int<lo, hi>` と Rust の整数 | `into`、`saturate` |
| [`FromInteger`](#frominteger) | `std` | なし | `Int<lo, hi>`、Rust の整数、`Decimal` | `into` の変換先 |
| [`ToText`](#totext) | `std::narrative::text` | なし | `Text`、`String`、整数、`Rat`、`Decimal` | `t![…]` の `{expr}`、`to_text` |
| [`RandChoose`](#randchoose) | `std` | あり | `Int<lo, hi>` とフィールドのないすべての列挙型 | `rng_choose` |
| [`Asset`](#asset) | `std` | なし | 5 つのファイルのアセット型と 6 つのシェーダー型 | `asset` |
| [`Step`](#step) | `std::narrative` | なし | `Say`、`Choice<A, N>` | `push_step`、`s!`、`choice!` |
| [`ShaderArg`](#shaderarg) | `std::screen::shader` | なし | `()` と `#[shader_arg]` 構造体 | `filter` などのシェーダーのコンストラクター |
| [`Shader`](#shader) | `std::screen::shader` | なし | 6 つのシェーダー型 | `std` の内部 |

プレリュードにないトレイトは、`std::narrative::text::ToText` のようにパスで書くか、`use` でインポートします。

## ゲームに対する規則 {#rules-for-games}

- ゲームの中の `trait` アイテムはエラーです（`E1012`）。
- ゲームの中の `impl Trait for Type` はエラーです（`E1023`）。`Eq` の場合、メッセージは `Eq` が自動であることを指摘します。
- `derive`、ユーザー定義の関連型、`<T as Trait>::f` のような修飾パスはありません。
- ゲームは自分の構造体と列挙型に固有の `impl` ブロックを書き、メソッドや関連関数を定義できます。これはトレイトとは無関係です。

`std` の中では、すべてのトレイトが `#[builtin]` でなければなりません（`E1013`）。

## 境界 {#bounds}

関数、構造体、列挙型、`impl` ブロックは、型と定数についてジェネリックにできます。型パラメータには `+` でつないだ境界を付けられます。

```kis
use std::narrative::text::{ToText, to_text};

const fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

const fn describe<T: Eq + ToText>(a: T, b: T) -> Text {
    if a == b { to_text(a) } else { "different" }
}
```

- 境界はパラメータリストにだけ書きます。`where` 節はありません。
- 境界はトレイトを指さなければなりません（`E2008`）。境界を実装しない型引数は `E2026` です。
- ジェネリックなコードの中では、型パラメータはちょうど境界のトレイトだけを実装します。`T: Eq` のない `T` に対する `a == b` は `E2017` です。
- `pushdown fn` の中では、すべての型引数が[有限型](../concepts/kinds.md#finite-types)でなければなりません。ジェネリックなコードは引数の組ごとに具体化されるので、物語は有限なプログラムのままです。

## `Eq` {#eq}

```kis
#[builtin] pub trait Eq {}
```

`Eq` は、値を `==` と `!=` で比較できる型を表します。両辺は同じ型で、その型は `Eq` でなければなりません（`E2017`）。`std::ui` の関数 `select` も値の型に `T: Eq` を要求します。

`Eq` は自動的に実装されます。書くことは何もありません。

| 型 | `Eq` |
| -- | ---- |
| `bool`、`()`、`Int<lo, hi>`、Rust の整数 | 常に |
| 関数ポインタと名前付き関数 | 常に。同一性で比較します |
| タプル、配列 | すべての要素型が `Eq` のとき |
| Kagari で宣言された構造体と列挙型（皆さんのものも `std` のものも） | すべてのバリアントのすべてのフィールドが `Eq` のとき。同じように組み立てられた二つの値が等しくなります |
| `&T`、`&mut T` | `T` が `Eq` のとき。指す先の値を比較します |
| `#[builtin]` 型 | `std` の `impl Eq` によってのみ：`String`、`Eq` な `T` の `Vec<T>`、`Rat`、`Decimal`、`Text`、`Screen`、6 つのアセット型、6 つのシェーダー型、`ShaderArgs` |
| クロージャ | 決してならない |

したがって `Setting<T>`、`Meta<T>`、`Narrative`、`Element`、`Placed`、`Arm`、`Divergence`、画面の部品（`Entity`、`Prop<T>`、各レイヤー、`Hud`、`ChoiceMenu`、`Camera`、`Channel`）、保留中の変更（`Change<T>`、`Swap`、`ChannelChange`）は `Eq` ではなく、それらを含む構造体や列挙型も `Eq` ではありません。Kagari に浮動小数点数がない理由の一つは、すべての数値型を `Eq` にするためです。

## `Integer` {#integer}

```kis
#[builtin] pub trait Integer {
    const fn into<T: FromInteger>(self) -> T;
    const fn saturate<T: Integer>(self) -> T;
}
```

`Integer` を実装するのは `Int<lo, hi>` と、`u8`、`u16`、`u32`、`u64`、`u128`、`i8`、`i16`、`i32`、`i64`、`i128`、`usize`、`isize` です。この二つのメソッドが整数型どうしの唯一の変換で、変換先の型は文脈から推論されます。

| メソッド | 結果 |
| -------- | ---- |
| `x.into()` | 同じ値を整数型か `Decimal`（[`FromInteger`](#frominteger)）として返します。変換元の型のすべての値が変換先の型に収まらなければコンパイルエラー（`E2018`） |
| `x.saturate()` | 変換先の型（整数型）の範囲に切り詰めた値 |

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

`as` キャストはありません（`E2019`）。[数値](../syntax/numbers.md)を参照してください。

## `FromInteger` {#frominteger}

```kis
#[builtin] pub trait FromInteger {}
```

`FromInteger` は `Integer::into` の変換先を表します：`Integer` を実装するすべての型と `Decimal` です。整数はすべて `Decimal` に収まるので、`Decimal` への `into` は常にコンパイルできます。これは `Decimal::from_int` と同じ変換です。`saturate` の境界は `Integer` のままなので、`Decimal` への `saturate` は `E2026` です。

## `ToText` {#totext}

```kis
#[builtin] pub trait ToText {}
#[builtin] pub const fn to_text<T: ToText>(value: T) -> Text;
```

`ToText` は、`t![…]` の `{expr}` 項目で埋め込める型を表します。`{expr}` は `to_text(expr)` の呼び出しです。実装するのは `Text`、`String`、`Int<lo, hi>`、すべての Rust の整数、`Rat`、`Decimal` です。`bool` と列挙型は `ToText` ではないので、`match` で分岐してテキストを書きます。それ以外の型を埋め込むと `E2026` です。

`pushdown fn` の中では、`Int<0, 5>` の物語変数のような有限な実行時の値に対する `to_text` は他の `const fn` 呼び出しと同じく表化されるので、台詞は物語の状態の有限な関数のままです。

## `RandChoose` {#randchoose}

```kis
#[builtin] pub trait RandChoose {}
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

`RandChoose` は、シーンが `rng_choose::<T>()` で無作為に引ける型を表します。実装するのは `Int<lo, hi>` と、バリアントが一つ以上あってどのバリアントもフィールドを持たないすべての列挙型（自動）です。`bool` とデータを持つ列挙型は `RandChoose` ではありません。`rng_choose` を呼べるのはシーンだけです（`E3024`）。

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

[乱数](../concepts/scenes.md#randomness)を参照してください。

## `Asset` {#asset}

```kis
#[builtin] pub trait Asset {}
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

`Asset` は、値がちょうどプログラムのアセット定数である型を表します。`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` と、6 つのシェーダー型 `ColorShader<A>`、`MixShader<A>`、`TextureShader<A>`、`MaskShader<A>`、`LocalMotionShader<A>`、`TransitionalMotionShader<A>` です。これはアセット定数を宣言する `asset` の境界です。[アセット型](./builtin-types.md#asset-types)を参照してください。

## `Step` {#step}

```kis
#[builtin] pub trait Step {
    type Output;
}
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

`Step` は、物語が中断できる値を表します。実装するのは、`s!(…)` と書くステップ `Say`（`Output = ()`）と、`choice!{…}` と書くステップ `Choice<A, N>`（`Output = A`、アームの値の型）です。`push_step` はステップを実行し、その出力で再開します。関連型を持つ唯一のトレイトです。ステップを実行できるのはシーンだけです（`E3002`）。[ステップ](../concepts/steps.md)を参照してください。

## `ShaderArg` {#shaderarg}

```kis
#[builtin] pub trait ShaderArg {}
```

`ShaderArg` は、シェーダーが引数として受け取れる型を表します。ユニット型 `()` と、[`#[shader_arg]`](./attributes.md#shader_arg) を付けたすべての構造体です。`std::screen::shader` のコンストラクターは、`filter<A: ShaderArg>(shader: ColorShader<A>, args: A)` のように引数型をこれで制約します。[`std::screen::shader`](../../std/screen-shader.md) を参照してください。

## `Shader` {#shader}

```kis
#[builtin] pub trait Shader {}
```

`Shader` は 6 つのシェーダー型を表します。標準ライブラリが任意のロールのシェーダーを受け取るために使うもので、ゲームが使うことはありません。

## クロージャ型はトレイトではない {#closure-types-are-not-traits}

`Fn(A) -> B`、`const Fn(A) -> B`、`pushdown Fn(A) -> B` は Rust の `Fn` トレイトに似ていますが、Kagari では型です。[種別](../concepts/kinds.md)を持つクロージャ型で、境界には使えません。[関数ポインタ型とクロージャ型](./builtin-types.md#function-types)を参照してください。
