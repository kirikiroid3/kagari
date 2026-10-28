# `std::shaders` {#stdshaders}

> **安定性:** 暫定 — `std` のシェーダーライブラリです。色フィルター、ミックスモード、手続き的なマスク、テクスチャー、モーションを含みます。

[`std::screen::shader`](./screen-shader.md) のすべてのロールについて、既製のシェーダーを効果値として提供します。ライブラリの各シェーダーは 3 つのアイテムからなります。

- 引数の `#[shader_arg]` 構造体（引数の型が `()` の場合はありません）。
- シェーダーアセットそのものである定数。たとえば `BLUR: ColorShader<Blur>` です。
- 効果値を作る `const fn`。たとえば `blur(radius) -> Filter` です。

普段は関数を使います。定数は、引数を自分で組み立てるコードのためにあります。たとえば `filter(BLUR, Blur { radius: 4.0 })` は `blur(4.0)` と同じ値です。同じシェーダーの 2 つの値の間をトゥイーンすると引数が動く（[シェーダー効果](./screen.md#shader-effects) を参照）ので、`blur(0.0)` から `blur(6.0)` へのトゥイーンは少しずつぼかしていきます。

WGSL ファイルは `std` に同梱されています。`kagari build` はゲームが使うものを `std.kiarchive` に詰めるので、ゲームがコピーする必要はありません。`std::shaders` のアイテムはプレリュードにありません。

| モジュール | ロール | 内容 |
| ------ | ---- | -------- |
| [`std::shaders::color`](./shaders-color.md) | 色（`Filter`） | CSS のフィルター関数、まとめたフィルター、ガウスぼかし |
| [`std::shaders::mix`](./shaders-mix.md) | ミックス（`Mix`） | W3C のブレンドモード、加算合成、すりガラス |
| [`std::shaders::mask`](./shaders-mask.md) | マスク（`Mask`） | 線形、放射、ひし形、時計、縞、市松、ノイズ、ガウス、角丸矩形のランプ |
| [`std::shaders::texture`](./shaders-texture.md) | テクスチャー（`Paint`） | 単色、グラデーション、ガウスの光点、雨、雪 |
| [`std::shaders::motion`](./shaders-motion.md) | ローカルモーションと遷移モーション | ループ（bob、sway、pulse、spin、orbit）、減衰する揺れ、ODE で動く移動、経路（arc、bounce、spiral） |

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
