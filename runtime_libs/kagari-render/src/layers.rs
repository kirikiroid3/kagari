//! Offscreen layers: isolated groups, blend backdrops, shadows and transition sides.

use crate::pipelines::LINEAR_FORMAT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayerId(pub u32);

struct Layer {
    view: wgpu::TextureView,
    texture: wgpu::Texture,
    size: (u32, u32),
    free: bool,
}

/// Reusable `Rgba16Float` render targets.
#[derive(Default)]
pub(crate) struct LayerPool {
    layers: Vec<Option<Layer>>,
}

/// A linear-light target texture with every usage the renderer needs.
pub(crate) fn linear_texture(device: &wgpu::Device, label: &str, size: (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0.max(1),
            height: size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: LINEAR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

impl LayerPool {
    /// A free layer of `size`, created when none is free.
    pub fn acquire(&mut self, device: &wgpu::Device, size: (u32, u32)) -> LayerId {
        let mut vacant = None;
        for (i, slot) in self.layers.iter_mut().enumerate() {
            match slot {
                Some(layer) if layer.free && layer.size == size => {
                    layer.free = false;
                    return LayerId(u32::try_from(i).unwrap_or(u32::MAX));
                }
                None if vacant.is_none() => vacant = Some(i),
                _ => {}
            }
        }
        let texture = linear_texture(device, "kagari layer", size);
        let layer = Layer {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            size,
            free: false,
        };
        let index = match vacant {
            Some(i) => {
                if let Some(slot) = self.layers.get_mut(i) {
                    *slot = Some(layer);
                }
                i
            }
            None => {
                self.layers.push(Some(layer));
                self.layers.len().saturating_sub(1)
            }
        };
        LayerId(u32::try_from(index).unwrap_or(u32::MAX))
    }

    pub fn release(&mut self, id: LayerId) {
        if let Some(Some(layer)) = self.layers.get_mut(id.0 as usize) {
            layer.free = true;
        }
    }

    /// Drops every layer whose size is not `size` (the design size changed).
    pub fn retain_size(&mut self, size: (u32, u32)) {
        for slot in &mut self.layers {
            if slot.as_ref().is_some_and(|l| l.size != size) {
                *slot = None;
            }
        }
    }

    pub fn view(&self, id: LayerId) -> Option<&wgpu::TextureView> {
        self.layers.get(id.0 as usize)?.as_ref().map(|l| &l.view)
    }

    pub fn texture(&self, id: LayerId) -> Option<&wgpu::Texture> {
        self.layers.get(id.0 as usize)?.as_ref().map(|l| &l.texture)
    }
}
