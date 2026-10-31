//! A sampled screen as `Frame` nodes: the camera group over the Background, Stage and Front
//! planes (under the screen filter), and the OverWindow plane outside the camera.

use std::rc::Rc;

use kagari_vm::{Text, Value};

use super::sample::{
    VisualCamera, VisualContent, VisualEntity, VisualImageLayer, VisualLayer, VisualScreen,
};
use crate::anim::decimal_f32;
use crate::assets::AssetStore;
use crate::frame::{Affine, Node, Pose, Rect, TransitionDraw};
use crate::system::SystemState;
use crate::ui::text::TextSystem;
use crate::values::{Flip, Plane, Resize};

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

/// Content sized by its layer's `Resize`: the layer's box, and the content's rect in the box
/// (centred; larger than the box only under `Cover`, and drawn only inside the box).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Resized {
    pub size: [f32; 2],
    pub content: Rect,
}

/// Content of natural size `natural` sized by `resize`, on a screen of `screen` px.
pub(crate) fn resized(resize: Resize, natural: (u32, u32), screen: [f32; 2]) -> Resized {
    let (nw, nh) = (natural.0 as f32, natural.1 as f32);
    let ratio = |target: f32, own: f32| if own > 0.0 { target / own } else { 0.0 };
    let exact = |w: f32, h: f32| Resized {
        size: [w, h],
        content: Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
    };
    let fit = |[w, h]: [f32; 2], cover: bool| {
        let (kw, kh) = (ratio(w, nw), ratio(h, nh));
        let k = if cover { kw.max(kh) } else { kw.min(kh) };
        let (cw, ch) = (nw * k, nh * k);
        Resized {
            size: [w, h],
            content: Rect {
                x: (w - cw) / 2.0,
                y: (h - ch) / 2.0,
                w: cw,
                h: ch,
            },
        }
    };
    match resize {
        Resize::Natural => exact(nw, nh),
        Resize::Factor(factor) => {
            let k = decimal_f32(factor).max(0.0);
            exact(nw * k, nh * k)
        }
        Resize::Width(w) => {
            let w = w as f32;
            exact(w, nh * ratio(w, nw))
        }
        Resize::Height(h) => {
            let h = h as f32;
            exact(nw * ratio(h, nh), h)
        }
        Resize::Stretch([w, h]) => exact(w as f32, h as f32),
        Resize::Contain([w, h]) => fit([w as f32, h as f32], false),
        Resize::Cover([w, h]) => fit([w as f32, h as f32], true),
        Resize::CoverScreen => fit(screen, true),
    }
}

/// `content` sized by `resize`; `None` when it is not loaded.
fn resize_content(
    content: &VisualContent,
    resize: Resize,
    screen: [f32; 2],
    assets: &AssetStore,
) -> Option<Resized> {
    let Some(natural) = content_size(content, assets) else {
        if let VisualContent::Image(id) = content {
            log::debug!("image `{}` not loaded; skipped", assets.path(*id));
        }
        return None;
    };
    Some(resized(resize, natural, screen))
}

/// The box of an image layer: its new content's during a swap, else its content's.
pub(crate) fn layer_box(
    image: &VisualImageLayer,
    screen: [f32; 2],
    assets: &AssetStore,
) -> Option<[f32; 2]> {
    let size = |content: &Option<VisualContent>| {
        let content = content.as_ref()?;
        resize_content(content, image.resize, screen, assets).map(|r| r.size)
    };
    match &image.layer {
        VisualLayer::Empty => None,
        VisualLayer::Shown(content) => {
            resize_content(content, image.resize, screen, assets).map(|r| r.size)
        }
        VisualLayer::Transition { from, to, .. } => size(to).or_else(|| size(from)),
    }
}

/// One image layer with its box's top-left corner at `origin`.
pub(crate) fn image_layer_node(
    image: &VisualImageLayer,
    origin: [f32; 2],
    screen: [f32; 2],
    assets: &AssetStore,
) -> Option<Node> {
    layer_node(
        &image.layer,
        |content| {
            let r = resize_content(content, image.resize, screen, assets)?;
            Some(placed(r, origin))
        },
        false,
        false,
    )
}

