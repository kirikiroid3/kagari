//! The first phase of a frame: one walk over the node tree that produces the vertices, the
//! uniform slots and the ordered list of passes and copies.

use std::ops::Range;

use kagari_bytecode::AssetId;
use kagari_engine::{Affine, AssetStore, Blend, Dir, Node, Rect, ThumbKey, TransitionDraw};

use crate::Gfx;
use crate::atlas::GlyphAtlas;
use crate::layers::{LayerId, LayerPool};
use crate::pipelines::{Pipe, Uniforms, Vertex};
use crate::textures::TextureCache;

/// A pixel rectangle of a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PxRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl PxRect {
    pub fn full(size: (u32, u32)) -> PxRect {
        PxRect {
            x: 0,
            y: 0,
            w: size.0,
            h: size.1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn intersect(&self, o: &PxRect) -> PxRect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.x.saturating_add(self.w).min(o.x.saturating_add(o.w));
        let y1 = self.y.saturating_add(self.h).min(o.y.saturating_add(o.h));
        PxRect {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        }
    }

    /// The pixels covering the float box `[x0, x1) × [y0, y1)`: min floored, max ceiled, clamped
    /// at 0.
    pub fn covering(x0: f32, y0: f32, x1: f32, y1: f32) -> PxRect {
        let px0 = x0.floor().max(0.0) as u32;
        let py0 = y0.floor().max(0.0) as u32;
        let px1 = x1.ceil().max(0.0) as u32;
        let py1 = y1.ceil().max(0.0) as u32;
        PxRect {
            x: px0,
            y: py0,
            w: px1.saturating_sub(px0),
            h: py1.saturating_sub(py0),
        }
    }
}

/// What a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    Design,
    Layer(LayerId),
}

/// The group-1 bind group of a draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BindKey {
    Image(AssetId),
    Thumb(ThumbKey),
    Still,
    Atlas(u16),
    Layer(LayerId),
    Blend { src: LayerId, backdrop: LayerId },
    Blur { src: LayerId },
    Transition { from: LayerId, to: LayerId, mask: Option<AssetId> },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draw {
    pub pipe: Pipe,
    pub bind: BindKey,
    pub verts: Range<u32>,
    pub scissor: PxRect,
    /// Byte offset of the uniform slot.
    pub uniform: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    Pass { target: Target, clear: bool, draws: Vec<Draw> },
    /// `rect` of `from` into the same pixels of `to`.
    Copy { from: Target, to: LayerId, rect: PxRect },
}

/// The output of the planning phase.
#[derive(Default)]
pub(crate) struct Plan {
    pub verts: Vec<Vertex>,
    pub uniforms: Vec<u8>,
    pub ops: Vec<Op>,
}

/// The traversal state inherited by a subtree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ctx {
    pub xf: Affine,
    pub alpha: f32,
    pub scissor: PxRect,
    pub target: Target,
}

/// `p ∘ g`: apply `g` first, then `p`.
pub(crate) fn compose(p: &Affine, g: &Affine) -> Affine {
    let [a1, b1, c1, d1, e1, f1] = p.0;
    let [a2, b2, c2, d2, e2, f2] = g.0;
    Affine([
        a1 * a2 + c1 * b2,
        b1 * a2 + d1 * b2,
        a1 * c2 + c1 * d2,
        b1 * c2 + d1 * d2,
        a1 * e2 + c1 * f2 + e1,
        b1 * e2 + d1 * f2 + f1,
    ])
}

pub(crate) fn apply(m: &Affine, x: f32, y: f32) -> [f32; 2] {
    let [a, b, c, d, e, f] = m.0;
    [a * x + c * y + e, b * x + d * y + f]
}

/// The pixels covering `rect` transformed by `xf`.
pub(crate) fn transformed_bounds(xf: &Affine, rect: &Rect) -> PxRect {
    let corners = [
        apply(xf, rect.x, rect.y),
        apply(xf, rect.x + rect.w, rect.y),
        apply(xf, rect.x, rect.y + rect.h),
        apply(xf, rect.x + rect.w, rect.y + rect.h),
    ];
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for [x, y] in corners {
        min = [min[0].min(x), min[1].min(y)];
        max = [max[0].max(x), max[1].max(y)];
    }
    if !(min[0].is_finite() && min[1].is_finite() && max[0].is_finite() && max[1].is_finite()) {
        return PxRect::full((0, 0));
    }
    PxRect::covering(min[0], min[1], max[0], max[1])
}

