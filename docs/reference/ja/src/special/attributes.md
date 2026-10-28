# 属性 {#attributes}

属性はアイテムの直前に `#[name]` と書きます。Kagari の属性はちょうど 4 つで、どれも引数を取りません。ゲームが使うのはそのうち 3 つで、`#[builtin]` は標準ライブラリ専用です。

| 属性 | 付けられる場所 | 意味 | 誤用 |
| ---- | -------------- | ---- | ---- |
| [`#[scene]`](#scene) | `pushdown fn` | シーン：ステップの実行、シーンの呼び出し、乱数の取得ができる | `E3018` |
| [`#[entry]`](#entry) | `#[scene]` の関数 | エントリーシーン：システムがここから物語を開始できる | `E3018`、`E3008`、`E3009` |
| [`#[must_use]`](#must_use) | `struct`、`enum` | その型の値は完了させなければならず、捨ててはいけない | `E3018`。使用箇所で警告 `W3023` |
| [`#[builtin]`](#builtin) | 標準ライブラリのみ：関数、構造体、トレイト | コンパイラとエンジンが実装する | `E1008`、`E1009`、`E1013` |

## 構文 {#syntax}

```kix
/// The first scene of the game.
#[scene]
#[entry]
pub pushdown fn Main() {
    s!(t!["It was the last day of summer."]);
}
```

- 属性はアイテムの前に置きます。対象は関数（`impl` の中の関連関数を含む）、`struct`、`enum`、`const`、`static`、`type`、`mod`、`use`、`impl`、`trait` です。アイテムのドキュメントコメントの後、`pub` の前に書きます。
- 複数の属性を、1 行にでも別々の行にでも、任意の順序で重ねられます。
- `///` のドキュメントコメントはコメントであり、属性ではありません。
- 内部属性（`#![…]`）は存在せず、属性の後には必ずアイテムが続かなければなりません。どちらも構文エラーです（`E0001`）。文や式に属性は付けられません。
- 4 つ以外の名前は `E1006` で、その属性は無視されます。
- `#[must_use("reason")]` のように引数付きで書いた属性は `E1007` です。属性自体は適用されます。

*暫定。* パーサーは構造体のフィールド、タプルのフィールド、列挙型のバリアントに付いた属性も受け付けますが、現在のコンパイラはそこでは属性を無視します。未知の名前も同様です。この挙動に頼らないでください。

## `#[scene]` {#scene}

`#[scene]` は `pushdown fn` を**シーン**にします。シーンだけが次のことをできます。

- ステップの実行：`s!(…)`、`choice!{…}`、`push_step` の呼び出し（それ以外では `E3002`）
- 他のシーン（自分自身を含む）の呼び出し（それ以外では `E3003`）
- `rng_choose` による乱数の取得（それ以外では `E3024`）

`#[scene]` のない `pushdown fn` は**即時手続き**です。シーンの中のすべてのループと呼び出しのすべての循環は、ステップを通らなければなりません（`E5001`、`E5002`）。シーンへのポインタの型は `scene fn(…) -> R` です。

```kix
#[scene]
pushdown fn Greeting() {
    s!(who = "Mio", t!["Hello."]);
}
```

`pushdown fn` 以外への `#[scene]`、たとえば `fn` や `const fn` への `#[scene]` は `E3018` です。

[シーンと即時手続き](../concepts/scenes.md)を参照してください。

## `#[entry]` {#entry}

`#[entry]` はシーンを**エントリーシーン**、つまりゲームシステムが物語を開始できる場所にします。同じ関数に `#[scene]` が必要で、なければ `#[entry]` は `E3018` です。二つの属性の順序は問いません。

- システムはエントリーシーンを `nav::start(Scene, args, on_exit)` で開始します。第 1 引数は `#[entry]` シーンのパスでなければならず、他の式は書けません（`E2027`）。`args` はその引数のタプルで、`on_exit` が戻り値を受け取ります。
- 他のシーンと同じく、引数型と戻り値型は有限型でなければなりません（`E3008`）。さらに、`fn` のコードは実行時にテキストを作れるので、これらに `Text` を含めることはできません（`E3009`）。
- エントリーシーンが戻ると物語は終わります。その戻り値が**エンディング**です。
- コンパイラは各エントリーシーンから物語を解析します。エントリーシーンはジェネリックパラメータなしで書いてください。ジェネリックな `#[entry]` シーンは開始点として使われません。

```kix
pub enum Ending { NormalEnd, AzusaEnd }

#[scene]
#[entry]
pub pushdown fn Main(cleared_azusa: bool) -> Ending {
    s!(t!["Morning."]);
    if cleared_azusa { Ending::AzusaEnd } else { Ending::NormalEnd }
}
```

[エントリーシーンとエンディング](../concepts/entry.md)を参照してください。

## `#[must_use]` {#must_use}

`struct` または `enum` に付けた `#[must_use]` は、その型の値が未完了の操作であることを表します。それ以外のアイテムに付けると `E3018` です。コンパイラは次の場合に警告します（`W3023`）。

- 式文がその型の値、またはその型への参照を生む場合
- `let` がそのような値をローカル変数に束縛し、関数がそのローカル変数に対してその型のメソッドを一度も呼ばない場合

標準ライブラリは保留中の変更 `Change<T>`、`Swap`、`ChannelChange` にこの属性を付けています。画面の変更は `apply_animation` を通してのみ起きるので、`apply_animation` の書き忘れはコンパイル時に見つかります。

```kis
const fn roof(mut screen: Screen) -> Screen {
    screen.background().clear();                                // W3023: nothing happens
    screen.background().clear().apply_animation(fade(ms(500))); // completed
    screen
}
```

ゲームが自分の型に付けることもできます。その場合、警告は値を完了させる手段としてその型のメソッドを列挙します。

## `#[builtin]` {#builtin}

`#[builtin]` は、コンパイラとエンジンが実装するアイテムを表します。標準ライブラリでのみ使えます（`E1008`）。`std` の宣言を読むときに目にします。

```kis
#[builtin] pub struct Screen;
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
#[builtin] pub trait Eq {}
```

| 対象 | 規則 |
| ---- | ---- |
| 関数 | 本体を持ちません（`E1009`）。他の関数と同じように呼び出され、種別の規則も他の関数と同じく適用されます |
| 構造体 | フィールドを持ちません（`E1009`）。型は不透明です。[不透明なビルトイン型](./builtin-types.md#opaque-builtin-types)を参照してください |
| トレイト | `std` のトレイトはすべて `#[builtin]` でなければなりません（`E1013`）。[言語トレイト](./traits.md)を参照してください |

`#[builtin]` でない本体のない関数は `E1010` です。

## 診断 {#diagnostics}

| コード | 条件 |
| ------ | ---- |
| [`E0001`](../diagnostics/syntax.md#e0001) | 内部属性 `#![…]`、またはアイテムが続かない属性 |
| [`E1006`](../diagnostics/modules.md#e1006) | 未知の属性 |
| [`E1007`](../diagnostics/modules.md#e1007) | 引数付きの属性 |
| [`E1008`](../diagnostics/modules.md#e1008) | 標準ライブラリの外の `#[builtin]` |
| [`E1009`](../diagnostics/modules.md#e1009) | 本体を持つ `#[builtin]` 関数、またはフィールドを持つ `#[builtin]` 構造体 |
| [`E1013`](../diagnostics/modules.md#e1013) | `#[builtin]` のない標準ライブラリのトレイト |
| [`E3018`](../diagnostics/kinds.md#e3018) | `pushdown fn` 以外への `#[scene]`、`#[scene]` のない `#[entry]`、構造体・列挙型以外への `#[must_use]` |
| [`E3008`](../diagnostics/kinds.md#e3008)、[`E3009`](../diagnostics/kinds.md#e3009) | エントリーシーンの引数型または戻り値型が有限でない、または `Text` を含む |
| [`E2027`](../diagnostics/types.md#e2027) | `#[entry]` シーンでないものに対する `nav::start` |
| [`W3023`](../diagnostics/kinds.md#w3023) | 捨てられた、または完了されなかった `#[must_use]` の値 |
