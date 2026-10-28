//! The first phase of a frame: one walk over the node tree that produces the vertices, the
//! uniform slots, the shader arguments, the motion compute dispatches and the ordered list of
//! passes and copies.

use std::collections::HashMap;
use std::ops::Range;

use kagari_bytecode::AssetId;
use kagari_engine::{
    Affine, AssetStore, Dir, MotionStage, Node, Pose, Rect, ShaderEffect, ThumbKey, TransitionDraw,
};
use kagari_shader::{MOTION_SLOTS, ShaderRole};

use crate::Gfx;
use crate::atlas::GlyphAtlas;
use crate::layers::{LayerId, LayerPool};
use crate::pipelines::{MotionUniform, Pipe, Uniforms, Vertex};
use crate::shaders::ShaderCache;
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

/// The third texture of a transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TransitionMask {
    /// The white texture (no mask).
    None,
    /// A rule mask image.
    Image(AssetId),
    /// A layer a mask shader was drawn into.
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
    /// Two layers (mix source and backdrop; masked content and mask).
    Pair {
        a: LayerId,
        b: LayerId,
    },
    Blur {
        src: LayerId,
    },
    Transition {
        from: LayerId,
        to: LayerId,
        mask: TransitionMask,
    },
    /// The empty group of mask shaders.
    Empty,
    /// The white texture (an unmasked paint).
    White,
}

/// The pipeline of a draw: a built-in one or a user shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DrawPipe {
    Fixed(Pipe),
    User(AssetId, ShaderRole),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draw {
    pub pipe: DrawPipe,
    pub bind: BindKey,
    pub verts: Range<u32>,
    pub scissor: PxRect,
    /// Byte offset of the uniform slot.
    pub uniform: u32,
    /// Byte offset of the shader arguments (user shaders only).
    pub args: Option<u32>,
}

/// One motion stage dispatch of the frame's compute pass.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComputeOp {
    pub shader: AssetId,
    pub role: ShaderRole,
    /// Byte offset of the `MotionUniform`.
    pub uniform: u32,
    /// Byte offset of the shader arguments.
    pub args: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    Pass {
        target: Target,
        clear: bool,
        draws: Vec<Draw>,
    },
    /// `rect` of `from` into the same pixels of `to`.
    Copy {
        from: Target,
        to: LayerId,
        rect: PxRect,
    },
}

/// The output of the planning phase.
#[derive(Default)]
pub(crate) struct Plan {
    pub verts: Vec<Vertex>,
    pub uniforms: Vec<u8>,
    /// Packed shader arguments, each at a storage-offset-aligned position.
    pub args: Vec<u8>,
    /// Motion stages, dispatched in order before the first pass.
    pub compute: Vec<ComputeOp>,
    pub ops: Vec<Op>,
}

/// The traversal state inherited by a subtree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ctx {
    pub xf: Affine,
    pub alpha: f32,
    pub scissor: PxRect,
    pub target: Target,
    /// The motion displacement slot of geometry (0 = identity).
    pub slot: u32,
    /// Inside a `Motion`: geometry moves on the GPU, so clips and transition bounds computed on
    /// the CPU no longer narrow the scissor.
    pub free: bool,
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

/// `len` rounded up to a multiple of `align` (≥ 1).
pub(crate) fn align_up(len: usize, align: usize) -> usize {
    let align = align.max(1);
    len.div_ceil(align).saturating_mul(align)
}

/// `(x, y, scale, rotation)`.
fn pose_vec(p: &Pose) -> [f32; 4] {
    [p.position[0], p.position[1], p.scale, p.rotation]
}

/// The uniform of one motion stage in slot `slot` under the parent transform `xf`.
pub(crate) fn motion_uniform(
    xf: &Affine,
    pose: &Pose,
    stage: &MotionStage,
    slot: u32,
    chained: bool,
) -> MotionUniform {
    let [a, b, c, d, e, f] = xf.0;
    let (from, to, timing) = match stage {
        MotionStage::Local { effect } => ([0.0; 4], [0.0; 4], [effect.time, 0.0, 0.0, 0.0]),
        MotionStage::Transitional {
            effect,
            from,
            to,
            progress,
        } => (
            pose_vec(from),
            pose_vec(to),
            [effect.time, *progress, 0.0, 0.0],
        ),
    };
    MotionUniform {
        parent0: [a, c, e, 0.0],
        parent1: [b, d, f, 0.0],
        pose: pose_vec(pose),
        from,
        to,
        timing,
        slot,
        chained: u32::from(chained),
        pad: [0; 2],
    }
}

