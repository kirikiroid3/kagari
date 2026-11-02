---
title: "標準ライブラリの使い方"
---

`std` は、どの Kagari プログラムにも一緒にコンパイルされるライブラリです。`std` 自体も Kagari で書かれており、`#[builtin]` を付けたアイテムだけはコンパイラかエンジンが提供します。

設計の土台にあるのは、ビジュアルノベルの制作現場で実際に作られているものです。名前付きのスロットを持つキャラクター、ルール画像や単色を挟んだ背景のトランジション、定番のモーション、タイトルカード、イントロとループに分かれた BGM、台詞に結び付いたボイス、プレイヤーが見たものに応じて解放されるギャラリーなどです。ほかのエンジンでは制作者が自前のマクロで用意していた部分を、`std` は型があり検査される値として提供します。

## モジュール {#modules}

|モジュール|内容|種別|ページ|
|---|---|---|---|
|`std`|`String`、`Vec`、`Range`、`Rat`、`Decimal`、`Option`、`Result`、`Eq`、`Integer`、`FromInteger`、`RandChoose`、`Asset`、アセット型、`asset`、`system_font`|`const fn`|[std](./std.md)|
|`std::prelude`|どのモジュールにも自動でインポートされる名前|—|[std::prelude](./prelude.md)|
|`std::time`|`Duration`、`ZERO`、`ms`、`secs`|`const fn`|[std::time](./time.md)|
|`std::narrative`|`Step`、`push_step`、`rng_choose`、`Say`、`Choice`、`Meta`、`Narrative` とそのレコード|`pushdown fn`（`push_step`、`rng_choose`、`Meta::write`）、`fn`（`Narrative`、`Meta::get`/`set`）、`const fn`（`meta`）|[std::narrative](./narrative.md)|
|`std::narrative::text`|`Text`、`t![…]` 用の組み込み関数、`ToText`、テキスト効果、`TextStyle`|`const fn`|[std::narrative::text](./narrative-text.md)|
|`std::screen`|`Screen`、エンティティ、プロパティ、レイヤー、Hud、カメラ、選択肢メニュー、座標、色|`const fn`|[std::screen](./screen.md)|
|`std::screen::anim`|`Easing`、`Tween`、`Transition`、`Motion` とプリセット|`const fn`|[std::screen::anim](./screen-anim.md)|
|`std::screen::audio`|チャンネル、ループ、サウンド、フェード|`const fn`|[std::screen::audio](./screen-audio.md)|
|`std::stage`|`Character`、`hide_all`、`side_face`|`const fn`|[std::stage](./stage.md)|
|`std::system`|`System`、`Settings`、`Setting`、モード、既読・閲覧済みの集合、バインディング|`fn`（ハンドル、閲覧済みの集合）、`const fn`（コンストラクター）|[std::system](./system.md)|
|`std::system::nav`|モード間の移動、物語の開始とロード|`fn`|[std::system::nav](./system-nav.md)|
|`std::system::saves`|セーブスロットとその情報|`fn`|[std::system::saves](./system-saves.md)|
|`std::ui`|`Element`、シグナル、コンポーネント、UI 構文の展開先|`fn`|[std::ui](./ui.md)|
|`std::ui::style`|`Paint`、`Look`、`ButtonStyle`、`SliderStyle`|`const fn`|[std::ui::style](./ui-style.md)|

## プレリュード {#the-prelude}

どのモジュールにもプレリュードが暗黙にインポートされるので、[`std::prelude`](./prelude.md) の名前は `use` なしで使えます。モジュール内で宣言した名前やインポートした名前は、プレリュードの名前より優先されます。

それ以外のアイテムは、パスを指定してインポートします。

```kis
use std::screen::anim::rule_with;
use std::narrative::text::speed;
```


## 設計規則 {#design-rules}

ライブラリのアイテムはすべて、次の規則に従っています。これを知っておけば、まだ調べていないアイテムがどう振る舞うかも見当がつきます。

