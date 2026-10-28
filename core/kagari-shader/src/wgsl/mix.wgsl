// Mix role: `fn effect(uv: vec2<f32>) -> vec4<f32>` returns the final composited pixel.
// The W3C Compositing Level 1 helpers below are what the std mix shaders build on.

@group(1) @binding(0) var kg_source_tex: texture_2d<f32>;
@group(1) @binding(1) var kg_backdrop_tex: texture_2d<f32>;
@group(1) @binding(2) var kg_sampler: sampler;
@group(2) @binding(0) var<storage, read> args: Args;

// The entity's content, group opacity applied.
fn kg_source(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(kg_source_tex, kg_sampler, uv, 0.0) * kg_opacity();
}

// What is below the entity.
fn kg_backdrop(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(kg_backdrop_tex, kg_sampler, uv, 0.0);
}

fn kg_hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb * 2.0 * cs;
    }
    let t = 2.0 * cs - 1.0;
    return cb + t - cb * t;
}

fn kg_soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    let d = select(sqrt(cb), ((16.0 * cb - 12.0) * cb + 4.0) * cb, cb <= 0.25);
    return cb + (2.0 * cs - 1.0) * (d - cb);
}

fn kg_dodge(cb: f32, cs: f32) -> f32 {
    if cb == 0.0 {
        return 0.0;
    }
    if cs >= 1.0 {
        return 1.0;
    }
    return min(1.0, cb / (1.0 - cs));
}

fn kg_burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 {
        return 1.0;
    }
    if cs <= 0.0 {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}

fn kg_lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn kg_clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = kg_lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var o = c;
    if n < 0.0 {
        o = l + (o - l) * l / (l - n);
    }
    if x > 1.0 {
        o = l + (o - l) * (1.0 - l) / (x - l);
    }
    return o;
}

fn kg_set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return kg_clip_color(c + (l - kg_lum(c)));
}

fn kg_sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn kg_set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    if mx <= mn {
        return vec3<f32>(0.0);
    }
    return (c - mn) * s / (mx - mn);
}

// Straight colour clamped to 0..1.
fn kg_straight(c: vec4<f32>) -> vec3<f32> {
    return clamp(select(c.rgb / c.a, vec3<f32>(0.0), c.a == 0.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Source `s` over backdrop `b` (premultiplied) with mixed colour `m`.
fn kg_composite(s: vec4<f32>, b: vec4<f32>, m: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(
        s.rgb * (1.0 - b.a) + b.rgb * (1.0 - s.a) + s.a * b.a * m,
        s.a + b.a * (1.0 - s.a),
    );
}

@fragment
fn kg_fs(i: KgVsOut) -> @location(0) vec4<f32> {
    return effect(i.position.xy / kg_frame.target_size);
}
