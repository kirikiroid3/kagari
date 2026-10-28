//! The second phase of a frame: uploads the planned vertices and uniforms, then records every
//! pass and copy into one command encoder.

use std::collections::HashMap;

use crate::Gfx;
use crate::atlas::GlyphAtlas;
use crate::layers::LayerPool;
use crate::pipelines::UNIFORM_SIZE;
use crate::plan::{BindKey, Op, Plan, PxRect, Target};
use crate::textures::{Still, TextureCache};

const INITIAL_VERTEX_BYTES: u64 = 64 * 1024;

/// The vertex buffer, the uniform buffer and the group-0 bind group over it.
pub(crate) struct FrameBuffers {
    vertex: wgpu::Buffer,
    uniform: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
}

fn buffer(gfx: &Gfx, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    gfx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn uniform_bind(gfx: &Gfx, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
    gfx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kagari uniforms"),
        layout: &gfx.pipes.uniforms,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: uniform,
                offset: 0,
                size: wgpu::BufferSize::new(UNIFORM_SIZE),
            }),
        }],
    })
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
        FrameBuffers {
            vertex: buffer(gfx, "kagari vertices", INITIAL_VERTEX_BYTES, wgpu::BufferUsages::VERTEX),
            bind: uniform_bind(gfx, &uniform),
            uniform,
        }
    }

    /// Writes the plan's vertices and uniforms, growing either buffer by doubling.
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
            self.bind = uniform_bind(gfx, &self.uniform);
        }
        if !verts.is_empty() {
            gfx.queue.write_buffer(&self.vertex, 0, verts);
        }
        if !plan.uniforms.is_empty() {
            gfx.queue.write_buffer(&self.uniform, 0, &plan.uniforms);
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
            BindKey::Blend { src, backdrop } => {
                Some(gfx.pipes.textures(&gfx.device, &layers(&[src, backdrop])?))
            }
            BindKey::Transition { from, to, mask } => {
                let mut views = layers(&[from, to])?;
                let mask = match mask {
                    Some(id) => &self.textures.get_image(id)?.view,
                    None => &self.textures.white.view,
                };
                views.push(mask);
                Some(gfx.pipes.textures(&gfx.device, &views))
            }
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

/// Records every op of `plan` into `encoder`.
pub(crate) fn encode(res: &Resources<'_>, plan: &Plan, encoder: &mut wgpu::CommandEncoder) {
    let mut binds: HashMap<BindKey, Option<wgpu::BindGroup>> = HashMap::new();
    for op in &plan.ops {
        match op {
            Op::Pass { target, clear, draws } => {
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
                    let s = draw.scissor;
                    pass.set_scissor_rect(s.x, s.y, s.w, s.h);
                    pass.set_pipeline(res.gfx.pipes.get(draw.pipe));
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
