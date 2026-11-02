---
title: "std::shaders"
---

> **安定性:** 暫定 — 標準ライブラリのシェーダー集です。色フィルター、合成モード、手続き的なマスク、テクスチャー、モーションがあります。

[`std::screen::shader`](./screen-shader.md) のすべての役割について、すぐに使えるシェーダーを効果値として用意しています。各シェーダーは次の 3 つのアイテムで構成されます。

- 引数をまとめた `#[shader_arg]` 構造体（引数の型が `()` のシェーダーにはありません）
- シェーダーアセットそのものを表す定数（例：`BLUR: ColorShader<Blur>`）
- 効果値を作る `const fn`（例：`blur(radius) -> Filter`）

ふだんは関数を使います。定数は、引数を自前で組み立てるコード向けです。たとえば `filter(BLUR, Blur { radius: 4.0 })` は `blur(4.0)` と同じ値になります。同じシェーダーの 2 つの値の間を補間すると、引数が連続的に変化します（[シェーダー効果](./screen.md#shader-effects)を参照）。そのため、`blur(0.0)` から `blur(6.0)` へ補間すると、だんだんぼけていきます。

WGSL ファイルは `std` に同梱されています。ゲームで使うものは `kagari build` が `std.kiarchive` にまとめるので、ゲーム側でコピーする必要はありません。`std::shaders` のアイテムはどれもプレリュードに含まれません。

| モジュール | 役割 | 内容 |
| ------ | ---- | -------- |
| [`std::shaders::color`](./shaders-color.md) | 色（`Filter`） | CSS のフィルター関数、それらをまとめたフィルター、ガウスぼかし |
| [`std::shaders::mix`](./shaders-mix.md) | 合成（`Mix`） | W3C のブレンドモード、加算合成、すりガラス |
| [`std::shaders::mask`](./shaders-mask.md) | マスク（`Mask`） | 線形、放射状、ひし形、時計回り、縞、市松、ノイズ、ガウス、角丸矩形のグラデーション |
| [`std::shaders::texture`](./shaders-texture.md) | テクスチャー（`Paint`） | 単色、グラデーション、ガウス状の光の玉、雨、雪 |
| [`std::shaders::motion`](./shaders-motion.md) | ローカルモーションと遷移モーション | 繰り返しの動き（bob、sway、pulse、spin、orbit）、減衰する揺れ、ODE で駆動する移動、経路に沿った移動（arc、bounce、spiral） |

```kis
use std::screen::CENTER;
use std::screen::anim::masked;
use std::shaders::{color, mask, mix, motion};

const fn memory(mut screen: Screen) -> Screen {
    screen.filter().change(Some(color::sepia(0.8))).apply_animation(tween(ms(800)));
    screen.entity(LIGHT).mix().change(Some(mix::screen())).apply_animation(instant());
    screen.entity(HANAKO).local_motion().change(Some(motion::bob(12.0, ms(2000)))).apply_animation(instant());
    screen.background().change(BG::ROOF).apply_animation(masked(mask::radial(CENTER), ms(1000)));
    screen
}
```
