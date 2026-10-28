//! The glyph atlas: coverage masks packed on `R8Unorm` pages by a shelf packer. Page 0 also
//! holds a 4×4 white block that `Fill` samples.

use std::collections::HashMap;

use kagari_text::{Fonts, GlyphKey};

use crate::Gfx;

/// The side of one page, in texels.
pub(crate) const PAGE_SIZE: u32 = 2048;
const MAX_PAGES: usize = 4;
/// The side of the white block at (0, 0) of page 0.
const WHITE: u32 = 4;

/// Where a mask sits in the atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AtlasGlyph {
    pub page: u16,
    /// `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
}

struct Shelf {
    y: u32,
    h: u32,
    x: u32,
}

struct Page {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl Page {
    fn new(gfx: &Gfx) -> Page {
        let texture = gfx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kagari glyph atlas"),
            size: wgpu::Extent3d {
                width: PAGE_SIZE,
                height: PAGE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Page {
            bind: gfx.pipes.textures(&gfx.device, &[&view]),
            texture,
            shelves: Vec::new(),
            next_y: 0,
        }
    }

    /// A `w`×`h` rectangle: an existing shelf of a close height with room, else a new shelf.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let max_h = h.saturating_add(h / 4).saturating_add(1);
        for shelf in &mut self.shelves {
            if h <= shelf.h && shelf.h <= max_h && shelf.x.saturating_add(w) <= PAGE_SIZE {
                let x = shelf.x;
                shelf.x = shelf.x.saturating_add(w);
                return Some((x, shelf.y));
            }
        }
        if w > PAGE_SIZE || self.next_y.saturating_add(h) > PAGE_SIZE {
            return None;
        }
        let y = self.next_y;
        self.shelves.push(Shelf { y, h, x: w });
        self.next_y = y.saturating_add(h);
        Some((0, y))
    }

    fn write(&self, gfx: &Gfx, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        gfx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    fn reserve_white(&mut self, gfx: &Gfx) {
        if let Some((x, y)) = self.alloc(WHITE, WHITE) {
            let white = [u8::MAX; (WHITE * WHITE) as usize];
            self.write(gfx, x, y, WHITE, WHITE, &white);
        }
    }
}

pub(crate) struct GlyphAtlas {
    pages: Vec<Page>,
    /// `None`: the glyph has no mask (empty or not rasterizable).
    entries: HashMap<(GlyphKey, u32), Option<AtlasGlyph>>,
    warned_full: bool,
}

impl GlyphAtlas {
    pub fn new(gfx: &Gfx) -> GlyphAtlas {
        let mut page = Page::new(gfx);
        page.reserve_white(gfx);
        GlyphAtlas {
            pages: vec![page],
            entries: HashMap::new(),
            warned_full: false,
        }
    }

    /// Resets every entry when more than one page is in use or page 0 is over 90 % full.
    pub fn begin_frame(&mut self, gfx: &Gfx) {
        self.warned_full = false;
        let full = self
            .pages
            .first()
            .is_some_and(|p| p.next_y as f32 > PAGE_SIZE as f32 * 0.9);
        if self.pages.len() > 1 || full {
            self.pages.truncate(1);
            self.entries.clear();
            if let Some(page) = self.pages.first_mut() {
                page.shelves.clear();
                page.next_y = 0;
                page.reserve_white(gfx);
            }
        }
    }

    /// The centre of the white block, in page-0 uv.
    pub fn white_uv(&self) -> [f32; 2] {
        let c = (WHITE as f32 / 2.0) / PAGE_SIZE as f32;
        [c, c]
    }

    pub fn bind(&self, page: u16) -> Option<&wgpu::BindGroup> {
        self.pages.get(usize::from(page)).map(|p| &p.bind)
    }

    /// The mask of `key` (its outline ring when `outline_px > 0`), rasterized and uploaded on
    /// first use. `None` for an empty mask or a full atlas.
    pub fn glyph(
        &mut self,
        gfx: &Gfx,
        fonts: &Fonts,
        key: GlyphKey,
        outline_px: f32,
    ) -> Option<AtlasGlyph> {
        let outline_q = (outline_px.max(0.0) * 64.0).round() as u32;
        if let Some(entry) = self.entries.get(&(key, outline_q)) {
            return *entry;
        }
        let Some(mask) = fonts.rasterize(key, outline_px.max(0.0)) else {
            self.entries.insert((key, outline_q), None);
            return None;
        };
        let pad_w = mask.width.saturating_add(2);
        let pad_h = mask.height.saturating_add(2);
        let expected = (mask.width as usize).saturating_mul(mask.height as usize);
        if mask.width == 0
            || mask.height == 0
            || mask.alpha.len() != expected
            || pad_w > PAGE_SIZE
            || pad_h > PAGE_SIZE
        {
            self.entries.insert((key, outline_q), None);
            return None;
        }
        let mut place = None;
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some(at) = page.alloc(pad_w, pad_h) {
                place = Some((i, at));
                break;
            }
        }
        if place.is_none() && self.pages.len() < MAX_PAGES {
            let mut page = Page::new(gfx);
            if let Some(at) = page.alloc(pad_w, pad_h) {
                place = Some((self.pages.len(), at));
            }
            self.pages.push(page);
        }
        let Some((index, (x, y))) = place else {
            if !self.warned_full {
                self.warned_full = true;
                log::warn!("glyph atlas full; glyph skipped");
            }
            return None;
        };
        let mut padded = vec![0u8; (pad_w as usize).saturating_mul(pad_h as usize)];
        let row = mask.width as usize;
        for (r, src) in mask.alpha.chunks_exact(row).enumerate() {
            let start = r
                .saturating_add(1)
                .saturating_mul(pad_w as usize)
                .saturating_add(1);
            if let Some(dst) = padded.get_mut(start..start.saturating_add(row)) {
                dst.copy_from_slice(src);
            }
        }
        let page = self.pages.get(index)?;
        page.write(gfx, x, y, pad_w, pad_h, &padded);
        let s = PAGE_SIZE as f32;
        let x0 = x.saturating_add(1) as f32;
        let y0 = y.saturating_add(1) as f32;
        let glyph = AtlasGlyph {
            page: u16::try_from(index).unwrap_or(u16::MAX),
            uv: [
                x0 / s,
                y0 / s,
                (x0 + mask.width as f32) / s,
                (y0 + mask.height as f32) / s,
            ],
            width: mask.width,
            height: mask.height,
            left: mask.left,
            top: mask.top,
        };
        self.entries.insert((key, outline_q), Some(glyph));
        Some(glyph)
    }
}
