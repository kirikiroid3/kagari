//! The loaded fonts: faces, their `wght` 700 instances, line metrics and coverage.

use std::fmt;
use std::sync::Arc;

use skrifa::font::{Blob, Font, Kind};
use skrifa::{ScalerFont, Tag};

use crate::{FontId, GlyphKey, Mask};

/// Ascender and descender used when a font has no line metrics, in em.
const FALLBACK_ASCENDER: f32 = 0.88;
const FALLBACK_DESCENDER: f32 = 0.12;

/// A font file that cannot be used.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontError {
    NotAFont,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::NotAFont => f.write_str("not a supported font file (TTF, OTF, TTC or OTC)"),
        }
    }
}

impl std::error::Error for FontError {}

/// One loaded face.
pub(crate) struct Entry {
    pub font: Font,
    /// The `wght` 700 instance, for a font with a `wght` axis.
    pub bold: Option<Font>,
    /// Unhinted scalers in font units; outlines are scaled linearly in the path sink.
    pub scaler: ScalerFont,
    pub bold_scaler: Option<ScalerFont>,
    pub units_per_em: f32,
    /// Line ascender in em (positive up).
    pub ascender: f32,
    /// Line descender in em (positive down).
    pub descender: f32,
}

/// Ascender and descender in px at one size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct LineMetrics {
    pub asc: f32,
    pub desc: f32,
}

impl LineMetrics {
    /// The metrics without any font.
    pub fn fallback(size_px: f32) -> LineMetrics {
        LineMetrics {
            asc: FALLBACK_ASCENDER * size_px,
            desc: FALLBACK_DESCENDER * size_px,
        }
    }
}

/// Every font the game loaded, in load order.
pub struct Fonts {
    entries: Vec<Entry>,
}

impl fmt::Debug for Fonts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fonts")
            .field("count", &self.entries.len())
            .finish()
    }
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::new()
    }
}

impl Fonts {
    pub fn new() -> Fonts {
        Fonts {
            entries: Vec::new(),
        }
    }

    /// Face 0 of a collection (TTC/OTC); plain TTF/OTF otherwise.
    pub fn add(&mut self, bytes: Arc<[u8]>) -> Result<FontId, FontError> {
        let font = Font::new(Blob::Shared(Arc::new(bytes)), 0).ok_or(FontError::NotAFont)?;
        if !matches!(font.kind(), Kind::Sfnt(..)) {
            return Err(FontError::NotAFont);
        }
        let id = FontId(u32::try_from(self.entries.len()).map_err(|_| FontError::NotAFont)?);
        let wght = Tag::new(b"wght");
        let bold = font
            .axes()
            .iter()
            .any(|axis| axis.tag() == wght)
            .then(|| font.instance_builder().variations([(wght, 700.0)]).build());
        let units_per_em = f32::from(font.units_per_em().max(1));
        let (ascender, descender) = match font.metrics().h_line() {
            Some(line) => (
                line.ascender.to_f32() / units_per_em,
                -line.descender.to_f32() / units_per_em,
            ),
            None => (FALLBACK_ASCENDER, FALLBACK_DESCENDER),
        };
        let scaler = ScalerFont::unhinted(font.clone(), None);
        let bold_scaler = bold.as_ref().map(|b| ScalerFont::unhinted(b.clone(), None));
        self.entries.push(Entry {
            font,
            bold,
            scaler,
            bold_scaler,
            units_per_em,
            ascender,
            descender,
        });
        Ok(id)
    }

    /// Coverage mask of one glyph; `outline_px > 0` gives the stroked outline ring of that width
    /// instead of the fill.
    pub fn rasterize(&self, key: GlyphKey, outline_px: f32) -> Option<Mask> {
        crate::raster::rasterize(self.entry(key.font)?, key, outline_px)
    }

    pub(crate) fn entry(&self, id: FontId) -> Option<&Entry> {
        self.entries.get(usize::try_from(id.0).ok()?)
    }

    /// The ids of every loaded font, in load order.
    pub(crate) fn ids(&self) -> impl Iterator<Item = FontId> + '_ {
        (0..self.entries.len()).filter_map(|i| u32::try_from(i).ok().map(FontId))
    }

    /// Whether `id` maps every char of `text`.
    pub(crate) fn covers(&self, id: FontId, text: &str) -> bool {
        self.entry(id).is_some_and(|entry| {
            let charmap = entry.font.charmap();
            text.chars().all(|ch| charmap.map_unicode(ch).is_some())
        })
    }

    /// Line ascender and descender of `id` at `size_px`; the fallback ratios for a missing font.
    pub(crate) fn line_metrics(&self, id: Option<FontId>, size_px: f32) -> LineMetrics {
        match id.and_then(|id| self.entry(id)) {
            Some(entry) => LineMetrics {
                asc: entry.ascender * size_px,
                desc: entry.descender * size_px,
            },
            None => LineMetrics::fallback(size_px),
        }
    }

    /// The first font of `preferred`, `fallback`, then every other font in load order that maps
    /// every char of `grapheme`; else the first of them that is loaded (`preferred` when it is,
    /// drawing .notdef); `None` without any font.
    pub(crate) fn pick(
        &self,
        preferred: FontId,
        fallback: FontId,
        grapheme: &str,
    ) -> Option<FontId> {
        let chain: Vec<FontId> = [preferred, fallback]
            .into_iter()
            .chain(self.ids().filter(|id| *id != preferred && *id != fallback))
            .filter(|id| self.entry(*id).is_some())
            .collect();
        chain
            .iter()
            .copied()
            .find(|id| self.covers(*id, grapheme))
            .or_else(|| chain.first().copied())
    }
}
