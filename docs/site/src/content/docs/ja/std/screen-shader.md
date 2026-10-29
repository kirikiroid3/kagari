---
title: "std::screen::shader"
---

> **安定性:** 暫定 — シェーダーのロール、型付きのシェーダー引数、画面が持つ効果値（フィルター、ミックス、マスク、ペイント、モーション）です。

**シェーダー**は、レンダラーが GPU で実行する WGSL ファイル（`.wgsl`）です。シェーダーはどれも 6 つの**ロール**のうち 1 つを持ち、アセット定数の型がロールと引数の型を表します。

```kis
use std::screen::shader::{ColorShader, filter};

#[shader_arg]
struct Memory {
    strength: Decimal,
}

const MEMORY: ColorShader<Memory> = asset("memory.wgsl");
```

シェーダーだけを画面に置くことはありません。このモジュールのコンストラクターがシェーダーと引数を組にして**効果値**を作り、画面は効果値を持ちます。

| ロール | アセット型 | 効果値 | コンストラクター | 使う場所 |
| ---- | ---------- | ------------ | ----------- | ------------- |
| 色 | [`ColorShader<A>`](#colorshader) | [`Filter`](#filter) | [`filter`](#filter-fn) | [`Screen::filter`](./screen.md#screen-filter)、[`Entity::filter`](./screen.md#entity-filter) |
| ミックス | [`MixShader<A>`](#mixshader) | [`Mix`](#mix) | [`mix`](#mix-fn) | [`Entity::mix`](./screen.md#entity-mix) |
| マスク | [`MaskShader<A>`](#maskshader) | [`Mask`](#mask) | [`mask`](#mask-fn) | [`Entity::mask`](./screen.md#entity-mask)、[`Paint::masked`](#paint-masked)、[`anim::masked`](./screen-anim.md#transition-constructors) |
| テクスチャー | [`TextureShader<A>`](#textureshader) | [`Paint`](#paint) | [`paint`](#paint-fn) | [`ImageLayer::paint`](./screen.md#imagelayer) |
| ローカルモーション | [`LocalMotionShader<A>`](#localmotionshader) | [`LocalMotion`](#localmotion) | [`local_motion`](#local_motion-fn) | [`Entity::local_motion`](./screen.md#entity-local_motion) |
| 遷移モーション | [`TransitionalMotionShader<A>`](#transitionalmotionshader) | [`TransitionalMotion`](#transitionalmotion) | [`transitional_motion`](#transitional_motion-fn) | [`Entity::travel`](./screen.md#entity-travel) |

コンストラクターはシェーダーの引数型 `A` の引数を受け取ります。そのため、シェーダーに別の型の引数を渡すことも、あるロールのシェーダーを別のロールの場所で使うこともできません。効果値はフィールドを隠しているので、作る方法はコンストラクターだけです。

効果値は `Eq` です。同じシェーダーファイルを等しい引数で使う 2 つの効果は等しくなります。効果値は[論理画面](./screen.md#the-logical-screen)の一部なので、フィルターの引数だけが違う 2 つの画面は別の画面です。シェーダー型と効果値は有限型ではありません。

`std` は既製のシェーダーを [`std::shaders`](./shaders.md) に同梱しています。各ロールのシェーダーの書き方、引数のレイアウト、シェーダーが受け取る時刻は [シェーダーの書き方](./guides/shaders.md) で説明します。このモジュールのアイテムはプレリュードにありません。`use std::screen::shader::…` で取り込んでください。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`ShaderArg`](#shaderarg)、[`Shader`](#shader) | ビルトイントレイト | 引数の型、シェーダーの型 |
| [`ColorShader<A>`](#colorshader) … [`TransitionalMotionShader<A>`](#transitionalmotionshader) | ビルトイン構造体 | 1 つのロールのシェーダーファイル |
| [`ShaderArgs`](#shaderargs) | ビルトイン構造体 | 型を消した引数 |
| [`Filter`](#filter)、`filter` | 構造体 + コンストラクター | 色フィルター |
| [`Mix`](#mix)、`mix` | 構造体 + コンストラクター | 合成の方法 |
| [`Mask`](#mask)、`mask` | 構造体 + コンストラクター | アルファマスク |
| [`Paint`](#paint)、`paint` | 構造体 + コンストラクター | 生成された画像内容 |
| [`LocalMotionBehavior`](#localmotionbehavior) | 列挙型 | ローカルモーションの時刻の進み方 |
| [`LocalMotion`](#localmotion)、`local_motion` | 構造体 + コンストラクター | エンティティのポーズの周りのモーション |
| [`TransitionalMotion`](#transitionalmotion)、`transitional_motion` | 構造体 + コンストラクター | あるポーズから別のポーズへの経路 |

## トレイト：`ShaderArg` {#shaderarg}

```kis
#[builtin] pub trait ShaderArg {}
```

シェーダーが引数として受け取れる型です。ユニット型 `()` と、[`#[shader_arg]`](../reference/special/attributes.md#shader_arg) を付けたすべての構造体です。実装するのはコンパイラで、ゲームは実装できません。`#[shader_arg]` 構造体は、[引数](./guides/shaders.md#arguments) に挙げた型の名前付きフィールドを持ちます。

## トレイト：`Shader` {#shader}

```kis
#[builtin] pub trait Shader {}
```

以下の 6 つのシェーダーアセット型です。

## 構造体：`ColorShader<A>` {#colorshader}

```kis
#[builtin] pub struct ColorShader<A>;
impl<A> Asset for ColorShader<A> {}
impl<A> Eq for ColorShader<A> {}
impl<A> Shader for ColorShader<A> {}
```

下に描かれたものの色を変える、引数の型が `A` の WGSL ファイルです。[`filter`](#filter-fn) で使います。`asset("….wgsl")` で宣言します。

## 構造体：`MixShader<A>` {#mixshader}

```kis
#[builtin] pub struct MixShader<A>;
impl<A> Asset for MixShader<A> {}
impl<A> Eq for MixShader<A> {}
impl<A> Shader for MixShader<A> {}
```

エンティティを下にあるものへ合成する WGSL ファイルです。[`mix`](#mix-fn) で使います。

## 構造体：`TextureShader<A>` {#textureshader}

```kis
#[builtin] pub struct TextureShader<A>;
impl<A> Asset for TextureShader<A> {}
impl<A> Eq for TextureShader<A> {}
impl<A> Shader for TextureShader<A> {}
```

画像の内容を生成する WGSL ファイルです。[`paint`](#paint-fn) で使います。

## 構造体：`MaskShader<A>` {#maskshader}

```kis
#[builtin] pub struct MaskShader<A>;
impl<A> Asset for MaskShader<A> {}
impl<A> Eq for MaskShader<A> {}
impl<A> Shader for MaskShader<A> {}
```

画素ごとのアルファ値を計算する WGSL ファイルです。[`mask`](#mask-fn) で使います。

## 構造体：`LocalMotionShader<A>` {#localmotionshader}

```kis
#[builtin] pub struct LocalMotionShader<A>;
impl<A> Asset for LocalMotionShader<A> {}
impl<A> Eq for LocalMotionShader<A> {}
impl<A> Shader for LocalMotionShader<A> {}
```

エンティティをそのポーズの周りで動かす WGSL ファイルです。[`local_motion`](#local_motion-fn) で使います。

## 構造体：`TransitionalMotionShader<A>` {#transitionalmotionshader}

```kis
#[builtin] pub struct TransitionalMotionShader<A>;
impl<A> Asset for TransitionalMotionShader<A> {}
impl<A> Eq for TransitionalMotionShader<A> {}
impl<A> Shader for TransitionalMotionShader<A> {}
```

エンティティをあるポーズから別のポーズへ動かす WGSL ファイルです。[`transitional_motion`](#transitional_motion-fn) で使います。

## 構造体：`ShaderArgs` {#shaderargs}

```kis
#[builtin] pub struct ShaderArgs;
impl Eq for ShaderArgs {}
```

効果値が持つ、型を消したシェーダー引数です。値は持っている引数で比較されます。ゲームが直接作ることはありません。

## 構造体：`Filter` {#filter}

```kis
pub struct Filter {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

色フィルター、つまり色シェーダーとその引数です。フィルターは覆うものの色を変えます。画面フィルターなら `Background`、`Stage`、`Front` プレーン、エンティティのフィルターならそのエンティティです。

### `filter(shader, args)` {#filter-fn}

```kis
pub const fn filter<A: ShaderArg>(shader: ColorShader<A>, args: A) -> Filter
```

- `shader` `ColorShader<A>` — シェーダー。
- `args` `A` — その引数。
- 戻り値：`Filter` — `args` を与えたフィルター `shader`。
- 種別：`const fn` · ビルトイン：いいえ。

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

エンティティを下にあるものへ合成する方法、つまりミックスシェーダーとその引数です。シェーダーはエンティティと背景を見て合成後の画素を返すので、ブレンドモードやすりガラスのような効果はミックスです。

### `mix(shader, args)` {#mix-fn}

```kis
pub const fn mix<A: ShaderArg>(shader: MixShader<A>, args: A) -> Mix
```

- 戻り値：`Mix` — `args` を与えたミックス `shader`。
- 種別：`const fn` · ビルトイン：いいえ。

## 構造体：`Mask` {#mask}

```kis
pub struct Mask {
    shader: ShaderAsset,
    args: ShaderArgs,
}
```

アルファマスク、つまりマスクシェーダーとその引数です。シェーダーは矩形の各画素に 0.0 から 1.0 の値を返します。マスクは 3 か所で使います。

- [`Entity::mask`](./screen.md#entity-mask)：エンティティの矩形の上で、エンティティのアルファにマスクが乗算されます。
- [`Paint::masked`](#paint-masked)：ペイントの上で、ペイントのアルファにマスクが乗算されます。
- [`anim::masked`](./screen-anim.md#transition-constructors)：移動するしきい値がマスクの値に達した画素から、トランジションが切り替わります。

### `mask(shader, args)` {#mask-fn}

```kis
pub const fn mask<A: ShaderArg>(shader: MaskShader<A>, args: A) -> Mask
```

- 戻り値：`Mask` — `args` を与えたマスク `shader`。
- 種別：`const fn` · ビルトイン：いいえ。

## 構造体：`Paint` {#paint}

```kis
pub struct Paint {
    texture: ShaderAsset,
    args: ShaderArgs,
    size: Size,
    mask: Option<Mask>,
}
```

`size` ピクセルの生成された画像内容、つまりテクスチャーシェーダーとその引数で、マスクを付けることもできます。[`ImageLayer`](./screen.md#imagelayer) は `ImageLayer::paint` で、画像の代わりにペイントを表示します。ペイントは `size` ピクセルの画像と同じように配置、サイズ決定、反転され、トランジションで差し替えられます。グラデーション、光の玉、天候はペイントです。

### `paint(shader, args, size)` {#paint-fn}

```kis
pub const fn paint<A: ShaderArg>(shader: TextureShader<A>, args: A, size: Size) -> Paint
```

- `size` `Size` — 内容のサイズ（ピクセル）。
- 戻り値：`Paint` — `shader` が `args` で描く `size` ピクセルの内容。マスクなし。
- 種別：`const fn` · ビルトイン：いいえ。

### `paint.masked(mask)` {#paint-masked}

```kis
pub const fn masked(mut self, mask: Mask) -> Paint
```

- 戻り値：ペイントの矩形の上で計算した `mask` をアルファに乗算した、このペイント。2 回目の `masked` は最初のマスクを置き換えます。
- 種別：`const fn` · ビルトイン：いいえ。

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

ローカルモーションの時刻の進み方です。ローカルモーションのシェーダーは秒単位の時刻 `t` を受け取ります。振る舞いは、モーション開始からの時間をシェーダーに渡す前に変換します。

| バリアント | シェーダーが受け取る `t` |
| ------- | ------------------- |
| `Periodic { after, period }` | 0 から `after` まで 1 回進み、その後の `period` を繰り返す：`after + ((time − after) mod period)`。`period` が 0 なら `after` で止まる |
| `Stall { after }` | `after` まで進み、その後は止まる：`min(time, after)` |

`Periodic { after: ZERO, period }` は単純なループで、0 でない `after` はループの前に導入部を再生します。`Stall` は、減衰する揺れのように落ち着くモーションに向いています。

## 構造体：`LocalMotion` {#localmotion}

```kis
pub struct LocalMotion {
    shader: ShaderAsset,
    args: ShaderArgs,
    behavior: LocalMotionBehavior,
}
```

エンティティのポーズの周りのモーション、つまりローカルモーションシェーダーとその引数、時刻の規則です。シェーダーはそのフレームでのエンティティのポーズ（トゥイーンと [`Motion`](./screen-anim.md#motion) を反映したもの）を受け取り、描くポーズを返します。変わるのはエンティティの描き方だけで、論理的な位置、拡大率、回転はそのままです。

### `local_motion(shader, args, behavior)` {#local_motion-fn}

```kis
pub const fn local_motion<A: ShaderArg>(shader: LocalMotionShader<A>, args: A, behavior: LocalMotionBehavior) -> LocalMotion
```

- `behavior` `LocalMotionBehavior` — モーションの時刻の進み方。
- 戻り値：`LocalMotion` — `args` を与え、時刻を `behavior` で変換するローカルモーション `shader`。
- 種別：`const fn` · ビルトイン：いいえ。

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

あるポーズから別のポーズへの経路、つまり遷移モーションシェーダーとその引数、タイミングです。[`Entity::travel`](./screen.md#entity-travel) がエンティティをこれに沿って動かします。シェーダーは開始ポーズ、終了ポーズ、イージング適用後の 0 から 1 の進行度、時刻を受け取り、描くポーズを返します。長さが過ぎると、シェーダーが何を返したかにかかわらず、エンティティは終了ポーズにあります。

### `transitional_motion(shader, args, duration)` {#transitional_motion-fn}

```kis
pub const fn transitional_motion<A: ShaderArg>(shader: TransitionalMotionShader<A>, args: A, duration: Duration) -> TransitionalMotion
```

- `duration` `Duration` — トラベルにかかる時間。
- 戻り値：`TransitionalMotion` — `args` を与え、`duration` かけて進むモーション `shader`。イージングは `Linear`、遅延なし。
- 種別：`const fn` · ビルトイン：いいえ。

### `motion.ease(easing)`、`motion.delay(delay)` {#transitionalmotion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> TransitionalMotion
pub const fn delay(mut self, delay: Duration) -> TransitionalMotion
```

`ease` は進行度に `easing` をかけたこのモーションを返し、`delay` はカーソルの `delay` 後に始まるこのモーションを返します。イージングはトゥイーンと同じく検査され、不正な `CubicBezier` や `Steps(0)` は定数評価エラーです。種別：`const fn` · ビルトイン：いいえ。

```kis
use std::screen::{degrees, pose};
use std::screen::anim::Easing;
use std::shaders::motion::arc;

screen.entity(BIRD).travel(pose(point(900, 400), 1.0, degrees(0.0)), arc(200.0, ms(1200)).ease(Easing::OutQuad));
```
