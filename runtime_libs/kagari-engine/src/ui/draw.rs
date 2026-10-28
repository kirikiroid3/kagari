//! Drawing the mounted trees into `Frame` nodes, with the dialog still rule.

use std::collections::HashMap;
use std::rc::Rc;

use kagari_vm::Text;

use super::ModeId;
use super::element::{Arena, ElemId, Fit, Kind};
use super::focus::{LookCtx, button_style, current_option, draw_children};
use super::layout::{self, thumb_size};
use super::style::{ButtonStyleSpec, LookKey, Paint, SliderStyleSpec, choose, look};
use super::text::{Px, TextAlign, TextStyleSpec, TextSystem};
use crate::EngineState;
use crate::assets::AssetStore;
use crate::frame::{Affine, Blend, Node, Rect, Reveal};
use crate::screen::draw::{draw_screen, image_layer_node};
use crate::screen::sample::{VisualLayer, VisualScreen};
use crate::story::reveal::{ClickWait, MessageView};
use crate::system::SystemState;
use crate::system::modes::ModeKind;

/// The nodes of every mount, bottom to top, and whether the renderer must capture them as the
/// still. Under the topmost dialog the modes below are drawn live once, as the frame that
/// captures the still; later frames draw that still, then the dialog and what is above it.
pub(crate) fn frame_nodes(state: &mut EngineState, now_ms: u64) -> (Vec<Node>, bool) {
    layout::run(state);
    let (w, h) = state.system.resolution().unwrap_or((0, 0));
    let dialog = state
        .ui
        .mounts
        .iter()
        .rposition(|m| m.kind == ModeKind::Dialog);
    let count = state.ui.mounts.len();
    let mut nodes = Vec::new();
    let (range, capture) = match dialog {
        None => {
            state.ui.still_for = None;
            (0..count, false)
        }
        Some(d) => {
            let id = state.ui.mounts.get(d).map(|m| m.id);
            if state.ui.still_for == id {
                nodes.push(Node::Still {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: w as f32,
                        h: h as f32,
                    },
                });
                (d..count, false)
            } else {
                state.ui.still_for = id;
                (0..d, true)
            }
        }
    };
    for index in range {
        nodes.extend(draw_mount(state, index, now_ms));
    }
    (nodes, capture)
}

/// The nodes of one mount (a save thumbnail draws the game mount alone).
pub(crate) fn mount_nodes(state: &mut EngineState, mode: ModeId, now_ms: u64) -> Vec<Node> {
    layout::run(state);
    match state.ui.mounts.iter().position(|m| m.id == mode) {
        Some(index) => draw_mount(state, index, now_ms),
        None => Vec::new(),
    }
}

fn draw_mount(state: &mut EngineState, index: usize, now_ms: u64) -> Vec<Node> {
    let ui = &mut state.ui;
    let Some(mount) = ui.mounts.get(index) else {
        return Vec::new();
    };
    let root = mount.root;
    let looks = LookCtx::of(&mount.focus, ui.focus_visible);
    let system_style = state.text.system_style().ok().copied();
    let game = state.game.as_ref();
    let visual = game.map(|g| g.screen.sample(g.clock.story_ms(now_ms)));
    let mut classes = HashMap::new();
    classify(&ui.arena, root, &mut classes);
    let mut drawer = Drawer {
        arena: &mut ui.arena,
        text: &mut state.text,
        assets: &state.assets,
        looks,
        now_ms,
        animations: state.system.animations(),
        system: system_style,
        game: GameDraw {
            window_fade: visual
                .as_ref()
                .map_or(1.0, |v| v.hud.text_window.clamp(0.0, 1.0)),
            visual,
            message: game.and_then(|g| g.message(now_ms)),
            hide_window: game.is_some_and(|g| g.hide_window),
            resolution: state.system.resolution().unwrap_or((1, 1)),
            state: &state.system,
        },
        classes,
        in_window: false,
        deferred: Vec::new(),
    };
    let mut out = Vec::new();
    drawer.draw(root, &mut out, None);
    out.extend(drawer.deferred);
    out
}

