//! Paragraph layout: horizontal lines and vertical columns, ruby and emphasis dots.

use std::ops::Range;

use crate::breaking::{BreakItem, Line, break_lines, is_space};
use crate::fonts::{Fonts, LineMetrics};
use crate::shape::{Mode, Orient, ShapedGlyph, ShapedGrapheme, shape_text};
use crate::{Align, FontId, Glyph, GlyphKey, Layout, LayoutMetrics, Paragraph, RubyText, Span};

/// The emphasis dot, and the one used when no font has it.
const SESAME_DOT: &str = "\u{FE45}";
const BULLET: &str = "\u{2022}";

/// One base grapheme of the paragraph.
struct Gr {
    span: usize,
    /// Byte range in the concatenated base text.
    range: Range<usize>,
    newline: bool,
    unit: u32,
    font: Option<FontId>,
    orient: Orient,
    glyphs: Vec<ShapedGlyph>,
    /// Shaped advance.
    adv: f32,
    /// Advance plus letter spacing (0 for a newline).
    spaced: f32,
    /// Ruby padding before and after.
    lead: f32,
    trail: f32,
    /// The ruby base the grapheme belongs to.
    base: Option<usize>,
    /// Whether a reading sits over the grapheme.
    has_ruby: bool,
}

impl Gr {
    fn extent(&self) -> f32 {
        self.lead + self.spaced + self.trail
    }
}

/// A reading over graphemes `graphemes` (a whole group base, or one mono-ruby grapheme).
struct Piece {
    graphemes: Range<usize>,
    reading: Vec<ShapedGrapheme>,
    len: f32,
}

/// Where a grapheme landed: main-axis start of its cell and of its body (after the lead), the
/// cross position (baseline, or column centre when vertical) and its line.
#[derive(Clone, Copy, Default)]
struct Place {
    cell: f32,
    body: f32,
    cross: f32,
    line: usize,
}

/// Lays `p` out into positioned glyphs.
pub fn layout(fonts: &Fonts, p: &Paragraph) -> (Layout, LayoutMetrics) {
    let mode = if p.style.vertical {
        Mode::Vertical
    } else {
        Mode::Horizontal
    };
    let (text, mut grs) = shape_base(fonts, p, mode);
    let pieces = rubies(fonts, p, &mut grs, mode);
    let items: Vec<BreakItem<'_>> = grs
        .iter()
        .map(|g| BreakItem {
            text: text.get(g.range.clone()).unwrap_or(""),
            start: g.range.start,
            newline: g.newline,
            extent: g.extent(),
            advance: g.spaced,
            ruby: g.base,
        })
        .collect();
    let lines = break_lines(&text, &items, p.style.limit, p.style.hanging_indent);
    let mut ctx = Ctx {
        fonts,
        p,
        text: &text,
        grs: &grs,
        places: vec![Place::default(); grs.len()],
        glyphs: Vec::new(),
    };
    let (size, end) = if p.style.vertical {
        ctx.place_columns(&lines)
    } else {
        ctx.place_lines(&lines)
    };
    for piece in &pieces {
        ctx.place_ruby(piece, mode);
    }
    ctx.place_dots(mode);
    let units = grs.iter().filter(|g| !g.newline).count();
    let layout = Layout {
        size,
        units: u32::try_from(units).unwrap_or(u32::MAX),
        glyphs: ctx.glyphs,
    };
    (layout, LayoutMetrics { end })
}

