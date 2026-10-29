---
title: "アイテム"
---

**アイテム**はモジュールに属する宣言です。関数、型、定数、static、実装、モジュール、インポートがあります。アイテムはファイルの最上位、インラインモジュール、ブロック（[ブロック内のアイテム](#items-in-blocks)を参照）に置けます。関数と定数は `impl` の中にも置けます。

## 概要 {#overview}

| アイテム | 形 | 参照 |
| -------- | -- | ---- |
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

アイテムの一般的な形は次のとおりです。

```text
Item       = Attribute* "pub"? ItemBody
Attribute  = "#" "[" Path "]"
```

アイテムは宣言より前で使えます。ファイル内のアイテムの順序に意味はありません。

## 構造体 {#structs}

```kis
struct Point { x: i32, y: i32 }     // record struct
struct Affection(Int<0, 5>);        // tuple struct
struct Marker;                      // unit struct
struct Pair<T> { a: T, b: T }       // generic struct
```

- **レコード構造体**は名前付きのフィールドを持ち、`Point { x: 1, y: 2 }` で作ります。
- **タプル構造体**は番号付きのフィールドを持ち、`.0`、`.1` … でアクセスします。その名前はコンストラクタ関数でもあり、`Affection(3)` のように使います。コンストラクタは `const fn` で、`const fn(…) -> …` 型の関数ポインタの値として使えます。
- **ユニット構造体**はフィールドを持ちません。その名前が唯一の値です。
- 構造体は型や定数についてジェネリックにできます。[ジェネリクス](./types.md#generics)を参照してください。`where` 節やジェネリクス引数の既定値はありません。
- フィールドには `pub` や属性を付けられます。コンパイラは現在フィールドの可視性を検査しません。[可視性](./modules.md#visibility)を参照してください。
- `derive` はありません。等価性は自動です。[等価性](./types.md#equality)を参照してください。

## 列挙型 {#enums}

```kis
enum Route { None, Hanako, Azusa }

enum Ending {
    Normal,
    Good(Int<0, 3>),
    Bad { reason: Route },
}
```

- バリアントは、構造体の 3 つの形と同じく、ユニットバリアント、タプルバリアント、レコードバリアントのいずれかです。
- バリアントはインポートしない限り `Route::Hanako` のように列挙型を通して指定します。`use Route::*;` ですべてのバリアントをインポートできます。`Option` と `Result` のバリアントは[プレリュード](./modules.md#the-prelude)にあります。
- タプルバリアントの名前は、タプル構造体の名前と同じくコンストラクタ関数です。`Ending::Good(2)` のように使います。
- 明示的な判別値（`A = 1`）は構文エラーです。列挙型は数値を持ちません。数値への変換には `match` を使います。
- どのバリアントもデータを持たない列挙型は**フィールドなし列挙型**です。フィールドなし列挙型は `RandChoose` を実装し、`choice!` のアームの値になります。

## 定数 {#constants}

```kis
const HANAKO: EntityId = EntityId(216);
const MAX_AFFECTION: Int<0, 100> = 5 * 20;
const ROOF: ImageAsset = asset("bg/roof.png");
```

- 型は必須です。
- 初期化子は[定数式](./constants.md)でなければなりません（[`E3010`](../../diagnostics/kinds.md#e3010)）。値はコンパイラが計算します。値のない定数は [`E1011`](../../diagnostics/modules.md#e1011) です。
- 型に参照を含めることはできません（[`E3012`](../../diagnostics/kinds.md#e3012)）。
- 定数は、その型にかかわらず物語の中を含めどこでも使えます。値がコンパイル時にわかっているからです。
- 定数は `impl` の中（**関連定数**。`Point::ORIGIN` や `Self::ORIGIN` として使います）やブロックの中にも置けます。
- 定数の初期化子は `const` のコードとして検査され、`const fn` だけを呼べます。

## static {#statics}

`static` は、ゲームの実行全体を通じて 1 つの場所を持つ値を宣言します。用途はちょうど 2 つで、コンパイラは `mut` の有無で区別します。

### 物語変数 {#story-variables}

```kis
static mut ROUTE: Route = Route::None;
static mut AFFECTION_MIO: Int<0, 5> = 0;
```

`static mut` は**物語変数**です。物語の状態であり、物語とともにセーブ・復元されます。

- 型は[有限型](../concepts/kinds.md#finite-types)でなければならず（[`E3008`](../../diagnostics/kinds.md#e3008)）、参照を含められません（[`E3012`](../../diagnostics/kinds.md#e3012)）。
- 初期化子は定数式です（[`E3010`](../../diagnostics/kinds.md#e3010)）。
- 読み書きできるのは `pushdown fn` のコードだけです（[`E3007`](../../diagnostics/kinds.md#e3007)）。

意味については[物語変数](../concepts/story.md#story-variables)を参照してください。

### ハンドル {#handles}

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

不変の `static` は**ハンドル**で、`Setting<T>` か `Meta<T>` を保持します（[`E3011`](../../diagnostics/kinds.md#e3011)）。初期化子は定数を引数とする `setting(…)` か `meta(…)` の呼び出しです。これらの呼び出しは `static` の初期化子でしか使えません（[`E4007`](../../diagnostics/eval.md#e4007)）。普通の定数には `const` を使います。設定とメタは[ゲームシステム](../concepts/system.md)で説明します。

## 型エイリアス {#type-aliases}

```kis
type Pair<T> = (T, T);
type Board = Square<8>;
```

型エイリアスは型に別の名前を付けます。新しい型は作りません。ジェネリックにできます。型のないエイリアスは [`E1011`](../../diagnostics/modules.md#e1011)、自分自身に展開されるエイリアスは [`E2010`](../../diagnostics/types.md#e2010) です。構造体や列挙型のエイリアスを通して関連アイテムを指定でき、`impl` の対象にもできます。`Board::new()` のように使います。

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

**固有実装** `impl Type { … }` は、構造体や列挙型に関連関数と関連定数を追加します。`self` パラメータを持つ関数は**メソッド**です。[メソッドと関連関数](./functions.md#methods-and-associated-functions)を参照してください。

- 対象は構造体か列挙型で、ジェネリクス引数を付けて書きます。`impl` は使うジェネリクスパラメータを宣言します（`impl<T> Pair<T> { … }`）。標準ライブラリの構造体や列挙型も対象にできます。プリミティブ型は拡張できません（[`E1022`](../../diagnostics/modules.md#e1022)）。
- `impl` の中では `Self` が対象の型です。
- 関連アイテムは `pub` が付いていなければプライベートです。1 つの型で同じ名前の関連アイテムが 2 つあると、別の `impl` ブロックにあっても [`E1014`](../../diagnostics/modules.md#e1014) です。
- 1 つの型に、ゲームのどのモジュールからでも複数の `impl` ブロックを書けます。
- `impl Trait for Type` は標準ライブラリ専用です（[`E1023`](../../diagnostics/modules.md#e1023)）。

## トレイト {#traits}

ユーザーのコードはトレイトを宣言せず（[`E1012`](../../diagnostics/modules.md#e1012)）、実装もしません（[`E1023`](../../diagnostics/modules.md#e1023)）。標準ライブラリが固定のトレイト集合を宣言し、コードはそれをジェネリクスパラメータの境界として使います（[トレイト境界](./types.md#trait-bounds)を参照）。各トレイトは[言語トレイト](../special/traits.md)で説明します。

## モジュールとインポート {#modules-and-imports}

`mod` はモジュールを宣言し、`use` は名前をインポートします。どちらも[ソースファイルとモジュール](./modules.md)で説明します。

## ブロック内のアイテム {#items-in-blocks}

ブロックには文と並べてアイテムを置けます。そのアイテムは宣言より前を含むブロック全体と、入れ子のブロックから見えます。ブロックの外からは見えません。

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

ブロック内のアイテムは普通のアイテムです。入れ子の関数は外側の関数のローカル変数をキャプチャせず、入れ子の `const` はコンパイル時に計算されます。ファイルモジュール（`mod name;`）はブロックの中では宣言できません（[`E1005`](../../diagnostics/modules.md#e1005)）が、インラインモジュールは宣言できます。

## 属性 {#attributes}

属性はアイテムの前、`pub` より前に `#[name]` と書きます。コンパイラが知っている属性は 4 つです。

| 属性 | 対象 |
| ---- | ---- |
| `#[scene]` | `pushdown fn` |
| `#[entry]` | `#[scene]` の関数 |
| `#[must_use]` | 構造体または列挙型 |
| `#[builtin]` | 標準ライブラリの関数、構造体、トレイト |

それ以外の属性は [`E1006`](../../diagnostics/modules.md#e1006)、引数付きの属性（`#[scene(x)]`）は [`E1007`](../../diagnostics/modules.md#e1007)、誤ったアイテムへの属性は [`E3018`](../../diagnostics/kinds.md#e3018) です。内部属性 `#![…]` は構文エラーです。各属性の意味は[属性](../special/attributes.md)で説明します。
