---
title: "std::shaders"
---

> **Stability:** Tentative — the std shader library: colour filters, mix modes, procedural masks,
> textures and motions.

Ready-made shaders for every role of [`std::screen::shader`](./screen-shader.md), as effect
values. Each shader of the library comes as three items:

- a `#[shader_arg]` struct of its arguments (none when the argument type is `()`);
- a constant, the shader asset itself, such as `BLUR: ColorShader<Blur>`;
- a `const fn` that builds the effect value, such as `blur(radius) -> Filter`.

The function is the usual way in. The constant is there for code that builds the arguments itself,
for example `filter(BLUR, Blur { radius: 4.0 })`, which is the same value as `blur(4.0)`. Tweening
between two values of the same shader moves the arguments (see
[Shader effects](./screen.md#shader-effects)), so `blur(0.0)` to `blur(6.0)` blurs gradually.

The WGSL files ship inside `std`. `kagari build` packs the ones a game uses into `std.kiarchive`;
a game never copies them. Nothing of `std::shaders` is in the prelude.

| Module | Role | Contents |
| ------ | ---- | -------- |
| [`std::shaders::color`](./shaders-color.md) | colour (`Filter`) | the CSS filter functions, a combined filter, a Gaussian blur |
| [`std::shaders::mix`](./shaders-mix.md) | mix (`Mix`) | the W3C blend modes, additive compositing, frosted glass |
| [`std::shaders::mask`](./shaders-mask.md) | mask (`Mask`) | linear, radial, diamond, clock, stripe, checker, noise, Gaussian and rounded-rectangle ramps |
| [`std::shaders::texture`](./shaders-texture.md) | texture (`Paint`) | solid colour, gradients, Gaussian spots, rain, snow |
| [`std::shaders::motion`](./shaders-motion.md) | local and transitional motion | loops (bob, sway, pulse, spin, orbit), a decaying quake, an ODE-driven move, and paths (arc, bounce, spiral) |

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
