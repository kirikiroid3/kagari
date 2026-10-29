---
title: "演出"
---

画面が *いつ* 変わるかは物語が決め、*どう* 変わるかはレンダー関数が決めます。この章ではその
モデルを説明します。画面 API の全体（座標、プレーン、エンティティ、プロパティ、レイヤー、
アニメーション、オーディオ）は STD リファレンスの
[`std::screen`](../../std/screen.md) にあります。

## レンダー関数 {#render-functions}

```kis
pub type Render = const fn(Screen) -> Screen;   // std::screen, in the prelude
```

**レンダー** は、ステップ前の画面からステップ後の画面への `const fn` です。両者をつなぐ
アニメーションも含みます。ステップはこれを `render` 属性として受け取ります。

```kix
s!(who = "Azusa", render = azusa_smiles, t!["Did you forget again?"]);

const fn azusa_smiles(mut screen: Screen) -> Screen {
    screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
    screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
    screen
}
```

物語がステップに到達すると、エンジンはまず画面のステップ限りの状態（タイムライン、予定された
アニメーション、再生待ちのワンショット音、選択肢メニュー）をリセットし、現在の画面に対して
レンダーを実行し、その結果を表示して待ちます。`render` のないステップも同じリセットを受け、
論理画面は変わりません。

### インラインレンダー {#inline-renders}

何もキャプチャしないクロージャは `const fn` ポインタに型強制されるので、短いレンダーはその場で
書けます。

```kix
s!(
    render = |mut screen| { screen.background().change(BG::ROOF).apply_animation(fade(ms(500))); screen },
    t!["The wind on the roof is cold."]
);
```

クロージャは定数を使い、`const fn` を呼べますが、何もキャプチャしません。ローカル変数や物語変数を
使うクロージャは `Render` ではありません。

### レンダーの合成 {#composing-renders}

ステップの `render` は一つです。複数のレンダーは普通の `const fn` で組み合わせます。

```kis
const fn roof_scene(screen: Screen) -> Screen {
    azusa_smiles(roof_background(screen))
}
```

## 一行ずつ読む {#walkthrough}

```kis
screen.entity(AZUSA).image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
```

1. `screen.entity(AZUSA)` は id が `AZUSA`（例えば `EntityId(217)`）のエンティティを選び、
   画面になければスポーンします。これは画面への [参照](../syntax/values.md) で、変更すると画面が
   変わります。
2. `.image()` はエンティティのメインの `ImageLayer` です。`.change(AZUSA::CLOSE_EYE_SMILE)` は
   そのレイヤーを画像 `AZUSA::CLOSE_EYE_SMILE` に差し替えるよう求めます。結果は保留中の変更で
   ある `&mut Swap`、つまりレイヤーへの参照です。
3. レイヤーの差し替えなので `Transition` が必要です。これがないと変更は起こりません。
4. `.apply_animation(cut())` で画像が即座に切り替わり、変更が反映されます。

参照はローカル変数に束縛できるので、同じエンティティを何度も変更するレンダーでは一度だけ名前を
付けられます。

```kis
let azusa = screen.entity(AZUSA);
azusa.image().change(AZUSA::CLOSE_EYE_SMILE).apply_animation(cut());
azusa.opacity().change(1.0).apply_animation(tween(ms(300)));
```

## 画面は ECS ワールド {#the-screen-is-an-ecs-world}

`Screen` はエンティティ・コンポーネント・システムのパターンに従います。

| 部分 | 定義場所 | 役割 |
| ---- | -------- | ---- |
| エンティティ | レンダー関数 | レンダーがスポーン・変更・デスポーンする。`EntityId` 定数で識別する |
| コンポーネント | `std::screen`（固定） | エンティティに付くデータ：プレーン、z、位置、アンカー、拡大率、回転、不透明度、色フィルター、ミックス、マスク、ローカルモーション、塗り、画像パーツ、ラベル、動画、反転、ゲート |
| リソース | `std::screen`（固定） | 画面そのもののデータ：Hud、選択肢メニュー、カメラ、画面フィルター、オーディオチャンネル |
| システム | エンジン | 実行時にエンティティを処理する。例えばアニメーションをフレームごとに進める |