fn scale(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3] * k]
}

fn blend_index(b: Blend) -> f32 {
    let i: u8 = match b {
        Blend::Normal => 0,
        Blend::Add => 1,
        Blend::Multiply => 2,
        Blend::Screen => 3,
        Blend::Overlay => 4,
        Blend::Darken => 5,
        Blend::Lighten => 6,
        Blend::ColorDodge => 7,
        Blend::ColorBurn => 8,
        Blend::HardLight => 9,
        Blend::SoftLight => 10,
        Blend::Difference => 11,
        Blend::Exclusion => 12,
        Blend::Hue => 13,
        Blend::Saturation => 14,
        Blend::Color => 15,
        Blend::Luminosity => 16,
    };
    f32::from(i)
}

fn dir_index(d: Dir) -> f32 {
    match d {
        Dir::Left => 0.0,
        Dir::Right => 1.0,
        Dir::Up => 2.0,
        Dir::Down => 3.0,
    }
}

/// Everything the walk reads and fills.
pub(crate) struct Planner<'a> {
    pub gfx: &'a Gfx,
    pub textures: &'a mut TextureCache,
    pub atlas: &'a mut GlyphAtlas,
    pub layers: &'a mut LayerPool,
    pub assets: &'a AssetStore,
    /// The design target size; every layer has it.
    pub size: (u32, u32),
    /// The uniform slot stride.
    pub slot: usize,
    pub max_dim: u32,
    pub has_still: bool,
    pub plan: Plan,
    /// The slot holding only the target size.
    pub base: u32,
}

