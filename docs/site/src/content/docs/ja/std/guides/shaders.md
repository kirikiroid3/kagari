---
title: "シェーダーの書き方"
---

シェーダーとは、アセットディレクトリに置く WGSL ファイルで、レンダラーが GPU 上で実行するものです。この手引きでは、[`std::screen::shader`](../screen-shader.md) にある 6 つの役割それぞれについて、シェーダーの書き方を説明します。ファイルに何を定義するか、どんな引数を読めるか、どの時刻を受け取るかを順に見ていきます。[`std::shaders`](../shaders.md) の既製シェーダーも同じ規則で書かれているので、そのソースもよい参考になります。

## 最初のシェーダー {#a-first-shader}

例として、かかった範囲の彩度を下げる色フィルターを作ります。効果の強さは引数で指定します。Kagari 側では、引数の型とアセット定数を宣言します。

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

WGSL ファイルでは、同じフィールドを並べた `Args` を宣言し、`effect` を定義します。

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

ファイルに書くのは `Args`、`effect`、それに自分で使う補助関数だけです。Kagari は、役割ごとの**プレリュード**をファイルの先頭に付け足します。バインディング（`args` など）、エントリーポイント、`kg_` で始まる補助関数はプレリュードが宣言します。バインディングや、`@vertex`、`@fragment`、`@compute` の関数を自分で宣言してはいけません。

## 役割 {#roles}

アセットの型でシェーダーの役割が決まり、役割によってファイルに定義する `effect` 関数の形が決まります。

