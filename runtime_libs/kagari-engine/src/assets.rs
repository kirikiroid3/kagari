//! Decoded assets: images, fonts, shader sources, audio durations and save thumbnails. Assets
//! load lazily from the platform on first use and stay cached for the process lifetime.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use kagari_bytecode::AssetKind;
use kagari_text::{FontId, Fonts};
use kagari_vm::{AssetId, Program, Trap};

use crate::Platform;
use crate::frame::ThumbKey;
use crate::values::host_trap;

/// RGBA8, straight alpha, row-major.
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A save thumbnail: straight-alpha sRGB RGBA8.
pub struct Thumbnail {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

pub struct AssetStore {
    program: Rc<Program>,
    /// Indexed by `AssetId.0`.
    images: Vec<Option<ImageData>>,
    fonts: Fonts,
    font_ids: Vec<Option<FontId>>,
    audio_ms: Vec<Option<Option<u32>>>,
    /// WGSL sources, indexed like `images`.
    shaders: Vec<Option<Rc<str>>>,
    thumbs: HashMap<ThumbKey, Thumbnail>,
    next_generation: u64,
}

fn index(id: AssetId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

impl AssetStore {
    pub fn new(program: Rc<Program>) -> AssetStore {
        let count = program.module().assets.len();
        AssetStore {
            program,
            images: (0..count).map(|_| None).collect(),
            fonts: Fonts::new(),
            font_ids: vec![None; count],
            audio_ms: vec![None; count],
            shaders: vec![None; count],
            thumbs: HashMap::new(),
            next_generation: 1,
        }
    }

    /// The path of an asset; `""` out of range.
    pub fn path(&self, id: AssetId) -> &str {
        self.program
            .module()
            .assets
            .get(index(id))
            .map_or("", |asset| self.program.string(asset.path))
    }

    pub fn image(&self, id: AssetId) -> Option<&ImageData> {
        self.images.get(index(id))?.as_ref()
    }

    pub fn image_size(&self, id: AssetId) -> Option<(u32, u32)> {
        self.image(id).map(|image| (image.width, image.height))
    }

    /// The WGSL source of a loaded shader asset.
    pub fn shader_source(&self, id: AssetId) -> Option<&str> {
        self.shaders.get(index(id))?.as_deref()
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    pub fn font(&self, id: AssetId) -> Option<FontId> {
        *self.font_ids.get(index(id))?
    }

    /// The loaded font with the lowest asset id.
    pub fn first_font(&self) -> Option<FontId> {
        self.font_ids.iter().flatten().next().copied()
    }

    pub fn thumbnail(&self, key: ThumbKey) -> Option<&Thumbnail> {
        self.thumbs.get(&key)
    }

    /// `Some((w, h, rgba))` stores a thumbnail under a new generation; `None` removes it.
    pub(crate) fn set_thumbnail(&mut self, key: ThumbKey, rgba: Option<(u32, u32, Arc<[u8]>)>) {
        match rgba {
            Some((width, height, rgba)) => {
                let generation = self.next_generation;
                self.next_generation = self.next_generation.saturating_add(1);
                self.thumbs.insert(
                    key,
                    Thumbnail {
                        generation,
                        width,
                        height,
                        rgba,
                    },
                );
            }
            None => {
                self.thumbs.remove(&key);
            }
        }
    }

    /// Loads every font asset, in asset order.
    pub(crate) fn load_fonts(&mut self, p: &mut dyn Platform) -> Result<(), Trap> {
        let fonts: Vec<AssetId> = self
            .program
            .module()
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.kind == AssetKind::Font)
            .filter_map(|(i, _)| u32::try_from(i).ok().map(AssetId))
            .collect();
        for id in fonts {
            self.ensure_font(id, p)?;
        }
        Ok(())
    }

    fn error(&self, id: AssetId, reason: impl std::fmt::Display) -> Trap {
        host_trap(format!("asset `{}`: {reason}", self.path(id)))
    }

    fn check_kind(
        &self,
        id: AssetId,
        ok: impl Fn(AssetKind) -> bool,
        what: &str,
    ) -> Result<(), Trap> {
        match self.program.module().assets.get(index(id)) {
            Some(asset) if ok(asset.kind) => Ok(()),
            _ => Err(self.error(id, format!("not {what}"))),
        }
    }

    fn read(&self, id: AssetId, p: &mut dyn Platform) -> Result<Vec<u8>, Trap> {
        match p.read_asset(self.path(id)) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(self.error(id, "not found")),
            Err(e) => Err(self.error(id, format!("read failed: {e}"))),
        }
    }

    pub(crate) fn ensure_image(
        &mut self,
        id: AssetId,
        p: &mut dyn Platform,
    ) -> Result<(u32, u32), Trap> {
        if let Some(size) = self.image_size(id) {
            return Ok(size);
        }
        self.check_kind(id, |k| k == AssetKind::Image, "an image")?;
        let bytes = self.read(id, p)?;
        let decoded = image::load_from_memory(&bytes)
            .map_err(|e| self.error(id, format!("decode failed: {e}")))?
            .into_rgba8();
        let (width, height) = decoded.dimensions();
        if let Some(slot) = self.images.get_mut(index(id)) {
            *slot = Some(ImageData {
                width,
                height,
                rgba: decoded.into_raw(),
            });
        }
        Ok((width, height))
    }

    /// Loads the WGSL source of a shader asset; a source that is not UTF-8 is an error, like an
    /// image that does not decode.
    pub(crate) fn ensure_shader(&mut self, id: AssetId, p: &mut dyn Platform) -> Result<(), Trap> {
        if self.shader_source(id).is_some() {
            return Ok(());
        }
        self.check_kind(id, |k| k == AssetKind::Shader, "a shader")?;
        let bytes = self.read(id, p)?;
        let source =
            String::from_utf8(bytes).map_err(|e| self.error(id, format!("decode failed: {e}")))?;
        if let Some(slot) = self.shaders.get_mut(index(id)) {
            *slot = Some(Rc::from(source));
        }
        Ok(())
    }

    pub(crate) fn ensure_font(
        &mut self,
        id: AssetId,
        p: &mut dyn Platform,
    ) -> Result<FontId, Trap> {
        if let Some(font) = self.font(id) {
            return Ok(font);
        }
        self.check_kind(id, |k| k == AssetKind::Font, "a font")?;
        let bytes = self.read(id, p)?;
        let font = self
            .fonts
            .add(Arc::from(bytes))
            .map_err(|e| self.error(id, format!("font load failed: {e}")))?;
        if let Some(slot) = self.font_ids.get_mut(index(id)) {
            *slot = Some(font);
        }
        Ok(font)
    }

    /// The length of an audio asset; `None` when the platform cannot tell. Cached.
    pub(crate) fn audio_duration_ms(
        &mut self,
        id: AssetId,
        p: &mut dyn Platform,
    ) -> Result<Option<u32>, Trap> {
        if let Some(Some(duration)) = self.audio_ms.get(index(id)) {
            return Ok(*duration);
        }
        self.check_kind(
            id,
            |k| matches!(k, AssetKind::Audio | AssetKind::Voice),
            "audio",
        )?;
        let bytes = self.read(id, p)?;
        let duration = p.audio_duration_ms(self.path(id), &bytes);
        if let Some(slot) = self.audio_ms.get_mut(index(id)) {
            *slot = Some(duration);
        }
        Ok(duration)
    }
}
