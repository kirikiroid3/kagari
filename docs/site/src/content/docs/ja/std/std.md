---
title: "std"
---

> **安定性:** 安定 — ビルトイン型、`Range`、`Option` と `Result`、std のトレイト、アセット型。

クレートのルート `std` は、すべてのプログラムが必要とする型を宣言します：実行時の文字列とリスト、正確な数値、範囲、ジェネリックな直和型 `Option` と `Result`、std のシグネチャを書くためのトレイト、そしてアセット型と `asset`・`system_font` です。ルートの関数はすべて `const fn` です。

ルートはサブモジュールも宣言します。

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

`Int<lo, hi>`、Rust の整数型（`u8` … `u128`、`i8` … `i128`、`usize`、`isize`）、`bool` は言語の型で、std のアイテムではありません。`Int<lo, hi>` は整数 `lo..=hi` を保持し、物語が実行時に保持できる唯一の整数型です。[数値](../reference/syntax/numbers.md) を参照してください。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`String`](#struct-string)|ビルトイン構造体|実行時の文字列|
|[`Vec<T>`](#struct-vect)|ビルトイン構造体|伸長可能なリスト|
|[`Rat`](#struct-rat)|ビルトイン構造体|正確な有理数|
|[`Decimal`](#struct-decimal)|ビルトイン構造体|正確な十進小数|
|[`Range<T>`](#struct-ranget)|構造体|`start..end` と `start..=end`|
|[`Option<T>`](#enum-optiont)|列挙型|省略可能な値|
|[`Result<T, E>`](#enum-resultt-e)|列挙型|成功値またはエラー値|
|[`Eq`](#trait-eq)|ビルトイントレイト|構造的な等価性|
|[`Integer`](#trait-integer)|ビルトイントレイト|整数型|
|[`RandChoose`](#trait-randchoose)|ビルトイントレイト|物語が乱択できる型|
|[`Asset`](#trait-asset)|ビルトイントレイト|アセット型|
|[`ImageAsset` … `ShaderAsset`](#asset-types)|ビルトイン構造体|アセットファイルの種類ごとの型|
|[`asset`](#function-assetpath)|ビルトイン `const fn`|プログラムのアセットを読み込む|
|[`system_font`](#function-system_fontpath)|ビルトイン `const fn`|ビルドするマシンにインストールされたフォントを読み込む|

## Struct: `String` {#struct-string}

```kis
#[builtin] pub struct String;
```

`fn` のコードが組み立てる実行時の文字列です。文字列リテラルの型は `String` ではなく `Text` です。`String` は `fn` のコードが実行時に作るものです。`String` は `Eq` であり、`t![…]` に埋め込めます（`ToText` です）。有限型ではないので、物語は保持できません。

- Kind: — · Builtin: yes

## Struct: `Vec<T>` {#struct-vect}

```kis
#[builtin] pub struct Vec<T>;
```

伸長可能なリストです。`Vec` は他のすべての型と同じく値型です：`push` と `set` は呼び出された場所を変更し、コピーには影響しません。`T` が `Eq` なら `Vec<T>` も `Eq` です。`Vec` に対する `for x in v` はメソッドではなく言語の規則です（[コンパイラ規則](./appendix/rules.md#for-loops) を参照）。`Vec` は有限型ではありません。

- Kind: — · Builtin: yes

### `Vec::new()` {#vecnew}

```kis
#[builtin] pub const fn new() -> Vec<T>;
```

- Returns: `Vec<T>` — 空のリスト。
- Kind: `const fn` · Builtin: yes

### `Vec::len(&self)` {#veclenself}

```kis
#[builtin] pub const fn len(&self) -> usize;
```

- Returns: `usize` — 要素の数。
- Kind: `const fn` · Builtin: yes

### `Vec::is_empty(&self)` {#vecis_emptyself}

```kis
#[builtin] pub const fn is_empty(&self) -> bool;
```

- Returns: `bool` — 要素がないかどうか。
- Kind: `const fn` · Builtin: yes

### `Vec::get(&self, index)` {#vecgetself-index}

```kis
#[builtin] pub const fn get(&self, index: usize) -> Option<T>;
```

- `index` `usize` — 0 から数えた位置。
- Returns: `Option<T>` — `index` の要素。`index` が範囲外なら `None`。
- Kind: `const fn` · Builtin: yes

### `Vec::set(&mut self, index, value)` {#vecsetmut-self-index-value}

```kis
#[builtin] pub const fn set(&mut self, index: usize, value: T);
```

- `index` `usize` — 置き換える位置。
- `value` `T` — 新しい要素。
- Kind: `const fn` · Builtin: yes

`index` の要素を置き換えます。範囲外の `index` は評価エラーで、定数評価では [`E4005`](../diagnostics/eval.md#e4005) です。

### `Vec::push(&mut self, value)` {#vecpushmut-self-value}

```kis
#[builtin] pub const fn push(&mut self, value: T);
```

- `value` `T` — 追加する要素。
- Kind: `const fn` · Builtin: yes

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

## Struct: `Rat` {#struct-rat}

```kis
#[builtin] pub struct Rat;
```

正確な有理数です。`Rat` にはリテラルがないので、`Rat::new` で作ります。`Rat` は `Eq` かつ `ToText`（正確に書き出されます）です。有限型ではありません。

- Kind: — · Builtin: yes

### `Rat::new(num, den)` {#ratnewnum-den}

```kis
#[builtin] pub const fn new(num: i128, den: i128) -> Rat;
```

- `num` `i128` — 分子。
- `den` `i128` — 分母。
- Returns: `Rat` — 既約分数にした `num / den`。
- Kind: `const fn` · Builtin: yes

`den == 0` はゼロ除算です（定数評価では [`E4002`](../diagnostics/eval.md#e4002)）。`Rat::new(1, 3)` は 3 分の 1 です。

### `Rat::to_decimal(self)` {#ratto_decimalself}

```kis
#[builtin] pub const fn to_decimal(self) -> Decimal;
```

- Returns: `Decimal` — 整数部と小数点以下最大 7 桁を持つこの値。ゼロ方向に切り捨てます。
- Kind: `const fn` · Builtin: yes

`Rat::new(1, 3).to_decimal()` は `0.3333333` です。

## Struct: `Decimal` {#struct-decimal}

```kis
#[builtin] pub struct Decimal;
```

`0.5` のようなリテラルで書く正確な十進小数です。浮動小数点数の正確な代替（Kagari に浮動小数点型はありません）であり、`std` のすべての割合の型です。`Decimal / Decimal` は正確に計算したあと、小数点以下 7 桁にゼロ方向で切り捨てます。`Decimal` は `Eq` かつ `ToText`（正確に書き出されます）です。有限型ではありません。

- Kind: — · Builtin: yes

## Struct: `Range<T>` {#struct-ranget}

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
- Kind: — · Builtin: no

`a..b` と `a..=b` は `Range` の値です。式では両端が必要で、`a..`、`..b`、`..` はパターンにのみ書けます。`0..7` は `Range<Int<0, 7>>` です。型付けは [コンパイラ規則](./appendix/rules.md#type-rules)、反復は [`for` ループ](./appendix/rules.md#for-loops) を参照してください。

## Enum: `Option<T>` {#enum-optiont}

```kis
pub enum Option<T> {
    None,
    Some(T),
}
```

省略可能な値です。プレリュードは型とそのバリアント `None`、`Some` をインポートします。

- Kind: — · Builtin: no

## Enum: `Result<T, E>` {#enum-resultt-e}

```kis
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

成功値またはエラー値です。プレリュードは型とそのバリアント `Ok`、`Err` をインポートします。`nav::load` は `Result` を返します。

- Kind: — · Builtin: no

## Trait: `Eq` {#trait-eq}

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

値を `==` と `!=` で比較できる型です。`ui::select` の境界です。

- Kind: — · Builtin: yes

`Eq` は、プリミティブ型、`Int`、関数ポインタ、そしてフィールドがすべて `Eq` の構造体・列挙型・タプル・配列に自動で付きます。`#[builtin]` 型には見えるフィールドがないため、`std` が上の `impl` で明示します：`String`、`Eq` な `T` の `Vec<T>`、`Rat`、`Decimal`、アセット型、`Text`、`Screen` は `Eq` です。ハンドルや保留中の変更（`Setting`、`Meta`、`Narrative`、`Entity`、`Prop`、`Change`、レイヤー、`Swap`、`Channel`、`ChannelChange`、`Element` など）とクロージャは `Eq` ではありません。`Eq` でない型の値の比較は [`E2017`](../diagnostics/types.md#e2017) です。

## Trait: `Integer` {#trait-integer}

```kis
#[builtin] pub trait Integer {
    const fn into<T: Integer>(self) -> T;
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

整数型：`Int<lo, hi>` と Rust の整数です。

- Kind: — · Builtin: yes

### `Integer::into(self)` {#integerintoself}

```kis
const fn into<T: Integer>(self) -> T;
```

- Returns: `T` — `T` としてのこの値。
- Kind: `const fn` · Builtin: yes

損失のない変換です。`Self` のすべての値が `T` に収まらない限りコンパイルエラー（[`E2018`](../diagnostics/types.md#e2018)）です：`Int<0, 5>` は `u8` や `Int<0, 10>` に変換できますが、`u8` から `Int<0, 5>` への変換はコンパイルできません。

### `Integer::saturate(self)` {#integersaturateself}

```kis
const fn saturate<T: Integer>(self) -> T;
```

- Returns: `T` — `T` の範囲に丸め込んだこの値。
- Kind: `const fn` · Builtin: yes

`saturate` に収まる条件はありません：`300u32.saturate()` を `u8` にすると `255` です。

```kis
const fn level(n: u32) -> Int<0, 10> {
    n.saturate()
}
```

## Trait: `RandChoose` {#trait-randchoose}

```kis
#[builtin] pub trait RandChoose {}
impl<const LO: i64, const HI: i64> RandChoose for Int<LO, HI> {}
```

物語が [`narrative::rng_choose`](./narrative.md#function-rng_choose) で乱択できる型です：フィールドを持たない列挙型（暗黙に）と `Int<lo, hi>`。

- Kind: — · Builtin: yes

## Trait: `Asset` {#trait-asset}

```kis
#[builtin] pub trait Asset {}
impl Asset for ImageAsset {}
impl Asset for AudioAsset {}
impl Asset for VoiceAsset {}
impl Asset for VideoAsset {}
impl Asset for FontAsset {}
```

値がちょうどプログラムのアセット定数である型です。[`asset`](#function-assetpath) の境界です。下の 5 つのファイル型と、[`std::screen::shader`](./screen-shader.md) の 6 つのシェーダー型（`ColorShader<A>` など）が実装しています。

- Kind: — · Builtin: yes

`std` のトレイトはすべて `#[builtin]` で、トレイトを宣言・実装できるのは `std` だけです。ゲーム内の `trait` アイテムや `impl Trait for T` はエラーです。[コンパイラ規則](./appendix/rules.md#traits) を参照してください。

## アセット型 {#asset-types}

```kis
#[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
#[builtin] pub struct VoiceAsset;
#[builtin] pub struct VideoAsset;
#[builtin] pub struct FontAsset;
#[builtin] pub struct ShaderAsset;
```

アセットファイルの種類ごとの不透明な型です。6 つすべてが `Eq` です。最初の 5 つは `Asset` で、有限型です：アセット型の値はちょうどその定数です。

- Kind: — · Builtin: yes

|型|ファイル|使用箇所|
|---|---|---|
|`ImageAsset`|画像：PNG、JPEG、WebP|画像レイヤー、ルールマスク、サイド画像、UI の画像とペイント|
|`AudioAsset`|音楽または効果音のファイル|音楽と環境音のループ、ワンショットのサウンド|
|`VoiceAsset`|行に結び付いたボイスファイル|`Say::voice`、`Narrative::replay_voice`|
|`VideoAsset`|動画ファイル|ムービーレイヤー|
|`FontAsset`|フォントファイル|`System::font`、`TextEffect::font`、`TextStyle::font`|
|`ShaderAsset`|任意のロールのシェーダーファイルの識別子。効果値が保持する|[`std::screen::shader`](./screen-shader.md) の効果値|

`ShaderAsset` は `Asset` ではなく、プレリュードにもありません。ゲームはシェーダーを `ColorShader<A>` などロールの型で宣言し、`ShaderAsset` を名前で書くことはありません。

ランタイムがどのファイル形式をデコードできるかは [ランタイムプロファイル](./appendix/profiles.md) に記載しています。

- `VideoAsset` — Profile: not in `native_v0`。

## Function: `asset(path)` {#function-assetpath}

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

- `path` `Text` — プログラムのアセットディレクトリからの相対パスを表す文字列リテラル。
- Returns: `A` — アセット。
- Kind: `const fn` · Builtin: yes

アセットは定数として宣言します。

```kis
const CG_001: ImageAsset = asset("cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
const THEME: AudioAsset = asset("bgm/theme.ogg");
```

- 型引数 `A` は定数の型から推論され、アセット型でなければなりません（そうでなければ [`E2026`](../diagnostics/types.md#e2026)）。
- 引数は文字列リテラルでなければならず（[`E2028`](../diagnostics/types.md#e2028)）、空であってはなりません（[`E1028`](../diagnostics/modules.md#e1028)）。
- パスはアセットディレクトリ（`kagari.toml` の `[assets] dir`、既定は `assets`。マニフェストがなければルートファイルのディレクトリ）からの相対パスで、そこから外へ出てはなりません（[`E1029`](../diagnostics/modules.md#e1029)）。`std` の中では、パスは `std` 自身からの相対パスで、`std` は [`std::shaders`](./shaders.md) のファイルを同梱しています。
- ファイルがなければコンパイルエラーです（[`E1027`](../diagnostics/modules.md#e1027)）。表情、曲、ボイスは定数なので、名前の綴り間違いは実行時の失敗ではなく名前解決のエラーになります。
- アセットディレクトリ直下の `system-fonts` と `std` ディレクトリは、システムフォントと std のアセットのために予約されています（[`E1032`](../diagnostics/modules.md#e1032)）。
- `kagari build` は、プログラムが使うすべてのアセットの形式をプロジェクトのランタイムプロファイルに照らして検査し、すべてのシェーダーをそのロールに照らして検査します。[ランタイムプロファイル](./appendix/profiles.md) と [シェーダーの検査](./guides/shaders.md#checking-shaders) を参照してください。

## Function: `system_font(path)` {#function-system_fontpath}

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

- `path` `Text` — システムフォントディレクトリのいずれかからの相対パスを表す文字列リテラル。
- Returns: `FontAsset` — ゲームに同梱されるフォント。
- Kind: `const fn` · Builtin: yes

標準ライブラリはフォントを同梱しておらず、`System::font` には何かのフォントを指定する必要があります。ゲームをビルドするマシンにインストールされたフォントは、直接指定できます。

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
const SANS: FontAsset = system_font("truetype/dejavu/DejaVuSans.ttf");
```

- ディレクトリは順に探索され、最初にファイルが見つかったものが使われます。
  - Windows：`%LOCALAPPDATA%\Microsoft\Windows\Fonts`、次に `%WINDIR%\Fonts`
  - macOS：`~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts`
  - それ以外：`$XDG_DATA_HOME/fonts`、`~/.fonts`、次に `$XDG_DATA_DIRS` の各ディレクトリの `fonts`
- どこにも見つからないフォントは [`E1030`](../diagnostics/modules.md#e1030) です。
- `kagari.toml` の `[assets] system-fonts = false` はシステムフォントを禁止します。そのとき `system_font` はすべて [`E1031`](../diagnostics/modules.md#e1031) です。既定ではシステムフォントを使えます。
- `kagari build` はシステムフォントを他のアセットと同じように `system-fonts.kiarchive` に詰め、パスは `system-fonts/` にソースに書いたパスを続けたものになります。そのため、アセットディレクトリ直下の `system-fonts` ディレクトリは予約されています（[`E1032`](../diagnostics/modules.md#e1032)）。
- システムフォントは、`asset("…")` で作ったものと同じ `FontAsset` 定数です。
