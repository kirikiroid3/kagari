---
title: "std::shaders::motion"
---

> **安定性:** 暫定 — モーションシェーダーです。エンティティのポーズの周りのループや落ち着く動き、2 つのポーズの間の経路を含みます。

モーションには 2 種類あります。

- [`Entity::local_motion`](./screen.md#entity-local_motion) のための**ローカルモーション**は [`LocalMotion`](./screen-shader.md#localmotion) を返します。エンティティを描画の上でだけポーズの周りで動かします。ループはいつまでも繰り返し、落ち着く動きは終わりの位置で止まります。
- [`Entity::travel`](./screen.md#entity-travel) のための**遷移モーション**は [`TransitionalMotion`](./screen-shader.md#transitionalmotion) を返します。エンティティのポーズから `travel` に渡したポーズへ、一定の時間をかけて進む経路です。`.ease(…)` と `.delay(…)` で調整できます。

距離はピクセル単位で、角度は時計回りに回ります。どの関数も `const fn` で、ビルトインではありません。

```kis
use std::screen::{degrees, offset, pose};
use std::shaders::motion::{bounce, decaying_quake, sine_integral, sway};

screen.entity(LANTERN).local_motion().change(Some(sway(degrees(6.0), ms(2400)))).apply_animation(instant());
screen.entity(HANAKO).travel(pose(point(1200, 1080), 1.0, degrees(0.0)), bounce(40.0, ms(900)));
screen.entity(SIGN).local_motion().change(Some(decaying_quake(12, 4, ms(800)))).apply_animation(instant());
screen.entity(CARD).local_motion().change(Some(sine_integral(offset(0, -60), 12.0, ms(2000)))).apply_animation(instant());
```

## 概要 {#overview}

| 関数 | 種類 | モーション |
| -------- | ---- | ------ |
| [`bob(height, period)`](#bob) | ローカル、ループ | 上がって戻る |
| [`sway(angle, period)`](#sway) | ローカル、ループ | アンカーを中心に揺れる |
| [`pulse(scale, period)`](#pulse) | ローカル、ループ | 大きくなって小さくなる |
| [`spin(period)`](#spin) | ローカル、ループ | アンカーを中心に回る |
| [`orbit(radius, period)`](#orbit) | ローカル、ループ | ポーズを通る円を回る |
| [`decaying_quake(horizontal, vertical, duration)`](#decaying_quake) | ローカル、落ち着く | 次第に収まる揺れ |
| [`sine_integral(amplitude, rate, settle)`](#sine_integral) | ローカル、落ち着く | 行き過ぎながら近づく |
| [`arc(height, duration)`](#arc) | 遷移 | 放物線の弧 |
| [`bounce(overshoot, duration)`](#bounce) | 遷移 | 行き過ぎてから到着する |
| [`spiral(turns, duration)`](#spiral) | 遷移 | 終点へ向かう渦巻き |

ループは振る舞い `Periodic { after: ZERO, period }` を使います。すぐに始まり、`period` ごとに繰り返します。トゥイーンでループの引数を変えても時刻はそのまま続くので、bob を `bob(0.0, p)` から `bob(20.0, p)` へ跳ばずに大きくできます。

## Bob {#bob}

```kis
#[shader_arg]
pub struct Bob {
    pub height: Decimal,
    pub period: Duration,
}
pub const BOB: LocalMotionShader<Bob> = asset("shaders/local_motion/bob.wgsl");
pub const fn bob(height: Decimal, period: Duration) -> LocalMotion
```

`period` ごとに 1 回、`height` ピクセル上がって戻る動きを、緩急を付けていつまでも繰り返します（`y − height·(0.5 − 0.5·cos(2π·t/period))`）。

## Sway {#sway}

```kis
#[shader_arg]
pub struct Sway {
    pub angle: Angle,
    pub period: Duration,
}
pub const SWAY: LocalMotionShader<Sway> = asset("shaders/local_motion/sway.wgsl");
pub const fn sway(angle: Angle, period: Duration) -> LocalMotion
```

`period` ごとに 1 回、アンカーを中心に左右へ `angle` ずつ、いつまでも揺れます（`rotation + angle·sin(2π·t/period)`）。

## Pulse {#pulse}

```kis
#[shader_arg]
pub struct Pulse {
    pub scale: Decimal,
    pub period: Duration,
}
pub const PULSE: LocalMotionShader<Pulse> = asset("shaders/local_motion/pulse.wgsl");
pub const fn pulse(scale: Decimal, period: Duration) -> LocalMotion
```

`period` ごとに 1 回、アンカーを中心に `scale` 倍の大きさまで大きくなって戻る動きを、いつまでも繰り返します。

## Spin {#spin}

```kis
#[shader_arg]
pub struct Spin {
    pub period: Duration,
}
pub const SPIN: LocalMotionShader<Spin> = asset("shaders/local_motion/spin.wgsl");
pub const fn spin(period: Duration) -> LocalMotion
```

`period` ごとに 1 回、アンカーを中心に時計回りにいつまでも回ります。

## Orbit {#orbit}

```kis
#[shader_arg]
pub struct Orbit {
    pub radius: Decimal,
    pub period: Duration,
}
pub const ORBIT: LocalMotionShader<Orbit> = asset("shaders/local_motion/orbit.wgsl");
pub const fn orbit(radius: Decimal, period: Duration) -> LocalMotion
```

ポーズを通る半径 `radius` ピクセルの円の上を、`period` ごとに 1 周、時計回りにいつまでも回ります。エンティティはポーズから始まり、1 周ごとにポーズへ戻ります。

## Decaying quake {#decaying_quake}

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

次第に収まる揺れです（TyranoScript の `[quake2]`）。60 fps の各フレームで向きを交互に変えながら `horizontal` と `vertical` ピクセルだけ動き、振幅は `duration` の間に 2 次関数的に 0 まで小さくなります。その後はポーズで止まります。振る舞いは `Stall { after: duration }` です。

## Sine integral {#sine_integral}

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

行き過ぎながら近づく動きで、微分方程式で定義するモーションの例でもあります。エンティティは `amplitude · (2/π) · Si(rate · t)` だけ動きます。これは `x = 0` から始めた `dx/dt = amplitude · (2/π) · sin(rate · t) / t` の解で、`amplitude` を行き過ぎてから、振動しながらそこに落ち着きます。`rate` の単位はラジアン毎秒です。振る舞いは `Stall { after: settle }` なので、モーションは `settle` の時点の位置で止まります。

`Si` は閉じた形を持ちません。シェーダーはフレーム間で何も保持しないので、毎フレーム 0 から `t` まで数値積分します。モーションが変えるのはエンティティの描き方だけです。エンティティの論理的な位置は元のままで、落ち着いた後は画面上で `amplitude` だけずれて見えます。

## Arc {#arc}

```kis
#[shader_arg]
pub struct Arc {
    pub height: Decimal,
}
pub const ARC: TransitionalMotionShader<Arc> = asset("shaders/transitional_motion/arc.wgsl");
pub const fn arc(height: Decimal, duration: Duration) -> TransitionalMotion
```

中間点で直線より `height` ピクセル上に頂点がある放物線に沿って、`duration` をかけて動きます。進捗 `p` に対して `mix(start, end, p) − (0, height · 4p(1 − p))` です。拡大率と回転は線形に変わります。

## Bounce {#bounce}

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

行き過ぎてから到着する動きです（animate.css の `bounceIn*`）。`duration` をかけて、まっすぐ動いて終点を `overshoot` ピクセル行き過ぎ、それより少なく逆側へ戻り、さらに少なくもう一度行き過ぎてから落ち着きます。拡大率と回転は線形に変わります。始点と終点の位置が同じなら、位置は弾みません。

## Spiral {#spiral}

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

`duration` をかけて、終点の周りを時計回りに `turns` 回まわりながら、距離を線形に縮めて終点へ渦を巻いて入ります。拡大率と回転は線形に変わります。