impl<'a> Planner<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gfx: &'a Gfx,
        textures: &'a mut TextureCache,
        atlas: &'a mut GlyphAtlas,
        layers: &'a mut LayerPool,
        assets: &'a AssetStore,
        size: (u32, u32),
        slot: usize,
        has_still: bool,
    ) -> Planner<'a> {
        let max_dim = gfx.device.limits().max_texture_dimension_2d;
        let mut planner = Planner {
            gfx,
            textures,
            atlas,
            layers,
            assets,
            size,
            slot,
            max_dim,
            has_still,
            plan: Plan::default(),
            base: 0,
        };
        planner.base = planner.uniform(Uniforms::new(size));
        planner
    }

    /// Plans `nodes` onto the cleared design target.
    pub fn run(mut self, nodes: &[Node]) -> Plan {
        self.begin_pass(Target::Design);
        let ctx = Ctx {
            xf: Affine::IDENTITY,
            alpha: 1.0,
            scissor: PxRect::full(self.size),
            target: Target::Design,
        };
        for node in nodes {
            self.node(node, ctx);
        }
        self.plan
    }

    /// Appends a uniform slot; returns its byte offset.
    pub fn uniform(&mut self, u: Uniforms) -> u32 {
        let offset = self.plan.uniforms.len();
        self.plan.uniforms.extend_from_slice(bytemuck::bytes_of(&u));
        self.plan.uniforms.resize(offset.saturating_add(self.slot), 0);
        u32::try_from(offset).unwrap_or(u32::MAX)
    }

    /// Uniforms with the target size and the given parameters.
    pub fn params(&mut self, a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> u32 {
        let mut u = Uniforms::new(self.size);
        u.a = a;
        u.b = b;
        u.c = c;
        self.uniform(u)
    }

    /// Opens a pass that clears `target`.
    pub fn begin_pass(&mut self, target: Target) {
        self.plan.ops.push(Op::Pass {
            target,
            clear: true,
            draws: Vec::new(),
        });
    }

    pub fn copy(&mut self, from: Target, to: LayerId, rect: PxRect) {
        if !rect.is_empty() {
            self.plan.ops.push(Op::Copy { from, to, rect });
        }
    }

    pub fn acquire(&mut self) -> LayerId {
        self.layers.acquire(&self.gfx.device, self.size)
    }

    /// Records a draw into `target`, merging it into the previous draw when possible.
    pub fn draw(&mut self, target: Target, pipe: Pipe, bind: BindKey, verts: Range<u32>, scissor: PxRect, uniform: u32) {
        if scissor.is_empty() || verts.is_empty() {
            return;
        }
        let draw = Draw {
            pipe,
            bind,
            verts,
            scissor,
            uniform,
        };
        if let Some(Op::Pass { target: t, draws, .. }) = self.plan.ops.last_mut()
            && *t == target
        {
            if let Some(last) = draws.last_mut()
                && last.pipe == draw.pipe
                && last.bind == draw.bind
                && last.scissor == draw.scissor
                && last.uniform == draw.uniform
                && last.verts.end == draw.verts.start
            {
                last.verts.end = draw.verts.end;
            } else {
                draws.push(draw);
            }
            return;
        }
        self.plan.ops.push(Op::Pass {
            target,
            clear: false,
            draws: vec![draw],
        });
    }

    fn vertex_index(&self) -> u32 {
        u32::try_from(self.plan.verts.len()).unwrap_or(u32::MAX)
    }

    /// A quad of corners `p` (top-left, top-right, bottom-left, bottom-right) in target px.
    pub fn quad_points(&mut self, p: [[f32; 2]; 4], uv: [f32; 4], color: [f32; 4]) -> Range<u32> {
        let start = self.vertex_index();
        let [u0, v0, u1, v1] = uv;
        let corner = |i: usize, u: f32, v: f32, local: [f32; 2]| Vertex {
            pos: p.get(i).copied().unwrap_or_default(),
            uv: [u, v],
            color,
            local,
        };
        let tl = corner(0, u0, v0, [0.0, 0.0]);
        let tr = corner(1, u1, v0, [1.0, 0.0]);
        let bl = corner(2, u0, v1, [0.0, 1.0]);
        let br = corner(3, u1, v1, [1.0, 1.0]);
        self.plan.verts.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        start..self.vertex_index()
    }

    /// `rect` transformed by `xf`.
    pub fn quad(&mut self, xf: &Affine, rect: &Rect, uv: [f32; 4], color: [f32; 4]) -> Range<u32> {
        let p = [
            apply(xf, rect.x, rect.y),
            apply(xf, rect.x + rect.w, rect.y),
            apply(xf, rect.x, rect.y + rect.h),
            apply(xf, rect.x + rect.w, rect.y + rect.h),
        ];
        self.quad_points(p, uv, color)
    }

    /// The whole target, uv 0..1 (layer composites).
    pub fn full_quad(&mut self, color: [f32; 4]) -> Range<u32> {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: self.size.0 as f32,
            h: self.size.1 as f32,
        };
        self.quad(&Affine::IDENTITY, &rect, [0.0, 0.0, 1.0, 1.0], color)
    }

    /// Composites layer `src` onto `target` with `color` (premultiplied multiplier).
    pub fn composite(&mut self, src: LayerId, target: Target, pipe: Pipe, color: [f32; 4], scissor: PxRect) {
        let verts = self.full_quad(color);
        let base = self.base;
        self.draw(target, pipe, BindKey::Layer(src), verts, scissor, base);
    }

    pub fn node(&mut self, node: &Node, ctx: Ctx) {
        match node {
            Node::Group {
                transform,
                opacity,
                blend,
                clip,
                children,
            } => self.group(ctx, transform, *opacity, *blend, clip.as_ref(), children),
            Node::Image {
                rect,
                image,
                uv,
                flip_x,
                flip_y,
                tint,
            } => {
                let color = scale(*tint, ctx.alpha);
                if color[3] <= 0.0 && color[0] <= 0.0 && color[1] <= 0.0 && color[2] <= 0.0 {
                    return;
                }
                if self
                    .textures
                    .image(self.gfx, self.assets, *image, self.max_dim)
                    .is_none()
                {
                    return;
                }
                let (mut u0, mut v0, mut u1, mut v1) = (uv.x, uv.y, uv.x + uv.w, uv.y + uv.h);
                if *flip_x {
                    std::mem::swap(&mut u0, &mut u1);
                }
                if *flip_y {
                    std::mem::swap(&mut v0, &mut v1);
                }
                let verts = self.quad(&ctx.xf, rect, [u0, v0, u1, v1], color);
                let base = self.base;
                self.draw(ctx.target, Pipe::Image, BindKey::Image(*image), verts, ctx.scissor, base);
            }
            Node::Fill { rect, color } => {
                let color = scale(*color, ctx.alpha);
                let [u, v] = self.atlas.white_uv();
                let verts = self.quad(&ctx.xf, rect, [u, v, u, v], color);
                let base = self.base;
                self.draw(ctx.target, Pipe::Mask, BindKey::Atlas(0), verts, ctx.scissor, base);
            }
            Node::NineSlice {
                rect,
                image,
                insets,
                tint,
            } => self.nine_slice(ctx, rect, *image, insets, scale(*tint, ctx.alpha)),
            Node::Text {
                origin,
                layout,
                reveal,
                opacity,
            } => self.text(ctx, *origin, layout, *reveal, *opacity),
            Node::Transition {
                bounds,
                kind,
                progress,
                from,
                to,
            } => self.transition(ctx, bounds, kind, *progress, from, to),
            Node::Still { rect } => {
                if !self.has_still {
                    return;
                }
                let verts = self.quad(&ctx.xf, rect, [0.0, 0.0, 1.0, 1.0], [ctx.alpha; 4]);
                let base = self.base;
                self.draw(ctx.target, Pipe::Linear, BindKey::Still, verts, ctx.scissor, base);
            }
            Node::Thumbnail {
                rect,
                slot,
                generation,
            } => {
                if self.textures.thumb(*slot).map(|(g, _)| g) != Some(*generation) {
                    return;
                }
                let verts = self.quad(&ctx.xf, rect, [0.0, 0.0, 1.0, 1.0], [ctx.alpha; 4]);
                let base = self.base;
                self.draw(ctx.target, Pipe::Image, BindKey::Thumb(*slot), verts, ctx.scissor, base);
            }
        }
    }

    fn group(
        &mut self,
        ctx: Ctx,
        transform: &Affine,
        opacity: f32,
        blend: Blend,
        clip: Option<&Rect>,
        children: &[Node],
    ) {
        let alpha = ctx.alpha * opacity;
        if alpha <= 0.0 {
            return;
        }
        let scissor = match clip {
            Some(rect) => ctx.scissor.intersect(&transformed_bounds(&ctx.xf, rect)),
            None => ctx.scissor,
        };
        if scissor.is_empty() {
            return;
        }
        let xf = compose(&ctx.xf, transform);
        if blend == Blend::Normal && children.len() <= 1 {
            let inner = Ctx {
                xf,
                alpha,
                scissor,
                target: ctx.target,
            };
            for child in children {
                self.node(child, inner);
            }
            return;
        }
        let layer = self.acquire();
        self.begin_pass(Target::Layer(layer));
        let inner = Ctx {
            xf,
            alpha: 1.0,
            scissor,
            target: Target::Layer(layer),
        };
        for child in children {
            self.node(child, inner);
        }
        match blend {
            Blend::Normal => self.composite(layer, ctx.target, Pipe::Linear, [alpha; 4], scissor),
            Blend::Add => self.composite(layer, ctx.target, Pipe::LinearAdd, [alpha; 4], scissor),
            mode => {
                let backdrop = self.acquire();
                self.copy(ctx.target, backdrop, scissor);
                let uniform = self.params([blend_index(mode), alpha, 0.0, 0.0], [0.0; 4], [0.0; 4]);
                let verts = self.full_quad([1.0; 4]);
                self.draw(
                    ctx.target,
                    Pipe::Blend,
                    BindKey::Blend {
                        src: layer,
                        backdrop,
                    },
                    verts,
                    scissor,
                    uniform,
                );
                self.layers.release(backdrop);
            }
        }
        self.layers.release(layer);
    }

    fn nine_slice(&mut self, ctx: Ctx, rect: &Rect, image: AssetId, insets: &[f32; 4], color: [f32; 4]) {
        let Some(entry) = self.textures.image(self.gfx, self.assets, image, self.max_dim) else {
            return;
        };
        let (iw, ih) = (entry.width as f32, entry.height as f32);
        let [t, r, b, l] = insets.map(|v| v.max(0.0));
        let (mut dl, mut dr, mut dt, mut db) = (l, r, t, b);
        if dl + dr > rect.w && dl + dr > 0.0 {
            let k = rect.w.max(0.0) / (dl + dr);
            dl *= k;
            dr *= k;
        }
        if dt + db > rect.h && dt + db > 0.0 {
            let k = rect.h.max(0.0) / (dt + db);
            dt *= k;
            db *= k;
        }
        let xs = [rect.x, rect.x + dl, rect.x + rect.w - dr, rect.x + rect.w];
        let ys = [rect.y, rect.y + dt, rect.y + rect.h - db, rect.y + rect.h];
        let us = [0.0, l / iw, 1.0 - r / iw, 1.0];
        let vs = [0.0, t / ih, 1.0 - b / ih, 1.0];
        let base = self.base;
        for row in 0..3 {
            for col in 0..3 {
                let cell = |a: &[f32; 4], i: usize| {
                    (
                        a.get(i).copied().unwrap_or(0.0),
                        a.get(i.saturating_add(1)).copied().unwrap_or(0.0),
                    )
                };
                let (x0, x1) = cell(&xs, col);
                let (y0, y1) = cell(&ys, row);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let (u0, u1) = cell(&us, col);
                let (v0, v1) = cell(&vs, row);
                let cell_rect = Rect {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                };
                let verts = self.quad(&ctx.xf, &cell_rect, [u0, v0, u1, v1], color);
                self.draw(ctx.target, Pipe::Image, BindKey::Image(image), verts, ctx.scissor, base);
            }
        }
    }

    fn transition(
        &mut self,
        ctx: Ctx,
        bounds: &Rect,
        kind: &TransitionDraw,
        progress: f32,
        from: &[Node],
        to: &[Node],
    ) {
        if ctx.alpha <= 0.0 {
            return;
        }
        let scissor = ctx.scissor.intersect(&transformed_bounds(&ctx.xf, bounds));
        if scissor.is_empty() {
            return;
        }
        let a = self.acquire();
        let b = self.acquire();
        for (layer, nodes) in [(a, from), (b, to)] {
            self.begin_pass(Target::Layer(layer));
            let inner = Ctx {
                xf: ctx.xf,
                alpha: 1.0,
                scissor,
                target: Target::Layer(layer),
            };
            for node in nodes {
                self.node(node, inner);
            }
        }
        let p = if progress.is_nan() { 0.0 } else { progress.clamp(0.0, 1.0) };
        let mut mask = None;
        let (pa, pb, pc) = match *kind {
            TransitionDraw::Crossfade => ([0.0, p, 0.0, 0.0], [0.0; 4], [0.0; 4]),
            TransitionDraw::Rule {
                mask: id,
                softness,
                reverse,
            } => {
                if self.textures.rule_mask(self.gfx, self.assets, id, self.max_dim) {
                    mask = Some(id);
                    let rev = if reverse { 1.0 } else { 0.0 };
                    ([1.0, p, f32::from(softness), rev], [0.0; 4], [0.0; 4])
                } else {
                    ([0.0, p, 0.0, 0.0], [0.0; 4], [0.0; 4])
                }
            }
            TransitionDraw::Wipe { dir } => ([2.0, p, 0.0, 0.0], [dir_index(dir), 0.0, 0.0, 0.0], [0.0; 4]),
            TransitionDraw::Blinds { dir, count } => (
                [3.0, p, 0.0, 0.0],
                [dir_index(dir), count.max(1) as f32, 0.0, 0.0],
                [0.0; 4],
            ),
            TransitionDraw::ThroughColor {
                color,
                hold_fraction,
            } => (
                [4.0, p, 0.0, 0.0],
                [0.0, 0.0, hold_fraction.clamp(0.0, 1.0), 0.0],
                color,
            ),
        };
        let uniform = self.params(pa, pb, pc);
        let verts = self.quad(&ctx.xf, bounds, [0.0, 0.0, 1.0, 1.0], [ctx.alpha; 4]);
        self.draw(
            ctx.target,
            Pipe::Transition,
            BindKey::Transition { from: a, to: b, mask },
            verts,
            scissor,
            uniform,
        );
        self.layers.release(a);
        self.layers.release(b);
    }
}
