//! The second phase of a frame: uploads the planned vertices, uniforms and shader arguments, then
//! records the motion compute pass and every render pass and copy into one command encoder.

use std::collections::HashMap;

use kagari_shader::{MAX_ARGS_BYTES, STATES_BYTES, TRANSFORMS_BYTES};

use crate::Gfx;
use crate::atlas::GlyphAtlas;
use crate::layers::LayerPool;
use crate::pipelines::{MOTION_UNIFORM_SIZE, UNIFORM_SIZE};
use crate::plan::{BindKey, DrawPipe, Op, Plan, PxRect, Target, TransitionMask};
use crate::shaders::ShaderCache;
use crate::textures::{Still, TextureCache};

const INITIAL_VERTEX_BYTES: u64 = 64 * 1024;

/// The identity displacement of motion slot 0: rows `(1, 0, 0, 0)` and `(0, 1, 0, 0)`.
const IDENTITY_ROWS: [f32; 8] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// The per-frame buffers and the bind groups over them: vertices, uniforms (group 0 together with
/// the motion displacement rows), shader arguments and the motion buffers.
pub(crate) struct FrameBuffers {
    vertex: wgpu::Buffer,
    uniform: wgpu::Buffer,
    /// The raster group 0: the uniforms and the displacement rows.
    pub bind: wgpu::BindGroup,
    /// Motion displacement rows (uniform for raster draws, storage for the motion pass).
    transforms: wgpu::Buffer,
    /// The shader arguments and their group-2 bind group (user shaders only).
    args: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    /// The motion pass's group 0 (over `uniform`) and group 1 (states and rows).
    motion: Option<(wgpu::BindGroup, wgpu::BindGroup)>,
}

fn buffer(gfx: &Gfx, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    gfx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn binding(buffer: &wgpu::Buffer, size: u64) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset: 0,
        size: wgpu::BufferSize::new(size),
    })
}

fn uniform_bind(gfx: &Gfx, uniform: &wgpu::Buffer, transforms: &wgpu::Buffer) -> wgpu::BindGroup {
    gfx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kagari uniforms"),
        layout: &gfx.pipes.uniforms,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: binding(uniform, UNIFORM_SIZE),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: binding(transforms, TRANSFORMS_BYTES),
            },
        ],
    })
}

fn args_buffer(gfx: &Gfx, size: u64) -> Option<(wgpu::Buffer, wgpu::BindGroup)> {
    let user = gfx.pipes.user.as_ref()?;
    let args = buffer(gfx, "kagari shader args", size, wgpu::BufferUsages::STORAGE);
    let bind = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kagari shader args"),
        layout: &user.args,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: binding(&args, u64::from(MAX_ARGS_BYTES)),
        }],
    });
    Some((args, bind))
}

fn motion_uniform_bind(gfx: &Gfx, uniform: &wgpu::Buffer) -> Option<wgpu::BindGroup> {
    let motion = gfx.pipes.user.as_ref()?.motion.as_ref()?;
    Some(gfx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kagari motion uniform"),
        layout: &motion.uniform,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: binding(uniform, MOTION_UNIFORM_SIZE),
        }],
    }))
}

/// Doubles `cap` until it holds `len`.
fn grown(cap: u64, len: u64) -> u64 {
    let mut cap = cap.max(1);
    while cap < len {
        cap = cap.saturating_mul(2);
    }
    cap
}

