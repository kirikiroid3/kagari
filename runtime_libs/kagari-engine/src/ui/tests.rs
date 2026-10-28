#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::element::{Align, ElemId, Kind, Mods, Place};
use super::focus::{UiInput, wheel};
use super::layout::layout_mount;
use super::reactive::CompKind;
use super::text::TextSystem;
use super::{ModeId, UiState};
use crate::assets::AssetStore;
use crate::frame::Rect;
use crate::system::modes::ModeKind;
use crate::test_support::{STD_TYPES, engine};

/// An element arena built directly, laid out in a 1280×720 design box.
struct Tree {
    ui: UiState,
    text: TextSystem,
    assets: AssetStore,
}

impl Tree {
    fn new() -> Tree {
        let (_vm, state) = engine(STD_TYPES);
        Tree {
            ui: UiState::new(),
            text: TextSystem::new(),
            assets: state.assets,
        }
    }

    fn node(&mut self, kind: Kind, mods: Mods) -> ElemId {
        let id = self.ui.arena.alloc(kind, 0);
        self.ui.arena.get_mut(id).unwrap().mods = mods;
        id
    }

    /// A leaf of a fixed size.
    fn leaf(&mut self, w: Option<u32>, h: Option<u32>) -> ElemId {
        self.node(
            Kind::Column,
            Mods {
                width: w,
                height: h,
                ..Mods::default()
            },
        )
    }

    fn children(&mut self, parent: ElemId, kids: &[ElemId]) {
        for (index, kid) in kids.iter().enumerate() {
            let index = u32::try_from(index).unwrap();
            self.ui.arena.get_mut(*kid).unwrap().parent = Some((parent, index));
        }
        self.ui.arena.get_mut(parent).unwrap().children = kids.iter().map(|k| Some(*k)).collect();
    }

    fn mount(&mut self, root: ElemId) {
        let scope = self.ui.graph.create(CompKind::Root, None, false);
        self.ui
            .push_mount(ModeId(1), ModeKind::Root, true, scope, root);
    }

    fn layout(&mut self) {
        layout_mount(&mut self.ui, &mut self.text, &self.assets, 0, (1280, 720));
    }

    fn rect(&self, id: ElemId) -> Rect {
        self.ui.arena.get(id).unwrap().rect
    }
}

fn column(height: Option<u32>, align: Option<(Align, Align)>, gap: u32) -> (Kind, Mods) {
    (
        Kind::Column,
        Mods {
            height,
            align,
            gap,
            ..Mods::default()
        },
    )
}

#[test]
fn spacer_takes_the_free_space() {
    let mut t = Tree::new();
    let (kind, mods) = column(Some(300), None, 10);
    let root = t.node(kind, mods);
    let a = t.leaf(None, Some(50));
    let spacer = t.node(Kind::Spacer, Mods::default());
    let b = t.leaf(None, Some(50));
    t.children(root, &[a, spacer, b]);
    t.mount(root);
    t.layout();
    assert_eq!(t.rect(a).y, 0.0);
    assert_eq!(t.rect(spacer).y, 60.0);
    assert_eq!(t.rect(spacer).h, 180.0);
    assert_eq!(t.rect(b).y, 250.0);
}

#[test]
fn space_between_spreads_items() {
    let mut t = Tree::new();
    let (kind, mods) = column(Some(100), Some((Align::SpaceBetween, Align::Start)), 0);
    let root = t.node(kind, mods);
    let items: Vec<ElemId> = (0..3).map(|_| t.leaf(None, Some(20))).collect();
    t.children(root, &items);
    t.mount(root);
    t.layout();
    let ys: Vec<f32> = items.iter().map(|i| t.rect(*i).y).collect();
    assert_eq!(ys, [0.0, 40.0, 80.0]);
}

#[test]
fn stretch_fills_the_cross_axis() {
    let mut t = Tree::new();
    let (kind, mut mods) = column(None, Some((Align::Start, Align::Stretch)), 0);
    mods.width = Some(400);
    let root = t.node(kind, mods);
    let stretched = t.leaf(None, Some(20));
    let fixed = t.leaf(Some(30), Some(20));
    t.children(root, &[stretched, fixed]);
    t.mount(root);
    t.layout();
    assert_eq!(t.rect(stretched).w, 400.0);
    assert_eq!(t.rect(fixed).w, 30.0);
}

#[test]
fn layer_places_the_anchor_at_the_point() {
    let mut t = Tree::new();
    let root = t.node(
        Kind::Layer {
            places: vec![Some(Place::At {
                x: 640.0,
                y: 520.0,
                ax: 0.5,
                ay: 0.5,
            })],
        },
        Mods::default(),
    );
    let child = t.leaf(Some(300), Some(100));
    t.children(root, &[child]);
    t.mount(root);
    t.layout();
    let r = t.rect(child);
    assert_eq!((r.x, r.y, r.w, r.h), (490.0, 470.0, 300.0, 100.0));
}

