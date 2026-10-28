#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;

use kagari_bytecode::StaticKind;
use kagari_vm::{AssetId, StaticId, Value};

use super::*;
use crate::assets::AssetStore;
use crate::frame::Frame;
use crate::saves::Timestamp;
use crate::screen::sample::{VisualCamera, VisualEntity, VisualHud, VisualLayer, VisualScreen};
use crate::test_support::{FakePlatform, STD_TYPES, engine};
use crate::values::{BlendMode, Flip, Plane};
use crate::{EngineState, Platform};

/// `FakePlatform`, except that PNG assets are a 100×200 image.
struct PngPlatform;

impl Platform for PngPlatform {
    fn read_store(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        FakePlatform.read_store(name)
    }
    fn write_store(&mut self, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        FakePlatform.write_store(name, bytes)
    }
    fn delete_store(&mut self, name: &str) -> std::io::Result<()> {
        FakePlatform.delete_store(name)
    }
    fn read_asset(&mut self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        if path.ends_with(".png") {
            let mut bytes = Vec::new();
            image::RgbaImage::new(100, 200)
                .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
                .unwrap();
            return Ok(Some(bytes));
        }
        FakePlatform.read_asset(path)
    }
    fn audio_duration_ms(&mut self, path: &str, bytes: &[u8]) -> Option<u32> {
        FakePlatform.audio_duration_ms(path, bytes)
    }
    fn render_thumbnail(&mut self, f: &Frame, a: &AssetStore, w: u32, h: u32) -> Option<Vec<u8>> {
        FakePlatform.render_thumbnail(f, a, w, h)
    }
    fn local_time(&self) -> Timestamp {
        FakePlatform.local_time()
    }
    fn random_seed(&mut self) -> u64 {
        7
    }
    fn set_title(&mut self, _title: &str) {}
}

/// Engine state with `a.png` (100×200) loaded; returns its id.
fn setup() -> (EngineState, AssetId) {
    let (_vm, mut state) = engine(STD_TYPES);
    let program = state.program.clone();
    let index = program
        .module()
        .assets
        .iter()
        .position(|a| program.string(a.path) == "a.png")
        .unwrap();
    let id = AssetId(u32::try_from(index).unwrap());
    state.assets.ensure_image(id, &mut PngPlatform).unwrap();
    (state, id)
}

fn entity(id: u32) -> VisualEntity {
    VisualEntity {
        id,
        plane: Plane::Stage,
        z: 0,
        position: [0.0, 0.0],
        scale: 1.0,
        rotation_deg: 0.0,
        opacity: 1.0,
        anchor: [0.0, 0.0],
        fill: [0.0; 4],
        fill_size: None,
        parts: Vec::new(),
        label: VisualLayer::Empty,
        label_style: None,
        label_width: None,
        flip: Flip::None,
        blend: BlendMode::Normal,
        gate: None,
    }
}

fn screen(entities: Vec<VisualEntity>) -> VisualScreen {
    VisualScreen {
        entities,
        camera: VisualCamera {
            offset: [0.0, 0.0],
            zoom: 1.0,
            rotation_deg: 0.0,
        },
        hud: VisualHud {
            text_window: 1.0,
            quick_menu: 1.0,
            side_image: VisualLayer::Empty,
        },
    }
}

fn draw(state: &mut EngineState, v: &VisualScreen) -> ScreenPlanes {
    draw_screen(
        v,
        (1280, 720),
        &state.assets,
        &mut state.text,
        &state.system,
    )
}

