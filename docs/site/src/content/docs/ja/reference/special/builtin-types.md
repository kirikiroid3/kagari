---
title: "組み込み型"
---

ゲームで使う型の多くは `std` にある普通の宣言で、ゲーム側の型と同じく Kagari で書かれています。ただし一部の型は、コンパイラが名前で特別扱いします。専用の構文や検査があったり、内部表現がまったく見えなかったりする型です。この章ではそうした型を挙げ、それぞれの特別な点を説明します。不透明かどうか、[有限型](../concepts/kinds.md#finite-types)かどうか、`Eq` かどうか、どの[関数種別](../concepts/kinds.md)で使うか、コピーすると何が起きるか、といった点です。メソッドについては[標準ライブラリのリファレンス](../../std/usage.md)を参照してください。

Kagari の型はすべて値セマンティクスに従います。代入、引数渡し、戻り値の返却では値がコピーされます（[値、参照、クロージャ](../syntax/values.md)を参照）。ただし後述のハンドル型でコピーされるのはハンドルです。そのため、コピー元とコピー先は同じ保存先を指します。

## 概要 {#overview}

| 型 | 定義 | 有限 | `Eq` | 主に使う種別 | 標準ライブラリのページ |
| -- | ---- | ---- | ---- | ------------ | ---------------------- |
| `bool`、`()` | 言語 | はい | はい | すべて | — |
| `Int<lo, hi>` | 言語 | はい | はい | すべて | — |
| `u8` … `u128`、`i8` … `i128`、`usize`、`isize` | 言語 | いいえ | はい | `fn`、`const fn` | — |
| `Rat`、`Decimal` | `#[builtin]` 構造体 | いいえ | はい | `fn`、`const fn` | [`std`](../../std/std.md) |
| `String` | `#[builtin]` 構造体 | いいえ | はい | `fn`、`const fn` | [`std`](../../std/std.md) |
| `Text` | `#[builtin]` 構造体 | はい | はい | すべて | [`std::narrative::text`](../../std/narrative-text.md) |
| `ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` | `#[builtin]` 構造体 | はい | はい | すべて | [`std`](../../std/std.md) |
| `ColorShader<A>`、`MixShader<A>`、`TextureShader<A>`、`MaskShader<A>`、`LocalMotionShader<A>`、`TransitionalMotionShader<A>` | `#[builtin]` 構造体 | いいえ | はい | `const fn` | [`std::screen::shader`](../../std/screen-shader.md) |
| `Vec<T>` | `#[builtin]` 構造体 | いいえ | `T: Eq` なら | `fn`、`const fn` | [`std`](../../std/std.md) |
| `Range<T>` | 構造体 | `T` が有限なら | `T: Eq` なら | すべて | [`std`](../../std/std.md) |
| `Option<T>`、`Result<T, E>` | 列挙型 | 型引数が有限なら | 型引数が `Eq` なら | すべて | [`std`](../../std/std.md) |
| `Setting<T>`、`Meta<T>` | `#[builtin]` 構造体 | いいえ | いいえ | `fn`（後述） | [`std::system`](../../std/system.md)、[`std::narrative`](../../std/narrative.md) |
| `Narrative` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::narrative`](../../std/narrative.md) |
| `Screen` | `#[builtin]` 構造体 | いいえ | はい | `const fn` | [`std::screen`](../../std/screen.md) |
| `Entity`、`Prop<T>`、`ImageLayer`、`LabelLayer`、`MovieLayer`、`Hud`、`ChoiceMenu`、`Camera`、`Channel` | `#[builtin]` 構造体 | いいえ | いいえ | `const fn` | [`std::screen`](../../std/screen.md)、[`std::screen::audio`](../../std/screen-audio.md) |
| `Change<T>`、`Swap`、`ChannelChange` | `#[must_use]` `#[builtin]` 構造体 | いいえ | いいえ | `const fn` | [`std::screen`](../../std/screen.md)、[`std::screen::audio`](../../std/screen-audio.md) |
| `Element`、`Placed` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::ui`](../../std/ui.md) |
| `Arm`、`Divergence` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::narrative`](../../std/narrative.md)、[`std::system::saves`](../../std/system-saves.md) |
| `Render` | `const fn(Screen) -> Screen` の型エイリアス | はい | はい | すべて | [`std::screen`](../../std/screen.md) |
| `Duration` | 構造体 | いいえ | はい | すべて（物語では定数として） | [`std::time`](../../std/time.md) |
| 関数ポインタ | 言語 | `fn` 以外の種別なら | はい | すべて | — |
| クロージャ `Fn(…)` | 言語 | いいえ | いいえ | すべて（物語では第二級） | — |

