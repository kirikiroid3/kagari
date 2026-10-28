# ランタイムプロファイル {#runtime-profiles}

**ランタイムプロファイル**は、ランタイムが提供するもの、つまり実装している std アイテムと、デコードできるアセットの種類・ファイル形式を表す名前です。プロジェクトは動作させたいプロファイルを `kagari.toml` に列挙し、`kagari build` はプログラムが使うすべてのアセットをそれぞれのプロファイルに照らして検査します。

```toml
[build]
runtime-compatibility = ["native_v0"]
```

`runtime-compatibility` の既定値は `["native_v0"]` です。リストは空であってはならず、未知の名前はマニフェストのエラーです。[プロジェクト](../../../../reference/ja/src/tools/projects.md) を参照してください。

## プロファイル {#profiles}

|プロファイル|ランタイム|
|---|---|
|`native_v0`|デスクトップランタイム（`kagari-desktop-runtime`）、バージョン 0|

現在のプロファイルは `native_v0` だけです。

## `native_v0` の対応表 {#native_v0-support-matrix}

### モジュール別 {#by-module}

|モジュール|`native_v0` での対応|
|---|---|
|`std`（型、トレイト、`asset`、`system_font`）|対応。ただし `VideoAsset` と `ShaderAsset` のファイルは使えません（後述）|
|`std::time`|対応|
|`std::narrative`|対応：`Say`、無効なアームを含む `Choice`、`rng_choose`、`Meta`、`Narrative` のすべてのメソッド。時間付き入力は予定で、どのプロファイルにもありません|
|`std::narrative::text`|対応：ルビとモノルビ、すべてのテキスト効果（スタイル、圏点、表示速度、ポーズ、マーク）、`hanging_indent` と `vertical` を含む `TextStyle` のすべてのフィールド|
|`std::screen`|対応。ただしムービーレイヤー（`Entity::movie`、`MovieLayer`）とシェーダー（`Screen::filter`、`Entity::filter`、`Entity::shader`）は、動画とシェーダーのアセットが必要なため使えません|
|`std::screen::anim`|対応|
|`std::screen::audio`|対応|
|`std::stage`|対応|
|`std::system`、`std::system::nav`、`std::system::saves`|対応|
|`std::ui`、`std::ui::style`|対応|

### アセットの種類と形式 {#asset-kinds-and-formats}

形式は拡張子ではなく、ファイルの内容から判定します。

|アセット型|対応する形式|対応しない形式|
|---|---|---|
|`ImageAsset`|PNG、ベースラインおよびプログレッシブ JPEG、非可逆・可逆・拡張（アルファまたはメタデータ付き）WebP|算術・可逆・階層・12 ビット符号化の JPEG、アニメーション WebP、GIF、BMP、AVIF、JPEG XL|
|`AudioAsset`、`VoiceAsset`|Ogg Vorbis、FLAC、WAV（PCM）、WAV（IEEE 浮動小数点）、MP3|Ogg Opus、Ogg FLAC、その他の Ogg コーデック、圧縮 WAV、AAC/M4A|
|`FontAsset`|TrueType、OpenType（CFF）、TrueType/OpenType コレクション（`.ttc`）|WOFF、WOFF2|
|`VideoAsset`|—|すべての形式：動画アセットには対応していません|
|`ShaderAsset`|—|すべての形式：シェーダーアセット（WGSL）には対応していません|

## 対応していないアイテムを使った場合 {#using-an-unsupported-item}

**対応していないアセットはビルドを失敗させます。** プログラムが使うアセットのうち、列挙されたプロファイルがデコードできないものごとに、`kagari build` はエラーを 1 行出力し、何も出力ファイルを書きません。

```text
error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`
error: asset `music/theme.ogg` (audio): Ogg Opus is not supported by runtime profile `native_v0`
error: asset `bgimage/bad.jxl` (image): JPEG XL is not supported by runtime profile `native_v0`
```

したがって、`native_v0` を対象とするゲームは `VideoAsset` や `ShaderAsset` の定数をどこにも使えません。その他のアセットは対応する形式に変換してください。これらのアセットを受け取るアイテムについては次のとおりです。

|アイテム|`kagari build` 時|`native_v0` の実行時|
|---|---|---|
|`MovieLayer::change(…)`（`Movie` は `VideoAsset` を保持します）|拒否（動画アセット）|—|
|`Screen::filter`、`Entity::filter`、`Entity::shader` に対する `Prop::change(Some(SHADER))`|拒否（シェーダーアセット）|—|
|`system::watched(VIDEO)`|拒否（動画アセット）|—|
|`Entity::movie()`、`MovieLayer::clear(…)`、フィルターやシェーダーのプロパティに対する `change(None)`|受理|論理画面（セーブ、等価性、一貫性）には保持されますが、描画はされません。描くものがないためです|

デスクトップランタイムのレンダラーは動画を再生せず、ゲームが用意したシェーダーも実行しません。そのため `native_v0` ではムービーレイヤーとフィルターがピクセルを生むことはなく、`watched` の集合が埋まることもありません。

`native_v0` は唯一のプロファイルで、`runtime-compatibility` は空にできないため、すべての `kagari build` は `native_v0` に照らして検査します。現在、プロジェクトがこの検査を外すことはできません。

**存在しない std アイテムは起動時に失敗します。** より新しい `std` でビルドした `.kagari` ファイルは、インストールされたランタイムが提供しないビルトインを呼ぶことがあります。その場合ランタイムは「the bytecode imports `…`, which this runtime does not provide」としてゲームの起動を拒否します。ランタイムが知らないエンジン機能を必要とする `.kagari` ファイルも同様に拒否されます。出荷するランタイムに合ったツールチェーンでゲームをビルドしてください。
