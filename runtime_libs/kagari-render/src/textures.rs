//! GPU copies of images, save thumbnails and the still.

use std::collections::{HashMap, HashSet};

use kagari_bytecode::AssetId;
use kagari_engine::{AssetStore, ThumbKey};

use crate::layers::linear_texture;
use crate::Gfx;

/// Image textures above this total (`w·h·4` summed) evict the least recently used ones.
const IMAGE_BUDGET: u64 = 512 * 1024 * 1024;

/// One uploaded texture.
pub(crate) struct Entry {
    pub view: wgpu::TextureView,
    pub bind: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
    last_used: u64,
}

impl Entry {
    fn bytes(&self) -> u64 {
        u64::from(self.width)
            .saturating_mul(u64::from(self.height))
            .saturating_mul(4)
    }
}

/// Straight-alpha sRGB RGBA8 → premultiplied in sRGB encoding: `c' = (c·a + 127)/255`.
pub(crate) fn premultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for &[r, g, b, a] in rgba.as_chunks::<4>().0 {
        let a32 = u32::from(a);
        let mul = |c: u8| {
            let v = u32::from(c).saturating_mul(a32).saturating_add(127) / 255;
            u8::try_from(v).unwrap_or(u8::MAX)
        };
        out.extend_from_slice(&[mul(r), mul(g), mul(b), a]);
    }
    out
}