/// Shapes every span and numbers the units; returns the concatenated base text.
fn shape_base(fonts: &Fonts, p: &Paragraph, mode: Mode) -> (String, Vec<Gr>) {
    let mut text = String::new();
    let mut grs = Vec::new();
    let mut unit = 0u32;
    for (index, span) in p.spans.iter().enumerate() {
        let offset = text.len();
        text.push_str(&span.text);
        let shaped = shape_text(
            fonts,
            &span.text,
            span.font,
            p.fallback,
            span.size_px,
            span.bold,
            mode,
        );
        for g in shaped {
            let spaced = if g.newline {
                0.0
            } else {
                g.adv + p.style.letter_spacing
            };
            grs.push(Gr {
                span: index,
                range: g.range.start.saturating_add(offset)..g.range.end.saturating_add(offset),
                newline: g.newline,
                unit,
                font: g.font,
                orient: g.orient,
                glyphs: g.glyphs,
                adv: g.adv,
                spaced,
                lead: 0.0,
                trail: 0.0,
                base: None,
                has_ruby: false,
            });
            if !g.newline {
                unit = unit.saturating_add(1);
            }
        }
    }
    (text, grs)
}

/// Groups ruby bases, shapes their readings and pads bases narrower than their reading.
fn rubies(fonts: &Fonts, p: &Paragraph, grs: &mut [Gr], mode: Mode) -> Vec<Piece> {
    let mut bases: Vec<(u32, Range<usize>)> = Vec::new();
    let mut previous: Option<u32> = None;
    for i in 0..grs.len() {
        let Some(gr) = grs.get(i) else { break };
        let ruby = if gr.newline {
            None
        } else {
            p.spans.get(gr.span).and_then(|s| s.ruby)
        };
        match ruby {
            Some(r) if previous == Some(r) => {
                if let Some(last) = bases.last_mut() {
                    last.1.end = i.saturating_add(1);
                }
            }
            Some(r) => bases.push((r, i..i.saturating_add(1))),
            None => {}
        }
        previous = ruby;
    }
    let mut pieces = Vec::new();
    for (index, (ruby, range)) in bases.into_iter().enumerate() {
        for g in grs.get_mut(range.clone()).into_iter().flatten() {
            g.base = Some(index);
        }
        let Some(first) = grs.get(range.start) else {
            continue;
        };
        let Some(span) = p.spans.get(first.span) else {
            continue;
        };
        let readings: Vec<(Range<usize>, &str)> =
            match p.rubies.get(usize::try_from(ruby).unwrap_or(usize::MAX)) {
                Some(RubyText::Group(reading)) => vec![(range.clone(), reading.as_str())],
                Some(RubyText::Mono(readings)) => range
                    .clone()
                    .zip(readings)
                    .map(|(i, reading)| (i..i.saturating_add(1), reading.as_str()))
                    .collect(),
                None => Vec::new(),
            };
        for (graphemes, reading) in readings {
            if reading.is_empty() {
                continue;
            }
            let shaped = shape_text(
                fonts,
                reading,
                span.font,
                p.fallback,
                p.style.ruby_size_px,
                false,
                mode,
            );
            let len: f32 = shaped.iter().map(|g| g.adv).sum();
            let Some(cell) = grs.get_mut(graphemes.clone()) else {
                continue;
            };
            let base: f32 = cell.iter().map(|g| g.spaced).sum();
            for g in cell.iter_mut() {
                g.has_ruby = true;
            }
            if len > base {
                let pad = (len - base) / 2.0;
                if let Some(g) = cell.first_mut() {
                    g.lead += pad;
                }
                if let Some(g) = cell.last_mut() {
                    g.trail += pad;
                }
            }
            pieces.push(Piece {
                graphemes,
                reading: shaped,
                len,
            });
        }
    }
    pieces
}

/// Size and ascent of one line or column.
struct LineBox {
    size: f32,
    metrics: LineMetrics,
    ruby: bool,
    /// Main-axis length with the indent, without trailing spaces.
    length: f32,
}

/// Glyph style shared by every glyph of a span.
struct Style<'a> {
    span: &'a Span,
    font: FontId,
    size_px: f32,
    embolden: bool,
    skew: bool,
    unit: u32,
}

struct Ctx<'a> {
    fonts: &'a Fonts,
    p: &'a Paragraph,
    text: &'a str,
    grs: &'a [Gr],
    places: Vec<Place>,
    glyphs: Vec<Glyph>,
}

