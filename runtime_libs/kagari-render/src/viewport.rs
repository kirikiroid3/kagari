//! Where the design box lands on the surface.

/// The rectangle of the surface the design target is drawn into, in surface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub surface: (u32, u32),
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    /// The largest rectangle of the design aspect ratio that fits the surface, centred.
    pub fn letterbox(design: (u32, u32), surface: (u32, u32)) -> Viewport {
        let (dw, dh) = design;
        let (sw, sh) = surface;
        if dw == 0 || dh == 0 || sw == 0 || sh == 0 {
            return Viewport {
                surface: (0, 0),
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
        }
        let s = (f64::from(sw) / f64::from(dw)).min(f64::from(sh) / f64::from(dh));
        let width = ((f64::from(dw) * s).round() as u32).min(sw);
        let height = ((f64::from(dh) * s).round() as u32).min(sh);
        Viewport {
            surface,
            x: sw.saturating_sub(width) / 2,
            y: sh.saturating_sub(height) / 2,
            width,
            height,
        }
    }

    /// The whole of a target of `size`.
    pub fn full(size: (u32, u32)) -> Viewport {
        Viewport {
            surface: size,
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        }
    }

    /// A surface pixel position in design pixels; not clamped. `(-1, -1)` for an empty viewport.
    pub fn to_design(&self, design: (u32, u32), px: f64, py: f64) -> (f32, f32) {
        if self.width == 0 || self.height == 0 {
            return (-1.0, -1.0);
        }
        let x = (px - f64::from(self.x)) * f64::from(design.0) / f64::from(self.width);
        let y = (py - f64::from(self.y)) * f64::from(design.1) / f64::from(self.height);
        (x as f32, y as f32)
    }
}
