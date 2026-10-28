#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! Fonts, shaping, rich-text layout (ruby, vertical text, emphasis dots, outline, shadow) and
//! glyph rasterization for the Kagari runtimes. Nothing here touches a platform: the engine
//! lays text out with [`layout`], and the renderer draws [`Layout`] glyphs through
//! [`Fonts::rasterize`].

mod breaking;
mod fonts;
mod layout;
mod raster;
mod shape;

pub use fonts::{FontError, Fonts};
pub use layout::layout;

use unicode_segmentation::UnicodeSegmentation;

/// A font loaded into [`Fonts`], numbered in load order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FontId(pub u32);

/// Positioned glyphs of one paragraph.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Layout {
    /// The text box.
    pub size: [f32; 2],
    /// Reveal units in reading order.
    pub units: u32,
    /// Base, ruby and emphasis glyphs.
    pub glyphs: Vec<Glyph>,
}

/// One glyph of a [`Layout`].
#[derive(Clone, PartialEq, Debug)]
pub struct Glyph {
    pub key: GlyphKey,
    /// Baseline origin, relative to the layout origin.
    pub pos: [f32; 2],
    /// Premultiplied linear.
    pub color: [f32; 4],
    /// Width px, color.
    pub outline: Option<(f32, [f32; 4])>,
    /// Offset, blur px, color.
    pub shadow: Option<([f32; 2], f32, [f32; 4])>,
    /// Ruby and emphasis glyphs carry their base's first unit.
    pub unit: u32,
}

/// Everything that decides a glyph's coverage mask.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GlyphKey {
    pub font: FontId,
    pub glyph: u32,
    /// Size in px × 64.
    pub size_q: u32,
    pub embolden: bool,
    pub skew: bool,
    pub rotate90: bool,
}

/// `alpha`: `width·height` coverage bytes, row-major, top row first. `left`: columns from the glyph
/// origin to the left edge (positive right). `top`: rows from the baseline up to the top row (positive
/// up). The top-left pixel sits at `(origin.x + left, origin.y − top)` in y-down design space.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub alpha: Vec<u8>,
}

/// The input of [`layout`]: styled spans of base text plus their ruby readings.
#[derive(Clone, PartialEq, Debug)]
pub struct Paragraph {
    pub spans: Vec<Span>,
    pub rubies: Vec<RubyText>,
    pub style: ParagraphStyle,
    /// Tried after a span's own font when that font lacks a grapheme.
    pub fallback: FontId,
}

/// A run of base text with one style.
#[derive(Clone, PartialEq, Debug)]
pub struct Span {
    pub text: String,
    pub font: FontId,
    pub size_px: f32,
    /// Premultiplied linear.
    pub color: [f32; 4],
    pub bold: bool,
    pub italic: bool,
    pub outline: Option<(f32, [f32; 4])>,
    pub shadow: Option<([f32; 2], f32, [f32; 4])>,
    pub emphasis_dots: bool,
    /// Index into `rubies`; consecutive spans with the same index form one base.
    pub ruby: Option<u32>,
}

/// A ruby reading: one string over the whole base, or one string per base grapheme.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RubyText {
    Group(String),
    Mono(Vec<String>),
}

/// Paragraph-wide layout settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ParagraphStyle {
    /// Wrap width, or column length when vertical.
    pub limit: Option<f32>,
    /// Line pitch as a multiple of the line's largest font size.
    pub line_height: f32,
    /// Px added after every base grapheme.
    pub letter_spacing: f32,
    pub align: Align,
    pub ruby_size_px: f32,
    /// Continuation lines of a line opening with `「`, `『` or `（` are indented past it.
    pub hanging_indent: bool,
    /// Columns top to bottom, right to left.
    pub vertical: bool,
}

/// Line alignment inside the widest line (the column length when vertical).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Measurements [`layout`] returns besides the glyphs.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct LayoutMetrics {
    /// Pen after the last base grapheme, on its baseline (horizontal) or column centre (vertical).
    pub end: [f32; 2],
}

/// Reveal units of `text`: extended grapheme clusters, excluding line breaks.
pub fn units(text: &str) -> u32 {
    let count = text.graphemes(true).filter(|g| !is_newline(g)).count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// A grapheme that ends a line: `\n` (alone or as `\r\n`). It has no glyph and is not a unit.
pub(crate) fn is_newline(grapheme: &str) -> bool {
    grapheme.ends_with('\n')
}

#[cfg(test)]
mod tests;
