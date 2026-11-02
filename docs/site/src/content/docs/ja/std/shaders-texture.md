---
title: "std::shaders::texture"
---

> **安定性:** 暫定 — 画像の内容を生成するシェーダー集です。単色、グラデーション、ガウス状の光の玉、雨、雪があります。

[`ImageLayer::paint`](./screen.md#imagelayer) で使うテクスチャーシェーダーです。どの関数も `size` ピクセルの[`Paint`](./screen-shader.md#paint)（塗り）を返します。画像レイヤーは画像の代わりにこの塗りを表示し、画像と同じようにトランジションで差し替えます。塗りには [`Paint::masked`](./screen-shader.md#paint-masked) でマスクをかけられます。塗りの中の位置は、左上の角からのピクセル数で表します。

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
| [`radial_gradient(center, inner, outer, size)`](#radial_gradient) | [`RadialGradient`](#radial_gradient) | 放射状グラデーション |
| [`gaussian_spots(spots, size)`](#gaussian_spots) | [`GaussianSpot`](#gaussian_spots)、[`GaussianSpots<N>`](#gaussian_spots) | ぼんやりした丸い光の玉 |
| [`rain(color, density, speed, angle, size)`](#rain) | [`Rain`](#rain) | 降りしきる雨の筋 |
| [`snow(color, density, speed, radius, size)`](#snow) | [`Snow`](#snow) | 舞い落ちる雪 |

以下の各節では、引数の構造体、シェーダーの定数、関数をまとめて示します。関数はどれも `const fn` で、組み込みではありません。

## 単色 {#solid}

```kis
#[shader_arg]
pub struct Solid {
    pub color: Color,
}
pub const SOLID: TextureShader<Solid> = asset("shaders/texture/solid.wgsl");
pub const fn solid(color: Color, size: Size) -> Paint
```

`size` ピクセルを `color` 一色で塗ります。マスクと組み合わせると図形を描けます。たとえば `solid(WHITE, s).masked(rounded_rect(16.0, 1.0))` は角の丸い白いパネルになります。

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

`size` ピクセルの線形グラデーションです。`angle` の方向（0° で左から右、90° で上から下）に沿って、始点側の辺の `from` から終点側の辺の `to` へ色が変わります。変化のしかたは [`mask::linear`](./shaders-mask.md#linear) と同じです。色はリニア色空間で混ぜます。

## 放射状グラデーション {#radial_gradient}

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

`size` ピクセルの放射状グラデーションです。`center`（塗りの大きさに対する割合）で `inner`、そこから最も遠い角で `outer` になるように色が変わります。変化のしかたは [`mask::radial`](./shaders-mask.md#radial) と同じです。

## ガウス状の光の玉 {#gaussian_spots}

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

`spots` の光の玉をすべて足し合わせた、`size` ピクセルの塗りです。光の玉はそれぞれ `position`（塗りの左上の角からのピクセル数）を中心とするぼんやりした丸い光です。中心の色が `color` で、標準偏差 `radius` ピクセルのガウス分布に従って周囲へ薄くなります。`radius` が 0.0 以下の光の玉は何も描きません。足し合わせたアルファが 1.0 を超える場所では、アルファがちょうど 1.0 になるよう合計を縮めます。

- 描けるのは 64 個までです。65 個目以降は無視します。
- 引数の型は `GaussianSpots<N>` で、`count` には `N` が入ります。シェーダーの型が `N` によって変わるため、このシェーダーには公開の定数がありません。
- `N` が同じ 2 つの `gaussian_spots` の塗りの間を補間すると、すべての光の玉が動きます。

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

`size` ピクセルの雨です。塗りを幅 8 ピクセルの列に分け、そのうち `density` パーセントの列に雨粒を置きます。雨粒は `color` の筋で、真下から `angle` だけ傾いた向きに、毎秒 `speed` ピクセルの速さで落ちます。`rain` が描く筋の長さは 24 ピクセルです。長さ（`length`）を変えたい場合は、[`paint`](./screen-shader.md#paint-fn) と `RAIN` を使って `Rain` を自分で組み立ててください。

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

`size` ピクセルの雪です。雪は奥行きの違う 3 つの層に分かれ、揺れながら降ります。48 ピクセル間隔の格子のマスのうち、`density` パーセントに雪片を置きます。いちばん手前の層では雪片の半径が `radius` ピクセル、落ちる速さが毎秒 `speed` ピクセルです。奥の 2 層では、雪片はそれより小さく、落ちるのも遅くなります。
