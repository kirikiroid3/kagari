---
title: "std アイテムのコンパイラ規則"
sidebar:
  label: "std アイテムに関するコンパイラ規則"
---

この付録は、シグネチャ以外にコンパイラが std アイテムに課す規則をまとめたものです：物語が保持できる std の型、リテラルとフォームの型付け、特別なシグネチャ、そしてどの評価の失敗がコンパイルエラーになるか。各規則には、違反したときの診断を記載します。診断の説明は [診断](../../diagnostics/index.md) にあります。

## 種別 {#kinds}

- std の関数はすべて、宣言された種別を持ちます：ページの `Kind:` バッジのとおりです。
- 言語の呼び出し規則は、std のメソッドとビルトインにも自作の関数とまったく同じように適用されます（[`E3001`](../../diagnostics/kinds.md#e3001)）。そのため、レンダー（`const fn`）は `Setting::get` を呼べず、UI コンポーネント（`fn`）は `push_step` を呼べません。
- `push_step`（つまり `s!` と `choice!`）と `rng_choose` は、シーンからのみ呼べます（[`E3002`](../../diagnostics/kinds.md#e3002)、[`E3024`](../../diagnostics/kinds.md#e3024)）。

## 有限型 {#finite-types}

物語が実行時に保持できるのは有限型だけです（[`E3008`](../../diagnostics/kinds.md#e3008)）。std の型のうち、次は有限型です。

- `Text`
- 5 つのファイルのアセット型（`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset`）
- `Render`（`const fn` ポインタ）
- `Option`、`Result`、`Range` など、フィールドがすべて有限型の std の構造体と列挙型。例：`Visibility`、`PlayMode`

次は有限型ではありません。

- `String`、`Vec<T>`、`Rat`、`Decimal`
- `Screen`、`Narrative`、`Meta`、`Setting` など、すべての `#[builtin]` のハンドル型・状態型。`ShaderAsset`、`ColorShader<A>` などのシェーダー型、`ShaderArgs` も含むので、`std::screen::shader` の効果値も有限型ではありません
- Rust の整数や `Decimal` を保持するデータ：`Duration`、`Color`、`EntityId`、`Part`、`Mark`、`Point`、`TextEffect`、`TextStyle` など

物語のコードでも、畳み込まれる定数式の中では有限型でない値を書けます：ステップ内の `ms(300)` や `Mark(1)` は問題ありません。実行時引数を持つ `const fn` 呼び出しは表化され、その結果は有限型でなければなりません。

## 型の規則 {#type-rules}

- 文字列リテラルの型は `Text`、小数点を含むリテラルの型は `Decimal` です。
- `t![…]` の `{expr}` は `to_text(expr)` なので、`T: ToText` である `expr: T` が必要です（[`E2026`](../../diagnostics/types.md#e2026)）。
- 物語の中では、有限型の実行時の値に対する `to_text` は他の `const fn` 呼び出しと同じく表化されるので、行は物語の状態の有限な関数のままです。
- `#[entry]` シーンの引数と戻り値の型は `Text` を含んではなりません。`fn` のコードが `to_text` で実行時のテキストを作れるからです（[`E3009`](../../diagnostics/kinds.md#e3009)）。
- `a..b` と `a..=b` の型は `Range<T>` で、`T` は `a` と `b` の両方が変換できる型です：`0..7` は使い方が別の整数型を期待しない限り `Range<i32>`、`last: Int<0, 7>` なら `0..last` は `Range<Int<0, 7>>` です。範囲式には両端が必要で、`a..`、`..b`、`..` はパターンにのみ書けます（[`E2030`](../../diagnostics/types.md#e2030)）。
- `x: S` を `T` に変換する `x.into()`（どちらも `Integer`）には、`S` のすべての値が `T` に収まることが必要です（[`E2018`](../../diagnostics/types.md#e2018)）。`x.saturate()` にはその条件はありません。

## フォーム {#forms}

- `choice!` の id は文字列リテラルで、関数内で一意です（[`E3019`](../../diagnostics/kinds.md#e3019)）。
- `choice!` のアームの値はフィールドを持たない列挙型のバリアントで、1 つの選択肢につき各バリアント 1 回までです（[`E3020`](../../diagnostics/kinds.md#e3020)）。
- ルビの親文字は文字列リテラルで、モノルビの読みは親文字 1 文字につき 1 つの読みを持つ配列リテラルです（[`E3021`](../../diagnostics/kinds.md#e3021)）。
- `!["…"](e)` の効果は定数式です（[`E3022`](../../diagnostics/kinds.md#e3022)）。

## トレイト {#traits}

- トレイトを宣言するのは `std` だけで、どれも `#[builtin]` です。ゲーム内の `trait` アイテムや `impl Trait for T` はエラーです（[`E1012`](../../diagnostics/modules.md#e1012)、[`E1023`](../../diagnostics/modules.md#e1023)）。
- `Eq` は、プリミティブ型、`Int`、関数ポインタ、そしてフィールドがすべて `Eq` の構造体・列挙型・タプル・配列に成り立ちます。`#[builtin]` 型では、`std` 内の `impl Eq` があるときだけ成り立ちます。クロージャは決して `Eq` ではありません。`Eq` でない型への `==` は [`E2017`](../../diagnostics/types.md#e2017) です。
- 満たされない境界（`A: Asset`、`T: RandChoose`、`T: ToText`、`A: ShaderArg` など）は [`E2026`](../../diagnostics/types.md#e2026) です。
- `ShaderArg` は `()` と `#[shader_arg]` を付けた構造体に成り立ち、それ以外には成り立ちません。

## `#[shader_arg]` {#shader_arg}

シェーダーの引数型は `()` か、`#[shader_arg]` を付けた構造体です。[引数](../guides/shaders.md#arguments) を参照してください。

- 構造体以外への `#[shader_arg]` は [`E3018`](../../diagnostics/kinds.md#e3018) です。
- 構造体は名前付きフィールドを持ち、型パラメータを持ってはなりません（const パラメータは使えます）（[`E3025`](../../diagnostics/kinds.md#e3025)）。
- 各フィールドは `bool`、`i32`、`u32`、`Decimal`、`Rat`、`Color`、`Point`、`Offset`、`Size`、`Anchor`、`Angle`、`Duration`、Kagari で宣言されたバリアントが 1 つ以上でフィールドのない列挙型、`#[shader_arg]` 構造体、またはこれらの配列 `[T; N]`（`N` > 0）のいずれかです（[`E3026`](../../diagnostics/kinds.md#e3026)）。

## `asset::<A>` {#asseta}

- `A: Asset` が必要です（[`E2026`](../../diagnostics/types.md#e2026)）。
- 引数は文字列リテラルで（[`E2028`](../../diagnostics/types.md#e2028)）、空ではなく（[`E1028`](../../diagnostics/modules.md#e1028)）、アセットディレクトリの中で（[`E1029`](../../diagnostics/modules.md#e1029)）その予約されたディレクトリ `system-fonts` と `std` の外を指し（[`E1032`](../../diagnostics/modules.md#e1032)）、ファイルが存在しなければなりません（[`E1027`](../../diagnostics/modules.md#e1027)）。
- `system_font` も同じリテラルの規則に従います。固有のエラーは [`E1030`](../../diagnostics/modules.md#e1030) と [`E1031`](../../diagnostics/modules.md#e1031) です。

## `nav::start(entry, args, on_exit)` {#navstartentry-args-on_exit}

シグネチャ `start<E, A, R>(entry: E, args: A, on_exit: Fn(R))` は特別です（[`E2027`](../../diagnostics/types.md#e2027)）。

- `entry` は `#[entry]` シーンを指す `scene fn(P1, …, Pn) -> R` ポインタです。
- `args` はタプル `(P1, …, Pn)` です。
- `on_exit` は `Fn(R)` です。

## `const fn` 内の設定とメタ {#settings-and-meta-in-const-fn}

- `Setting<T>` ハンドルは、`const fn` 内では値としてだけ、つまり `Entity::gate` に渡す形でだけ使えます。`get` と `set` は `fn` です。
- メタフィールドは `const` のコードでは一切使えません（[`E3013`](../../diagnostics/kinds.md#e3013)）。
- `Setting` と `Meta` のハンドルは、不変の `static` の初期化子としてだけ存在します（[`E3011`](../../diagnostics/kinds.md#e3011)、[`E4007`](../../diagnostics/eval.md#e4007)）。

## `#[must_use]` {#must_use}

`Change<T>`、`Swap`、`ChannelChange` が対象です。捨てられた保留中の変更や、ローカル変数に束縛されたまま `apply_animation` で完了されない変更は警告（[`W3023`](../../diagnostics/kinds.md#w3023)）になり、その変更は起こりません。

## `for` ループ {#for-loops}

- `Vec<T>`、配列、`Integer` な `T` の `Range<T>` に対する `for` は、`fn` と `const fn` で使えます。それ以外は反復できません（[`E2031`](../../diagnostics/types.md#e2031)）。
- `pushdown fn` では、`for` は配列と、両端が定数式の範囲に対してだけ使えます（[`E3006`](../../diagnostics/kinds.md#e3006)）。

## 定数評価のエラー {#const-evaluation-errors}

std のコードが定数評価で実行されるとき、次の失敗はコンパイルエラーです。

|失敗|診断|
|---|---|
|整数のオーバーフロー|[`E4001`](../../diagnostics/eval.md#e4001)|
|ゼロ除算（`Rat::new(_, 0)`、`Duration::div(0)` など）|[`E4002`](../../diagnostics/eval.md#e4002)|
|範囲外の `Vec::set`|[`E4005`](../../diagnostics/eval.md#e4005)|
|`static` の初期化子以外での `Setting` や `Meta` のハンドル|[`E4007`](../../diagnostics/eval.md#e4007)|
|`Motion` の規則違反：キーがない、`at` が狭義単調増加でないか 0 に等しい、最後のキーが恒等でない、`repeat` = 0|[`E4008`](../../diagnostics/eval.md#e4008)|
|トゥイーンまたは遷移モーションでの、0–1 の外にある `CubicBezier` の x、および `Steps(0)`|[`E4009`](../../diagnostics/eval.md#e4009)|
|テキストマークの重複|[`E4010`](../../diagnostics/eval.md#e4010)|
|マークに合わせている間の `begin_text`|[`E4011`](../../diagnostics/eval.md#e4011)|
|0.0–1.0 が必要な箇所（不透明度、音量、アンカー）での範囲外の `Decimal`|[`E4012`](../../diagnostics/eval.md#e4012)|
|`Duration` のオーバーフローやアンダーフロー|[`E4013`](../../diagnostics/eval.md#e4013)|

実行時には、`fn` のコードで起きた同じ失敗は、実行中のゲームの評価エラーになります。