#[test]
fn a_root_without_size_fills_the_screen() {
    let mut t = Tree::new();
    let root = t.node(Kind::Column, Mods::default());
    t.mount(root);
    t.layout();
    let r = t.rect(root);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 1280.0, 720.0));
}

#[test]
fn each_rows_splice_into_a_row() {
    let mut t = Tree::new();
    let root = t.node(
        Kind::Row,
        Mods {
            gap: 8,
            ..Mods::default()
        },
    );
    // The each's own modifiers are ignored when it is spliced.
    let each = t.node(
        Kind::Each,
        Mods {
            gap: 100,
            ..Mods::default()
        },
    );
    let items: Vec<ElemId> = (0..3).map(|_| t.leaf(Some(46), Some(46))).collect();
    t.children(each, &items);
    t.children(root, &[each]);
    t.mount(root);
    t.layout();
    let xs: Vec<f32> = items.iter().map(|i| t.rect(*i).x).collect();
    assert_eq!(xs, [0.0, 54.0, 108.0]);
}

/// `show(…, || each(…).gap(40)).at(…)` in a layer: the show lays out as a column, and the each
/// inside it is not spliced, so its own gap applies.
#[test]
fn each_inside_a_placed_show_keeps_its_gap() {
    let mut t = Tree::new();
    let root = t.node(
        Kind::Layer {
            places: vec![Some(Place::At {
                x: 640.0,
                y: 150.0,
                ax: 0.5,
                ay: 0.5,
            })],
        },
        Mods::default(),
    );
    let show = t.node(Kind::Show, Mods::default());
    let each = t.node(
        Kind::Each,
        Mods {
            gap: 40,
            ..Mods::default()
        },
    );
    let items: Vec<ElemId> = (0..3).map(|_| t.leaf(Some(500), Some(42))).collect();
    t.children(each, &items);
    t.children(show, &[each]);
    t.children(root, &[show]);
    t.mount(root);
    t.layout();
    let ys: Vec<f32> = items.iter().map(|i| t.rect(*i).y).collect();
    assert_eq!(ys, [47.0, 129.0, 211.0]);
}

#[test]
fn scroll_shrinks_and_scrolls_by_wheel_notches() {
    let mut t = Tree::new();
    let root = t.node(Kind::Column, Mods::default());
    let scroll = t.node(
        Kind::Scroll {
            offset: 0.0,
            content: 0.0,
        },
        Mods {
            grow: Some(1),
            ..Mods::default()
        },
    );
    let content = t.leaf(Some(400), Some(2000));
    t.children(scroll, &[content]);
    let button = t.node(
        Kind::Button {
            on_click: None,
            disabled: false,
            style: None,
        },
        Mods {
            height: Some(40),
            ..Mods::default()
        },
    );
    t.children(root, &[scroll, button]);
    t.mount(root);
    t.layout();
    assert_eq!(t.rect(scroll).h, 680.0);
    assert_eq!(t.rect(button).y, 680.0);

    t.ui.pointer = Some([10.0, 10.0]);
    assert_eq!(wheel(&mut t.ui, 0, -1.0), UiInput::Consumed);
    t.layout();
    assert_eq!(t.rect(content).y, -40.0);

    for _ in 0..100 {
        wheel(&mut t.ui, 0, -1.0);
    }
    t.layout();
    let Kind::Scroll { offset, .. } = t.ui.arena.get(scroll).unwrap().kind else {
        panic!("not a scroll");
    };
    assert_eq!(offset, 1320.0);

    // Outside every scroll the wheel is left to the bindings.
    t.ui.pointer = Some([10.0, 700.0]);
    assert_eq!(wheel(&mut t.ui, 0, -1.0), UiInput::Ignored);
}

#[test]
fn scroll_into_view_scrolls_inside_the_padding() {
    let mut t = Tree::new();
    let root = t.node(Kind::Column, Mods::default());
    let scroll = t.node(
        Kind::Scroll {
            offset: 0.0,
            content: 0.0,
        },
        Mods {
            height: Some(200),
            padding: [50.0, 0.0, 50.0, 0.0],
            ..Mods::default()
        },
    );
    let content = t.node(Kind::Column, Mods::default());
    let rows: Vec<ElemId> = (0..10).map(|_| t.leaf(Some(100), Some(50))).collect();
    t.children(content, &rows);
    t.children(scroll, &[content]);
    t.children(root, &[scroll]);
    t.mount(root);
    t.layout();
    let last = *rows.last().unwrap();
    assert!(super::focus::scroll_into_view(&mut t.ui.arena, last, true));
    t.layout();
    let Kind::Scroll { offset, .. } = t.ui.arena.get(scroll).unwrap().kind else {
        panic!("not a scroll");
    };
    // 500 px of content in a 100 px view (200 minus the padding).
    assert_eq!(offset, 400.0);
    let r = t.rect(last);
    assert!(r.y >= 50.0 && r.y + r.h <= 150.0, "{r:?}");
}