「有限」かどうかで、[`pushdown fn`](../concepts/kinds.md#pushdown-fn) がその型の値を実行時に保持できるかが決まります（`E3008`）。ただし定数式なら、型を問わず物語の中に書けます。コンパイル時に畳み込まれるためです。

## 不透明な組み込み型 {#opaque-builtin-types}

[`#[builtin]`](./attributes.md#builtin) を付けて宣言した構造体は、コンパイラとエンジンが実装します。外から見えるフィールドはありません。

- 構造体リテラルで作ることも、フィールドを読むことも、パターンで分解することもできません。値を得る手段は、`std` の関数やメソッド、リテラル（`Text`、`Decimal`）、エンジンから渡される値（`Narrative`）に限られます。
- `Eq` になるのは、`std` に `impl Eq` が書かれている場合だけです。構造的な `Eq` の判定は、こうした型の中身までは見ません（[言語トレイト](./traits.md#eq)を参照）。
- 有限型になるのは `Text` とアセット型だけで、それ以外の組み込み型はすべて無限の型として扱います。
- 内部表現は `.kagari` ファイルに含まれません。そのため、エンジンのバージョンが上がって表現が変わっても、ゲームは壊れません。

組み込み型を宣言できるのは `std` だけです（`E1008`）。ほとんどはプレリュードに入っていますが、画面の状態を表す型（`Entity`、`Prop<T>`、各レイヤーなど）は入っていません。これらは `Screen` のメソッドから `&mut` 参照として受け取るもので、型名を書く機会がほとんどないためです。

### `Text` {#text}

`Text` は定数のテキストです。`"Mio"` のような装飾なしのものと、[`t![…]`](../syntax/forms.md) で組み立てるルビ・テキスト効果・埋め込み付きのものがあります。文字列リテラルの型は `String` ではなく `Text` です。

```kis
const NAME: Text = "Mio";
```

- **有限。** 物語の中に現れる `Text` の値は、プログラム中の定数テキストに限られます。そのため、物語変数や `pushdown fn` の引数・戻り値に `Text` を使えます。
- **開始シーンには使えない。** `fn` のコードは `to_text` で実行時に新しいテキストを作れます。そのため、[開始シーン](../concepts/entry.md)の引数や戻り値の型には `Text` を含められません（`E3009`）。
- **埋め込み。** `t![…]` 内の `{expr}` は `to_text(expr)` の呼び出しと同じで、`expr: T` が [`T: ToText`](./traits.md#totext) を満たす必要があります。`pushdown fn` の中で有限の実行時の値に `to_text` を使うと、他の `const fn` 呼び出しと同じく表化されます。そのため台詞は、物語の状態から決まる有限の関数のままです。
- `Eq`：はい。

### `String` {#string}

`String` は `fn` のコードで使う実行時の文字列です。有限型ではなく、`Eq` と `ToText` を実装しています。

*暫定。* 現在の `std` には、`String` を作る関数も変更する関数もありません。ゲームで表示するテキストには `Text` を使います。

### アセット型 {#asset-types}

`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` は、ゲームに同梱するファイルを指す型です。6 つのシェーダー型（`ColorShader<A>` など）も同じくファイルを指し、こちらは特定の役割を持ち、引数の型が `A` である WGSL ファイル（`.wgsl`）を表します。値を作れるのは、`asset` または `system_font` を定数として呼び出したときだけです。

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const THEME: AudioAsset = asset("bgm/theme.ogg");
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

- `asset::<A>` の型引数は定数の型から推論されます。この型は [`Asset`](./traits.md#asset) を実装している必要があります。
- 引数には次の条件があります。文字列リテラルであること（`E2028`）、空でないこと（`E1028`）、アセットディレクトリ内を指すこと（`E1029`）、その直下の予約ディレクトリ `system-fonts` と `std` の外を指すこと（`E1032`）。ファイルが存在しないとエラー `E1027` になります。`system_font` には別の規則があります（`E1030`、`E1031`）。標準ライブラリのリファレンスの [`std`](../../std/std.md) を参照してください。
- **有限。** ファイルを指すアセット型の値は、プログラム中のアセット定数に限られます。そのため物語の中でも保持でき、アセット名を書き間違えても実行時の失敗ではなく、名前解決のエラーとして見つかります。一方、シェーダー型は有限型ではありません。シェーダーを使うのはレンダーで、物語ではないためです。
- `Eq`：はい。
- 効果値が内部に持つ `ShaderAsset` は、役割を問わずシェーダーファイルそのものを識別する型です。`Asset` を実装しておらず、プレリュードにもありません。ゲームのコードで型名を書くことはありません。[`std::screen::shader`](../../std/screen-shader.md) を参照してください。

### `Setting<T>` と `Meta<T>` {#setting-and-meta}

設定とメタデータは、セーブとは別に保存される永続的なグローバル値です。どちらも、**ハンドル**を保持する不変の `static` として宣言します。

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- 不変の `static` に保持できるのは `Setting<T>` か `Meta<T>` だけです（`E3011`）。また `setting(…)` と `meta(…)` は、`static` の初期化子としてしか呼べません（`E4007`）。初期化子に書いた値は、保存ファイルに値がないときに使われます。
- ハンドルそのものも値です。関数に渡したり、`Settings` のようなレコードに格納したりできます。コピーされるのはハンドルなので、コピー元とコピー先は同じ保存値を読み書きします。
- **種別。** `get` と `set` は `fn` です。`Meta::write` は `pushdown fn` で、物語はメタデータに書き込むだけで、読むことはありません。`const fn` では、`Entity::gate` に渡すときのように `Setting` のハンドルを値として扱うことしかできず、`get` を呼ぶとエラー `E3001` になります。`const` のコードでは、メタデータにまったく触れられません（`E3013`）。
- 有限型ではなく、`Eq` でもありません。

[ゲームシステム](../concepts/system.md)と[レンダーの中の設定](../concepts/presentation.md#settings-in-renders)を参照してください。

### `Narrative` {#narrative}

`Narrative` は、システムが実行中の物語に触れる唯一の手段です。エンジンがこの値を渡すのは、ゲーム画面（`System::game`、`fn(Narrative) -> Element`）とゲームオーバーレイ（`fn(Narrative) -> GameOverlay`）だけです。内部表現は隠されているので、システムのコードから物語変数は読めません。メソッドはすべて `fn` で、値の読み取りはリアクティブです。コピーしても、指すのは同じ実行中のゲームです。有限型ではなく、`Eq` でもありません。

### `Screen` とその部品 {#screen}

`Screen` は、ステップが表示する画面と、そのステップのタイムラインをまとめたものです。どの[レンダー](#render)も `Screen` を受け取って `Screen` を返します。メソッドはすべて `const fn` です。

- **値。** レンダーは直前の画面のコピーを受け取り、新しい画面を返します。コピーを変更しても、呼び出し元の画面は変わりません。
- **部品。** `Entity`、`Prop<T>`、`ImageLayer`、`LabelLayer`、`MovieLayer`、`Hud`、`ChoiceMenu`、`Camera`、`Channel` は画面を構成する部品です。`screen.entity(id)` や `screen.music()` などのメソッドから `&mut` 参照として受け取り、その参照を通して変更するとその画面が変わります。
- **保留中の変更。** `Change<T>`、`Swap`、`ChannelChange` は、`change`、`clear`、`stop` が返す保留中の変更です。[`#[must_use]`](./attributes.md#must_use) が付いており、`apply_animation` を呼ぶまでは何も起きません。保留中の変更を捨てると警告 `W3023` になります。
- `Screen` は `Eq` なので、コンパイラは[一貫性](../concepts/presentation.md#consistency)を確かめるために画面どうしを比較できます。部品は `Eq` ではありません。いずれも有限型ではありません。

### `Element` と `Placed` {#element}

`Element` は、エンジンがレイアウトする UI 要素のツリーです。`Placed` は、`layer![…]` の中で位置を決めた要素です。どちらも UI 構文と `std::ui` の関数で作り、それらはすべて `fn` です。有限型ではなく、`Eq` でもありません。

### `Arm` と `Divergence` {#arm-and-divergence}

`Arm` は、現在の選択肢で選べるアームを表します。`Narrative::menu` で取得し、`Narrative::select` に渡して選びます。`Divergence` は、セーブデータが現在のプログラムと合わなくなったときに `nav::load` が返すエラーです。どちらも `fn` のコードにしか現れません。有限型ではなく、`Eq` でもありません。

## 数値 {#numbers}

| 型 | 定義 | 有限 | 備考 |
| -- | ---- | ---- | ---- |
| `Int<lo, hi>` | 言語のプリミティブ型 | はい | `lo..=hi` の範囲の整数。`lo` と `hi` は `i64` の定数で、`lo <= hi` を満たす必要がある（`E2004`）。物語が実行時に保持できる唯一の整数型 |
| `u8` … `u128`、`i8` … `i128`、`usize`、`isize` | 言語のプリミティブ型 | いいえ | Rust と同じ整数型。演算子も Rust と同じ |
| `Decimal` | `std` の `#[builtin]` 構造体 | いいえ | 誤差のない十進数。小数点付きのリテラル（`0.5`）は `Decimal` になる。浮動小数点数の代わりに使う |
| `Rat` | `std` の `#[builtin]` 構造体 | いいえ | 誤差のない有理数。リテラルはなく、`Rat::new(num, den)` と書く |

どれも `Eq` を実装しており、整数、`Rat`、`Decimal` は大小比較（`<`、`<=` など）もできます。異なる整数型を混ぜて計算することはできず（`E2014`）、変換は [`Integer`](./traits.md#integer) のメソッド `into` と `saturate` でしか行えません。`as` によるキャストはありません（`E2019`）。算術、リテラルの型の決まり方、飽和、オーバーフローについては[数値](../syntax/numbers.md)で説明しています。

## `Vec<T>` {#vec}

`Vec<T>` は、`fn` と `const fn` のコードで使う可変長のリストです。他の型と同じく値として振る舞います。`push` や `set` で変わるのは呼び出した対象だけで、コピーには影響しません。

- `v[i]` で要素にアクセスでき、添字には任意の整数型を使えます。`for x in v` で要素を順に処理できます。どちらも `Vec<T>` と配列のために言語に組み込まれた機能です。
- 範囲外を `get` すると `None` が返ります。範囲外を `set` すると評価エラーになります（コンパイル時評価では `E4005`）。
- 有限型ではありません。`T: Eq` なら `Eq` です。

## `Range<T>` {#range}

`Range<T>` は `std` の普通の構造体で、公開フィールド `start`、`end`、`inclusive` を持ちます。コンパイラがこの型を特別扱いするのは次の二か所です。

- **範囲式。** `a..b` と `a..=b` は `Range<T>` を作ります。`T` は両端の値をどちらも変換できる型です。たとえば `0..7` は、使われ方から別の整数型が求められない限り `Range<i32>` になります。`last: Int<0, 7>` のとき、`0..last` は `Range<Int<0, 7>>` です。範囲の値として書けるのは `a..b` と `a..=b` だけで、`a..`、`..b`、`..` などはエラー `E2030` になります。パターンにはパターン用の範囲の書き方（`a..`、`..=b`、`a..b`、`a..=b`）があります。[範囲パターン](../syntax/patterns.md#range-patterns)を参照してください。
- **`for` ループ。** `for` で反復できるのは、`T` が整数型の `Range<T>` です（それ以外は `E2031`）。`pushdown fn` の中では、両端が定数式でなければなりません（`E3006`）。

## `Option<T>` と `Result<T, E>` {#option-and-result}

どちらも `std` の普通のジェネリックな列挙型で、バリアント `None`、`Some`、`Ok`、`Err` はプレリュードでインポートされています。コンパイラがこれらを特別扱いするのは `?` 演算子のためです。`Option` に `?` を使うには `Option` を返す関数の中である必要があり、`Result` に使うにはエラー型が同じ `Result` を返す関数の中である必要があります（`E2034`）。エラーの型変換は行いません。型引数が有限なら有限型で、型引数が `Eq` なら `Eq` です。

## 関数ポインタ型とクロージャ型 {#function-types}

関数の型には、呼び出す先のコードの[種別](../concepts/kinds.md)が含まれます。そのため、ポインタやクロージャを経由しても呼び出しの規則が保たれます。

| 型 | 入る値 | 有限 | 対応するクロージャ型 |
| -- | ------ | ---- | -------------------- |
| `fn(A) -> B` | 名前付きの `fn` | いいえ | `Fn(A) -> B` |
| `const fn(A) -> B` | 名前付きの `const fn`、または何もキャプチャしないクロージャ | はい | `const Fn(A) -> B` |
| `pushdown fn(A) -> B` | 名前付きの即時手続き | はい | `pushdown Fn(A) -> B` |
| `scene fn(A) -> B` | 名前付きのシーン | はい | なし |
| `Fn(A) -> B`、`const Fn(A) -> B`、`pushdown Fn(A) -> B` | その種別のクロージャまたは関数 | いいえ | — |

- 名前付きの関数は、種別がちょうど一致するポインタ型やクロージャ型に型強制できます。何もキャプチャしないクロージャは同じ種別のポインタに型強制できますが、キャプチャするクロージャはできません（`E2036`）。ポインタは同じ種別のクロージャ型に型強制できます。
- `pushdown` と `scene` がキーワードになるのは、`fn` の直前（`pushdown` は `Fn` の直前も）に置いたときだけです。
- **有限。** 物語の種別や `const` のポインタは有限型です。入る値は、プログラム中に有限個しかない名前付き関数だからです。クロージャは有限型にはならず、物語の中では[第二級](../syntax/values.md)の値です。
- **`Eq`。** 関数ポインタは、同じ関数を指すかどうかで比較します。クロージャは `Eq` になりません（`E2017`）。

### `Render` {#render}

```kis
pub type Render = const fn(Screen) -> Screen;
```

`Render` は `std::screen` とプレリュードにあるエイリアスで、新しい型ではありません。レンダーとは、ステップ前の画面からステップ後の画面を作る `const fn` です。ステップは `render` 属性でレンダーを受け取ります。`const fn` のポインタなので `Render` は有限型で `Eq` でもあり、`Render` を渡す場所には何もキャプチャしないクロージャをその場で書けます。[レンダー関数](../concepts/presentation.md#render-functions)を参照してください。

```kis
const fn clear_background(mut screen: Screen) -> Screen {
    screen.background().clear().apply_animation(fade(ms(500)));
    screen
}

const CLEAR: Render = clear_background;
```

## `Duration` {#duration}

```kis
pub struct Duration { pub ms: u32 }
```

`Duration` は `std::time` の普通の構造体で、組み込み型ではありません。ここで取り上げるのは、`std` の時間がすべて整数ミリ秒の `Duration` で表されるからです。単位付きの文字列やフレーム数で時間を指定することはありません。プレリュードには `Duration`、`ms(n)`、`secs(n)` が入っています。

- 中身が `u32` なので有限型ではありません。物語の中で時間を使うのは、レンダー内の `fade(ms(500))` のような定数式だけです。
- `Eq`：はい（構造的に判定）。
- `div` は切り捨てです。`ms(1000).div(3)` は `ms(333)` になります。ミリ秒単位で `u32` に収まらない時間になると、コンパイル時評価でエラー `E4013` になります。`secs` で作った場合も、`plus` や `times` で計算した場合も、画面のタイムラインが進んでその長さに達した場合も同じです。
