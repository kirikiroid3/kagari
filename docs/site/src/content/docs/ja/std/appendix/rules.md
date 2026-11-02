---
title: "標準ライブラリに関するコンパイラの規則"
sidebar:
  label: "コンパイラの規則"
---

この付録では、標準ライブラリのアイテムについて、シグネチャだけでは表せない規則のうちコンパイラが検査するものをまとめます。物語が保持できる標準ライブラリの型、リテラルと組み込み構文の型付け、特別なシグネチャ、コンパイルエラーになる評価の失敗などです。規則ごとに、違反したときに出る診断を示します。診断の説明は[診断](../../diagnostics/index.md)にあります。

## 種別 {#kinds}

- 標準ライブラリの関数の種別は、宣言どおりです。各ページの `Kind:` バッジに示しています。
- 言語の呼び出し規則は、標準ライブラリのメソッドや組み込みのアイテムにも、自分で書いた関数とまったく同じように適用されます（[`E3001`](../../diagnostics/kinds.md#e3001)）。そのため、レンダー（`const fn`）から `Setting::get` は呼べず、UI コンポーネント（`fn`）から `push_step` は呼べません。
- `push_step`（したがって `s!` と `choice!`）と `rng_choose` は、シーンからしか呼べません（[`E3002`](../../diagnostics/kinds.md#e3002)、[`E3024`](../../diagnostics/kinds.md#e3024)）。

## 有限型 {#finite-types}

物語が実行時に保持できるのは有限型だけです（[`E3008`](../../diagnostics/kinds.md#e3008)）。標準ライブラリの型のうち、有限型は次のとおりです。

- `Text`
- ファイルを表す 5 種類のアセット型（`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset`）
- `Render`（`const fn` へのポインタ）
- `Option`、`Result`、`Range` をはじめ、フィールドがすべて有限型である標準ライブラリの構造体と列挙型（例：`Visibility`、`PlayMode`）

次の型は有限型ではありません。

- `String`、`Vec<T>`、`Rat`、`Decimal`
- `Screen`、`Narrative`、`Meta`、`Setting` など、`#[builtin]` のハンドル型・状態型すべて。`ShaderAsset`、`ColorShader<A>` などのシェーダー型、`ShaderArgs` もこれに含まれるので、`std::screen::shader` の効果値も有限型ではありません
- Rust の整数や `Decimal` を内部に持つデータ。`Duration`、`Color`、`EntityId`、`Part`、`Mark`、`Point`、`TextEffect`、`TextStyle` など

ただし物語のコードでも、畳み込まれる定数式の中なら有限型でない値を書けます。ステップ内の `ms(300)` や `Mark(1)` は問題ありません。実行時の値を引数にとる `const fn` の呼び出しは表化され、その結果は有限型でなければなりません。

## 型の規則 {#type-rules}

- 文字列リテラルの型は `Text`、小数点を含むリテラルの型は `Decimal` です。
- `t![…]` の中の `{expr}` は `to_text(expr)` と同じなので、`expr: T` に対して `T: ToText` が必要です（[`E2026`](../../diagnostics/types.md#e2026)）。
- 物語の中では、有限型の実行時の値に対する `to_text` も、ほかの `const fn` 呼び出しと同様に表化されます。そのため台詞は、物語の状態から有限に決まる関数のままです。
- `#[entry]` シーンの引数と戻り値の型には `Text` を含められません。`fn` のコードは `to_text` で実行時にテキストを作れるからです（[`E3009`](../../diagnostics/kinds.md#e3009)）。
- `a..b` と `a..=b` の型は `Range<T>` です。`T` は `a` と `b` のどちらも変換できる型です。たとえば `0..7` は、使われ方から別の整数型が求められない限り `Range<i32>` になり、`last: Int<0, 7>` のとき `0..last` は `Range<Int<0, 7>>` になります。範囲式には両端が必要で、`a..`、`..b`、`..` はパターンの中にしか書けません（[`E2030`](../../diagnostics/types.md#e2030)）。
- `x: S`（`Integer`）を `T`（`FromInteger`、つまり整数型か `Decimal`）に変換する `x.into()` は、`S` のすべての値が `T` に収まる場合にだけ使えます（[`E2018`](../../diagnostics/types.md#e2018)）。整数はすべて `Decimal` に収まります。`x.saturate()` にはこの条件はありませんが、変換先は `Integer` でなければなりません（[`E2026`](../../diagnostics/types.md#e2026)）。

## 組み込み構文 {#forms}

- `choice!` の id は文字列リテラルで、関数内で重複できません（[`E3019`](../../diagnostics/kinds.md#e3019)）。
- `choice!` のアームの値はフィールドのない列挙型のバリアントで、一つの選択肢の中で同じバリアントは一度しか使えません（[`E3020`](../../diagnostics/kinds.md#e3020)）。
- ルビの親文字は文字列リテラルです。モノルビの読みは配列リテラルで、親文字 1 文字につき読みを一つずつ書きます（[`E3021`](../../diagnostics/kinds.md#e3021)）。
- `!["…"](e)` の効果は定数式でなければなりません（[`E3022`](../../diagnostics/kinds.md#e3022)）。

## トレイト {#traits}

- トレイトを宣言できるのは `std` だけで、どれも `#[builtin]` です。ゲームの中で `trait` アイテムや `impl Trait for T` を書くとエラーになります（[`E1012`](../../diagnostics/modules.md#e1012)、[`E1023`](../../diagnostics/modules.md#e1023)）。
- `Eq` を満たすのは、プリミティブ型、`Int`、関数ポインタ、そしてフィールドがすべて `Eq` である構造体・列挙型・タプル・配列です。`#[builtin]` の型は、`std` の中に `impl Eq` がある場合に限り `Eq` を満たします。クロージャが `Eq` を満たすことはありません。`Eq` でない型に `==` を使うとエラー [`E2017`](../../diagnostics/types.md#e2017) になります。
- 境界（`A: Asset`、`T: RandChoose`、`T: ToText`、`A: ShaderArg` など）を満たさない場合はエラー [`E2026`](../../diagnostics/types.md#e2026) になります。
- `ShaderArg` を満たすのは `()` と `#[shader_arg]` を付けた構造体だけです。

## `#[shader_arg]` {#shader_arg}

シェーダーの引数の型は、`()` か `#[shader_arg]` を付けた構造体です。[引数](../guides/shaders.md#arguments)を参照してください。

- 構造体以外に `#[shader_arg]` を付けるとエラー [`E3018`](../../diagnostics/kinds.md#e3018) になります。
- 構造体のフィールドには名前が必要で、型パラメータは持てません（const パラメータは使えます）（[`E3025`](../../diagnostics/kinds.md#e3025)）。
- 各フィールドの型は、`bool`、`i32`、`u32`、`Decimal`、`Rat`、`Color`、`Point`、`Offset`、`Size`、`Anchor`、`Angle`、`Duration`、Kagari で宣言した列挙型（バリアントが一つ以上あり、どれもフィールドを持たないもの）、`#[shader_arg]` 構造体、またはこれらの配列 `[T; N]`（`N` > 0）のいずれかです（[`E3026`](../../diagnostics/kinds.md#e3026)）。

## `asset::<A>` {#asseta}

- `A: Asset` が必要です（[`E2026`](../../diagnostics/types.md#e2026)）。
- 引数は文字列リテラルで（[`E2028`](../../diagnostics/types.md#e2028)）、空であってはならず（[`E1028`](../../diagnostics/modules.md#e1028)）、アセットディレクトリの中（[`E1029`](../../diagnostics/modules.md#e1029)）で、かつ予約済みの最上位ディレクトリ `system-fonts` と `std` の外（[`E1032`](../../diagnostics/modules.md#e1032)）を指していなければなりません。指す先のファイルも存在する必要があります（[`E1027`](../../diagnostics/modules.md#e1027)）。
- `system_font` にも同じリテラルの規則が適用されます。`system_font` に固有のエラーは [`E1030`](../../diagnostics/modules.md#e1030) と [`E1031`](../../diagnostics/modules.md#e1031) です。

## `nav::start(entry, args, on_exit)` {#navstartentry-args-on_exit}

シグネチャ `start<E, A, R>(entry: E, args: A, on_exit: Fn(R))` には特別な規則があります（[`E2027`](../../diagnostics/types.md#e2027)）。

- `entry` は、`#[entry]` シーンを指す `scene fn(P1, …, Pn) -> R` 型のポインタです。
- `args` はタプル `(P1, …, Pn)` です。
- `on_exit` は `Fn(R)` です。

## `const fn` の中での設定とメタデータ {#settings-and-meta-in-const-fn}

- `const fn` の中で `Setting<T>` ハンドルを使えるのは、値として扱う場合、つまり `Entity::gate` に渡す場合だけです。`get` と `set` は `fn` です。
- メタデータのフィールドは、`const` のコードでは一切使えません（[`E3013`](../../diagnostics/kinds.md#e3013)）。
- `Setting` と `Meta` のハンドルは、不変の `static` の初期化子としてしか作れません（[`E3011`](../../diagnostics/kinds.md#e3011)、[`E4007`](../../diagnostics/eval.md#e4007)）。

## `#[must_use]` {#must_use}

対象は `Change<T>`、`Swap`、`ChannelChange` です。保留中の変更を捨てたり、ローカル変数に束縛したまま `apply_animation` で完了させなかったりすると警告（[`W3023`](../../diagnostics/kinds.md#w3023)）が出て、その変更は起こりません。

## `for` ループ {#for-loops}

- `fn` と `const fn` では、`Vec<T>`、配列、`T` が `Integer` である `Range<T>` を `for` で反復できます。それ以外は反復できません（[`E2031`](../../diagnostics/types.md#e2031)）。
- `pushdown fn` では、`for` で反復できるのは配列と、両端が定数式の範囲だけです（[`E3006`](../../diagnostics/kinds.md#e3006)）。

## 定数評価のエラー {#const-evaluation-errors}

標準ライブラリのコードを定数評価で実行したとき、次の失敗はコンパイルエラーになります。

|失敗|診断|
|---|---|
|整数のオーバーフロー|[`E4001`](../../diagnostics/eval.md#e4001)|
|ゼロ除算（`Rat::new(_, 0)`、`Duration::div(0)` など）|[`E4002`](../../diagnostics/eval.md#e4002)|
|範囲外を指す `Vec::set`|[`E4005`](../../diagnostics/eval.md#e4005)|
|`static` の初期化子以外にある `Setting` や `Meta` のハンドル|[`E4007`](../../diagnostics/eval.md#e4007)|
|`Motion` の規則違反（キーがない、`at` が狭義単調増加でないか 0 に等しい、最後のキーが恒等でない、`repeat` = 0）|[`E4008`](../../diagnostics/eval.md#e4008)|
|補間または遷移モーションで、`CubicBezier` の x が 0–1 の範囲外、または `Steps(0)`|[`E4009`](../../diagnostics/eval.md#e4009)|
|テキストのマークの重複|[`E4010`](../../diagnostics/eval.md#e4010)|
|マークに位置を合わせている間の `begin_text`|[`E4011`](../../diagnostics/eval.md#e4011)|
|0.0–1.0 でなければならない箇所（不透明度、音量、アンカー）で範囲外の `Decimal`|[`E4012`](../../diagnostics/eval.md#e4012)|
|`Duration` のオーバーフローまたはアンダーフロー|[`E4013`](../../diagnostics/eval.md#e4013)|

実行時に `fn` のコードで同じ失敗が起きた場合は、実行中のゲームの評価エラーになります。
