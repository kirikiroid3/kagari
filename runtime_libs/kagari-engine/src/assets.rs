//! Decoded assets: images, fonts, shader sources, audio durations, video facts and save
//! thumbnails. Assets load lazily from the platform on first use and stay cached for the process
//! lifetime.

use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;

use kagari_bytecode::AssetKind;
use kagari_text::{FontId, Fonts};
use kagari_vm::{AssetId, Program, Trap};

use crate::Platform;
use crate::frame::ThumbKey;
use crate::values::host_trap;

/// A loaded image: its size, read from the header, and its encoded bytes. The renderer decodes
/// them when it uploads the texture (again after an eviction or with a new renderer), so no
/// decoded copy stays in memory.
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    encoded: Vec<u8>,
}

impl ImageData {
    /// The pixels: RGBA8, straight alpha, row-major.
    pub fn decode_rgba(&self) -> image::ImageResult<Vec<u8>> {
        Ok(decode_image(&self.encoded)?.into_rgba8().into_raw())
    }
}

/// A save thumbnail: straight-alpha sRGB RGBA8.
pub struct Thumbnail {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// The facts the engine needs about a movie: its size, its length and whether it has audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    pub has_audio: bool,
}

pub struct AssetStore {
    program: Rc<Program>,
    /// Indexed by `AssetId.0`.
    images: Vec<Option<ImageData>>,
    fonts: Fonts,
    font_ids: Vec<Option<FontId>>,
    audio_ms: Vec<Option<Option<u32>>>,
    /// Movie facts; `Some(None)` once a movie failed to read or decode.
    videos: Vec<Option<Option<VideoInfo>>>,
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
            videos: vec![None; count],
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

    /// The facts of a movie loaded by `ensure_video`; `None` before that or when it is unreadable.
    pub fn video_info(&self, id: AssetId) -> Option<VideoInfo> {
        (*self.videos.get(index(id))?)?
    }

    pub fn video_size(&self, id: AssetId) -> Option<(u32, u32)> {
        self.video_info(id).map(|info| (info.width, info.height))
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

    /// Loads an image: its size from the header (a file that is not a supported image traps),
    /// and the encoded bytes for the renderer.
    pub(crate) fn ensure_image(
        &mut self,
        id: AssetId,
        p: &mut dyn Platform,
    ) -> Result<(u32, u32), Trap> {
        if let Some(size) = self.image_size(id) {
            return Ok(size);
        }
        self.check_kind(id, |k| k == AssetKind::Image, "an image")?;
        let encoded = self.read(id, p)?;
        let (width, height) = image_dimensions(&encoded)
            .map_err(|e| self.error(id, format!("decode failed: {e}")))?;
        if let Some(slot) = self.images.get_mut(index(id)) {
            *slot = Some(ImageData {
                width,
                height,
                encoded,
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

    /// Asks the platform about a movie once. A movie that cannot be read or decoded is logged and
    /// cached as unreadable: it is not drawn, and it adds nothing to the timeline.
    pub(crate) fn ensure_video(&mut self, id: AssetId, p: &mut dyn Platform) -> Result<(), Trap> {
        if matches!(self.videos.get(index(id)), Some(Some(_))) {
            return Ok(());
        }
        self.check_kind(id, |k| k == AssetKind::Video, "a video")?;
        let info = p.video_info(self.path(id));
        if info.is_none() {
            log::warn!(
                "video `{}`: cannot be read or decoded; not drawn",
                self.path(id)
            );
        }
        if let Some(slot) = self.videos.get_mut(index(id)) {
            *slot = Some(info);
        }
        Ok(())
    }
}

const JXL_CODESTREAM: &[u8] = &[0xFF, 0x0A];
const JXL_CONTAINER: &[u8] = &[
    0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
];

fn is_jxl(bytes: &[u8]) -> bool {
    bytes.starts_with(JXL_CODESTREAM) || bytes.starts_with(JXL_CONTAINER)
}

/// The width and height in an image asset's header, without decoding the pixels.
fn image_dimensions(bytes: &[u8]) -> image::ImageResult<(u32, u32)> {
    if is_jxl(bytes) {
        let decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(bytes))?;
        return Ok(image::ImageDecoder::dimensions(&decoder));
    }
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()
}

/// Decodes an image asset. `image` has no JPEG XL decoder, so JPEG XL goes to `jxl-oxide`
/// directly (not through `image::hooks`, which are process-wide), under the same default
/// allocation limit `load_from_memory` applies.
fn decode_image(bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    if is_jxl(bytes) {
        let mut decoder = jxl_oxide::integration::JxlDecoder::new(Cursor::new(bytes))?;
        image::ImageDecoder::set_limits(&mut decoder, image::Limits::default())?;
        return image::DynamicImage::from_decoder(decoder);
    }
    image::load_from_memory(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::io::Cursor;

    use super::decode_image;

    #[test]
    fn decodes_bmp() {
        let mut source = image::RgbaImage::new(3, 2);
        source.put_pixel(2, 1, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        source
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Bmp)
            .unwrap();
        let decoded = decode_image(&bytes).unwrap().into_rgba8();
        assert_eq!(decoded, source);
    }

    #[test]
    fn decodes_jxl() {
        let bytes = include_bytes!("../../kagari-compat/tests/fixtures/still.jxl");
        let decoded = decode_image(bytes).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 128]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 255, 0, 255]);
        assert_eq!(decoded.get_pixel(2, 1).0, [128, 128, 128, 255]);
    }
}
