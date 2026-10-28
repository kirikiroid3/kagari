#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

use crate::*;

#[test]
fn pack_layout() {
    let spot = ArgValue::Struct(vec![
        ArgValue::Vec2([1.0, 2.0]),
        ArgValue::Vec4([3.0, 4.0, 5.0, 6.0]),
        ArgValue::F32(7.0),
    ]);
    let v = ArgValue::Struct(vec![
        ArgValue::U32(9),
        ArgValue::Array(vec![spot.clone(), spot]),
    ]);
    let bytes = pack(&v);
    assert_eq!(bytes.len(), 112);
    let f = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 9);
    assert_eq!(f(16), 1.0);
    assert_eq!(f(32), 3.0);
    assert_eq!(f(48), 7.0);
    assert_eq!(f(16 + 48), 1.0);
    assert_eq!(f(16 + 48 + 32), 7.0);
    assert!(bytes[4..16].iter().all(|b| *b == 0));
}

#[test]
fn lerp_cases() {
    let a = ArgValue::Struct(vec![
        ArgValue::F32(0.0),
        ArgValue::U32(1),
        ArgValue::Vec2([0.0, 2.0]),
    ]);
    let b = ArgValue::Struct(vec![
        ArgValue::F32(2.0),
        ArgValue::U32(5),
        ArgValue::Vec2([4.0, 2.0]),
    ]);
    assert_eq!(
        lerp(&a, &b, 0.5),
        Some(ArgValue::Struct(vec![
            ArgValue::F32(1.0),
            ArgValue::U32(5),
            ArgValue::Vec2([2.0, 2.0])
        ]))
    );
    assert_eq!(
        lerp(&ArgValue::I32(1), &ArgValue::I32(3), 0.0),
        Some(ArgValue::I32(3))
    );
    assert_eq!(lerp(&ArgValue::F32(1.0), &ArgValue::U32(3), 0.5), None);
    assert_eq!(
        lerp(
            &ArgValue::Array(vec![ArgValue::F32(0.0)]),
            &ArgValue::Array(vec![]),
            0.5
        ),
        None
    );
}

#[cfg(feature = "validate")]
mod validation {
    use crate::*;

    fn minimal(role: ShaderRole) -> &'static str {
        match role {
            ShaderRole::Color | ShaderRole::Texture => {
                "alias Args = KgNoArgs;\nfn effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(uv, 0.0, 1.0); }\n"
            }
            ShaderRole::Mix => {
                "alias Args = KgNoArgs;\nfn effect(uv: vec2<f32>) -> vec4<f32> {\n    let s = kg_source(uv);\n    let b = kg_backdrop(uv);\n    return kg_composite(s, b, kg_straight(s));\n}\n"
            }
            ShaderRole::Mask => {
                "struct Args { k: f32 }\nfn effect(uv: vec2<f32>) -> f32 { return uv.x * args.k; }\n"
            }
            ShaderRole::LocalMotion => {
                "alias Args = KgNoArgs;\nfn effect(state: KgState, t: f32) -> KgState { var s = state; s.rotation += t; return s; }\n"
            }
            ShaderRole::TransitionalMotion => {
                "alias Args = KgNoArgs;\nfn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState { return KgState(mix(start.position, end.position, progress), end.scale, end.rotation); }\n"
            }
        }
    }

    #[test]
    fn every_role_accepts_minimal() {
        for role in ShaderRole::ALL {
            if let Err(e) = validate(role, minimal(role)) {
                panic!("{role}: {e}");
            }
        }
    }

    #[test]
    fn missing_args_fails() {
        let src = "fn effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }\n";
        assert!(validate(ShaderRole::Color, src).is_err());
    }

    #[test]
    fn mask_returning_vec4_fails() {
        let src = "alias Args = KgNoArgs;\nfn effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }\n";
        assert!(validate(ShaderRole::Mask, src).is_err());
    }

    #[test]
    fn error_line_is_user_relative() {
        let src = "alias Args = KgNoArgs;\nfn effect(uv: vec2<f32>) -> vec4<f32> {\n    return undefined_thing;\n}\n";
        let e = validate(ShaderRole::Color, src).unwrap_err();
        assert_eq!(e.line, Some(3), "{e}");
    }

    #[test]
    fn std_shaders_validate() {
        let mut failures = Vec::new();
        for path in kagari_std::FILES {
            let Some(rest) = path.strip_prefix("shaders/") else {
                continue;
            };
            let Some((dir, _)) = rest.split_once('/') else {
                continue;
            };
            if !path.ends_with(".wgsl") {
                continue;
            }
            let role = match dir {
                "color" => ShaderRole::Color,
                "mix" => ShaderRole::Mix,
                "texture" => ShaderRole::Texture,
                "mask" => ShaderRole::Mask,
                "local_motion" => ShaderRole::LocalMotion,
                "transitional_motion" => ShaderRole::TransitionalMotion,
                other => panic!("unknown shader role directory `{other}` in `{path}`"),
            };
            let src = kagari_std::file(path).unwrap_or_else(|| panic!("`{path}` missing"));
            if let Err(e) = validate(role, src) {
                failures.push(format!("{path} ({role}): {e}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
