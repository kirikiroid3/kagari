#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The wgpu compositor for the engine's [`Frame`] draw list. A frame is drawn at design size
//! into a linear-light, premultiplied `Rgba16Float` target, then letterboxed onto a surface
//! ([`Renderer::render`]), kept offscreen ([`Renderer::render_offscreen`]) or read back as
//! sRGB RGBA8 ([`Renderer::render_to_rgba`]). Glyph masks come from the engine's
//! `AssetStore::fonts()`.

mod atlas;
mod blur;
mod encode;
mod layers;
mod pipelines;
mod plan;
mod readback;
mod text;
mod textures;
mod viewport;

pub use viewport::Viewport;

use kagari_engine::{AssetStore, Frame, Node, ThumbKey};

use crate::atlas::GlyphAtlas;
use crate::encode::{DesignTarget, FrameBuffers, Resources};
use crate::layers::{LayerPool, linear_texture};
use crate::pipelines::{Pipe, Pipelines, uniform_slot};
use crate::plan::Planner;
use crate::readback::Readback;
use crate::textures::{Still, TextureCache};

/// The device, the queue and the pipelines, shared by every cache.
pub(crate) struct Gfx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipes: Pipelines,
}

/// Draws [`Frame`]s with wgpu.
pub struct Renderer {
    gfx: Gfx,
    textures: TextureCache,
    atlas: GlyphAtlas,
    layers: LayerPool,
    buffers: FrameBuffers,
    design: Option<DesignTarget>,
    still: Option<Still>,
    readback: Option<Readback>,
    slot: u64,
}

