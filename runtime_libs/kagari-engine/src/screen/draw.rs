//! A sampled screen as `Frame` nodes: the camera group over the Background, Stage and Front
//! planes, and the OverWindow plane outside the camera.

use std::rc::Rc;

use kagari_vm::{AssetId, Text, Value};

use super::sample::{VisualCamera, VisualEntity, VisualLayer, VisualScreen};
use crate::anim::color_premul_linear;
use crate::assets::AssetStore;
use crate::frame::{Affine, Blend, Dir, Node, Rect, TransitionDraw};
use crate::system::SystemState;
use crate::ui::text::TextSystem;
use crate::values::{BlendMode, Direction, Flip, Plane, TransitionKind};

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;

/// The screen's nodes, split around the text window.
pub(crate) struct ScreenPlanes {
    /// The camera group over Background, Stage and Front.
    pub world: Node,
    /// OverWindow entities, not under the camera.
    pub over_window: Vec<Node>,
}

/// Draws a sampled screen at the design `resolution`.
pub(crate) fn draw_screen(
    v: &VisualScreen,
    resolution: (u32, u32),
    assets: &AssetStore,
    text: &mut TextSystem,
    system: &SystemState,
) -> ScreenPlanes {
    let size = [resolution.0 as f32, resolution.1 as f32];
    let mut world = Vec::new();
    let mut over_window = Vec::new();
    for entity in &v.entities {
        let Some(node) = entity_node(entity, size, assets, text, system) else {
            continue;
        };
        if entity.plane == Plane::OverWindow {
            over_window.push(node);
        } else {
            world.push(node);
        }
    }
    ScreenPlanes {
        world: Node::Group {
            transform: camera_transform(&v.camera, size),
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            children: world,
        },
        over_window,
    }
}

/// One image layer at its natural size with its top-left corner at `origin`.
pub(crate) fn image_layer_node(
    layer: &VisualLayer<AssetId>,
    origin: [f32; 2],
    assets: &AssetStore,
) -> Option<Node> {
    layer_node(layer, |id| natural_rect(assets, id, origin), false, false)
}

/// `T(c)·S(zoom)·R(−rot)·T(−c)·T(−offset)`, with `c` the screen centre.
pub(crate) fn camera_transform(camera: &VisualCamera, size: [f32; 2]) -> Affine {
    let c = [size[0] / 2.0, size[1] / 2.0];
    [
        translate(c),
        scale(camera.zoom),
        rotate(-camera.rotation_deg),
        translate([-c[0], -c[1]]),
        translate([-camera.offset[0], -camera.offset[1]]),
    ]
    .into_iter()
    .fold(Affine::IDENTITY, mul)
}

/// `p ∘ q`: applies `q` first.
fn mul(p: Affine, q: Affine) -> Affine {
    let [pa, pb, pc, pd, pe, pf] = p.0;
    let [qa, qb, qc, qd, qe, qf] = q.0;
    Affine([
        pa * qa + pc * qb,
        pb * qa + pd * qb,
        pa * qc + pc * qd,
        pb * qc + pd * qd,
        pa * qe + pc * qf + pe,
        pb * qe + pd * qf + pf,
    ])
}

fn translate(t: [f32; 2]) -> Affine {
    Affine([1.0, 0.0, 0.0, 1.0, t[0], t[1]])
}

fn scale(s: f32) -> Affine {
    Affine([s, 0.0, 0.0, s, 0.0, 0.0])
}

/// Clockwise in y-down coordinates.
fn rotate(degrees: f32) -> Affine {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Affine([cos, sin, -sin, cos, 0.0, 0.0])
}

fn natural_rect(assets: &AssetStore, id: AssetId, origin: [f32; 2]) -> Option<Rect> {
    let Some((w, h)) = assets.image_size(id) else {
        log::debug!("image `{}` not loaded; skipped", assets.path(id));
        return None;
    };
    Some(Rect {
        x: origin[0],
        y: origin[1],
        w: w as f32,
        h: h as f32,
    })
}

fn image_node(id: AssetId, rect: Rect, flip_x: bool, flip_y: bool) -> Node {
    Node::Image {
        rect,
        image: id,
        uv: Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        flip_x,
        flip_y,
        tint: [1.0; 4],
    }
}