#[test]
fn a_select_aligns_its_label_like_a_button() {
    let mut t = Tree::new();
    let root = t.node(Kind::Column, Mods::default());
    let label = t.leaf(Some(40), Some(20));
    let select = t.node(
        Kind::Select {
            options: vec![Some((kagari_vm::Value::Bool(true), label))],
            value: Some(kagari_vm::Value::Bool(true)),
            on_change: None,
            disabled: false,
            style: None,
        },
        Mods {
            width: Some(200),
            height: Some(100),
            align: Some((Align::Start, Align::Start)),
            ..Mods::default()
        },
    );
    t.children(root, &[select]);
    t.mount(root);
    t.layout();
    let r = t.rect(label);
    assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 40.0, 20.0));
}

/// A title page whose button counts clicks into a signal that a reactive text shows.
const COUNTER: &str = r#"
use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    System {
        title: "Counter",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 5,
        auto_slots: 1,
        backlog: 10,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn Title() -> RootPage {
    let (count, set_count) = signal::<u32>(0);
    RootPage(column![
        text!(t!["Count " {count()}]),
        button!{on_click = || set_count(count() + 1), t!["Add"]}.width(200).height(40),
        std::ui::show(|| count() > 1, || std::ui::label(t!["Many"])),
    ].gap(10).padding(std::screen::Insets { top: 20, right: 0, bottom: 0, left: 30 }))
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
"#;

fn texts(nodes: &[crate::frame::Node]) -> Vec<([f32; 2], u32)> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            crate::frame::Node::Text { origin, layout, .. } => out.push((*origin, layout.units)),
            crate::frame::Node::Group { children, .. } => out.extend(texts(children)),
            _ => {}
        }
    }
    out
}

#[test]
fn a_mounted_page_reacts_to_clicks() {
    use crate::HostCtx;
    use crate::input::{InputEvent, MouseButton};
    use crate::system::SystemConfig;
    use crate::system::modes::ModeFrame;
    use crate::test_support::{FakePlatform, bound_engine};

    let (mut vm, mut state) = bound_engine(COUNTER);
    let mut platform = FakePlatform;
    let program = vm.program().clone();
    let mut ctx = HostCtx {
        state: &mut state,
        platform: &mut platform,
        now_ms: 0,
    };
    let system = vm.call(&mut ctx, program.main(), Vec::new()).unwrap();
    let config = SystemConfig::decode(&vm, &ctx.state.std, &system).unwrap();
    ctx.state.assets.load_fonts(ctx.platform).unwrap();
    let font = ctx.state.assets.font(config.font).unwrap();
    let style = ctx
        .state
        .text
        .decode_style(&ctx.state.assets, &config.text)
        .unwrap();
    ctx.state.text.set_system(font, style);
    let start = config.start.clone();
    ctx.state.system.config = Some(config);

    let frame = ModeFrame {
        kind: ModeKind::Root,
        component: start,
        narrative: None,
        autofocus: true,
    };
    super::mount(&mut ctx, &mut vm, &frame).unwrap();
    super::flush(&mut ctx, &mut vm).unwrap();
    let (nodes, capture) = super::draw::frame_nodes(ctx.state, 0);
    assert!(!capture);
    // "Count 0" and "Add", in the padded column; the button is focused but focus is hidden.
    let shown = texts(&nodes);
    assert_eq!(shown.len(), 2, "{nodes:?}");
    assert_eq!(shown[0], ([30.0, 20.0], 7));
    assert_eq!(shown[1].1, 3);

    // Click the button twice: the signal re-runs the text and turns the `show` on.
    let button = ctx.state.ui.mounts[0].focus.order[0];
    let r = ctx.state.ui.arena.get(button).unwrap().rect;
    assert_eq!((r.x, r.w, r.h), (30.0, 200.0, 40.0));
    let (x, y) = (r.x + 10.0, r.y + 10.0);
    super::handle_input(&mut ctx, &mut vm, &InputEvent::MouseMove { x, y }).unwrap();
    for _ in 0..2 {
        for pressed in [true, false] {
            let event = InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
            };
            let used = super::handle_input(&mut ctx, &mut vm, &event).unwrap();
            assert_eq!(used, UiInput::Consumed);
        }
        super::flush(&mut ctx, &mut vm).unwrap();
    }
    let (nodes, _) = super::draw::frame_nodes(ctx.state, 16);
    let shown = texts(&nodes);
    assert_eq!(shown.len(), 3, "{nodes:?}");
    assert_eq!(shown[0].1, 7);
    // "Many" sits below the 40 px button with the 10 px gap.
    assert_eq!(shown[2].0[1], (r.y + r.h + 10.0).round());
    // The hovered button draws the default hover background.
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, crate::frame::Node::Fill { rect, .. } if rect.x == 30.0))
    );

    ctx.state.ui.unmount_all();
    assert!(ctx.state.ui.mounts.is_empty());
}