impl Renderer {
    /// `target_format` is the format of the views [`Renderer::render`] presents into.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Renderer {
        let gfx = Gfx {
            device: device.clone(),
            queue: queue.clone(),
            pipes: Pipelines::new(device, target_format),
        };
        let slot = uniform_slot(&device.limits());
        Renderer {
            textures: TextureCache::new(&gfx),
            atlas: GlyphAtlas::new(&gfx),
            layers: LayerPool::default(),
            buffers: FrameBuffers::new(&gfx, slot),
            design: None,
            still: None,
            readback: None,
            slot,
            gfx,
        }
    }

    /// Renders `frame` at design size, then letterboxes it into `target` (size =
    /// `viewport.surface`). Glyph masks come from `assets.fonts()`.
    pub fn render(&mut self, frame: &Frame, assets: &AssetStore, target: &wgpu::TextureView, viewport: Viewport) {
        let mut encoder = self.prepare(frame, assets);
        if let Some(design) = &self.design {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kagari present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if viewport.width > 0 && viewport.height > 0 {
                pass.set_viewport(
                    viewport.x as f32,
                    viewport.y as f32,
                    viewport.width as f32,
                    viewport.height as f32,
                    0.0,
                    1.0,
                );
                pass.set_pipeline(self.gfx.pipes.get(Pipe::Present));
                pass.set_bind_group(0, &self.buffers.bind, &[0]);
                pass.set_bind_group(1, &design.bind, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.gfx.queue.submit([encoder.finish()]);
        self.textures.evict();
    }

    /// Renders the design target only (updating the still when `frame.capture_still`).
    pub fn render_offscreen(&mut self, frame: &Frame, assets: &AssetStore) {
        let encoder = self.prepare(frame, assets);
        self.gfx.queue.submit([encoder.finish()]);
        self.textures.evict();
    }

    /// `frame` stretched to `width`×`height`: sRGB RGBA8, straight alpha (always 255),
    /// row-major, `width·height·4` bytes; empty on failure.
    pub fn render_to_rgba(&mut self, frame: &Frame, assets: &AssetStore, width: u32, height: u32) -> Vec<u8> {
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let max = self.gfx.device.limits().max_texture_dimension_2d;
        if width > max || height > max {
            log::error!("frame readback failed: {width}×{height} exceeds the GPU limit {max}");
            return Vec::new();
        }
        if self.readback.as_ref().is_none_or(|r| r.size != (width, height)) {
            self.readback = Readback::new(&self.gfx, (width, height));
        }
        let mut encoder = self.prepare(frame, assets);
        let (Some(readback), Some(design)) = (&self.readback, &self.design) else {
            log::error!("frame readback failed: cannot size a {width}×{height} readback");
            return Vec::new();
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kagari resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &readback.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(self.gfx.pipes.get(Pipe::Resolve));
            pass.set_bind_group(0, &self.buffers.bind, &[0]);
            pass.set_bind_group(1, &design.bind, &[]);
            pass.draw(0..3, 0..1);
        }
        readback.copy(&mut encoder);
        let submission = self.gfx.queue.submit([encoder.finish()]);
        self.textures.evict();
        match readback.read(&self.gfx, submission) {
            Ok(rgba) => rgba,
            Err(e) => {
                log::error!("frame readback failed: {e}");
                // Its buffer may still be mapped or pending a map, which would fail every later
                // copy into it: start over with a fresh one next time.
                self.readback = None;
                Vec::new()
            }
        }
    }

    /// Thumbnail nodes of `frame` whose generation differs from the uploaded one (or that were
    /// never uploaded), without duplicates. A generation `set_thumbnail` refused is not listed.
    pub fn stale_thumbnails(&self, frame: &Frame) -> Vec<(ThumbKey, u64)> {
        let mut out = Vec::new();
        let mut stack: Vec<&Node> = frame.nodes.iter().collect();
        while let Some(node) = stack.pop() {
            match node {
                Node::Group { children, .. } => stack.extend(children),
                Node::Transition { from, to, .. } => {
                    stack.extend(from);
                    stack.extend(to);
                }
                Node::Thumbnail { slot, generation, .. } => {
                    let fresh = self.textures.thumb(*slot).map(|(g, _)| g) == Some(*generation)
                        || self.textures.thumb_rejected(*slot, *generation);
                    if !fresh && !out.contains(&(*slot, *generation)) {
                        out.push((*slot, *generation));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Uploads a save thumbnail. `rgba`: straight-alpha sRGB RGBA8; a length other than
    /// `width·height·4`, or a side above the GPU's texture limit, is logged and keeps the old
    /// texture.
    pub fn set_thumbnail(&mut self, slot: ThumbKey, generation: u64, width: u32, height: u32, rgba: &[u8]) {
        self.textures
            .set_thumbnail(&self.gfx, slot, generation, width, height, rgba);
    }

    /// Plans and encodes the design passes (plus the still copy) of `frame`.
    fn prepare(&mut self, frame: &Frame, assets: &AssetStore) -> wgpu::CommandEncoder {
        let max = self.gfx.device.limits().max_texture_dimension_2d;
        let size = (frame.size.0.clamp(1, max), frame.size.1.clamp(1, max));
        if size != frame.size && frame.size.0 > 0 && frame.size.1 > 0 {
            log::warn!(
                "frame size {}×{} exceeds the GPU limit {max}; clipped",
                frame.size.0,
                frame.size.1
            );
        }
        if self.design.as_ref().is_none_or(|d| d.size != size) {
            let texture = linear_texture(&self.gfx.device, "kagari design", size);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.design = Some(DesignTarget {
                bind: self.gfx.pipes.textures(&self.gfx.device, &[&view]),
                texture,
                view,
                size,
            });
            self.layers.retain_size(size);
        }
        self.textures.begin_frame();
        self.atlas.begin_frame(&self.gfx);
        let plan = Planner::new(
            &self.gfx,
            &mut self.textures,
            &mut self.atlas,
            &mut self.layers,
            assets,
            size,
            usize::try_from(self.slot).unwrap_or(usize::MAX),
            self.still.is_some(),
        )
        .run(&frame.nodes);
        self.buffers.upload(&self.gfx, &plan);
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kagari frame") });
        let Some(design) = &self.design else {
            return encoder;
        };
        encode::encode(
            &Resources {
                gfx: &self.gfx,
                design,
                layers: &self.layers,
                textures: &self.textures,
                atlas: &self.atlas,
                still: self.still.as_ref(),
                buffers: &self.buffers,
            },
            &plan,
            &mut encoder,
        );
        if frame.capture_still {
            if self.still.as_ref().is_none_or(|s| s.size != size) {
                self.still = Some(Still::new(&self.gfx, size));
            }
            if let Some(still) = &self.still {
                encoder.copy_texture_to_texture(
                    design.texture.as_image_copy(),
                    still.texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        encoder
    }
}

#[cfg(test)]
mod tests;
