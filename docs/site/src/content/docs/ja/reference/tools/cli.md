---
title: "kagari CLI"
---

`kagari` は Kagari のツールチェインです。プロジェクトの作成、検査、ビルド、出力の削除、整形のほか、プレイヤー向けのパッケージ作成、`.kagari` ファイルの逆アセンブル、言語サーバーの起動、物語のデバッグもこのコマンドで行います。

```text
kagari <COMMAND>

Commands:
  init     Create a new project in PATH
  check    Check the project and print its diagnostics
  build    Build the project's bytecode and asset archives
  clean    Delete the build outputs and the analysis cache
  release  Build the project and package it with the runtime of a platform: an ISO image for Windows and Linux, an app in a ZIP archive for macOS
  fmt      Format the project's source files, or the given files
  disasm   Disassemble a `.kagari` bytecode file
  lsp      Run the language server over stdio
  debug    Debug the story: the query engine's REPL on the headless game (by default), a game in a window (`--window`), or a running game (`--attach`)
  dap      Run the debug adapter (Debug Adapter Protocol) over stdio
  help     Print this message or the help of the given subcommand(s)
```

`kagari --version` でツールチェインのバージョンを表示します。`kagari.toml` の `[toolchain] kagari` は、このバージョンと合っている必要があります。個々のコマンドのヘルプは `kagari help <COMMAND>` か `kagari <COMMAND> --help` で表示できます。

## インストール {#installing}

[Kagari のインストール](../../tutorial/install.md)で紹介しているインストーラーを使うと、Linux、macOS、Windows のいずれでも `kagari` と `kagari-desktop-runtime` が `PATH` に入ります。NixOS では、同じページで説明している flake を使ってください。

Kagari リポジトリから両方をビルドすることもできます（Rust と Cargo が必要です）。

```sh
cargo install --path tooling/kagari-cli
cargo install --path runtime/kagari-desktop-runtime
```

これで、両方が Cargo の `bin` ディレクトリにインストールされます。リポジトリの中であれば、インストールせずに `cargo run -p kagari-cli -- <COMMAND>` で `kagari` を実行することもできます。

## プロジェクトの探し方 {#finding-the-project}

`check`、`build`、`clean`、`release`、およびファイルを指定しない `fmt` は、プロジェクトを対象に動きます。`--manifest-path <PATH>` で `kagari.toml` を指定すればそれを使い、指定しなければカレントディレクトリから親ディレクトリへさかのぼって最初に見つかった `kagari.toml` を使います。マニフェストの書き方は[プロジェクト](./projects.md)を参照してください。

## 終了ステータス {#exit-statuses}

終了ステータスは、どのコマンドでも共通です。

| ステータス | 意味 |
| ---------: | ---- |
| 0 | 成功。 |
| 1 | コマンドは実行できたが、問題が見つかった。エラーの診断、対応していないアセット形式、整形されていないファイル、不正な `.kagari` ファイルなど。 |
| 2 | コマンドを実行できなかった。使い方の誤り、入出力エラー、`kagari.toml` が見つからない、マニフェストが不正、ツールチェインのバージョンが合わない、ランタイムをダウンロードできない、など。メッセージは `kagari: <message>` の形で標準エラー出力に出ます。 |

## コンパイラオプション {#compiler-options}

コンパイラ用のコマンドラインフラグはありません。オプションはすべて `kagari.toml` のキーで指定するので、`check`、`build`、言語サーバーの結果が食い違うことはありません。

| キー | 既定値 | 効果 |
| ---- | -----: | ---- |
| `[build] state-cap` | 200 000 | 解析で探索する物語の状態数の上限（超えると `W5006`）。 |
| `[build] recursion-depth` | 2048 | 実行時に入れ子にできる呼び出しの上限。 |
| `[build] keyframe-check` | `false` | キーフレームの配置を検査する（`W5011`）。 |
| `[assets] system-fonts` | `true` | `system_font("…")` でインストール済みのフォントを指定できるようにする（オフのときは `E1031`）。 |

