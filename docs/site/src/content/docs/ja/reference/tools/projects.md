---
title: "プロジェクト"
---

Kagari のゲームは**プロジェクト**という単位で作ります。プロジェクトとは、マニフェスト `kagari.toml`、ソースファイル、アセットをまとめたディレクトリです。`kagari build` でコンパイルすると、1 つの `.kagari` ファイルと、アセットを収めた `.kiarchive` アーカイブができます。これを[ランタイム](./runtime.md)で実行します。

## プロジェクトの構成 {#project-layout}

`kagari init` を実行すると、次の構成ができます（詳しくは [`kagari init`](./cli.md#kagari-init) を参照）。

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

必須のファイルは `kagari.toml` とルートファイルだけです。モジュールツリーはルートファイルを起点に広がります（[ソースファイルとモジュール](../syntax/modules.md)を参照）。`src/` や `assets/` の下のディレクトリ名はテンプレート上の慣習で、決まりではありません。

Kagari リポジトリの `example/test_game1` が、これと同じ構成の完全なプロジェクトです。

## `kagari.toml` {#kagaritoml}

マニフェストは 4 つのテーブルからなる TOML ファイルです。すべてのキーを書いた例を示します。

```toml
[project]
name = "my-game"          # required
version = "0.1.0"         # required, semver
identifier = "com.example.my-game"  # reverse-DNS name; required by `kagari release --macos`
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

全体に共通する規則は次のとおりです。

- 知らないテーブルやキーを書くとエラーになります。
- パスはすべて、`kagari.toml` を置いたディレクトリからの相対パスで書きます。絶対パスはエラーです。
- `kagari` コマンドは、カレントディレクトリから親ディレクトリへとさかのぼって `kagari.toml` を探します。`--manifest-path` を指定した場合はそのファイルを使います。

### `[project]` {#project}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `name` | 文字列 | 必須 | プロジェクト名。空文字列は不可。 |
| `version` | 文字列 | 必須 | プロジェクトのバージョン。`"0.1.0"` のような [semver](https://semver.org) 形式。 |
| `identifier` | 文字列 | なし | ゲームを識別する逆 DNS 形式の名前（例：`"com.example.my-game"`）。`.` で区切った 2 つ以上の部分からなり、各部分には ASCII の英字、数字、`-` を使えます。[`kagari release --macos`](./cli.md#kagari-release) では必須で、アプリのバンドル識別子になります。 |
| `entry` | 文字列（パス） | `"src/main.kis"` | ルートのソースファイル。拡張子は `.kis` か `.kix`。 |

`identifier` には自分が所有するドメインをもとにした名前を付け、ゲームを公開した後は変えないでください。macOS はこの名前でアプリを識別します。`kagari init` はこのキーを書きません。値が不正な場合はマニフェストのエラー（終了ステータス 2）になります。

### `[toolchain]` {#toolchain}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `kagari` | 文字列 | 必須 | 実行する `kagari` のバージョンが満たすべき semver の条件（`"0.1"`、`">=0.1, <0.3"` など）。 |

条件を満たさないと、プロジェクトを読み込むコマンドはすべて `this project requires kagari <req>, but this is kagari <version>` と出力し、終了ステータス 2 で止まります。`kagari init` は、プロジェクトを作成したツールチェインの `major.minor` をここに書き込みます。

### `[assets]` {#assets}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `dir` | 文字列（パス） | `"assets"` | アセットディレクトリ。`asset("…")` のパスはここを基準にした相対パス。 |
| `system-fonts` | 真偽値 | `true` | `system_font("…")` で、ビルドするマシンにインストールされたフォントを指定できるかどうか。`false` にすると、`system_font` の呼び出しはすべてエラー `E1031` になります。 |

アセットディレクトリ直下の `system-fonts/` と `std/` は予約済みの名前で、使うとエラーになります（`E1032`）。ビルド出力では、システムフォントと標準ライブラリ自身のアセット（シェーダーなど）をこの名前の下に格納するためです。

`system_font("…")` は、ゲームをビルドするマシン上で次のディレクトリを順に探します。

| プラットフォーム | ディレクトリ |
| ---------------- | ------------ |
| Windows | `%LOCALAPPDATA%\Microsoft\Windows\Fonts`、`%WINDIR%\Fonts` |
| macOS | `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts` |
| Linux など | `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、続いて `$XDG_DATA_DIRS` の各ディレクトリ内の `fonts`（既定は `/usr/local/share:/usr/share`） |

フォントファイルはビルド出力にコピーされるので、プレイヤーの環境にそのフォントがなくても表示できます。配布する前に、フォントのライセンスを確認してください。

### `[build]` {#build}

| キー | 型 | 既定値 | 意味 |
| ---- | -- | ------ | ---- |
| `targets` | 文字列の配列 | `["v0"]` | `kagari build` がビルドするターゲット。指定できるのは `v0`。空にはできず、重複は無視。 |
| `runtime-compatibility` | 文字列の配列 | `["native_v0"]` | 対応すべき[ランタイムプロファイル](#runtime-profiles)。すべてのアセットが、これらのプロファイルのデコーダーで読める必要があります。指定できるのは `native_v0`。空にはできず、重複は無視。 |
| `out-dir` | 文字列（パス） | `"target"` | 出力先のディレクトリ。 |
| `state-cap` | 整数 | `200000` | [コンパイラオプション](#compiler-options)の `state-cap`。 |
| `recursion-depth` | 整数 | `2048` | コンパイラオプションの `recursion-depth`。 |
| `keyframe-check` | 真偽値 | `false` | コンパイラオプションの `keyframe-check`。 |

## コンパイラオプション {#compiler-options}

マニフェストのキーのうち、コンパイラの検査内容や出力を左右する 4 つをコンパイラオプションと呼びます。`kagari check`、`kagari build`、言語サーバーのいずれもこの設定に従います。

| オプション | キー | 既定値 | 意味 |
| ---------- | ---- | -----: | ---- |
| 状態数の上限 | `[build] state-cap` | 200 000 | 解析で探索する物語の状態数の上限。超えた時点で解析を打ち切り、`W5006` を報告します。 |
| 再帰の深さ | `[build] recursion-depth` | 2048 | 実行時に入れ子にできる呼び出しの上限。呼び出しフレームはすべて数えます（`fn`、クロージャ、シーン、即時手続き）。これを超えるとゲームはエラーで止まります。値は `.kagari` ファイルに書き込まれます。 |
| キーフレーム検査 | `[build] keyframe-check` | オフ | キーフレームの配置を検査します（`W5011`）。プレイヤーが直接たどり着けるステップ（各選択肢と、各開始シーンの最初のステップ）には、キーフレームとなるレンダーを置くべきだという検査です。[演出](../concepts/presentation.md)を参照してください。 |
| システムフォント | `[assets] system-fonts` | オン | `system_font("…")` でインストール済みのフォントを指定できるようにします（オフのときはエラー `E1031`）。 |

`W5006` と `W5011` については[物語の解析（E5xxx）](../../diagnostics/analysis.md)を、`E1031` については[モジュール・アイテム・名前（E1xxx）](../../diagnostics/modules.md)を参照してください。

## ターゲットと出力ディレクトリ {#targets-and-output-directories}

**ターゲット**とは、`.kagari` ファイルを動かすエンジンのメジャーバージョンのことです。現在のターゲットは `v0` だけです。`kagari build` はターゲットごとに `<out-dir>/<target>` というディレクトリを分けて出力します。

```text
target/v0/
├── game.kagari
├── bgimage.kiarchive
├── bgm.kiarchive
├── fgimage.kiarchive
├── image.kiarchive
└── system-fonts.kiarchive
```

（上は `example/test_game1` の出力です。）`kagari build` は、書き出す前にそのディレクトリから以前のビルドの `game.kagari`、すべての `.kiarchive`、すべての `.kagari-cache` を削除します。それ以外のファイルはそのまま残します。[デバッグビルド](./cli.md#build-profiles)（`kagari build --profile debug`）では、デバッグ用の成果物 `game.kagari-cache` も同じディレクトリに書き出します。

出力ディレクトリの直下には、`kagari check` と `kagari build` が[解析キャッシュ](./cli.md#the-analysis-cache) `analysis.kagari-cache` を置きます。また [`kagari release`](./cli.md#kagari-release) は、パッケージを `release/windows`、`release/linux`（ISO イメージ）、`release/macos`（アプリを収めた ZIP アーカイブ）に書き出します。[`kagari clean`](./cli.md#kagari-clean) を実行すると、解析キャッシュ、全ターゲットのビルド出力、パッケージが削除されます。新しく作ったプロジェクトの `.gitignore` では、既定の出力ディレクトリ `/target` を除外しています。

## アセットとアーカイブ {#assets-and-archives}

`kagari build` がアーカイブに詰めるのは、プログラムが `asset("…")` と `system_font("…")` で参照しているアセットだけです。アセットディレクトリを丸ごと詰めるわけではありません。アセットは `game.kagari` と同じ場所の `.kiarchive` ファイルに入ります。

| アセットのパス | アーカイブ |
| -------------- | ---------- |
| `characters/character1/image/normal.png` | `characters.kiarchive`（最上位のディレクトリごとに 1 つ） |
| `title.png`（アセットディレクトリ直下） | `data.kiarchive` |
| システムフォント（例：`system-fonts/msgothic.ttc`） | `system-fonts.kiarchive` |
| 標準ライブラリのアセット（例：`std::shaders` のシェーダー `std/shaders/color/blur.wgsl`） | `std.kiarchive` |

アーカイブ内の各項目には、アセットの完全なパスがそのまま名前として付きます。ランタイムは、あるアーカイブ内のアセットが初めて必要になった時点でそのアーカイブを開きます。アーカイブが見つからなくても、起動時にはエラーになりません。ランタイムは警告をログに出し、そのアセットは使う時点で存在しない扱いになります。

ランタイムは、ベースのアーカイブと同じ場所にある番号付きのパッチアーカイブ（`bgimage.001.kiarchive`、`bgimage.002.kiarchive`、…）も、番号が途切れるところまで読み込みます。各パッチは、それより前のアーカイブの項目を追加・置換・削除します。`kagari build` が書き出すのはベースのアーカイブだけです。

### ランタイムプロファイル {#runtime-profiles}

**ランタイムプロファイル**とは、あるランタイムが備えているデコーダーの組み合わせに付けた名前です。`kagari build` は何かを書き出す前に、参照されているすべてのアセットを、`build.runtime-compatibility` に挙げたすべてのプロファイルに照らして検査します。アセットの形式は、ファイルの拡張子ではなく中身で判定します。現在のプロファイルは、[デスクトップランタイム](./runtime.md)に対応する `native_v0` だけです。

| アセットの種類 | `native_v0` がデコードできる形式 |
| -------------- | -------------------------------- |
| 画像 | PNG（APNG は既定の画像を表示）、ベースラインおよびプログレッシブ JPEG、非可逆・可逆・拡張（アルファやメタデータ付き）の WebP、BMP（非圧縮、RLE、ビットフィールド）、静止画の JPEG XL（RGB またはグレースケール） |
| 音声、ボイス | Ogg Vorbis、Ogg FLAC、FLAC、WAV（PCM または IEEE 浮動小数点）、MP3、モノラルまたはステレオの AAC-LC（ADTS の `.aac` または MP4/M4A）、モノラルまたはステレオの ALAC（MP4/M4A） |
| フォント | TrueType、OpenType（CFF）、TrueType/OpenType コレクション |
| シェーダー | WGSL ソース（UTF-8 テキスト） |
| 動画 | 映像が VP9 か AV1、音声が Opus か Vorbis（音声なしも可）の WebM。映像が H.264、音声が AAC-LC（音声なしも可）の MP4 |

RIFF チャンクや `data` チャンクのサイズがファイルの終端を越えている WAV も、ファイルの終わりまで再生します。ストリーミング向けのエンコーダー（パイプに出力する ffmpeg や TTS サービスなど）が仮の値 `0xFFFFFFFF` を残したファイルがこれに当たります。

MP4/M4A ファイルでは、最初の音声トラックを再生します。`kagari build` はサンプル記述がファイルのどこにあっても読み取れるので、インデックスが末尾にあるファイル（ffmpeg の既定）もそのまま使えます。なお、ランタイムは AAC のエンコーダー遅延を除去しません。AAC ファイルの先頭にはエンコーダーが入れるプライミング（通常は 1024 サンプル、48 kHz で約 21 ms の無音）があり、末尾にはパディングがあります。ループ位置などの再生位置は、このプライミングの先頭から数えます。継ぎ目なくループさせたい音楽には、Ogg Vorbis、FLAC、WAV を使ってください。

上記以外の形式はすべて受け付けません。具体的には、アニメーション WebP、算術符号・可逆・階層型・12 ビットの JPEG、GIF、JPEG や PNG を埋め込んだ BMP や CMYK の BMP、AVIF、アニメーションや CMYK の JPEG XL、Ogg Opus、その他のコーデックの Ogg、圧縮された WAV、AAC-LC 以外の AAC プロファイル（HE-AAC、Main、LTP など）、3 チャンネル以上の AAC や ALAC、その他のコーデック（Opus、MP3、FLAC など）の MP4/M4A、WOFF、WOFF2 です。動画は、対応外の映像コーデック（VP8、H.265 など）、複数の映像トラック、対応外の音声コーデック、複数の音声トラック、WebM 内のその他のトラック、対応外のコンテナのどれかに当てはまると受け付けません。問題があると、アセットとプロファイルの組ごとに 1 行ずつ出力します。

```text
error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
error: asset `movie/intro.webm` (video): WebM/Matroska with another codec, several video tracks or an unreadable header is not supported by runtime profile `native_v0`
```

動画を VP9 と Opus の WebM に変換するには、次のようにします。

```sh
ffmpeg -i in.mov -c:v libvpx-vp9 -c:a libopus out.webm
```

AV1 にする場合は `-c:v libsvtav1` を、H.264 にする場合は `-c:v libx264 -c:a aac` を指定し、出力先を `.mp4` にします。

### シェーダーの検査 {#shader-check}

プロファイルの検査が済むと、`kagari build` は参照されているすべてのシェーダー（標準ライブラリのものも含む）を、その定数が宣言している役割に合わせてコンパイルします。たとえば `ColorShader<…>` であれば、色シェーダーとして検査します。2 つの役割で使われているファイルは、役割ごとに 1 回ずつ検査します。不正なシェーダーがあると 1 つにつき 1 行出力し、誤りが `.wgsl` ファイルの中にある場合はその行番号も示します。

```text
error: shader `shader/broken.wgsl` (color): line 4: no definition in scope for identifier: `undefined_name`
```

役割には `color`、`mix`、`texture`、`mask`、`local motion`、`transitional motion` があります。

シェーダーにエラーがあった場合、ビルドは出力ディレクトリを作らず、以前の出力も消さずに、終了ステータス 1 で終わります。標準ライブラリのリファレンスの[ランタイムプロファイル](../../std/appendix/profiles.md)も参照してください。

## ゲームの配布 {#shipping-a-game}

Windows と Linux では、ランタイムの実行ファイルと `<out-dir>/<target>` の中身を 1 つのディレクトリにまとめて配布します。

```text
MyGame/
├── kagari-desktop-runtime      (kagari-desktop-runtime.exe on Windows; you may rename it)
├── vcruntime140.dll            (Windows only)
├── avcodec-63.dll, avformat-63.dll, avutil-61.dll, swscale-10.dll, swresample-7.dll   (Windows)
├── libavcodec.so.63, libavformat.so.63, libavutil.so.61, libswscale.so.10, libswresample.so.7   (Linux)
├── ffmpeg-LICENSE.txt
├── game.kagari
└── *.kiarchive
```

ディレクトリを引数に指定せずに起動すると、ランタイムは自分の実行ファイルと同じディレクトリから `game.kagari` を探します。設定やセーブはそこには書き込まず、ユーザーごとのデータディレクトリに保存します（[データディレクトリ](./runtime.md#data-directory)を参照）。プレイヤーの環境には、[システム要件](./runtime.md#system-requirements)に挙げたシステムライブラリが必要です。Windows のランタイムは Microsoft Visual C++ ランタイムライブラリの `vcruntime140.dll` を必要としますが、これが入っていない Windows は珍しくありません。ランタイムと同じ場所に置いておけば、インストールしなくても使えます。使用している Kagari のバージョンに合わせて、`https://kagari-download.suitsu31.club/windows_x86_installer/<version>/vcruntime140.dll` からダウンロードしてください。

macOS では、アプリバンドル `<name>.app` として配布します。ランタイムは `Contents/MacOS/<name>` に置き、ゲームのファイル（`game.kagari` とアーカイブ）、`ffmpeg-LICENSE.txt`、アイコンは `Contents/Resources` に、FFmpeg のライブラリは `Contents/Frameworks` に置きます。アプリ内のランタイムは、自分のディレクトリに `game.kagari` がなければ `Contents/Resources` からゲームを読み込みます。アプリには署名が必要なので、手作業で組み立てずに [`kagari release --macos`](./cli.md#the-macos-app) で作成してください。

ランタイムは、同じ場所（macOS のアプリでは `Contents/Frameworks`）にある FFmpeg のライブラリを使って動画をデコードします。これらがないとランタイムは起動しません。使用している Kagari のバージョンに合わせて、`https://kagari-download.suitsu31.club/windows_x86_installer/<version>/<file>`（Windows）、`https://kagari-download.suitsu31.club/linux_x86_bin/<version>/<file>`（Linux）、`https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/<file>`（macOS）からダウンロードしてください。ライブラリと一緒に配布するライセンス文書 `ffmpeg-LICENSE.txt` も、あわせてダウンロードしてください。

[`kagari release`](./cli.md#kagari-release) を使うと、このディレクトリを Windows 用・Linux 用には ISO イメージとして、macOS 用にはアプリを収めた ZIP アーカイブとして作成できます。このコマンドはゲームをビルドし、`kagari` と同じバージョンの対象プラットフォーム向けランタイム、FFmpeg のライブラリとそのライセンス（Windows 向けには `vcruntime140.dll`、macOS 向けにはアプリのアイコンも）をダウンロードして、`<out-dir>/release/<platform>/<name>.iso`（macOS 向けは `<out-dir>/release/macos/<name>.zip`）に書き出します。Windows 用のイメージでは、ランタイムの名前は `run.exe` になります。
