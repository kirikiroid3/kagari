# ランタイム {#the-runtime}

`kagari-desktop-runtime` は、`kagari build` でビルドしたゲームを Windows、macOS、Linux で実行します。ウィンドウを開き、GPU で描画し、オーディオを再生し、キーボード・マウス・ゲームパッドを読み取ります。ランタイムプロファイル `native_v0` を実装しています。`--headless` を付けると、ウィンドウなしでスクリプトに従って実行します。[ヘッドレスモード](./headless.md)を参照してください。

## ゲームの実行 {#running-a-game}

ゲームをビルドし、`kagari build` が書き出したディレクトリをランタイムに渡します。

```sh
kagari build --manifest-path example/test_game1/kagari.toml
kagari-desktop-runtime example/test_game1/target/v0
```

Kagari リポジトリ内でなら、何もインストールせずに次のように実行できます。

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

プレイにはランタイムのリリースビルドを使ってください。デバッグビルドはかなり遅くなります。

## コマンドライン {#command-line}

```text
kagari-desktop-runtime [OPTIONS] [GAME_DIR]
```

| 引数 / オプション | 意味 |
| ----------------- | ---- |
| `GAME_DIR` | `game.kagari` と `.kiarchive` ファイルのあるディレクトリ。既定はランタイム実行ファイルのディレクトリ。 |
| `--data-dir <DIR>` | 設定とセーブのディレクトリ。既定はプラットフォームのデータディレクトリ（[データディレクトリ](#data-directory)を参照）。 |
| `--backend <BACKEND>` | ウィンドウのバックエンド。`wayland` または `x11`（Linux のみ。他の OS では警告を出して無視）。既定は `WAYLAND_DISPLAY` が設定されていて空でなければ `wayland`、そうでなければ `x11`。 |
| `--headless` | ウィンドウなしで実行します。`--script` と `--out` が必要です。[ヘッドレスモード](./headless.md)を参照。 |
| `--script <FILE>` | ヘッドレスの入力スクリプト。`--headless` が必要です。 |
| `--out <DIR>` | ヘッドレスのキャプチャ（と、`--data-dir` がなければセーブ）の出力先。`--headless` が必要です。 |
| `--seed <N>` | ヘッドレスモードで新しい物語すべてに使う乱数シード。既定は `1`。`--headless` が必要です。 |
| `-h`、`--help` | ヘルプを表示します。 |
| `-V`、`--version` | バージョンを表示します。 |

`GAME_DIR` に `game.kagari` がない場合、ランタイムはステータス 2 で止まり、`target/v0` のような `kagari build` が書き出したディレクトリを渡すよう促します。

`GAME_DIR` の既定は実行ファイルのディレクトリなので、配布するゲームでは実行ファイルを `game.kagari` とアーカイブの隣に置きます。プレイヤーは引数なしで起動できます。[ゲームの配布](./projects.md#shipping-a-game)を参照してください。

## ウィンドウ {#the-window}

ウィンドウのタイトルとフルスクリーン状態は、ゲームのシステムが設定したものになり、実行中の変更にも追従します。ウィンドウはゲームのデザイン解像度で開き、必要ならプライマリモニタの 90% に収まるよう縮小されます。ウィンドウのサイズを変えると、画面は収まるように拡大縮小され、余白には黒帯が入ります。マウスの位置はデザインピクセルに変換し直されます。

ウィンドウを閉じるとゲームは正常に終了し（ゲームの終了処理が実行されます）、ステータス 0 で終わります。

## データディレクトリ {#data-directory}

設定とセーブは、データディレクトリ内の `<name>.kisave` という名前のファイルです。既定では、プラットフォームのデータディレクトリの `Kagari/<title>` です。`<title>` はゲームのウィンドウタイトルです。

| プラットフォーム | 既定のデータディレクトリ |
| ---------------- | ------------------------ |
| Linux | `$XDG_DATA_HOME/Kagari/<title>`（既定は `~/.local/share/Kagari/<title>`） |
| macOS | `~/Library/Application Support/Kagari/<title>` |
| Windows | ローミング AppData フォルダの `Kagari\<title>`（`%APPDATA%\Kagari\<title>`） |

`<title>` では、`/ \ : * ? " < > |` と制御文字が `_` に置き換えられ、前後の空白と末尾のドットが取り除かれ、空のタイトルは `Kagari Game` になります。したがってタイトルを変えると設定とセーブの保存先も変わります。配布後はタイトルを変えないでください。

`--data-dir <DIR>` はこのディレクトリを置き換え、タイトルに依存しないようにします。ファイルはアトミックに書き込まれる（一時ファイルに書いてから名前を変える）ため、セーブ中にクラッシュしても以前のファイルは壊れません。データディレクトリが見つからない場合、ランタイムは警告をログに出し、設定とセーブは保存されません。

## ランタイムプロファイル `native_v0` {#runtime-profile-native_v0}

デスクトップランタイムはランタイムプロファイル `native_v0` を実装しています。動画を除く std 全体です。対応するアセット形式は[ランタイムプロファイル](./projects.md#runtime-profiles)にあります。`kagari build` はデコードできないアセットと検証に通らないシェーダーを拒否するので、`runtime-compatibility = ["native_v0"]` でビルドできたゲームには実行時に非対応のアセットはありません。シェーダーにはストレージバッファを持つ GPU が必要で、モーションのシェーダーにはさらにコンピュートシェーダーが必要です。これらのない GPU では、ランタイムは警告をログに出し、その効果を設定されていないものとして描きます。STD リファレンスの[ランタイムプロファイル](../../../../std/ja/src/appendix/profiles.md)も参照してください。

デバイスがなくてもランタイムは動作を続けます。

- オーディオ出力がない: `audio output unavailable (…); running silent` をログに出し、音なしで動作します。
- ゲームパッドが使えない: 警告をログに出し、キーボードとマウスだけで動作します。
- アーカイブが見つからない、音声をデコードできない: 警告をログに出し、そのアセットは存在しないものとして扱われます。

## ログ {#logging}

ランタイムは stderr にログを出します。既定のレベルは `kagari=info,wgpu=warn` で、使用中の GPU アダプタ、ウィンドウのバックエンド、データディレクトリが出力されます。変更するには `RUST_LOG` を設定します（例: `RUST_LOG=debug`）。

## ゲームがエラーで停止したとき {#when-the-game-stops-with-an-error}

ゲームのコードが実行時に失敗すると、ゲームはエラーで停止します。算術オーバーフロー、ゼロ除算、範囲外のインデックス、[`recursion-depth`](./projects.md#compiler-options) より深い呼び出し、std の関数が発生させたエラーなどです。そのとき:

1. ゲームが停止します。保留中の遷移は破棄され、すべての音声が止まります。以降のエラーは無視されます。
2. ウィンドウは黒い画面になり、左上に白い文字でエラーメッセージを表示します。
3. ランタイムは `kagari-desktop-runtime: game stopped: <message>` を stderr に出力します。
4. ウィンドウはプレイヤーが閉じるまで開いたままで、閉じたときの終了ステータスは 3 です。

起動中（`main` の実行中、ウィンドウが開く前）にゲームのコードでエラーが起きると、同じメッセージを出力し、ただちにステータス 3 で終了します。それ以外の理由でゲームを開始できない場合は、`kagari-desktop-runtime: cannot start the game: <reason>` を出力してステータス 2 で終了します。

- `.kagari` ファイルが不正、または検証に失敗した。
- `.kagari` ファイルが、このランタイムにないビルトインをインポートしている（``the bytecode imports `<path>`, which this runtime does not provide``）。たとえば新しいランタイム向けにビルドされたファイルです。
- フォントを読み込めない。フォントアセットが見つからないかデコードできない、またはシステムの既定フォントが読み込まれていない（``the system font `<path>` is not loaded``）。
- システムのテキストスタイルが、読み込めないアセットを参照している。

## 終了ステータス {#exit-statuses}

| ステータス | 意味 |
| ---------: | ---- |
| 0 | ゲームが正常に終了した。 |
| 2 | 使い方の誤り、ゲームディレクトリに `game.kagari` がない、またはゲームを開始できない（不正な `.kagari` ファイル、このランタイムにないビルトインのインポート、フォント（またはその他のテキストスタイルのアセット）の読み込み失敗）。[ゲームがエラーで停止したとき](#when-the-game-stops-with-an-error)を参照してください。 |
| 3 | ゲームがエラーで停止した。 |
| 4 | ウィンドウまたは GPU の障害。ウィンドウを開けない、使える GPU アダプタがない、GPU のメモリ不足、デバイスの喪失。 |
| 5 | ヘッドレススクリプトのエラー（ヘッドレスモードのみ）。 |

ステータス 0 以外では、ランタイムは `kagari-desktop-runtime: <message>` を stderr に出力します。

## システム要件 {#system-requirements}

実行時には、Vulkan、Metal、DirectX 12、OpenGL のいずれかの GPU ドライバが必要です。使えるハードウェアアダプタがない場合はソフトウェアのフォールバックアダプタを試し、それもなければステータス 4 で終了します。Linux ではさらに Wayland または X11 と ALSA が必要です。

ソースからランタイムをビルドするには、次のシステムパッケージが必要です。

| システム | パッケージ |
| -------- | ---------- |
| Debian、Ubuntu | `libasound2-dev libudev-dev pkg-config` |
| Fedora | `alsa-lib-devel systemd-devel` |
| macOS、Windows | なし |
| NixOS | リポジトリで `direnv allow`（または `nix develop`）を実行すると、必要なものがそろった開発シェルに入ります |
