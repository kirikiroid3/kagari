//! `Node::Text`: glyph masks from the atlas, drawn as shadows, then outlines, then fills.

use std::collections::BTreeMap;

use kagari_engine::{Rect, Reveal};
use kagari_text::Layout;

use crate::atlas::AtlasGlyph;
use crate::blur::{MAX_SIGMA, gaussian_weights, pack, radius};
use crate::pipelines::{Pipe, Uniforms};
use crate::plan::{BindKey, Ctx, Planner, PxRect, Target, transformed_bounds};

/// The reveal alpha of a glyph of reveal unit `unit`.
pub(crate) fn reveal_alpha(reveal: Reveal, unit: u32) -> f32 {
    match reveal {
        Reveal::All => 1.0,
        Reveal::Units(n) => {
            if unit < n {
                1.0
            } else {
                0.0
            }
        }
        Reveal::UnitsWithFade { full, partial } => {
            if unit < full {
                1.0
            } else if unit == full {
                if partial.is_nan() { 0.0 } else { partial.clamp(0.0, 1.0) }
            } else {
                0.0
            }
        }
    }
}

/// The design-space quad of mask `m` for a glyph at baseline `pos`, moved by `offset`.
fn glyph_rect(origin: [f32; 2], pos: [f32; 2], offset: [f32; 2], m: &AtlasGlyph) -> Rect {
    Rect {
        x: (origin[0] + pos[0]).round() + m.left as f32 + offset[0],
        y: (origin[1] + pos[1]).round() - m.top as f32 + offset[1],
        w: m.width as f32,
        h: m.height as f32,
    }
}

/// Which mask of a glyph to draw, where and in what colour.
#[derive(Clone, Copy)]
struct MaskLook {
    /// 0 for the fill, else the outline ring width.
    outline_px: f32,
    offset: [f32; 2],
    color: [f32; 4],
}

impl MaskLook {
    fn new(outline_px: f32, offset: [f32; 2], color: [f32; 4]) -> MaskLook {
        MaskLook {
            outline_px,
            offset,
            color,
        }
    }
}

fn scale(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3] * k]
}

/// The union of two pixel rectangles (an empty one is ignored).
fn union(a: Option<PxRect>, b: PxRect) -> Option<PxRect> {
    if b.is_empty() {
        return a;
    }
    let Some(a) = a else { return Some(b) };
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.x.saturating_add(a.w).max(b.x.saturating_add(b.w));
    let y1 = a.y.saturating_add(a.h).max(b.y.saturating_add(b.h));
    Some(PxRect {
        x: x0,
        y: y0,
        w: x1.saturating_sub(x0),
        h: y1.saturating_sub(y0),
    })
}