impl FrameBuffers {
    pub fn new(gfx: &Gfx, slot: u64) -> FrameBuffers {
        let uniform = buffer(
            gfx,
            "kagari uniforms",
            slot.saturating_mul(64),
            wgpu::BufferUsages::UNIFORM,
        );
        let motion_layouts = gfx.pipes.user.as_ref().and_then(|u| u.motion.as_ref());
        let mut usage = wgpu::BufferUsages::UNIFORM;
        if motion_layouts.is_some() {
            usage |= wgpu::BufferUsages::STORAGE;
        }
        let transforms = buffer(gfx, "kagari motion transforms", TRANSFORMS_BYTES, usage);
        gfx.queue
            .write_buffer(&transforms, 0, bytemuck::cast_slice(&IDENTITY_ROWS));
        let motion = match (motion_layouts, motion_uniform_bind(gfx, &uniform)) {
            (Some(layouts), Some(uniform_bind)) => {
                let states = buffer(
                    gfx,
                    "kagari motion states",
                    STATES_BYTES,
                    wgpu::BufferUsages::STORAGE,
                );
                let buffers = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("kagari motion buffers"),
                    layout: &layouts.buffers,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: binding(&states, STATES_BYTES),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: binding(&transforms, TRANSFORMS_BYTES),
                        },
                    ],
                });
                Some((uniform_bind, buffers))
            }
            _ => None,
        };
        FrameBuffers {
            vertex: buffer(
                gfx,
                "kagari vertices",
                INITIAL_VERTEX_BYTES,
                wgpu::BufferUsages::VERTEX,
            ),
            bind: uniform_bind(gfx, &uniform, &transforms),
            args: args_buffer(gfx, u64::from(MAX_ARGS_BYTES)),
            motion,
            transforms,
            uniform,
        }
    }

    /// Writes the plan's vertices, uniforms and arguments, growing each buffer by doubling. The
    /// argument buffer always holds `MAX_ARGS_BYTES` past the last offset, the binding size.
    pub fn upload(&mut self, gfx: &Gfx, plan: &Plan) {
        let verts: &[u8] = bytemuck::cast_slice(&plan.verts);
        let vlen = verts.len() as u64;
        if vlen > self.vertex.size() {
            self.vertex = buffer(
                gfx,
                "kagari vertices",
                grown(self.vertex.size(), vlen),
                wgpu::BufferUsages::VERTEX,
            );
        }
        let ulen = plan.uniforms.len() as u64;
        if ulen > self.uniform.size() {
            self.uniform = buffer(
                gfx,
                "kagari uniforms",
                grown(self.uniform.size(), ulen),
                wgpu::BufferUsages::UNIFORM,
            );
            self.bind = uniform_bind(gfx, &self.uniform, &self.transforms);
            if let (Some((uniform_bind, _)), Some(new)) =
                (&mut self.motion, motion_uniform_bind(gfx, &self.uniform))
            {
                *uniform_bind = new;
            }
        }
        let alen = (plan.args.len() as u64).saturating_add(u64::from(MAX_ARGS_BYTES));
        if let Some((args, _)) = &self.args
            && alen > args.size()
            && let Some(new) = args_buffer(gfx, grown(args.size(), alen))
        {
            self.args = Some(new);
        }
        if !verts.is_empty() {
            gfx.queue.write_buffer(&self.vertex, 0, verts);
        }
        if !plan.uniforms.is_empty() {
            gfx.queue.write_buffer(&self.uniform, 0, &plan.uniforms);
        }
        if let Some((args, _)) = &self.args
            && !plan.args.is_empty()
        {
            gfx.queue.write_buffer(args, 0, &plan.args);
        }
    }
}

/// The design target: its texture plus a bind group that samples it.
pub(crate) struct DesignTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind: wgpu::BindGroup,
    pub size: (u32, u32),
}

/// What encoding reads.
pub(crate) struct Resources<'a> {
    pub gfx: &'a Gfx,
    pub design: &'a DesignTarget,
    pub layers: &'a LayerPool,
    pub textures: &'a TextureCache,
    pub atlas: &'a GlyphAtlas,
    pub still: Option<&'a Still>,
    pub shaders: &'a ShaderCache,
    pub buffers: &'a FrameBuffers,
}

