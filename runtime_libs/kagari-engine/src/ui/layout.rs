//! Layout: measures and arranges every mounted tree in design px
//! (`docs/site/src/content/docs/en/std/ui.md` §Layout).

use super::UiState;
use super::element::{Align, Arena, ElemId, Fit, Kind, Mods};
use super::focus::{self, LookCtx};
use super::style::{ButtonStyleSpec, SliderStyleSpec, choose, look};
use super::text::{TextStyleSpec, TextSystem};
use crate::EngineState;
use crate::assets::AssetStore;
use crate::frame::Rect;
use crate::saves::{THUMB_HEIGHT, THUMB_WIDTH};

/// Lays out every mount when something changed since the last layout.
pub(crate) fn run(state: &mut EngineState) {
    if !state.ui.layout_dirty {
        return;
    }
    // Nothing mounts before `main` returned the resolution.
    let Some(resolution) = state.system.resolution() else {
        return;
    };
    state.ui.layout_dirty = false;
    for index in 0..state.ui.mounts.len() {
        layout_mount(
            &mut state.ui,
            &mut state.text,
            &state.assets,
            index,
            resolution,
        );
    }
}

/// Lays out mount `index` in the design box `resolution`, then updates its focus order, focus
/// and scroll-into-view.
pub(crate) fn layout_mount(
    ui: &mut UiState,
    text: &mut TextSystem,
    assets: &AssetStore,
    index: usize,
    resolution: (u32, u32),
) {
    let Some(mount) = ui.mounts.get(index) else {
        return;
    };
    let root = mount.root;
    let autofocus = mount.autofocus;
    let looks = LookCtx::of(&mount.focus, ui.focus_visible);
    let mut layouter = Layouter {
        arena: &mut ui.arena,
        text,
        assets,
        looks,
    };
    let rect = layouter.root_rect(root, resolution);
    layouter.arrange(root, rect, None);
    let order = focus::order(&ui.arena, root);
    let Some(mount) = ui.mounts.get_mut(index) else {
        return;
    };
    focus::after_layout(&mut mount.focus, order, autofocus);
    let request = mount.focus.scroll_request.take();
    let scrolled = matches!((request, mount.focus.focused), (Some(down), Some(focused))
        if focus::scroll_into_view(&mut ui.arena, focused, down));
    // Focus set or handed on by `after_layout` may select another look, whose text can
    // measure differently: arrange again with the looks drawing will use.
    let settled = LookCtx::of(&mount.focus, ui.focus_visible);
    if scrolled || settled != looks {
        let mut layouter = Layouter {
            arena: &mut ui.arena,
            text,
            assets,
            looks: settled,
        };
        layouter.arrange(root, rect, None);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// What measuring an element needs of its kind.
enum Shape {
    Flex(Axis),
    /// A button or toggle: a centred column of its children in the look's text.
    Control,
    Select,
    Scroll,
    /// Fills the available box.
    Fill,
    /// A label (no own style) or `text!`.
    Text(Option<TextStyleSpec>, Option<kagari_vm::Text>),
    Image(Fit, Option<kagari_vm::AssetId>),
    Fixed([f32; 2]),
}

impl Shape {
    fn of(kind: &Kind) -> Shape {
        match kind {
            Kind::Column | Kind::Each | Kind::Show => Shape::Flex(Axis::Y),
            Kind::Row => Shape::Flex(Axis::X),
            Kind::Button { .. } | Kind::Toggle { .. } => Shape::Control,
            Kind::Select { .. } => Shape::Select,
            Kind::Scroll { .. } => Shape::Scroll,
            Kind::Layer { .. } | Kind::GameScreen | Kind::Message { .. } => Shape::Fill,
            Kind::Label { text } => Shape::Text(None, Some(text.clone())),
            Kind::Text { style, content } => Shape::Text(*style, content.clone()),
            Kind::Image { fit, source } => Shape::Image(*fit, *source),
            Kind::Spacer => Shape::Fixed([0.0; 2]),
            Kind::Thumbnail { .. } => Shape::Fixed([THUMB_WIDTH as f32, THUMB_HEIGHT as f32]),
            Kind::Slider { style, .. } => {
                let thumb = thumb_size(style.as_deref());
                Shape::Fixed([200.0, thumb[1].max(24.0)])
            }
        }
    }
}

struct Layouter<'a> {
    arena: &'a mut Arena,
    text: &'a mut TextSystem,
    assets: &'a AssetStore,
    looks: LookCtx,
}

fn sum_padding(mods: &Mods) -> [f32; 2] {
    let [top, right, bottom, left] = mods.padding;
    [left + right, top + bottom]
}

fn inner_rect(rect: Rect, mods: &Mods) -> Rect {
    let [top, right, bottom, left] = mods.padding;
    Rect {
        x: rect.x + left,
        y: rect.y + top,
        w: (rect.w - left - right).max(0.0),
        h: (rect.h - top - bottom).max(0.0),
    }
}

fn fixed(mods: &Mods) -> [Option<f32>; 2] {
    [mods.width.map(|w| w as f32), mods.height.map(|h| h as f32)]
}

/// The main and cross components of `v` along `axis`.
fn split(axis: Axis, v: [f32; 2]) -> (f32, f32) {
    match axis {
        Axis::X => (v[0], v[1]),
        Axis::Y => (v[1], v[0]),
    }
}

/// The thumb size of a slider style, or the default's.
pub(crate) fn thumb_size(style: Option<&SliderStyleSpec>) -> [f32; 2] {
    style.map_or(SliderStyleSpec::default_style().thumb_size, |s| {
        s.thumb_size
    })
}

impl Layouter<'_> {
    fn root_rect(&self, root: ElemId, resolution: (u32, u32)) -> Rect {
        let mods = self
            .arena
            .get(root)
            .map(|n| n.mods.clone())
            .unwrap_or_default();
        Rect {
            x: 0.0,
            y: 0.0,
            w: mods.width.unwrap_or(resolution.0) as f32,
            h: mods.height.unwrap_or(resolution.1) as f32,
        }
    }

    /// The text style labels inside control `id` use: its current look's.
    fn control_text(&self, id: ElemId) -> Option<TextStyleSpec> {
        let node = self.arena.get(id)?;
        let default = ButtonStyleSpec::default_style();
        let style = match &node.kind {
            Kind::Button { style, .. }
            | Kind::Toggle { style, .. }
            | Kind::Select { style, .. } => style.as_deref().unwrap_or(&default),
            _ => return None,
        };
        let choice = choose(style, self.looks.interaction(id, &node.kind));
        look(style, choice.key)
            .text
            .or_else(|| self.text.system_style().ok().copied())
    }

    /// The size of `text` in `style`, wrapped at the inner box.
    fn text_size(
        &mut self,
        text: &kagari_vm::Text,
        style: Option<TextStyleSpec>,
        inner: [f32; 2],
    ) -> [f32; 2] {
        let Some(style) = style.or_else(|| self.text.system_style().ok().copied()) else {
            return [0.0; 2];
        };
        let limit = if style.vertical { inner[1] } else { inner[0] };
        match self.text.layout(self.assets, text, &style, Some(limit)) {
            Ok(layout) => layout.layout.size,
            Err(trap) => {
                log::warn!("text not laid out: {trap}");
                [0.0; 2]
            }
        }
    }

    /// The flex items of `id`: its children. Each and show children are spliced in (through
    /// nested fragments too) only when `id` is a real row or column; anywhere else, such as a
    /// fragment laid out as a column or a button's content, a fragment child is one item.
    fn flex_items(&self, id: ElemId) -> Vec<ElemId> {
        let splice = self
            .arena
            .get(id)
            .is_some_and(|node| matches!(node.kind, Kind::Column | Kind::Row));
        let mut items = Vec::new();
        self.collect_items(id, splice, &mut items);
        items
    }

    fn collect_items(&self, id: ElemId, splice: bool, items: &mut Vec<ElemId>) {
        for child in self.arena.children(id) {
            match self.arena.get(child) {
                Some(node) if splice && node.kind.is_fragment() => {
                    self.collect_items(child, true, items);
                }
                Some(_) => items.push(child),
                None => {}
            }
        }
    }

    /// The current option label of a select.
    fn select_label(&self, id: ElemId) -> Option<ElemId> {
        let Kind::Select { options, value, .. } = &self.arena.get(id)?.kind else {
            return None;
        };
        focus::current_option(options, value.as_ref())
            .and_then(|i| options.get(i)?.as_ref().map(|o| o.1))
    }

    /// The border box `id` prefers within `avail`.
    fn measure(
        &mut self,
        id: ElemId,
        avail: [f32; 2],
        inherited: Option<TextStyleSpec>,
    ) -> [f32; 2] {
        let Some(node) = self.arena.get(id) else {
            return [0.0; 2];
        };
        let mods = node.mods.clone();
        let shape = Shape::of(&node.kind);
        let pad = sum_padding(&mods);
        let fix = fixed(&mods);
        let inner = [
            (fix[0].unwrap_or(avail[0]) - pad[0]).max(0.0),
            (fix[1].unwrap_or(avail[1]) - pad[1]).max(0.0),
        ];
        let content = match shape {
            Shape::Flex(axis) => self.flex_content(id, axis, inner, &mods, inherited),
            Shape::Control => {
                let text = self.control_text(id);
                self.flex_content(id, Axis::Y, inner, &mods, text)
            }
            Shape::Select => {
                let text = self.control_text(id);
                match self.select_label(id) {
                    Some(label) => self.measure(label, inner, text),
                    None => [0.0; 2],
                }
            }
            Shape::Scroll => match self.arena.children(id).first() {
                Some(child) => self.measure(*child, inner, inherited),
                None => [0.0; 2],
            },
            Shape::Fill => inner,
            Shape::Text(style, Some(text)) => self.text_size(&text, style.or(inherited), inner),
            Shape::Text(_, None) => [0.0; 2],
            Shape::Image(fit, source) => {
                let natural = source
                    .and_then(|s| self.assets.image_size(s))
                    .map_or([0.0; 2], |(w, h)| [w as f32, h as f32]);
                image_content(fit, natural, fix, inner)
            }
            Shape::Fixed(size) => size,
        };
        [
            fix[0].unwrap_or(content[0] + pad[0]),
            fix[1].unwrap_or(content[1] + pad[1]),
        ]
    }

    /// The preferred content of a flex container: main sums, cross maximum.
    fn flex_content(
        &mut self,
        id: ElemId,
        axis: Axis,
        inner: [f32; 2],
        mods: &Mods,
        inherited: Option<TextStyleSpec>,
    ) -> [f32; 2] {
        let items = self.flex_items(id);
        let gaps = mods.gap as f32 * items.len().saturating_sub(1) as f32;
        let mut main = gaps;
        let mut cross = 0.0f32;
        for item in items {
            let (m, c) = split(axis, self.measure(item, inner, inherited));
            main += m;
            cross = cross.max(c);
        }
        match axis {
            Axis::X => [main, cross],
            Axis::Y => [cross, main],
        }
    }

    /// Places `id` at `rect` and arranges its children.
    fn arrange(&mut self, id: ElemId, rect: Rect, inherited: Option<TextStyleSpec>) {
        let Some(node) = self.arena.get_mut(id) else {
            return;
        };
        node.rect = rect;
        let mods = node.mods.clone();
        let kind = node.kind.clone();
        let inner = inner_rect(rect, &mods);
        let start = (Align::Start, Align::Start);
        match kind {
            Kind::Column | Kind::Each | Kind::Show => {
                self.flex_arrange(id, Axis::Y, inner, &mods, start, inherited);
            }
            Kind::Row => self.flex_arrange(id, Axis::X, inner, &mods, start, inherited),
            Kind::Button { .. } | Kind::Toggle { .. } => {
                let text = self.control_text(id);
                let center = (Align::Center, Align::Center);
                self.flex_arrange(id, Axis::Y, inner, &mods, center, text);
            }
            Kind::Select { .. } => {
                // Its current label lays out as a button's content, honouring `align`.
                let text = self.control_text(id);
                if let Some(label) = self.select_label(id) {
                    let center = (Align::Center, Align::Center);
                    self.arrange_items(&[label], Axis::Y, inner, &mods, center, text);
                }
            }
            Kind::Layer { places } => {
                let children = self
                    .arena
                    .get(id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for (child, place) in children.iter().zip(&places) {
                    let (Some(child), Some(place)) = (child, place) else {
                        continue;
                    };
                    let r = match *place {
                        super::element::Place::Cover => inner,
                        super::element::Place::At { x, y, ax, ay } => {
                            let [w, h] = self.measure(*child, [inner.w, inner.h], inherited);
                            Rect {
                                x: inner.x + x - ax * w,
                                y: inner.y + y - ay * h,
                                w,
                                h,
                            }
                        }
                    };
                    self.arrange(*child, r, inherited);
                }
            }
            Kind::Scroll { offset, .. } => {
                let Some(child) = self.arena.children(id).first().copied() else {
                    return;
                };
                let [w, h] = self.measure(child, [inner.w, inner.h], inherited);
                let fixed_w = self
                    .arena
                    .get(child)
                    .is_some_and(|n| n.mods.width.is_some());
                let offset = offset.clamp(0.0, (h - inner.h).max(0.0));
                if let Some(Kind::Scroll { offset: o, content }) =
                    self.arena.get_mut(id).map(|n| &mut n.kind)
                {
                    *o = offset;
                    *content = h;
                }
                let r = Rect {
                    x: inner.x,
                    y: inner.y - offset,
                    w: if fixed_w { w } else { inner.w },
                    h,
                };
                self.arrange(child, r, inherited);
            }
            _ => {}
        }
    }

    fn flex_arrange(
        &mut self,
        id: ElemId,
        axis: Axis,
        inner: Rect,
        mods: &Mods,
        default_align: (Align, Align),
        inherited: Option<TextStyleSpec>,
    ) {
        let items = self.flex_items(id);
        self.arrange_items(&items, axis, inner, mods, default_align, inherited);
    }

    /// Places `items` as the flex items of a box with `mods` (std `ui.md#layout`).
    fn arrange_items(
        &mut self,
        items: &[ElemId],
        axis: Axis,
        inner: Rect,
        mods: &Mods,
        default_align: (Align, Align),
        inherited: Option<TextStyleSpec>,
    ) {
        if items.is_empty() {
            return;
        }
        let count = items.len() as f32;
        let size = [inner.w, inner.h];
        let (inner_main, cross_size) = split(axis, size);
        let gap = mods.gap as f32;
        let (main_align, cross_align) = mods.align.unwrap_or(default_align);
        let mut mains = Vec::with_capacity(items.len());
        let mut crosses = Vec::with_capacity(items.len());
        let mut grows = Vec::with_capacity(items.len());
        let mut scrolls = Vec::with_capacity(items.len());
        let mut fixed_cross = Vec::with_capacity(items.len());
        for item in items {
            let (m, c) = split(axis, self.measure(*item, size, inherited));
            mains.push(m);
            crosses.push(c);
            let node = self.arena.get(*item);
            let spacer = node.is_some_and(|n| matches!(n.kind, Kind::Spacer));
            let grow = node.and_then(|n| n.mods.grow).unwrap_or(u32::from(spacer));
            grows.push(grow as f32);
            scrolls.push(node.is_some_and(|n| matches!(n.kind, Kind::Scroll { .. })));
            fixed_cross.push(node.is_some_and(|n| {
                let [w, h] = fixed(&n.mods);
                match axis {
                    Axis::X => h.is_some(),
                    Axis::Y => w.is_some(),
                }
            }));
        }
        let total: f32 = mains.iter().sum::<f32>() + gap * (count - 1.0);
        let mut free = inner_main - total;
        let grow_sum: f32 = grows.iter().sum();
        if free > 0.0 && grow_sum > 0.0 {
            for (main, grow) in mains.iter_mut().zip(&grows) {
                *main += free * grow / grow_sum;
            }
            free = 0.0;
        } else if free < 0.0 {
            let scroll_sum: f32 = mains
                .iter()
                .zip(&scrolls)
                .filter(|(_, s)| **s)
                .map(|(m, _)| *m)
                .sum();
            if scroll_sum > 0.0 {
                for (main, _) in mains.iter_mut().zip(&scrolls).filter(|(_, s)| **s) {
                    *main -= main.min(-free * *main / scroll_sum);
                }
            }
        }
        let (mut pos, extra) = if free > 0.0 {
            match main_align {
                Align::Start | Align::Stretch => (0.0, 0.0),
                Align::Center => (free / 2.0, 0.0),
                Align::End => (free, 0.0),
                Align::SpaceBetween if items.len() > 1 => (0.0, free / (count - 1.0)),
                Align::SpaceBetween => (0.0, 0.0),
            }
        } else {
            (0.0, 0.0)
        };
        let (main_start, cross_start) = match axis {
            Axis::X => (inner.x, inner.y),
            Axis::Y => (inner.y, inner.x),
        };
        for (k, item) in items.iter().enumerate() {
            let (Some(main), Some(pref_cross), Some(fixed)) =
                (mains.get(k), crosses.get(k), fixed_cross.get(k))
            else {
                continue;
            };
            let cross = if cross_align == Align::Stretch && !fixed {
                cross_size
            } else {
                *pref_cross
            };
            let cross_offset = match cross_align {
                Align::Center => (cross_size - cross) / 2.0,
                Align::End => cross_size - cross,
                Align::Start | Align::Stretch | Align::SpaceBetween => 0.0,
            };
            let m = main_start + pos;
            let c = cross_start + cross_offset;
            let rect = match axis {
                Axis::X => Rect {
                    x: m,
                    y: c,
                    w: *main,
                    h: cross,
                },
                Axis::Y => Rect {
                    x: c,
                    y: m,
                    w: cross,
                    h: *main,
                },
            };
            self.arrange(*item, rect, inherited);
            pos += main + gap + extra;
        }
    }
}

/// The preferred content of an image: its natural size for `Natural`; else the fixed sides,
/// the free one following the natural aspect, or the inner box.
fn image_content(
    fit: Fit,
    natural: [f32; 2],
    fixed: [Option<f32>; 2],
    inner: [f32; 2],
) -> [f32; 2] {
    if fit == Fit::Natural {
        return natural;
    }
    let [nw, nh] = natural;
    match fixed {
        [Some(_), None] if nw > 0.0 => [inner[0], inner[0] * nh / nw],
        [None, Some(_)] if nh > 0.0 => [inner[1] * nw / nh, inner[1]],
        _ => inner,
    }
}