impl<'a> Ctx<'a> {
    fn span(&self, index: usize) -> Option<&'a Span> {
        self.p.spans.get(index)
    }

    fn metrics_of(&self, g: &Gr) -> LineMetrics {
        match self.span(g.span) {
            Some(span) => self
                .fonts
                .line_metrics(g.font.or(Some(span.font)), span.size_px),
            None => LineMetrics::fallback(0.0),
        }
    }

    /// The line's box; an empty line takes its newline's span, else the previous grapheme's.
    fn line_box(&self, line: &Line) -> LineBox {
        let mut size = 0.0f32;
        let mut metrics = LineMetrics {
            asc: 0.0,
            desc: 0.0,
        };
        let mut ruby = false;
        let graphemes = self.grs.get(line.range.clone()).unwrap_or(&[]);
        for g in graphemes {
            let m = self.metrics_of(g);
            size = size.max(self.span(g.span).map_or(0.0, |s| s.size_px));
            metrics.asc = metrics.asc.max(m.asc);
            metrics.desc = metrics.desc.max(m.desc);
            ruby |= g.has_ruby;
        }
        if graphemes.is_empty() {
            let reference = line
                .newline
                .or_else(|| line.range.start.checked_sub(1))
                .and_then(|i| self.grs.get(i));
            if let Some(g) = reference {
                size = self.span(g.span).map_or(0.0, |s| s.size_px);
                metrics = self.metrics_of(g);
            } else if let Some(span) = self.p.spans.first() {
                size = span.size_px;
                metrics = self.fonts.line_metrics(Some(span.font), span.size_px);
            }
        }
        let trailing = graphemes
            .iter()
            .rev()
            .take_while(|g| is_space(self.text.get(g.range.clone()).unwrap_or("")))
            .count();
        let kept = graphemes.len().saturating_sub(trailing);
        let length = line.indent + graphemes.iter().take(kept).map(Gr::extent).sum::<f32>();
        LineBox {
            size,
            metrics,
            ruby,
            length,
        }
    }

    fn align(&self, total: f32, length: f32) -> f32 {
        match self.p.style.align {
            Align::Start => 0.0,
            Align::Center => (total - length) / 2.0,
            Align::End => total - length,
        }
    }

    fn base_style(&self, g: &Gr) -> Option<Style<'a>> {
        let span = self.span(g.span)?;
        Some(Style {
            span,
            font: g.font?,
            size_px: span.size_px,
            embolden: span.bold,
            skew: span.italic,
            unit: g.unit,
        })
    }

    fn push(&mut self, style: &Style<'_>, glyph: &ShapedGlyph, rotate90: bool, pos: [f32; 2]) {
        self.glyphs.push(Glyph {
            key: GlyphKey {
                font: style.font,
                glyph: glyph.glyph,
                size_q: (style.size_px * 64.0).round() as u32,
                embolden: style.embolden,
                skew: style.skew,
                rotate90,
            },
            pos,
            color: style.span.color,
            outline: style.span.outline,
            shadow: style.span.shadow,
            unit: style.unit,
        });
    }

    /// Horizontal lines, top to bottom; returns the size and the end pen.
    fn place_lines(&mut self, lines: &[Line]) -> ([f32; 2], [f32; 2]) {
        let boxes: Vec<LineBox> = lines.iter().map(|l| self.line_box(l)).collect();
        let width = boxes.iter().map(|b| b.length).fold(0.0f32, f32::max);
        let ruby_px = self.p.style.ruby_size_px;
        let mut top = 0.0f32;
        let mut end = [0.0f32; 2];
        for (index, (line, b)) in lines.iter().zip(&boxes).enumerate() {
            let ruby = if b.ruby { ruby_px } else { 0.0 };
            let content = b.metrics.asc + b.metrics.desc + ruby;
            let height = (self.p.style.line_height * b.size).max(content);
            let baseline = top + ruby + (height - content) / 2.0 + b.metrics.asc;
            let mut pen = self.align(width, b.length) + line.indent;
            for i in line.range.clone() {
                let Some(g) = self.grs.get(i) else { continue };
                let body = pen + g.lead;
                if let Some(place) = self.places.get_mut(i) {
                    *place = Place {
                        cell: pen,
                        body,
                        cross: baseline,
                        line: index,
                    };
                }
                if let Some(style) = self.base_style(g) {
                    let mut x = body;
                    for glyph in &g.glyphs {
                        self.push(
                            &style,
                            glyph,
                            false,
                            [x + glyph.x_off, baseline - glyph.y_off],
                        );
                        x += glyph.x_adv;
                    }
                }
                pen += g.extent();
            }
            end = [pen, baseline];
            top += height;
        }
        ([width, top], end)
    }

    /// Vertical columns, right to left; returns the size and the end pen.
    fn place_columns(&mut self, lines: &[Line]) -> ([f32; 2], [f32; 2]) {
        let boxes: Vec<LineBox> = lines.iter().map(|l| self.line_box(l)).collect();
        let length = boxes.iter().map(|b| b.length).fold(0.0f32, f32::max);
        let widths: Vec<f32> = boxes
            .iter()
            .map(|b| self.p.style.line_height * b.size)
            .collect();
        let total: f32 = widths.iter().sum();
        let mut right = total;
        let mut end = [0.0f32; 2];
        for (index, ((line, b), w)) in lines.iter().zip(&boxes).zip(&widths).enumerate() {
            let cx = right - w / 2.0;
            let mut pen = self.align(length, b.length) + line.indent;
            for i in line.range.clone() {
                let Some(g) = self.grs.get(i) else { continue };
                let body = pen + g.lead;
                if let Some(place) = self.places.get_mut(i) {
                    *place = Place {
                        cell: pen,
                        body,
                        cross: cx,
                        line: index,
                    };
                }
                if let Some(style) = self.base_style(g) {
                    let m = self.metrics_of(g);
                    self.push_column(&style, g.orient, &g.glyphs, cx, body, m);
                }
                pen += g.extent();
            }
            end = [cx, pen];
            right -= w;
        }
        ([total, length], end)
    }

    /// Glyphs of one grapheme in a column centred on `cx`, from `y` down.
    fn push_column(
        &mut self,
        style: &Style<'_>,
        orient: Orient,
        glyphs: &[ShapedGlyph],
        cx: f32,
        y: f32,
        m: LineMetrics,
    ) {
        let mut pen = y;
        for glyph in glyphs {
            if orient == Orient::Rotated {
                let ox = cx - (m.asc - m.desc) / 2.0;
                self.push(style, glyph, true, [ox + glyph.y_off, pen + glyph.x_off]);
                pen += glyph.x_adv;
            } else {
                self.push(style, glyph, false, [cx + glyph.x_off, pen - glyph.y_off]);
                pen -= glyph.y_adv;
            }
        }
    }

    /// The reading of `piece`, centred over the part of its base on the base's first line.
    fn place_ruby(&mut self, piece: &Piece, mode: Mode) {
        let Some(first) = self.grs.get(piece.graphemes.start) else {
            return;
        };
        let Some(place) = self.places.get(piece.graphemes.start).copied() else {
            return;
        };
        let Some(span) = self.span(first.span) else {
            return;
        };
        let last = piece
            .graphemes
            .clone()
            .rfind(|i| self.places.get(*i).is_some_and(|p| p.line == place.line))
            .unwrap_or(piece.graphemes.start);
        let cell_end = match (self.places.get(last), self.grs.get(last)) {
            (Some(p), Some(g)) => p.cell + g.extent(),
            _ => place.cell,
        };
        let mut pen = place.cell + (cell_end - place.cell - piece.len) / 2.0;
        let ruby_px = self.p.style.ruby_size_px;
        let base_metrics = self.metrics_of(first);
        let (size_px, unit) = (span.size_px, first.unit);
        for g in &piece.reading {
            let Some(font) = g.font else {
                pen += g.adv;
                continue;
            };
            let style = Style {
                span,
                font,
                size_px: ruby_px,
                embolden: false,
                skew: false,
                unit,
            };
            let m = self.fonts.line_metrics(Some(font), ruby_px);
            if mode == Mode::Horizontal {
                let baseline = place.cross - base_metrics.asc - m.desc;
                let mut x = pen;
                for glyph in &g.glyphs {
                    self.push(
                        &style,
                        glyph,
                        false,
                        [x + glyph.x_off, baseline - glyph.y_off],
                    );
                    x += glyph.x_adv;
                }
            } else {
                let band = place.cross + size_px / 2.0 + ruby_px / 2.0;
                self.push_column(&style, g.orient, &g.glyphs, band, pen, m);
            }
            pen += g.adv;
        }
    }

    /// Emphasis dots over every dotted grapheme without a reading.
    fn place_dots(&mut self, mode: Mode) {
        let mut cache: Option<(usize, Vec<ShapedGrapheme>)> = None;
        for (i, g) in self.grs.iter().enumerate() {
            let Some(span) = self.span(g.span) else {
                continue;
            };
            let text = self.text.get(g.range.clone()).unwrap_or("");
            if !span.emphasis_dots || g.newline || g.has_ruby || text.trim().is_empty() {
                continue;
            }
            let Some(place) = self.places.get(i).copied() else {
                continue;
            };
            if cache.as_ref().is_none_or(|(s, _)| *s != g.span) {
                let dot_px = span.size_px / 2.0;
                let sesame = self
                    .fonts
                    .pick(span.font, self.p.fallback, SESAME_DOT)
                    .is_some_and(|f| self.fonts.covers(f, SESAME_DOT));
                let dot = if sesame { SESAME_DOT } else { BULLET };
                let dot_mode = if mode == Mode::Horizontal {
                    Mode::Horizontal
                } else {
                    Mode::ForceUpright
                };
                let shaped = shape_text(
                    self.fonts,
                    dot,
                    span.font,
                    self.p.fallback,
                    dot_px,
                    false,
                    dot_mode,
                );
                cache = Some((g.span, shaped));
            }
            let Some((_, shaped)) = cache.as_ref() else {
                continue;
            };
            let Some(dot) = shaped.first() else { continue };
            let Some(font) = dot.font else { continue };
            let dot_px = span.size_px / 2.0;
            let style = Style {
                span,
                font,
                size_px: dot_px,
                embolden: false,
                skew: false,
                unit: g.unit,
            };
            let centre = place.body + g.adv / 2.0;
            let start = centre - dot.adv / 2.0;
            let mut out = Vec::new();
            if mode == Mode::Horizontal {
                let base = self.metrics_of(g);
                let m = self.fonts.line_metrics(Some(font), dot_px);
                let baseline = place.cross - base.asc - m.desc;
                let mut x = start;
                for glyph in &dot.glyphs {
                    out.push((*glyph, false, [x + glyph.x_off, baseline - glyph.y_off]));
                    x += glyph.x_adv;
                }
            } else {
                let band = place.cross + span.size_px / 2.0 + self.p.style.ruby_size_px / 2.0;
                let mut y = start;
                for glyph in &dot.glyphs {
                    out.push((*glyph, false, [band + glyph.x_off, y - glyph.y_off]));
                    y -= glyph.y_adv;
                }
            }
            for (glyph, rotate90, pos) in out {
                let key = GlyphKey {
                    font: style.font,
                    glyph: glyph.glyph,
                    size_q: (style.size_px * 64.0).round() as u32,
                    embolden: false,
                    skew: false,
                    rotate90,
                };
                self.glyphs.push(Glyph {
                    key,
                    pos,
                    color: span.color,
                    outline: span.outline,
                    shadow: span.shadow,
                    unit: style.unit,
                });
            }
        }
    }
}
