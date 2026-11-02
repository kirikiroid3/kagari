---
title: "演出"
---

画面を *いつ* 変えるかは物語が決め、*どう* 変えるかはレンダー関数が決めます。この章では、その
仕組みを説明します。画面 API の全体（座標、描画面、エンティティ、プロパティ、レイヤー、
アニメーション、音声）は、標準ライブラリのリファレンスの
[`std::screen`](../../std/screen.md) にまとめてあります。

## レンダー関数 {#render-functions}

```kis
pub type Render = const fn(Screen) -> Screen;   // std::screen, in the prelude
```

**レンダー** とは、ステップ前の画面を受け取ってステップ後の画面を返す `const fn` です。前後の
画面をつなぐアニメーションもここに含みます。ステップには `render` 属性として渡します。

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);

const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
    screen
}
```

物語がステップに着くと、エンジンはまず、そのステップ限りの画面の状態（タイムライン、予約された
アニメーション、再生待ちの単発の音、選択肢メニュー）をリセットします。次に現在の画面に対して
レンダーを実行し、結果を表示して入力を待ちます。`render` のないステップでも同じリセットは行われ、
論理画面はそのまま残ります。

### インラインレンダー {#inline-renders}

何もキャプチャしないクロージャは `const fn` ポインタに型強制できるので、短いレンダーはその場に
書けます。

```kix
s!(
    render = |mut screen| { screen.background().change(BG::ROOF).apply_animation(fade(ms(500))); screen },
    t!["The wind on the roof is cold."]
);
```

クロージャの中では定数を使ったり `const fn` を呼んだりできますが、キャプチャはできません。
ローカル変数や物語変数を使うクロージャは `Render` になりません。

### レンダーの合成 {#composing-renders}

ステップに指定できる `render` は一つだけです。複数のレンダーを組み合わせるときは、普通の
`const fn` を書きます。

```kis
const fn roof_scene(screen: Screen) -> Screen {
    azusa_smiles(roof_background(screen))
}
```

## 一行ずつ読む {#walkthrough}

```kis
screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
```

1. `screen.entity(AZUSA)` は、id が `AZUSA`（たとえば `EntityId(217)`）のエンティティを取り出し
   ます。画面になければ生成します。戻り値は画面への [参照](../syntax/values.md) なので、これを
   変更すると画面が変わります。
2. `.image()` はエンティティのメインの `ImageLayer` です。`.change(AZUSA::CLOSE_EYE_SMILE)` で、
   そのレイヤーを画像 `AZUSA::CLOSE_EYE_SMILE` に差し替えるよう指示します。結果は保留中の変更を
   表す `&mut Swap` で、これもレイヤーへの参照です。
3. レイヤーの差し替えには `Transition` が必要です。指定しないと変更は反映されません。
4. `.apply_animation(cut())` で画像が即座に切り替わり、変更が反映されます。

参照はローカル変数に束縛できます。同じエンティティを何度も変更するレンダーでは、一度名前を
付けておけば済みます。

```kis
let azusa = screen.entity(AZUSA);
azusa.image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
azusa.opacity().change(1.0).apply_animation(tween(ms(300)));
```

## 画面は ECS ワールド {#the-screen-is-an-ecs-world}

`Screen` はエンティティ・コンポーネント・システム（ECS）の考え方に沿っています。

| 部分 | 定義する場所 | 役割 |
| ---- | ------------ | ---- |
| エンティティ | レンダー関数 | レンダーが生成・変更・消去する。`EntityId` 定数で識別 |
| コンポーネント | `std::screen`（固定） | エンティティに付くデータ。描画面、z、位置、アンカー、拡大率、回転、不透明度、色フィルター、合成、マスク、ローカルモーション、塗り、画像パーツ、ラベル、動画、反転、ゲート |
| リソース | `std::screen`（固定） | 画面そのもののデータ。Hud、選択肢メニュー、カメラ、画面フィルター、音声チャンネル |
| システム | エンジン | 実行時にエンティティを処理する。たとえばアニメーションを 1 フレームずつ進める |

`EntityId(0)` は背景エンティティ `BACKGROUND` 用に予約されています。制作者が使う id は 1 から
始めます。

コンポーネントの型はすべて等価性を持つので、二つの画面はいつでも比較できます。システムは Kagari で
書くものではなく、論理画面にも含まれません。ステップが見せる画面とは、レンダー後のエンティティと
コンポーネントの集合のことで、アニメーション中にエンジンが描く各フレームのことではありません。

コンポーネントの種類は固定で、制作者が追加することはできません。その代わり、自分のモジュールに
`const fn` の補助関数や定数データ（キャラクターの立ち位置、表情画像、暗転などの演出の実例）を
用意して、ライブラリを拡張します。`std::stage` はこうした層の標準的な例で、キャラクター関連の
機能は `std::screen` の上に書いた普通の `const fn` です。制作者の補助関数も、ほかのコードと
同じように型検査を受けます。

## 変更にはアニメーションが必要 {#changes-need-animations}

見た目に関わる値を変更すると保留中の変更ができ、`apply_animation` を呼んで初めて画面に反映
されます。そのため、見た目の変更には必ずアニメーションの方法を書きます。即座に切り替える場合も
同じです。

対象によって受け付けるアニメーションの型が違うので、位置の変更に `Transition` を渡すといった
取り違えは型検査で見つかります。

| 対象 | 変更方法 | アニメーションの型 |
| ---- | -------- | ------------------ |
| プロパティ `Prop<T>`：位置、拡大率、回転、不透明度、フィルター・合成・マスク・ローカルモーション（シェーダー効果）、塗り。Hud の表示状態。カメラのオフセット・ズーム・回転。チャンネルの音量 | `.change(v)` で `Change<T>` を得る | `Tween`：長さ、イージング、遅延 |
| レイヤー：`ImageLayer`（背景とサイド画像を含む）、`LabelLayer`、`MovieLayer` | `.change(x)`、`.paint(p)`、`.clear()` で `Swap` を得る | `Transition`：カット、クロスフェード、ルール画像、マスク、ワイプ、ブラインド、単色を挟む切り替え |
| エンティティ全体、またはカメラ全体 | `.apply_animation(m)` を直接呼ぶ | `Motion`：出発点に戻る相対的な経路 |
| シェーダーの経路に沿ったエンティティの姿勢 | `.travel(to, motion)` を直接呼ぶ | `TransitionalMotion`：長さ、イージング、遅延 |
| 音楽・環境音のチャンネル | `.change(track)` または `.stop()` で `ChannelChange` を得る | `AudioFade` |

フィルター、ブレンドモード、マスク、グラデーションや天候などの生成画像、シェーダーで動かす
モーションは、どれも **シェーダー** です。シェーダーは GPU で動く WGSL ファイルで、画面には
シェーダーと引数を組にした効果値として保持します。画面と各エンティティには `filter()` があり、
エンティティにはさらに `mix()`（下にあるものとの合成方法）、`mask()`、`local_motion()` が
あります。画像レイヤーには、生成した `paint` を表示することもできます。効果を同じシェーダーの
別の引数へ補間すると、引数の値が補間されます。標準ライブラリには既製のシェーダーが `std::shaders`
に入っています。詳しくは [`std::screen::shader`](../../std/screen-shader.md) を参照してください。

音楽はステップをまたいで流れ続けます。レンダーが音楽チャンネルを再生中の曲のままにしておけば、
曲が頭から始まり直すことはありません。曲を頭から再生し直したいときは、レンダーで
`screen.replay_bgm()` を呼びます。

```kis
const fn mio_enters(mut screen: Screen) -> Screen {
    screen.entity(MIO).position().change(point(640, 80)).apply_animation(instant());
    screen.entity(MIO).image().change(MIO::NEUTRAL).apply_animation(cut());
    screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)).ease(Easing::OutQuad));
    screen
}
```

生成したばかりのエンティティは不透明度 0.0 で見えない状態から始まります。そのため、登場の
しかたは必ずアニメーションとして書くことになります。

`Change<T>`、`Swap`、`ChannelChange` には `#[must_use]` が付いています。
`screen.background().change(BG::ROOF);` のように変更が保留のまま文が終わると警告になります。
直接設定するのは、補間できない配置の設定だけです。エンティティの `set_plane`、`set_z`、
`set_anchor`、`set_flip`、`set_fill_size`、`gate` と、画像レイヤーの `set_resize`（内容を
どの大きさで表示するか。[`Resize`](../../std/screen.md#resize) を参照）がこれにあたります。

動画レイヤー（`entity.movie()`）は、差し替えた時点から動画を再生します。動画はエンティティの矩形に
収まる大きさで中央に置かれ、画像パーツより上、ラベルより下に描かれます。ステップは単発の効果音と
同じく、ループしない動画が終わるまで待ちます。ループする動画ではステップは待ちません。
[`MovieLayer`](../../std/screen.md#movielayer) を参照してください。

### アニメーションは論理画面を変えない {#animations-never-change-the-logical-screen}

ステップの **論理画面** とは、レンダー後に残る永続的な状態のことです。

- エンティティとそのコンポーネント
- Hud、カメラ、画面フィルター
- 音声チャンネル

セーブ、画面の等価比較、[一貫性検査](#consistency) は論理画面だけを見ます。そしてアニメーションが
論理画面を変えることはありません。

- `Tween` と `Transition` は、常に古い値から `change` で設定した値へと進みます。論理上の値は、
  `change` を実行した時点で新しい値になっています。
- `Motion` は出発点に戻って終わる相対的な経路でなければなりません。戻らないモーションはコンパイル
  エラーになります。
- ステップのタイムライン、予約されたアニメーション、単発の音、選択肢メニューはそのステップだけの
  もので、論理画面には含まれません。次のステップに入るときにエンジンがリセットします。

そのため、クリックでアニメーションを終わらせても、スキップモードにしても、プレイヤーが
アニメーションをオフにしても、論理画面は変わりません。

## レンダーは物語を見ない {#renders-are-blind-to-the-story}

レンダーは `const fn` なので、次の性質があります。

- **決定的。** 入力の画面が同じなら、出力の画面も必ず同じです。セーブをロードすると物語を
  再生しますが、再生では画面を正確に再現できなければなりません（[セーブ、再生、決定性](./saves.md)
  を参照）。
- **実行される範囲では必ず停止する。** コンパイラは、各レンダーを、そのステップに届きうるすべての
  画面に対して、評価予算の範囲内で評価します（[一貫性](#consistency) を参照）。終わらないレンダーは
  コンパイルエラーになるため、エンジンは必ずステップにたどり着きます。
- **物語を見ない。** 入力は画面だけです。`const fn` は物語変数を読めません。また `render` 属性は
  関数ポインタなので、物語変数をキャプチャすることもできません。

逆に、物語が画面を読むこともありません。`Screen` は [有限型](./kinds.md#finite-types) ではない
ので、`pushdown fn` が画面を保持することはできません。物語の状態が演出を決めるのであって、
その逆はありません。

物語によって演出を変えたいときは、物語の側で選びます。

```kix
match AFFECTION_MIO {
    3..=5 => s!(who = "Mio", render = mio_smiles, t!["You came."]),
    _ => s!(who = "Mio", render = mio_neutral, t!["Oh. It's you."]),
}
```

レンダーのポインタは有限な値です。`render = if MET_MIO { mio_smiles } else { mio_neutral }`
のように式で選ぶことも、物語変数に保存することもできます。どちらの場合も、コンパイラはその選択を
物語の状態の一部として扱います。

## キーフレーム {#keyframes}

出力が入力の画面に左右されないレンダーを **キーフレーム** と呼びます。キーフレームは画面全体を
宣言します。キーフレームかどうかはコンパイラが自動で判定するので、注釈は要りません。それ以外の
レンダーは *差分* です。

`Screen::empty()` は、背景エンティティ `BACKGROUND` だけがある画面を返します。背景は不透明で
画像はなく、メインの画像レイヤーは表示する画像の解像度に関係なく画面全体を覆います（デザイン
解像度に対する `Resize::Cover`）。Hud はテキストウィンドウとクイックメニューを表示し、カメラは
初期状態、画面フィルターは `None`、音声チャンネルはすべて無音です。これを使えば、キーフレームを
明示的に書くこともできます。

```kis
const fn roof_evening(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(BG::ROOF_EVENING).apply_animation(fade(ms(500)));
    screen
}
```

あるステップの画面を作り直すとき、物語の先頭までさかのぼってレンダーを実行し直す必要はありません。
経路上の直近のキーフレームから実行すれば十分です。キーフレームは、さかのぼる範囲をこうして
限定します。

<!-- diagram: presentation-keyframes (mermaid-ish)
flowchart LR
  subgraph with["経路上にキーフレームがある場合"]
    K1["K キーフレーム"] ==> d1((差分)) ==> d2((差分)) ==> d3((差分)) ==> K2["K キーフレーム"] ==> d4((差分)) ==> d5((差分)) ==> T1(("目標ステップ"))
    K2 -.->|"直近のキーフレームから評価"| T1
  end
  subgraph without["経路上にキーフレームがない場合"]
    r1((レンダー)) ==> r2((レンダー)) ==> r3((...)) ==> T2(("目標ステップ"))
    r1 -.->|"先頭から全レンダーを実行し直す"| T2
  end
-->
{{#include ../diagrams/presentation-keyframes.svg}}

キーフレームは、選択肢の箇所と章の始まりに置くことをおすすめします。キーフレームの配置の検査は
任意で、[`kagari.toml`](../tools/projects.md) の `[build]` テーブルに `keyframe-check = true` と
書くと有効になります。有効にすると、すべての `choice!` と各開始シーンの最初のステップに
キーフレームのレンダーが必要になり、足りないステップには警告 `W5011` が出ます。レンダーのない
ステップはキーフレームではありません。電話のように、どこで起きてもおかしくない割り込みシーンは、
キーフレームで *始めないでください*。割り込んだ先の画面に重ねて見せるためのシーンだからです。

## 一貫性 {#consistency}

レンダーは *直前の* 画面を変換するので、あるステップの画面は、そこまでどの経路をたどったかで
変わります。

```kix
match choice!{
    id = "where",
    t!["Go to the roof"] => Where::Roof,
    t!["Stay in the classroom"] => Where::Classroom,
} {
    Where::Roof => s!(render = roof, t!["The wind is cold."]),
    Where::Classroom => s!(render = classroom, t!["It's quiet."]),
}
s!(who = "Azusa", t!["So, about tomorrow…"]);
```

<!-- diagram: presentation-consistency (mermaid-ish)
flowchart TD
  C["choice!{ id = \"where\", … }"] ==>|Where::Roof| A["s!(t![\"The wind is cold.\"]) background = BG::ROOF"]
  C ==>|Where::Classroom| B["s!(t![\"It's quiet.\"]) background = BG::CLASSROOM"]
  A ==> M["s!(who = \"Azusa\", …) background in { ROOF, CLASSROOM }"]
  B ==> M
  M -.- W["コンパイラはこのステップを警告（W5004）、両方の経路を添えて報告"]
-->
{{#include ../diagrams/presentation-consistency.svg}}

最後のステップは、一方の経路では屋上を、もう一方の経路では教室を表示します。意図したものなら
問題ありません。分岐の後始末を忘れただけなら、作者以外の誰も気づかない典型的なバグです。

コンパイラは、ステップごとにそこへ届きうる画面を求め、表示される画面（レンダー後の論理画面）が
物語の状態だけでは決まらないステップを報告します。報告には、食い違う経路それぞれで最後に画面を
変えたレンダーも示します。これが警告 `W5004` です。

あるステップでの *物語の状態* とは、物語自身が区別できる情報すべてです。具体的には、ステップ、
物語変数の値、コールスタック上の各フレームのうち後で使うローカル変数、開始引数、スタック上の
呼び出し位置を指します。これらがすべて一致する到達どうしは、同じ画面を表示しなければなりません。
したがって、電話のようにどこで起きてもおかしくない割り込みシーンは、呼び出し位置ごとに違う画面を
表示してかまいませんが、一つの呼び出し位置で二通りの画面を表示してはいけません。合流による
食い違いは、合流直後の最初のステップで一度だけ報告します。それより後のステップの違いは、同じ原因に
よるものだからです。

報告を受けたら、次のどちらかの方法で直します。

- 後始末の忘れであれば、報告された最初の不整合なステップに **キーフレームを入れます**。警告が
  提案するのもこの方法です。
- 意図した違いであれば、**依存を物語変数に移します**。各分岐で変数を設定し、合流後のステップの
  レンダーをその変数で選びます。こうすると、コンパイラはその依存を物語の状態として扱います。

## 非有界な画面 {#unbounded-screens}

レンダーは入力の画面からコンポーネントを読み、それをもとに新しい値を計算できます。たとえば、
エンティティを今の位置から 10 単位右へ動かす場合です。プレイヤーが何度も戻ってくる拠点のような
ループの中では、これで周回のたびに違う画面ができます。するとステップでの画面の集合に上限がなくなり、
一貫性検査が終わらなくなります。

レンダーはチューリング完全な `const fn` なので、この集合が有限かどうかは一般には判定できません。
*暫定:* コンパイラは、各レンダーをそのステップに届く画面に対して評価し、変化がなくなるまで
集合を求めます。あるステップの集合が固定の上限（現在は 64 画面）を超えたら計算を打ち切り、
新しい画面を生んだレンダーをエラー `E5005` として報告します。その後、そのレンダーは画面を
変えなかったものとみなして集合を求め直すので、プログラムの残りの部分も検査できます。

この規則は健全です。プログラムが受理されれば、求めた集合は正確です。ただし、上限を超えたところで
初めて有界になるレンダーは拒否されることがあります。ループの中では、絶対的な値を設定する
レンダー（キーフレームや、「さらに 10 単位」ではなく `change(point(640, 80))`）を書いて
ください。

## レンダーの中の設定 {#settings-in-renders}

レンダーで設定を使いたいことがあります。たとえば、プレイヤーが有効にしたときだけ装飾の効果を
描く場合です。しかし、レンダーは設定の値を読めません。値は実行時にしか分からないうえ、読めて
しまうとステップの画面が設定に左右され、ここまでの解析が設定のあらゆる値について成り立つとは
言えなくなるからです。

代わりに、レンダーでは **遅延参照** を使います。`static PETALS_ON: Setting<bool>` の
`PETALS_ON` は、設定の値ではなく設定を指すハンドルです（[設定とメタデータ](./system.md#settings-and-meta)
を参照）。`const fn` の中でハンドルから値を読み出す方法はありません。`Setting::get` は `fn` で
あり、ハンドルに対する `match` や算術もありません。レンダーにできるのは、ハンドルを
`Entity::gate` に渡すことだけです。値はエンジンが描画時に読み、設定が真のあいだだけ
エンティティを描きます。

```kis
static PETALS_ON: Setting<bool> = setting(true);
const PETALS: EntityId = EntityId(40);
const PETALS_IMAGE: ImageAsset = asset("petals.png");

const fn petals(mut screen: Screen) -> Screen {
    screen.entity(PETALS).set_plane(Plane::Front);
    screen.entity(PETALS).gate(Some(PETALS_ON));
    screen.entity(PETALS).image().change(PETALS_IMAGE).apply_animation(cut());
    screen.entity(PETALS).opacity().change(1.0).apply_animation(tween(ms(500)));
    screen
}
```

ハンドルは論理画面の一部です。ハンドルどうしの等価比較では、どの設定を指しているかを比べます。
そのため、ハンドルを使うレンダーも入力の画面だけで結果が決まる関数のままで、画面どうしの比較も
引き続きできます。ゲートを外すには `gate(None)` を使います。

## `Hud` {#the-hud}

CG を見せている間はテキストウィンドウを隠したい、といった場面があります。これは物語からシステム
への要求です。この要求は画面を経由させ、情報の流れを一方向に保ちます。

```kis
pub enum Visibility { Shown, Hidden }

impl Hud {
    #[builtin] pub const fn text_window(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn quick_menu(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn side_image(&mut self) -> &mut ImageLayer;
}
```

`Hud` は画面のリソースで、`screen.hud()` で取り出します。レンダーはほかのプロパティやレイヤーと
同じように変更し、システム側のゲーム画面は [`Narrative` ハンドル](./system.md#the-narrative-handle)
を通じてそれを読みます。物語が Hud を読むことはなく、Hud が物語の出力を読む形です。Hud は論理画面の
一部なので、一貫性検査の対象にもなります。

```kix
s!(render = |mut screen| { screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant()); screen });
```

`Prop<Visibility>` はフェードとして補間されます。`instant()` の代わりに `tween(ms(300))` を
使えば、ウィンドウがフェードアウトします。サイド画像はテキストの横に出る話者の顔画像で、ほかの
レイヤーと同じく `Transition` で差し替えます。

## 標準の画面 API {#the-standard-screen-api}

画面 API は標準ライブラリで定義しています。

- [`std::screen`](../../std/screen.md)：座標と色、描画面と描画順、エンティティ（生成時の既定値と
  消去を含む）、プロパティとレイヤー、Hud、選択肢メニュー、カメラ、シェーダー効果のスロット、
  ステップのタイムライン。
- [`std::screen::shader`](../../std/screen-shader.md) と
  [`std::shaders`](../../std/shaders.md)：シェーダーの役割、効果値、標準ライブラリの
  シェーダー集。
- [`std::screen::anim`](../../std/screen-anim.md)：イージング、補間、
  トランジション、モーションと、それぞれのプリセット。
- [`std::screen::audio`](../../std/screen-audio.md)：チャンネル、ループ、
  単発の音。
- [`std::stage`](../../std/stage.md)：その上に作ったキャラクター層。

`#[builtin]` の関数には本体がありません。画面とのインターフェースとして、エンジンが実装しています。
それ以外は普通の `const fn` のコードで、制作者が書く関数と同じようにコンパイラが評価します。
