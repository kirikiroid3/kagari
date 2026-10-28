// The seven CSS filter functions in TyranoScript `[filter]` order: grayscale, sepia, saturate,
// hue-rotate, invert, brightness, contrast.

struct Args {
    grayscale: f32,
    sepia: f32,
    saturation: f32,
    hue: f32,
    invert: f32,
    brightness: f32,
    contrast: f32,
}

// Premultiplied linear → straight sRGB.
fn to_srgb(c: vec4<f32>) -> vec4<f32> {
    let s = kg_unpremultiply(c);
    return vec4<f32>(kg_linear_to_srgb(s.rgb), s.a);
}

// Straight sRGB → premultiplied linear, the colour clamped to 0..1.
fn from_srgb(c: vec4<f32>) -> vec4<f32> {
    let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return kg_premultiply(vec4<f32>(kg_srgb_to_linear(rgb), c.a));
}

// W3C Filter Effects grayscale(a), on straight sRGB.
fn grayscale(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let b = 1.0 - clamp(amount, 0.0, 1.0);
    return vec3<f32>(
        dot(vec3<f32>(0.2126 + 0.7874 * b, 0.7152 - 0.7152 * b, 0.0722 - 0.0722 * b), c),
        dot(vec3<f32>(0.2126 - 0.2126 * b, 0.7152 + 0.2848 * b, 0.0722 - 0.0722 * b), c),
        dot(vec3<f32>(0.2126 - 0.2126 * b, 0.7152 - 0.7152 * b, 0.0722 + 0.9278 * b), c),
    );
}

// W3C Filter Effects sepia(a), on straight sRGB.
fn sepia(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let b = 1.0 - clamp(amount, 0.0, 1.0);
    return vec3<f32>(
        dot(vec3<f32>(0.393 + 0.607 * b, 0.769 - 0.769 * b, 0.189 - 0.189 * b), c),
        dot(vec3<f32>(0.349 - 0.349 * b, 0.686 + 0.314 * b, 0.168 - 0.168 * b), c),
        dot(vec3<f32>(0.272 - 0.272 * b, 0.534 - 0.534 * b, 0.131 + 0.869 * b), c),
    );
}

// W3C Filter Effects saturate(s), on straight sRGB.
fn saturation(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let s = max(amount, 0.0);
    return vec3<f32>(
        dot(vec3<f32>(0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s), c),
        dot(vec3<f32>(0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s), c),
        dot(vec3<f32>(0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s), c),
    );
}

// W3C Filter Effects hue-rotate(θ) = L + cos θ·C + sin θ·S, on straight sRGB.
fn hue_rotate(c: vec3<f32>, angle: f32) -> vec3<f32> {
    let k = cos(angle);
    let n = sin(angle);
    return vec3<f32>(
        dot(vec3<f32>(0.213 + 0.787 * k - 0.213 * n, 0.715 - 0.715 * k - 0.715 * n, 0.072 - 0.072 * k + 0.928 * n), c),
        dot(vec3<f32>(0.213 - 0.213 * k + 0.143 * n, 0.715 + 0.285 * k + 0.140 * n, 0.072 - 0.072 * k - 0.283 * n), c),
        dot(vec3<f32>(0.213 - 0.213 * k - 0.787 * n, 0.715 - 0.715 * k + 0.715 * n, 0.072 + 0.928 * k + 0.072 * n), c),
    );
}

// W3C Filter Effects invert(a), on straight sRGB.
fn invert(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let a = clamp(amount, 0.0, 1.0);
    return a * (1.0 - c) + (1.0 - a) * c;
}

// W3C Filter Effects brightness(b), on straight sRGB.
fn brightness(c: vec3<f32>, amount: f32) -> vec3<f32> {
    return max(amount, 0.0) * c;
}

// W3C Filter Effects contrast(k), on straight sRGB.
fn contrast(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let k = max(amount, 0.0);
    return k * c + 0.5 - 0.5 * k;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = to_srgb(kg_source(uv));
    var rgb = grayscale(c.rgb, args.grayscale);
    rgb = sepia(rgb, args.sepia);
    rgb = saturation(rgb, args.saturation);
    rgb = hue_rotate(rgb, args.hue);
    rgb = invert(rgb, args.invert);
    rgb = brightness(rgb, args.brightness);
    rgb = contrast(rgb, args.contrast);
    return from_srgb(vec4<f32>(rgb, c.a));
}
