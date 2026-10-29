---
title: "標準ライブラリの使い方"
---

`std` は、すべての Kagari プログラムとともにコンパイルされるライブラリです。Kagari で書かれており、`#[builtin]` が付いたアイテムはコンパイラまたはエンジンが提供します。

設計は、ビジュアルノベルのスタジオが実際に作っているものに沿っています：名前付きスロットを持つキャラクター、ルール画像や単色を通した背景トランジション、プリセットのモーション、タイトルカード、イントロとループを持つ BGM、行に結び付いたボイス、プレイヤーが見たものから解放されるギャラリー。他のエンジンがスタジオのマクロに任せている部分を、`std` は型付きで検査される値として提供します。

## モジュール {#modules}

|モジュール|内容|種別|ページ|
|---|---|---|---|
|`std`|`String`、`Vec`、`Range`、`Rat`、`Decimal`、`Option`、`Result`、`Eq`、`Integer`、`RandChoose`、`Asset`、アセット型、`asset`、`system_font`|`const fn`|[std](./std.md)|
|`std::prelude`|すべてのモジュールにインポートされる名前|—|[std::prelude](./prelude.md)|
|`std::time`|`Duration`、`ZERO`、`ms`、`secs`|`const fn`|[std::time](./time.md)|
|`std::narrative`|`Step`、`push_step`、`rng_choose`、`Say`、`Choice`、`Meta`、`Narrative` とそのレコード|`pushdown fn`（`push_step`、`rng_choose`、`Meta::write`）、`fn`（`Narrative`、`Meta::get`/`set`）、`const fn`（`meta`）|[std::narrative](./narrative.md)|
|`std::narrative::text`|`Text`、`t![…]` のビルトイン、`ToText`、テキスト効果、`TextStyle`|`const fn`|[std::narrative::text](./narrative-text.md)|
|`std::screen`|`Screen`、エンティティ、プロパティ、レイヤー、Hud、カメラ、選択肢メニュー、座標、色|`const fn`|[std::screen](./screen.md)|
|`std::screen::anim`|`Easing`、`Tween`、`Transition`、`Motion` とプリセット|`const fn`|[std::screen::anim](./screen-anim.md)|
|`std::screen::audio`|チャンネル、ループ、サウンド、フェード|`const fn`|[std::screen::audio](./screen-audio.md)|
|`std::stage`|`Character`、`hide_all`、`side_face`|`const fn`|[std::stage](./stage.md)|
|`std::system`|`System`、`Settings`、`Setting`、モード、既読・既見の集合、バインディング|`fn`（ハンドル、既見の集合）、`const fn`（コンストラクタ）|[std::system](./system.md)|
|`std::system::nav`|モード間の移動、物語の開始とロード|`fn`|[std::system::nav](./system-nav.md)|
|`std::system::saves`|セーブスロットとその情報|`fn`|[std::system::saves](./system-saves.md)|
|`std::ui`|`Element`、シグナル、コンポーネント、UI フォームの展開先|`fn`|[std::ui](./ui.md)|
|`std::ui::style`|`Paint`、`Look`、`ButtonStyle`、`SliderStyle`|`const fn`|[std::ui::style](./ui-style.md)|

## プレリュード {#the-prelude}

すべてのモジュールはプレリュードを暗黙にインポートします。[`std::prelude`](./prelude.md) の名前は `use` なしで使えます。モジュール内で宣言した名前やインポートした名前は、プレリュードの名前より優先されます。

それ以外はパスを指定してインポートします。

```kis
use std::screen::anim::rule_with;
use std::narrative::text::speed;
```

## 設計規則 {#design-rules}

ライブラリのすべてのアイテムは次の規則に従います。まだ調べていないアイテムでも、何を期待できるかがわかります。

