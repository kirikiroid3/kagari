---
title: "アイテム"
---

**アイテム**とは、モジュールに属する宣言のことです。関数、型、定数、static、実装、モジュール、インポートがあります。アイテムを置けるのは、ファイルの最上位、インラインモジュールの中、ブロックの中（[ブロック内のアイテム](#items-in-blocks)を参照）です。関数と定数は `impl` の中にも置けます。

## アイテムの一覧 {#overview}

| アイテム | 書き方 | 参照先 |
| -------- | ------ | ------ |
| 関数 | `fn f() {}`、`const fn f() {}`、`pushdown fn f() {}` | [関数](./functions.md) |
| 構造体 | `struct Point { x: i32, y: i32 }`、`struct Id(u32);`、`struct Marker;` | [構造体](#structs) |
| 列挙型 | `enum Route { None, Hanako, Azusa }` | [列挙型](#enums) |
| 定数 | `const HANAKO: EntityId = EntityId(216);` | [定数](#constants) |
| 物語変数 | `static mut ROUTE: Route = Route::None;` | [物語変数](#story-variables) |
| ハンドル | `static MUSIC: Setting<Volume> = setting(volume(60));` | [ハンドル](#handles) |
| 型エイリアス | `type Pair<T> = (T, T);` | [型エイリアス](#type-aliases) |
| 実装 | `impl Point { … }` | [実装](#implementations) |
| モジュール | `mod story;`、`mod ui { … }` | [ソースファイルとモジュール](./modules.md#modules) |
| インポート | `use story::Day1;` | [`use` 宣言](./modules.md#use-declarations) |

アイテムはおおむね次の形をしています。

```text
Item       = Attribute* "pub"? ItemBody
Attribute  = "#" "[" Path "]"
```

アイテムは宣言より前の位置でも使えます。ファイル内でアイテムをどの順に並べても意味は変わりません。

## 構造体 {#structs}

```kis
struct Point { x: i32, y: i32 }     // record struct
struct Affection(Int<0, 5>);        // tuple struct
struct Marker;                      // unit struct
struct Pair<T> { a: T, b: T }       // generic struct
```

- **レコード構造体**のフィールドには名前があります。値は `Point { x: 1, y: 2 }` のように作ります。
- **タプル構造体**のフィールドには番号が振られ、`.0`、`.1`、… で取り出します。構造体名はそのままコンストラクター関数としても使え、`Affection(3)` のように書きます。コンストラクターは `const fn` なので、`const fn(…) -> …` 型の関数ポインタの値としても使えます。
- **ユニット構造体**にはフィールドがなく、構造体名そのものが唯一の値です。
- 構造体には型や定数のジェネリクスパラメータを付けられます（[ジェネリクス](./types.md#generics)を参照）。`where` 節や、ジェネリクス引数の既定値はありません。
- フィールドには `pub` や属性を付けられます。ただし、現在のコンパイラはフィールドの可視性を検査しません（[可視性](./modules.md#visibility)を参照）。
- `derive` はありません。等価性は自動で決まります（[等価性](./types.md#equality)を参照）。

## 列挙型 {#enums}

```kis
enum Route { None, Hanako, Azusa }

enum Ending {
    Normal,
    Good(Int<0, 3>),
    Bad { reason: Route },
}
```

- バリアントには、構造体の 3 つの形に対応して、ユニットバリアント、タプルバリアント、レコードバリアントがあります。
- バリアントは、インポートしていなければ `Route::Hanako` のように列挙型名を付けて書きます。`use Route::*;` と書くと、すべてのバリアントをインポートできます。`Option` と `Result` のバリアントは[プレリュード](./modules.md#the-prelude)に含まれています。
- タプルバリアントの名前は、タプル構造体の名前と同じくコンストラクター関数として使えます（`Ending::Good(2)`）。
- 判別値を明示する書き方（`A = 1`）は構文エラーです。列挙型には数値としての値がないので、数値に変換したいときは `match` を使います。
- どのバリアントもデータを持たない列挙型を**フィールドのない列挙型**と呼びます。フィールドのない列挙型は `RandChoose` を実装しており、`choice!` のアームの値として使えます。

## 定数 {#constants}

```kis
const HANAKO: EntityId = EntityId(216);
const MAX_AFFECTION: Int<0, 100> = 5 * 20;
const ROOF: ImageAsset = asset("bg/roof.png");
```

- 型の指定は必須です。
- 初期化子は[定数式](./constants.md)でなければならず（[`E3010`](../../diagnostics/kinds.md#e3010)）、値はコンパイラが計算します。値を書かずに定数を宣言するとエラー [`E1011`](../../diagnostics/modules.md#e1011) になります。
- 型に参照を含めることはできません（[`E3012`](../../diagnostics/kinds.md#e3012)）。
- 定数の値はコンパイル時に決まるので、型にかかわらず、物語の中も含めてどこでも使えます。
- 定数は `impl` の中（**関連定数**。`Point::ORIGIN` や `Self::ORIGIN` のように参照します）やブロックの中にも置けます。
- 定数の初期化子は `const` のコードと同じ規則で検査されるので、呼び出せるのは `const fn` だけです。

## static {#statics}

`static` は、ゲームの実行中ずっと同じ場所に存在する値を宣言します。用途は次の 2 つだけで、コンパイラは `mut` の有無で見分けます。

### 物語変数 {#story-variables}

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

`static mut` で宣言した値は**物語変数**です。物語の状態を表し、物語と一緒にセーブ・復元されます。

- 型は[有限型](../concepts/kinds.md#finite-types)でなければならず（[`E3008`](../../diagnostics/kinds.md#e3008)）、参照を含めることもできません（[`E3012`](../../diagnostics/kinds.md#e3012)）。
- 初期化子は定数式です（[`E3010`](../../diagnostics/kinds.md#e3010)）。
- 読み書きできるのは `pushdown fn` のコードだけです（[`E3007`](../../diagnostics/kinds.md#e3007)）。

物語変数の意味については[物語変数](../concepts/story.md#story-variables)を参照してください。

### ハンドル {#handles}

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

`mut` を付けない `static` は**ハンドル**で、`Setting<T>` か `Meta<T>` を保持します（[`E3011`](../../diagnostics/kinds.md#e3011)）。初期化子には、定数を引数にした `setting(…)` か `meta(…)` の呼び出しを書きます。この 2 つは `static` の初期化子でしか呼び出せません（[`E4007`](../../diagnostics/eval.md#e4007)）。普通の定数には `const` を使ってください。設定とメタデータについては[ゲームシステム](../concepts/system.md)で説明します。

## 型エイリアス {#type-aliases}

```kis
type Pair<T> = (T, T);
type Board = Square<8>;
```

型エイリアスは、既存の型に別名を付けるもので、新しい型は作りません。ジェネリクスパラメータも付けられます。型を書かずにエイリアスを宣言するとエラー [`E1011`](../../diagnostics/modules.md#e1011)、自分自身に展開されるエイリアスはエラー [`E2010`](../../diagnostics/types.md#e2010) になります。構造体や列挙型のエイリアスを使って関連アイテムを参照したり、`impl` の対象にしたりできます（`Board::new()`）。

## 実装 {#implementations}

```kis
struct Square<const N: u64> {
    side: u64,
}

impl<const N: u64> Square<N> {
    const CELLS: u64 = N * N;

    pub const fn new() -> Self {
        Self { side: N }
    }

    pub const fn cells(&self) -> u64 {
        self.side * Self::CELLS
    }
}
```

**固有実装** `impl Type { … }` は、構造体や列挙型に関連関数や関連定数を追加します。`self` パラメータを取る関数は**メソッド**になります（[メソッドと関連関数](./functions.md#methods-and-associated-functions)を参照）。

- 対象には構造体か列挙型を、ジェネリクス引数も含めて書きます。そこで使うジェネリクスパラメータは `impl` 側で宣言します（`impl<T> Pair<T> { … }`）。標準ライブラリの構造体や列挙型も対象にできますが、プリミティブ型は拡張できません（[`E1022`](../../diagnostics/modules.md#e1022)）。
- `impl` の中では、`Self` が対象の型を表します。
- 関連アイテムは、`pub` を付けない限り非公開です。同じ型に同じ名前の関連アイテムが 2 つあると、`impl` ブロックが別々でもエラー [`E1014`](../../diagnostics/modules.md#e1014) になります。
- 1 つの型に対して `impl` ブロックをいくつでも書けます。書く場所はゲーム内のどのモジュールでもかまいません。
- `impl Trait for Type` は標準ライブラリにしか書けません（[`E1023`](../../diagnostics/modules.md#e1023)）。

## トレイト {#traits}

ユーザーのコードでは、トレイトを宣言することも（[`E1012`](../../diagnostics/modules.md#e1012)）、実装することも（[`E1023`](../../diagnostics/modules.md#e1023)）できません。トレイトは標準ライブラリが決まったものだけを宣言しており、コードではそれをジェネリクスパラメータの境界として使います（[トレイト境界](./types.md#trait-bounds)を参照）。各トレイトについては[言語トレイト](../special/traits.md)で説明します。

## モジュールとインポート {#modules-and-imports}

`mod` でモジュールを宣言し、`use` で名前をインポートします。どちらも[ソースファイルとモジュール](./modules.md)で説明します。

## ブロック内のアイテム {#items-in-blocks}

ブロックの中には、文と並べてアイテムを置けます。ブロック内のアイテムは、宣言より前も含めたブロック全体と、その内側のブロックから参照できます。ブロックの外からは参照できません。

```kis
fn uses() -> u64 {
    const STEP: u64 = 2;
    fn helper(x: u64) -> u64 {
        x + STEP
    }
    use crate::widen as w;
    helper(1) + w(3u8)
}

fn widen<T: Integer>(value: T) -> u64 {
    Integer::into(value)
}
```

ブロック内に置いても、アイテムとしての扱いは変わりません。内側の関数が外側の関数のローカル変数をキャプチャすることはなく、内側の `const` もコンパイル時に計算されます。ファイルモジュール（`mod name;`）はブロック内では宣言できません（[`E1005`](../../diagnostics/modules.md#e1005)）が、インラインモジュールなら宣言できます。

## 属性 {#attributes}

属性は、アイテムの前（`pub` よりも前）に `#[name]` の形で書きます。コンパイラが認識する属性は次の 4 つです。

| 属性 | 付けられる対象 |
| ---- | -------------- |
| `#[scene]` | `pushdown fn` |
| `#[entry]` | `#[scene]` を付けた関数 |
| `#[must_use]` | 構造体、列挙型 |
| `#[builtin]` | 標準ライブラリの関数、構造体、トレイト |

これ以外の属性はエラー [`E1006`](../../diagnostics/modules.md#e1006)、引数付きの属性（`#[scene(x)]`）はエラー [`E1007`](../../diagnostics/modules.md#e1007)、付けられないアイテムに付けた属性はエラー [`E3018`](../../diagnostics/kinds.md#e3018) になります。内部属性 `#![…]` は構文エラーです。各属性の意味は[属性](../special/attributes.md)で説明します。