fn stage_effect(stage: &MotionStage) -> (&ShaderEffect, ShaderRole) {
    match stage {
        MotionStage::Local { effect } => (effect, ShaderRole::LocalMotion),
        MotionStage::Transitional { effect, .. } => (effect, ShaderRole::TransitionalMotion),
    }
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
    pub shaders: &'a mut ShaderCache,
    pub assets: &'a AssetStore,
    /// The design target size; every layer has it.
    pub size: (u32, u32),
    /// The uniform slot stride.
    pub slot: usize,
    /// The storage buffer offset alignment of shader arguments.
    pub args_align: usize,
    pub max_dim: u32,
    pub has_still: bool,
    pub plan: Plan,
    /// The slot holding only the target size (motion slot 0).
    pub base: u32,
    /// The uniform slots holding only the target size and a motion slot, by motion slot.
    bases: HashMap<u32, u32>,
    /// The next free motion slot.
    next_motion: u32,
    motion_warned: bool,
}

impl<'a> Planner<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gfx: &'a Gfx,
        textures: &'a mut TextureCache,
        atlas: &'a mut GlyphAtlas,
        layers: &'a mut LayerPool,
        shaders: &'a mut ShaderCache,
        assets: &'a AssetStore,
        size: (u32, u32),
        slot: usize,
        has_still: bool,
    ) -> Planner<'a> {
        let limits = gfx.device.limits();
        let mut planner = Planner {
            gfx,
            textures,
            atlas,
            layers,
            shaders,
            assets,
            size,
            slot,
            args_align: usize::try_from(limits.min_storage_buffer_offset_alignment)
                .unwrap_or(256)
                .max(4),
            max_dim: limits.max_texture_dimension_2d,
            has_still,
            plan: Plan::default(),
            base: 0,
            bases: HashMap::new(),
            next_motion: 1,
            motion_warned: false,
        };
        planner.base = planner.uniform(Uniforms::new(size));
        planner.bases.insert(0, planner.base);
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
            slot: 0,
            free: false,
        };
        for node in nodes {
            self.node(node, ctx);
        }
        self.plan
    }

    /// Appends a uniform slot holding `bytes`; returns its byte offset.
    pub fn uniform_bytes(&mut self, bytes: &[u8]) -> u32 {
        let offset = self.plan.uniforms.len();
        self.plan.uniforms.extend_from_slice(bytes);
        self.plan
            .uniforms
            .resize(offset.saturating_add(self.slot), 0);
        u32::try_from(offset).unwrap_or(u32::MAX)
    }

    /// Appends a uniform slot; returns its byte offset.
    pub fn uniform(&mut self, u: Uniforms) -> u32 {
        self.uniform_bytes(bytemuck::bytes_of(&u))
    }

    /// The uniform slot of plain geometry in motion slot `slot`.
    pub fn base_for(&mut self, slot: u32) -> u32 {
        if let Some(&offset) = self.bases.get(&slot) {
            return offset;
        }
        let mut u = Uniforms::new(self.size);
        u.xf_slot = slot;
        let offset = self.uniform(u);
        self.bases.insert(slot, offset);
        offset
    }

    /// Uniforms with the target size, motion slot `slot` and the given parameters.
    pub fn params(&mut self, slot: u32, a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> u32 {
        let mut u = Uniforms::new(self.size);
        u.xf_slot = slot;
        u.a = a;
        u.b = b;
        u.c = c;
        self.uniform(u)
    }

    /// Appends packed shader arguments at an aligned offset; returns the offset.
    pub fn args(&mut self, bytes: &[u8]) -> u32 {
        let offset = align_up(self.plan.args.len(), self.args_align);
        self.plan.args.resize(offset, 0);
        self.plan.args.extend_from_slice(bytes);
        let end = align_up(self.plan.args.len(), 4);
        self.plan.args.resize(end, 0);
        u32::try_from(offset).unwrap_or(u32::MAX)
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

    /// Records a draw with a built-in pipeline.
    pub fn draw(
        &mut self,
        target: Target,
        pipe: Pipe,
        bind: BindKey,
        verts: Range<u32>,
        scissor: PxRect,
        uniform: u32,
    ) {
        self.push(
            target,
            Draw {
                pipe: DrawPipe::Fixed(pipe),
                bind,
                verts,
                scissor,
                uniform,
                args: None,
            },
        );
    }

    /// Records a draw into `target`, merging it into the previous draw when possible.
    pub fn push(&mut self, target: Target, draw: Draw) {
        if draw.scissor.is_empty() || draw.verts.is_empty() {
            return;
        }
        if let Some(Op::Pass {
            target: t, draws, ..
        }) = self.plan.ops.last_mut()
            && *t == target
        {
            if let Some(last) = draws.last_mut()
                && last.pipe == draw.pipe
                && last.bind == draw.bind
                && last.scissor == draw.scissor
                && last.uniform == draw.uniform
                && last.args == draw.args
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
        self.quad_flipped(xf, rect, false, false, uv, color)
    }

    /// `rect` transformed by `xf`, its corners mirrored horizontally / vertically (so `uv` and
    /// the local coordinates flip with them).
    fn quad_flipped(
        &mut self,
        xf: &Affine,
        rect: &Rect,
        flip_x: bool,
        flip_y: bool,
        uv: [f32; 4],
        color: [f32; 4],
    ) -> Range<u32> {
        let (mut x0, mut x1) = (rect.x, rect.x + rect.w);
        let (mut y0, mut y1) = (rect.y, rect.y + rect.h);
        if flip_x {
            std::mem::swap(&mut x0, &mut x1);
        }
        if flip_y {
            std::mem::swap(&mut y0, &mut y1);
        }
        let p = [
            apply(xf, x0, y0),
            apply(xf, x1, y0),
            apply(xf, x0, y1),
            apply(xf, x1, y1),
        ];
        self.quad_points(p, uv, color)
    }

    /// The whole target, uv 0..1 (layer composites).
    pub fn full_quad(&mut self, color: [f32; 4]) -> Range<u32> {
        let rect = self.full_rect();
        self.quad(&Affine::IDENTITY, &rect, [0.0, 0.0, 1.0, 1.0], color)
    }

    fn full_rect(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: self.size.0 as f32,
            h: self.size.1 as f32,
        }
    }

    /// Composites layer `src` onto `target` with `color` (premultiplied multiplier).
    pub fn composite(
        &mut self,
        src: LayerId,
        target: Target,
        pipe: Pipe,
        color: [f32; 4],
        scissor: PxRect,
    ) {
        let verts = self.full_quad(color);
        let base = self.base;
        self.draw(target, pipe, BindKey::Layer(src), verts, scissor, base);
    }

    /// Prepares the pipeline of `effect` under `role`; whether it can be drawn.
    fn available(&mut self, effect: &ShaderEffect, role: ShaderRole) -> bool {
        self.shaders
            .prepare(self.gfx, self.assets, effect.shader, role)
    }

    /// Records a user shader draw of `effect`.
    #[allow(clippy::too_many_arguments)]
    fn shader_draw(
        &mut self,
        target: Target,
        effect: &ShaderEffect,
        role: ShaderRole,
        bind: BindKey,
        verts: Range<u32>,
        scissor: PxRect,
        uniform: u32,
    ) {
        let args = self.args(&effect.args);
        self.push(
            target,
            Draw {
                pipe: DrawPipe::User(effect.shader, role),
                bind,
                verts,
                scissor,
                uniform,
                args: Some(args),
            },
        );
    }

    /// Draws mask shader `effect` over `rect` (under `ctx.xf`, in `ctx.slot`) into a new cleared
    /// layer, or `None` when the shader is unavailable.
    fn mask_layer(
        &mut self,
        ctx: &Ctx,
        effect: &ShaderEffect,
        rect: &Rect,
        flip: (bool, bool),
    ) -> Option<LayerId> {
        if !self.available(effect, ShaderRole::Mask) {
            return None;
        }
        let layer = self.acquire();
        self.begin_pass(Target::Layer(layer));
        let uniform = self.params(
            ctx.slot,
            [effect.time, 0.0, 0.0, 0.0],
            [0.0, 0.0, rect.w, rect.h],
            [0.0; 4],
        );
        let verts = self.quad_flipped(
            &ctx.xf,
            rect,
            flip.0,
            flip.1,
            [0.0, 0.0, 1.0, 1.0],
            [1.0; 4],
        );
        self.shader_draw(
            Target::Layer(layer),
            effect,
            ShaderRole::Mask,
            BindKey::Empty,
            verts,
            ctx.scissor,
            uniform,
        );
        Some(layer)
    }

    /// Plans `children` into a new cleared layer at alpha 1 (same transform, slot and scissor).
    fn isolate(&mut self, ctx: &Ctx, scissor: PxRect, xf: Affine, children: &[Node]) -> LayerId {
        let layer = self.acquire();
        self.begin_pass(Target::Layer(layer));
        let inner = Ctx {
            xf,
            alpha: 1.0,
            scissor,
            target: Target::Layer(layer),
            ..*ctx
        };
        for child in children {
            self.node(child, inner);
        }
        layer
    }

    pub fn node(&mut self, node: &Node, ctx: Ctx) {
        match node {
            Node::Group {
                transform,
                opacity,
                clip,
                children,
            } => self.group(ctx, transform, *opacity, clip.as_ref(), children),
            Node::Filter { effect, children } => self.filter(ctx, effect, children),
            Node::Mix {
                effect,
                opacity,
                children,
            } => self.mix(ctx, effect, *opacity, children),
            Node::Masked {
                effect,
                rect,
                children,
            } => self.masked(ctx, effect, rect, children),
            Node::Motion {
                stages,
                pose,
                children,
            } => self.motion(ctx, stages, pose, children),
            Node::Paint {
                rect,
                texture,
                mask,
                flip_x,
                flip_y,
            } => self.paint(ctx, rect, texture, mask.as_ref(), (*flip_x, *flip_y)),
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
                let base = self.base_for(ctx.slot);
                self.draw(
                    ctx.target,
                    Pipe::Image,
                    BindKey::Image(*image),
                    verts,
                    ctx.scissor,
                    base,
                );
            }
            Node::Fill { rect, color } => {
                let color = scale(*color, ctx.alpha);
                let [u, v] = self.atlas.white_uv();
                let verts = self.quad(&ctx.xf, rect, [u, v, u, v], color);
                let base = self.base_for(ctx.slot);
                self.draw(
                    ctx.target,
                    Pipe::Mask,
                    BindKey::Atlas(0),
                    verts,
                    ctx.scissor,
                    base,
                );
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
                let base = self.base_for(ctx.slot);
                self.draw(
                    ctx.target,
                    Pipe::Linear,
                    BindKey::Still,
                    verts,
                    ctx.scissor,
                    base,
                );
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
                let base = self.base_for(ctx.slot);
                self.draw(
                    ctx.target,
                    Pipe::Image,
                    BindKey::Thumb(*slot),
                    verts,
                    ctx.scissor,
                    base,
                );
            }
        }
    }

    fn group(
        &mut self,
        ctx: Ctx,
        transform: &Affine,
        opacity: f32,
        clip: Option<&Rect>,
        children: &[Node],
    ) {
        let alpha = ctx.alpha * opacity;
        if alpha <= 0.0 {
            return;
        }
        let scissor = match clip {
            Some(rect) if !ctx.free => ctx.scissor.intersect(&transformed_bounds(&ctx.xf, rect)),
            _ => ctx.scissor,
        };
        if scissor.is_empty() {
            return;
        }
        let xf = compose(&ctx.xf, transform);
        if children.len() <= 1 {
            let inner = Ctx {
                xf,
                alpha,
                scissor,
                ..ctx
            };
            for child in children {
                self.node(child, inner);
            }
            return;
        }
        let layer = self.isolate(&ctx, scissor, xf, children);
        self.composite(layer, ctx.target, Pipe::Linear, [alpha; 4], scissor);
        self.layers.release(layer);
    }

    /// `Node::Filter`: the children in a layer, then the colour shader over the whole target.
    fn filter(&mut self, ctx: Ctx, effect: &ShaderEffect, children: &[Node]) {
        if ctx.alpha <= 0.0 || ctx.scissor.is_empty() {
            return;
        }
        let layer = self.isolate(&ctx, ctx.scissor, ctx.xf, children);
        if self.available(effect, ShaderRole::Color) {
            let (w, h) = (self.size.0 as f32, self.size.1 as f32);
            let uniform = self.params(0, [effect.time, 0.0, 0.0, 0.0], [0.0, 0.0, w, h], [0.0; 4]);
            let verts = self.full_quad([ctx.alpha; 4]);
            self.shader_draw(
                ctx.target,
                effect,
                ShaderRole::Color,
                BindKey::Layer(layer),
                verts,
                ctx.scissor,
                uniform,
            );
        } else {
            self.composite(layer, ctx.target, Pipe::Linear, [ctx.alpha; 4], ctx.scissor);
        }
        self.layers.release(layer);
    }

    /// `Node::Mix`: the children in a layer, a copy of the backdrop, then the mix shader, which
    /// replaces the target's pixels.
    fn mix(&mut self, ctx: Ctx, effect: &ShaderEffect, opacity: f32, children: &[Node]) {
        let alpha = ctx.alpha * opacity;
        if alpha <= 0.0 || ctx.scissor.is_empty() {
            return;
        }
        let layer = self.isolate(&ctx, ctx.scissor, ctx.xf, children);
        if self.available(effect, ShaderRole::Mix) {
            let backdrop = self.acquire();
            self.copy(ctx.target, backdrop, ctx.scissor);
            let (w, h) = (self.size.0 as f32, self.size.1 as f32);
            let uniform = self.params(
                0,
                [effect.time, 0.0, alpha, 0.0],
                [0.0, 0.0, w, h],
                [0.0; 4],
            );
            let verts = self.full_quad([1.0; 4]);
            self.shader_draw(
                ctx.target,
                effect,
                ShaderRole::Mix,
                BindKey::Pair {
                    a: layer,
                    b: backdrop,
                },
                verts,
                ctx.scissor,
                uniform,
            );
            self.layers.release(backdrop);
        } else {
            self.composite(layer, ctx.target, Pipe::Linear, [alpha; 4], ctx.scissor);
        }
        self.layers.release(layer);
    }

    /// `Node::Masked`: the children in a layer, the mask shader over `rect` in another, then the
    /// first times the second's red channel.
    fn masked(&mut self, ctx: Ctx, effect: &ShaderEffect, rect: &Rect, children: &[Node]) {
        if ctx.alpha <= 0.0 || ctx.scissor.is_empty() {
            return;
        }
        let content = self.isolate(&ctx, ctx.scissor, ctx.xf, children);
        match self.mask_layer(&ctx, effect, rect, (false, false)) {
            Some(mask) => {
                let verts = self.full_quad([ctx.alpha; 4]);
                let base = self.base;
                self.draw(
                    ctx.target,
                    Pipe::MaskComposite,
                    BindKey::Pair {
                        a: content,
                        b: mask,
                    },
                    verts,
                    ctx.scissor,
                    base,
                );
                self.layers.release(mask);
            }
            None => self.composite(
                content,
                ctx.target,
                Pipe::Linear,
                [ctx.alpha; 4],
                ctx.scissor,
            ),
        }
        self.layers.release(content);
    }

    /// `Node::Paint`: the texture shader over `rect`, times its mask shader when it has one.
    fn paint(
        &mut self,
        ctx: Ctx,
        rect: &Rect,
        texture: &ShaderEffect,
        mask: Option<&ShaderEffect>,
        flip: (bool, bool),
    ) {
        if ctx.alpha <= 0.0
            || ctx.scissor.is_empty()
            || !self.available(texture, ShaderRole::Texture)
        {
            return;
        }
        let mask = mask.and_then(|m| self.mask_layer(&ctx, m, rect, flip));
        let uniform = self.params(
            ctx.slot,
            [texture.time, 0.0, 0.0, 0.0],
            [0.0, 0.0, rect.w, rect.h],
            [0.0; 4],
        );
        let verts = self.quad_flipped(
            &ctx.xf,
            rect,
            flip.0,
            flip.1,
            [0.0, 0.0, 1.0, 1.0],
            [ctx.alpha; 4],
        );
        let bind = match mask {
            Some(layer) => BindKey::Layer(layer),
            None => BindKey::White,
        };
        self.shader_draw(
            ctx.target,
            texture,
            ShaderRole::Texture,
            bind,
            verts,
            ctx.scissor,
            uniform,
        );
        if let Some(layer) = mask {
            self.layers.release(layer);
        }
    }

    /// `Node::Motion`: one compute dispatch per available stage writes the displacement of a new
    /// motion slot, which the children are drawn in.
    fn motion(&mut self, ctx: Ctx, stages: &[MotionStage], pose: &Pose, children: &[Node]) {
        let mut available = Vec::with_capacity(stages.len());
        for stage in stages {
            let (effect, role) = stage_effect(stage);
            if self.available(effect, role) {
                available.push(stage);
            }
        }
        if available.is_empty() || self.next_motion >= MOTION_SLOTS {
            if !available.is_empty() && !self.motion_warned {
                self.motion_warned = true;
                log::warn!(
                    "more than {} motion shaders in one frame; the rest are not moved",
                    MOTION_SLOTS.saturating_sub(1)
                );
            }
            for child in children {
                self.node(child, ctx);
            }
            return;
        }
        let slot = self.next_motion;
        self.next_motion = slot.saturating_add(1);
        for (i, stage) in available.into_iter().enumerate() {
            let (effect, role) = stage_effect(stage);
            let u = motion_uniform(&ctx.xf, pose, stage, slot, i > 0);
            let uniform = self.uniform_bytes(bytemuck::bytes_of(&u));
            let args = self.args(&effect.args);
            self.plan.compute.push(ComputeOp {
                shader: effect.shader,
                role,
                uniform,
                args,
            });
        }
        let inner = Ctx {
            slot,
            free: true,
            ..ctx
        };
        for child in children {
            self.node(child, inner);
        }
    }

    fn nine_slice(
        &mut self,
        ctx: Ctx,
        rect: &Rect,
        image: AssetId,
        insets: &[f32; 4],
        color: [f32; 4],
    ) {
        let Some(entry) = self
            .textures
            .image(self.gfx, self.assets, image, self.max_dim)
        else {
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
        let base = self.base_for(ctx.slot);
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
                self.draw(
                    ctx.target,
                    Pipe::Image,
                    BindKey::Image(image),
                    verts,
                    ctx.scissor,
                    base,
                );
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
        let scissor = if ctx.free {
            ctx.scissor
        } else {
            ctx.scissor.intersect(&transformed_bounds(&ctx.xf, bounds))
        };
        if scissor.is_empty() {
            return;
        }
        let a = self.isolate(&ctx, scissor, ctx.xf, from);
        let b = self.isolate(&ctx, scissor, ctx.xf, to);
        let p = if progress.is_nan() {
            0.0
        } else {
            progress.clamp(0.0, 1.0)
        };
        let mut mask = TransitionMask::None;
        let crossfade = ([0.0, p, 0.0, 0.0], [0.0; 4], [0.0; 4]);
        let (pa, pb, pc) = match kind {
            TransitionDraw::Crossfade => crossfade,
            TransitionDraw::Rule {
                mask: id,
                softness,
                reverse,
            } => {
                if self
                    .textures
                    .rule_mask(self.gfx, self.assets, *id, self.max_dim)
                {
                    mask = TransitionMask::Image(*id);
                    let rev = if *reverse { 1.0 } else { 0.0 };
                    ([1.0, p, f32::from(*softness), rev], [0.0; 4], [0.0; 4])
                } else {
                    crossfade
                }
            }
            TransitionDraw::Wipe { dir } => (
                [2.0, p, 0.0, 0.0],
                [dir_index(*dir), 0.0, 0.0, 0.0],
                [0.0; 4],
            ),
            TransitionDraw::Blinds { dir, count } => (
                [3.0, p, 0.0, 0.0],
                [dir_index(*dir), (*count).max(1) as f32, 0.0, 0.0],
                [0.0; 4],
            ),
            TransitionDraw::ThroughColor {
                color,
                hold_fraction,
            } => (
                [4.0, p, 0.0, 0.0],
                [0.0, 0.0, hold_fraction.clamp(0.0, 1.0), 0.0],
                *color,
            ),
            TransitionDraw::Masked {
                mask: effect,
                softness,
                reverse,
            } => match self.mask_layer(&ctx, effect, bounds, (false, false)) {
                Some(layer) => {
                    mask = TransitionMask::Layer(layer);
                    let rev = if *reverse { 1.0 } else { 0.0 };
                    ([5.0, p, f32::from(*softness), rev], [0.0; 4], [0.0; 4])
                }
                None => crossfade,
            },
        };
        let uniform = self.params(ctx.slot, pa, pb, pc);
        let verts = self.quad(&ctx.xf, bounds, [0.0, 0.0, 1.0, 1.0], [ctx.alpha; 4]);
        self.draw(
            ctx.target,
            Pipe::Transition,
            BindKey::Transition {
                from: a,
                to: b,
                mask,
            },
            verts,
            scissor,
            uniform,
        );
        if let TransitionMask::Layer(layer) = mask {
            self.layers.release(layer);
        }
        self.layers.release(a);
        self.layers.release(b);
    }
}
