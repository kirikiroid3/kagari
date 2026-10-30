---
title: "エディタ対応"
sidebar:
  label: "エディタ支援"
---

Kagari はエディタ向けに 4 つの部品を提供します。

| 部品 | 提供するもの | 場所 |
| ---- | ------------ | ---- |
| 言語サーバー | 診断、整形、定義へ移動、ホバー、ドキュメントのアウトライン | `kagari lsp` |
| デバッグアダプター | ブレークポイント、ステップ実行、物語のコールスタックと変数、デバッガーのコマンド | `kagari dap` |
| tree-sitter 文法 | `.kis` と `.kix` の構文ハイライトと構造 | `tooling/tree-sitter-kagari` |
| Zed 拡張機能 | 上のすべてを Zed にそのまま導入できる形にしたもの | `tooling/zed-kagari` |

いずれも `kagari` 実行ファイルが必要です。[インストール](./cli.md#installing)を参照してください。

## 言語サーバー {#the-language-server}

`kagari lsp` は、stdin と stdout で Language Server Protocol を話す言語サーバーを実行します。`.kis` と `.kix` で終わるファイルに対して、コマンド `kagari` を引数 `lsp` で起動するようエディタを設定してください。

### 機能 {#features}

| 機能 | LSP リクエスト | 備考 |
| ---- | -------------- | ---- |
| 診断 | `textDocument/publishDiagnostics` | `kagari check` と同じ診断を、開いていないファイルも含めプロジェクトのすべてのファイルについて出します。編集のたびにバックグラウンドで再計算します。 |
| 整形 | `textDocument/formatting` | `kagari fmt` と同じ出力です。構文エラーのあるファイルは変更しません。 |
| 定義へ移動 | `textDocument/definition` | パス、ローカル束縛、メソッド呼び出しで動作します。std の宣言へも移動できます（後述）。フィールドアクセスと `use` のパスは移動の対象外です。 |
| ホバー | `textDocument/hover` | カーソル下のもののヘッダー（シグネチャ）または型と、その `///` ドキュメントコメントを表示します。 |
| ドキュメントシンボル | `textDocument/documentSymbol` | ファイルのアイテムを、書かれたとおりの入れ子で、列挙型のバリアントも含めて返します。エディタはこれをアウトラインとして表示します。 |

その他のリクエストには対応していません。サーバーはドキュメント全体を同期するので、保存していないテキストも反映されます。

### 言語サーバーにおけるプロジェクト {#projects-in-the-language-server}

サーバーは、ルートに `kagari.toml` があるワークスペースフォルダごとにプロジェクトを読み込みます。また開いたファイルごとに、そのディレクトリから親ディレクトリへ `kagari.toml` を探して見つかったプロジェクトを読み込みます。ファイルは、それを含むプロジェクトのうちルートが最も深いものの一部として検査されます。`kagari check` と同様に、マニフェストの[コンパイラオプション](./projects.md#compiler-options)が適用されます。

どのプロジェクトにも属さないファイルには構文の診断だけが出ます。`kagari.toml` が不正な場合や別のツールチェインのバージョンを要求する場合、サーバーはそのエラーをエディタにメッセージとして表示します。

エディタがファイル監視に対応していれば、サーバーは `.kis`/`.kix` ファイル、`kagari.toml` ファイル、アセットディレクトリ内のファイルの作成と削除を監視します。そのため、アセットファイルを追加・削除すると診断が更新されます。`kagari.toml` が変更されるとプロジェクトを読み込み直します。

std のソースはコンパイラに内蔵されています。定義へ移動でそれを開けるように、サーバーはシステムの一時ディレクトリの `kagari-std-<version>` にコピーを書き出します。このコピーを編集しても効果はありません。

## フォーマッタ {#the-formatter}

`kagari fmt` と言語サーバーの整形は同じフォーマッタを使います。rustfmt に似たスタイルで、設定項目はありません。

- 行はできる限り **100 桁**に収まるように折り返されます。幅は表示幅で数えるため、全角文字は 2 桁です。
- インデントは 1 段につき **4 スペース**です。
- グループ（引数リスト、フォーム、メソッドチェーン、ブロック）は、1 行に収まればそのまま 1 行にし、収まらなければ要素ごとに改行します。それに合わせてリストのカンマを追加・削除します。
- コメントはすべて保持され、属するコードに付いたままになります。コードと同じ行の後ろにあるコメントはその行末に残り、それ以外のコメントは次のトークンの前に残ります。
- 連続する空行は 1 行にまとめられます。行末に空白は残りません。ファイルはちょうど 1 つの改行で終わります（空のファイルは空のままです）。
- フォーム（`s!`、`choice!`、`t![…]`、UI フォーム）も整形されます。

構文エラーのあるファイルは決して整形されません。フォーマッタは整形後に出力を再パースし、入力と同じトークンとコメントを持つことを確かめます。一致しなければ（フォーマッタのバグです）エラーとして整形を拒否し、ファイルは変更されません。

コマンドラインのフラグは [`kagari fmt`](./cli.md#kagari-fmt) を参照してください。

## Zed {#zed}

`tooling/zed-kagari` の Zed 拡張機能は、`.kis` と `.kix` ファイル向けの `Kagari` 言語を提供します。tree-sitter 文法によるハイライト、アウトライン、インデント、括弧の規則と、言語サーバーです。

インストール手順:

1. `kagari` をインストールし、`PATH` に置きます（`cargo install --path tooling/kagari-cli`）。
2. Zed でコマンド **zed: install dev extension** を実行し、Kagari リポジトリの `tooling/zed-kagari` ディレクトリを選びます。

拡張機能は `kagari lsp` を起動します。別の実行ファイルや引数を使うには、Zed の `settings.json` で設定します。

```json
{
  "lsp": {
    "kagari": {
      "binary": {
        "path": "/path/to/kagari",
        "arguments": ["lsp"]
      }
    }
  }
}
```

`kagari` が設定されておらず `PATH` にもない場合、Zed は `` `kagari` was not found on PATH `` と報告します。

### Zed でのデバッグ {#debugging-in-zed}

拡張機能はデバッグアダプター `Kagari` も登録します。これはワークツリーで `kagari dap` を実行します（すべての設定は[デバッガー](./debugger.md#editors)を参照してください）。デバッグシナリオを `.zed/debug.json` に追加します。

```json
[
  {
    "label": "Kagari: run in a window",
    "adapter": "Kagari",
    "request": "launch",
    "mode": "window",
    "stopOnEntry": true,
    "watch": true
  },
  {
    "label": "Kagari: query engine",
    "adapter": "Kagari",
    "request": "launch",
    "mode": "headless",
    "seed": 1
  },
  {
    "label": "Kagari: attach",
    "adapter": "Kagari",
    "request": "attach",
    "port": 4711
  }
]
```

launch はまずデバッグプロファイルでビルドします（`"buildFirst": false` で省略できます）。ブレークポイントは行をクリックして設定します。`s!(…)` や `choice!{…}` のある行は、物語がそのステップに入ったときに止まります。物語のコードのそれ以外の行は、そのコードが実行される前に止まります。デバッグコンソールは `ls` や `nav crate::story::Ending#0` などのデバッガーのコマンドを受け付けます。`"watch": true` にすると、ソースファイルを保存するたびにゲームが再ビルドされ、実行中のセッションに[ホットスワップ](./debugger.md#hot-swap)されます。デバッグコンソールの `swap` は同じことをその場で行います。

## その他のエディタ {#other-editors}

LSP クライアントを持つエディタなら言語サーバーを使えます。ファイル拡張子 `kis` と `kix` に対してコマンド `kagari lsp` を登録してください。Debug Adapter Protocol クライアントを持つエディタならデバッグアダプターを使えます。[デバッガー](./debugger.md#editors)の launch と attach の設定で、`kagari dap` を stdio で起動してください。

構文ハイライトには `tooling/tree-sitter-kagari` の tree-sitter 文法（`grammar.js` と、`src/` の生成済みパーサー）を使います。スコープは `source.kagari` で、`.kis` と `.kix` の両方を扱います。この文法向けのハイライト・アウトライン・インデント・括弧のクエリは `tooling/zed-kagari/languages/kagari/`（`highlights.scm`、`outline.scm`、`indents.scm`、`brackets.scm`）にあります。Neovim や Helix などのエディタでも、キャプチャ名をそのエディタの慣習に合わせて調整すれば再利用できます。