fn apply(m: Affine, p: [f32; 2]) -> [f32; 2] {
    let [a, b, c, d, e, f] = m.0;
    [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
}

fn close(a: [f32; 2], b: [f32; 2]) {
    assert!(
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
        "{a:?} != {b:?}"
    );
}

fn world_children(planes: &ScreenPlanes) -> &[Node] {
    match &planes.world {
        Node::Group { children, .. } => children,
        other => panic!("world is {other:?}"),
    }
}

/// The local group (M and its children) of an entity node.
fn local(node: &Node) -> (Affine, &[Node]) {
    let Node::Group { children, .. } = node else {
        panic!("entity is {node:?}")
    };
    match children.last().unwrap() {
        Node::Group {
            transform,
            children,
            ..
        } => (*transform, children),
        other => panic!("local is {other:?}"),
    }
}

#[test]
fn camera_maps_offset_point_to_centre() {
    let camera = VisualCamera {
        offset: [10.0, 20.0],
        zoom: 2.0,
        rotation_deg: 90.0,
    };
    let m = camera_transform(&camera, [1280.0, 720.0]);
    close(apply(m, [650.0, 380.0]), [640.0, 360.0]);
    // One px right of it: rotated by −90° (to up), then zoomed ×2.
    close(apply(m, [651.0, 380.0]), [640.0, 358.0]);
}

#[test]
fn anchored_scaled_main_image() {
    let (mut state, png) = setup();
    let mut e = entity(1);
    e.position = [640.0, 720.0];
    e.anchor = [0.5, 1.0];
    e.scale = 2.0;
    e.parts.push((0, VisualLayer::Shown(png)));
    let planes = draw(&mut state, &screen(vec![e]));
    let (m, children) = local(&world_children(&planes)[0]);
    let Node::Image { rect, .. } = &children[0] else {
        panic!("{children:?}")
    };
    assert_eq!((rect.x, rect.y, rect.w, rect.h), (0.0, 0.0, 100.0, 200.0));
    close(apply(m, [rect.x, rect.y]), [540.0, 320.0]);
    close(apply(m, [rect.x + rect.w, rect.y + rect.h]), [740.0, 720.0]);
}

#[test]
fn gated_entity_is_skipped_unless_setting_true() {
    let (mut state, png) = setup();
    let program = state.program.clone();
    let index = program
        .module()
        .statics
        .iter()
        .position(|s| s.kind == StaticKind::Setting)
        .unwrap();
    let gate = StaticId(u32::try_from(index).unwrap());
    let mut e = entity(1);
    e.parts.push((0, VisualLayer::Shown(png)));
    e.gate = Some(gate);
    let v = screen(vec![e]);
    assert!(world_children(&draw(&mut state, &v)).is_empty());
    state.system.stores.settings.insert(gate, Value::Bool(true));
    assert_eq!(world_children(&draw(&mut state, &v)).len(), 1);
}

#[test]
fn transparent_entity_is_skipped() {
    let (mut state, png) = setup();
    let mut e = entity(1);
    e.opacity = 0.0;
    e.parts.push((0, VisualLayer::Shown(png)));
    assert!(world_children(&draw(&mut state, &screen(vec![e]))).is_empty());
}

#[test]
fn flip_x_mirrors_a_non_main_part() {
    let (mut state, png) = setup();
    let mut e = entity(1);
    e.fill_size = Some([300, 300]);
    e.parts.push((1, VisualLayer::Shown(png)));
    e.flip = Flip::Horizontal;
    let planes = draw(&mut state, &screen(vec![e]));
    let (_, children) = local(&world_children(&planes)[0]);
    let Node::Image { rect, flip_x, .. } = &children[0] else {
        panic!("{children:?}")
    };
    assert!(*flip_x);
    assert_eq!((rect.x, rect.y), (200.0, 0.0));
}

#[test]
fn over_window_entity_is_outside_the_camera() {
    let (mut state, png) = setup();
    let mut stage = entity(1);
    stage.parts.push((0, VisualLayer::Shown(png)));
    let mut over = entity(2);
    over.plane = Plane::OverWindow;
    over.parts.push((0, VisualLayer::Shown(png)));
    let planes = draw(&mut state, &screen(vec![stage, over]));
    assert_eq!(world_children(&planes).len(), 1);
    assert_eq!(planes.over_window.len(), 1);
}

#[test]
fn unsized_fill_covers_the_screen_outside_m() {
    let (mut state, _) = setup();
    let mut e = entity(1);
    e.position = [100.0, 100.0];
    e.fill = [0.0, 0.0, 0.0, 1.0];
    let planes = draw(&mut state, &screen(vec![e]));
    let Node::Group { children, .. } = &world_children(&planes)[0] else {
        panic!()
    };
    let Node::Fill { rect, .. } = &children[0] else {
        panic!("{children:?}")
    };
    assert_eq!((rect.x, rect.y, rect.w, rect.h), (0.0, 0.0, 1280.0, 720.0));
}

#[test]
fn image_layer_transition_spans_both_images() {
    let (state, png) = setup();
    let layer = VisualLayer::Transition {
        from: None,
        to: Some(png),
        kind: crate::values::TransitionKind::Crossfade,
        progress: 0.25,
        hold_fraction: 0.0,
    };
    let node = image_layer_node(&layer, [10.0, 520.0], &state.assets).unwrap();
    let Node::Transition {
        bounds, from, to, ..
    } = node
    else {
        panic!()
    };
    assert_eq!(
        (bounds.x, bounds.y, bounds.w, bounds.h),
        (10.0, 520.0, 100.0, 200.0)
    );
    assert!(from.is_empty());
    assert_eq!(to.len(), 1);
}
