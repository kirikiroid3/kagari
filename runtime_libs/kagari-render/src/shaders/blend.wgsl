
// Separable and non-separable blend modes of W3C Compositing Level 1, §5.8, composited over the
// backdrop copy. `u.a.x` = mode (the `Blend` enum order), `u.a.y` = group opacity.

@group(1) @binding(0) var t_src: texture_2d<f32>;
@group(1) @binding(1) var t_backdrop: texture_2d<f32>;
@group(1) @binding(2) var s0: sampler;

fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb * 2.0 * cs;
    }
    let t = 2.0 * cs - 1.0;
    return cb + t - cb * t;
}

fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    let d = select(sqrt(cb), ((16.0 * cb - 12.0) * cb + 4.0) * cb, cb <= 0.25);
    return cb + (2.0 * cs - 1.0) * (d - cb);
}

fn dodge(cb: f32, cs: f32) -> f32 {
    if cb == 0.0 {
        return 0.0;
    }
    if cs >= 1.0 {
        return 1.0;
    }
    return min(1.0, cb / (1.0 - cs));
}

fn burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 {
        return 1.0;
    }
    if cs <= 0.0 {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
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

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + (l - lum(c)));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    if mx <= mn {
        return vec3<f32>(0.0);
    }
    return (c - mn) * s / (mx - mn);
}

fn per_channel(mode: u32, cb: f32, cs: f32) -> f32 {
    switch mode {
        case 2u: { return cb * cs; }
        case 3u: { return cb + cs - cb * cs; }
        case 4u: { return hard_light(cs, cb); }
        case 5u: { return min(cb, cs); }
        case 6u: { return max(cb, cs); }
        case 7u: { return dodge(cb, cs); }
        case 8u: { return burn(cb, cs); }
        case 9u: { return hard_light(cb, cs); }
        case 10u: { return soft_light(cb, cs); }
        case 11u: { return abs(cb - cs); }
        case 12u: { return cb + cs - 2.0 * cb * cs; }
        default: { return cs; }
    }
}

fn mix_mode(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 13u: { return set_lum(set_sat(cs, sat(cb)), lum(cb)); }
        case 14u: { return set_lum(set_sat(cb, sat(cs)), lum(cb)); }
        case 15u: { return set_lum(cs, lum(cb)); }
        case 16u: { return set_lum(cb, lum(cs)); }
        default: {
            return vec3<f32>(
                per_channel(mode, cb.r, cs.r),
                per_channel(mode, cb.g, cs.g),
                per_channel(mode, cb.b, cs.b),
            );
        }
    }
}

@fragment
fn fs_blend(i: VsOut) -> @location(0) vec4<f32> {
    let s = textureSample(t_src, s0, i.uv) * u.a.y;
    let b = textureSample(t_backdrop, s0, i.uv);
    let cs = clamp(select(s.rgb / s.a, vec3<f32>(0.0), s.a == 0.0), vec3<f32>(0.0), vec3<f32>(1.0));
    let cb = clamp(select(b.rgb / b.a, vec3<f32>(0.0), b.a == 0.0), vec3<f32>(0.0), vec3<f32>(1.0));
    let m = mix_mode(u32(u.a.x), cb, cs);
    return vec4<f32>(
        s.rgb * (1.0 - b.a) + b.rgb * (1.0 - s.a) + s.a * b.a * m,
        s.a + b.a * (1.0 - s.a),
    );
}
