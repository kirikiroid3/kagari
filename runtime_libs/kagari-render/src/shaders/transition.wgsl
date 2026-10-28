
// Transitions between two layers over `bounds`. `u.a` = (kind, progress, softness, reverse),
// `u.b` = (dir, count, hold fraction, unused), `u.c` = the through colour (premultiplied linear).
// Kinds: 0 crossfade, 1 rule, 2 wipe, 3 blinds, 4 through colour. Dirs: 0 left, 1 right, 2 up,
// 3 down.

@group(1) @binding(0) var t_from: texture_2d<f32>;
@group(1) @binding(1) var t_to: texture_2d<f32>;
@group(1) @binding(2) var t_mask: texture_2d<f32>;
@group(1) @binding(3) var s0: sampler;

fn wipe(f: vec2<f32>, dir: u32, p: f32) -> f32 {
    switch dir {
        case 0u: { return select(0.0, 1.0, f.x >= 1.0 - p); }
        case 1u: { return select(0.0, 1.0, f.x <= p); }
        case 2u: { return select(0.0, 1.0, f.y >= 1.0 - p); }
        default: { return select(0.0, 1.0, f.y <= p); }
    }
}

@fragment
fn fs_transition(i: VsOut) -> @location(0) vec4<f32> {
    let uv = i.position.xy / u.target_size;
    let a = textureSample(t_from, s0, uv);
    let b = textureSample(t_to, s0, uv);
    let m = textureSample(t_mask, s0, i.local);
    let kind = u32(u.a.x);
    let p = clamp(u.a.y, 0.0, 1.0);
    let dir = u32(u.b.x);
    var out: vec4<f32>;
    switch kind {
        case 1u: {
            let rgb = select(m.rgb / m.a, vec3<f32>(0.0), m.a == 0.0);
            var luma = 255.0 * dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
            if u.a.w > 0.5 {
                luma = 255.0 - luma;
            }
            let s = max(u.a.z, 1.0);
            let t = p * (255.0 + s);
            let w = clamp((t - luma) / s, 0.0, 1.0);
            out = a * (1.0 - w) + b * w;
        }
        case 2u: {
            let w = wipe(i.local, dir, p);
            out = a * (1.0 - w) + b * w;
        }
        case 3u: {
            let n = max(u.b.y, 1.0);
            let w = wipe(fract(i.local * n), dir, p);
            out = a * (1.0 - w) + b * w;
        }
        case 4u: {
            let hold = u.b.z;
            let d = (1.0 - hold) / 2.0;
            let c = u.c;
            if d <= 0.0 {
                out = select(b, c, p < 1.0);
            } else if p < d {
                out = mix(a, c, p / d);
            } else if p < d + hold {
                out = c;
            } else {
                out = mix(c, b, (p - d - hold) / d);
            }
        }
        default: {
            out = a * (1.0 - p) + b * p;
        }
    }
    return out * i.color.a;
}
