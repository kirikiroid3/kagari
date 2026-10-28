# Shaders

`core/kagari-shader` is the contract between Kagari shader files (`.wgsl`), the compiler, the
build tool, the engine and the renderer. It has no dependencies by default; the `validate` feature
pulls in `naga` (the same major version as wgpu's) for the build tool and the renderer.

## Roles

`ShaderRole` names what a file does. The std type `std::screen::shader::<Role>Shader<A>` of an
asset fixes its role; the same file used under two roles is composed and validated once per role.

| Role | Function the file defines | Stage |
|---|---|---|
| `Color` | `fn effect(uv: vec2<f32>) -> vec4<f32>` (design-target uv) | fragment |
| `Mix` | `fn effect(uv: vec2<f32>) -> vec4<f32>`, the final composited pixel | fragment |
| `Texture` | `fn effect(uv: vec2<f32>) -> vec4<f32>` (uv over the paint rect) | fragment |
| `Mask` | `fn effect(uv: vec2<f32>) -> f32` in 0..1 | fragment |
| `LocalMotion` | `fn effect(state: KgState, t: f32) -> KgState` | compute |
| `TransitionalMotion` | `fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState` | compute |

Colours are premultiplied linear light; times are seconds. WGSL reserves `from`, so motion
parameters cannot use that name.

## Preludes

`compose(role, source)` is `prelude(role) + "\n" + source`; `prelude_lines(role)` is the number of
lines before the user source, used to map naga errors back to user lines. The prelude is
`src/wgsl/common.wgsl`, then `raster.wgsl` or `motion.wgsl`, then `src/wgsl/<role>.wgsl`. Every
prelude identifier starts with `kg_`/`Kg`. Entry points: `kg_vs`/`kg_fs` (raster), `kg_cs`
(motion).

Bindings (the renderer's pipeline layouts must match):

- Raster group 0: `kg_frame: KgFrame` (uniform, the first 64 bytes of the renderer's `Uniforms`:
  `target_size`, `xf_slot`, pad, `a` = (time, _, opacity, _), `b` = (_, _, size), `c`) and
  `kg_transforms: array<vec4<f32>, 512>` (uniform; motion displacement rows, slot 0 identity).
  `kg_vs` displaces vertices by slot `xf_slot`.
- Raster group 1: Color `kg_source_tex`, `kg_sampler`; Mix `kg_source_tex`, `kg_backdrop_tex`,
  `kg_sampler` (plus the W3C blend helpers `kg_hard_light` … `kg_set_sat`, `kg_straight`,
  `kg_composite` for std mix shaders); Texture `kg_mask_tex`, `kg_sampler`; Mask empty.
- Motion group 0: `kg_motion: KgMotionIn` (uniform: parent affine rows, CPU pose, `kg_from`,
  `kg_to`, `timing` = (time, progress), `slot`, `chained`); group 1: `kg_states`, `kg_out`
  (storage read_write). `kg_write` stores the state and the displacement
  `P · M(state) · M(pose)⁻¹ · P⁻¹` (identity when the pose scale or `det P` is 0).
- Every role, group 2 binding 0: `args: Args` (storage, read). The file declares `struct Args`,
  or `alias Args = KgNoArgs;` when its Kagari argument type is `()`.

## Arguments

`ArgValue` is the converted Kagari `#[shader_arg]` value. `pack` lays it out with WGSL storage
rules (scalars 4/4, `vec2` 8/8, `vec4` 16/16, struct align = max member align with size rounded
up, array stride `roundUp(align, size)`), little-endian with zero padding. Packed values are at
most `MAX_ARGS_BYTES`. `lerp` interpolates floats and vectors and switches integers to the target;
it returns `None` on a shape mismatch, so tweens fall back to the target value.

## Validation

`validate(role, source)` parses `compose(role, source)` with naga's WGSL front end and runs the
validator with all flags. `ShaderError { line, message }` carries the 1-based user line, or `None`
when the error lies in the prelude or has no span. The crate's test `std_shaders_validate` checks
every std shader under `shaders/<role>/` with the role taken from its directory name.
