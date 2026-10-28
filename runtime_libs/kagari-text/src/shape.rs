//! Shaping text into graphemes with positioned glyphs, through harfrust.

use std::ops::Range;

use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag};
use unicode_segmentation::UnicodeSegmentation;

use crate::fonts::Fonts;
use crate::{FontId, is_newline};

/// How a grapheme is set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Orient {
    /// Left to right on a horizontal line.
    Horizontal,
    /// Upright in a vertical column, shaped top to bottom.
    Upright,
    /// Shaped left to right and turned 90° clockwise in a vertical column.
    Rotated,
}

/// Which orientation the graphemes of a text take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Horizontal,
    /// Upright for CJK scripts, rotated otherwise.
    Vertical,
    /// Upright whatever the script (emphasis dots).
    ForceUpright,
}

/// One glyph in px; offsets and advances as harfrust reports them (y up).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct ShapedGlyph {
    pub glyph: u32,
    pub x_off: f32,
    pub y_off: f32,
    pub x_adv: f32,
    pub y_adv: f32,
}

/// One extended grapheme cluster of shaped text.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ShapedGrapheme {
    /// Byte range in the shaped text.
    pub range: Range<usize>,
    pub newline: bool,
    pub font: Option<FontId>,
    pub orient: Orient,
    pub glyphs: Vec<ShapedGlyph>,
    /// Advance along the line or column, without letter spacing.
    pub adv: f32,
}

/// Graphemes and glyphs of `text`. Fonts follow [`Fonts::pick`] from `preferred`; `bold` shapes
/// with the `wght` 700 instance where the font has one.
pub(crate) fn shape_text(
    fonts: &Fonts,
    text: &str,
    preferred: FontId,
    fallback: FontId,
    size_px: f32,
    bold: bool,
    mode: Mode,
) -> Vec<ShapedGrapheme> {
    let mut graphemes: Vec<ShapedGrapheme> = text
        .grapheme_indices(true)
        .map(|(start, g)| {
            let newline = is_newline(g);
            ShapedGrapheme {
                range: start..start.saturating_add(g.len()),
                newline,
                font: if newline {
                    None
                } else {
                    fonts.pick(preferred, fallback, g)
                },
                orient: orient_of(g, mode),
                glyphs: Vec::new(),
                adv: 0.0,
            }
        })
        .collect();
    let starts: Vec<usize> = graphemes.iter().map(|g| g.range.start).collect();
    let mut first = 0;
    while first < graphemes.len() {
        let Some(head) = graphemes.get(first) else {
            break;
        };
        let (font, orient, newline) = (head.font, head.orient, head.newline);
        let end = graphemes
            .iter()
            .skip(first)
            .position(|g| g.newline || g.font != font || g.orient != orient)
            .map_or(graphemes.len(), |n| first.saturating_add(n.max(1)));
        if let (false, Some(font)) = (newline, font) {
            let start_byte = starts.get(first).copied().unwrap_or(0);
            let end_byte = graphemes
                .get(end.saturating_sub(1))
                .map_or(start_byte, |g| g.range.end);
            let run = text.get(start_byte..end_byte).unwrap_or("");
            for (cluster, glyph) in shape_run(fonts, font, bold, size_px, orient, run, start_byte) {
                let index = starts.partition_point(|s| *s <= cluster).saturating_sub(1);
                if let Some(g) = graphemes.get_mut(index) {
                    g.adv += match orient {
                        Orient::Upright => -glyph.y_adv,
                        Orient::Horizontal | Orient::Rotated => glyph.x_adv,
                    };
                    g.glyphs.push(glyph);
                }
            }
        }
        first = end.max(first.saturating_add(1));
    }
    graphemes
}

/// Upright in vertical text: Han, Hiragana, Katakana, CJK symbols and punctuation, full-width
/// forms and Hangul.
fn orient_of(grapheme: &str, mode: Mode) -> Orient {
    match mode {
        Mode::Horizontal => Orient::Horizontal,
        Mode::ForceUpright => Orient::Upright,
        Mode::Vertical => {
            let upright = grapheme.chars().next().is_some_and(|ch| {
                matches!(ch as u32,
                    0x1100..=0x11FF | 0x3000..=0x303F | 0x3040..=0x309F | 0x30A0..=0x30FF
                    | 0x3130..=0x318F | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
                    | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
                    | 0x20000..=0x3FFFF)
            });
            if upright {
                Orient::Upright
            } else {
                Orient::Rotated
            }
        }
    }
}

/// Shapes one run in one font; returns `(cluster byte offset, glyph)` pairs. `base` is the run's
/// byte offset in the text, so clusters index the whole text.
fn shape_run(
    fonts: &Fonts,
    font: FontId,
    bold: bool,
    size_px: f32,
    orient: Orient,
    run: &str,
    base: usize,
) -> Vec<(usize, ShapedGlyph)> {
    let Some(entry) = fonts.entry(font) else {
        return Vec::new();
    };
    let face = if bold {
        entry.bold.as_ref().unwrap_or(&entry.font)
    } else {
        &entry.font
    };
    let upem = face.units_per_em().max(1);
    let shaper = ShaperFont::new(face).with_scale(i32::from(upem));
    let mut buffer = Buffer::new();
    for (offset, ch) in run.char_indices() {
        let cluster = u32::try_from(base.saturating_add(offset)).unwrap_or(u32::MAX);
        buffer.push(u32::from(ch), cluster);
    }
    let vertical = orient == Orient::Upright;
    buffer.set_direction(if vertical {
        Direction::TopToBottom
    } else {
        Direction::LeftToRight
    });
    buffer.guess_segment_properties();
    let features = if vertical {
        vec![Feature::new(Tag::new(b"vert"), 1, ..)]
    } else {
        Vec::new()
    };
    if harfrust::shape(
        &shaper,
        &mut buffer,
        ShapeOptions::new().features(&features),
    )
    .is_err()
    {
        return Vec::new();
    }
    let scale = size_px / f32::from(upem);
    buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions())
        .map(|(info, pos)| {
            let glyph = ShapedGlyph {
                glyph: info.glyph_id,
                x_off: pos.x_offset as f32 * scale,
                y_off: pos.y_offset as f32 * scale,
                x_adv: pos.x_advance as f32 * scale,
                y_adv: pos.y_advance as f32 * scale,
            };
            (usize::try_from(info.cluster).unwrap_or(usize::MAX), glyph)
        })
        .collect()
}