/// What the game view's elements read of the running game.
struct GameDraw<'a> {
    /// The screen sampled at the frame's story time; `None` without a game.
    visual: Option<VisualScreen>,
    message: Option<MessageView>,
    hide_window: bool,
    /// The text-window fade alpha (1 without a game).
    window_fade: f32,
    resolution: (u32, u32),
    state: &'a SystemState,
}

/// Whether each element's subtree holds a `message` and a game screen.
fn classify(arena: &Arena, id: ElemId, out: &mut HashMap<ElemId, (bool, bool)>) -> (bool, bool) {
    let own = match arena.get(id).map(|n| &n.kind) {
        Some(Kind::Message { .. }) => (true, false),
        Some(Kind::GameScreen) => (false, true),
        _ => (false, false),
    };
    let found = draw_children(arena, id)
        .into_iter()
        .fold(own, |(m, g), child| {
            let (cm, cg) = classify(arena, child, out);
            (m || cm, g || cg)
        });
    out.insert(id, found);
    found
}

/// Multiplies the fills and image tints of `nodes` by `k` (the window opacity); text is kept.
fn fade_window(nodes: &mut [Node], k: f32) {
    for node in nodes {
        match node {
            Node::Fill { color, .. } => *color = color.map(|c| c * k),
            Node::Image { tint, .. } | Node::NineSlice { tint, .. } => *tint = tint.map(|c| c * k),
            Node::Group { children, .. } => fade_window(children, k),
            Node::Transition { from, to, .. } => {
                fade_window(from, k);
                fade_window(to, k);
            }
            Node::Text { .. } | Node::Still { .. } | Node::Thumbnail { .. } => {}
        }
    }
}

struct Drawer<'a> {
    arena: &'a mut Arena,
    text: &'a mut TextSystem,
    assets: &'a AssetStore,
    looks: LookCtx,
    now_ms: u64,
    animations: bool,
    system: Option<TextStyleSpec>,
    game: GameDraw<'a>,
    classes: HashMap<ElemId, (bool, bool)>,
    /// Inside the window subtree.
    in_window: bool,
    /// Drawn after the mount's whole tree: the side image and the OverWindow plane.
    deferred: Vec<Node>,
}

/// `rect` without its padding (`[top, right, bottom, left]`).
pub(super) fn inner_rect(rect: Rect, padding: [f32; 4]) -> Rect {
    let [top, right, bottom, left] = padding;
    Rect {
        x: rect.x + left,
        y: rect.y + top,
        w: (rect.w - left - right).max(0.0),
        h: (rect.h - top - bottom).max(0.0),
    }
}

fn group(opacity: f32, clip: Option<Rect>, children: Vec<Node>) -> Node {
    Node::Group {
        transform: Affine::IDENTITY,
        opacity,
        blend: Blend::Normal,
        clip,
        children,
    }
}

const FULL_UV: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};

/// The nodes of `paint` over `rect` at `alpha`; an image not loaded draws nothing.
fn paint_nodes(paint: Paint, rect: Rect, alpha: f32, assets: &AssetStore, out: &mut Vec<Node>) {
    if alpha <= 0.0 {
        return;
    }
    let image_loaded = |image| {
        let loaded = assets.image_size(image).is_some();
        if !loaded {
            log::debug!("image `{}` not loaded; skipped", assets.path(image));
        }
        loaded
    };
    match paint {
        Paint::None => {}
        Paint::Color(c) => out.push(Node::Fill {
            rect,
            color: c.map(|v| v * alpha),
        }),
        Paint::Image(image) if image_loaded(image) => out.push(Node::Image {
            rect,
            image,
            uv: FULL_UV,
            flip_x: false,
            flip_y: false,
            tint: [alpha; 4],
        }),
        Paint::NineSlice { image, insets } if image_loaded(image) => out.push(Node::NineSlice {
            rect,
            image,
            insets,
            tint: [alpha; 4],
        }),
        Paint::Image(_) | Paint::NineSlice { .. } => {}
    }
}

