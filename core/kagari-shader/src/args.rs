//! Shader argument values and their WGSL storage-address-space layout.

/// A shader argument value, already converted from its Kagari value.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    F32(f32),
    I32(i32),
    U32(u32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    Struct(Vec<ArgValue>),
    Array(Vec<ArgValue>),
}

fn round_up(n: usize, align: usize) -> usize {
    match n.checked_rem(align) {
        Some(0) | None => n,
        Some(r) => n.saturating_add(align.saturating_sub(r)),
    }
}

/// (align, size) under WGSL storage layout rules.
fn layout(v: &ArgValue) -> (usize, usize) {
    match v {
        ArgValue::F32(_) | ArgValue::I32(_) | ArgValue::U32(_) => (4, 4),
        ArgValue::Vec2(_) => (8, 8),
        ArgValue::Vec4(_) => (16, 16),
        ArgValue::Struct(ms) => {
            let mut align = 1;
            let mut size = 0usize;
            for m in ms {
                let (a, s) = layout(m);
                align = align.max(a);
                size = round_up(size, a).saturating_add(s);
            }
            (align, round_up(size, align))
        }
        ArgValue::Array(es) => {
            let (a, s) = es.first().map_or((4, 0), layout);
            (a, round_up(s, a).saturating_mul(es.len()))
        }
    }
}

fn write(v: &ArgValue, out: &mut Vec<u8>) {
    let start = out.len();
    match v {
        ArgValue::F32(x) => out.extend_from_slice(&x.to_le_bytes()),
        ArgValue::I32(x) => out.extend_from_slice(&x.to_le_bytes()),
        ArgValue::U32(x) => out.extend_from_slice(&x.to_le_bytes()),
        ArgValue::Vec2(xs) => xs
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        ArgValue::Vec4(xs) => xs
            .iter()
            .for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
        ArgValue::Struct(ms) => {
            for m in ms {
                let (a, _) = layout(m);
                let at = start.saturating_add(round_up(out.len().saturating_sub(start), a));
                out.resize(at, 0);
                write(m, out);
            }
        }
        ArgValue::Array(es) => {
            let (a, s) = es.first().map_or((4, 0), layout);
            let stride = round_up(s, a);
            for (i, e) in es.iter().enumerate() {
                out.resize(start.saturating_add(stride.saturating_mul(i)), 0);
                write(e, out);
            }
        }
    }
    let (_, size) = layout(v);
    out.resize(start.saturating_add(size), 0);
}

/// Little-endian bytes of `v` laid out as WGSL storage memory, zero padded.
pub fn pack(v: &ArgValue) -> Vec<u8> {
    let mut out = Vec::with_capacity(layout(v).1);
    write(v, &mut out);
    out
}

/// Interpolates `a` → `b` at `t`; `None` unless both have the same shape. Integers switch to `b`.
pub fn lerp(a: &ArgValue, b: &ArgValue, t: f32) -> Option<ArgValue> {
    let f = |x: f32, y: f32| x + (y - x) * t;
    Some(match (a, b) {
        (ArgValue::F32(x), ArgValue::F32(y)) => ArgValue::F32(f(*x, *y)),
        (ArgValue::I32(_), ArgValue::I32(y)) => ArgValue::I32(*y),
        (ArgValue::U32(_), ArgValue::U32(y)) => ArgValue::U32(*y),
        (ArgValue::Vec2(x), ArgValue::Vec2(y)) => ArgValue::Vec2([f(x[0], y[0]), f(x[1], y[1])]),
        (ArgValue::Vec4(x), ArgValue::Vec4(y)) => {
            ArgValue::Vec4([f(x[0], y[0]), f(x[1], y[1]), f(x[2], y[2]), f(x[3], y[3])])
        }
        (ArgValue::Struct(xs), ArgValue::Struct(ys)) if xs.len() == ys.len() => ArgValue::Struct(
            xs.iter()
                .zip(ys)
                .map(|(x, y)| lerp(x, y, t))
                .collect::<Option<_>>()?,
        ),
        (ArgValue::Array(xs), ArgValue::Array(ys)) if xs.len() == ys.len() => ArgValue::Array(
            xs.iter()
                .zip(ys)
                .map(|(x, y)| lerp(x, y, t))
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}
