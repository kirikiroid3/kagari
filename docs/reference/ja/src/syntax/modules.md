# ソースファイルとモジュール {#source-files-and-modules}

ゲームは Rust と同じく**モジュール**のツリーです。各モジュールは、ソースファイル、インラインの `mod { … }` ブロック、または関数本体の中でアイテムを含むブロックのいずれかです。この章では、ファイル、モジュールツリー、パス、可視性、`use`、標準ライブラリとそのプレリュード、アセットを説明します。

## `.kis` と `.kix` {#kis-and-kix}

| 拡張子 | 内容 |
| ------ | ---- |
| `.kis` | コア言語 |
| `.kix` | コア言語に加えて[フォーム](./forms.md)。`s!(…)`、`choice!{…}`、`t![…]`、および `button!{…}` などの UI フォーム |

フォームは普通の式を表す固定の構文なので、`.kix` ファイルに書けることはすべて、より冗長になりますが `.kis` ファイルでも書けます。フォームはマクロではありません。Kagari にマクロはなく、フォームの集合は言語で固定されています。`.kis` ファイルの中のフォームは構文エラーです（[`E0001`](../diagnostics/syntax.md#e0001)）。

拡張子が決めるのは構文だけです。コードに何ができるかは決めません。それは[関数種別](../concepts/kinds.md)の役割です。どの種別の関数もどちらのファイルにも書けます。慣習として、物語と UI は `.kix` ファイルに、フォームを使わないコード（型、定数、補助関数、レンダー関数）は `.kis` ファイルに書きます。

## ルートファイル {#the-root-file}

コンパイラは 1 つの**ルートファイル**から始めます。`kagari.toml` の `project.entry` で、既定は `src/main.kis` です（[プロジェクト](../tools/projects.md)を参照）。ルートファイルは `.kis` か `.kix` でなければなりません（[`E1001`](../diagnostics/modules.md#e1001)）。ルートファイルはモジュール `crate` であり、ゲームシステムを記述する `fn main()` を宣言しなければなりません（[`E7001`](../diagnostics/emit.md#e7001)）。他のファイルはすべて `mod` 宣言を通じてたどられます。どの `mod` からも宣言されないファイルはゲームの一部ではありません。

ゲームのすべてのファイルはまとめてコンパイルされ、1 つの `.kagari` ファイルになります。

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

- `mod name;` は**ファイルモジュール**を宣言します。そのファイルは次の節のとおりに探されます。
- `mod name { … }` は、波括弧の中のアイテムからなる**インラインモジュール**を宣言します。
- アイテムを含む関数本体やブロックは名前のない**ブロックスコープ**です。[ブロック内のアイテム](./items.md#items-in-blocks)を参照してください。ファイルモジュールはブロックの中では宣言できません（[`E1005`](../diagnostics/modules.md#e1005)）。
- アイテムは宣言より前で使えます。

### モジュールのファイルの場所 {#where-the-file-of-a-module-is}

`mod name;` に対して、コンパイラは宣言したモジュールの**子ディレクトリ**を探します。

- ルートファイル、または `mod.kis` か `mod.kix` という名前のファイルでは、子ディレクトリはそのファイルのディレクトリです。
- それ以外のファイル `foo.kis` や `foo.kix` では、ファイルの隣のディレクトリ `foo/` です。
- インラインモジュール `mod a { … }` の中では、囲んでいる各インラインモジュールの名前（`a/`）が付け加わります。

子ディレクトリの中で、`name.kis`、`name.kix`、`name/mod.kis`、`name/mod.kix` の 4 つの候補を試します。ちょうど 1 つが存在しなければなりません。1 つもなければ [`E1002`](../diagnostics/modules.md#e1002)、2 つ以上あれば [`E1003`](../diagnostics/modules.md#e1003)、すでに別のモジュールとして読み込まれたファイルなら [`E1004`](../diagnostics/modules.md#e1004) です。

<!-- diagram: modules-tree (mermaid-ish)
flowchart TD
  root["crate: src/main.kix（ルート）"] ==>|"mod characters;"| ch["crate::characters: src/characters.kis"]
  root ==>|"mod story;"| st["crate::story: src/story.kix"]
  root ==>|"mod ui;"| ui["crate::ui: src/ui/mod.kis"]
  st ==>|"mod day1;"| d1["crate::story::day1: src/story/day1.kix"]
  ui ==>|"mod menu;"| me["crate::ui::menu: src/ui/menu.kis"]
-->
{{#include ../diagrams/modules-tree.svg}}

ここで `story.kix` は `mod day1;` を宣言しているので、その子ディレクトリは `src/story/` です。`ui/mod.kis` は `ui` 自身のファイルなので、その子ディレクトリは `src/ui/` です。

## パス {#paths}

**パス**はアイテムを指す名前で、`std::screen::size` や `Route::Hanako` のように `::` で区切ったセグメントからなります。パスは次のキーワードで始めることができます。

| 先頭 | 指すもの |
| ---- | -------- |
| `crate` | ゲームのルートモジュール |
| `self` | 現在のモジュール |
| `super` | 親モジュール。`super::super` で 2 階層上がります。ルートより上に行くと [`E1016`](../diagnostics/modules.md#e1016) です |
| `Self` | 現在の `impl` の型（[メソッド](./functions.md#methods-and-associated-functions)を参照） |

パスを `::` で始めることはできません。ジェネリクス引数は、型の中では `Vec<u8>`、式の中ではターボフィッシュを使って `Vec::<u8>::new()` と書きます。

普通のパスの最初のセグメントは次の順に探されます。

1. 囲んでいるブロックスコープ（内側から順に）
2. 現在のモジュールのアイテムとインポート
3. 標準ライブラリ `std`（モジュールが自分で `std` を定義またはインポートしていない場合）
4. [プレリュード](#the-prelude)
5. プリミティブ型 `bool`、`Int`、Rust の整数型

2 番目以降のセグメントは、それまでのパスが指すモジュール、列挙型、型の中で探されます。モジュールのアイテム、列挙型のバリアント、型の関連関数と関連定数です。見つからない名前は [`E1016`](../diagnostics/modules.md#e1016) です。

### 名前空間 {#namespaces}

各モジュールには 2 つの名前空間があります。

| 名前空間 | 含むもの |
| -------- | -------- |
| 型 | モジュール、構造体、列挙型、型エイリアス、トレイト |
| 値 | 関数、定数、static、タプル構造体とユニット構造体のコンストラクタ |

1 つの名前は各名前空間で 1 回ずつ使えます。同じ名前空間での 2 つ目の定義は [`E1014`](../diagnostics/modules.md#e1014) です。名前空間が分かれているので、定数とモジュールは同じ名前を共有できます。`AZUSA` はエンティティ ID であり、`AZUSA::CLOSE_EYE_SMILE` は彼女の画像の 1 つです。

```kis
pub const AZUSA: EntityId = EntityId(217);

pub mod AZUSA {
    pub const CLOSE_EYE_SMILE: ImageAsset = asset("azusa/close_eye_smile.png");
}
```

## 可視性 {#visibility}

アイテムは `pub` が付いていなければ**プライベート**です。プライベートなアイテムは、それを定義したモジュールと、その中に入れ子になったすべてのモジュールから見えます。それ以外の場所から名前で参照すると [`E1017`](../diagnostics/modules.md#e1017) です。`pub(crate)` のような制限付きの形はありません。

- `impl` の関連関数と関連定数も同じ規則に従い、`pub` が付いていなければプライベートです。
- 列挙型のバリアントは列挙型と同じだけ見えます。
- 構造体のフィールドには標準ライブラリと同じく `pub` を付けられます。ただしコンパイラは現在フィールドの可視性を検査しません。構造体が見える場所ならどこでも、すべてのフィールドを読み書きできます。

## `use` 宣言 {#use-declarations}

`use` は名前を現在のモジュールまたはブロックに持ち込みます。

| 形 | 効果 |
| -- | ---- |
| `use std::screen::size;` | `size` をインポートします |
| `use characters::{HANAKO, AZUSA};` | 複数の名前をインポートします。グループは入れ子にできます |
| `use crate::widen as w;` | 別の名前でインポートします |
| `use crate::widen as _;` | 名前を束縛せずにインポートします |
| `use Route::*;` | グロブインポート。モジュールの見えるすべての名前、または列挙型のすべてのバリアント |
| `pub use self::text::Text;` | 再エクスポート。その名前がこのモジュールの一部として他から見えます |

- インポートは、その名前を持つすべての名前空間からアイテムを取り込みます。`use characters::AZUSA;` は定数とモジュールの両方をインポートします。
- `use` のパスにジェネリクス引数は書けません。
- モジュール内で定義した名前や名前を指定してインポートした名前は、グロブインポートより優先されます。
- 2 つのグロブインポートが 1 つの名前に別々のアイテムを持ち込んだ場合、その名前を使うと [`E1018`](../diagnostics/modules.md#e1018) です。
- どこにもたどり着かないインポートは [`E1015`](../diagnostics/modules.md#e1015)、プライベートなアイテムのインポートは [`E1017`](../diagnostics/modules.md#e1017) です。

## 標準ライブラリ {#the-standard-library}

標準ライブラリはモジュール `std` です。そのアイテムは [STD リファレンス](../../../../std/ja/src/std.md)で説明しています。

| モジュール | 内容 | STD リファレンス |
| ---------- | ---- | ---------------- |
| `std` | `String`、`Vec`、`Range`、`Rat`、`Decimal`、`Option`、`Result`。トレイト `Eq`、`Integer`、`RandChoose`、`Asset`。アセット型、`asset`、`system_font` | [std](../../../../std/ja/src/std.md) |
| `std::prelude` | すべてのモジュールにインポートされる名前 | [std::prelude](../../../../std/ja/src/prelude.md) |
| `std::time` | `Duration`（ミリ秒単位の整数）、`ms`、`secs` | [std::time](../../../../std/ja/src/time.md) |
| `std::narrative` | ステップ（`Step`、`push_step`、`Say`、`Choice`）、`rng_choose`、メタフィールド、`Narrative` ハンドル | [std::narrative](../../../../std/ja/src/narrative.md) |
| `std::narrative::text` | `Text`、テキスト効果、`ToText`、テキストスタイル | [std::narrative::text](../../../../std/ja/src/narrative-text.md) |
| `std::screen` | 画面。エンティティ、プロパティ、レイヤー、Hud、カメラ、フィルター | [std::screen](../../../../std/ja/src/screen.md) |
| `std::screen::anim` | トゥイーン、トランジション、モーション、イージング | [std::screen::anim](../../../../std/ja/src/screen-anim.md) |
| `std::screen::audio` | 音楽と環境音のチャンネル、効果音、音声のフェード | [std::screen::audio](../../../../std/ja/src/screen-audio.md) |
| `std::stage` | `std::screen` の上に作られたキャラクター、スロット、サイドフェイス | [std::stage](../../../../std/ja/src/stage.md) |
| `std::system` | `System`、設定、既読集合、入力の割り当て | [std::system](../../../../std/ja/src/system.md) |
| `std::system::nav` | モード間の移動、物語の開始とロード | [std::system::nav](../../../../std/ja/src/system-nav.md) |
| `std::system::saves` | セーブスロットとセーブ情報 | [std::system::saves](../../../../std/ja/src/system-saves.md) |
| `std::ui` | UI 要素、シグナル、UI フォームの実体となる関数 | [std::ui](../../../../std/ja/src/ui.md) |
| `std::ui::style` | UI コントロールの見た目 | [std::ui::style](../../../../std/ja/src/ui-style.md) |

## プレリュード {#the-prelude}

**プレリュード**はすべてのモジュールに自動でインポートされます。モジュール自身のアイテムとインポートはプレリュードより優先されます。プレリュードの内容は次のとおりです。

| 元のモジュール | 名前 |
| -------------- | ---- |
| `std` | `String`、`Vec`、`Rat`、`Decimal`、`Range`、`Option`、`Result`、`Eq`、`Integer`、`RandChoose`、`ImageAsset`、`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset`、`ShaderAsset`、`asset`、`system_font`。バリアント `None`、`Some`、`Ok`、`Err` |
| `std::time` | `Duration`、`ms`、`secs` |
| `std::screen` | `Screen`、`Render`、`EntityId`、`Part`、`Point`、`Offset`、`Size`、`Anchor`、`Color`、`Plane`、`Visibility`、`point`、`offset`、`rgb`、`hex`、`BLACK`、`WHITE`、`TRANSPARENT` |
| `std::screen::anim` | `Easing`、`Tween`、`Transition`、`Motion`、`tween`、`instant`、`cut`、`fade`、`rule`、`wipe`、`through`、`motion`、`jump`、`nod`、`shake`、`quake` |
| `std::screen::audio` | `looped`、`sound`、`crossfade` |
| `std::stage` | `Character`、`character` |
| `std::narrative` | `Text`、`TextEffect`、`Say`、`Choice`、`ChoiceArm`、`push_step`、`rng_choose`、`Meta`、`meta`、`Narrative` |
| `std::narrative::text` | `emphasis`、`dots`、`color_text`、`bold`、`pause`、`click_wait`、`mark` |
| `std::system` | `nav`、`Setting`、`setting`、`System`、`Settings`、`Volume`、`volume`、`RootPage`、`Overlay`、`GameOverlay`、`Dialog` |
| `std::ui` | `signal`、`Element` |

それ以外は `use std::screen::size;` のように `use` でインポートします。

## アセット {#assets}

**アセット**はゲームのファイルで、定数に結び付けて使います。

```kis
const CG_001: ImageAsset = asset("cg/cg_001.png");
const AZUSA_0072: VoiceAsset = asset("voice/azusa_0072.ogg");
```

```kis
#[builtin] pub const fn asset<A: Asset>(path: narrative::Text) -> A;
```

アセット型は 6 つあり、トレイト `Asset` を実装するのはこれらだけです。

| 型 | ファイル |
| -- | -------- |
| `ImageAsset` | 画像。PNG、JPEG、WebP |
| `AudioAsset` | 音楽または効果音 |
| `VoiceAsset` | 台詞に結び付くボイスファイル |
| `VideoAsset` | 動画 |
| `FontAsset` | フォント |
| `ShaderAsset` | WGSL シェーダー（`.wgsl`）。フィルター、または天候やパーティクルなどの生成コンテンツ |

`A` は定数の型で決まります。`asset::<ImageAsset>("…")` と明示することもできます。パスは次の規則に従います。

- 文字列リテラルであり（[`E2028`](../diagnostics/types.md#e2028)）、空ではないこと（[`E1028`](../diagnostics/modules.md#e1028)）。
- **アセットディレクトリ**からの相対パスであること。アセットディレクトリは `kagari.toml` の `[assets] dir` で、既定はプロジェクトディレクトリの `assets` です。ディレクトリの区切りには `/` と `\` のどちらも使えます。
- アセットディレクトリの外に出ないこと。絶対パス、ドライブ（`C:`）、外に出る `..` は使えません（[`E1029`](../diagnostics/modules.md#e1029)）。
- システムフォント用に予約された最上位ディレクトリ `system-fonts` に入らないこと（[`E1032`](../diagnostics/modules.md#e1032)）。
- ゲームのコンパイル時にファイルが存在すること（[`E1027`](../diagnostics/modules.md#e1027)）。ファイルがなければプレイ中の失敗ではなくコンパイルエラーになります。

アセット型は[有限型](../concepts/kinds.md#finite-types)です。その値はちょうどプログラムのアセット定数です。そのため物語は、たとえばルートに応じてボイスを選ぶように、実行時にアセットを選べます。2 つのアセット値は、同じファイルを指すとき等しくなります。

### システムフォント {#system-fonts}

ビルドするマシンにインストールされたフォントは、アセットディレクトリにコピーせずに結び付けられます。

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

```kis
#[builtin] pub const fn system_font(path: narrative::Text) -> FontAsset;
```

パスはシステムフォントディレクトリからの相対パスを表す空でない文字列リテラルで、そのディレクトリの外に出てはいけません。ディレクトリは次の順に探され、ファイルが最初に見つかったものが使われます。

| プラットフォーム | ディレクトリ |
| ---------------- | ------------ |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`、`%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts` |
| Linux など | `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、続いて `$XDG_DATA_DIRS`（既定は `/usr/local/share:/usr/share`）の各ディレクトリの `fonts` |

見つからないフォントは [`E1030`](../diagnostics/modules.md#e1030) です。`kagari.toml` の `[assets] system-fonts = false` はシステムフォントを禁止します（[`E1031`](../diagnostics/modules.md#e1031)）。標準ライブラリはフォントを同梱しておらず、既定のフォント `System::font` は必須です。

### ビルドでのアセット {#assets-in-the-build}

`kagari build` は、プログラムが使うすべてのアセットファイルを `.kagari` ファイルの隣のアーカイブ（`.kiarchive`）にまとめます。各ファイルはアセットディレクトリからの相対パス（区切りは `/`）で格納され、そのパスの最初のディレクトリにちなんだ名前のアーカイブに入ります。

| アセットパス | アーカイブ |
| ------------ | ---------- |
| `bg/classroom.png` | `bg.kiarchive` |
| `cg/cg_001.png` | `cg.kiarchive` |
| `logo.png`（アセットディレクトリの直下） | `data.kiarchive` |
| システムフォント `msgothic.ttc` | `system-fonts.kiarchive`（`system-fonts/msgothic.ttc` として） |

つまり、ファイルを最上位ディレクトリでどうまとめるかによって、ゲームのデータがどのアーカイブに分かれるかが決まります。ビルドの出力については [`kagari` CLI](../tools/cli.md) を参照してください。
