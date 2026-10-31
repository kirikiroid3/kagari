---
title: "プロジェクト"
---

Kagari のゲームは**プロジェクト**です。プロジェクトは、マニフェスト `kagari.toml`、ソースファイル、アセットを収めたディレクトリです。`kagari build` はプロジェクトを 1 つの `.kagari` ファイルと `.kiarchive` アセットアーカイブにコンパイルし、[ランタイム](./runtime.md)がそれを実行します。

## プロジェクトの構成 {#project-layout}

`kagari init` は次の構成を作ります（[`kagari init`](./cli.md#kagari-init) を参照）。

```text
my-game/
├── kagari.toml                 the manifest
├── .gitignore                  ignores /target
├── src/
│   ├── main.kis                the root file (`project.entry`)
│   ├── scenario/mod.kix        the story
│   ├── scripts/mod.kis
│   ├── scripts/characters.kis
│   ├── system/mod.kix          the game system
│   └── effect/mod.kis
├── assets/                     `asset("…")` paths are relative to this directory
│   ├── bgimage/
│   ├── sound/
│   ├── music/
│   └── characters/character1/
│       ├── image/normal.png
│       └── voice/0001.wav
└── target/                     build output (`build.out-dir`)
    ├── analysis.kagari-cache   the analysis cache
    └── v0/
        ├── game.kagari
        ├── *.kiarchive
        └── game.kagari-cache   debug builds only
```

必須なのは `kagari.toml` とルートファイルだけです。モジュールツリーはルートファイルから広がります。[ソースファイルとモジュール](../syntax/modules.md)を参照してください。`src/` と `assets/` の下のディレクトリ名はテンプレートの慣習であり、規則ではありません。

Kagari リポジトリの `example/test_game1` は同じ形の完全なプロジェクトです。

## `kagari.toml` {#kagaritoml}

マニフェストは 4 つのテーブルからなる TOML です。すべてのキーを使った例を示します。

```toml
[project]
name = "my-game"          # required
version = "0.1.0"         # required, semver
entry = "src/main.kis"    # the root file; default "src/main.kis"

[toolchain]
kagari = "0.1"            # required; the toolchain versions the project accepts

[assets]
dir = "assets"            # the asset directory; default "assets"
system-fonts = true       # whether `system_font("…")` may name installed fonts; default true

[build]
targets = ["v0"]          # engine major versions to build for; default ["v0"]
runtime-compatibility = ["native_v0"]  # runtime profiles whose decoders the assets must suit; default ["native_v0"]
out-dir = "target"        # default "target"
state-cap = 200000
recursion-depth = 2048
keyframe-check = false
```

共通の規則:

- 未知のテーブルやキーはエラーです。
- パスはすべて `kagari.toml` のあるディレクトリからの相対パスです。絶対パスはエラーです。
- `kagari` ツールは、`--manifest-path` で指定されない限り、カレントディレクトリから親ディレクトリへ順に `kagari.toml` を探します。

### `[project]` {#project}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `name` | 文字列 | 必須 | プロジェクト名。空にはできません。 |
| `version` | 文字列 | 必須 | プロジェクトのバージョン。`"0.1.0"` のような [semver](https://semver.org) のバージョンです。 |
| `entry` | 文字列（パス） | `"src/main.kis"` | ルートのソースファイル。`.kis` か `.kix` で終わる必要があります。 |

### `[toolchain]` {#toolchain}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `kagari` | 文字列 | 必須 | 実行中の `kagari` のバージョンが満たすべき semver の条件（`"0.1"`、`">=0.1, <0.3"`）。 |

条件を満たさない場合、プロジェクトを読み込むコマンドはすべて `this project requires kagari <req>, but this is kagari <version>` を出して終了ステータス 2 で止まります。`kagari init` は、プロジェクトを作ったツールチェインの `major.minor` を書き込みます。

### `[assets]` {#assets}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `dir` | 文字列（パス） | `"assets"` | アセットディレクトリ。`asset("…")` のパスはここからの相対パスです。 |
| `system-fonts` | 真偽値 | `true` | `system_font("…")` がビルドするマシンにインストールされたフォントを指せるかどうか。`false` のとき、`system_font` の呼び出しはすべて `E1031` になります。 |

アセットディレクトリ直下の `system-fonts/` と `std/` ディレクトリは予約されています（`E1032`）。システムフォントと、シェーダーなど標準ライブラリ自身のアセットは、ビルド出力の中でこれらの名前の下に格納されます。

`system_font("…")` は、ゲームをビルドするマシンの次のディレクトリを順に探します。

| プラットフォーム | ディレクトリ |
| ---------------- | ------------ |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`、`%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts` |
| Linux など | `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、続いて `$XDG_DATA_DIRS` の各ディレクトリの `fonts`（既定は `/usr/local/share:/usr/share`） |

フォントファイルはビルド出力にコピーされるため、プレイヤーがそのフォントをインストールしている必要はありません。配布する前にフォントのライセンスを確認してください。

### `[build]` {#build}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `targets` | 文字列の配列 | `["v0"]` | `kagari build` がビルドするターゲット。対応しているのは `v0`。空にはできず、重複は無視されます。 |
| `runtime-compatibility` | 文字列の配列 | `["native_v0"]` | すべてのアセットが対応すべきデコーダを持つ[ランタイムプロファイル](#runtime-profiles)。対応しているのは `native_v0`。空にはできず、重複は無視されます。 |
| `out-dir` | 文字列（パス） | `"target"` | 出力ディレクトリ。 |
| `state-cap` | 整数 | `200000` | [コンパイラオプション](#compiler-options) `state-cap`。 |
| `recursion-depth` | 整数 | `2048` | コンパイラオプション `recursion-depth`。 |
| `keyframe-check` | 真偽値 | `false` | コンパイラオプション `keyframe-check`。 |

## コンパイラオプション {#compiler-options}

コンパイラオプションは、コンパイラが検査・出力する内容を変える 4 つのマニフェストキーです。`kagari check`、`kagari build`、言語サーバーはいずれもこれを使います。

| オプション | キー | 既定値 | 意味 |
| ---------- | ---- | -----: | ---- |
| 状態数の上限 | `[build] state-cap` | 200 000 | 解析が探索する物語の状態数の上限。超えると解析を打ち切り、`W5006` を報告します。 |
| 再帰の深さ | `[build] recursion-depth` | 2048 | 実行時の呼び出しの入れ子の上限。すべての呼び出しフレーム（`fn`、クロージャ、シーン、即時手続き）を数えます。これより深い呼び出しはゲームをエラーで停止させます。`.kagari` ファイルに書き込まれます。 |
| キーフレーム検査 | `[build] keyframe-check` | オフ | キーフレーム配置の検査（`W5011`）を行います。プレイヤーが直接到達しうるステップ（各選択肢と、各エントリーシーンの最初のステップ）にはキーフレームとなるレンダーがあるべきです。[演出](../concepts/presentation.md)を参照してください。 |
| システムフォント | `[assets] system-fonts` | オン | `system_font("…")` がインストール済みフォントを指せるようにします（オフのときは `E1031`）。 |

`W5006` と `W5011` は[物語の解析（E5xxx）](../../diagnostics/analysis.md)、`E1031` は[モジュール・アイテム・名前（E1xxx）](../../diagnostics/modules.md)を参照してください。

## ターゲットと出力ディレクトリ {#targets-and-output-directories}

**ターゲット**は、`.kagari` ファイルが対象とするエンジンのメジャーバージョンです。ターゲットは `v0` だけです。`kagari build` はターゲットごとに専用のディレクトリ `<out-dir>/<target>` へ書き出します。

```text
target/v0/
├── game.kagari
├── bgimage.kiarchive
├── bgm.kiarchive
├── fgimage.kiarchive
├── image.kiarchive
└── system-fonts.kiarchive
```

（これは `example/test_game1` の出力です。）`kagari build` は書き出す前に、以前のビルドがそのディレクトリに残した `game.kagari`、すべての `.kiarchive`、すべての `.kagari-cache` を削除します。それ以外のファイルには触れません。[デバッグビルド](./cli.md#build-profiles)（`kagari build --profile debug`）では、デバッグ用成果物 `game.kagari-cache` もそこに書き出します。

出力ディレクトリの直下には、`kagari check` と `kagari build` が[解析キャッシュ](./cli.md#the-analysis-cache) `analysis.kagari-cache` を保存し、[`kagari release`](./cli.md#kagari-release) が `release/windows` と `release/linux` にイメージを書き出します。[`kagari clean`](./cli.md#kagari-clean) は解析キャッシュ、すべてのターゲットのビルド出力、イメージを削除します。新しいプロジェクトの `.gitignore` は、既定の出力ディレクトリである `/target` を無視します。

## アセットとアーカイブ {#assets-and-archives}

`kagari build` がパックするのは、プログラムが（`asset("…")` と `system_font("…")` で）参照するアセットだけで、アセットディレクトリ全体ではありません。アセットは `game.kagari` の隣の `.kiarchive` ファイルに入ります。

| アセットのパス | アーカイブ |
| -------------- | ---------- |
| `characters/character1/image/normal.png` | `characters.kiarchive`（最上位のディレクトリごとに 1 つ） |
| `title.png`（アセットディレクトリ直下） | `data.kiarchive` |
| システムフォント（例: `system-fonts/msgothic.ttc`） | `system-fonts.kiarchive` |
| 標準ライブラリのアセット（例: `std::shaders` のシェーダー `std/shaders/color/blur.wgsl`） | `std.kiarchive` |

アーカイブ内のエントリ名はアセットの完全なパスです。ランタイムは、ゲームがあるアーカイブのアセットを初めて必要としたときにそのアーカイブを開きます。アーカイブが見つからなくても起動時のエラーにはなりません。ランタイムは警告をログに出し、そのアセットは使用時に存在しないものとして扱われます。

ランタイムは、ベースのアーカイブの隣にある番号付きのパッチアーカイブ（`bgimage.001.kiarchive`、`bgimage.002.kiarchive`、… 最初に欠けた番号まで）も読みます。各パッチは、それより前のアーカイブのエントリを追加・置換・削除します。`kagari build` が書き出すのはベースのアーカイブだけです。

### ランタイムプロファイル {#runtime-profiles}

**ランタイムプロファイル**は、ランタイムが持つデコーダの集合の名前です。`kagari build` は何かを書き出す前に、参照されるすべてのアセットを `build.runtime-compatibility` のすべてのプロファイルに照らして検査します。アセットの形式はファイル拡張子ではなく内容で判定します。プロファイルは[デスクトップランタイム](./runtime.md)の `native_v0` だけです。

| アセットの種類 | `native_v0` がデコードできる形式 |
| -------------- | -------------------------------- |
| 画像 | PNG（APNG は既定の画像を表示）、ベースラインおよびプログレッシブ JPEG、非可逆・可逆・拡張（アルファまたはメタデータ付き）WebP、BMP（非圧縮、RLE またはビットフィールド）、静止画の JPEG XL（RGB またはグレースケール） |
| オーディオ、ボイス | Ogg Vorbis、Ogg FLAC、FLAC、WAV（PCM または IEEE 浮動小数点）、MP3、モノラルまたはステレオの AAC-LC（ADTS の `.aac`、または MP4/M4A）、モノラルまたはステレオの ALAC（MP4/M4A） |
| フォント | TrueType、OpenType（CFF）、TrueType/OpenType コレクション |
| シェーダー | WGSL ソース（UTF-8 テキスト） |
| 動画 | なし |

RIFF または `data` チャンクのサイズがファイルの終端を越える WAV（ストリーミング用のエンコーダー、たとえばパイプへ書き出す ffmpeg や TTS サービスが残す `0xFFFFFFFF` のプレースホルダーなど）は、ファイルの終端まで再生されます。

MP4/M4A ファイルは最初のオーディオトラックを再生します。`kagari build` はサンプル記述がファイルのどこにあっても読むので、インデックスが末尾にあるファイル（ffmpeg の既定）も受け付けます。ランタイムは AAC のエンコーダー遅延を取り除きません。AAC ファイルはエンコーダーのプライミング（通常 1024 サンプル、48 kHz で約 21 ms の無音）で始まり、パディングで終わります。ループ位置などの再生位置はプライミングの先頭から数えます。途切れずにループさせたい音楽には Ogg Vorbis、FLAC、WAV を使ってください。

それ以外はすべて拒否されます。アニメーション WebP、算術・可逆・階層・12 ビット符号化の JPEG、GIF、JPEG または PNG を埋め込んだ BMP や CMYK の BMP、AVIF、アニメーションまたは CMYK の JPEG XL、Ogg Opus、その他のコーデックの Ogg、圧縮 WAV、HE-AAC など AAC-LC 以外の AAC プロファイル、3 チャンネル以上の AAC または ALAC、その他のコーデック（Opus、MP3、FLAC など）の MP4/M4A、WOFF、WOFF2 です。動画のアセットは内容にかかわらず拒否されます。問題はアセットとプロファイルの組ごとに 1 行ずつ出力されます。

```text
error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
```

### シェーダーの検査 {#shader-check}

プロファイルの検査に続いて、`kagari build` は参照されるすべてのシェーダーを、標準ライブラリのものも含め、その定数が宣言するロールとしてコンパイルします（`ColorShader<…>` はカラーシェーダーとして検査する、など）。2 つのロールで使われるファイルはロールごとに 1 回ずつ検査します。不正なシェーダーごとに 1 行出力し、誤りが `.wgsl` ファイル内にあればその行番号を示します。

```text
error: shader `shader/broken.wgsl` (color): line 4: no definition in scope for identifier: `undefined_name`
```

ロールは `color`、`mix`、`texture`、`mask`、`local motion`、`transitional motion` です。

このときビルドは、出力ディレクトリを作らず以前の出力も削除せずに、ステータス 1 で終了します。STD リファレンスの[ランタイムプロファイル](../../std/appendix/profiles.md)も参照してください。

## ゲームの配布 {#shipping-a-game}

配布するゲームは、ランタイムの実行ファイルと `<out-dir>/<target>` の中身を収めた 1 つのディレクトリです。

```text
MyGame/
├── kagari-desktop-runtime      (kagari-desktop-runtime.exe on Windows; you may rename it)
├── game.kagari
└── *.kiarchive
```

ディレクトリ引数なしで起動すると、ランタイムは自身の実行ファイルのディレクトリで `game.kagari` を探します。設定とセーブはそこには書かれず、ユーザーごとのデータディレクトリに保存されます（[データディレクトリ](./runtime.md#data-directory)を参照）。プレイヤーには[システム要件](./runtime.md#system-requirements)に挙げたシステムライブラリが必要です。

[`kagari release`](./cli.md#kagari-release) は、このディレクトリの中身を Windows 用または Linux 用の ISO イメージとして作ります。ゲームをビルドし、`kagari` と同じバージョンのそのプラットフォーム向けランタイムをダウンロードして、`<out-dir>/release/<platform>/<name>.iso` に書き出します。