/// The box of `r` with its top-left corner at `origin`, and its content's rect.
fn placed(r: Resized, origin: [f32; 2]) -> (Rect, Rect) {
    let [w, h] = r.size;
    let clip = Rect {
        x: origin[0],
        y: origin[1],
        w,
        h,
    };
    let content = Rect {
        x: origin[0] + r.content.x,
        y: origin[1] + r.content.y,
        ..r.content
    };
    (clip, content)
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

/// `content` drawn over `rect`, cropped to `clip`: the part of the content inside the clip is
/// drawn over the part of `rect` inside it. `None` when nothing is inside.
fn content_node(
    content: &VisualContent,
    clip: Rect,
    rect: Rect,
    flip_x: bool,
    flip_y: bool,
) -> Option<(Rect, Node)> {
    let shown = intersect(rect, clip)?;
    let mut uv = Rect {
        x: (shown.x - rect.x) / rect.w,
        y: (shown.y - rect.y) / rect.h,
        w: shown.w / rect.w,
        h: shown.h / rect.h,
    };
    // A flipped quad samples its uv range mirrored: mirror the range so that the shown part
    // stays the one inside the clip.
    if flip_x {
        uv.x = 1.0 - uv.x - uv.w;
    }
    if flip_y {
        uv.y = 1.0 - uv.y - uv.h;
    }
    let node = match content {
        VisualContent::Image(id) => Node::Image {
            rect: shown,
            image: *id,
            uv,
            flip_x,
            flip_y,
            tint: [1.0; 4],
        },
        VisualContent::Paint(paint) => Node::Paint {
            rect: shown,
            size: [paint.size[0] as f32, paint.size[1] as f32],
            uv,
            texture: paint.texture.clone(),
            mask: paint.mask.clone(),
            flip_x,
            flip_y,
        },
    };
    Some((shown, node))
}

/// The overlap of `a` and `b`; `None` when it is empty.
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let w = (a.x + a.w).min(b.x + b.w) - x;
    let h = (a.y + a.h).min(b.y + b.h) - y;
    (w > 0.0 && h > 0.0).then_some(Rect { x, y, w, h })
}

/// An image layer whose content is drawn by `place(content)`: `(clip, rect)`, the content
/// over `rect` cropped to `clip`.
fn layer_node(
    layer: &VisualLayer<VisualContent>,
    place: impl Fn(&VisualContent) -> Option<(Rect, Rect)>,
    flip_x: bool,
    flip_y: bool,
) -> Option<Node> {
    let node = |content: &VisualContent| -> Option<(Rect, Node)> {
        let (clip, rect) = place(content)?;
        content_node(content, clip, rect, flip_x, flip_y)
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

/// An entity: `Mix | Group(opacity) > Filter? > [screen fill?, Motion? > Group(M) > Masked? >
/// local]`, where M places the entity's box (`T(pos)·R(rot)·S(scale)·T(−anchor·box)`). The box
/// is the `MAIN` layer's box, else the fill size, else the label's size.
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
    let main = e.parts.iter().find(|(part, _)| *part == 0);
    let (bw, bh) = main
        .and_then(|(_, image)| layer_box(image, screen, assets))
        .map(|[w, h]| (w, h))
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
    let mut parts: Vec<&(u8, VisualImageLayer)> = e.parts.iter().collect();
    parts.sort_by_key(|(part, _)| *part);
    for (part, image) in parts {
        let is_main = *part == 0;
        let place = |content: &VisualContent| -> Option<(Rect, Rect)> {
            let r = resize_content(content, image.resize, screen, assets)?;
            let [w, h] = r.size;
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
            Some(placed(r, [x, y]))
        };
        if let Some(node) = layer_node(&image.layer, place, flip_x, flip_y) {
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
