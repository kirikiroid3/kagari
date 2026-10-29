---
title: "kagari CLI"
---

`kagari` は Kagari のツールチェインです。プロジェクトの作成・検査・ビルド・整形、`.kagari` ファイルの逆アセンブル、言語サーバーの実行を行います。

```text
kagari <COMMAND>

Commands:
  init    Create a new project in PATH
  check   Check the project and print its diagnostics
  build   Build the project's bytecode and asset archives
  fmt     Format the project's source files, or the given files
  disasm  Disassemble a `.kagari` bytecode file
  lsp     Run the language server over stdio
  help    Print this message or the help of the given subcommand(s)
```

`kagari --version` はツールチェインのバージョンを表示します。`kagari.toml` の `[toolchain] kagari` はこのバージョンに合致する必要があります。`kagari help <COMMAND>` または `kagari <COMMAND> --help` で各コマンドのヘルプを表示します。

## インストール {#installing}

Kagari リポジトリからツールをビルドします。

```sh
cargo install --path tooling/kagari-cli
```

これで `kagari` 実行ファイルが Cargo の `bin` ディレクトリにインストールされます。リポジトリ内では、インストールせずに `cargo run -p kagari-cli -- <COMMAND>` で実行することもできます。

## プロジェクトの探索 {#finding-the-project}

`check`、`build`、（ファイル指定なしの）`fmt` はプロジェクトに対して動作します。`--manifest-path <PATH>` で指定された `kagari.toml` を使い、指定がなければカレントディレクトリとその親ディレクトリで最初に見つかった `kagari.toml` を使います。マニフェストについては[プロジェクト](./projects.md)を参照してください。

## 終了ステータス {#exit-statuses}

すべてのコマンドが同じ終了ステータスを使います。

| ステータス | 意味 |
| ---------: | ---- |
| 0 | 成功。 |
| 1 | コマンドは実行され、問題が見つかった。エラーの診断、非対応のアセット形式、整形されていないファイル、不正な `.kagari` ファイルなど。 |
| 2 | コマンドを実行できなかった。使い方の誤り、IO エラー、`kagari.toml` が見つからない、マニフェストが不正、ツールチェインのバージョン不一致など。メッセージは `kagari: <message>` の形で stderr に出力されます。 |

## コンパイラオプション {#compiler-options}

コンパイラにはコマンドラインフラグがありません。オプションは `kagari.toml` のキーなので、`check`、`build`、言語サーバーの結果は常に一致します。

| キー | 既定値 | 効果 |
| ---- | -----: | ---- |
| `[build] state-cap` | 200 000 | 解析が探索する物語の状態数の上限（超えると `W5006`）。 |
| `[build] recursion-depth` | 2048 | 実行時の呼び出しの入れ子の上限。 |
| `[build] keyframe-check` | `false` | キーフレーム配置の検査（`W5011`）を行う。 |
| `[assets] system-fonts` | `true` | `system_font("…")` がインストール済みフォントを指せるようにする（オフのときは `E1031`）。 |

詳しくは[コンパイラオプション](./projects.md#compiler-options)を参照してください。

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

何も書き出さずにプロジェクトをコンパイルし、すべての診断を stdout に出力します。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--resolutions` | プログラム中の各名前が何に解決されるかも出力します。インポートのデバッグに便利です。 |

エラーの診断が 1 つでもあればステータス 1 で終了します。警告だけならステータス 0 です。診断コードの一覧は[診断](../../diagnostics/index.md)にあります。

## `kagari build` {#kagari-build}

```text
kagari build [--manifest-path <PATH>] [--target <TARGET>]...
```

プロジェクトをコンパイルし、ターゲットごとに `<out-dir>/<target>/game.kagari` とアセットアーカイブを書き出します。[ターゲットと出力ディレクトリ](./projects.md#targets-and-output-directories)と[アセットとアーカイブ](./projects.md#assets-and-archives)を参照してください。

| オプション | 意味 |
| ---------- | ---- |
| `--manifest-path <PATH>` | プロジェクトの `kagari.toml`。 |
| `--target <TARGET>` | このターゲットだけをビルドします。複数回指定できます。それぞれ `build.targets` に含まれている必要があり、含まれていなければステータス 2 です。既定は `build.targets` のすべてのターゲット。 |

処理の順序:

1. コンパイルします。診断は stderr に出力されます。エラーの診断があればステータス 1 で止まります。
2. 参照されるすべてのアセットを `build.runtime-compatibility` に照らして検査します（[ランタイムプロファイル](./projects.md#runtime-profiles)）。問題があればステータス 1 で止まります。この時点ではまだ何も書き出しも削除もしていません。
3. ターゲットごとに、出力ディレクトリを作り、そこにある古い `game.kagari` と `.kiarchive` ファイルを削除し、`game.kagari`、続いてアーカイブを書き出します。書き出したファイルは 1 つずつ表示されます。

```text
wrote target/v0/game.kagari
wrote target/v0/bgimage.kiarchive (3 files)
```

```sh
kagari build
kagari build --manifest-path example/test_game1/kagari.toml
kagari build --target v0
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