impl Drawer<'_> {
    fn draw(&mut self, id: ElemId, out: &mut Vec<Node>, inherited: Option<TextStyleSpec>) {
        let opens_window = !self.in_window && self.classes.get(&id) == Some(&(true, false));
        if !opens_window {
            self.draw_element(id, out, inherited);
            return;
        }
        // The window subtree: fills and images take the window opacity, and the whole fades
        // with the text window.
        self.in_window = true;
        let mut window = Vec::new();
        self.draw_element(id, &mut window, inherited);
        self.in_window = false;
        fade_window(&mut window, self.game.state.window_opacity());
        out.push(group(self.game.window_fade, None, window));
    }

    fn draw_element(&mut self, id: ElemId, out: &mut Vec<Node>, inherited: Option<TextStyleSpec>) {
        let Some(node) = self.arena.get(id) else {
            return;
        };
        let rect = node.rect;
        let mods = node.mods.clone();
        let kind = node.kind.clone();
        let inner = inner_rect(rect, mods.padding);
        let mut local = Vec::new();
        // A spliced fragment is no box of its own (§4.5): its modifiers are ignored.
        let spliced = self.arena.is_spliced(id);
        if !spliced {
            paint_nodes(mods.background, rect, 1.0, self.assets, &mut local);
        }
        let mut alpha = if spliced { 1.0 } else { mods.opacity };
        let mut child_text = inherited;
        if let Some(style) = button_style(&kind) {
            let choice = choose(&style, self.looks.interaction(id, &kind));
            if choice.dim {
                alpha *= 0.5;
            }
            let drawn = match self.arena.get_mut(id) {
                Some(node) => node.look.draw(
                    &style,
                    choice,
                    self.now_ms,
                    self.animations,
                    self.system.as_ref(),
                ),
                None => return,
            };
            for (paint, a) in &drawn.backgrounds {
                paint_nodes(*paint, rect, *a, self.assets, &mut local);
            }
            if drawn.choice.outline {
                self.outline(&style, rect, &mut local);
            }
            child_text = drawn.text;
        }
        match &kind {
            Kind::Label { text } => self.text(text, inherited, inner, &mut local),
            Kind::Text {
                style,
                content: Some(text),
            } => self.text(text, style.or(inherited), inner, &mut local),
            Kind::Image {
                fit,
                source: Some(image),
            } => self.image(*fit, *image, inner, &mut local),
            Kind::Thumbnail { slot } => {
                if let Some(t) = self.assets.thumbnail(*slot) {
                    local.push(Node::Thumbnail {
                        rect: inner,
                        slot: *slot,
                        generation: t.generation,
                    });
                }
            }
            Kind::Slider {
                value,
                lo,
                hi,
                disabled,
                style,
                ..
            } => {
                if *disabled {
                    alpha *= 0.5;
                }
                self.slider(*value, *lo, *hi, style.as_deref(), inner, &mut local);
            }
            Kind::Select { options, value, .. } => {
                let label = current_option(options, value.as_ref())
                    .and_then(|i| options.get(i)?.as_ref().map(|o| o.1));
                if let Some(label) = label {
                    self.draw(label, &mut local, child_text);
                }
            }
            Kind::GameScreen => self.game_screen(rect, &mut local),
            Kind::Message { style } => self.message(style, inner, &mut local),
            _ => {}
        }
        let children = draw_children(self.arena, id);
        if matches!(kind, Kind::Scroll { .. }) {
            let mut inside = Vec::new();
            for child in children {
                self.draw(child, &mut inside, child_text);
            }
            local.push(group(1.0, Some(rect), inside));
        } else {
            for child in children {
                self.draw(child, &mut local, child_text);
            }
        }
        if alpha < 1.0 {
            out.push(group(alpha.max(0.0), None, local));
        } else {
            out.extend(local);
        }
    }

    /// `text` in `style` (else `System::text`) at the start of the inner box, offset by the
    /// style's alignment along the line.
    fn text(
        &mut self,
        text: &Text,
        style: Option<TextStyleSpec>,
        inner: Rect,
        out: &mut Vec<Node>,
    ) {
        let Some(style) = style.or(self.system) else {
            return;
        };
        let limit = if style.vertical { inner.h } else { inner.w };
        let layout = match self.text.layout(self.assets, text, &style, Some(limit)) {
            Ok(layout) => layout.layout,
            Err(trap) => {
                log::warn!("text not drawn: {trap}");
                return;
            }
        };
        out.push(Node::Text {
            origin: aligned_origin(&style, layout.size, inner),
            layout,
            reveal: Reveal::All,
            opacity: 1.0,
        });
    }

    fn image(&self, fit: Fit, image: kagari_vm::AssetId, inner: Rect, out: &mut Vec<Node>) {
        let Some((iw, ih)) = self.assets.image_size(image) else {
            log::debug!("image `{}` not loaded; skipped", self.assets.path(image));
            return;
        };
        let (iw, ih) = (iw as f32, ih as f32);
        if iw <= 0.0 || ih <= 0.0 {
            return;
        }
        let (rect, uv) = match fit {
            Fit::Stretch => (inner, FULL_UV),
            Fit::Natural => (
                Rect {
                    x: inner.x,
                    y: inner.y,
                    w: iw,
                    h: ih,
                },
                FULL_UV,
            ),
            Fit::Contain => {
                let s = (inner.w / iw).min(inner.h / ih);
                let (w, h) = (iw * s, ih * s);
                (
                    Rect {
                        x: inner.x + (inner.w - w) / 2.0,
                        y: inner.y + (inner.h - h) / 2.0,
                        w,
                        h,
                    },
                    FULL_UV,
                )
            }
            Fit::Cover => {
                let s = (inner.w / iw).max(inner.h / ih);
                let (uw, uh) = ((inner.w / (iw * s)).min(1.0), (inner.h / (ih * s)).min(1.0));
                (
                    inner,
                    Rect {
                        x: (1.0 - uw) / 2.0,
                        y: (1.0 - uh) / 2.0,
                        w: uw,
                        h: uh,
                    },
                )
            }
        };
        out.push(Node::Image {
            rect,
            image,
            uv,
            flip_x: false,
            flip_y: false,
            tint: [1.0; 4],
        });
    }

    /// A toggle that is on without a pressed or hover look: four 2 px strips inside its box in
    /// the idle text colour.
    fn outline(&self, style: &ButtonStyleSpec, r: Rect, out: &mut Vec<Node>) {
        let Some(text) = look(style, LookKey::Idle).text.or(self.system) else {
            return;
        };
        let color = text.color();
        let t = 2.0f32.min(r.w / 2.0).min(r.h / 2.0);
        for rect in [
            Rect {
                x: r.x,
                y: r.y,
                w: r.w,
                h: t,
            },
            Rect {
                x: r.x,
                y: r.y + r.h - t,
                w: r.w,
                h: t,
            },
            Rect {
                x: r.x,
                y: r.y + t,
                w: t,
                h: r.h - 2.0 * t,
            },
            Rect {
                x: r.x + r.w - t,
                y: r.y + t,
                w: t,
                h: r.h - 2.0 * t,
            },
        ] {
            out.push(Node::Fill { rect, color });
        }
    }

    /// Track, fill up to the thumb centre, and thumb.
    fn slider(
        &self,
        value: i128,
        lo: i128,
        hi: i128,
        style: Option<&SliderStyleSpec>,
        r: Rect,
        out: &mut Vec<Node>,
    ) {
        let style = style
            .copied()
            .unwrap_or_else(SliderStyleSpec::default_style);
        let [tw, th] = thumb_size(Some(&style));
        let f = match (hi.checked_sub(lo), value.checked_sub(lo)) {
            (Some(span), Some(at)) if span > 0 => (at as f64 / span as f64).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        let cx = r.x + tw / 2.0 + f * (r.w - tw).max(0.0);
        let mut track = Rect {
            x: r.x + tw / 2.0,
            y: r.y,
            w: (r.w - tw).max(0.0),
            h: r.h,
        };
        if style.bars {
            track.y = r.y + (r.h - 4.0) / 2.0;
            track.h = 4.0f32.min(r.h);
        }
        let fill = Rect {
            w: (cx - track.x).max(0.0),
            ..track
        };
        let thumb = Rect {
            x: cx - tw / 2.0,
            y: r.y + (r.h - th) / 2.0,
            w: tw,
            h: th,
        };
        paint_nodes(style.track, track, 1.0, self.assets, out);
        paint_nodes(style.fill, fill, 1.0, self.assets, out);
        paint_nodes(style.thumb, thumb, 1.0, self.assets, out);
    }

    /// The game screen scaled into `rect`; its OverWindow plane and the side image go to the
    /// deferred list, above the mount's whole tree.
    fn game_screen(&mut self, rect: Rect, out: &mut Vec<Node>) {
        let Some(v) = &self.game.visual else {
            return;
        };
        let (w, h) = self.game.resolution;
        let (fw, fh) = (w as f32, h as f32);
        let transform = Affine([rect.w / fw, 0.0, 0.0, rect.h / fh, rect.x, rect.y]);
        let planes = draw_screen(v, (w, h), self.assets, self.text, self.game.state);
        out.push(Node::Group {
            transform,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: Some(rect),
            children: vec![planes.world],
        });
        let mut above = Vec::new();
        let side = &v.hud.side_image;
        let side_asset = match side {
            VisualLayer::Empty => None,
            VisualLayer::Shown(a) => Some(*a),
            VisualLayer::Transition { from, to, .. } => to.or(*from),
        };
        if !self.game.hide_window
            && let Some(asset) = side_asset
        {
            let ih = self
                .assets
                .image_size(asset)
                .map_or(0.0, |(_, ih)| ih as f32);
            if let Some(node) = image_layer_node(side, [0.0, fh - ih], self.assets) {
                above.push(group(v.hud.text_window.clamp(0.0, 1.0), None, vec![node]));
            }
        }
        above.extend(planes.over_window);
        if !above.is_empty() {
            self.deferred.push(Node::Group {
                transform,
                opacity: 1.0,
                blend: Blend::Normal,
                clip: Some(rect),
                children: above,
            });
        }
    }

    /// The current line with its reveal, and the blinking click-wait glyph.
    fn message(&mut self, style: &kagari_vm::Value, inner: Rect, out: &mut Vec<Node>) {
        let Some(view) = self.game.message.clone() else {
            return;
        };
        let style = match self.text.decode_style(self.assets, style) {
            Ok(style) => style,
            Err(trap) => {
                log::warn!("message not drawn: {trap}");
                return;
            }
        };
        let limit = if style.vertical { inner.h } else { inner.w };
        let laid = match self
            .text
            .layout(self.assets, &view.text, &style, Some(limit))
        {
            Ok(laid) => laid,
            Err(trap) => {
                log::warn!("message not drawn: {trap}");
                return;
            }
        };
        let origin = aligned_origin(&style, laid.layout.size, inner);
        out.push(Node::Text {
            origin,
            layout: laid.layout,
            reveal: view.reveal,
            opacity: 1.0,
        });
        let blink_on = self.now_ms.checked_div(500).and_then(|n| n.checked_rem(2)) == Some(0);
        if view.click_wait == ClickWait::None || !blink_on {
            return;
        }
        let tri_style = TextStyleSpec {
            size: Px::new(style.size.get() * 0.6),
            ..style
        };
        let tri_text = Text::Plain(Rc::from("\u{25BC}"));
        let Ok(tri) = self.text.layout(self.assets, &tri_text, &tri_style, None) else {
            return;
        };
        let Some(first) = tri.layout.glyphs.first() else {
            return;
        };
        let end = laid.metrics.end;
        let at = if style.vertical {
            [
                origin[0] + end[0] - tri.layout.size[0] / 2.0,
                origin[1] + end[1],
            ]
        } else {
            [
                origin[0] + end[0] - first.pos[0],
                origin[1] + end[1] - first.pos[1],
            ]
        };
        out.push(Node::Text {
            origin: at.map(f32::round),
            layout: tri.layout.clone(),
            reveal: Reveal::All,
            opacity: 1.0,
        });
    }
}

/// The rounded origin of a text box of `size` in `inner`: at the start, offset by the style's
/// alignment along the line (down the column for vertical text).
fn aligned_origin(style: &TextStyleSpec, size: [f32; 2], inner: Rect) -> [f32; 2] {
    let free = if style.vertical {
        inner.h - size[1]
    } else {
        inner.w - size[0]
    };
    let shift = match style.align {
        TextAlign::Start => 0.0,
        TextAlign::Center => free / 2.0,
        TextAlign::End => free,
    };
    let origin = if style.vertical {
        [inner.x, inner.y + shift]
    } else {
        [inner.x + shift, inner.y]
    };
    origin.map(f32::round)
}
