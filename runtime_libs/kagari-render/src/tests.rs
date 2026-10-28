#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::mem::offset_of;

use kagari_engine::{Affine, MotionStage, Pose, ShaderEffect};
use kagari_shader::ShaderRole;

use crate::Viewport;
use crate::blur::gaussian_weights;
use crate::pipelines::{MOTION_UNIFORM_SIZE, MotionUniform, UNIFORM_SIZE, Uniforms};
use crate::plan::{align_up, motion_uniform};
use crate::textures::thumbnail_problem;

#[test]
fn letterbox_fits_height() {
    let v = Viewport::letterbox((1280, 720), (1920, 1200));
    assert_eq!(
        v,
        Viewport {
            surface: (1920, 1200),
            x: 0,
            y: 60,
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn letterbox_fits_width() {
    let v = Viewport::letterbox((1280, 720), (1000, 1000));
    assert_eq!((v.x, v.y, v.width, v.height), (0, 218, 1000, 563));
}

#[test]
fn to_design_inverts_letterbox() {
    let v = Viewport::letterbox((1280, 720), (1920, 1200));
    assert_eq!(v.to_design((1280, 720), 960.0, 600.0), (640.0, 360.0));
    assert_eq!(v.to_design((1280, 720), 0.0, 30.0), (0.0, -20.0));
}

#[test]
fn gaussian_weights_are_normalised() {
    let w = gaussian_weights(2.0);
    assert_eq!(w.len(), 7);
    let total = w[0] + 2.0 * w[1..].iter().sum::<f32>();
    assert!((total - 1.0).abs() < 1e-6, "{total}");
}

#[test]
fn oversized_thumbnails_are_refused() {
    assert_eq!(thumbnail_problem(320, 180, 320 * 180 * 4, 8192), None);
    assert_eq!(
        thumbnail_problem(9000, 1, 9000 * 4, 8192).as_deref(),
        Some("is 9000×1, larger than the GPU limit 8192")
    );
    assert_eq!(
        thumbnail_problem(2, 2, 15, 8192).as_deref(),
        Some("has 15 bytes, expected 2×2×4")
    );
}

fn parse(source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    module
}

/// `(member offsets, size)` of struct `name`.
fn struct_layout(module: &naga::Module, name: &str) -> (Vec<u32>, u32) {
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, t)| t.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no struct {name}"));
    match &ty.inner {
        naga::TypeInner::Struct { members, span } => {
            (members.iter().map(|m| m.offset).collect(), *span)
        }
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn renderer_shaders_validate() {
    let color = include_str!("shaders/color.wgsl");
    for (name, own) in [
        ("quad", include_str!("shaders/quad.wgsl")),
        ("blur", include_str!("shaders/blur.wgsl")),
        ("transition", include_str!("shaders/transition.wgsl")),
        ("present", include_str!("shaders/present.wgsl")),
        (
            "mask_composite",
            include_str!("shaders/mask_composite.wgsl"),
        ),
    ] {
        let module = parse(&[color, own].concat());
        let (_, size) = struct_layout(&module, "Uniforms");
        assert_eq!(u64::from(size), UNIFORM_SIZE, "{name}");
    }
}

#[test]
fn uniforms_start_with_the_user_frame() {
    let src =
        "alias Args = KgNoArgs;\nfn effect(uv: vec2<f32>) -> vec4<f32> { return kg_source(uv); }\n";
    let module = parse(&kagari_shader::compose(ShaderRole::Color, src));
    let (offsets, size) = struct_layout(&module, "KgFrame");
    let ours = [
        offset_of!(Uniforms, target_size),
        offset_of!(Uniforms, xf_slot),
        offset_of!(Uniforms, pad0),
        offset_of!(Uniforms, a),
        offset_of!(Uniforms, b),
        offset_of!(Uniforms, c),
    ]
    .map(|o| u32::try_from(o).unwrap());
    assert_eq!(offsets, ours);
    assert_eq!(size, 64);
    assert_eq!(offset_of!(Uniforms, weights), 64);

    let color = parse(
        &[
            include_str!("shaders/color.wgsl"),
            include_str!("shaders/quad.wgsl"),
        ]
        .concat(),
    );
    assert_eq!(struct_layout(&color, "Uniforms").0[..6], ours);
}

#[test]
fn motion_uniform_matches_the_prelude() {
    let src =
        "alias Args = KgNoArgs;\nfn effect(state: KgState, t: f32) -> KgState { return state; }\n";
    let module = parse(&kagari_shader::compose(ShaderRole::LocalMotion, src));
    let (offsets, size) = struct_layout(&module, "KgMotionIn");
    assert_eq!(u64::from(size), MOTION_UNIFORM_SIZE);
    assert_eq!(MOTION_UNIFORM_SIZE, 112);
    let ours = [
        offset_of!(MotionUniform, parent0),
        offset_of!(MotionUniform, parent1),
        offset_of!(MotionUniform, pose),
        offset_of!(MotionUniform, from),
        offset_of!(MotionUniform, to),
        offset_of!(MotionUniform, timing),
        offset_of!(MotionUniform, slot),
        offset_of!(MotionUniform, chained),
        offset_of!(MotionUniform, pad),
        offset_of!(MotionUniform, pad) + 4,
    ]
    .map(|o| u32::try_from(o).unwrap());
    assert_eq!(offsets, ours);
}

#[test]
fn args_offsets_are_aligned() {
    assert_eq!(align_up(0, 256), 0);
    assert_eq!(align_up(1, 256), 256);
    assert_eq!(align_up(256, 256), 256);
    assert_eq!(align_up(257, 256), 512);
    assert_eq!(align_up(5, 0), 5);
}

#[test]
fn motion_uniform_carries_parent_rows_and_timing() {
    let xf = Affine([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let pose = Pose {
        position: [10.0, 20.0],
        scale: 1.5,
        rotation: 0.25,
    };
    let effect = ShaderEffect {
        shader: kagari_bytecode::AssetId(3),
        args: std::rc::Rc::from(vec![0u8; 4]),
        time: 2.5,
    };
    let local = motion_uniform(
        &xf,
        &pose,
        &MotionStage::Local {
            effect: effect.clone(),
        },
        7,
        true,
    );
    assert_eq!(local.parent0, [1.0, 3.0, 5.0, 0.0]);
    assert_eq!(local.parent1, [2.0, 4.0, 6.0, 0.0]);
    assert_eq!(local.pose, [10.0, 20.0, 1.5, 0.25]);
    assert_eq!(local.timing, [2.5, 0.0, 0.0, 0.0]);
    assert_eq!((local.slot, local.chained), (7, 1));
    let to = Pose {
        position: [30.0, 40.0],
        scale: 1.0,
        rotation: 0.0,
    };
    let travel = MotionStage::Transitional {
        effect,
        from: pose,
        to,
        progress: 0.5,
    };
    let t = motion_uniform(&xf, &pose, &travel, 1, false);
    assert_eq!(t.from, [10.0, 20.0, 1.5, 0.25]);
    assert_eq!(t.to, [30.0, 40.0, 1.0, 0.0]);
    assert_eq!(t.timing, [2.5, 0.5, 0.0, 0.0]);
    assert_eq!(t.chained, 0);
}
