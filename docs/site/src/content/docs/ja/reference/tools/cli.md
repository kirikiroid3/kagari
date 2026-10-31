---
title: "kagari CLI"
---

`kagari` は Kagari のツールチェインです。プロジェクトの作成・検査・ビルド・掃除・整形、プレイヤー向けのゲームのパッケージ化、`.kagari` ファイルの逆アセンブル、言語サーバーの実行、物語のデバッグを行います。

```text
kagari <COMMAND>

Commands:
  init     Create a new project in PATH
  check    Check the project and print its diagnostics
  build    Build the project's bytecode and asset archives
  clean    Delete the build outputs and the analysis cache
  release  Build the project and package it with the runtime of a platform into an ISO image
  fmt      Format the project's source files, or the given files
  disasm   Disassemble a `.kagari` bytecode file
  lsp      Run the language server over stdio
  debug    Debug the story: the query engine's REPL on the headless game (by default), a game in a window (`--window`), or a running game (`--attach`)
  dap      Run the debug adapter (Debug Adapter Protocol) over stdio
  help     Print this message or the help of the given subcommand(s)
```

`kagari --version` はツールチェインのバージョンを表示します。`kagari.toml` の `[toolchain] kagari` はこのバージョンに合致する必要があります。`kagari help <COMMAND>` または `kagari <COMMAND> --help` で各コマンドのヘルプを表示します。

## インストール {#installing}

[Kagari のインストール](../../tutorial/install.md) のインストーラーは、Linux と Windows で `kagari` と `kagari-desktop-runtime` を `PATH` に置きます。NixOS ではそこで説明している flake を使います。

代わりに Kagari リポジトリから両方をビルドするには、次のようにします（Rust と Cargo が必要です）。

```sh
cargo install --path tooling/kagari-cli
cargo install --path runtime/kagari-desktop-runtime
```

これで両方が Cargo の `bin` ディレクトリにインストールされます。リポジトリ内では、インストールせずに `cargo run -p kagari-cli -- <COMMAND>` で `kagari` を実行することもできます。

## プロジェクトの探索 {#finding-the-project}

`check`、`build`、`clean`、`release`、（ファイル指定なしの）`fmt` はプロジェクトに対して動作します。`--manifest-path <PATH>` で指定された `kagari.toml` を使い、指定がなければカレントディレクトリとその親ディレクトリで最初に見つかった `kagari.toml` を使います。マニフェストについては[プロジェクト](./projects.md)を参照してください。

## 終了ステータス {#exit-statuses}

すべてのコマンドが同じ終了ステータスを使います。

| ステータス | 意味 |
| ---------: | ---- |
| 0 | 成功。 |
| 1 | コマンドは実行され、問題が見つかった。エラーの診断、非対応のアセット形式、整形されていないファイル、不正な `.kagari` ファイルなど。 |
| 2 | コマンドを実行できなかった。使い方の誤り、IO エラー、`kagari.toml` が見つからない、マニフェストが不正、ツールチェインのバージョン不一致、ランタイムをダウンロードできないなど。メッセージは `kagari: <message>` の形で stderr に出力されます。 |

## コンパイラオプション {#compiler-options}

コンパイラにはコマンドラインフラグがありません。オプションは `kagari.toml` のキーなので、`check`、`build`、言語サーバーの結果は常に一致します。

| キー | 既定値 | 効果 |
| ---- | -----: | ---- |
| `[build] state-cap` | 200 000 | 解析が探索する物語の状態数の上限（超えると `W5006`）。 |
| `[build] recursion-depth` | 2048 | 実行時の呼び出しの入れ子の上限。 |
| `[build] keyframe-check` | `false` | キーフレーム配置の検査（`W5011`）を行う。 |
| `[assets] system-fonts` | `true` | `system_font("…")` がインストール済みフォントを指せるようにする（オフのときは `E1031`）。 |

