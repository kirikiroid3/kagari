---
title: "シェーダーの書き方"
---

シェーダーは、アセットディレクトリに置く WGSL ファイルで、レンダラーが GPU で実行します。このガイドでは、[`std::screen::shader`](../screen-shader.md) の 6 つのロールそれぞれについて、ファイルが定義するもの、読む引数、受け取る時刻を説明します。[`std::shaders`](../shaders.md) の既製のシェーダーも同じ規則に従っているので、そのソースはよい例になります。

## 最初のシェーダー {#a-first-shader}

覆うものの彩度を下げ、その強さを引数に取る色フィルターです。Kagari 側では引数の型とアセット定数を宣言します。

```kis
use std::screen::shader::{ColorShader, filter};

/// The arguments of `MEMORY`.
#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");

const fn flashback(mut screen: Screen) -> Screen {
    screen.filter().change(Some(filter(MEMORY, Memory { strength: 0.8 }))).apply_animation(tween(ms(800)));
    screen
}
```

WGSL ファイルでは、同じフィールドを持つ `Args` を宣言し、`effect` を定義します。

```wgsl
// The flashback filter: desaturates the stage by `strength`.

struct Args {
    strength: f32,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = kg_source(uv);
    let gray = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(mix(c.rgb, vec3<f32>(gray), clamp(args.strength, 0.0, 1.0)), c.a);
}
```

ファイルに書くのは `Args`、`effect`、自分のヘルパーだけです。Kagari はファイルの前にロールごとの**プレリュード**を置きます。プレリュードはバインディング（`args` など）、エントリーポイント、`kg_` ヘルパーを宣言します。バインディングや `@vertex`、`@fragment`、`@compute` の関数を自分で宣言しないでください。

## ロール {#roles}

アセット型がロールを決め、ロールがファイルの定義する `effect` 関数を決めます。