`EntityId(0)` は背景エンティティ `BACKGROUND` 用に予約されています。スタジオの id は 1 から
始めます。

すべてのコンポーネント型は等価性を持つので、二つの画面は常に比較できます。システムは Kagari で
書かれず、論理画面の一部でもありません。ステップが見せる画面とは、レンダー後のエンティティと
そのコンポーネントの集合であり、アニメーション中にエンジンが描くフレームではありません。

コンポーネントの集合は固定で、スタジオはコンポーネントを追加しません。代わりに自分のモジュールで
`const fn` のヘルパーと定数データ（キャラクターの立ち位置、表情画像、暗転のようなレシピなど）を
足してライブラリを拡張します。`std::stage` はそうした層の標準的な例で、そのキャラクターは
`std::screen` の上の普通の `const fn` です。スタジオのヘルパーも他のコードと同じく型検査されます。

## 変更にはアニメーションが必要 {#changes-need-animations}

見えるものの変更は保留中の変更を生み、`apply_animation` だけがそれを画面上の実際の変更にします。
したがって、見える変更はすべて、答えが「即座に」であってもアニメーションの仕方を明示します。

対象ごとにアニメーションの型が異なるので、位置の変更に `Transition` を渡すような食い違いは
型検査で見つかります。

| 対象 | 変更方法 | アニメーション型 |
| ---- | -------- | ---------------- |
| プロパティ `Prop<T>`：位置、拡大率、回転、不透明度、フィルター・ミックス・マスク・ローカルモーション（シェーダー効果）、塗り。Hud の表示状態。カメラのオフセット・ズーム・回転。チャンネル音量 | `.change(v)` で `Change<T>` を得る | `Tween`：長さ、イージング、遅延 |
| レイヤー：`ImageLayer`（背景とサイド画像を含む）、`LabelLayer`、`MovieLayer` | `.change(x)`、`.paint(p)` または `.clear()` で `Swap` を得る | `Transition`：カット、クロスフェード、ルール、マスク、ワイプ、ブラインド、色を経由 |
| エンティティまたはカメラ全体 | 直接 `.apply_animation(m)` | `Motion`：出発点に戻る相対的な経路 |
| シェーダーの経路に沿ったエンティティのポーズ | 直接 `.travel(to, motion)` | `TransitionalMotion`：長さ、イージング、遅延 |
| 音楽または環境音のチャンネル | `.change(track)` または `.stop()` で `ChannelChange` を得る | `AudioFade` |

フィルター、ブレンドモード、マスク、グラデーションや天候などの生成コンテンツ、シェーダーによる
モーションは**シェーダー**です。GPU で実行される WGSL ファイルで、画面はシェーダーと引数を組にした
効果値として持ちます。画面と各エンティティは `filter()` を持ち、エンティティはさらに `mix()`
（下にあるものへの合成方法）、`mask()`、`local_motion()` を持ちます。画像レイヤーは生成された
`paint` を表示できます。効果を同じシェーダーの別の引数へトゥイーンすると、引数が補間されます。
標準ライブラリは既製のシェーダーを `std::shaders` に同梱しています。
[`std::screen::shader`](../../std/screen-shader.md) を参照してください。

音楽はステップをまたいで続きます。レンダーが音楽チャンネルをすでに再生中の曲のままにしておけば、
曲は最初からやり直しません。レンダーで曲を頭から再生し直すには `screen.replay_bgm()` を使います。

```kis
const fn mio_enters(mut screen: Screen) -> Screen {
    screen.entity(MIO).position().change(point(640, 80)).apply_animation(instant());
    screen.entity(MIO).image().change(MIO::NEUTRAL).apply_animation(cut());
    screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)).ease(Easing::OutQuad));
    screen
}
```