詳しくは[コンパイラオプション](./projects.md#compiler-options)を参照してください。

## 解析キャッシュ {#the-analysis-cache}

物語の解析は、プロジェクトのコンパイルで最も時間のかかる部分です。停止性、到達可能性、行き詰まり選択肢、画面とその一貫性、キーフレーム、ゲームシステムを検査するために、物語のすべての状態を探索します（[物語の解析](../../diagnostics/analysis.md)、[ゲームシステム](../../diagnostics/system.md)）。`kagari check` と `kagari build` はその結果を `<out-dir>/analysis.kagari-cache`（既定では `target/analysis.kagari-cache`）に保存し、解析が依存するものが変わらない間はそれを再利用します。

- プログラムのすべてのソースファイルの内容（標準ライブラリを含む）
- [コンパイラオプション](#compiler-options)（`state-cap`、`recursion-depth`、`keyframe-check`、`system-fonts`）
- アセットディレクトリとシステムフォントのディレクトリの場所
- ツールチェインのバージョン

ファイルを書き出した時点からこれらがすべて変わっていなければ、解析は再実行されず、その診断はファイルから取り出されます。出力は新たに解析した場合と同じです。そうでなければ解析が実行され、ファイルは書き直されます。それより前の検査がエラーを報告している間は、解析もファイルの書き出しも行われません。このため、`kagari check` も出力ディレクトリに書き込みます。

ファイルが存在しない、読めない、または不正な場合は無視され、置き換えられます。ファイルを書き出せないときは `kagari: warning: cannot write the analysis cache: <reason>` を stderr に出力し、終了ステータスを変えずにコマンドを続けます。ファイルは [`kagari clean`](#kagari-clean) で削除できます。コンパイラ自体を開発していて、バージョンを変えずにコンパイラを変更した場合は `kagari clean` を実行してください。キャッシュは 2 つのコンパイラを区別できないためです。

## `kagari init` {#kagari-init}

```text
kagari init [PATH] [--name <NAME>]
```

`PATH`（既定はカレントディレクトリ）に新しいプロジェクトを作ります。ディレクトリがなければ作成します。

| オプション | 意味 |
| ---------- | ---- |
| `PATH` | プロジェクトのディレクトリ。既定は `.`。 |
| `--name <NAME>` | プロジェクト名。既定はディレクトリ名。 |

`kagari init` は新しいファイルしか作りません。テンプレートのファイルが 1 つでも `PATH` に既に存在すれば、何も変更せずにステータス 2 で止まります。書き出す構成は[プロジェクトの構成](./projects.md#project-layout)のとおりで、`[toolchain] kagari` には実行中のツールチェインの `major.minor` が入ります。

テンプレートの `src/main.kis` はゲームの既定フォント（`FONT`）を宣言します。`kagari init` は[システムフォントのディレクトリ](./projects.md#assets)からインストール済みのフォントを選び、`system_font("…")` を書き込みます。日本語のゴシック体（游ゴシック、メイリオ、MS ゴシック、ヒラギノ角ゴシック、Noto Sans CJK/JP）を優先し、次に一般的なサンセリフ体（Segoe UI、Arial、Helvetica、Noto Sans、DejaVu Sans、Liberation Sans）を選び、どれもなければ最初に見つかったフォントファイルを使います。フォントが 1 つもインストールされていない場合は `asset("fonts/default.ttf")` を書き込み、`assets/fonts/default.ttf` にフォントを置くか `FONT` を変更するよう警告します。

```sh
kagari init my-game
kagari init --name "My Game"
```

## `kagari check` {#kagari-check}

```text
kagari check [--manifest-path <PATH>] [--resolutions]
```

プロジェクトをコンパイルし、すべての診断を stdout に出力します。ビルド出力は書き出さず、書き出すのは[解析キャッシュ](#the-analysis-cache)だけです。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--resolutions` | プログラム中の各名前が何に解決されるかも出力します。インポートのデバッグに便利です。 |

エラーの診断が 1 つでもあればステータス 1 で終了します。警告だけならステータス 0 です。診断コードの一覧は[診断](../../diagnostics/index.md)にあります。

## `kagari build` {#kagari-build}

```text
kagari build [--manifest-path <PATH>] [--target <TARGET>]... [--profile <PROFILE>]
```

プロジェクトをコンパイルし、ターゲットごとに `<out-dir>/<target>/game.kagari` とアセットアーカイブを書き出します。`--profile debug` のときは `game.kagari-cache` も書き出します。[ターゲットと出力ディレクトリ](./projects.md#targets-and-output-directories)と[アセットとアーカイブ](./projects.md#assets-and-archives)を参照してください。`kagari check` と同じく、[解析キャッシュ](#the-analysis-cache)を再利用・更新します。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--target <TARGET>` | このターゲットだけをビルドします。複数回指定できます。それぞれ `build.targets` に含まれている必要があり、含まれていなければステータス 2 です。既定は `build.targets` のすべてのターゲット。 |
| `--profile <PROFILE>` | `release` または `debug`。[ビルドプロファイル](#build-profiles)を参照してください。既定は `release`。 |

処理の順序:

1. コンパイルします。診断は stderr に出力されます。エラーの診断があればステータス 1 で止まります。
2. 参照されるすべてのアセットを `build.runtime-compatibility` に照らして検査します（[ランタイムプロファイル](./projects.md#runtime-profiles)）。問題があればステータス 1 で止まります。この時点ではまだビルド出力を何も書き出しも削除もしていません。更新されている可能性があるのは解析キャッシュだけです。
3. ターゲットごとに、出力ディレクトリを作り、そこにある古い `game.kagari`、`.kiarchive`、`.kagari-cache` ファイルを削除し、`game.kagari`、続いてアーカイブ、（デバッグプロファイルのときだけ）`game.kagari-cache` を書き出します。書き出したファイルは 1 つずつ表示されます。

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

プロファイルは、`kagari build` がバイトコードとアーカイブのほかに何を書き出すかを決めます。`game.kagari` ファイルはどちらのプロファイルでもバイト単位で同一なので、デバッグビルドはリリースビルドとまったく同じように動作します。

| プロファイル | 書き出すもの |
| ------------ | ------------ |
| `release`（既定） | `game.kagari` とアーカイブ。 |
| `debug` | 同じものに加えて、デバッグ用成果物 `game.kagari-cache`。 |

デバッグ用成果物は[デバッガー](./debugger.md)などのツールのためのもので、次の内容を収めます。

- 探索された物語のグラフ。解析が見つけた到達可能なすべての物語状態を、シーンと呼び出し文脈ごとに収めます。各状態には、ステップ（[ステップ ID](../concepts/steps.md#step-ids) で示します）、シーン呼び出し、乱数の抽選、その状態で保持されるローカル変数と物語変数、選択肢のアーム・抽選された値・シーンの終了ごとの後続状態、どのレンダーがキーフレームかが含まれます。
- すべての関数のデバッグ情報。ローカル変数の名前、関数のコード上でそれらが置かれる場所、レキシカルスコープです。
- 対応する `game.kagari` のハッシュ。古い成果物を別のビルドに対して読まず、拒否できるようにするためのものです。

これはアーカイブの後に表示されます。

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
wrote target/v0/game.kagari-cache
```

リリースビルドは、以前のデバッグビルドがターゲットディレクトリに残した `game.kagari-cache` を削除します。ランタイムが `game.kagari-cache` を読むのはデバッガーが接続したときだけなので、出荷するゲームには不要です。

## `kagari clean` {#kagari-clean}

```text
kagari clean [--manifest-path <PATH>]
```

`kagari check`、`kagari build`、`kagari release` が出力ディレクトリ（`build.out-dir`）に書き出したものを削除します。

- [解析キャッシュ](#the-analysis-cache)。出力ディレクトリ直下のすべての `.kagari-cache` ファイルです。
- すべてのターゲット（`v0`）のディレクトリにある `game.kagari`、すべての `.kiarchive` ファイル、すべての `.kagari-cache` ファイル。ターゲットが `build.targets` に含まれているかどうかは問いません。
- `release/windows` と `release/linux` にあるすべての `.iso` ファイル。

その後、空になったターゲットとリリースのディレクトリ、出力ディレクトリを削除します。それ以外のファイルは残り、それを収めたディレクトリも残ります。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |

削除したファイルの数を、プロジェクトのディレクトリからの相対パスで示した出力ディレクトリとともに表示します。

```text
removed 4 files from target
```

ステータス 0 で終了します。実行できないとき（`kagari.toml` が見つからない、IO エラー）はステータス 2 です。

```sh
kagari clean
kagari clean --manifest-path example/test_game1/kagari.toml
```

## `kagari release` {#kagari-release}

```text
kagari release [--manifest-path <PATH>] (--windows | --linux)
```

1 つのプラットフォームのプレイヤー向けにゲームをパッケージ化します。ゲームをビルドし、そのプラットフォームのランタイムをダウンロードして、両方を 1 つの ISO ディスクイメージ `<out-dir>/release/<platform>/<name>.iso` に書き出します。`<name>` はプロジェクト名（`[project] name`）で、`/ \ : * ? " < > |` と制御文字は `_` に置き換えられ、前後の空白と末尾のドットは取り除かれます。インターネット接続が必要です。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--windows` | Windows（x86_64）のランタイムを `run.exe` として、必要な C ランタイムライブラリ `vcruntime140.dll` と一緒にパッケージ化します。 |
| `--linux` | Linux（x86_64）のランタイム `kagari-desktop-runtime` をパッケージ化します。 |

`--windows` と `--linux` のどちらか一方だけが必要です。手順は次の順です。

1. `build.targets` の最初のターゲットを、[`kagari build`](#kagari-build) とまったく同じようにリリースプロファイルでビルドし、同じ行を表示します。問題があれば、何もダウンロードせずにステータス 1 で止まります。
2. `kagari` と同じバージョンのランタイムを `https://kagari-download.suitsu31.club` からダウンロードし、公開されている SHA-256 チェックサムと照合します。そのため、ランタイムはバイトコードをビルドしたコンパイラと常に一致します。Windows 向けでは、Microsoft Visual C++ のランタイムライブラリ `vcruntime140.dll` もダウンロードして照合します。ランタイムはこれがないと起動できず、プレイヤーの Windows には入っていないことがよくあります。`run.exe` の隣に置けば、インストールせずに使われます。ダウンロードに失敗したとき、またはこのバージョンのランタイムが公開されていないとき（Kagari 0.1.0 は公開していません）はステータス 2 で止まります。環境変数 `KAGARI_DOWNLOAD_URL` でサーバーを置き換えられます。
3. そのプラットフォームの以前の `.iso` ファイルを削除し、イメージを書き出します。イメージのルートディレクトリには、ランタイム（Windows では `run.exe` と `vcruntime140.dll`、Linux では `kagari-desktop-runtime`）、`game.kagari`、アーカイブが入ります。

```text
downloading https://kagari-download.suitsu31.club/linux_x86_bin/<version>/kagari-desktop-runtime-<version>-x86_64-linux
wrote target/release/linux/My Game.iso (4 files)
```

イメージは Windows（ダブルクリックでマウントされます）でも Linux（デスクトップがマウントするか、`mount -o loop`）でも開け、ファイル名は書き出したとおりで、Linux のランタイムは実行可能です。プレイヤーはその中のランタイム（Windows では `run.exe`）を引数なしで起動します。[ゲームの配布](./projects.md#shipping-a-game)を参照してください。

```sh
kagari release --windows
kagari release --linux --manifest-path example/test_game1/kagari.toml
```

## `kagari fmt` {#kagari-fmt}

```text
kagari fmt [--check] [--manifest-path <PATH>] [FILES]...
```

ソースファイルをその場で整形します。`FILES` がなければ、プロジェクトのルートファイルのディレクトリ（通常は `src/`）以下にあるすべての `.kis` と `.kix` ファイルを整形します。名前が `.` で始まるディレクトリは飛ばします。`FILES` を指定すると、そのファイルだけを整形し、プロジェクトは不要です。

| オプション | 意味 |
| ---------- | ---- |
| `FILES` | プロジェクトのファイルの代わりに整形するファイル。`.kis` か `.kix` で終わる必要があります（そうでなければステータス 2）。 |
| `--check` | 書き込まず、変更されるファイルごとに `would reformat <file>` を表示します。 |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`（`FILES` を指定したときは無視されます）。 |

変更したファイルごとに `formatted <file>` を表示します。構文エラーのあるファイルは変更せず、`<file>: cannot format: N syntax error(s)` と、最初のエラーの位置とメッセージを表示します。`--check` で整形が必要なファイルが見つかったとき、または整形できないファイルがあったときはステータス 1 で終了します。整形のスタイルは[エディタ対応](./editors.md#the-formatter)で説明しています。

```sh
kagari fmt
kagari fmt --check
kagari fmt src/scenario/mod.kix
```

## `kagari disasm` {#kagari-disasm}

```text
kagari disasm <FILE> [--all]
```

`.kagari` ファイルをデコード・検証し、その関数をテキストで表示します。コンパイラが何を生成したかを見たり、受け取ったファイルを調べたりするのに使います。

| オプション | 意味 |
| ---------- | ---- |
| `FILE` | `.kagari` ファイル。 |
| `--all` | std の関数も表示します。既定ではゲーム自身の関数（パスが `crate::` で始まるもの）だけを表示します。 |

ファイルが正しい `.kagari` ファイルでなければ、理由を stderr に出力してステータス 1 になります。テキスト形式は安定していません。

```sh
kagari disasm target/v0/game.kagari
```

## `kagari lsp` {#kagari-lsp}

```text
kagari lsp
```

言語サーバーを実行し、stdin と stdout で Language Server Protocol を話します。エディタが起動するもので、手で実行するものではありません。[エディタ対応](./editors.md#the-language-server)を参照してください。

## `kagari debug` {#kagari-debug}

```text
kagari debug [--manifest-path <PATH>] [--target <TARGET>] [--entry <PATH>] [--seed <N>] [--no-build] [--watch]
kagari debug --window [--runtime <PATH>] [--manifest-path <PATH>] [--target <TARGET>] [--no-build] [--watch]
kagari debug --attach <ADDRESS> [--manifest-path <PATH>] [--target <TARGET>] [--watch]
```

プロジェクトをデバッグプロファイルでビルドし、その物語をデバッグします。既定ではクエリエンジン（ウィンドウなしのゲーム）上で、`--window` ではウィンドウで、`--attach` では `--debug-port` 付きですでに実行中のゲームでデバッグします。stdin から 1 行に 1 コマンドを読みます。`--watch` は、プロジェクトのファイルが変わるたびにゲームを再ビルドしてホットスワップします。オプションとコマンドは[デバッガー](./debugger.md#kagari-debug)にあります。入力が終わるか `quit` の後はステータス 0、プロジェクトをビルドできないかゲームに接続できないときはステータス 2 です。

## `kagari dap` {#kagari-dap}

```text
kagari dap
```

デバッグアダプターを実行し、stdin と stdout で Debug Adapter Protocol を話します。エディタが起動するもので、手で実行するものではありません。launch と attach の設定は[デバッガー](./debugger.md#editors)にあります。
