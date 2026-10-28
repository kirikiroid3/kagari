# `std::shaders::texture` {#stdshaderstexture}

> **安定性:** 暫定 — 生成された画像内容です。単色、グラデーション、ガウスの光点、雨、雪を含みます。

[`ImageLayer::paint`](./screen.md#imagelayer) のためのテクスチャーシェーダーです。どの関数も `size` ピクセルの [`Paint`](./screen-shader.md#paint) を返します。ペイントは画像レイヤーが画像の代わりに表示し、画像と同じようにトランジションで差し替えられます。ペイントには [`Paint::masked`](./screen-shader.md#paint-masked) でマスクを付けられます。ペイント内の位置は、左上の角からのピクセル数です。

```kis
use std::screen::{CENTER, degrees, rgba, size};
use std::shaders::texture::{GaussianSpot, gaussian_spots, radial_gradient, snow};

screen.background().paint(radial_gradient(CENTER, hex(0xFFE8C0), hex(0x402010), size(1920, 1080))).apply_animation(fade(ms(500)));
screen.entity(WEATHER).set_plane(Plane::Front);
screen.entity(WEATHER).image().paint(snow(WHITE, 40, 60.0, 3.0, size(1920, 1080))).apply_animation(fade(ms(1000)));
screen.entity(WEATHER).opacity().change(1.0).apply_animation(tween(ms(1000)));
screen.entity(GLOW).image().paint(gaussian_spots([
    GaussianSpot { position: point(120, 90), color: rgba(255, 200, 120, 200), radius: 40.0 },
    GaussianSpot { position: point(280, 200), color: rgba(120, 200, 255, 160), radius: 60.0 },
], size(400, 300))).apply_animation(fade(ms(300)));
```

## 概要 {#overview}

| 関数 | 引数 | 内容 |
| -------- | --------- | ------- |
| [`solid(color, size)`](#solid) | [`Solid`](#solid) | 単色 |
| [`linear_gradient(angle, from, to, size)`](#linear_gradient) | [`LinearGradient`](#linear_gradient) | 線形グラデーション |
| [`radial_gradient(center, inner, outer, size)`](#radial_gradient) | [`RadialGradient`](#radial_gradient) | 放射グラデーション |
| [`gaussian_spots(spots, size)`](#gaussian_spots) | [`GaussianSpot`](#gaussian_spots)、[`GaussianSpots<N>`](#gaussian_spots) | 柔らかく丸い光点 |
| [`rain(color, density, speed, angle, size)`](#rain) | [`Rain`](#rain) | 降る雨の筋 |
| [`snow(color, density, speed, radius, size)`](#snow) | [`Snow`](#snow) | 降る雪片 |

以下の各節に、引数の構造体、シェーダー定数、関数を載せます。関数は `const fn` で、ビルトインではありません。

## 単色 {#solid}

```kis
#[shader_arg]
pub struct Solid {
    pub color: Color,
}
pub const SOLID: TextureShader<Solid> = asset("shaders/texture/solid.wgsl");
pub const fn solid(color: Color, size: Size) -> Paint
```

`color` で塗った `size` ピクセルです。マスクを付ければ形を描けます。`solid(WHITE, s).masked(rounded_rect(16.0, 1.0))` は角の丸い白いパネルです。

## 線形グラデーション {#linear_gradient}

```kis
#[shader_arg]
pub struct LinearGradient {
    pub angle: Angle,
    pub from: Color,
    pub to: Color,
}
pub const LINEAR_GRADIENT: TextureShader<LinearGradient> = asset(
    "shaders/texture/linear_gradient.wgsl",
);
pub const fn linear_gradient(angle: Angle, from: Color, to: Color, size: Size) -> Paint
```

`angle`（0° は左から右、90° は上から下）に沿って、始まりの辺の `from` から終わりの辺の `to` へ変わる `size` ピクセルです。ランプは [`mask::linear`](./shaders-mask.md#linear) と同じです。色はリニア光で混ぜます。

## 放射グラデーション {#radial_gradient}

```kis
#[shader_arg]
pub struct RadialGradient {
    pub center: Anchor,
    pub inner: Color,
    pub outer: Color,
}
pub const RADIAL_GRADIENT: TextureShader<RadialGradient> = asset(
    "shaders/texture/radial_gradient.wgsl",
);
pub const fn radial_gradient(center: Anchor, inner: Color, outer: Color, size: Size) -> Paint
```

`center`（ペイントに対する割合）の `inner` から最も遠い角の `outer` へ変わる `size` ピクセルです。ランプは [`mask::radial`](./shaders-mask.md#radial) と同じです。

## ガウスの光点 {#gaussian_spots}

```kis
#[shader_arg]
pub struct GaussianSpot {
    pub position: Point,
    pub color: Color,
    pub radius: Decimal,
}
#[shader_arg]
pub struct GaussianSpots<const N: usize> {
    pub count: u32,
    pub spots: [GaussianSpot; N],
}
pub const fn gaussian_spots<const N: usize>(spots: [GaussianSpot; N], size: Size) -> Paint
```

`spots` を足し合わせた `size` ピクセルです。光点は `position`（ペイントの左上の角からのピクセル数）にある柔らかく丸い光で、中心の色が `color`、標準偏差 `radius` ピクセルのガウス関数で減衰します。`radius` が 0.0 以下なら何も描きません。足し合わせたアルファが 1.0 を超える場所では、アルファが 1.0 になるよう合計を縮小します。

- 描かれる光点は最大 64 個で、65 個目以降は無視されます。
- `GaussianSpots<N>` が引数の型で、`count` は `N` です。シェーダーの型が `N` に依存するので、公開の定数はありません。
- 同じ `N` の 2 つの `gaussian_spots` ペイントの間をトゥイーンすると、すべての光点が動きます。

## 雨 {#rain}

```kis
#[shader_arg]
pub struct Rain {
    pub color: Color,
    pub density: u32,
    pub speed: Decimal,
    pub angle: Angle,
    pub length: Decimal,
}
pub const RAIN: TextureShader<Rain> = asset("shaders/texture/rain.wgsl");
pub const fn rain(color: Color, density: u32, speed: Decimal, angle: Angle, size: Size) -> Paint
```

雨の `size` ピクセルです。ペイントを幅 8 ピクセルの列に分け、そのうち `density` パーセントの列に雨粒があります。雨粒は `color` の筋で、真下から `angle` だけ傾いて毎秒 `speed` ピクセルで落ちます。`rain` は長さ 24 ピクセルの筋を描きます。別の `length` にするには、[`paint`](./screen-shader.md#paint-fn) と `RAIN` で `Rain` を組み立ててください。

## 雪 {#snow}

```kis
#[shader_arg]
pub struct Snow {
    pub color: Color,
    pub density: u32,
    pub speed: Decimal,
    pub radius: Decimal,
}
pub const SNOW: TextureShader<Snow> = asset("shaders/texture/snow.wgsl");
pub const fn snow(
    color: Color,
    density: u32,
    speed: Decimal,
    radius: Decimal,
    size: Size,
) -> Paint
```

奥行きの異なる 3 層で揺れながら降る雪片の `size` ピクセルです。48 ピクセルの格子のマスのうち `density` パーセントに雪片があります。最も手前の層の雪片は半径 `radius` ピクセルで毎秒 `speed` ピクセルで落ち、奥の 2 層はより小さく、より遅くなります。