1. **種別。**
   - `Screen` を作る・変えるもの、すべてのアニメーション・トランジション・テキスト効果の値、すべての座標ヘルパーは `const fn` です。
   - 実行中のゲームに対するエンジンの視点、設定とメタの読み取り、ナビゲーション、セーブ、UI は `fn` です。
   - `pushdown fn` は `push_step`、`rng_choose`、`Meta::write` だけです。
   - タイムラインの意味を持つエンジン所有の状態は、`#[builtin]` のメソッドを持つ `#[builtin]` 型です。単純なデータ（座標、色、トゥイーン、スタイル）は、`pub` フィールドと `const fn` のビルダーを持つ通常の構造体です。
2. **単位。**
   - 位置と長さは、デザイン解像度 `System::resolution` における `i32`/`u32` のピクセルです。
   - 時間は `time::Duration`（ミリ秒単位の整数）です。
   - 割合（アンカー、拡大率、不透明度、音量）は `Decimal` で、`0.5` のようなリテラルで書きます。不透明度と音量の範囲は `0.0..=1.0` です。
   - 設定の音量は `Int<0, 100>` です。
   - 単位付き文字列はなく、どの量にも第二の尺度はありません。
3. **隠れたデフォルトはありません。** アニメーションのコンストラクタはすべて長さを明示的に受け取ります。暗黙の値は中立的なものだけです：イージング `Linear`、遅延 0、ルールのぼかし 0、繰り返し 1、恒等変換。
4. **変更にはアニメーションが必要です。** 目に見えるプロパティはすべて、`Prop<T>`（補間され、`Tween` を受け取る）かレイヤー（`ImageLayer`、`LabelLayer`、`MovieLayer`。`Transition` で差し替える）です。補間できないレイアウト設定（`set_plane`、`set_z`、`set_anchor`、`set_flip`、`set_fill_size`、`gate`）だけが直接のセッターです。`Change<T>`、`Swap`、`ChannelChange` は `#[must_use]` で、`apply_animation` のない変更は起こりません（警告 `W3023`）。
5. **アニメーションは開始状態も終了状態も変えません。** ステップの論理画面とは、レンダー後の永続的な状態です：コンポーネントを持つエンティティに加え、Hud、カメラ、画面フィルター、オーディオチャンネル。スナップショット、等価性、一貫性検査が使うのはこれです。
   - `Tween` と `Transition` は、常に古い値から `change` で設定した値へ進みます。
   - `Motion` は相対的な経路で、恒等（オフセット 0、拡大率 1、回転 0）で終わらなければなりません。そうでなければ定数評価エラーです。
   - タイムライン、予約されたアニメーション、ワンショットのサウンド、選択肢メニューはそのステップだけのもので、論理画面には含まれません。
   - したがって、クリック（ステップのタイムラインを完了させる）、スキップ、`animations` 設定のオフが論理画面を変えることはありません。
6. **コンポーネントの集合は固定です。** スタジオはコンポーネントを追加しません。`const fn` のヘルパーと定数データでライブラリを拡張します。[`std::stage`](./stage.md) はそのような層の例です。
7. **オーバーロードや型変換なしで型検査できます。** アセット型を受け取る関数はトレイト境界を使い、補間もトレイト境界を使います。少数の特別なシグネチャ（`nav::start`）は [std アイテムのコンパイラ規則](./appendix/rules.md) に記載しています。トレイトを宣言するのは `std` だけで、すべて `#[builtin]` です：`Eq`、`Integer`、`RandChoose`、`Asset`、`Step`、`ToText`、`ShaderArg`、`Shader`。ゲームがこれらを実装することはありません。`Eq` は自動で、`ShaderArg` は `#[shader_arg]` で得られます。
8. **参照**は共有ポインタです。`const fn` と `fn` のコードではローカル変数に束縛できるので、レンダーはエンティティを一度名指しして、複数の文で変更できます。`pushdown fn` では引数としてのみ使えます。

例：

```kis
const fn hanako_appears(mut screen: Screen) -> Screen {
    let hanako = screen.entity(HANAKO);
    hanako.image().change(HANAKO::SMILE).apply_animation(cut());
    hanako.opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```
