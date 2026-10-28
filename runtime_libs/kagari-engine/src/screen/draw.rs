//! A sampled screen as `Frame` nodes: the camera group over the Background, Stage and Front
//! planes (under the screen filter), and the OverWindow plane outside the camera.

use std::rc::Rc;

use kagari_vm::{Text, Value};

use super::sample::{VisualCamera, VisualContent, VisualEntity, VisualLayer, VisualScreen};
use crate::assets::AssetStore;
use crate::frame::{Affine, Node, Pose, Rect, TransitionDraw};
use crate::system::SystemState;
use crate::ui::text::TextSystem;
use crate::values::{Flip, Plane};

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;

/// The screen's nodes, split around the text window.
pub(crate) struct ScreenPlanes {
    /// The camera group over Background, Stage and Front, inside the screen filter.
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
    let mut world = Node::Group {
        transform: camera_transform(&v.camera, size),
        opacity: 1.0,
        clip: None,
        children: world,
    };
    if let Some(effect) = &v.filter {
        world = Node::Filter {
            effect: effect.clone(),
            children: vec![world],
        };
    }
    ScreenPlanes { world, over_window }
}

/// The natural size of an image layer's content: the image's, or the paint's.
pub(crate) fn content_size(content: &VisualContent, assets: &AssetStore) -> Option<(u32, u32)> {
    match content {
        VisualContent::Image(id) => assets.image_size(*id),
        VisualContent::Paint(paint) => Some((paint.size[0], paint.size[1])),
    }
}

/// One image layer at its natural size with its top-left corner at `origin`.
pub(crate) fn image_layer_node(
    layer: &VisualLayer<VisualContent>,
    origin: [f32; 2],
    assets: &AssetStore,
) -> Option<Node> {
    layer_node(
        layer,
        |content| natural_rect(assets, content, origin),
        false,
        false,
    )
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

fn natural_rect(assets: &AssetStore, content: &VisualContent, origin: [f32; 2]) -> Option<Rect> {
    let Some((w, h)) = content_size(content, assets) else {
        if let VisualContent::Image(id) = content {
            log::debug!("image `{}` not loaded; skipped", assets.path(*id));
        }
        return None;
    };
    Some(Rect {
        x: origin[0],
        y: origin[1],
        w: w as f32,
        h: h as f32,
    })
}

fn content_node(content: &VisualContent, rect: Rect, flip_x: bool, flip_y: bool) -> Node {
    match content {
        VisualContent::Image(id) => Node::Image {
            rect,
            image: *id,
            uv: Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            flip_x,
            flip_y,
            tint: [1.0; 4],
        },
        VisualContent::Paint(paint) => Node::Paint {
            rect,
            texture: paint.texture.clone(),
            mask: paint.mask.clone(),
            flip_x,
            flip_y,
        },
    }
}

/// An image layer whose content is drawn at `rect(content)`.
fn layer_node(
    layer: &VisualLayer<VisualContent>,
    rect: impl Fn(&VisualContent) -> Option<Rect>,
    flip_x: bool,
    flip_y: bool,
) -> Option<Node> {
    let node = |content: &VisualContent| -> Option<(Rect, Node)> {
        let r = rect(content)?;
        Some((r, content_node(content, r, flip_x, flip_y)))
    };
    match layer {
        VisualLayer::Empty => None,
        VisualLayer::Shown(content) => node(content).map(|(_, node)| node),
        VisualLayer::Transition {
            from,
            to,
            kind,
            progress,
        } => transition_node(
            from.as_ref().and_then(node),
            to.as_ref().and_then(node),
            kind,
            *progress,
        ),
    }
}

/// A `Node::Transition` over the union of both sides' rects.
fn transition_node(
    from: Option<(Rect, Node)>,
    to: Option<(Rect, Node)>,
    kind: &TransitionDraw,
    progress: f32,
) -> Option<Node> {
    let bounds = match (&from, &to) {
        (Some((a, _)), Some((b, _))) => union(*a, *b),
        (Some((r, _)), None) | (None, Some((r, _))) => *r,
        (None, None) => return None,
    };
    Some(Node::Transition {
        bounds,
        kind: kind.clone(),
        progress,
        from: from.map(|(_, node)| node).into_iter().collect(),
        to: to.map(|(_, node)| node).into_iter().collect(),
    })
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

/// The size of the MAIN layer's content: `to` during a transition, else `from`.
fn main_size(e: &VisualEntity, assets: &AssetStore) -> Option<(u32, u32)> {
    let (_, layer) = e.parts.iter().find(|(part, _)| *part == 0)?;
    let size = |content: &Option<VisualContent>| {
        content
            .as_ref()
            .and_then(|content| content_size(content, assets))
    };
    match layer {
        VisualLayer::Empty => None,
        VisualLayer::Shown(content) => content_size(content, assets),
        VisualLayer::Transition { from, to, .. } => size(to).or_else(|| size(from)),
    }
}

/// An entity: `Mix | Group(opacity) > Filter? > [screen fill?, Motion? > Group(M) > Masked? >
/// local]`, where M places the entity's box (`T(pos)·R(rot)·S(scale)·T(−anchor·box)`).
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
    let mut parts: Vec<&(u8, VisualLayer<VisualContent>)> = e.parts.iter().collect();
    parts.sort_by_key(|(part, _)| *part);
    for (part, layer) in parts {
        let is_main = *part == 0;
        let rect = |content: &VisualContent| -> Option<Rect> {
            let (w, h) = content_size(content, assets)?;
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
        VisualLayer::Transition { kind, progress, .. } => local.extend(transition_node(
            label_from.map(text_node),
            label_to.map(text_node),
            kind,
            *progress,
        )),
    }
    if let Some(effect) = &e.mask {
        local = vec![Node::Masked {
            effect: effect.clone(),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: bw,
                h: bh,
            },
            children: local,
        }];
    }

    // GPU motion needs an invertible pose: a zero scale is drawn at 1 and left to the stages.
    let entity_scale = if !e.stages.is_empty() && e.scale == 0.0 {
        1.0
    } else {
        e.scale
    };
    let m = [
        translate(e.position),
        rotate(e.rotation_deg),
        scale(entity_scale),
        translate([-e.anchor[0] * bw, -e.anchor[1] * bh]),
    ]
    .into_iter()
    .fold(Affine::IDENTITY, mul);
    let mut placed = Node::Group {
        transform: m,
        opacity: 1.0,
        clip: None,
        children: local,
    };
    if !e.stages.is_empty() {
        placed = Node::Motion {
            stages: e.stages.clone(),
            pose: Pose {
                position: e.position,
                scale: entity_scale,
                rotation: e.rotation_deg.to_radians(),
            },
            children: vec![placed],
        };
    }
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
    children.push(placed);
    if let Some(effect) = &e.filter {
        children = vec![Node::Filter {
            effect: effect.clone(),
            children,
        }];
    }
    Some(match &e.mix {
        Some(effect) => Node::Mix {
            effect: effect.clone(),
            opacity: e.opacity,
            children,
        },
        None => Node::Group {
            transform: Affine::IDENTITY,
            opacity: e.opacity,
            clip: None,
            children,
        },
    })
}
