use kagari_archive::save::{Save, ValueRef};
use kagari_vm::Value;

use super::{TestPlatform, boot, key, line, mouse, narrative, story};
use crate::story::narrative::NarrativeImport;
use crate::{Key, MouseButton, Reveal};

#[test]
fn say_advances_and_records_read_flags() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    assert_eq!(line(&e).as_deref(), Some("One."));
    let crate::story::Current::Say(say) = &e.state.game.as_ref().unwrap().current else {
        panic!("not at a line");
    };
    assert!(!say.read);
    key(&mut e, &mut p, Key::Space, 10);
    assert_eq!(line(&e).as_deref(), Some("One."));
    let view = e.state.game.as_ref().unwrap().message(10).unwrap();
    assert_eq!(view.reveal, Reveal::All);
    key(&mut e, &mut p, Key::Space, 20);
    assert_eq!(line(&e).as_deref(), Some("Two."));
    e.update(&mut p, 21);
    let meta = Save::from_bytes(p.stores["meta.kisave"].clone()).unwrap();
    let Some(ValueRef::Table(read)) = meta.root().get("read") else {
        panic!("no read table");
    };
    assert_eq!(read.get("crate::Start#0"), Some(ValueRef::Bool(true)));

    let e = boot(&story(), &mut p);
    let crate::story::Current::Say(say) = &e.state.game.as_ref().unwrap().current else {
        panic!("not at a line");
    };
    assert_eq!(line(&e).as_deref(), Some("One."));
    assert!(say.read);
}

fn visibility(v: &Value, field: usize) -> u32 {
    let Value::Adt(hud) = v else { panic!() };
    let Value::Adt(vis) = &hud.fields[field] else {
        panic!()
    };
    vis.variant
}

#[test]
fn hide_window_then_advance_unhides() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    e.update(&mut p, 1000);
    mouse(&mut e, &mut p, MouseButton::Right, 1000);
    let hud = narrative(&mut e, &mut p, NarrativeImport::Hud, vec![], 1000);
    assert_eq!((visibility(&hud, 0), visibility(&hud, 1)), (1, 1));
    key(&mut e, &mut p, Key::Space, 1010);
    let hud = narrative(&mut e, &mut p, NarrativeImport::Hud, vec![], 1010);
    assert_eq!((visibility(&hud, 0), visibility(&hud, 1)), (0, 0));
    assert_eq!(line(&e).as_deref(), Some("One."));
    key(&mut e, &mut p, Key::Space, 1020);
    assert_eq!(line(&e).as_deref(), Some("Two."));
}

#[test]
fn a_centred_message_sits_in_the_middle_of_its_box() {
    let source = format!(
        "use std::ui::message;\nuse std::narrative::text::TextAlign;\n{}",
        story()
    )
    .replace(
        "game_screen!(game = game)",
        "message(game, text_style(26, WHITE).align(TextAlign::Center))",
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&source, &mut p);
    e.update(&mut p, 5000);
    let frame = e.frame(5000);
    fn first_text(nodes: &[crate::frame::Node]) -> Option<&crate::frame::Node> {
        nodes.iter().find_map(|node| match node {
            crate::frame::Node::Text { .. } => Some(node),
            crate::frame::Node::Group { children, .. } => first_text(children),
            _ => None,
        })
    }
    let Some(crate::frame::Node::Text { origin, layout, .. }) = first_text(&frame.nodes) else {
        panic!("no message text");
    };
    // "One." is centred in the 1280 px wide game view.
    let expected = ((1280.0 - layout.size[0]) / 2.0).round();
    assert_eq!(origin[0], expected);
}
