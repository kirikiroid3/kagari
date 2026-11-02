---
title: "ランタイムプロファイル"
---

**ランタイムプロファイル**とは、ランタイムが何を提供するかを表す名前です。どの標準ライブラリのアイテムを実装しているか、どの種類・形式のアセットをデコードできるかが、プロファイルによって決まります。プロジェクトは動作対象のプロファイルを `kagari.toml` に並べて書き、`kagari build` はプログラムが使うすべてのアセットを、それぞれのプロファイルに照らして検査します。

```toml
[build]
runtime-compatibility = ["native_v0"]
```

`runtime-compatibility` の既定値は `["native_v0"]` です。空のリストは指定できず、知らない名前を書くとマニフェストのエラーになります。[プロジェクト](../../reference/tools/projects.md)を参照してください。

## プロファイル {#profiles}

|プロファイル|ランタイム|
|---|---|
|`native_v0`|デスクトップ版ランタイム（`kagari-desktop-runtime`）のバージョン 0|

現時点のプロファイルは `native_v0` だけです。

## `native_v0` の対応状況 {#native_v0-support-matrix}

### モジュールごとの対応 {#by-module}

|モジュール|`native_v0` での対応|
|---|---|
|`std`（型、トレイト、`asset`、`system_font`）|対応|
|`std::time`|対応|
|`std::narrative`|対応。`Say`、無効なアームを含む `Choice`、`rng_choose`、`Meta`、`Narrative` の全メソッド。時間制限付きの入力は予定の段階で、どのプロファイルにもありません|
|`std::narrative::text`|対応。ルビとモノルビ、全テキスト効果（スタイル、圏点、表示速度、一時停止、マーク）、`hanging_indent` と `vertical` を含む `TextStyle` の全フィールド|
|`std::screen`|対応。ムービーのレイヤー（`Entity::movie`、`MovieLayer`）も含む|
|`std::screen::shader`、`std::shaders`|対応。シェーダーはすべて `kagari build` の時点で検証します。ストレージバッファに対応していない GPU（モーションの役割では、コンピュートシェーダーに対応していない GPU）では、効果をかけずに内容だけを描画します|
|`std::screen::anim`|対応|
|`std::screen::audio`|対応|
|`std::stage`|対応|
|`std::system`、`std::system::nav`、`std::system::saves`|対応|
|`std::ui`、`std::ui::style`|対応|

### アセットの種類と形式 {#asset-kinds-and-formats}

形式は拡張子ではなく、ファイルの中身で判定します。

|アセット型|対応する形式|対応しない形式|
|---|---|---|
|`ImageAsset`|PNG、ベースラインおよびプログレッシブ JPEG、非可逆・可逆・拡張（アルファやメタデータ付き）WebP、BMP（非圧縮、RLE、ビットフィールド）、静止画の JPEG XL（RGB またはグレースケール）|算術符号化・可逆・階層型・12 ビットの JPEG、アニメーション WebP、GIF、JPEG や PNG を埋め込んだ BMP、CMYK の BMP、AVIF、アニメーションや CMYK の JPEG XL|
|`AudioAsset`、`VoiceAsset`|Ogg Vorbis、Ogg FLAC、FLAC、WAV（PCM）、WAV（IEEE 浮動小数点）、MP3、モノラルまたはステレオの AAC-LC（ADTS または MP4/M4A）、モノラルまたはステレオの ALAC（MP4/M4A）|Ogg Opus、その他の Ogg コーデック、圧縮 WAV、HE-AAC などほかの AAC プロファイル、3 チャンネル以上の AAC や ALAC、ほかのコーデックの MP4/M4A|
|`FontAsset`|TrueType、OpenType（CFF）、TrueType/OpenType コレクション（`.ttc`）|WOFF、WOFF2|
|`VideoAsset`|映像が VP9 か AV1、音声が Opus か Vorbis（音声なしも可）の WebM。映像が H.264、音声が AAC-LC（音声なしも可）の MP4|VP8 などほかの映像コーデック、複数の映像トラック、ほかの音声コーデック、ほかのトラックを含む WebM、ほかのコンテナ|
|`ColorShader`、`MixShader`、`TextureShader`、`MaskShader`、`LocalMotionShader`、`TransitionalMotionShader`|型が示す役割として検証を通る WGSL ソース（UTF-8 テキスト）|テキストでないデータ、誤りのある WGSL|

## 対応していないものを使った場合 {#using-an-unsupported-item}

**対応していないアセットがあるとビルドは失敗します。** プログラムが使うアセットのうち、指定したプロファイルでデコードできないものがあれば、`kagari build` はアセットごとにエラーを 1 行ずつ出し、出力ファイルは何も書きません。

```text
error: asset `movie/intro.webm` (video): WebM/Matroska with another codec, several video tracks or an unreadable header is not supported by runtime profile `native_v0`
error: asset `music/theme.ogg` (audio): Ogg Opus is not supported by runtime profile `native_v0`
error: asset `bgimage/title.gif` (image): GIF is not supported by runtime profile `native_v0`
```

このようなアセットは、対応している形式に変換してください。ムービーなら、たとえば次のようにします。

```sh
ffmpeg -i in.mov -c:v libvpx-vp9 -c:a libopus out.webm
```

AV1 にするなら `-c:v libsvtav1` を、H.264 にするなら `-c:v libx264 -c:a aac` を指定し、出力を `.mp4` にします。

**不正なシェーダーがあるとビルドは失敗します。** `kagari build` は、`std::shaders` のものも含め、プログラムが使うすべてのシェーダーを、型が示す役割としてコンパイルします。不正なシェーダーがあれば、シェーダーと役割の組ごとにエラーを 1 行出します。誤りが `.wgsl` ファイルの中にあれば、その行番号も示します。

```text
error: shader `shader/broken.wgsl` (color): line 4: no definition in scope for identifier: `undefined_name`
```

プロファイルは `native_v0` しかなく、`runtime-compatibility` は空にできないので、`kagari build` は必ず `native_v0` に照らして検査します。現時点では、この検査をプロジェクト側で外すことはできません。

**標準ライブラリのアイテムが足りないと起動時に失敗します。** 新しい `std` でビルドした `.kagari` ファイルは、インストール済みのランタイムが提供していない組み込みのアイテムを呼び出すことがあります。その場合、ランタイムは「the bytecode imports `…`, which this runtime does not provide」と出してゲームの起動を拒否します。ランタイムが知らないエンジン機能を必要とする `.kagari` ファイルも、同じように拒否します。ゲームは、配布するランタイムと対応するツールチェーンでビルドしてください。