| アセット型 | `effect` | 入力 |
| ---------- | -------- | ----- |
| `ColorShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` は画面上の位置で、デザイン解像度全体を 0..1 で表す。`kg_source(uv)` でフィルターがかかる範囲の色を読む |
| `MixShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` は色シェーダーと同じ。`kg_source(uv)` でエンティティの色（不透明度を適用済み）を、`kg_backdrop(uv)` でその下の色を読む。返すのは**最終的な**ピクセルの色なので、合成はシェーダー自身が行う |
| `TextureShader<A>` | `fn effect(uv: vec2<f32>) -> vec4<f32>` | `uv` は塗り全体を 0..1 で表す。`kg_size()` は塗りのピクセル単位の大きさ（`size` の値）で、レイヤーの [`Resize`](../screen.md#resize) には左右されない |
| `MaskShader<A>` | `fn effect(uv: vec2<f32>) -> f32` | `uv` はマスクをかける矩形全体を 0..1 で表す。`kg_size()` はその矩形のピクセル単位の大きさ。0.0 から 1.0 の値を返す。範囲外の値はこの範囲に丸める |
| `LocalMotionShader<A>` | `fn effect(state: KgState, t: f32) -> KgState` | `state` はこのフレームでのエンティティの姿勢、`t` は変換後の時刻 |
| `TransitionalMotionShader<A>` | `fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState` | 始点と終点の姿勢、イージング適用後の進み具合（0..1）、時刻 |

`from` のように、Kagari では名前に使えても WGSL では予約されている語があります。WGSL ファイルの中では、そうした語を引数名や `Args` のフィールド名に使えません。`effect` の引数名は自由に付けられます。

合成シェーダーは、通常の合成処理の代わりに使われます。合成シェーダーのプレリュードには、標準ライブラリのブレンドモードが使っている W3C の合成式の補助関数があります。`kg_straight(c)`（範囲内に丸めたストレートアルファの色）、`kg_composite(s, b, m)`（ソース `s` を背景 `b` の上に、混ぜた色 `m` で合成する）、それにブレンド関数 `kg_hard_light`、`kg_soft_light`、`kg_dodge`、`kg_burn`、`kg_lum`、`kg_clip_color`、`kg_set_lum`、`kg_sat`、`kg_set_sat` です。乗算モードなら次のように書けます。

```wgsl
alias Args = KgNoArgs;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    return kg_composite(s, b, kg_straight(b) * kg_straight(s));
}
```

エンティティを上下にふわふわ動かすローカルモーションの例です。

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

- **色**は、どこでも乗算済みアルファで、リニア色空間の値です。`kg_source` と `kg_backdrop` が返す値、`Color` 型の引数、`effect` が返す値のすべてが該当します。CSS のように sRGB の値で計算したいときは、乗算済みアルファを戻してから変換し、計算が終わったら逆に変換します。使う関数は `kg_srgb_to_linear`、`kg_linear_to_srgb`、`kg_unpremultiply`、`kg_premultiply` です。
- **位置と大きさ**は、デザイン解像度でのピクセル単位です。y 軸は下向きです。
- **`KgState`** は `{ position: vec2<f32>, scale: f32, rotation: f32 }` です。`position` はエンティティのアンカーの位置、`scale` は拡大率、`rotation` は時計回りの回転角（ラジアン）です。不透明度はモーションでは扱いません。
- **時刻**は秒単位です。`t` も `kg_time()` も同じです。
- **プレリュードの名前**は `kg_`、`Kg`、`KG_` のいずれかで始まります。自分で付ける名前には、これらの接頭辞を使わないでください。

プレリュードが役割ごとに用意するものは次のとおりです。

| 補助関数・定数 | 役割 | 値 |
| ------ | ----- | ----- |
| `KG_PI` | すべて | π |
| `kg_time()` | すべて | 効果の時刻（秒） |
| `kg_srgb_to_linear(c)`、`kg_linear_to_srgb(c)` | すべて | `vec3<f32>` に対する伝達関数 |
| `kg_unpremultiply(c)`、`kg_premultiply(c)` | すべて | `vec4<f32>` を乗算済みアルファとストレートアルファの間で変換する。完全に透明な色を `kg_unpremultiply` にかけると 0 になる |
| `kg_size()` | テクスチャー、マスク | 塗り、またはマスクをかける矩形のピクセル単位の大きさ |
| `kg_target_size()`、`kg_texel()` | 色、合成、テクスチャー、マスク | デザイン解像度（ピクセル）と、`uv` で表した 1 ピクセル分の大きさ |
| `kg_source(uv)` | 色、合成 | 効果がかかる対象の色 |
| `kg_backdrop(uv)`、`kg_opacity()` | 合成 | エンティティの下にある色と、エンティティの不透明度 |

## 引数 {#arguments}

シェーダー型の `A` は、そのシェーダーの引数の型です。`()` か、[`#[shader_arg]`](../../reference/special/attributes.md#shader_arg) を付けた構造体を指定します。`#[shader_arg]` 構造体のフィールドには名前が必要で、型パラメータは使えません（const パラメータは使えます）。各フィールドの型は下の表のいずれかです。WGSL ファイルでは、対応するメンバーを同じ順序で並べた `struct Args` を宣言します。引数の型が `()` なら `alias Args = KgNoArgs;` と書きます。メンバーの対応は名前ではなく順序で決まるので、WGSL のメンバーに別の名前を付けてもかまいません。たとえば Kagari のフィールド名が WGSL の予約語に当たる場合に使えます。値はプレリュードが `args` という名前でバインドします。

| Kagari のフィールド型 | WGSL の型 | 値 |
| ----------------- | --------- | ----- |
| `bool` | `u32` | 0 または 1 |
| `i32`、`u32` | `i32`、`u32` | そのまま |
| `Decimal`、`Rat` | `f32` | 最も近い `f32` の値 |
| `Color` | `vec4<f32>` | 乗算済みアルファ、リニア色空間 |
| `Point`、`Offset`、`Size` | `vec2<f32>` | ピクセル |
| `Anchor` | `vec2<f32>` | 割合 |
| `Angle` | `f32` | ラジアン、時計回り |
| `Duration` | `f32` | 秒 |
| フィールドを持たない列挙型 | `u32` | 宣言順でのバリアントの番号（0 始まり） |
| `#[shader_arg]` 構造体 | 同じメンバーを同じ順序で並べた構造体 | フィールドごとに変換 |
| `[T; N]`（`N` > 0） | `array<T', N>` | 要素ごとに変換 |

値は WGSL のストレージの規則（アラインメントとパディングを含む）に従ってメモリに並べるので、対応する WGSL の宣言をそのまま書けば正しく読めます。配列や入れ子の構造体も使えます。

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

- Kagari 側はコンパイラが検査します。構造体以外に `#[shader_arg]` を付けるとエラー [`E3018`](../../diagnostics/kinds.md#e3018)、フィールドに名前のない構造体や型パラメータのある構造体に付けるとエラー [`E3025`](../../diagnostics/kinds.md#e3025)、表にない型のフィールドや長さ 0 の配列があるとエラー [`E3026`](../../diagnostics/kinds.md#e3026) になります。
- WGSL の `Args` が Kagari の型と一致しているかどうかは、今のところどこでも検査していません。両者は手作業で揃えてください。*予定：* 両者を突き合わせるビルド時の検査。
- メモリに並べた値の大きさは 16384 バイトまでです。これを超えると、そのスロットは効果なしで描画され、警告がログに出ます。

## 効果の時刻 {#effect-time}

どの効果も、`kg_time()` で秒単位の時刻を受け取ります（モーションでは `t` でも受け取ります）。

- **フィルター、合成、マスク、ローカルモーション、塗り**の時刻は、それぞれの**スロット**ごとの時計で進みます。スロットとは効果を設定する場所のことで、画面のフィルター、エンティティのフィルター・合成・マスク・ローカルモーション、画像レイヤーの塗りがあります。スロットに新しいシェーダーを設定すると、設定した補間やトランジションの開始時点で時計が 0 から動き始めます。その後の変更で同じシェーダーが使われ続ける限り、引数をどう変えても、ステップをまたいでも、時計は止まりません。ここでいう「同じシェーダー」とは、塗りならテクスチャーシェーダーとマスクシェーダーの両方が同じこと、ローカルモーションならシェーダーと振る舞いの両方が同じことです。
- 次のステップへ進むと、変更のなかったスロットの時計はそのまま引き継がれます。ロード直後のように引き継ぐ時計がない場合、ステップの開始時点ですでに効果が設定されているスロットは、ステップの開始時点から時計が動き始めます。
- **トランジションで使うマスク**の時刻は、トランジションの開始から数えます。
- **移動**（`travel`）の時刻は、移動の開始から数えます。`progress` はイージングを適用した移動の進み具合で、0 から 1 まで変化します。
- `animations` 設定がオフのときは、効果の時刻はすべて 0 になり、移動は一瞬で終わります。

ローカルモーションでは、[`LocalMotionBehavior`](../screen-shader.md#localmotionbehavior) が時計の値を変換してからシェーダーに渡します。`Periodic { after, period }` は `0..after` を 1 回再生した後、`after..after + period` を繰り返します。`Stall { after }` は `after` で止まります。そのため、繰り返しのシェーダーは `t` が大きくなりすぎないことを前提にでき、収束するモーションは式の終わりでぴたりと止められます。

シェーダーはフレームをまたいで状態を保持できません。毎フレーム、時刻と引数だけから効果を計算します。微分方程式に従うモーションは、[`sine_integral`](../shaders-motion.md#sine_integral) のように、毎フレームシェーダーの中で 0 から `t` まで解き直します。

## 引数の補間 {#tweening-arguments}

効果のスロットは `Prop` なので、`change(…).apply_animation(tween(…))` でアニメーションできます。変更前と変更後の効果が同じシェーダーを使っている場合（ローカルモーションでは振る舞いも同じ場合）、補間によって引数が変化します。`f32` の値やベクトル（小数、色、点、大きさ、角度、時間の長さ）は補間のイージングに従って変わり、`u32`／`i32` の値（整数、`bool`、列挙型）は補間の開始時に切り替わります。それ以外の変更はすべて、補間の開始時に切り替わります。引数を補間している間も、効果の時計は進み続けます。

```kis
use std::shaders::color::blur;

screen.filter().change(Some(blur(0.0))).apply_animation(instant());
screen.wait(ms(500));
screen.filter().change(Some(blur(6.0))).apply_animation(tween(ms(1500)));
```

## シェーダーの検査 {#checking-shaders}

`kagari build` は、ゲームで使うすべてのシェーダーを、使われている役割ごとに検査します。役割に対応するプレリュードをファイルの先頭に付け、その結果を検証します。問題が見つかると、アセットのパス、役割、ファイル内の行番号を 1 行で出力し、ビルドは終了ステータス 1 で失敗します（`Args` がない場合のように、ファイル内の特定の行の問題でなければ行番号は出ません）。

```text
error: shader `memory.wgsl` (color): line 9: …
```

2 つの役割で使っているファイルは、役割ごとに 1 回ずつ検査します。実行時に、ある役割のシェーダーを GPU が実行できない場合（たとえばストレージバッファやコンピュートシェーダーに対応していない場合）は、そのスロットを `None` として描画し、警告をログに出します。詳しくは[シェーダー効果](../screen.md#shader-effects)を参照してください。

シェーダーファイルは普通のアセットとして扱います。`asset("…")` のパスはアセットディレクトリからの相対パスで、ビルド時にゲームのアーカイブへまとめられます。`std` のシェーダーは `std.kiarchive` にまとめられます。アセットディレクトリ直下の `std` ディレクトリは、そのために予約されています。
