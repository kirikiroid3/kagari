---
title: "std::shaders::motion"
---

> **安定性:** 暫定 — モーションシェーダー集です。エンティティの姿勢を基準にした繰り返しの動きや収束する動き、2 つの姿勢の間を結ぶ経路があります。

モーションは 2 種類に分かれます。

- **ローカルモーション**は [`Entity::local_motion`](./screen.md#entity-local_motion) で使い、[`LocalMotion`](./screen-shader.md#localmotion) を返します。エンティティを姿勢の周りで動かしますが、変わるのは見た目だけです。繰り返しの動きは終わりなく続き、収束する動きは最後の位置で止まります。
- **遷移モーション**は [`Entity::travel`](./screen.md#entity-travel) で使い、[`TransitionalMotion`](./screen-shader.md#transitionalmotion) を返します。エンティティの現在の姿勢から `travel` に渡した姿勢まで、指定の時間をかけてたどる経路です。`.ease(…)` と `.delay(…)` で調整できます。

距離の単位はピクセルで、角度は時計回りを正とします。関数はどれも `const fn` で、組み込みではありません。

```kis
use std::screen::{degrees, offset, pose};
use std::shaders::motion::{bounce, decaying_quake, sine_integral, sway};

screen.entity(LANTERN).local_motion().change(Some(sway(degrees(6.0), ms(2400)))).apply_animation(instant());
screen.entity(HANAKO).travel(pose(point(1200, 1080), 1.0, degrees(0.0)), bounce(40.0, ms(900)));
screen.entity(SIGN).local_motion().change(Some(decaying_quake(12, 4, ms(800)))).apply_animation(instant());
screen.entity(CARD).local_motion().change(Some(sine_integral(offset(0, -60), 12.0, ms(2000)))).apply_animation(instant());
```

## 概要 {#overview}

| 関数 | 種類 | 動き |
| -------- | ---- | ------ |
| [`bob(height, period)`](#bob) | ローカル、繰り返し | 浮き上がって戻る |
| [`sway(angle, period)`](#sway) | ローカル、繰り返し | アンカーを支点に左右に揺れる |
| [`pulse(scale, period)`](#pulse) | ローカル、繰り返し | 大きくなったり小さくなったりする |
| [`spin(period)`](#spin) | ローカル、繰り返し | アンカーを中心に回転する |
| [`orbit(radius, period)`](#orbit) | ローカル、繰り返し | 姿勢の位置を通る円を描いて回る |
| [`decaying_quake(horizontal, vertical, duration)`](#decaying_quake) | ローカル、収束 | だんだん収まる揺れ |
| [`sine_integral(amplitude, rate, settle)`](#sine_integral) | ローカル、収束 | 行き過ぎながら目標に近づく |
| [`arc(height, duration)`](#arc) | 遷移 | 放物線を描いて移動する |
| [`bounce(overshoot, duration)`](#bounce) | 遷移 | 行き過ぎて弾んでから到着する |
| [`spiral(turns, duration)`](#spiral) | 遷移 | 渦を巻いて終点に入る |

繰り返しの動きは、振る舞いとして `Periodic { after: ZERO, period }` を使います。すぐに始まり、`period` ごとに繰り返します。補間で引数を変えても周期の時刻は途切れないので、たとえば `bob(0.0, p)` から `bob(20.0, p)` へ、動きを飛ばさずに少しずつ大きくできます。

## 上下の揺れ {#bob}

```kis
#[shader_arg]
pub struct Bob {
    pub height: Decimal,
    pub period: Duration,
}
pub const BOB: LocalMotionShader<Bob> = asset("shaders/local_motion/bob.wgsl");
pub const fn bob(height: Decimal, period: Duration) -> LocalMotion
```

`height` ピクセル浮き上がってから戻る動きを、`period` ごとに 1 回、終わりなく繰り返します。動きには緩急が付きます（`y − height·(0.5 − 0.5·cos(2π·t/period))`）。

## 左右の揺れ {#sway}

```kis
#[shader_arg]
pub struct Sway {
    pub angle: Angle,
    pub period: Duration,
}
pub const SWAY: LocalMotionShader<Sway> = asset("shaders/local_motion/sway.wgsl");
pub const fn sway(angle: Angle, period: Duration) -> LocalMotion
```

アンカーを支点に、左右それぞれ `angle` まで傾く揺れを、`period` ごとに 1 往復、終わりなく繰り返します（`rotation + angle·sin(2π·t/period)`）。

## 脈動 {#pulse}

```kis
#[shader_arg]
pub struct Pulse {
    pub scale: Decimal,
    pub period: Duration,
}
pub const PULSE: LocalMotionShader<Pulse> = asset("shaders/local_motion/pulse.wgsl");
pub const fn pulse(scale: Decimal, period: Duration) -> LocalMotion
```

アンカーを中心に `scale` 倍まで大きくなってから元に戻る動きを、`period` ごとに 1 回、終わりなく繰り返します。

## 回転 {#spin}

```kis
#[shader_arg]
pub struct Spin {
    pub period: Duration,
}
pub const SPIN: LocalMotionShader<Spin> = asset("shaders/local_motion/spin.wgsl");
pub const fn spin(period: Duration) -> LocalMotion
```

アンカーを中心に、`period` ごとに 1 回転の速さで、時計回りに回り続けます。

## 周回 {#orbit}

```kis
#[shader_arg]
pub struct Orbit {
    pub radius: Decimal,
    pub period: Duration,
}
pub const ORBIT: LocalMotionShader<Orbit> = asset("shaders/local_motion/orbit.wgsl");
pub const fn orbit(radius: Decimal, period: Duration) -> LocalMotion
```

姿勢の位置を通る半径 `radius` ピクセルの円の上を、`period` ごとに 1 周、時計回りに回り続けます。エンティティは姿勢の位置から動き始め、1 周するたびにそこへ戻ってきます。

## 減衰する揺れ {#decaying_quake}

```kis
#[shader_arg]
pub struct DecayingQuake {
    pub horizontal: i32,
    pub vertical: i32,
    pub duration: Duration,
}
pub const DECAYING_QUAKE: LocalMotionShader<DecayingQuake> = asset(
    "shaders/local_motion/decaying_quake.wgsl",
);
pub const fn decaying_quake(horizontal: i32, vertical: i32, duration: Duration) -> LocalMotion
```

だんだん収まっていく揺れです（TyranoScript の `[quake2]` に相当）。横に `horizontal` ピクセル、縦に `vertical` ピクセルの幅で、60 fps の 1 フレームごとに向きを反転させながら揺れます。振幅は `duration` をかけて 2 次関数的に 0 まで小さくなり、その後は姿勢の位置で静止します。振る舞いは `Stall { after: duration }` です。

## 正弦積分 {#sine_integral}

```kis
#[shader_arg]
pub struct SineIntegral {
    pub amplitude: Offset,
    pub rate: Decimal,
}
pub const SINE_INTEGRAL: LocalMotionShader<SineIntegral> = asset(
    "shaders/local_motion/sine_integral.wgsl",
);
pub const fn sine_integral(amplitude: Offset, rate: Decimal, settle: Duration) -> LocalMotion
```

行き過ぎながら目標に近づく動きです。微分方程式で定義したモーションの例でもあります。エンティティの移動量は `amplitude · (2/π) · Si(rate · t)` です。これは `x = 0` を初期値とする `dx/dt = amplitude · (2/π) · sin(rate · t) / t` の解で、いったん `amplitude` を行き過ぎ、行きつ戻りつしながらそこに落ち着きます。`rate` の単位はラジアン毎秒です。振る舞いは `Stall { after: settle }` なので、`settle` の時点の位置で動きが止まります。

`Si` は初等関数では表せません。シェーダーはフレームをまたいで値を保持できないため、毎フレーム 0 から `t` までを数値積分して求めます。モーションが変えるのはエンティティの描画位置だけで、論理上の位置は元のままです。動きが落ち着いた後は、画面上で `amplitude` だけずれた位置に表示されます。

## 放物線 {#arc}

```kis
#[shader_arg]
pub struct Arc {
    pub height: Decimal,
}
pub const ARC: TransitionalMotionShader<Arc> = asset("shaders/transitional_motion/arc.wgsl");
pub const fn arc(height: Decimal, duration: Duration) -> TransitionalMotion
```

放物線に沿って、`duration` をかけて移動します。放物線の頂点は道のりの中間にあり、始点と終点を結ぶ直線より `height` ピクセル上です。進み具合を `p` とすると、位置は `mix(start, end, p) − (0, height · 4p(1 − p))` です。拡大率と回転は線形に変わります。

## 弾み {#bounce}

```kis
#[shader_arg]
pub struct Bounce {
    pub overshoot: Decimal,
}
pub const BOUNCE: TransitionalMotionShader<Bounce> = asset(
    "shaders/transitional_motion/bounce.wgsl",
);
pub const fn bounce(overshoot: Decimal, duration: Duration) -> TransitionalMotion
```

行き過ぎて弾んでから到着する動きです（animate.css の `bounceIn*` に相当）。まっすぐ進んで終点を `overshoot` ピクセル行き過ぎ、それより小さい幅で反対側へ戻り、さらに小さい幅でもう一度行き過ぎてから止まります。ここまでを `duration` で行います。拡大率と回転は線形に変わります。始点と終点の位置が同じ場合、位置は弾みません。

## 渦巻き {#spiral}

```kis
#[shader_arg]
pub struct Spiral {
    pub turns: Decimal,
}
pub const SPIRAL: TransitionalMotionShader<Spiral> = asset(
    "shaders/transitional_motion/spiral.wgsl",
);
pub const fn spiral(turns: Decimal, duration: Duration) -> TransitionalMotion
```

渦を巻きながら終点に入ります。`duration` をかけて、終点の周りを時計回りに `turns` 回まわりつつ、終点までの距離を線形に縮めます。拡大率と回転は線形に変わります。