スポーンしたエンティティは不透明度 0.0 の不可視状態で始まるので、登場には必ずアニメーションが
書かれます。

`Change<T>`、`Swap`、`ChannelChange` は `#[must_use]` なので、
`screen.background().change(BG::ROOF);` のように変更が保留のまま終わる文は警告になります。
補間できないレイアウト設定だけは直接設定します：`set_plane`、`set_z`、`set_anchor`、
`set_flip`、`set_fill_size`、`gate`。

### アニメーションは論理画面を変えない {#animations-never-change-the-logical-screen}

ステップの **論理画面** は、レンダー後に残る永続的な状態です。

- エンティティとそのコンポーネント
- Hud、カメラ、画面フィルター
- オーディオチャンネル

セーブ、画面の等価性、[一貫性検査](#consistency) は論理画面だけを使い、アニメーションは決して
それを変えません。

- `Tween` と `Transition` は常に古い値から `change` で設定した値へ進みます。論理的な値は
  `change` を実行した瞬間から新しい値です。
- `Motion` は出発点で終わらなければならない相対的な経路です。そうでないモーションはコンパイル
  エラーです。
- ステップのタイムライン、予定されたアニメーション、ワンショット音、選択肢メニューはそのステップ
  だけのもので、論理画面には含まれません。次のステップに入るときにエンジンがリセットします。

したがって、ステップのアニメーションを完了させるクリック、スキップモード、プレイヤーによる
アニメーションのオフは、どれも論理画面を変えられません。

## レンダーは物語を見ない {#renders-are-blind-to-the-story}

レンダーは `const fn` なので、次の性質を持ちます。

- **決定的。** 同じ入力画面からは常に同じ出力画面が得られます。セーブのロードは物語を再生し、
  再生は画面を正確に再現しなければなりません（[セーブ、再生、決定性](./saves.md) を参照）。
- **実行される範囲で全域的。** コンパイラは、各レンダーをそのステップに到達しうるすべての画面に
  対して評価予算の範囲で評価します（[一貫性](#consistency) を参照）。終わらないレンダーは
  コンパイルエラーなので、エンジンは必ずステップに到達します。
- **物語を見ない。** 入力は画面だけです。`const fn` は物語変数を読めず、`render` 属性は関数
  ポインタなので、物語変数をキャプチャすることもできません。

逆に物語も画面を読みません。`Screen` は [有限型](./kinds.md#finite-types) ではないので、
`pushdown fn` は画面を保持できません。物語の状態が演出を決め、その逆はありません。

物語に依存する演出は物語の側で選びます。

```kix
match AFFECTION_MIO {
    3..=5 => s!(who = "Mio", render = mio_smiles, t!["You came."]),
    _ => s!(who = "Mio", render = mio_neutral, t!["Oh. It's you."]),
}
```

レンダーのポインタは有限な値なので、`render = if MET_MIO { mio_smiles } else { mio_neutral }`
のように式で選んだり、物語変数に保存したりもできます。どちらの場合も、コンパイラはその選択を
物語の状態の一部として扱います。

## キーフレーム {#keyframes}

出力が入力画面に依存しないレンダーは **キーフレーム** で、画面全体を宣言します。キーフレームは
コンパイラが自動で判定するので、注釈は不要です。それ以外のレンダーは *差分* です。

`Screen::empty()` は背景エンティティ `BACKGROUND` だけを持つ画面を返します。背景は不透明で画像は
ありません。Hud はテキストウィンドウとクイックメニューを表示し、カメラは初期状態、画面フィルターは
`None`、オーディオチャンネルはすべて無音です。したがってキーフレームは明示的にも書けます。

```kis
const fn roof_evening(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(BG::ROOF_EVENING).apply_animation(fade(ms(500)));
    screen
}
```

キーフレームは、あるステップの画面を作り直すためにどこまで遡ってレンダーを再実行すればよいかを
制限します。物語の先頭からではなく、経路上の最後のキーフレームから再実行すれば済みます。

<!-- diagram: presentation-keyframes (mermaid-ish)
flowchart LR
  subgraph with["経路上にキーフレームがある場合"]
    K1["K キーフレーム"] ==> d1((差分)) ==> d2((差分)) ==> d3((差分)) ==> K2["K キーフレーム"] ==> d4((差分)) ==> d5((差分)) ==> T1(("目標ステップ"))
    K2 -.->|"最も近いキーフレームから評価"| T1
  end
  subgraph without["経路上にキーフレームがない場合"]
    r1((レンダー)) ==> r2((レンダー)) ==> r3((...)) ==> T2(("目標ステップ"))
    r1 -.->|"最初からすべてのレンダーを再実行"| T2
  end
-->
{{#include ../diagrams/presentation-keyframes.svg}}

キーフレームは選択肢の地点と章の始まりに置くことを推奨します。キーフレーム配置の検査は
オプトインで、[`kagari.toml`](../tools/projects.md) の `[build]` テーブルに
`keyframe-check = true` と書くと有効になります。有効なとき、すべての `choice!` と各エントリー
シーンの最初のステップにはキーフレームのレンダーが必要で、ないステップは警告 `W5011` になります。
レンダーのないステップはキーフレームではありません。どこでも起こりうる電話のような割り込みシーンは
キーフレームで *始めるべきではありません*。割り込んだ先の画面に重ねて見せるためのものだからです。

## 一貫性 {#consistency}

レンダーは *直前の* 画面を変換するので、あるステップの画面はそこへの到達経路に依存します。

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
  M -.- W["コンパイラがこのステップを報告 (W5004)、両方の経路とともに"]
-->
{{#include ../diagrams/presentation-consistency.svg}}

最後のステップは、一方の経路では屋上を、もう一方では教室を表示します。意図通りなら問題ありません。
分岐の消し忘れなら、まさに作者以外の誰にも見えない種類のバグです。

コンパイラはステップごとにそこへ到達しうる画面を計算し、表示される画面（レンダー後の論理画面）が
物語の状態で決まらないステップを報告します。報告には、異なる経路で最後に画面を変えたレンダーも
添えられます。これが警告 `W5004` です。

あるステップでの *物語の状態* とは、物語自身が区別できるものすべてです：ステップ、物語変数の値、
コールスタック上の各フレームのうち後で使われるローカル変数、エントリー引数、そしてスタック上の
呼び出し位置です。これらがすべて一致する二つの到達は、同じ画面を表示しなければなりません。
したがって、どこでも起こりうる電話のような割り込みシーンは、呼び出し位置ごとに異なる画面を
表示してよいものの、一つの呼び出し位置で二つの画面を表示してはいけません。合流は、その直後の
最初のステップで一度だけ報告されます。それ以降のステップの違いは同じ原因によるものだからです。

報告は次のどちらかの方法で直します。

- 違いが消し忘れなら、報告された最初の不整合なステップに **キーフレームを挿入** します。警告が
  提案するのはこの修正です。
- 意図した違いなら、**依存を物語変数に昇格** します。各分岐で変数を設定し、合流後のステップの
  レンダーをその変数で選びます。するとコンパイラはその依存を物語の状態として扱います。

## 非有界な画面 {#unbounded-screens}

レンダーは入力画面からコンポーネントを読み、そこから新しい値を計算できます。例えばエンティティを
今の位置から 10 単位右に動かすことです。プレイヤーが何度も戻ってくるハブのようなループの中では、
これが周回ごとに異なる画面を生むので、ステップでの画面の集合に上限がなくなり、一貫性検査が
終わらなくなります。

レンダーはチューリング完全な `const fn` なので、その集合が有限かどうかは一般には決定できません。
*暫定:* コンパイラは、各レンダーをそのステップに到達する画面に対して評価し、変化がなくなるまで
集合を計算します。あるステップの集合が固定の上限（現在 64 画面）を超えると計算を打ち切り、
新しい画面を生んだレンダーをエラー `E5005` として報告します。その後コンパイラはそのレンダーが
画面を変えなかったものとみなして集合を計算し直すので、プログラムの残りも検査されます。

この規則は健全です。プログラムが受理されたなら、計算された集合は正確です。ただし、上限を超えた
ところでのみ有界になるレンダーは拒否されることがあります。ループの中では絶対的な値を設定する
レンダー（キーフレーム、あるいは「さらに 10 単位」ではなく `change(point(640, 80))`）を書いて
ください。

## レンダーの中の設定 {#settings-in-renders}

レンダーが設定を必要とすることがあります。例えばプレイヤーが有効にしたときだけ装飾効果を描く
場合です。レンダーは設定の値を読めません。値は実行時にしか分からず、もし読めればステップの画面が
設定に依存し、上で述べた解析が設定のすべての値について成り立たなくなるからです。

代わりにレンダーは **遅延参照** を使います。`static PETALS_ON: Setting<bool>` の `PETALS_ON` は
設定の値ではなく設定へのハンドルです（[設定とメタ](./system.md#settings-and-meta) を参照）。
`const fn` の中でハンドルの値を読む方法はありません。`Setting::get` は `fn` で、ハンドルに対する
`match` や算術もありません。レンダーにできるのは `Entity::gate` に渡すことだけで、エンジンは描画時に
値を読みます。エンティティは設定が真の間だけ描かれます。

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

ハンドルは論理画面の一部です。ハンドルの等価性はどの設定を指すかを比較するので、ハンドルを使う
レンダーも入力画面の関数のままであり、画面は引き続き比較できます。`gate(None)` でゲートを
外します。

## `Hud` {#the-hud}

物語は、例えば CG の間にテキストウィンドウを隠したいことがあります。これは物語からシステムへの
要求です。この要求は画面を経由し、流れを一方向に保ちます。

```kis
pub enum Visibility { Shown, Hidden }

impl Hud {
    #[builtin] pub const fn text_window(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn quick_menu(&mut self) -> &mut Prop<Visibility>;
    #[builtin] pub const fn side_image(&mut self) -> &mut ImageLayer;
}
```

`Hud` は画面のリソースで、`screen.hud()` で取得します。レンダーは他のプロパティやレイヤーと同じ
ように変更し、システムのゲームビューは [`Narrative` ハンドル](./system.md#the-narrative-handle)
を通じてそれを読みます。物語は Hud を読みません。Hud が物語の出力を読むのです。Hud は論理画面の
一部なので、一貫性検査の対象にもなります。

```kix
s!(render = |mut screen| { screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant()); screen });
```

`Prop<Visibility>` はフェードとして補間されるので、`instant()` の代わりに `tween(ms(300))` を
使うとウィンドウがフェードアウトします。サイド画像はテキストの横に表示される話者の顔で、
他のレイヤーと同じく `Transition` で差し替えます。

## 標準の画面 API {#the-standard-screen-api}

画面 API は STD リファレンスが定義します。

- [`std::screen`](../../std/screen.md)：座標と色、プレーンと描画順、スポーン時の
  既定値とデスポーンを含むエンティティ、プロパティとレイヤー、Hud、選択肢メニュー、カメラ、
  シェーダー効果のスロット、ステップのタイムライン。
- [`std::screen::shader`](../../std/screen-shader.md) と
  [`std::shaders`](../../std/shaders.md)：シェーダーのロール、効果値、std の
  シェーダーライブラリ。
- [`std::screen::anim`](../../std/screen-anim.md)：イージング、トゥイーン、
  トランジション、モーションとそのプリセット。
- [`std::screen::audio`](../../std/screen-audio.md)：チャンネル、ループ、
  ワンショット音。
- [`std::stage`](../../std/stage.md)：その上に作られたキャラクター層。

`#[builtin]` 関数には本体がありません。エンジンが画面へのインターフェースとして実装します。
それ以外は普通の `const fn` のコードで、スタジオの関数と同じくコンパイラが評価します。