| アセット型 | `effect` | 入力 |
| ---------- | -------- | ----- |
| `ColorShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` は画面上の位置で、デザイン解像度全体で 0..1。`kg_source(uv)` はフィルターが覆うものを読む |
| `MixShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` は色と同じ。`kg_source(uv)` はエンティティ（不透明度適用済み）、`kg_backdrop(uv)` はその下にあるものを読む。**最終的な**画素を返すので、合成はシェーダーが行う |
| `TextureShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` はペイント全体で 0..1。`kg_size()` はペイントのサイズ（ピクセル）で、レイヤーの [`Resize`](../screen.md#resize) にかかわらずその `size` |
| `MaskShader<A>` | `fn effect(uv: vec2<f32>) -> f32` | `uv` はマスクされる矩形全体で 0..1。`kg_size()` はそのサイズ（ピクセル）。0.0 から 1.0 を返す。範囲外の値は切り詰められる |
| `LocalMotionShader<A>` | `fn effect(state: KgState, t: f32) -> KgState` | `state` はこのフレームでのエンティティのポーズ、`t` は変換後の時刻 |
| `TransitionalMotionShader<A>` | `fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState` | 開始ポーズと終了ポーズ、イージング適用後の進行度 0..1、時刻 |

WGSL は、`from` など Kagari では名前に使える語の一部を予約しています。WGSL ファイルの中では、引数や `Args` のフィールドにそれらの名前は付けられません。`effect` の引数名は自由です。

ミックスシェーダーは通常の合成を置き換えます。ミックスのプレリュードには、std のブレンドモードが使う W3C の合成式のヘルパーがあります。`kg_straight(c)`（切り詰めたストレートの色）、`kg_composite(s, b, m)`（ソース `s` を背景 `b` の上に、混ぜた色 `m` で合成）、ブレンド関数 `kg_hard_light`、`kg_soft_light`、`kg_dodge`、`kg_burn`、`kg_lum`、`kg_clip_color`、`kg_set_lum`、`kg_sat`、`kg_set_sat` です。乗算モードは次のとおりです。

```wgsl
alias Args = KgNoArgs;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    return kg_composite(s, b, kg_straight(b) * kg_straight(s));
}
```

エンティティを上下に揺らすローカルモーションです。

```wgsl
struct Args {
    height: f32,
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    s.position.y -= args.height * (0.5 - 0.5 * cos(2.0 * KG_PI * t / args.period));
    return s;
}
```

## 約束事 {#conventions}

- **色**はどこでも乗算済みで、リニア光です。`kg_source` と `kg_backdrop` が返す値、`Color` の引数、`effect` が返す値がすべてそうです。CSS と同じように sRGB の値で計算するには、乗算を戻して変換し、計算後に変換し直します：`kg_srgb_to_linear`、`kg_linear_to_srgb`、`kg_unpremultiply`、`kg_premultiply`。
- **位置とサイズ**はデザイン解像度のピクセルで、y は下向きです。
- **`KgState`** は `{ position: vec2<f32>, scale: f32, rotation: f32 }` です。`position` はエンティティのアンカーが置かれる位置、`scale` は拡大率、`rotation` はラジアン単位の時計回りの回転です。不透明度はモーションに含まれません。
- **時刻**は秒単位です：`t` と `kg_time()`。
- **プレリュードの名前**は `kg_`、`Kg`、`KG_` で始まります。自分の名前にこれらの接頭辞を使わないでください。

プレリュードがロールごとに提供するもの：

| ヘルパー | ロール | 値 |
| ------ | ----- | ----- |
| `KG_PI` | すべて | π |
| `kg_time()` | すべて | 効果の時刻（秒） |
| `kg_srgb_to_linear(c)`、`kg_linear_to_srgb(c)` | すべて | `vec3<f32>` の伝達関数 |
| `kg_unpremultiply(c)`、`kg_premultiply(c)` | すべて | 乗算済みとストレートの `vec4<f32>` の変換。完全に透明な色の乗算を戻すと 0 |
| `kg_size()` | テクスチャー、マスク | ペイントまたはマスクされる矩形のサイズ（ピクセル） |
| `kg_target_size()`、`kg_texel()` | 色、ミックス、テクスチャー、マスク | デザイン解像度（ピクセル）と、`uv` 単位での 1 ピクセル |
| `kg_source(uv)` | 色、ミックス | 効果の下にある内容 |
| `kg_backdrop(uv)`、`kg_opacity()` | ミックス | エンティティの下にあるものと、エンティティの不透明度 |

## 引数 {#arguments}

シェーダー型の `A` がシェーダーの引数型です。`()` か、[`#[shader_arg]`](../../reference/special/attributes.md#shader_arg) を付けた構造体です。`#[shader_arg]` 構造体は名前付きフィールドを持ち、型パラメータを持ちません（const パラメータは使えます）。各フィールドの型は次のいずれかです。WGSL ファイルは、対応するメンバーを同じ順序で並べた `struct Args` を宣言します。引数型が `()` なら `alias Args = KgNoArgs;` と書きます。メンバーは位置で対応するので、WGSL が Kagari のフィールド名を予約している場合などには、WGSL のメンバーに別の名前を付けられます。プレリュードは値を `args` としてバインドします。

| Kagari のフィールド型 | WGSL の型 | 値 |
| ----------------- | --------- | ----- |
| `bool` | `u32` | 0 または 1 |
| `i32`、`u32` | `i32`、`u32` | そのまま |
| `Decimal`、`Rat` | `f32` | 最も近い `f32` |
| `Color` | `vec4<f32>` | 乗算済みのリニア光 |
| `Point`、`Offset`、`Size` | `vec2<f32>` | ピクセル |
| `Anchor` | `vec2<f32>` | 割合 |
| `Angle` | `f32` | ラジアン、時計回り |
| `Duration` | `f32` | 秒 |
| フィールドのない列挙型 | `u32` | 宣言の中でのバリアントの番号（0 から） |
| `#[shader_arg]` 構造体 | 同じメンバーを同じ順序で持つ構造体 | フィールドごと |
| `[T; N]`（`N` > 0） | `array<T', N>` | 要素ごと |

値は WGSL のストレージの規則（アラインメントとパディングを含む）で配置されるので、対応する WGSL の宣言で書いたとおりに読めます。配列や入れ子の構造体も使えます。

```kis
use std::screen::{Color, Point};

#[shader_arg]
pub struct Spot {
    pub position: Point,
    pub color: Color,
    pub radius: Decimal,
}

#[shader_arg]
pub struct Spots {
    pub count: u32,
    pub spots: [Spot; 4],
}
```

```wgsl
struct Spot {
    position: vec2<f32>,
    color: vec4<f32>,
    radius: f32,
}

struct Args {
    count: u32,
    spots: array<Spot, 4>,
}
```

- Kagari 側はコンパイラが検査します。構造体以外への `#[shader_arg]` は [`E3018`](../../diagnostics/kinds.md#e3018)、名前付きフィールドを持たない構造体や型パラメータを持つ構造体は [`E3025`](../../diagnostics/kinds.md#e3025)、ほかの型のフィールドや長さ 0 の配列は [`E3026`](../../diagnostics/kinds.md#e3026) です。
- WGSL の `Args` が Kagari の型と合っているかは、まだ何も検査しません。手で揃えてください。*予定：* 両者を比べるビルド時の検査。
- 配置後の値は 16384 バイト以下でなければなりません。それより大きい値では、スロットを効果なしで描き、警告をログに出します。

## 効果の時刻 {#effect-time}

効果はどれも `kg_time()`（モーションでは `t` も）で秒単位の時刻を受け取ります。

- **フィルター、ミックス、マスク、ローカルモーション、ペイント**は、その**スロット**の時計で進みます。スロットとは、画面フィルター、エンティティのフィルター・ミックス・マスク・ローカルモーション、画像レイヤーのペイントです。時計は、スロットに新しいシェーダーが設定されたとき、それを設定したトゥイーンやトランジションの開始時に 0 から始まります。その後の変更が同じシェーダーを保つ間は、引数がどう変わっても、ステップをまたいでも進み続けます。ペイントでは同じテクスチャーシェーダーと同じマスクシェーダー、ローカルモーションでは同じシェーダーと同じ振る舞いが「同じシェーダー」です。
- ゲームが次のステップへ進むと、変わらなかったスロットの時計は引き継がれます。ロードの後など、引き継ぐ時計がないときは、ステップのエントリー時にすでに効果を持っているスロットの時計はステップのエントリーから始まります。
- **トランジションの中のマスク**は、トランジションの開始から数えます。
- **トラベル**は開始から数えます。`progress` はトラベルのイージング適用後の進行度で、0 から 1 まで進みます。
- `animations` 設定がオフのとき、効果の時刻はすべて 0 で、トラベルは時間がかかりません。

ローカルモーションの [`LocalMotionBehavior`](../screen-shader.md#localmotionbehavior) は、シェーダーに渡す前に時計を変換します。`Periodic { after, period }` は `0..after` を 1 回再生した後 `after..after + period` を繰り返し、`Stall { after }` は `after` で止まります。そのためループするシェーダーは `t` が小さいままであることを当てにでき、落ち着くモーションは式が終わるちょうどその場所で止まれます。

シェーダーはフレーム間の記憶を持ちません。各フレームは時刻と引数だけから効果を計算します。微分方程式に従うモーションは、[`sine_integral`](../shaders-motion.md#sine_integral) のように、毎フレームシェーダーの中で 0 から `t` まで解きます。

## 引数のトゥイーン {#tweening-arguments}

効果のスロットは `Prop` なので、`change(…).apply_animation(tween(…))` でアニメーションします。古い効果と新しい効果が同じシェーダー（ローカルモーションでは同じ振る舞いも）を使うとき、トゥイーンは引数を動かします。`f32` の値とベクトル（小数、色、点、サイズ、角度、長さ）はトゥイーンのイージングに従い、`u32`／`i32` の値（整数、`bool`、列挙型）は開始時に切り替わります。それ以外の変更はトゥイーンの開始時に切り替わります。引数のトゥイーン中も効果の時計は進み続けます。

```kis
use std::shaders::color::blur;

screen.filter().change(Some(blur(0.0))).apply_animation(instant());
screen.wait(ms(500));
screen.filter().change(Some(blur(6.0))).apply_animation(tween(ms(1500)));
```

## シェーダーの検査 {#checking-shaders}

`kagari build` は、ゲームが使うすべてのシェーダーを、使われているロールごとに検査します。ロールのプレリュードをファイルの前に置き、その結果を検証します。問題があると、アセットのパス、ロール、ファイル内の行（`Args` がないときなど、ファイルの行にない問題では行なし）を示す行を出力し、ビルドは終了ステータス 1 で失敗します。

```text
error: shader `memory.wgsl` (color): line 9: …
```

2 つのロールで使うファイルは、ロールごとに 1 回ずつ検査されます。実行時に、あるロールを実行できない GPU（たとえばストレージバッファやコンピュートシェーダーのないもの）では、スロットを `None` と同じように描き、警告をログに出します。[シェーダー効果](../screen.md#shader-effects) を参照してください。

シェーダーファイルは普通のアセットです。`asset("…")` のパスはアセットディレクトリからの相対パスで、ビルドはファイルをゲームのアーカイブにパックします。`std` のシェーダーは `std.kiarchive` にパックされます。アセットディレクトリ直下の `std` ディレクトリはそのために予約されています。