impl Planner<'_> {
    pub(crate) fn text(&mut self, ctx: Ctx, origin: [f32; 2], layout: &Layout, reveal: Reveal, opacity: f32) {
        let alpha = opacity * ctx.alpha;
        if alpha <= 0.0 || layout.glyphs.is_empty() {
            return;
        }
        let decorated = layout
            .glyphs
            .iter()
            .any(|g| g.outline.is_some() || g.shadow.is_some());
        if alpha < 0.999 && decorated {
            let layer = self.acquire();
            self.begin_pass(Target::Layer(layer));
            let inner = Ctx {
                alpha: 1.0,
                target: Target::Layer(layer),
                ..ctx
            };
            self.glyphs(inner, origin, layout, reveal, 1.0);
            self.composite(layer, ctx.target, Pipe::Linear, [alpha; 4], ctx.scissor);
            self.layers.release(layer);
        } else {
            self.glyphs(ctx, origin, layout, reveal, alpha);
        }
    }

    /// Uploads (on first use) and draws one mask.
    fn mask(
        &mut self,
        ctx: &Ctx,
        pipe: Pipe,
        glyph: &kagari_text::Glyph,
        origin: [f32; 2],
        look: MaskLook,
    ) -> Option<PxRect> {
        let m = self
            .atlas
            .glyph(self.gfx, self.assets.fonts(), glyph.key, look.outline_px)?;
        let rect = glyph_rect(origin, glyph.pos, look.offset, &m);
        let verts = self.quad(&ctx.xf, &rect, m.uv, look.color);
        let base = self.base;
        self.draw(ctx.target, pipe, BindKey::Atlas(m.page), verts, ctx.scissor, base);
        Some(transformed_bounds(&ctx.xf, &rect))
    }

    /// Every glyph at reveal alpha × `k`: all shadows, then all outlines, then all fills.
    fn glyphs(&mut self, ctx: Ctx, origin: [f32; 2], layout: &Layout, reveal: Reveal, k: f32) {
        let alphas: Vec<f32> = layout
            .glyphs
            .iter()
            .map(|g| reveal_alpha(reveal, g.unit) * k)
            .collect();
        let mut buckets: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, (g, a)) in layout.glyphs.iter().zip(&alphas).enumerate() {
            if *a > 0.0
                && let Some((_, blur, _)) = g.shadow
            {
                let blur = if blur.is_nan() { 0.0 } else { blur.max(0.0) };
                buckets.entry(blur.round() as u32).or_default().push(i);
            }
        }
        for (blur, indices) in buckets {
            self.shadows(ctx, origin, layout, &alphas, blur, &indices);
        }
        for (g, a) in layout.glyphs.iter().zip(&alphas) {
            if *a > 0.0
                && let Some((width, color)) = g.outline
                && width > 0.0
            {
                self.mask(&ctx, Pipe::Mask, g, origin, MaskLook::new(width, [0.0; 2], scale(color, *a)));
            }
        }
        for (g, a) in layout.glyphs.iter().zip(&alphas) {
            if *a > 0.0 {
                self.mask(&ctx, Pipe::Mask, g, origin, MaskLook::new(0.0, [0.0; 2], scale(g.color, *a)));
            }
        }
    }

    /// The shadows of one blur bucket: drawn into a layer (per-pixel maximum), blurred, then
    /// composited.
    fn shadows(&mut self, ctx: Ctx, origin: [f32; 2], layout: &Layout, alphas: &[f32], blur: u32, indices: &[usize]) {
        let layer = self.acquire();
        self.begin_pass(Target::Layer(layer));
        let inner = Ctx {
            target: Target::Layer(layer),
            ..ctx
        };
        let mut bounds = None;
        for &i in indices {
            let (Some(g), Some(a)) = (layout.glyphs.get(i), alphas.get(i)) else {
                continue;
            };
            let Some((offset, _, color)) = g.shadow else {
                continue;
            };
            let color = scale(color, *a);
            if let Some(b) = self.mask(&inner, Pipe::MaskMax, g, origin, MaskLook::new(0.0, offset, color)) {
                bounds = union(bounds, b);
            }
            if let Some((width, _)) = g.outline
                && width > 0.0
                && let Some(b) = self.mask(&inner, Pipe::MaskMax, g, origin, MaskLook::new(width, offset, color))
            {
                bounds = union(bounds, b);
            }
        }
        if let Some(bounds) = bounds {
            let mut scissor = ctx.scissor;
            if blur > 0 {
                let wanted = blur as f32 / 2.0;
                if wanted > MAX_SIGMA {
                    log::debug!("shadow blur {blur}px clamped to 64px");
                }
                let sigma = wanted.min(MAX_SIGMA);
                let r = radius(sigma);
                let pad = r.saturating_add(1);
                let grown = PxRect {
                    x: bounds.x.saturating_sub(pad),
                    y: bounds.y.saturating_sub(pad),
                    w: bounds.w.saturating_add(pad.saturating_mul(2)),
                    h: bounds.h.saturating_add(pad.saturating_mul(2)),
                };
                let blur_scissor = grown.intersect(&PxRect::full(self.size));
                let weights = pack(&gaussian_weights(sigma));
                let temp = self.acquire();
                for (src, dst, dir) in [(layer, temp, [1.0, 0.0]), (temp, layer, [0.0, 1.0])] {
                    let mut u = Uniforms::new(self.size);
                    u.a = [dir[0], dir[1], r as f32, 0.0];
                    u.weights = weights;
                    let uniform = self.uniform(u);
                    self.begin_pass(Target::Layer(dst));
                    let verts = self.full_quad([1.0; 4]);
                    self.draw(
                        Target::Layer(dst),
                        Pipe::Blur,
                        BindKey::Blur { src },
                        verts,
                        blur_scissor,
                        uniform,
                    );
                }
                self.layers.release(temp);
                scissor = blur_scissor.intersect(&ctx.scissor);
            }
            self.composite(layer, ctx.target, Pipe::Linear, [1.0; 4], scissor);
        }
        self.layers.release(layer);
    }
}