/// An image layer whose image `id` is drawn at `rect(id)`.
fn layer_node(
    layer: &VisualLayer<AssetId>,
    rect: impl Fn(AssetId) -> Option<Rect>,
    flip_x: bool,
    flip_y: bool,
) -> Option<Node> {
    let image = |id: &Option<AssetId>| -> Option<(Rect, Node)> {
        let id = (*id)?;
        let r = rect(id)?;
        Some((r, image_node(id, r, flip_x, flip_y)))
    };
    match layer {
        VisualLayer::Empty => None,
        VisualLayer::Shown(id) => image(&Some(*id)).map(|(_, node)| node),
        VisualLayer::Transition {
            from,
            to,
            kind,
            progress,
            hold_fraction,
        } => {
            let from = image(from);
            let to = image(to);
            transition_node(from, to, *kind, *progress, *hold_fraction)
        }
    }
}

/// A `Node::Transition` over the union of both sides' rects; a `Cut` shows `to`.
fn transition_node(
    from: Option<(Rect, Node)>,
    to: Option<(Rect, Node)>,
    kind: TransitionKind,
    progress: f32,
    hold_fraction: f32,
) -> Option<Node> {
    let kind = match kind {
        TransitionKind::Cut => return to.map(|(_, node)| node),
        TransitionKind::Crossfade => TransitionDraw::Crossfade,
        TransitionKind::Rule {
            mask,
            softness,
            reverse,
        } => TransitionDraw::Rule {
            mask,
            softness,
            reverse,
        },
        TransitionKind::Wipe(direction) => TransitionDraw::Wipe {
            dir: dir(direction),
        },
        TransitionKind::Blinds { direction, count } => TransitionDraw::Blinds {
            dir: dir(direction),
            count,
        },
        TransitionKind::ThroughColor { color, .. } => TransitionDraw::ThroughColor {
            color: color_premul_linear(color),
            hold_fraction,
        },
    };
    let bounds = match (&from, &to) {
        (Some((a, _)), Some((b, _))) => union(*a, *b),
        (Some((r, _)), None) | (None, Some((r, _))) => *r,
        (None, None) => return None,
    };
    Some(Node::Transition {
        bounds,
        kind,
        progress,
        from: from.map(|(_, node)| node).into_iter().collect(),
        to: to.map(|(_, node)| node).into_iter().collect(),
    })
}

