---
title: "属性"
---

属性は、アイテムの直前に `#[name]` の形で書きます。Kagari の属性は 5 種類だけで、どれも引数を取りません。ゲームのコードで使うのはそのうち 4 種類で、`#[builtin]` は標準ライブラリ専用です。

| 属性 | 付けられる対象 | 意味 | 誤った使い方 |
| ---- | -------------- | ---- | ------------ |
| [`#[scene]`](#scene) | `pushdown fn` | シーンにする。ステップの実行、シーンの呼び出し、乱数の取得ができる | `E3018` |
| [`#[entry]`](#entry) | `#[scene]` の関数 | 開始シーンにする。システムがここから物語を始められる | `E3018`、`E3008`、`E3009` |
| [`#[must_use]`](#must_use) | `struct`、`enum` | この型の値は捨てずに最後まで完了させる必要がある | `E3018`。使用箇所で警告 `W3023` |
| [`#[shader_arg]`](#shader_arg) | `struct` | この型をシェーダーの引数にできる | `E3018`、`E3025`、`E3026` |
| [`#[builtin]`](#builtin) | 標準ライブラリ内の関数、構造体、トレイトのみ | コンパイラとエンジンが実装する | `E1008`、`E1009`、`E1013` |

## 構文 {#syntax}

```kix
/// The first scene of the game.
#[scene]
#[entry]
pub pushdown fn Main() {
    s!(t!["It was the last day of summer."]);
}
```

- 属性はアイテムの前に置きます。付けられるのは関数（`impl` 内の関連関数を含む）、`struct`、`enum`、`const`、`static`、`type`、`mod`、`use`、`impl`、`trait` です。位置はドキュメントコメントより後、`pub` より前です。
- 属性は複数重ねて書けます。1 行にまとめても行を分けてもよく、順序も自由です。
- `///` のドキュメントコメントはただのコメントで、属性ではありません。
- 内部属性（`#![…]`）はありません。また、属性の後には必ずアイテムが続きます。どちらに反しても構文エラー（`E0001`）です。文や式には属性を付けられません。
- 5 種類以外の名前を書くとエラー `E1006` になり、その属性は無視されます。
- `#[must_use("reason")]` のように引数を付けるとエラー `E1007` になります。ただし属性そのものは有効です。

*暫定。* パーサーは構造体のフィールド、タプルのフィールド、列挙型のバリアントに付いた属性も受け付けます。ただし現在のコンパイラは、未知の名前も含めてそこに付いた属性を無視します。この挙動には頼らないでください。

## `#[scene]` {#scene}

`#[scene]` を付けた `pushdown fn` は**シーン**になります。次のことができるのはシーンだけです。

- ステップの実行：`s!(…)`、`choice!{…}`、`push_step` の呼び出し（シーン以外では `E3002`）
- 他のシーンの呼び出し。自分自身の呼び出しも含みます（シーン以外では `E3003`）
- `rng_choose` による乱数の取得（シーン以外では `E3024`）

`#[scene]` のない `pushdown fn` は**即時手続き**です。シーン内のループや呼び出しの循環は、どれも必ずステップを通る必要があります（`E5001`、`E5002`）。シーンを指すポインタの型は `scene fn(…) -> R` です。

```kix
#[scene]
pushdown fn Greeting() {
    s!(who = "Mio", t!["Hello."]);
}
```

`fn` や `const fn` など、`pushdown fn` 以外に `#[scene]` を付けるとエラー `E3018` になります。

詳しくは[シーンと即時手続き](../concepts/scenes.md)を参照してください。

## `#[entry]` {#entry}

`#[entry]` を付けたシーンは**開始シーン**になります。ゲームシステムが物語を始められる場所です。同じ関数に `#[scene]` も必要で、ないと `#[entry]` はエラー `E3018` になります。二つの属性の順序は問いません。

- システムは `nav::start(Scene, args, on_exit)` で開始シーンを始めます。第 1 引数には `#[entry]` シーンのパスを直接書く必要があり、それ以外の式は書けません（`E2027`）。`args` はシーンの引数を並べたタプルで、戻り値は `on_exit` が受け取ります。
- 他のシーンと同じく、引数と戻り値の型は有限型でなければなりません（`E3008`）。加えて `Text` を含むこともできません（`E3009`）。`fn` のコードは実行時にテキストを組み立てられるためです。
- 開始シーンから戻ると物語は終わります。そのときの戻り値が**エンディング**です。
- コンパイラは、開始シーンごとにそこを起点として物語を解析します。開始シーンにはジェネリックパラメータを付けないでください。ジェネリックな `#[entry]` シーンは起点として扱われません。

```kix
pub enum Ending { NormalEnd, AzusaEnd }

#[scene]
#[entry]
pub pushdown fn Main(cleared_azusa: bool) -> Ending {
    s!(t!["Morning."]);
    if cleared_azusa { Ending::AzusaEnd } else { Ending::NormalEnd }
}
```

詳しくは[開始シーンとエンディング](../concepts/entry.md)を参照してください。

## `#[must_use]` {#must_use}

`struct` や `enum` に `#[must_use]` を付けると、その型の値は「まだ終わっていない操作」を表すことになります。他のアイテムに付けるとエラー `E3018` になります。次の場合、コンパイラは警告 `W3023` を出します。

- 式文の結果がその型の値、またはその値への参照になっている場合
- `let` でその値をローカル変数に束縛したのに、関数内でその変数に対して型のメソッドを一度も呼んでいない場合

標準ライブラリでは、保留中の変更を表す `Change<T>`、`Swap`、`ChannelChange` にこの属性が付いています。画面は `apply_animation` を通さないと変わらないので、`apply_animation` の書き忘れをコンパイル時に見つけられます。

```kis
const fn roof(mut screen: Screen) -> Screen {
    screen.background().clear();                                // W3023: nothing happens
    screen.background().clear().apply_animation(fade(ms(500))); // completed
    screen
}
```

ゲーム側で定義した型に付けることもできます。その場合の警告には、値を完了させる手段としてその型のメソッドが並びます。

## `#[shader_arg]` {#shader_arg}

`struct` に `#[shader_arg]` を付けると、シェーダーの引数型になります。型が `ShaderArg` を実装するため、`ColorShader<Memory>` のようなシェーダーアセットの引数に使え、エンジンが値をシェーダーに渡します。他のアイテムに付けるとエラー `E3018` になります。

```kis
use std::screen::shader::ColorShader;

#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
```

- 名前付きフィールドが 1 つ以上あり、型パラメータがない構造体である必要があります。const パラメータは使えます。条件を満たさないとエラー `E3025` になります。
- 各フィールドの型は、シェーダーが読めるものに限ります。使えるのは `bool`、`i32`、`u32`、`Decimal`、`Rat`、std の `Color`、`Point`、`Offset`、`Size`、`Anchor`、`Angle`、`Duration`、フィールドのない列挙型、`#[shader_arg]` 構造体、およびこれらの配列 `[T; N]`（`N` > 0）です。それ以外の型や長さ 0 の配列を使うと、そのフィールドでエラー `E3026` になります。

各フィールドがシェーダーにどう渡るかは、標準ライブラリのリファレンスにある[シェーダーの書き方](../../std/guides/shaders.md#arguments)で説明しています。

## `#[builtin]` {#builtin}

`#[builtin]` は、コンパイラとエンジンが実装するアイテムに付ける属性です。標準ライブラリの中でしか使えません（`E1008`）。`std` の宣言を読むと目にします。

```kis
#[builtin] pub struct Screen;
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
#[builtin] pub trait Eq {}
```

| 対象 | 規則 |
| ---- | ---- |
| 関数 | 本体を書きません（`E1009`）。呼び出し方は普通の関数と同じで、種別の規則も同じように適用されます |
| 構造体 | フィールドを書きません（`E1009`）。中身の見えない不透明な型になります。[不透明な組み込み型](./builtin-types.md#opaque-builtin-types)を参照してください |
| トレイト | `std` のトレイトにはすべて `#[builtin]` が必要です（`E1013`）。[言語トレイト](./traits.md)を参照してください |

`#[builtin]` のない関数で本体を省くとエラー `E1010` になります。

## 診断 {#diagnostics}

| コード | 発生する場面 |
| ------ | ------------ |
| [`E0001`](../../diagnostics/syntax.md#e0001) | 内部属性 `#![…]`、または後にアイテムが続かない属性 |
| [`E1006`](../../diagnostics/modules.md#e1006) | 未知の属性 |
| [`E1007`](../../diagnostics/modules.md#e1007) | 引数付きの属性 |
| [`E1008`](../../diagnostics/modules.md#e1008) | 標準ライブラリ以外での `#[builtin]` |
| [`E1009`](../../diagnostics/modules.md#e1009) | 本体のある `#[builtin]` 関数、またはフィールドのある `#[builtin]` 構造体 |
| [`E1013`](../../diagnostics/modules.md#e1013) | `#[builtin]` のない標準ライブラリのトレイト |
| [`E3018`](../../diagnostics/kinds.md#e3018) | `pushdown fn` 以外への `#[scene]`、`#[scene]` のない `#[entry]`、構造体・列挙型以外への `#[must_use]`、構造体以外への `#[shader_arg]` |
| [`E3008`](../../diagnostics/kinds.md#e3008)、[`E3009`](../../diagnostics/kinds.md#e3009) | 開始シーンの引数型や戻り値型が有限でない、または `Text` を含む |
| [`E2027`](../../diagnostics/types.md#e2027) | `#[entry]` シーン以外を `nav::start` に渡した |
| [`W3023`](../../diagnostics/kinds.md#w3023) | `#[must_use]` の値を捨てた、または完了させていない |
| [`E3025`](../../diagnostics/kinds.md#e3025) | 名前付きフィールドがない、または型パラメータがある `#[shader_arg]` 構造体 |
| [`E3026`](../../diagnostics/kinds.md#e3026) | シェーダーが読めない型の `#[shader_arg]` フィールド、または長さ 0 の配列 |