1. **種別**
   - `Screen` を作ったり変えたりするもの、アニメーション、トランジション、テキスト効果の値、座標を扱う補助関数は、すべて `const fn` です。
   - 実行中のゲームをエンジン側から見る機能、設定とメタデータの読み取り、ナビゲーション、セーブ、UI は `fn` です。
   - `pushdown fn` は `push_step`、`rng_choose`、`Meta::write` の 3 つだけです。
   - エンジンが管理し、タイムラインに沿って変化する状態は、`#[builtin]` のメソッドを持つ `#[builtin]` 型として表します。単純なデータ（座標、色、補間、スタイル）は、`pub` フィールドと `const fn` のビルダーを持つ普通の構造体です。
2. **単位**
   - 位置と長さは、デザイン解像度 `System::resolution` を基準にした `i32`/`u32` のピクセル数です。
   - 時間は `time::Duration`（ミリ秒単位の整数）です。
   - 割合（アンカー、拡大率、不透明度、音量）は `Decimal` で、`0.5` のようなリテラルで書きます。不透明度と音量の範囲は `0.0..=1.0` です。
   - 設定の音量は `Int<0, 100>` です。
   - 単位付きの文字列はなく、同じ量を別の尺度で表すこともありません。
3. **隠れた既定値はない**　アニメーションのコンストラクターは、どれも長さを明示的に受け取ります。暗黙に決まる値は、イージング `Linear`、遅延 0、ルールのぼかし 0、繰り返し 1、恒等変換といった、効果のない値だけです。
4. **変更にはアニメーションが必要**　目に見えるプロパティは、`Prop<T>`（補間でき、`Tween` を受け取る）か、レイヤー（`ImageLayer`、`LabelLayer`、`MovieLayer`。`Transition` で差し替える）のどちらかです。直接値を設定するセッターがあるのは、補間できないレイアウト設定（`set_plane`、`set_z`、`set_anchor`、`set_flip`、`set_fill_size`、`gate`、`ImageLayer::set_resize`）だけです。`Change<T>`、`Swap`、`ChannelChange` には `#[must_use]` が付いており、`apply_animation` を呼ばない変更は反映されません（警告 `W3023`）。
5. **アニメーションは開始と終了の状態を変えない**　ステップの論理画面とは、レンダー後に残る状態のことです。コンポーネント付きのエンティティに加え、Hud、カメラ、画面フィルター、オーディオチャンネルが含まれます。スナップショット、等価性、一貫性の検査は、この論理画面を使います。
   - `Tween` と `Transition` は、必ず元の値から `change` で設定した値へ向かいます。
   - `Motion` は相対的な経路で、最後は恒等（オフセット 0、拡大率 1、回転 0）に戻らなければなりません。戻らない場合は定数評価エラーになります。
   - タイムライン、予約したアニメーション、単発のサウンド、選択肢メニューはそのステップだけのもので、論理画面には含まれません。
   - そのため、クリック（ステップのタイムラインを最後まで進める）やスキップをしても、`animations` 設定をオフにしても、論理画面は変わりません。
6. **コンポーネントの種類は固定**　制作者がコンポーネントを追加することはありません。ライブラリを拡張するときは、`const fn` の補助関数と定数データを使います。[`std::stage`](./stage.md) はそうした拡張の一例です。
7. **オーバーロードや暗黙の型変換なしで型検査できる**　アセット型を受け取る関数も、補間も、トレイト境界で表します。例外的な少数のシグネチャ（`nav::start`）は [標準ライブラリに関するコンパイラの規則](./appendix/rules.md) にまとめています。トレイトを宣言するのは `std` だけで、すべて `#[builtin]` です（`Eq`、`Integer`、`FromInteger`、`RandChoose`、`Asset`、`Step`、`ToText`、`ShaderArg`、`Shader`）。ゲーム側でこれらを実装することはありません。`Eq` は自動で実装され、`ShaderArg` は `#[shader_arg]` を付けると実装されます。
8. **参照**は共有ポインタです。`const fn` と `fn` のコードではローカル変数に束縛できるので、レンダーの中でエンティティを一度取り出せば、複数の文で続けて変更できます。`pushdown fn` では引数としてしか使えません。

例：

```kis
const fn hanako_appears(mut screen: Screen) -> Screen {
    let hanako = screen.entity(HANAKO);
    hanako.image().change(HANAKO::SMILE).apply_animation(cut());
    hanako.opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```
