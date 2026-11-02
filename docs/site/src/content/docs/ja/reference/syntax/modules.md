---
title: "ソースファイルとモジュール"
---

Rust と同じく、ゲームは**モジュール**の木構造でできています。モジュールになるのは、ソースファイル、インラインの `mod { … }` ブロック、関数本体の中でアイテムを含むブロックの三つです。この章では、ファイル、モジュールの木、パス、可視性、`use`、標準ライブラリとプレリュード、アセットについて説明します。

## `.kis` と `.kix` {#kis-and-kix}

| 拡張子 | 書けるもの |
| ------ | ---- |
| `.kis` | コア言語 |
| `.kix` | コア言語と[組み込み構文](./forms.md)（`s!(…)`、`choice!{…}`、`t![…]`、`button!{…}` などの UI 構文） |

組み込み構文は普通の式を簡潔に書くための決まった構文です。そのため `.kix` ファイルに書けるものは、記述が長くなるだけで、すべて `.kis` ファイルでも書けます。組み込み構文はマクロではありません。Kagari にはマクロがなく、組み込み構文の種類は言語で決まっています。`.kis` ファイルで組み込み構文を使うと構文エラー（[`E0001`](../../diagnostics/syntax.md#e0001)）になります。

拡張子で決まるのは構文だけで、そのコードに何ができるかは決まりません。それを決めるのは[関数種別](../concepts/kinds.md)です。どの種別の関数も、どちらの拡張子のファイルにも書けます。慣習として、物語と UI は `.kix` ファイルに、組み込み構文を使わないコード（型、定数、補助関数、レンダー）は `.kis` ファイルに書きます。

## ルートファイル {#the-root-file}

コンパイラは一つの**ルートファイル**から読み込みを始めます。ルートファイルは `kagari.toml` の `project.entry` で指定し、既定は `src/main.kis` です（[プロジェクト](../tools/projects.md)を参照）。拡張子は `.kis` か `.kix` でなければなりません（[`E1001`](../../diagnostics/modules.md#e1001)）。ルートファイルはモジュール `crate` にあたり、ゲームシステムを記述する `fn main()` を宣言する必要があります（[`E7001`](../../diagnostics/emit.md#e7001)）。ほかのファイルはすべて `mod` 宣言をたどって読み込まれ、どの `mod` からも宣言されていないファイルはゲームに含まれません。

ゲームのファイルはすべてまとめてコンパイルされ、一つの `.kagari` ファイルになります。

## モジュール {#modules}

```kis
// src/main.kix: the root module
mod characters;      // a file module
mod story;
mod ui;
mod helpers {        // an inline module
    pub const fn twice(x: u32) -> u32 { x * 2 }
}

use characters::{HANAKO, AZUSA};
```

- `mod name;` は**ファイルモジュール**を宣言します。対応するファイルの探し方は次の節で説明します。
- `mod name { … }` は、波括弧の中のアイテムからなる**インラインモジュール**を宣言します。
- アイテムを含む関数本体やブロックは、名前のない**ブロックスコープ**になります（[ブロック内のアイテム](./items.md#items-in-blocks)を参照）。ブロックの中でファイルモジュールは宣言できません（[`E1005`](../../diagnostics/modules.md#e1005)）。
- アイテムは宣言より前の位置からでも使えます。

### モジュールのファイルの場所 {#where-the-file-of-a-module-is}

`mod name;` があると、コンパイラは宣言元モジュールの**子ディレクトリ**を探します。子ディレクトリは次のように決まります。

- ルートファイル、または `mod.kis`・`mod.kix` という名前のファイルでは、そのファイルがあるディレクトリ。
- それ以外のファイル `foo.kis`・`foo.kix` では、ファイルと同じ階層にあるディレクトリ `foo/`。
- インラインモジュール `mod a { … }` の中では、囲んでいるインラインモジュールの名前ごとに `a/` のようなディレクトリを付け足したもの。

子ディレクトリでは、`name.kis`、`name.kix`、`name/mod.kis`、`name/mod.kix` の四つを候補として探し、ちょうど一つだけが存在しなければなりません。一つもなければ [`E1002`](../../diagnostics/modules.md#e1002)、二つ以上あれば [`E1003`](../../diagnostics/modules.md#e1003)、すでに別のモジュールとして読み込んだファイルであれば [`E1004`](../../diagnostics/modules.md#e1004) のエラーになります。

<!-- diagram: modules-tree (mermaid-ish)
flowchart TD
  root["crate: src/main.kix（ルート）"] ==>|"mod characters;"| ch["crate::characters: src/characters.kis"]
  root ==>|"mod story;"| st["crate::story: src/story.kix"]
  root ==>|"mod ui;"| ui["crate::ui: src/ui/mod.kis"]
  st ==>|"mod day1;"| d1["crate::story::day1: src/story/day1.kix"]
  ui ==>|"mod menu;"| me["crate::ui::menu: src/ui/menu.kis"]
-->
{{#include ../diagrams/modules-tree.svg}}

この例では、`story.kix` が `mod day1;` を宣言しているので、子ディレクトリは `src/story/` です。一方 `ui/mod.kis` は `ui` モジュールそのもののファイルなので、子ディレクトリは `src/ui/` になります。

## パス {#paths}

**パス**はアイテムを指す名前で、`std::screen::size` や `Route::Hanako` のようにセグメントを `::` でつないで書きます。パスの先頭には次のキーワードを置けます。

| 先頭 | 指すもの |
| ---- | -------- |
| `crate` | ゲームのルートモジュール |
| `self` | 現在のモジュール |
| `super` | 親モジュール。`super::super` なら 2 階層上。ルートより上をたどるとエラー [`E1016`](../../diagnostics/modules.md#e1016) |
| `Self` | 現在の `impl` の対象の型（[メソッド](./functions.md#methods-and-associated-functions)を参照） |

パスを `::` から書き始めることはできません。ジェネリクス引数は、型の中では `Vec<u8>`、式の中ではターボフィッシュを使って `Vec::<u8>::new()` と書きます。

修飾のないパスの最初のセグメントは、次の順に探します。

1. 囲んでいるブロックスコープ（内側から順に）
2. 現在のモジュールのアイテムとインポート
3. 標準ライブラリ `std`（モジュールが独自の `std` を定義・インポートしていない場合）
4. [プレリュード](#the-prelude)
5. プリミティブ型 `bool`、`Int`、Rust の整数型

二つ目以降のセグメントは、そこまでのパスが指すモジュール、列挙型、型の中から探します。対象はモジュールのアイテム、列挙型のバリアント、型の関連関数と関連定数です。見つからなければエラー [`E1016`](../../diagnostics/modules.md#e1016) になります。

### 名前空間 {#namespaces}

モジュールごとに名前空間が二つあります。

| 名前空間 | 含まれるもの |
| -------- | -------- |
| 型 | モジュール、構造体、列挙型、型エイリアス、トレイト |
| 値 | 関数、定数、static、タプル構造体とユニット構造体のコンストラクター |

同じ名前は、名前空間ごとに一度ずつ使えます。同じ名前空間で二度定義するとエラー [`E1014`](../../diagnostics/modules.md#e1014) です。名前空間が別なので、定数とモジュールに同じ名前を付けられます。次の例では、`AZUSA` はエンティティ ID、`AZUSA::CLOSE_EYE_SMILE` はアズサの画像の一つです。

```kis
pub const AZUSA: EntityId = EntityId(217);

pub mod AZUSA {
    pub const CLOSE_EYE_SMILE: ImageAsset = asset("azusa/close_eye_smile.png");
}
```

## 可視性 {#visibility}

`pub` を付けないアイテムは**プライベート**です。プライベートなアイテムは、定義したモジュールと、その内側に入れ子になったモジュールからだけ見えます。それ以外の場所から名前で参照するとエラー [`E1017`](../../diagnostics/modules.md#e1017) になります。`pub(crate)` のような範囲を限定した書き方はありません。

- `impl` の関連関数と関連定数にも同じ規則が当てはまり、`pub` を付けなければプライベートです。
- 列挙型のバリアントは、列挙型と同じ範囲から見えます。
- 構造体のフィールドにも、標準ライブラリと同様に `pub` を付けられます。ただし、コンパイラはまだフィールドの可視性を検査しません。構造体が見える場所なら、どのフィールドも読み書きできます。

## `use` 宣言 {#use-declarations}

`use` は、名前を現在のモジュールやブロックに取り込みます。

| 書き方 | 効果 |
| -- | ---- |
| `use std::screen::size;` | `size` をインポート |
| `use characters::{HANAKO, AZUSA};` | 複数の名前をまとめてインポート。グループは入れ子にできます |
| `use crate::widen as w;` | 別名でインポート |
| `use crate::widen as _;` | 名前を束縛せずにインポート |
| `use Route::*;` | グロブインポート。モジュールの見える名前すべて、または列挙型のバリアントすべて |
| `pub use self::text::Text;` | 再エクスポート。外からはこのモジュールの名前として見えます |

- インポートは、その名前があるすべての名前空間からアイテムを取り込みます。`use characters::AZUSA;` なら定数とモジュールの両方が入ります。
- `use` のパスにはジェネリクス引数を書けません。
- モジュール内で定義した名前や、名前を指定してインポートした名前は、グロブインポートより優先されます。
- 二つのグロブインポートが同じ名前で別々のアイテムを取り込むと、その名前を使った時点でエラー [`E1018`](../../diagnostics/modules.md#e1018) になります。
- どこも指していないインポートはエラー [`E1015`](../../diagnostics/modules.md#e1015)、プライベートなアイテムのインポートはエラー [`E1017`](../../diagnostics/modules.md#e1017) です。

## 標準ライブラリ {#the-standard-library}

標準ライブラリはモジュール `std` です。各アイテムの説明は[標準ライブラリのリファレンス](../../std/std.md)にあります。

| モジュール | 内容 | 標準ライブラリ |
| ---------- | ---- | ---------------- |
| `std` | `String`、`Vec`、`Range`、`Rat`、`Decimal`、`Option`、`Result`、トレイト `Eq`・`Integer`・`FromInteger`・`RandChoose`・`Asset`、アセット型、`asset`、`system_font` | [std](../../std/std.md) |
| `std::prelude` | すべてのモジュールにインポートされる名前 | [std::prelude](../../std/prelude.md) |
| `std::time` | `Duration`（ミリ秒単位の整数）、`ms`、`secs` | [std::time](../../std/time.md) |
| `std::narrative` | ステップ（`Step`、`push_step`、`Say`、`Choice`）、`rng_choose`、メタデータのフィールド、`Narrative` ハンドル | [std::narrative](../../std/narrative.md) |
| `std::narrative::text` | `Text`、テキスト効果、`ToText`、テキストのスタイル | [std::narrative::text](../../std/narrative-text.md) |
| `std::screen` | 画面（エンティティ、プロパティ、レイヤー、Hud、カメラ、フィルター） | [std::screen](../../std/screen.md) |
| `std::screen::anim` | 補間、トランジション、モーション、イージング | [std::screen::anim](../../std/screen-anim.md) |
| `std::screen::audio` | 音楽・環境音のチャンネル、効果音、音声のフェード | [std::screen::audio](../../std/screen-audio.md) |
| `std::stage` | `std::screen` の上に構築したキャラクター、スロット、サイドフェイス | [std::stage](../../std/stage.md) |
| `std::system` | `System`、設定、既読の集合、入力の割り当て | [std::system](../../std/system.md) |
| `std::system::nav` | モード間の移動、物語の開始とロード | [std::system::nav](../../std/system-nav.md) |
| `std::system::saves` | セーブスロットとセーブ情報 | [std::system::saves](../../std/system-saves.md) |
| `std::ui` | UI 要素、シグナル、UI 構文が呼び出す関数 | [std::ui](../../std/ui.md) |
| `std::ui::style` | UI コントロールの見た目 | [std::ui::style](../../std/ui-style.md) |

## プレリュード {#the-prelude}

**プレリュード**は、すべてのモジュールに自動でインポートされる名前の集まりです。モジュール自身のアイテムやインポートがあれば、そちらが優先されます。内容は次のとおりです。

| 元のモジュール | 名前 |
| -------------- | ---- |
| `std` | `String`、`Vec`、`Rat`、`Decimal`、`Range`、`Option`、`Result`、`Eq`、`Integer`、`RandChoose`、`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset`、`asset`、`system_font`、バリアント `None`・`Some`・`Ok`・`Err` |
| `std::time` | `Duration`、`ms`、`secs` |
| `std::screen` | `Screen`、`Render`、`EntityId`、`Part`、`Point`、`Offset`、`Size`、`Anchor`、`Color`、`Plane`、`Visibility`、`point`、`offset`、`rgb`、`hex`、`BLACK`、`WHITE`、`TRANSPARENT` |
| `std::screen::anim` | `Easing`、`Tween`、`Transition`、`Motion`、`tween`、`instant`、`cut`、`fade`、`rule`、`wipe`、`through`、`motion`、`jump`、`nod`、`shake`、`quake` |
| `std::screen::audio` | `looped`、`sound`、`crossfade` |
| `std::stage` | `Character`、`character` |
| `std::narrative` | `Text`、`TextEffect`、`Say`、`Choice`、`ChoiceArm`、`push_step`、`rng_choose`、`Meta`、`meta`、`Narrative` |
| `std::narrative::text` | `emphasis`、`dots`、`color_text`、`bold`、`pause`、`click_wait`、`mark` |
| `std::system` | `nav`、`Setting`、`setting`、`System`、`Settings`、`Volume`、`volume`、`RootPage`、`Overlay`、`GameOverlay`、`Dialog` |
| `std::ui` | `signal`、`Element` |

これ以外の名前は、`use std::screen::size;` のように `use` でインポートします。

## アセット {#assets}

**アセット**とは、定数に結び付けて使うゲーム用のファイルです。

```kis
const CG_001: ImageAsset = asset("cg/cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
```

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

トレイト `Asset` を実装している型は次のとおりです。

| 型 | ファイル |
| -- | -------- |
| `ImageAsset` | 画像（PNG、JPEG、WebP） |
| `AudioAsset` | 音楽または効果音 |
| `VoiceAsset` | 台詞に対応するボイスファイル |
| `VideoAsset` | 動画 |
| `FontAsset` | フォント |
| `ColorShader<A>`、`MixShader<A>`、`TextureShader<A>`、`MaskShader<A>`、`LocalMotionShader<A>`、`TransitionalMotionShader<A>` | 役割ごとの WGSL シェーダー（`.wgsl`）。引数の型は `A`。[`std::screen::shader`](../../std/screen-shader.md) を参照 |

`A` は通常、定数の型から決まります。`asset::<ImageAsset>("…")` のように明示しても構いません。パスには次の規則があります。

- 文字列リテラルであること（[`E2028`](../../diagnostics/types.md#e2028)）。空文字列は不可（[`E1028`](../../diagnostics/modules.md#e1028)）。
- **アセットディレクトリ**からの相対パスであること。アセットディレクトリは `kagari.toml` の `[assets] dir` で指定し、既定はプロジェクトディレクトリ内の `assets` です。区切り文字には `/` と `\` のどちらも使えます。
- アセットディレクトリの外を指さないこと。絶対パス、ドライブ指定（`C:`）、ディレクトリの外に出る `..` は使えません（[`E1029`](../../diagnostics/modules.md#e1029)）。
- 最上位の `system-fonts` と `std` ディレクトリの中を指さないこと。この二つはシステムフォントと標準ライブラリ自身のアセット用に予約されています（[`E1032`](../../diagnostics/modules.md#e1032)）。
- ゲームのコンパイル時にファイルが存在すること（[`E1027`](../../diagnostics/modules.md#e1027)）。ファイルがなければ、プレイ中ではなくコンパイル時にエラーになります。

ファイルのアセット型は[有限型](../concepts/kinds.md#finite-types)で、取りうる値はプログラム中のアセット定数そのものです。そのため物語の中で、ルートに応じてボイスを選ぶといったように、実行時にアセットを選べます。二つのアセット値は、同じファイルを指していれば等しくなります。シェーダー型は有限型ではなく、レンダーの中で使います。

### システムフォント {#system-fonts}

ビルドするマシンにインストールされているフォントは、アセットディレクトリにコピーしなくても定数に結び付けられます。

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

パスは、システムフォントのディレクトリからの相対パスを表す空でない文字列リテラルで、そのディレクトリの外を指してはいけません。ディレクトリは次の順に探し、最初にファイルが見つかったものを使います。

| プラットフォーム | ディレクトリ |
| ---------------- | ------------ |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`、`%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts` |
| Linux など | `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、続いて `$XDG_DATA_DIRS`（既定は `/usr/local/share:/usr/share`）の各ディレクトリ内の `fonts` |

フォントが見つからなければエラー [`E1030`](../../diagnostics/modules.md#e1030) になります。`kagari.toml` で `[assets] system-fonts = false` を指定すると、システムフォントは使えなくなります（[`E1031`](../../diagnostics/modules.md#e1031)）。標準ライブラリにはフォントが同梱されていないため、既定のフォント `System::font` は必ず指定してください。

### ビルド時のアセット {#assets-in-the-build}

`kagari build` は、プログラムが使うアセットファイルをすべて、`.kagari` ファイルと同じ場所のアーカイブ（`.kiarchive`）にまとめます。各ファイルはアセットディレクトリからの相対パス（区切りは `/`）で格納し、そのパスの最初のディレクトリ名をとったアーカイブに入れます。

| アセットのパス | アーカイブ |
| ------------ | ---------- |
| `bg/classroom.png` | `bg.kiarchive` |
| `cg/cg_001.png` | `cg.kiarchive` |
| `logo.png`（アセットディレクトリの直下） | `data.kiarchive` |
| システムフォント `msgothic.ttc` | `system-fonts.kiarchive`（`system-fonts/msgothic.ttc` として格納） |
| `std` のシェーダー（`std::shaders::color::BLUR` など） | `std.kiarchive`（`std/shaders/color/blur.wgsl` として格納） |

したがって、最上位ディレクトリでのファイルの分け方によって、ゲームのデータがどのアーカイブに分かれるかが決まります。ビルドの出力については [`kagari` CLI](../tools/cli.md) を参照してください。
