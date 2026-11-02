---
title: "ランタイム"
---

`kagari-desktop-runtime` は、`kagari build` でビルドしたゲームを Windows、macOS、Linux 上で動かすプログラムです。ウィンドウを開いて GPU で描画し、音声を再生し、キーボード・マウス・ゲームパッドの入力を受け取ります。対応するランタイムプロファイルは `native_v0` です。`--headless` を付けるとウィンドウを開かず、スクリプトの指示どおりに動きます（[ヘッドレスモード](./headless.md)を参照）。

## ゲームの実行 {#running-a-game}

ゲームをビルドしたら、`kagari build` の出力先ディレクトリをランタイムに渡します。

```sh
kagari build --manifest-path example/test_game1/kagari.toml
kagari-desktop-runtime example/test_game1/target/v0
```

Kagari のリポジトリ内なら、何もインストールせずに次のように実行できます。

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

遊ぶときはランタイムのリリースビルドを使ってください。デバッグビルドではかなり遅くなります。

## コマンドライン {#command-line}

```text
kagari-desktop-runtime [OPTIONS] [GAME_DIR]
```

| 引数 / オプション | 意味 |
| ----------------- | ---- |
| `GAME_DIR` | `game.kagari` と `.kiarchive` ファイルを置いたディレクトリ。既定はランタイムの実行ファイルがあるディレクトリ（macOS のアプリでは、そこに `game.kagari` がなければ `Contents/Resources`）。 |
| `--data-dir <DIR>` | 設定とセーブを保存するディレクトリ。既定は OS ごとのデータディレクトリ（[データディレクトリ](#data-directory)を参照）。 |
| `--backend <BACKEND>` | ウィンドウのバックエンド。`wayland` か `x11` を指定します（Linux 専用。ほかの OS では警告を出して無視）。既定では、`WAYLAND_DISPLAY` が空でない値に設定されていれば `wayland`、そうでなければ `x11`。 |
| `--headless` | ウィンドウを開かずに実行します。`--script` と `--out` も必要です。[ヘッドレスモード](./headless.md)を参照。 |
| `--script <FILE>` | ヘッドレス実行で使う入力スクリプト。`--headless` と併用します。 |
| `--out <DIR>` | ヘッドレス実行のキャプチャの出力先（`--data-dir` がなければセーブもここに保存）。`--headless` と併用します。 |
| `--seed <N>` | ヘッドレスモードで、新しく始まる物語すべてに使う乱数シード。既定は `1`。`--headless` と併用します。 |
| `--debug-port <PORT>` | `127.0.0.1:PORT` でデバッガーの接続を待ち受けます（`0` を指定すると空いているポートを選び、ログに出します）。`--headless` とは併用できません。[デバッガー](./debugger.md#window)を参照。デバッグビルドでは、このフラグか `--debug-stdio` があると、[ホットスワップ](./debugger.md#hot-swap)に必要な情報をゲームの開始時から記録します。 |
| `--debug-stdio` | 標準入出力を通じてデバッガーとやり取りします（`kagari debug --window` と `kagari dap` はこの方法でゲームを起動します）。`--headless`、`--debug-port` とは併用できません。 |
| `-h`、`--help` | ヘルプを表示します。 |
| `-V`、`--version` | バージョンを表示します。 |

`GAME_DIR` に `game.kagari` がないと、ランタイムはステータス 2 で終了し、`target/v0` など `kagari build` の出力先を渡すよう案内します。

`GAME_DIR` の既定は実行ファイルのあるディレクトリです。そのため配布するゲームでは、実行ファイルを `game.kagari` やアーカイブと同じ場所に置きます。こうしておけば、プレイヤーは引数なしで起動できます。macOS のアプリバンドルでは実行ファイルが `Contents/MacOS`、ゲーム本体が `Contents/Resources` に入ります。ランタイムは、自分のディレクトリに `game.kagari` がなければ `Contents/Resources` を探します。[ゲームの配布](./projects.md#shipping-a-game)を参照してください。

`--debug-port` も `--debug-stdio` も指定しなければ、デバッグ用の通信路は開きません。どちらかを指定すると、デバッガーから物語を止めたり、進めたり、中身を調べたりできます。デバッグビルドがゲームディレクトリに `game.kagari-cache` を書き出していれば、ランタイムはそれも読み込みます（[デバッガー](./debugger.md)を参照）。

## ウィンドウ {#the-window}

ウィンドウのタイトルとフルスクリーンかどうかは、ゲームのシステムが設定した値に従い、実行中に変わればそれに追従します。ウィンドウはゲームのデザイン解像度で開きますが、主モニターの 90% に収まらない場合は縮小します。ウィンドウの大きさを変えると、画面はウィンドウに収まるよう拡大縮小し、余った部分は黒帯になります。マウスの座標はデザイン解像度上のピクセルに換算し直します。

ウィンドウを閉じると、ゲームは通常どおり終了し（ゲーム側の終了処理も実行します）、ステータス 0 で終わります。

## データディレクトリ {#data-directory}

設定とセーブは、データディレクトリ内に `<name>.kisave` という名前のファイルとして保存します。データディレクトリの既定は、OS ごとのデータディレクトリの下の `Kagari/<title>` です。`<title>` はゲームのウィンドウタイトルです。

| OS | 既定のデータディレクトリ |
| ---------------- | ------------------------ |
| Linux | `$XDG_DATA_HOME/Kagari/<title>`（既定は `~/.local/share/Kagari/<title>`） |
| macOS | `~/Library/Application Support/Kagari/<title>` |
| Windows | ローミング用 AppData フォルダの `Kagari\<title>`（`%APPDATA%\Kagari\<title>`） |

`<title>` に含まれる `/ \ : * ? " < > |` と制御文字は `_` に置き換え、前後の空白と末尾のドットは取り除きます。タイトルが空なら `Kagari Game` になります。つまりタイトルを変えると、設定とセーブの保存先も変わります。配布を始めたらタイトルは変えないでください。

`--data-dir <DIR>` を指定すると、このディレクトリの代わりに指定先を使い、タイトルには左右されなくなります。ファイルは一時ファイルに書いてから名前を変える方式でアトミックに書き込むので、セーブ中にクラッシュしても以前のファイルは壊れません。データディレクトリが見つからない場合は警告をログに出し、設定とセーブは保存しません。

## ランタイムプロファイル `native_v0` {#runtime-profile-native_v0}

デスクトップ版ランタイムが実装するランタイムプロファイルは `native_v0` で、標準ライブラリ全体に対応しています。対応するアセット形式は[ランタイムプロファイル](./projects.md#runtime-profiles)に一覧があります。`kagari build` はデコードできないアセットや検証を通らないシェーダーを受け付けないので、`runtime-compatibility = ["native_v0"]` でビルドできたゲームなら、実行時に非対応のアセットが出てくることはありません。シェーダーを使うにはストレージバッファに対応した GPU が必要で、モーションのシェーダーにはさらにコンピュートシェーダーが必要です。これらに対応していない GPU では警告をログに出し、該当する効果は設定されていないものとして描画します。標準ライブラリのリファレンスにある[ランタイムプロファイル](../../std/appendix/profiles.md)も参照してください。

ムービーは、システムに GPU の動画デコーダー（Linux では VA-API、Windows では Direct3D 11、macOS では VideoToolbox）があればそれを使い、なければソフトウェアでデコードします。VideoToolbox が AV1 をデコードできるのは M3 以降のチップだけなので、M1・M2 の Mac では AV1 のムービーをソフトウェアでデコードします。H.264 と VP9 は、Apple シリコンの Mac ならどれでも VideoToolbox でデコードします。どちらの方式を使ったかは、ムービーごとにターゲット `kagari::video` でログに出ます（例：``movie `movie/intro.webm`: vaapi decoding``）。ソフトウェアに切り替えたときは `video: hardware decoding unavailable (…); using software` と出ます。デコードが間に合わなかったフレームでは前のフレームを表示し続け、ゲームがデコーダーを待つことはありません。

デバイスが欠けていても、ランタイムは動き続けます。

- 音声出力がない：`audio output unavailable (…); running silent` をログに出し、音なしで動作します。
- ゲームパッドに対応していない：警告をログに出し、キーボードとマウスだけで動作します。
- アーカイブが見つからない、または音声をデコードできない：警告をログに出し、そのアセットはないものとして扱います。

## ログ {#logging}

ランタイムのログは標準エラー出力に出ます。既定のレベルは `kagari=info,wgpu=warn` で、使用中の GPU アダプター、ウィンドウのバックエンド、データディレクトリが表示されます。レベルを変えるには `RUST_LOG` を設定します（例：`RUST_LOG=debug`）。物語の進み具合はターゲット `kagari::story` に記録します。物語がステップに入るたびに `step <step ID> say` か `step <step ID> choice` を 1 行出します（例：`step crate::Start#0 say`）。[ステップ ID](../concepts/steps.md#step-ids) を参照してください。

## ゲームがエラーで停止したとき {#when-the-game-stops-with-an-error}

ゲームのコードが実行時に失敗すると、ゲームはエラーで停止します。算術オーバーフロー、ゼロ除算、範囲外のインデックス、[`recursion-depth`](./projects.md#compiler-options) を超える深さの呼び出し、標準ライブラリの関数が出したエラーなどが該当します。停止すると次のようになります。

1. ゲームが止まります。保留中の遷移は破棄し、音声もすべて止めます。その後に起きたエラーは無視します。
2. ウィンドウは黒い画面になり、左上に白い文字でエラーメッセージが表示されます。
3. ランタイムは標準エラー出力に `kagari-desktop-runtime: game stopped: <message>` を出します。
4. ウィンドウはプレイヤーが閉じるまで開いたままで、閉じると終了ステータス 3 で終わります。

メッセージには、エラーの内容、関数名とソース上の位置（`file@byte offset`）が入ります。物語がすでにステップに到達していれば、最後に到達したステップの[ステップ ID](../concepts/steps.md#step-ids) を `after step <step ID>` の形で添えます。呼び出し元は、外側に向かって 1 行ずつ `called from <function> (<location>)` として続きます。例を示します。

```text
vm panic: index out of range at crate::story::FirstDate (story.kis@1234), after step crate::story::FirstDate#28
```

起動中（`main` の実行中で、ウィンドウを開く前）にゲームのコードでエラーが起きた場合も同じメッセージを出し、すぐにステータス 3 で終了します。ほかの理由でゲームを開始できない場合は、`kagari-desktop-runtime: cannot start the game: <reason>` を出してステータス 2 で終了します。理由には次のようなものがあります。

- `.kagari` ファイルが壊れている、または検証に通らない。
- `.kagari` ファイルが、このランタイムにない組み込みのアイテムをインポートしている（``the bytecode imports `<path>`, which this runtime does not provide``）。より新しいランタイム向けにビルドしたファイルなどで起こります。
- フォントを読み込めない。フォントのアセットが見つからないかデコードできない場合や、システムの既定フォントが読み込まれていない場合です（``the system font `<path>` is not loaded``）。
- システムのテキストスタイルが、読み込めないアセットを参照している。

## 終了ステータス {#exit-statuses}

| ステータス | 意味 |
| ---------: | ---- |
| 0 | ゲームが正常に終了した。 |
| 2 | 使い方の誤り、ゲームディレクトリに `game.kagari` がない、`--debug-port` のポートを開けない、またはゲームを開始できない（`.kagari` ファイルが不正、このランタイムにない組み込みのアイテムをインポートしている、フォントなどテキストスタイルのアセットを読み込めない）。[ゲームがエラーで停止したとき](#when-the-game-stops-with-an-error)を参照。 |
| 3 | ゲームがエラーで停止した。 |
| 4 | ウィンドウまたは GPU の障害。ウィンドウを開けない、使える GPU アダプターがない、GPU のメモリが足りない、デバイスが失われた。 |
| 5 | ヘッドレススクリプトのエラー（ヘッドレスモードのみ）。 |

ステータス 0 以外で終わるときは、標準エラー出力に `kagari-desktop-runtime: <message>` を出します。

## Windows のターミナルから実行する {#running-from-a-terminal-on-windows}

Windows ではランタイムは GUI プログラムなので、エクスプローラーから起動するとゲームのウィンドウだけが開き、コンソールウィンドウは開きません。ターミナルから起動した場合は、ヘルプ、バージョン、ログ、エラーなどの出力をそのターミナルに書き出します。ファイルやパイプへリダイレクトした出力もそのまま残ります。

ただし、`cmd` と PowerShell は GUI プログラムの終了を待ちません。プロンプトがすぐに戻ってきて、ランタイムの出力はその後ろに表示され、終了ステータスも取れません。ランタイムの終了を待って終了ステータスを受け取るには、次のようにします。

| シェル | コマンド |
| ------ | -------- |
| `cmd` | `start /wait kagari-desktop-runtime.exe target\v0`（その後 `%ERRORLEVEL%`） |
| PowerShell | `kagari-desktop-runtime.exe target\v0 \| Write-Output`（その後 `$LASTEXITCODE`） |

`kagari debug --window`、Git Bash 上で動かすスクリプト、CI など、ランタイムを自分で起動するプログラムは、ほかのプログラムと同じように終了を待ちます。

## システム要件 {#system-requirements}

実行には、Vulkan、Metal、DirectX 12、OpenGL のいずれかに対応した GPU ドライバーが必要です。使えるハードウェアのアダプターがなければソフトウェアの代替アダプターを試し、それも使えなければステータス 4 で終了します。Linux ではほかに Wayland または X11、ALSA、libudev、libva（Debian・Ubuntu では `libva2 libva-drm2`、Fedora では `libva`）が必要で、libva がないと起動しません。配布しているビルド済みの Linux 版は、glibc 2.35 以降の x86_64 環境で動きます（[Kagari のインストール](../../tutorial/install.md#linux)を参照）。ビルド済みの macOS 版は、macOS 11 以降の Apple シリコン（M1 以降）で動き、追加のライブラリは要りません。Intel Mac 向けの版はありません（[Kagari のインストール](../../tutorial/install.md#macos)を参照）。ムービーのデコードには、ランタイムの実行ファイルと同じ場所（macOS のアプリでは `Contents/Frameworks`）に同梱した FFmpeg のライブラリを使います（[ゲームの配布](./projects.md#shipping-a-game)を参照）。Windows では Microsoft Visual C++ のランタイム `vcruntime140.dll` が必要です。Visual C++ 再頒布可能パッケージでインストールする（Kagari のインストーラーは、未導入なら自動でインストールします）か、ランタイムと同じ場所に置いてください（[ゲームの配布](./projects.md#shipping-a-game)を参照）。見つからないと Windows が「VCRUNTIME140.dll が見つからない」と表示し、ランタイムは起動しません。

ソースからランタイムをビルドするには、次のシステムパッケージが必要です。

| システム | パッケージ |
| -------- | ---------- |
| Debian、Ubuntu | `libasound2-dev libudev-dev pkg-config libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev libclang-dev` |
| Fedora | `alsa-lib-devel systemd-devel`、FFmpeg の開発用ファイル、libclang |
| macOS | Apple シリコンが前提。Xcode コマンドラインツール（libclang を含む）と、リポジトリのスクリプトでビルドした FFmpeg（`KAGARI_FFMPEG_TARGET=macos packaging/ffmpeg/build.sh <prefix>`。meson、ninja、pkg-config、xz も必要）。FFmpeg の場所は `FFMPEG_DIR=<prefix>` で指定します |
| Windows | FFmpeg の開発用ファイル（`FFMPEG_DIR`、vcpkg、pkg-config のいずれかで見つけます）と libclang |
| NixOS | リポジトリで `direnv allow`（または `nix develop`）を実行すると、必要なものがすべてそろった開発シェルに入ります |
