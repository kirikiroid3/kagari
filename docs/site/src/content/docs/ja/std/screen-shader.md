---
title: "std::screen::shader"
---

> **安定性:** 暫定 — シェーダーの役割、型付きのシェーダー引数、画面が保持する効果値（フィルター、合成、マスク、塗り、モーション）を扱います。

**シェーダー**とは、レンダラーが GPU 上で実行する WGSL ファイル（`.wgsl`）です。シェーダーには 6 種類の**役割**があり、どのシェーダーもそのうちの 1 つに属します。役割と引数の型は、アセット定数の型で表します。

```kis
use std::screen::shader::{ColorShader, filter};

#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
```

シェーダーをそのまま画面に置くことはできません。このモジュールのコンストラクターでシェーダーと引数を組み合わせて**効果値**を作り、画面にはその効果値を設定します。

| 役割 | アセット型 | 効果値 | コンストラクター | 使う場所 |
| ---- | ---------- | ------------ | ----------- | ------------- |
| 色 | [`ColorShader<A>`](#colorshader) | [`Filter`](#filter) | [`filter`](#filter-fn) | [`Screen::filter`](./screen.md#screen-filter)、[`Entity::filter`](./screen.md#entity-filter) |
| 合成 | [`MixShader<A>`](#mixshader) | [`Mix`](#mix) | [`mix`](#mix-fn) | [`Entity::mix`](./screen.md#entity-mix) |
| マスク | [`MaskShader<A>`](#maskshader) | [`Mask`](#mask) | [`mask`](#mask-fn) | [`Entity::mask`](./screen.md#entity-mask)、[`Paint::masked`](#paint-masked)、[`anim::masked`](./screen-anim.md#transition-constructors) |
| テクスチャー | [`TextureShader<A>`](#textureshader) | [`Paint`](#paint) | [`paint`](#paint-fn) | [`ImageLayer::paint`](./screen.md#imagelayer) |
| ローカルモーション | [`LocalMotionShader<A>`](#localmotionshader) | [`LocalMotion`](#localmotion) | [`local_motion`](#local_motion-fn) | [`Entity::local_motion`](./screen.md#entity-local_motion) |
| 遷移モーション | [`TransitionalMotionShader<A>`](#transitionalmotionshader) | [`TransitionalMotion`](#transitionalmotion) | [`transitional_motion`](#transitional_motion-fn) | [`Entity::travel`](./screen.md#entity-travel) |

コンストラクターが受け取る引数の型は、シェーダーの引数型 `A` に限られます。そのため、シェーダーに型の違う引数を渡したり、ある役割のシェーダーを別の役割の場所で使ったりすることはできません。効果値のフィールドは非公開なので、コンストラクター以外の方法では作れません。

効果値は `Eq` を実装しています。同じシェーダーファイルを等しい引数で使った効果は、互いに等しくなります。効果値は[論理画面](./screen.md#the-logical-screen)に含まれるため、フィルターの引数だけが違う 2 つの画面も別の画面として扱います。シェーダー型と効果値は有限型ではありません。

`std` には既製のシェーダーが [`std::shaders`](./shaders.md) として付属しています。役割ごとのシェーダーの書き方、引数のレイアウト、シェーダーに渡される時刻については[シェーダーの書き方](./guides/shaders.md)で説明します。このモジュールのアイテムはプレリュードに含まれないので、`use std::screen::shader::…` で取り込んでください。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`ShaderArg`](#shaderarg)、[`Shader`](#shader) | 組み込みのトレイト | 引数の型、シェーダーの型 |
| [`ColorShader<A>`](#colorshader) … [`TransitionalMotionShader<A>`](#transitionalmotionshader) | 組み込みの構造体 | 役割ごとのシェーダーファイル |
| [`ShaderArgs`](#shaderargs) | 組み込みの構造体 | 型を消去した引数 |
| [`Filter`](#filter)、`filter` | 構造体とコンストラクター | 色のフィルター |
| [`Mix`](#mix)、`mix` | 構造体とコンストラクター | 合成の方法 |
| [`Mask`](#mask)、`mask` | 構造体とコンストラクター | アルファマスク |
| [`Paint`](#paint)、`paint` | 構造体とコンストラクター | シェーダーで生成する画像 |
| [`LocalMotionBehavior`](#localmotionbehavior) | 列挙型 | ローカルモーションの時刻の進み方 |
| [`LocalMotion`](#localmotion)、`local_motion` | 構造体とコンストラクター | エンティティの姿勢を基準にした動き |
| [`TransitionalMotion`](#transitionalmotion)、`transitional_motion` | 構造体とコンストラクター | ある姿勢から別の姿勢への経路 |

## トレイト：`ShaderArg` {#shaderarg}

```kis
#[builtin] pub trait ShaderArg {}
```

シェーダーの引数にできる型です。ユニット型 `()` と、[`#[shader_arg]`](../reference/special/attributes.md#shader_arg) を付けた構造体がこれに当たります。実装はコンパイラが行い、ゲーム側では実装できません。`#[shader_arg]` 構造体のフィールドには名前が必要で、型は[引数](./guides/shaders.md#arguments)に挙げたものに限られます。

## トレイト：`Shader` {#shader}

```kis
#[builtin] pub trait Shader {}
```

以下に挙げる 6 つのシェーダーアセット型が実装するトレイトです。

## 構造体：`ColorShader<A>` {#colorshader}

```kis
#[builtin] pub struct ColorShader<A>;
impl<A> Asset for ColorShader<A> {}
impl<A> Eq for ColorShader<A> {}
impl<A> Shader for ColorShader<A> {}
```

下に描かれたものの色を変える WGSL ファイルで、引数の型は `A` です。[`filter`](#filter-fn) に渡して使います。`asset("….wgsl")` で宣言します。

## 構造体：`MixShader<A>` {#mixshader}

```kis
#[builtin] pub struct MixShader<A>;
impl<A> Asset for MixShader<A> {}
impl<A> Eq for MixShader<A> {}
impl<A> Shader for MixShader<A> {}
```

エンティティを下にあるものと合成する WGSL ファイルです。[`mix`](#mix-fn) に渡して使います。

## 構造体：`TextureShader<A>` {#textureshader}

```kis
#[builtin] pub struct TextureShader<A>;
impl<A> Asset for TextureShader<A> {}
impl<A> Eq for TextureShader<A> {}
impl<A> Shader for TextureShader<A> {}
```

画像を生成する WGSL ファイルです。[`paint`](#paint-fn) に渡して使います。

## 構造体：`MaskShader<A>` {#maskshader}

```kis
#[builtin] pub struct MaskShader<A>;
impl<A> Asset for MaskShader<A> {}
impl<A> Eq for MaskShader<A> {}
impl<A> Shader for MaskShader<A> {}
```

画素ごとのアルファ値を計算する WGSL ファイルです。[`mask`](#mask-fn) に渡して使います。

## 構造体：`LocalMotionShader<A>` {#localmotionshader}

```kis
#[builtin] pub struct LocalMotionShader<A>;
impl<A> Asset for LocalMotionShader<A> {}
impl<A> Eq for LocalMotionShader<A> {}
impl<A> Shader for LocalMotionShader<A> {}
```

エンティティを現在の姿勢を基準に動かす WGSL ファイルです。[`local_motion`](#local_motion-fn) に渡して使います。

## 構造体：`TransitionalMotionShader<A>` {#transitionalmotionshader}

```kis
#[builtin] pub struct TransitionalMotionShader<A>;
impl<A> Asset for TransitionalMotionShader<A> {}
impl<A> Eq for TransitionalMotionShader<A> {}
impl<A> Shader for TransitionalMotionShader<A> {}
```

エンティティをある姿勢から別の姿勢へ動かす WGSL ファイルです。[`transitional_motion`](#transitional_motion-fn) に渡して使います。

## 構造体：`ShaderArgs` {#shaderargs}

```kis
#[builtin] pub struct ShaderArgs;
impl Eq for ShaderArgs {}
```

効果値が保持する、型を消去したシェーダー引数です。比較は保持している引数の値で行います。ゲームのコードで直接作ることはありません。

## 構造体：`Filter` {#filter}

```kis
pub struct Filter {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

色のフィルターで、色シェーダーとその引数の組です。フィルターは、対象の色を変えます。画面のフィルターなら `Background`、`Stage`、`Front` の各描画面が、エンティティのフィルターならそのエンティティが対象です。

### `filter(shader, args)` {#filter-fn}

```kis
pub const fn filter<A: ShaderArg>(shader: ColorShader<A>, args: A) -> Filter
```

- `shader` `ColorShader<A>` — 使うシェーダー。
- `args` `A` — シェーダーの引数。
- 戻り値：`Filter` — `shader` に `args` を与えたフィルター。
- 種別：`const fn` · 組み込み：いいえ。

```kis
screen.filter().change(Some(filter(MEMORY, Memory { strength: 0.8 }))).apply_animation(tween(ms(800)));
```

## 構造体：`Mix` {#mix}

```kis
pub struct Mix {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

エンティティを下にあるものと合成する方法で、合成シェーダーとその引数の組です。シェーダーはエンティティとその背後を参照して、合成後の画素を返します。ブレンドモードやすりガラスのような効果は、この合成で実現します。

### `mix(shader, args)` {#mix-fn}

```kis
pub const fn mix<A: ShaderArg>(shader: MixShader<A>, args: A) -> Mix
```

- 戻り値：`Mix` — `shader` に `args` を与えた合成。
- 種別：`const fn` · 組み込み：いいえ。

## 構造体：`Mask` {#mask}

```kis
pub struct Mask {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

アルファマスクで、マスクシェーダーとその引数の組です。シェーダーは矩形内の各画素について 0.0〜1.0 の値を返します。マスクを使う場所は次の 3 つです。

- [`Entity::mask`](./screen.md#entity-mask)：エンティティの矩形上で計算し、エンティティのアルファに掛けます。
- [`Paint::masked`](#paint-masked)：塗りの上で計算し、塗りのアルファに掛けます。
- [`anim::masked`](./screen-anim.md#transition-constructors)：動いていくしきい値がマスクの値に達した画素から、トランジションで切り替えます。

### `mask(shader, args)` {#mask-fn}

```kis
pub const fn mask<A: ShaderArg>(shader: MaskShader<A>, args: A) -> Mask
```

- 戻り値：`Mask` — `shader` に `args` を与えたマスク。
- 種別：`const fn` · 組み込み：いいえ。

## 構造体：`Paint` {#paint}

```kis
pub struct Paint {
    texture: ShaderAsset,
    args: ShaderArgs,
    size: Size,
    mask: Option<Mask>,
}
```

シェーダーで生成する `size` ピクセルの画像で、テクスチャーシェーダーとその引数の組です。マスクを付けることもできます。[`ImageLayer`](./screen.md#imagelayer) では、`ImageLayer::paint` を使うと画像の代わりに塗りを表示します。塗りは `size` ピクセルの画像と同じように配置、サイズ調整、[リサイズ](./screen.md#resize)、反転ができ、トランジションで差し替えることもできます。グラデーション、光の玉、天候の表現などは塗りで作ります。

### `paint(shader, args, size)` {#paint-fn}

```kis
pub const fn paint<A: ShaderArg>(shader: TextureShader<A>, args: A, size: Size) -> Paint
```

- `size` `Size` — 生成する画像の大きさ（ピクセル）。
- 戻り値：`Paint` — `shader` が `args` を使って描く `size` ピクセルの塗り。マスクはなし。
- 種別：`const fn` · 組み込み：いいえ。

### `paint.masked(mask)` {#paint-masked}

```kis
pub const fn masked(mut self, mask: Mask) -> Paint
```

- 戻り値：塗りの矩形上で計算した `mask` をアルファに掛けた塗り。もう一度 `masked` を呼ぶと、前のマスクを置き換えます。
- 種別：`const fn` · 組み込み：いいえ。

```kis
use std::screen::{degrees, size};
use std::shaders::mask::rounded_rect;
use std::shaders::texture::linear_gradient;

screen.entity(PANEL).image().paint(linear_gradient(degrees(90.0), WHITE, BLACK, size(400, 300)).masked(rounded_rect(32.0, 4.0))).apply_animation(fade(ms(300)));
```

## 列挙型：`LocalMotionBehavior` {#localmotionbehavior}

```kis
pub enum LocalMotionBehavior {
    Periodic { after: Duration, period: Duration },
    Stall { after: Duration },
}
```

ローカルモーションの時刻の進め方です。ローカルモーションのシェーダーは秒単位の時刻 `t` を受け取ります。モーション開始からの経過時間は、この指定に従って変換してからシェーダーに渡します。

| バリアント | シェーダーが受け取る `t` |
| ------- | ------------------- |
| `Periodic { after, period }` | 0 から `after` までは 1 回だけ進み、その後は `period` の区間を繰り返す（`after + ((time − after) mod period)`）。`period` が 0 なら `after` で止まる |
| `Stall { after }` | `after` まで進んだら止まる（`min(time, after)`） |

`Periodic { after: ZERO, period }` は単純なループになります。`after` を 0 以外にすると、ループの前に導入部分を再生します。`Stall` は、減衰していく揺れのように最後に静止するモーションに向いています。

## 構造体：`LocalMotion` {#localmotion}

```kis
pub struct LocalMotion {
    shader: ShaderAsset,
    args: ShaderArgs,
    behavior: LocalMotionBehavior,
}
```

エンティティの姿勢を基準にした動きで、ローカルモーションシェーダー、その引数、時刻の規則の組です。シェーダーはそのフレームでのエンティティの姿勢（補間と [`Motion`](./screen-anim.md#motion) を反映したもの）を受け取り、描画に使う姿勢を返します。変わるのは見た目だけで、論理上の位置、倍率、回転は変わりません。

### `local_motion(shader, args, behavior)` {#local_motion-fn}

```kis
pub const fn local_motion<A: ShaderArg>(shader: LocalMotionShader<A>, args: A, behavior: LocalMotionBehavior) -> LocalMotion
```

- `behavior` `LocalMotionBehavior` — モーションの時刻の進め方。
- 戻り値：`LocalMotion` — `shader` に `args` を与え、時刻を `behavior` で変換するローカルモーション。
- 種別：`const fn` · 組み込み：いいえ。

## 構造体：`TransitionalMotion` {#transitionalmotion}

```kis
pub struct TransitionalMotion {
    shader: ShaderAsset,
    args: ShaderArgs,
    duration: Duration,
    easing: Easing,
    delay: Duration,
}
```

ある姿勢から別の姿勢への経路で、遷移モーションシェーダー、その引数、タイミングの組です。[`Entity::travel`](./screen.md#entity-travel) でエンティティをこの経路に沿って動かします。シェーダーは開始と終了の姿勢、イージング適用後の進行度（0〜1）、時刻を受け取り、描画に使う姿勢を返します。指定の長さが過ぎると、シェーダーが何を返していても、エンティティは終了の姿勢になります。

### `transitional_motion(shader, args, duration)` {#transitional_motion-fn}

```kis
pub const fn transitional_motion<A: ShaderArg>(shader: TransitionalMotionShader<A>, args: A, duration: Duration) -> TransitionalMotion
```

- `duration` `Duration` — 移動にかかる時間。
- 戻り値：`TransitionalMotion` — `shader` に `args` を与え、`duration` をかけて進むモーション。イージングは `Linear`、遅延はなし。
- 種別：`const fn` · 組み込み：いいえ。

### `motion.ease(easing)`、`motion.delay(delay)` {#transitionalmotion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> TransitionalMotion
pub const fn delay(mut self, delay: Duration) -> TransitionalMotion
```

`ease` は進行度に `easing` を適用したモーションを返し、`delay` はカーソルから `delay` 遅れて始まるモーションを返します。イージングは補間と同じ検査を受け、不正な `CubicBezier` や `Steps(0)` は定数評価エラーになります。種別：`const fn` · 組み込み：いいえ。

```kis
use std::screen::{degrees, pose};
use std::screen::anim::Easing;
use std::shaders::motion::arc;

screen.entity(BIRD).travel(pose(point(900, 400), 1.0, degrees(0.0)), arc(200.0, ms(1200)).ease(Easing::OutQuad));
```
