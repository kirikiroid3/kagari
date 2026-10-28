# ビルトイン型 {#builtin-types}

ゲームが使う型の大半は `std` の普通の宣言で、皆さんの型と同じく Kagari で書かれています。ただし一部の型はコンパイラが名前で知っています。専用の構文や専用の検査があるか、あるいは見える表現をまったく持ちません。この章ではそれらを列挙し、それぞれの何が特別なのかを説明します。不透明かどうか、[有限型](../concepts/kinds.md#finite-types)かどうか、`Eq` かどうか、どの[関数種別](../concepts/kinds.md)で役立つか、コピーが何を意味するか、です。メソッドは [STD Reference](../../../../std/ja/src/usage.md) にあります。

Kagari のすべての型は値セマンティクスを持ちます。代入、引数渡し、戻り値は値をコピーします（[値、参照、クロージャ](../syntax/values.md)を参照）。以下のハンドル型ではコピーされる値がハンドルなので、両方のコピーは同じ保存先を指します。

## 概要 {#overview}

| 型 | 定義 | 有限 | `Eq` | 主な種別 | STD のページ |
| -- | ---- | ---- | ---- | -------- | ------------ |
| `bool`、`()` | 言語 | はい | はい | すべて | — |
| `Int<lo, hi>` | 言語 | はい | はい | すべて | — |
| `u8` … `u128`、`i8` … `i128`、`usize`、`isize` | 言語 | いいえ | はい | `fn`、`const fn` | — |
| `Rat`、`Decimal` | `#[builtin]` 構造体 | いいえ | はい | `fn`、`const fn` | [`std`](../../../../std/ja/src/std.md) |
| `String` | `#[builtin]` 構造体 | いいえ | はい | `fn`、`const fn` | [`std`](../../../../std/ja/src/std.md) |
| `Text` | `#[builtin]` 構造体 | はい | はい | すべて | [`std::narrative::text`](../../../../std/ja/src/narrative-text.md) |
| `ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` | `#[builtin]` 構造体 | はい | はい | すべて | [`std`](../../../../std/ja/src/std.md) |
| `ColorShader<A>`、`MixShader<A>`、`TextureShader<A>`、`MaskShader<A>`、`LocalMotionShader<A>`、`TransitionalMotionShader<A>` | `#[builtin]` 構造体 | いいえ | はい | `const fn` | [`std::screen::shader`](../../../../std/ja/src/screen-shader.md) |
| `Vec<T>` | `#[builtin]` 構造体 | いいえ | `T: Eq` なら | `fn`、`const fn` | [`std`](../../../../std/ja/src/std.md) |
| `Range<T>` | 構造体 | `T` が有限なら | `T: Eq` なら | すべて | [`std`](../../../../std/ja/src/std.md) |
| `Option<T>`、`Result<T, E>` | 列挙型 | 型引数が有限なら | 型引数が `Eq` なら | すべて | [`std`](../../../../std/ja/src/std.md) |
| `Setting<T>`、`Meta<T>` | `#[builtin]` 構造体 | いいえ | いいえ | `fn`（後述） | [`std::system`](../../../../std/ja/src/system.md)、[`std::narrative`](../../../../std/ja/src/narrative.md) |
| `Narrative` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::narrative`](../../../../std/ja/src/narrative.md) |
| `Screen` | `#[builtin]` 構造体 | いいえ | はい | `const fn` | [`std::screen`](../../../../std/ja/src/screen.md) |
| `Entity`、`Prop<T>`、`ImageLayer`、`LabelLayer`、`MovieLayer`、`Hud`、`ChoiceMenu`、`Camera`、`Channel` | `#[builtin]` 構造体 | いいえ | いいえ | `const fn` | [`std::screen`](../../../../std/ja/src/screen.md)、[`std::screen::audio`](../../../../std/ja/src/screen-audio.md) |
| `Change<T>`、`Swap`、`ChannelChange` | `#[must_use]` `#[builtin]` 構造体 | いいえ | いいえ | `const fn` | [`std::screen`](../../../../std/ja/src/screen.md)、[`std::screen::audio`](../../../../std/ja/src/screen-audio.md) |
| `Element`、`Placed` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::ui`](../../../../std/ja/src/ui.md) |
| `Arm`、`Divergence` | `#[builtin]` 構造体 | いいえ | いいえ | `fn` | [`std::narrative`](../../../../std/ja/src/narrative.md)、[`std::system::saves`](../../../../std/ja/src/system-saves.md) |
| `Render` | `const fn(Screen) -> Screen` の型エイリアス | はい | はい | すべて | [`std::screen`](../../../../std/ja/src/screen.md) |
| `Duration` | 構造体 | いいえ | はい | すべて（物語では定数として） | [`std::time`](../../../../std/ja/src/time.md) |
| 関数ポインタ | 言語 | `fn` 以外の種別なら | はい | すべて | — |
| クロージャ `Fn(…)` | 言語 | いいえ | いいえ | すべて（物語では第二級） | — |

「有限」は、[`pushdown fn`](../concepts/kinds.md#pushdown-fn) がその型の値を実行時に保持できるかどうかを決めます（`E3008`）。どの型の定数式も、コンパイル時に畳み込まれるので物語の中に書けます。

## 不透明なビルトイン型 {#opaque-builtin-types}

[`#[builtin]`](./attributes.md#builtin) 付きで宣言された構造体は、コンパイラとエンジンが実装します。見えるフィールドはありません。

- 構造体リテラルを書くことも、フィールドを読むことも、パターンで分解することもできません。値は `std` の関数とメソッド、リテラル（`Text`、`Decimal`）、またはエンジン（`Narrative`）からのみ得られます。
- `Eq` になるのは `std` が `impl Eq` でそう宣言した場合だけです。構造的な `Eq` の規則は中を見ません（[言語トレイト](./traits.md#eq)を参照）。
- 有限型になるのは `Text` とアセット型だけです。その他のビルトイン型はすべて無限として扱われます。
- 表現は `.kagari` ファイルに含まれないため、ゲームを壊さずにエンジンのバージョン間で変えられます。

ビルトイン型を宣言できるのは `std` だけです（`E1008`）。大半はプレリュードに入っていますが、画面状態の型（`Entity`、`Prop<T>`、各レイヤーなど）は入っていません。これらは `Screen` のメソッドから `&mut` 参照として受け取るもので、名前を書くことはまれだからです。

### `Text` {#text}

`Text` は定数テキストです。プレーン（`"Mio"`）またはリッチ（ルビ、テキスト効果、埋め込みを含む [`t![…]`](../syntax/forms.md) で作るもの）です。文字列リテラルの型は `String` ではなく `Text` です。

```kis
const NAME: Text = "Mio";
```

- **有限。** 物語の中の `Text` の値はプログラムの定数テキストなので、物語変数や `pushdown fn` の引数・戻り値を `Text` にできます。
- **エントリーシーンでは不可。** `fn` のコードは `to_text` で実行時に新しいテキストを作れるため、[エントリーシーン](../concepts/entry.md)の引数型と戻り値型に `Text` を含めることはできません（`E3009`）。
- **埋め込み。** `t![…]` の中の `{expr}` は `to_text(expr)` の呼び出しで、`expr: T` に [`T: ToText`](./traits.md#totext) を要求します。`pushdown fn` の中では、有限な実行時の値に対する `to_text` は他の `const fn` 呼び出しと同じく表化されるので、台詞は物語の状態の有限な関数のままです。
- `Eq`：はい。

### `String` {#string}

`String` は `fn` のコード向けの実行時文字列です。有限型ではなく、`Eq` と `ToText` を実装します。

*暫定。* 現在の `std` には `String` を作る関数も変更する関数もありません。ゲームが表示するテキストは `Text` です。

### アセット型 {#asset-types}

`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` は、ゲームと一緒にパックされるファイルを指します。6 つのシェーダー型（`ColorShader<A>` など）も同様で、引数の型が `A` である 1 つのロールの WGSL ファイル（`.wgsl`）を指します。値は `asset` または `system_font` の定数呼び出しでのみ作られます。

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const THEME: AudioAsset = asset("bgm/theme.ogg");
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

- `asset::<A>` の型引数は定数の型から推論され、[`Asset`](./traits.md#asset) を実装していなければなりません。
- 引数は文字列リテラルで（`E2028`）、空でなく（`E1028`）、アセットディレクトリの中で（`E1029`）その予約されたディレクトリ `system-fonts` と `std` の外を指す必要があります（`E1032`）。ファイルがなければ `E1027` です。`system_font` には独自の規則があります（`E1030`、`E1031`）。STD Reference の [`std`](../../../../std/ja/src/std.md) を参照してください。
- **有限。** ファイルのアセット型の値はちょうどプログラムのアセット定数なので、物語はそれを保持でき、アセット名の綴り間違いは実行時の失敗ではなく名前解決のエラーになります。シェーダー型は有限型ではありません。シェーダーを使うのはレンダーで、物語ではありません。
- `Eq`：はい。
- 効果値が保持する、ロールを持たないシェーダーファイルの識別子 `ShaderAsset` は、`Asset` ではなく、プレリュードにもありません。ゲームが名前で書くことはありません。[`std::screen::shader`](../../../../std/ja/src/screen-shader.md) を参照してください。

### `Setting<T>` と `Meta<T>` {#setting-and-meta}

設定とメタフィールドは、セーブの外に保存される永続的なグローバル値です。それぞれ**ハンドル**を保持する不変の `static` で宣言します。

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- 不変の `static` は `Setting<T>` か `Meta<T>` を保持しなければならず（`E3011`）、`setting(…)` と `meta(…)` は `static` の初期化子としてのみ呼べます（`E4007`）。初期化子は、保存ファイルに値がないときに使われる値です。
- ハンドル自体は値です。関数に渡したり、`Settings` のようなレコードに格納したりできます。コピーするとハンドルがコピーされ、両方のコピーは同じ保存値を読み書きします。
- **種別。** `get` と `set` は `fn` です。`Meta::write` は `pushdown fn` で、物語はメタを書くだけで読みません。`const fn` は `Setting` のハンドルを値としてのみ使えます（`Entity::gate` に渡す場合など）。そこで `get` を呼ぶと `E3001` です。`const` のコードはメタフィールドにまったく言及できません（`E3013`）。
- 有限型ではなく、`Eq` でもありません。

[ゲームシステム](../concepts/system.md)と[レンダーの中の設定](../concepts/presentation.md#settings-in-renders)を参照してください。

### `Narrative` {#narrative}

`Narrative` は、実行中の物語をシステムから見る唯一の窓です。エンジンはこれをゲームビュー（`System::game`、`fn(Narrative) -> Element`）とゲームオーバーレイ（`fn(Narrative) -> GameOverlay`）にだけ渡します。表現は隠されているので、システムのコードは物語変数を読めません。メソッドはすべて `fn` で、読み取りはリアクティブです。コピーは同じ実行中のゲームを指します。有限型ではなく、`Eq` でもありません。

### `Screen` とその部品 {#screen}

`Screen` は、ステップが表示する画面とそのステップのタイムラインです。すべての[レンダー](#render)の入力であり出力です。メソッドはすべて `const fn` です。

- **値。** レンダーは直前の画面のコピーを受け取り、新しい画面を返します。コピーを変えても呼び出し元の画面は変わりません。
- **部品。** `Entity`、`Prop<T>`、`ImageLayer`、`LabelLayer`、`MovieLayer`、`Hud`、`ChoiceMenu`、`Camera`、`Channel` は画面の部品です。`screen.entity(id)` や `screen.music()` などのメソッドから `&mut` 参照として受け取り、その参照を通した変更はその画面を変えます。
- **保留中の変更。** `Change<T>`、`Swap`、`ChannelChange` は `change`、`clear`、`stop` が返す保留中の変更です。これらは [`#[must_use]`](./attributes.md#must_use) で、`apply_animation` を呼ぶまで何も起きません。捨てられた保留中の変更は警告 `W3023` になります。
- `Screen` は `Eq` なので、コンパイラは[一貫性](../concepts/presentation.md#consistency)のために画面を比較できます。部品は `Eq` ではありません。いずれも有限型ではありません。

### `Element` と `Placed` {#element}

`Element` はエンジンがレイアウトする UI 要素ツリーで、`Placed` は `layer![…]` の中に配置された要素です。どちらも UI フォームと `std::ui` の関数で作られ、それらはすべて `fn` です。有限型ではなく、`Eq` でもありません。

### `Arm` と `Divergence` {#arm-and-divergence}

`Arm` は現在の選択肢の選べるアームで、`Narrative::menu` から読み、`Narrative::select` に渡し返します。`Divergence` は、セーブがもうプログラムに合わないときの `nav::load` のエラーです。どちらも `fn` のコードにしか現れません。有限型ではなく、`Eq` でもありません。

## 数値 {#numbers}

| 型 | 定義 | 有限 | 備考 |
| -- | ---- | ---- | ---- |
| `Int<lo, hi>` | 言語のプリミティブ | はい | 整数 `lo..=hi`。`lo` と `hi` は `lo <= hi` を満たす定数の `i64` 値です（`E2004`）。物語が実行時に保持できる唯一の整数型です |
| `u8` … `u128`、`i8` … `i128`、`usize`、`isize` | 言語のプリミティブ | いいえ | Rust の演算子を持つ Rust の整数 |
| `Decimal` | `std` の `#[builtin]` 構造体 | いいえ | 正確な十進数。小数点を含むリテラル（`0.5`）は `Decimal` です。浮動小数点数の代わりです |
| `Rat` | `std` の `#[builtin]` 構造体 | いいえ | 正確な有理数。リテラルはなく、`Rat::new(num, den)` と書きます |

いずれも `Eq` で、整数、`Rat`、`Decimal` には順序があります（`<`、`<=` など）。整数型どうしは算術で混ぜられず（`E2014`）、変換は [`Integer`](./traits.md#integer) のメソッド `into` と `saturate` だけで行います。`as` キャストはありません（`E2019`）。算術、リテラルの型付け、飽和、オーバーフローは[数値](../syntax/numbers.md)で説明します。

## `Vec<T>` {#vec}

`Vec<T>` は `fn` と `const fn` のコード向けの可変長リストです。他のすべての型と同じく値です。`push` と `set` は呼び出した場所を変え、コピーには影響しません。

- `v[i]` で任意の整数型の添字を使ってアクセスでき、`for x in v` で反復できます。どちらも `Vec<T>` と配列に対して言語に組み込まれています。
- 範囲外の `get` は `None` を返し、範囲外の `set` は評価エラーです（コンパイル時評価では `E4005`）。
- 有限型ではありません。`T: Eq` なら `Eq` です。

## `Range<T>` {#range}

`Range<T>` は公開フィールド `start`、`end`、`inclusive` を持つ `std` の普通の構造体です。コンパイラは二つの場所でこれを知っています。

- **範囲式。** `a..b` と `a..=b` は `Range<T>` を作ります。`T` は両端がともに変換できる型で、`0..7` は `Range<Int<0, 7>>` です。範囲の値になるのは `a..b` と `a..=b` だけで、`a..`、`..b`、`..` などの他の形は `E2030` です。パターンには独自の範囲の形（`a..`、`..=b`、`a..b`、`a..=b`）があります。[範囲パターン](../syntax/patterns.md#range-patterns)を参照してください。
- **`for` ループ。** `for` は `T` が整数型の `Range<T>` を反復します（そうでなければ `E2031`）。`pushdown fn` の中では両端が定数式でなければなりません（`E3006`）。

## `Option<T>` と `Result<T, E>` {#option-and-result}

どちらも `std` の普通のジェネリックな列挙型で、プレリュードはそのバリアント `None`、`Some`、`Ok`、`Err` をインポートします。コンパイラは `?` 演算子のためにこれらを知っています。`Option` に対しては `Option` を返す関数が、`Result` に対しては同じエラー型の `Result` を返す関数が必要です（`E2034`）。エラーの変換はありません。型引数が有限なら有限で、型引数が `Eq` なら `Eq` です。

## 関数ポインタ型とクロージャ型 {#function-types}

関数の型は呼び出すコードの[種別](../concepts/kinds.md)を持つので、呼び出し規則はポインタとクロージャを通しても成り立ちます。

| 型 | 保持するもの | 有限 | 対応するクロージャ型 |
| -- | ------------ | ---- | -------------------- |
| `fn(A) -> B` | 名前付きの `fn` | いいえ | `Fn(A) -> B` |
| `const fn(A) -> B` | 名前付きの `const fn`、または何もキャプチャしないクロージャ | はい | `const Fn(A) -> B` |
| `pushdown fn(A) -> B` | 名前付きの即時手続き | はい | `pushdown Fn(A) -> B` |
| `scene fn(A) -> B` | 名前付きのシーン | はい | なし |
| `Fn(A) -> B`、`const Fn(A) -> B`、`pushdown Fn(A) -> B` | その種別のクロージャまたは関数 | いいえ | — |

- 名前付きの関数は、ちょうど自分の種別のポインタ型またはクロージャ型に型強制されます。何もキャプチャしないクロージャは自分の種別のポインタに型強制されますが、キャプチャするクロージャはできません（`E2036`）。ポインタは同じ種別のクロージャ型に型強制されます。
- `pushdown` と `scene` がキーワードになるのは `fn` の直前（および `Fn` の直前の `pushdown`）だけです。
- **有限。** 物語の種別または `const` のポインタは有限です。その値はプログラムの有限個の名前付き関数だからです。クロージャは決して有限型ではなく、物語の中では[第二級](../syntax/values.md)です。
- **`Eq`。** 関数ポインタは同一性で比較されます。クロージャは決して `Eq` ではありません（`E2017`）。

### `Render` {#render}

```kis
pub type Render = const fn(Screen) -> Screen;
```

`std::screen` とプレリュードにある `Render` は新しい型ではなくエイリアスです。レンダーはステップ前の画面からステップ後の画面への `const fn` で、ステップはこれを `render` 属性として受け取ります。`const fn` のポインタなので `Render` は有限で `Eq` であり、`Render` が期待される場所には何もキャプチャしないクロージャをその場で書けます。[レンダー関数](../concepts/presentation.md#render-functions)を参照してください。

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

`Duration` は `std::time` の普通の構造体で、ビルトイン型ではありません。ここに載せるのは、`std` のあらゆる時間が整数ミリ秒の `Duration` だからです。単位付き文字列やフレーム数はありません。プレリュードには `Duration`、`ms(n)`、`secs(n)` があります。

- `u32` を保持するので有限型ではありません。物語は持続時間を、レンダーの中の `fade(ms(500))` のような定数式でのみ使います。
- `Eq`：はい（構造的に）。
- `div` は切り捨てます。`ms(1000).div(3)` は `ms(333)` です。`u32` ミリ秒に収まらない持続時間は、`secs`、`plus`、`times` で作られた場合も画面のタイムラインで到達した場合も、コンパイル時評価で `E4013` になります。