/// Uploads premultiplied RGBA8 as an `Rgba8Unorm` texture with its bind group.
fn upload(
    gfx: &Gfx,
    label: &str,
    width: u32,
    height: u32,
    premultiplied: &[u8],
) -> Entry {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gfx.queue.write_texture(
        texture.as_image_copy(),
        premultiplied,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width.saturating_mul(4)),
            rows_per_image: Some(height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = gfx.pipes.textures(&gfx.device, &[&view]);
    Entry {
        view,
        bind,
        width,
        height,
        last_used: 0,
    }
}

/// Image and thumbnail textures, plus a 1×1 white texture (the stand-in rule mask).
pub(crate) struct TextureCache {
    images: HashMap<AssetId, Entry>,
    warned: HashSet<AssetId>,
    thumbs: HashMap<ThumbKey, (u64, Entry)>,
    /// Thumbnails refused by `set_thumbnail`, so they are warned about and offered once.
    rejected_thumbs: HashSet<(ThumbKey, u64)>,
    pub white: Entry,
    frame: u64,
}

impl TextureCache {
    pub fn new(gfx: &Gfx) -> TextureCache {
        TextureCache {
            images: HashMap::new(),
            warned: HashSet::new(),
            thumbs: HashMap::new(),
            rejected_thumbs: HashSet::new(),
            white: upload(gfx, "kagari white", 1, 1, &[255; 4]),
            frame: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Uploads image `id` on first use and marks it used this frame; `None` (warned once per
    /// id) when it is not loaded or exceeds `max_dim`.
    pub fn image(
        &mut self,
        gfx: &Gfx,
        assets: &AssetStore,
        id: AssetId,
        max_dim: u32,
    ) -> Option<&Entry> {
        if !self.images.contains_key(&id) {
            let Some(data) = assets.image(id) else {
                if self.warned.insert(id) {
                    log::warn!("image asset #{} is not loaded; skipped", id.0);
                }
                return None;
            };
            if data.width > max_dim || data.height > max_dim {
                if self.warned.insert(id) {
                    log::warn!(
                        "image asset #{} is {}×{}, larger than the GPU limit {max_dim}; skipped",
                        id.0,
                        data.width,
                        data.height
                    );
                }
                return None;
            }
            let expected = (data.width as usize)
                .saturating_mul(data.height as usize)
                .saturating_mul(4);
            if data.width == 0 || data.height == 0 || data.rgba.len() != expected {
                if self.warned.insert(id) {
                    log::warn!("image asset #{} has no pixels; skipped", id.0);
                }
                return None;
            }
            let entry = upload(
                gfx,
                "kagari image",
                data.width,
                data.height,
                &premultiply(&data.rgba),
            );
            self.images.insert(id, entry);
        }
        let frame = self.frame;
        let entry = self.images.get_mut(&id)?;
        entry.last_used = frame;
        Some(entry)
    }

    /// An image already prepared this frame.
    pub fn get_image(&self, id: AssetId) -> Option<&Entry> {
        self.images.get(&id)
    }

    pub fn thumb(&self, slot: ThumbKey) -> Option<(u64, &Entry)> {
        self.thumbs.get(&slot).map(|(g, e)| (*g, e))
    }

    /// Prepares a rule mask; false (warned once per id) when it cannot be drawn, which makes
    /// the transition a crossfade.
    pub fn rule_mask(&mut self, gfx: &Gfx, assets: &AssetStore, id: AssetId, max_dim: u32) -> bool {
        if !self.images.contains_key(&id) && assets.image(id).is_none() {
            if self.warned.insert(id) {
                log::warn!("rule mask #{} is not loaded; drawing a crossfade", id.0);
            }
            return false;
        }
        self.image(gfx, assets, id, max_dim).is_some()
    }

    /// Whether `set_thumbnail` refused this generation of `slot`.
    pub fn thumb_rejected(&self, slot: ThumbKey, generation: u64) -> bool {
        self.rejected_thumbs.contains(&(slot, generation))
    }

    /// Uploads a thumbnail, unless `thumbnail_problem` finds one (warned once per generation;
    /// the old texture stays).
    pub fn set_thumbnail(
        &mut self,
        gfx: &Gfx,
        slot: ThumbKey,
        generation: u64,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        let max_dim = gfx.device.limits().max_texture_dimension_2d;
        if let Some(problem) = thumbnail_problem(width, height, rgba.len(), max_dim) {
            if self.rejected_thumbs.insert((slot, generation)) {
                log::warn!("thumbnail {slot:?} {problem}; ignored");
            }
            return;
        }
        let entry = upload(gfx, "kagari thumbnail", width, height, &premultiply(rgba));
        self.thumbs.insert(slot, (generation, entry));
    }

    /// Evicts least recently used images not used this frame while over the budget.
    pub fn evict(&mut self) {
        let mut total: u64 = self.images.values().map(Entry::bytes).fold(0, u64::saturating_add);
        if total <= IMAGE_BUDGET {
            return;
        }
        let mut old: Vec<(u64, AssetId)> = self
            .images
            .iter()
            .filter(|(_, e)| e.last_used != self.frame)
            .map(|(id, e)| (e.last_used, *id))
            .collect();
        old.sort_unstable();
        for (_, id) in old {
            if total <= IMAGE_BUDGET {
                break;
            }
            if let Some(e) = self.images.remove(&id) {
                total = total.saturating_sub(e.bytes());
            }
        }
    }
}

/// Why a `width`×`height` thumbnail of `len` bytes cannot become a texture, if it cannot.
pub(crate) fn thumbnail_problem(width: u32, height: u32, len: usize, max_dim: u32) -> Option<String> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if width == 0 || height == 0 || len != expected {
        Some(format!("has {len} bytes, expected {width}×{height}×4"))
    } else if width > max_dim || height > max_dim {
        Some(format!("is {width}×{height}, larger than the GPU limit {max_dim}"))
    } else {
        None
    }
}

/// The still: a copy of the design target, drawn by `Node::Still`.
pub(crate) struct Still {
    pub texture: wgpu::Texture,
    pub bind: wgpu::BindGroup,
    pub size: (u32, u32),
}

impl Still {
    pub fn new(gfx: &Gfx, size: (u32, u32)) -> Still {
        let texture = linear_texture(&gfx.device, "kagari still", size);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Still {
            bind: gfx.pipes.textures(&gfx.device, &[&view]),
            texture,
            size,
        }
    }
}
