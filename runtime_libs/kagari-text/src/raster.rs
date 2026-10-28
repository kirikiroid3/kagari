//! Glyph coverage masks: skrifa outlines filled or stroked by zeno.

use skrifa::{DrawMemory, GlyphId, PathSink};
use zeno::{Command, Point, Stroke};

use crate::fonts::Entry;
use crate::{GlyphKey, Mask};

/// Horizontal shear of synthetic italics, per px of height above the baseline.
const ITALIC_SHEAR: f32 = 0.2;

/// Collects an outline as zeno commands in y-down px.
struct Sink {
    commands: Vec<Command>,
    scale: f32,
    skew: bool,
    rotate90: bool,
}

impl Sink {
    fn point(&self, x: f32, y: f32) -> Point {
        let mut x = x * self.scale;
        let y_up = y * self.scale;
        if self.skew {
            x += ITALIC_SHEAR * y_up;
        }
        if self.rotate90 {
            Point::new(y_up, x)
        } else {
            Point::new(x, -y_up)
        }
    }
}

impl PathSink for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.commands.push(Command::MoveTo(p));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.commands.push(Command::LineTo(p));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.point(cx0, cy0), self.point(x, y));
        self.commands.push(Command::QuadTo(c, p));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
        self.commands.push(Command::CurveTo(c0, c1, p));
    }

    fn close(&mut self) {
        self.commands.push(Command::Close);
    }
}

/// A zeno render as `(alpha, placement)`.
type Rendered = (Vec<u8>, zeno::Placement);

fn fill(commands: &[Command]) -> Rendered {
    zeno::Mask::new(commands).render()
}

fn stroke(commands: &[Command], width: f32) -> Rendered {
    zeno::Mask::new(commands).style(Stroke::new(width)).render()
}

/// `over` drawn into `base` (per-pixel max), `over` lying inside `base`'s bounds.
fn max_into(base: &mut Rendered, over: &Rendered) {
    let (dx, dy) = (
        i64::from(over.1.left).saturating_sub(i64::from(base.1.left)),
        i64::from(over.1.top).saturating_sub(i64::from(base.1.top)),
    );
    let base_width = i64::from(base.1.width);
    let base_height = i64::from(base.1.height);
    let over_width = usize::try_from(over.1.width).unwrap_or(0);
    if over_width == 0 {
        return;
    }
    for (row, line) in over.0.chunks(over_width).enumerate() {
        let Some(y) = i64::try_from(row).ok().and_then(|r| r.checked_add(dy)) else {
            continue;
        };
        if !(0..base_height).contains(&y) {
            continue;
        }
        for (col, value) in line.iter().enumerate() {
            let Some(x) = i64::try_from(col).ok().and_then(|c| c.checked_add(dx)) else {
                continue;
            };
            if !(0..base_width).contains(&x) {
                continue;
            }
            let index = y.checked_mul(base_width).and_then(|i| i.checked_add(x));
            if let Some(slot) = index
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| base.0.get_mut(i))
            {
                *slot = (*slot).max(*value);
            }
        }
    }
}

pub(crate) fn rasterize(entry: &Entry, key: GlyphKey, outline_px: f32) -> Option<Mask> {
    let size = key.size_q as f32 / 64.0;
    let (scaler, synthetic_bold) = match (&entry.bold_scaler, key.embolden) {
        (Some(bold), true) => (bold, false),
        (_, embolden) => (&entry.scaler, embolden),
    };
    let mut sink = Sink {
        commands: Vec::new(),
        scale: size / entry.units_per_em,
        skew: key.skew,
        rotate90: key.rotate90,
    };
    scaler
        .outline(GlyphId::new(key.glyph))?
        .draw(DrawMemory::Scratch, &mut sink)
        .ok()?;
    if sink.commands.is_empty() {
        return Some(Mask::default());
    }
    let embolden_px = size / 24.0;
    let rendered = if outline_px > 0.0 {
        let width = 2.0 * outline_px + if synthetic_bold { embolden_px } else { 0.0 };
        stroke(&sink.commands, width)
    } else if synthetic_bold {
        let mut bold = stroke(&sink.commands, embolden_px);
        max_into(&mut bold, &fill(&sink.commands));
        bold
    } else {
        fill(&sink.commands)
    };
    let (alpha, placement) = rendered;
    Some(Mask {
        width: placement.width,
        height: placement.height,
        left: placement.left,
        top: placement.top.checked_neg().unwrap_or(i32::MAX),
        alpha,
    })
}