impl Resources<'_> {
    fn view(&self, target: Target) -> Option<&wgpu::TextureView> {
        match target {
            Target::Design => Some(&self.design.view),
            Target::Layer(id) => self.layers.view(id),
        }
    }

    fn texture(&self, target: Target) -> Option<&wgpu::Texture> {
        match target {
            Target::Design => Some(&self.design.texture),
            Target::Layer(id) => self.layers.texture(id),
        }
    }

    fn bind_group(&self, key: BindKey) -> Option<wgpu::BindGroup> {
        let gfx = self.gfx;
        let layers = |ids: &[crate::layers::LayerId]| -> Option<Vec<&wgpu::TextureView>> {
            ids.iter().map(|id| self.layers.view(*id)).collect()
        };
        match key {
            BindKey::Image(id) => self.textures.get_image(id).map(|e| e.bind.clone()),
            BindKey::Thumb(slot) => self.textures.thumb(slot).map(|(_, e)| e.bind.clone()),
            BindKey::Still => self.still.map(|s| s.bind.clone()),
            BindKey::Atlas(page) => self.atlas.bind(page).cloned(),
            BindKey::Layer(src) | BindKey::Blur { src } => {
                Some(gfx.pipes.textures(&gfx.device, &layers(&[src])?))
            }
            BindKey::Pair { a, b } => Some(gfx.pipes.textures(&gfx.device, &layers(&[a, b])?)),
            BindKey::Transition { from, to, mask } => {
                let mut views = layers(&[from, to])?;
                let mask = match mask {
                    TransitionMask::Image(id) => &self.textures.get_image(id)?.view,
                    TransitionMask::Layer(id) => self.layers.view(id)?,
                    TransitionMask::None => &self.textures.white.view,
                };
                views.push(mask);
                Some(gfx.pipes.textures(&gfx.device, &views))
            }
            BindKey::Empty => Some(gfx.pipes.empty_bind.clone()),
            BindKey::White => Some(self.textures.white.bind.clone()),
        }
    }
}

fn extent(rect: &PxRect) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: rect.w,
        height: rect.h,
        depth_or_array_layers: 1,
    }
}

/// Records the motion stages of `plan` in one compute pass.
fn encode_motion(res: &Resources<'_>, plan: &Plan, encoder: &mut wgpu::CommandEncoder) {
    let (Some((uniform, buffers)), Some((_, args))) = (&res.buffers.motion, &res.buffers.args)
    else {
        return;
    };
    if plan.compute.is_empty() {
        return;
    }
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("kagari motion"),
        timestamp_writes: None,
    });
    for op in &plan.compute {
        let Some(pipeline) = res.shaders.compute(op.shader, op.role) else {
            continue;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, uniform, &[op.uniform]);
        pass.set_bind_group(1, buffers, &[]);
        pass.set_bind_group(2, args, &[op.args]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

/// Records every op of `plan` into `encoder`: the motion pass first, then the passes and copies.
pub(crate) fn encode(res: &Resources<'_>, plan: &Plan, encoder: &mut wgpu::CommandEncoder) {
    encode_motion(res, plan, encoder);
    let mut binds: HashMap<BindKey, Option<wgpu::BindGroup>> = HashMap::new();
    for op in &plan.ops {
        match op {
            Op::Pass {
                target,
                clear,
                draws,
            } => {
                let Some(view) = res.view(*target) else {
                    log::error!("render target {target:?} is gone; pass skipped");
                    continue;
                };
                for draw in draws {
                    binds
                        .entry(draw.bind)
                        .or_insert_with(|| res.bind_group(draw.bind));
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("kagari pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if *clear {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_vertex_buffer(0, res.buffers.vertex.slice(..));
                for draw in draws {
                    let Some(Some(bind)) = binds.get(&draw.bind) else {
                        continue;
                    };
                    let pipeline = match draw.pipe {
                        DrawPipe::Fixed(pipe) => res.gfx.pipes.get(pipe),
                        DrawPipe::User(id, role) => match res.shaders.render(id, role) {
                            Some(p) => p,
                            None => continue,
                        },
                    };
                    if let Some(offset) = draw.args {
                        let Some((_, args)) = &res.buffers.args else {
                            continue;
                        };
                        pass.set_bind_group(2, args, &[offset]);
                    }
                    let s = draw.scissor;
                    pass.set_scissor_rect(s.x, s.y, s.w, s.h);
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &res.buffers.bind, &[draw.uniform]);
                    pass.set_bind_group(1, bind, &[]);
                    pass.draw(draw.verts.clone(), 0..1);
                }
            }
            Op::Copy { from, to, rect } => {
                let (Some(src), Some(dst)) = (res.texture(*from), res.layers.texture(*to)) else {
                    continue;
                };
                let origin = wgpu::Origin3d {
                    x: rect.x,
                    y: rect.y,
                    z: 0,
                };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: src,
                        mip_level: 0,
                        origin,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: dst,
                        mip_level: 0,
                        origin,
                        aspect: wgpu::TextureAspect::All,
                    },
                    extent(rect),
                );
            }
        }
    }
}
