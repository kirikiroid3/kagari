// Motion roles (local, transitional): one compute invocation per stage.

// Position in design px, rotation in radians (clockwise, y-down).
struct KgState {
    position: vec2<f32>,
    scale: f32,
    rotation: f32,
}

struct KgMotionIn {
    // Parent affine rows (a, c, e, 0) and (b, d, f, 0).
    parent0: vec4<f32>,
    parent1: vec4<f32>,
    // The CPU-sampled pose (x, y, scale, rotation).
    pose: vec4<f32>,
    // Travel endpoints (x, y, scale, rotation); `from` is a WGSL reserved word.
    kg_from: vec4<f32>,
    kg_to: vec4<f32>,
    // (time in seconds, progress, 0, 0).
    timing: vec4<f32>,
    slot: u32,
    chained: u32,
    kg_pad0: u32,
    kg_pad1: u32,
}

@group(0) @binding(0) var<uniform> kg_motion: KgMotionIn;
@group(1) @binding(0) var<storage, read_write> kg_states: array<vec4<f32>>;
@group(1) @binding(1) var<storage, read_write> kg_out: array<vec4<f32>>;

fn kg_time() -> f32 {
    return kg_motion.timing.x;
}

fn kg_state(v: vec4<f32>) -> KgState {
    return KgState(v.xy, v.z, v.w);
}

// A 2D affine: x' = m * x + t.
struct KgAffine {
    m: mat2x2<f32>,
    t: vec2<f32>,
}

fn kg_affine_mul(l: KgAffine, r: KgAffine) -> KgAffine {
    return KgAffine(l.m * r.m, l.m * r.t + l.t);
}

fn kg_affine_inverse(a: KgAffine) -> KgAffine {
    let d = determinant(a.m);
    let inv = mat2x2<f32>(
        vec2<f32>(a.m[1][1], -a.m[0][1]) / d,
        vec2<f32>(-a.m[1][0], a.m[0][0]) / d,
    );
    return KgAffine(inv, -(inv * a.t));
}

// T(position) R(rotation) S(scale).
fn kg_affine_of(s: KgState) -> KgAffine {
    let c = cos(s.rotation);
    let n = sin(s.rotation);
    let m = mat2x2<f32>(vec2<f32>(c, n), vec2<f32>(-n, c)) * s.scale;
    return KgAffine(m, s.position);
}

fn kg_write(s: KgState) {
    let slot = kg_motion.slot;
    kg_states[slot] = vec4<f32>(s.position, s.scale, s.rotation);
    let p = KgAffine(
        mat2x2<f32>(
            vec2<f32>(kg_motion.parent0.x, kg_motion.parent1.x),
            vec2<f32>(kg_motion.parent0.y, kg_motion.parent1.y),
        ),
        vec2<f32>(kg_motion.parent0.z, kg_motion.parent1.z),
    );
    var d = KgAffine(mat2x2<f32>(vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0)), vec2<f32>(0.0));
    if kg_motion.pose.z != 0.0 && determinant(p.m) != 0.0 {
        let pose = kg_affine_of(kg_state(kg_motion.pose));
        d = kg_affine_mul(
            kg_affine_mul(kg_affine_mul(p, kg_affine_of(s)), kg_affine_inverse(pose)),
            kg_affine_inverse(p),
        );
    }
    kg_out[2u * slot] = vec4<f32>(d.m[0][0], d.m[1][0], d.t.x, 0.0);
    kg_out[2u * slot + 1u] = vec4<f32>(d.m[0][1], d.m[1][1], d.t.y, 0.0);
}