詳しくは[コンパイラオプション](./projects.md#compiler-options)を参照してください。

## 解析キャッシュ {#the-analysis-cache}

プロジェクトのコンパイルで最も時間がかかるのは物語の解析です。停止性、到達可能性、行き詰まる選択肢、画面とその一貫性、キーフレーム、ゲームシステムを検査するため、物語のすべての状態を探索します（[物語の解析](../../diagnostics/analysis.md)、[ゲームシステム](../../diagnostics/system.md)を参照）。`kagari check` と `kagari build` は、解析結果を `<out-dir>/analysis.kagari-cache`（既定では `target/analysis.kagari-cache`）に保存します。次の項目がどれも変わっていなければ、保存した結果をそのまま使います。

- プログラムを構成するすべてのソースファイルの内容（標準ライブラリを含む）
- [コンパイラオプション](#compiler-options)（`state-cap`、`recursion-depth`、`keyframe-check`、`system-fonts`）
- アセットディレクトリとシステムフォントのディレクトリの場所
- ツールチェインのバージョン

キャッシュを書いた時点からこれらが一つも変わっていなければ、解析はやり直さず、診断もキャッシュから取り出します。出力は解析をやり直した場合と変わりません。どれかが変わっていれば解析を実行し、キャッシュを書き直します。それより前の段階の検査でエラーが出ている間は、解析を行わないので、キャッシュも書きません。なお、このキャッシュがあるため、`kagari check` も出力ディレクトリに書き込みを行います。

キャッシュファイルがない、読めない、または内容が不正な場合は、無視して作り直します。書き込めないときは標準エラー出力に `kagari: warning: cannot write the analysis cache: <reason>` と出し、終了ステータスはそのままで処理を続けます。キャッシュは [`kagari clean`](#kagari-clean) で削除できます。コンパイラ自体を開発していて、バージョンを変えずにコンパイラに手を入れた場合は、`kagari clean` を実行してください。キャッシュからは、変更前と変更後のコンパイラを見分けられません。

## `kagari init` {#kagari-init}

```text
kagari init [PATH] [--name <NAME>]
```

`PATH`（既定はカレントディレクトリ）に新しいプロジェクトを作ります。ディレクトリがなければ作成します。

| オプション | 意味 |
| ---------- | ---- |
| `PATH` | プロジェクトのディレクトリ。既定は `.`。 |
| `--name <NAME>` | プロジェクト名。既定はディレクトリ名。 |

`kagari init` は新しいファイルを作るだけで、既存のファイルは上書きしません。テンプレートに含まれるファイルが `PATH` に 1 つでもあれば、何も変更せずに終了ステータス 2 で止まります。作成される構成は[プロジェクトの構成](./projects.md#project-layout)のとおりで、`[toolchain] kagari` には実行したツールチェインの `major.minor` が入ります。

テンプレートの `src/main.kis` では、ゲームの既定のフォント（`FONT`）を宣言しています。`kagari init` は[システムフォントのディレクトリ](./projects.md#assets)からインストール済みのフォントを 1 つ選び、`system_font("…")` として書き込みます。優先するのは日本語のゴシック体（游ゴシック、メイリオ、MS ゴシック、ヒラギノ角ゴシック、Noto Sans CJK/JP）で、次に一般的なサンセリフ体（Segoe UI、Arial、Helvetica、Noto Sans、DejaVu Sans、Liberation Sans）です。どれもなければ、最初に見つかったフォントファイルを使います。フォントが 1 つもインストールされていなければ `asset("fonts/default.ttf")` と書き込み、`assets/fonts/default.ttf` にフォントを置くか `FONT` を書き換えるよう警告します。

```sh
kagari init my-game
kagari init --name "My Game"
```

## `kagari check` {#kagari-check}

```text
kagari check [--manifest-path <PATH>] [--resolutions]
```

プロジェクトをコンパイルし、すべての診断を標準出力に出します。ビルド出力は作らず、書き込むのは[解析キャッシュ](#the-analysis-cache)だけです。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--resolutions` | プログラム中の各名前が何に解決されたかも出力する。インポートの問題を調べるときに便利。 |

エラーの診断が 1 つでもあれば終了ステータス 1 で終わります。警告だけなら 0 です。診断コードの一覧は[診断](../../diagnostics/index.md)にあります。

## `kagari build` {#kagari-build}

```text
kagari build [--manifest-path <PATH>] [--target <TARGET>]... [--profile <PROFILE>]
```

プロジェクトをコンパイルし、ターゲットごとに `<out-dir>/<target>/game.kagari` とアセットのアーカイブを書き出します。`--profile debug` を付けると `game.kagari-cache` も書き出します。[ターゲットと出力ディレクトリ](./projects.md#targets-and-output-directories)と[アセットとアーカイブ](./projects.md#assets-and-archives)も参照してください。`kagari check` と同じく、[解析キャッシュ](#the-analysis-cache)を使い、必要に応じて更新します。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--target <TARGET>` | 指定したターゲットだけをビルドする。複数回指定可。`build.targets` にないターゲットを指定すると終了ステータス 2。既定では `build.targets` のすべてのターゲット。 |
| `--profile <PROFILE>` | `release` か `debug`（[ビルドプロファイル](#build-profiles)を参照）。既定は `release`。 |

処理は次の順に進みます。

1. コンパイルします。診断は標準エラー出力に出ます。エラーの診断があれば終了ステータス 1 で止まります。
2. 参照しているすべてのアセットを `build.runtime-compatibility` に照らして検査します（[ランタイムプロファイル](./projects.md#runtime-profiles)を参照）。問題があれば終了ステータス 1 で止まります。この段階ではビルド出力の書き込みも削除もまだ行っておらず、更新されている可能性があるのは解析キャッシュだけです。
3. ターゲットごとに出力ディレクトリを作り、そこにある古い `game.kagari`、`.kiarchive`、`.kagari-cache` を削除します。続いて `game.kagari`、アーカイブ、`game.kagari-cache`（デバッグプロファイルのときだけ）の順に書き出します。書き出したファイルは 1 つずつ表示します。

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
```

```sh
kagari build
kagari build --manifest-path example/test_game1/kagari.toml
kagari build --target v0
kagari build --profile debug
```

### ビルドプロファイル {#build-profiles}

プロファイルによって、`kagari build` がバイトコードとアーカイブ以外に何を書き出すかが決まります。`game.kagari` はどちらのプロファイルでもバイト単位で同じなので、デバッグビルドもリリースビルドとまったく同じように動きます。

| プロファイル | 書き出すもの |
| ------------ | ------------ |
| `release`（既定） | `game.kagari` とアーカイブ。 |
| `debug` | 上記に加えて、デバッグ用の成果物 `game.kagari-cache`。 |

デバッグ用の成果物は[デバッガー](./debugger.md)などのツールが使うもので、次の内容を含みます。

- 探索した物語のグラフ。解析で見つかった到達可能な物語の状態を、シーンと呼び出し文脈ごとにすべて収めています。各状態には、ステップ（[ステップ ID](../concepts/steps.md#step-ids) で表します）、シーンの呼び出し、乱数の抽選、その状態で保持しているローカル変数と物語変数が記録されます。さらに、選択肢のアーム、抽選された値、シーンの終了のそれぞれについて次の状態と、どのレンダーがキーフレームかも記録されます。
- すべての関数のデバッグ情報。ローカル変数の名前、関数のコード上で各変数がどこに置かれているか、レキシカルスコープを含みます。
- 対応する `game.kagari` のハッシュ。古い成果物を別のビルドと取り違えて読まないよう、照合して拒否するためのものです。

成果物はアーカイブの後に表示されます。

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
wrote target/v0/game.kagari-cache
```

リリースビルドでは、以前のデバッグビルドがターゲットディレクトリに残した `game.kagari-cache` を削除します。ランタイムが `game.kagari-cache` を読むのはデバッガーが接続したときだけなので、配布するゲームには含める必要がありません。

## `kagari clean` {#kagari-clean}

```text
kagari clean [--manifest-path <PATH>]
```

`kagari check`、`kagari build`、`kagari release` が出力ディレクトリ（`build.out-dir`）に書き出したものを削除します。

- [解析キャッシュ](#the-analysis-cache)。出力ディレクトリ直下にあるすべての `.kagari-cache` ファイルです。
- 各ターゲット（`v0`）のディレクトリにある `game.kagari`、すべての `.kiarchive` ファイル、すべての `.kagari-cache` ファイル。`build.targets` に挙げたターゲットかどうかは問いません。
- `release/windows`、`release/linux`、`release/macos` にあるすべての `.iso` と `.zip` のパッケージ。

削除した結果、ターゲットのディレクトリ、リリースのディレクトリ、出力ディレクトリが空になれば、それらも削除します。上記以外のファイルは残し、そのファイルを含むディレクトリも残します。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |

削除したファイルの数を、出力ディレクトリ（プロジェクトのディレクトリからの相対パス）と一緒に表示します。

```text
removed 4 files from target
```

終了ステータスは 0 です。実行できなかった場合（`kagari.toml` が見つからない、入出力エラーなど）は 2 になります。

```sh
kagari clean
kagari clean --manifest-path example/test_game1/kagari.toml
```

## `kagari release` {#kagari-release}

```text
kagari release [--manifest-path <PATH>] (--windows | --linux | --macos)
```

特定のプラットフォームのプレイヤー向けに、ゲームをパッケージにまとめます。ゲームをビルドし、そのプラットフォーム用のランタイムをダウンロードして、両方を 1 つのパッケージとして `<out-dir>/release/<platform>/` に書き出します。パッケージは、Windows と Linux では ISO ディスクイメージ `<name>.iso`、macOS ではアプリ `<name>.app` を収めた ZIP アーカイブ `<name>.zip` です。`<name>` はプロジェクト名（`[project] name`）をもとに、`/ \ : * ? " < > |` と制御文字を `_` に置き換え、前後の空白と末尾のドットを取り除いたものです。実行にはインターネット接続が必要です。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--windows` | Windows（x86_64）のランタイムを `run.exe` という名前で、必要な C ランタイムライブラリ `vcruntime140.dll` と FFmpeg のライブラリとともにパッケージにする。 |
| `--linux` | Linux（x86_64）のランタイム `kagari-desktop-runtime` を、FFmpeg のライブラリとともにパッケージにする。 |
| `--macos` | macOS（Apple シリコン）のランタイムを、FFmpeg のライブラリとともにアドホック署名したアプリとしてパッケージにする。[`project.identifier`](./projects.md#project) が必要。 |

`--windows`、`--linux`、`--macos` のうち、ちょうど 1 つを指定する必要があります。macOS 向けには `kagari.toml` に `project.identifier` が必要で、この値がアプリのバンドル識別子になります。書かれていなければ、ビルドもダウンロードもせずに終了ステータス 2 で止まります。

```text
kagari: a macOS release needs `project.identifier` in kagari.toml (a reverse-DNS name such as `com.example.game`)
```

処理は次の順に進みます。

1. `build.targets` の最初のターゲットを、リリースプロファイルでビルドします。処理も表示内容も [`kagari build`](#kagari-build) とまったく同じです。問題があれば、何もダウンロードせずに終了ステータス 1 で止まります。
2. `kagari` と同じバージョンのランタイムを、`https://kagari-download.suitsu31.club` にあるプラットフォームごとのディレクトリ（`windows_x86_installer`、`linux_x86_bin`、`macos_arm64_bin`）からダウンロードし、公開されている SHA-256 チェックサムと照合します。そのため、ランタイムは常にバイトコードをビルドしたコンパイラと同じバージョンになります。ランタイムが動画のデコードに使う FFmpeg のライブラリと、そのライセンス文書 `ffmpeg-LICENSE.txt` も、同様にダウンロードして照合します。Windows 向けには、Microsoft Visual C++ ランタイムライブラリの `vcruntime140.dll` もダウンロードして照合します。ランタイムはこれがないと起動できませんが、入っていない Windows は珍しくありません。`run.exe` と同じ場所に置けば、インストールしなくても使えます。macOS 向けには、アプリのアイコン `kagari.icns`（Kagari のロゴ）もダウンロードして照合します。ダウンロードに失敗した場合や、このバージョンのランタイムが公開されていない場合は、終了ステータス 2 で止まります（Kagari 0.1.0 はランタイムを公開しておらず、0.3.0 より前のバージョンには macOS 用のランタイムがありません）。ダウンロード元のサーバーは、環境変数 `KAGARI_DOWNLOAD_URL` で変更できます。
3. そのプラットフォーム用の以前のパッケージ（`.iso` と `.zip` のファイル）を削除し、新しいパッケージを書き出します。いったん `<name>.iso.part` または `<name>.zip.part` として書き、完成した時点で名前を変えます。途中で失敗した場合は、書きかけのファイルを削除します。Windows 用と Linux 用のイメージのルートディレクトリには、ランタイム（Windows では `run.exe` と `vcruntime140.dll`、Linux では `kagari-desktop-runtime`）、FFmpeg のライブラリと `ffmpeg-LICENSE.txt`（[ゲームの配布](./projects.md#shipping-a-game)を参照）、`game.kagari`、アーカイブが入ります。macOS 用のアーカイブには、[macOS のアプリ](#the-macos-app)で説明するアプリが入ります。

```text
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/kagari-desktop-runtime-<version>-x86_64-linux
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/libavcodec.so.63
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/libavformat.so.63
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/libavutil.so.61
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/libswscale.so.10
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/libswresample.so.7
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/ffmpeg-LICENSE.txt
wrote target/release/linux/My Game.iso (10 files)
```

イメージは Windows でも（ダブルクリックでマウント）、Linux でも（デスクトップ環境が自動でマウントするか、`mount -o loop` を使う）開けます。ファイル名は書き出したときのままで、Linux 用のランタイムには実行権限が付いています。プレイヤーは、イメージの中にあるランタイム（Windows では `run.exe`）を引数なしで起動します。詳しくは[ゲームの配布](./projects.md#shipping-a-game)を参照してください。

```sh
kagari release --windows
kagari release --linux --manifest-path example/test_game1/kagari.toml
kagari release --macos
```

### macOS のアプリ {#the-macos-app}

同じプロジェクトで `kagari release --macos` を実行すると、次のように表示されます。

```text
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/kagari-desktop-runtime-<version>-aarch64-macos
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/libavcodec.63.dylib
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/libavformat.63.dylib
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/libavutil.61.dylib
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/libswscale.10.dylib
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/libswresample.7.dylib
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/ffmpeg-LICENSE.txt
downloading https://kagari-download.suitsu31.club/macos_arm64_bin/<version>/kagari.icns
wrote target/release/macos/My Game.zip (13 files)
```

アーカイブには、アプリバンドルが 1 つ入っています。

```text
<name>.app/Contents/
  Info.plist                   the app's name, bundle identifier, version and icon
  MacOS/<name>                 the runtime
  Frameworks/*.dylib           the FFmpeg libraries
  Resources/                   game.kagari, *.kiarchive, kagari.icns, ffmpeg-LICENSE.txt
  _CodeSignature/CodeResources the hashes of the files the signature covers
```

アプリの名前はプロジェクト名（`[project] name`）、バンドル識別子は `project.identifier`、バージョンは `project.version` の `major.minor.patch` になります。動作には macOS 11 以降が必要です。ランタイムは、ゲームを `Contents/Resources` から、FFmpeg のライブラリを `Contents/Frameworks` から読み込みます。アーカイブにはファイルのパーミッションも記録されているので、Finder（ダブルクリック）や `ditto -x -k` で展開すれば、アプリはそのまま起動できます。プレイヤーは、展開した `<name>.app` を開きます。

ランタイムと FFmpeg のライブラリにはアドホック署名を施してあり、署名はアプリ内のすべてのファイルを対象にしています。ファイルを 1 つでも変更すると、署名は無効になります。この署名には Apple の Developer ID が含まれないため、ダウンロードしたアプリを初めて起動すると、macOS は「Apple が検証できなかった」と表示して開きません。プレイヤーは最初の 1 回だけ、**システム設定 → プライバシーとセキュリティ → このまま開く** から開く必要があります（macOS 14 以前では、Control キーを押しながらアプリをクリックし、**開く** を選びます）。2 回目以降は何もしなくても起動します。署名に Mac は必要なく、`kagari release --macos` は Linux や Windows でも実行できます。ダウンロードしたランタイムが arm64 の macOS 用実行ファイルでなかった場合は、アーカイブを書き出さずに終了ステータス 2 で止まります。

## `kagari fmt` {#kagari-fmt}

```text
kagari fmt [--check] [--manifest-path <PATH>] [FILES]...
```

ソースファイルを整形し、その場で書き換えます。`FILES` を指定しなければ、プロジェクトのルートファイルがあるディレクトリ（通常は `src/`）以下の `.kis` と `.kix` ファイルをすべて整形します。名前が `.` で始まるディレクトリは対象外です。`FILES` を指定すると、そのファイルだけを整形します。この場合、プロジェクトは必要ありません。

| オプション | 意味 |
| ---------- | ---- |
| `FILES` | プロジェクトのファイルの代わりに整形するファイル。拡張子は `.kis` か `.kix`（それ以外は終了ステータス 2）。 |
| `--check` | ファイルを書き換えず、変更が必要なファイルごとに `would reformat <file>` と表示する。 |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`（`FILES` を指定したときは無視）。 |

整形したファイルごとに `formatted <file>` と表示します。構文エラーのあるファイルは書き換えず、`<file>: cannot format: N syntax error(s)` と、最初のエラーの位置とメッセージを表示します。`--check` で整形の必要なファイルが見つかった場合や、整形できないファイルがあった場合は、終了ステータス 1 で終わります。整形のスタイルについては[エディタ対応](./editors.md#the-formatter)で説明しています。

```sh
kagari fmt
kagari fmt --check
kagari fmt src/scenario/mod.kix
```

## `kagari disasm` {#kagari-disasm}

```text
kagari disasm <FILE> [--all]
```

`.kagari` ファイルをデコードして検証し、中の関数をテキストで表示します。コンパイラが何を出力したかを確かめたり、受け取ったファイルの中身を調べたりするときに使います。

| オプション | 意味 |
| ---------- | ---- |
| `FILE` | 対象の `.kagari` ファイル。 |
| `--all` | 標準ライブラリの関数も表示する。既定では、ゲーム自身の関数（パスが `crate::` で始まるもの）だけを表示。 |

正しい `.kagari` ファイルでなければ、理由を標準エラー出力に出し、終了ステータス 1 で終わります。出力するテキストの形式は安定していません。

```sh
kagari disasm target/v0/game.kagari
```

## `kagari lsp` {#kagari-lsp}

```text
kagari lsp
```

言語サーバーを起動します。通信には標準入出力上の Language Server Protocol を使います。エディタが起動するもので、手で実行することはありません。[エディタ対応](./editors.md#the-language-server)を参照してください。

## `kagari debug` {#kagari-debug}

```text
kagari debug [--manifest-path <PATH>] [--target <TARGET>] [--entry <PATH>] [--seed <N>] [--no-build] [--watch]
kagari debug --window [--runtime <PATH>] [--manifest-path <PATH>] [--target <TARGET>] [--no-build] [--watch]
kagari debug --attach <ADDRESS> [--manifest-path <PATH>] [--target <TARGET>] [--watch]
```

プロジェクトをデバッグプロファイルでビルドし、物語をデバッグします。既定ではクエリエンジン（ウィンドウを出さないゲーム）の上でデバッグし、`--window` を付けるとウィンドウ付きのゲームで、`--attach` を付けると `--debug-port` 付きで実行中のゲームに接続してデバッグします。コマンドは標準入力から 1 行に 1 つずつ読み込みます。`--watch` を付けると、プロジェクトのファイルが変わるたびにゲームを再ビルドし、ホットスワップします。オプションとコマンドの詳細は[デバッガー](./debugger.md#kagari-debug)にあります。入力が終わるか `quit` で終了すると終了ステータス 0、プロジェクトをビルドできない場合やゲームに接続できない場合は 2 になります。

## `kagari dap` {#kagari-dap}

```text
kagari dap
```

デバッグアダプターを起動します。通信には標準入出力上の Debug Adapter Protocol を使います。エディタが起動するもので、手で実行することはありません。launch と attach の設定は[デバッガー](./debugger.md#editors)にあります。