fn dir(d: Direction) -> Dir {
    match d {
        Direction::Left => Dir::Left,
        Direction::Right => Dir::Right,
        Direction::Up => Dir::Up,
        Direction::Down => Dir::Down,
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    Rect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

fn blend(mode: BlendMode) -> Blend {
    match mode {
        BlendMode::Normal => Blend::Normal,
        BlendMode::Add => Blend::Add,
        BlendMode::Multiply => Blend::Multiply,
        BlendMode::Screen => Blend::Screen,
        BlendMode::Overlay => Blend::Overlay,
        BlendMode::Darken => Blend::Darken,
        BlendMode::Lighten => Blend::Lighten,
        BlendMode::ColorDodge => Blend::ColorDodge,
        BlendMode::ColorBurn => Blend::ColorBurn,
        BlendMode::HardLight => Blend::HardLight,
        BlendMode::SoftLight => Blend::SoftLight,
        BlendMode::Difference => Blend::Difference,
        BlendMode::Exclusion => Blend::Exclusion,
        BlendMode::Hue => Blend::Hue,
        BlendMode::Saturation => Blend::Saturation,
        BlendMode::Color => Blend::Color,
        BlendMode::Luminosity => Blend::Luminosity,
    }
}

/// The label layer's layouts: `(from, to)` during a transition, else `(None, shown)`.
type LabelLayouts = (
    Option<Rc<kagari_text::Layout>>,
    Option<Rc<kagari_text::Layout>>,
);

fn label_layouts(e: &VisualEntity, assets: &AssetStore, text: &mut TextSystem) -> LabelLayouts {
    let mut lay = |t: &Text| text.layout_label(assets, t, e.label_style.as_ref(), e.label_width);
    match &e.label {
        VisualLayer::Empty => (None, None),
        VisualLayer::Shown(t) => (None, Some(lay(t))),
        VisualLayer::Transition { from, to, .. } => {
            (from.as_ref().map(&mut lay), to.as_ref().map(&mut lay))
        }
    }
}

fn text_node(layout: Rc<kagari_text::Layout>) -> (Rect, Node) {
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: layout.size[0],
        h: layout.size[1],
    };
    let node = Node::Text {
        origin: [0.0, 0.0],
        layout,
        reveal: crate::frame::Reveal::All,
        opacity: 1.0,
    };
    (rect, node)
}

/// The size of the MAIN layer's image: `to` during a transition, else `from`.
fn main_size(e: &VisualEntity, assets: &AssetStore) -> Option<(u32, u32)> {
    let (_, layer) = e.parts.iter().find(|(part, _)| *part == 0)?;
    match layer {
        VisualLayer::Empty => None,
        VisualLayer::Shown(id) => assets.image_size(*id),
        VisualLayer::Transition { from, to, .. } => to
            .and_then(|id| assets.image_size(id))
            .or_else(|| from.and_then(|id| assets.image_size(id))),
    }
}

fn entity_node(
    e: &VisualEntity,
    screen: [f32; 2],
    assets: &AssetStore,
    text: &mut TextSystem,
    system: &SystemState,
) -> Option<Node> {
    if let Some(gate) = e.gate
        && !matches!(system.setting_value(gate), Some(Value::Bool(true)))
    {
        return None;
    }
    if e.opacity <= 0.0 {
        return None;
    }
    let (label_from, label_to) = label_layouts(e, assets, text);
    let label_size = label_to
        .as_ref()
        .or(label_from.as_ref())
        .map(|l| (l.size[0], l.size[1]));
    let (bw, bh) = main_size(e, assets)
        .map(|(w, h)| (w as f32, h as f32))
        .or_else(|| e.fill_size.map(|[w, h]| (w as f32, h as f32)))
        .or(label_size)
        .unwrap_or((0.0, 0.0));
    let flip_x = matches!(e.flip, Flip::Horizontal | Flip::Both);
    let flip_y = matches!(e.flip, Flip::Vertical | Flip::Both);

    let mut local = Vec::new();
    if let Some([w, h]) = e.fill_size
        && e.fill[3] > 0.0
    {
        local.push(Node::Fill {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
            color: e.fill,
        });
    }
    let mut parts: Vec<&(u8, VisualLayer<AssetId>)> = e.parts.iter().collect();
    parts.sort_by_key(|(part, _)| *part);
    for (part, layer) in parts {
        let is_main = *part == 0;
        let rect = |id: AssetId| -> Option<Rect> {
            let (w, h) = assets.image_size(id)?;
            let (w, h) = (w as f32, h as f32);
            let (mut x, mut y) = if is_main {
                (e.anchor[0] * (bw - w), e.anchor[1] * (bh - h))
            } else {
                (0.0, 0.0)
            };
            if flip_x {
                x = bw - x - w;
            }
            if flip_y {
                y = bh - y - h;
            }
            Some(Rect { x, y, w, h })
        };
        if let Some(node) = layer_node(layer, rect, flip_x, flip_y) {
            local.push(node);
        }
    }
    match &e.label {
        VisualLayer::Empty => {}
        VisualLayer::Shown(_) => local.extend(label_to.map(|l| text_node(l).1)),
        VisualLayer::Transition {
            kind,
            progress,
            hold_fraction,
            ..
        } => local.extend(transition_node(
            label_from.map(text_node),
            label_to.map(text_node),
            *kind,
            *progress,
            *hold_fraction,
        )),
    }

    let m = [
        translate(e.position),
        rotate(e.rotation_deg),
        scale(e.scale),
        translate([-e.anchor[0] * bw, -e.anchor[1] * bh]),
    ]
    .into_iter()
    .fold(Affine::IDENTITY, mul);
    let mut children = Vec::new();
    if e.fill_size.is_none() && e.fill[3] > 0.0 {
        children.push(Node::Fill {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: screen[0],
                h: screen[1],
            },
            color: e.fill,
        });
    }
    children.push(Node::Group {
        transform: m,
        opacity: 1.0,
        blend: Blend::Normal,
        clip: None,
        children: local,
    });
    Some(Node::Group {
        transform: Affine::IDENTITY,
        opacity: e.opacity,
        blend: blend(e.blend),
        clip: None,
        children,
    })
}
