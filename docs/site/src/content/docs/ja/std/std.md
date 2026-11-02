---
title: "std"
---

> **安定性:** 安定 — 組み込み型、`Range`、`Option` と `Result`、std のトレイト、アセット型。

クレートのルート `std` では、どのプログラムでも必要になる型を宣言しています。実行時の文字列とリスト、正確な数値、範囲、ジェネリックな直和型 `Option` と `Result`、std のシグネチャで使うトレイト、アセット型と `asset`・`system_font` です。ルートの関数はすべて `const fn` です。

サブモジュールもルートで宣言しています。

```kis
pub mod prelude;
pub mod time;
pub mod screen;
pub mod stage;
pub mod narrative;
pub mod ui;
pub mod system;
pub mod shaders;
```

`Int<lo, hi>`、Rust の整数型（`u8` … `u128`、`i8` … `i128`、`usize`、`isize`）、`bool` は言語の型であり、std のアイテムではありません。`Int<lo, hi>` は `lo..=hi` の整数を表す型で、物語が実行時に保持できる整数型はこれだけです。詳しくは [数値](../reference/syntax/numbers.md) を参照してください。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`String`](#struct-string)|組み込みの構造体|実行時の文字列|
|[`Vec<T>`](#struct-vect)|組み込みの構造体|伸長できるリスト|
|[`Rat`](#struct-rat)|組み込みの構造体|正確な有理数|
|[`Decimal`](#struct-decimal)|組み込みの構造体|正確な十進小数|
|[`Range<T>`](#struct-ranget)|構造体|`start..end` と `start..=end`|
|[`Option<T>`](#enum-optiont)|列挙型|省略可能な値|
|[`Result<T, E>`](#enum-resultt-e)|列挙型|成功値またはエラー値|
|[`Eq`](#trait-eq)|組み込みのトレイト|構造的な等価性|
|[`Integer`](#trait-integer)|組み込みのトレイト|整数型|
|[`FromInteger`](#trait-frominteger)|組み込みのトレイト|`Integer::into` の変換先|
|[`RandChoose`](#trait-randchoose)|組み込みのトレイト|物語で乱択できる型|
|[`Asset`](#trait-asset)|組み込みのトレイト|アセット型|
|[`ImageAsset` … `ShaderAsset`](#asset-types)|組み込みの構造体|アセットファイルの種類ごとの型|
|[`asset`](#function-assetpath)|組み込みの `const fn`|プログラムのアセットを読み込む|
|[`system_font`](#function-system_fontpath)|組み込みの `const fn`|ビルド環境にインストールされたフォントを読み込む|

## 構造体：`String` {#struct-string}

```kis
#[builtin] pub struct String;
```

`fn` のコードが実行時に組み立てる文字列です。文字列リテラルの型は `String` ではなく `Text` で、`String` は `fn` のコードが実行時に作るものです。`String` は `Eq` を満たし、`ToText` なので `t![…]` に埋め込めます。有限型ではないため、物語で保持することはできません。

- 種別：— · 組み込み：はい

## 構造体：`Vec<T>` {#struct-vect}

```kis
#[builtin] pub struct Vec<T>;
```

伸長できるリストです。ほかの型と同じく `Vec` も値型なので、`push` や `set` は呼び出した場所だけを書き換え、コピーには影響しません。`T` が `Eq` なら `Vec<T>` も `Eq` です。`Vec` に対する `for x in v` はメソッドではなく言語の規則です（[コンパイラの規則](./appendix/rules.md#for-loops) を参照）。`Vec` は有限型ではありません。

- 種別：— · 組み込み：はい

### `Vec::new()` {#vecnew}

```kis
#[builtin] pub const fn new() -> Vec<T>;
```

- 戻り値：`Vec<T>` — 空のリスト。
- 種別：`const fn` · 組み込み：はい

### `Vec::len(&self)` {#veclenself}

```kis
#[builtin] pub const fn len(&self) -> usize;
```

- 戻り値：`usize` — 要素数。
- 種別：`const fn` · 組み込み：はい

### `Vec::is_empty(&self)` {#vecis_emptyself}

```kis
#[builtin] pub const fn is_empty(&self) -> bool;
```

- 戻り値：`bool` — 要素が一つもないかどうか。
- 種別：`const fn` · 組み込み：はい

### `Vec::get(&self, index)` {#vecgetself-index}

```kis
#[builtin] pub const fn get(&self, index: usize) -> Option<T>;
```

- `index` `usize` — 0 から数えた位置。
- 戻り値：`Option<T>` — `index` の位置の要素。`index` が範囲外なら `None`。
- 種別：`const fn` · 組み込み：はい

### `Vec::set(&mut self, index, value)` {#vecsetmut-self-index-value}

```kis
#[builtin] pub const fn set(&mut self, index: usize, value: T);
```

- `index` `usize` — 置き換える位置。
- `value` `T` — 新しい要素。
- 種別：`const fn` · 組み込み：はい

`index` の位置の要素を置き換えます。`index` が範囲外だと評価エラーになり、定数評価ではエラー [`E4005`](../diagnostics/eval.md#e4005) になります。

### `Vec::push(&mut self, value)` {#vecpushmut-self-value}

```kis
#[builtin] pub const fn push(&mut self, value: T);
```

- `value` `T` — 追加する要素。
- 種別：`const fn` · 組み込み：はい

`value` を末尾に追加します。

```kis
const fn first_three() -> Vec<u32> {
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    v
}
```

## 構造体：`Rat` {#struct-rat}

```kis
#[builtin] pub struct Rat;
```

正確な有理数です。リテラルはないので、`Rat::new` で作ります。`Rat` は `Eq` と `ToText` を満たし、文字列にすると正確な値で書き出されます。有限型ではありません。

- 種別：— · 組み込み：はい

### `Rat::new(num, den)` {#ratnewnum-den}

```kis
#[builtin] pub const fn new(num: i128, den: i128) -> Rat;
```

- `num` `i128` — 分子。
- `den` `i128` — 分母。
- 戻り値：`Rat` — `num / den` を既約分数にした値。
- 種別：`const fn` · 組み込み：はい

`den == 0` のときはゼロ除算になります（定数評価ではエラー [`E4002`](../diagnostics/eval.md#e4002)）。たとえば `Rat::new(1, 3)` は 3 分の 1 です。

### `Rat::to_decimal(self)` {#ratto_decimalself}

```kis
#[builtin] pub const fn to_decimal(self) -> Decimal;
```

- 戻り値：`Decimal` — 整数部と小数点以下最大 7 桁で表した値。超える桁はゼロ方向に切り捨てます。
- 種別：`const fn` · 組み込み：はい

`Rat::new(1, 3).to_decimal()` は `0.3333333` になります。

## 構造体：`Decimal` {#struct-decimal}

```kis
#[builtin] pub struct Decimal;
```

`0.5` のようなリテラルで書く、正確な十進小数です。Kagari には浮動小数点型がなく、その代わりに使う型です。`std` の割合もすべてこの型で表します。`Decimal / Decimal` は正確に計算したうえで、小数点以下 7 桁までをゼロ方向に切り捨てます。`Decimal` は `Eq` と `ToText` を満たし、文字列にすると正確な値で書き出されます。有限型ではありません。

- 種別：— · 組み込み：はい

### `Decimal::from_int(value)` {#decimalfrom_intvalue}

```kis
#[builtin] pub const fn from_int<I: Integer>(value: I) -> Decimal;
```

- `value` `I` — 任意の整数。
- 戻り値：`Decimal` — `value` と等しい値（誤差なし）。
- 種別：`const fn` · 組み込み：はい

どの整数も表せるので、この変換は失敗しません。変換先を `Decimal` にした `value.into()` と同じです。`Decimal::from_int(3u32)` は `3.0` になります。

```kis
const fn moon_angle(day: u32) -> Decimal {
    Decimal::from_int(day) * 5.0
}
```

## 構造体：`Range<T>` {#struct-ranget}

```kis
pub struct Range<T> {
    pub start: T,
    pub end: T,
    pub inclusive: bool,
}
```

- `start` `T` — 最初の値。
- `end` `T` — 終端。
- `inclusive` `bool` — `start..end`（終端を含まない）なら `false`、`start..=end` なら `true`。
- 種別：— · 組み込み：いいえ

`a..b` と `a..=b` は `Range` の値です。式では両端とも省略できず、`a..`、`..b`、`..` はパターンでしか書けません。`0..7` は、使われる場所が別の整数型を求めていなければ `Range<i32>` になります。`last: Int<0, 7>` なら `0..last` は `Range<Int<0, 7>>` です。型付けは [コンパイラの規則](./appendix/rules.md#type-rules)、反復は [`for` ループ](./appendix/rules.md#for-loops) を参照してください。

## 列挙型：`Option<T>` {#enum-optiont}

```kis
pub enum Option<T> {
    None,
    Some(T),
}
```

省略可能な値です。型とバリアント `None`、`Some` はプレリュードでインポートされます。

- 種別：— · 組み込み：いいえ

## 列挙型：`Result<T, E>` {#enum-resultt-e}

```kis
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

成功値またはエラー値です。型とバリアント `Ok`、`Err` はプレリュードでインポートされます。`nav::load` の戻り値は `Result` です。

- 種別：— · 組み込み：いいえ

## トレイト：`Eq` {#trait-eq}

```kis
#[builtin] pub trait Eq {}
impl Eq for String {}
impl<T: Eq> Eq for Vec<T> {}
impl Eq for Rat {}
impl Eq for Decimal {}
impl Eq for ImageAsset {}
impl Eq for AudioAsset {}
impl Eq for VoiceAsset {}
impl Eq for VideoAsset {}
impl Eq for FontAsset {}
impl Eq for ShaderAsset {}
impl Eq for narrative::Text {}
impl Eq for screen::Screen {}
```

`==` と `!=` で比較できる型です。`ui::select` の境界に使います。

- 種別：— · 組み込み：はい

プリミティブ型、`Int`、関数ポインタ、およびフィールドがすべて `Eq` である構造体・列挙型・タプル・配列は、自動的に `Eq` になります。`#[builtin]` 型には外から見えるフィールドがないため、`std` が上の `impl` で明示しています。`Eq` になるのは `String`、`T` が `Eq` の `Vec<T>`、`Rat`、`Decimal`、アセット型、`Text`、`Screen` です。ハンドルや保留中の変更（`Setting`、`Meta`、`Narrative`、`Entity`、`Prop`、`Change`、レイヤー、`Swap`、`Channel`、`ChannelChange`、`Element` など）とクロージャは `Eq` ではありません。`Eq` でない型の値を比較するとエラー [`E2017`](../diagnostics/types.md#e2017) になります。

## トレイト：`Integer` {#trait-integer}

```kis
#[builtin] pub trait Integer {
    const fn into<T: FromInteger>(self) -> T;
    const fn saturate<T: Integer>(self) -> T;
}
impl Integer for u8 {}
impl Integer for u16 {}
impl Integer for u32 {}
impl Integer for u64 {}
impl Integer for u128 {}
impl Integer for i8 {}
impl Integer for i16 {}
impl Integer for i32 {}
impl Integer for i64 {}
impl Integer for i128 {}
impl Integer for usize {}
impl Integer for isize {}
impl<const LO: i64, const HI: i64> Integer for Int<LO, HI> {}
```

整数型、つまり `Int<lo, hi>` と Rust の整数型です。

- 種別：— · 組み込み：はい

### `Integer::into(self)` {#integerintoself}

```kis
const fn into<T: FromInteger>(self) -> T;
```

- 戻り値：`T` — 同じ値を `T` で表したもの。
- 種別：`const fn` · 組み込み：はい

別の整数型または `Decimal` へ、値を失わずに変換します。`Self` のどの値も `T` に収まる場合でなければコンパイルエラー（[`E2018`](../diagnostics/types.md#e2018)）になります。たとえば `Int<0, 5>` は `u8` や `Int<0, 10>` に変換できますが、`u8` から `Int<0, 5>` への変換はコンパイルできません。整数はどれも `Decimal` で表せるので、`7u8.into::<Decimal>()` は `7.0` になります。

### `Integer::saturate(self)` {#integersaturateself}

```kis
const fn saturate<T: Integer>(self) -> T;
```

- 戻り値：`T` — `T` の範囲内に丸め込んだ値。
- 種別：`const fn` · 組み込み：はい

`saturate` には収まるかどうかの制約がありません。`300u32.saturate()` を `u8` に変換すると `255` になります。

```kis
const fn level(n: u32) -> Int<0, 10> {
    n.saturate()
}
```

## トレイト：`FromInteger` {#trait-frominteger}

```kis
#[builtin] pub trait FromInteger {}
impl FromInteger for Decimal {}
```

[`Integer::into`](#integerintoself) で整数から変換できる型です。すべての `Integer` 型（暗黙に実装）と `Decimal` が該当します。`Integer` を境界に持つ型パラメータも `FromInteger` なので、`T: Integer` を使ったジェネリックなコードから `x.into::<T>()` を呼べます。`Integer::saturate` の変換先は整数型に限られ、`Decimal` へ `saturate` するとエラー [`E2026`](../diagnostics/types.md#e2026) になります。`FromInteger` はプレリュードに含まれないため、ジェネリック関数では `std::FromInteger` と書きます。

- 種別：— · 組み込み：はい

## トレイト：`RandChoose` {#trait-randchoose}

```kis
#[builtin] pub trait RandChoose {}
impl<const LO: i64, const HI: i64> RandChoose for Int<LO, HI> {}
```

物語が [`narrative::rng_choose`](./narrative.md#function-rng_choose) で乱択できる型です。フィールドのない列挙型（暗黙に実装）と `Int<lo, hi>` が該当します。

- 種別：— · 組み込み：はい

## トレイト：`Asset` {#trait-asset}

```kis
#[builtin] pub trait Asset {}
impl Asset for ImageAsset {}
impl Asset for AudioAsset {}
impl Asset for VoiceAsset {}
impl Asset for VideoAsset {}
impl Asset for FontAsset {}
```

取りうる値がプログラムのアセット定数そのものである型です。[`asset`](#function-assetpath) の境界に使います。下の 5 つのファイル型と、[`std::screen::shader`](./screen-shader.md) の 6 つのシェーダー型（`ColorShader<A>` など）が実装しています。

- 種別：— · 組み込み：はい

`std` のトレイトはすべて `#[builtin]` で、トレイトを宣言・実装できるのは `std` だけです。ゲーム側で `trait` アイテムや `impl Trait for T` を書くとエラーになります。[コンパイラの規則](./appendix/rules.md#traits) を参照してください。

## アセット型 {#asset-types}

```kis
#[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
#[builtin] pub struct VoiceAsset;
#[builtin] pub struct VideoAsset;
#[builtin] pub struct FontAsset;
#[builtin] pub struct ShaderAsset;
```

アセットファイルの種類ごとに用意した、中身の見えない型です。6 つとも `Eq` です。最初の 5 つは `Asset` を満たす有限型で、取りうる値はアセット定数そのものです。

- 種別：— · 組み込み：はい

|型|ファイル|使う場所|
|---|---|---|
|`ImageAsset`|画像：PNG、JPEG、WebP|画像レイヤー、ルールマスク、サイド画像、UI の画像と塗り|
|`AudioAsset`|音楽・効果音のファイル|音楽と環境音のループ、単発の効果音|
|`VoiceAsset`|台詞に対応するボイスファイル|`Say::voice`、`Narrative::replay_voice`|
|`VideoAsset`|動画ファイル|ムービーレイヤー|
|`FontAsset`|フォントファイル|`System::font`、`TextEffect::font`、`TextStyle::font`|
|`ShaderAsset`|役割を問わないシェーダーファイルの識別子。効果値が保持する|[`std::screen::shader`](./screen-shader.md) の効果値|

`ShaderAsset` は `Asset` ではなく、プレリュードにも含まれません。ゲームではシェーダーを `ColorShader<A>` など役割ごとの型で宣言するので、`ShaderAsset` という名前を書くことはありません。

ランタイムが読めるファイル形式は [ランタイムプロファイル](./appendix/profiles.md) にまとめています。

## 関数：`asset(path)` {#function-assetpath}

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

- `path` `Text` — プログラムのアセットディレクトリからの相対パス（文字列リテラル）。
- 戻り値：`A` — アセット。
- 種別：`const fn` · 組み込み：はい

アセットは定数として宣言します。

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
const THEME: AudioAsset = asset("bgm/theme.ogg");
```

- 型引数 `A` は定数の型から推論します。アセット型以外だとエラー [`E2026`](../diagnostics/types.md#e2026) になります。
- 引数は文字列リテラルに限られ（[`E2028`](../diagnostics/types.md#e2028)）、空文字列は書けません（[`E1028`](../diagnostics/modules.md#e1028)）。
- パスはアセットディレクトリからの相対パスです。アセットディレクトリは `kagari.toml` の `[assets] dir` で指定し、既定は `assets` です（マニフェストがなければルートファイルのあるディレクトリ）。ディレクトリの外を指すパスはエラーです（[`E1029`](../diagnostics/modules.md#e1029)）。`std` の中では `std` 自身からの相対パスになり、`std` は [`std::shaders`](./shaders.md) のファイルを同梱しています。
- ファイルが存在しなければコンパイルエラーです（[`E1027`](../diagnostics/modules.md#e1027)）。表情も曲もボイスも定数なので、名前を打ち間違えても実行時に失敗するのではなく、名前解決の段階でエラーになります。
- アセットディレクトリ直下の `system-fonts` と `std` は、システムフォントと std のアセット用に予約されています（[`E1032`](../diagnostics/modules.md#e1032)）。
- `kagari build` は、プログラムが使うアセットの形式をすべてプロジェクトのランタイムプロファイルと照合し、シェーダーもすべて役割に合っているか検査します。[ランタイムプロファイル](./appendix/profiles.md) と [シェーダーの検査](./guides/shaders.md#checking-shaders) を参照してください。

## 関数：`system_font(path)` {#function-system_fontpath}

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

- `path` `Text` — いずれかのシステムフォントディレクトリからの相対パス（文字列リテラル）。
- 戻り値：`FontAsset` — ゲームに同梱するフォント。
- 種別：`const fn` · 組み込み：はい

標準ライブラリにはフォントが同梱されていませんが、`System::font` には何らかのフォントを指定しなければなりません。ゲームをビルドする環境にインストールされているフォントなら、そのまま指定できます。

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
const SANS: FontAsset = system_font("truetype/dejavu/DejaVuSans.ttf");
```

- 次のディレクトリを順に探し、最初に見つかったファイルを使います。
  - Windows：`%LOCALAPPDATA%\Microsoft\Windows\Fonts`、次に `%WINDIR%\Fonts`
  - macOS：`~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts`
  - その他：`$XDG_DATA_HOME/fonts`、`~/.fonts`、次に `$XDG_DATA_DIRS` の各ディレクトリの `fonts`
- どこにも見つからなければエラー [`E1030`](../diagnostics/modules.md#e1030) になります。
- `kagari.toml` で `[assets] system-fonts = false` とするとシステムフォントを使えなくなり、`system_font` はすべてエラー [`E1031`](../diagnostics/modules.md#e1031) になります。既定では使えます。
- `kagari build` はシステムフォントもほかのアセットと同じく `system-fonts.kiarchive` にまとめます。格納時のパスは、`system-fonts/` の後にソースで書いたパスを続けたものです。このため、アセットディレクトリ直下の `system-fonts` ディレクトリは予約されています（[`E1032`](../diagnostics/modules.md#e1032)）。
- システムフォントも、`asset("…")` で作ったものと同じ `FontAsset` 定数です。
