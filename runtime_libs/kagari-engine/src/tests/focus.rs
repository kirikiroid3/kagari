//! §4.11 Focus: the initial focus, showing focus, and slider keys through a booted engine.

use kagari_vm::Value;

use super::{SYSTEM, TestPlatform, boot, key, static_id};
use crate::system::NavOp;
use crate::ui::{self, UiInput};
use crate::{Engine, InputEvent, Key, MouseButton};

fn setting(e: &Engine, path: &str) -> Value {
    let id = static_id(e, path);
    e.state.system.stores.settings[&id].clone()
}

fn which(e: &Engine) -> i128 {
    match setting(e, "crate::WHICH") {
        Value::Int(n) => n,
        other => panic!("WHICH is a {}", other.tag()),
    }
}

#[test]
fn focus_order_and_hidden_focus() {
    let src = format!(
        "{SYSTEM}{}",
        r#"
use std::ui::label;

static WHICH: Setting<Int<0, 2>> = setting(0);

fn Title() -> RootPage {
    RootPage(column![
        button!{on_click = || WHICH.set(2), label(t!["Two"])}.width(100).height(40).focus_order(2),
        button!{on_click = || WHICH.set(1), label(t!["One"])}.width(100).height(40).focus_order(1),
    ])
}
"#
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    e.update(&mut p, 0);
    let mut handled = None;
    let enter = InputEvent::Key {
        key: Key::Enter,
        pressed: true,
    };
    e.with_ctx(&mut p, 1, |ctx, vm| {
        handled = Some(ui::handle_input(ctx, vm, &enter)?);
        Ok(())
    });
    assert_eq!(handled, Some(UiInput::Ignored));
    e.input(
        InputEvent::Key {
            key: Key::Enter,
            pressed: false,
        },
        &mut p,
        1,
    );
    assert_eq!(which(&e), 0);
    key(&mut e, &mut p, Key::Down, 10);
    key(&mut e, &mut p, Key::Enter, 10);
    e.update(&mut p, 10);
    assert_eq!(which(&e), 1);
    key(&mut e, &mut p, Key::Down, 20);
    key(&mut e, &mut p, Key::Enter, 20);
    e.update(&mut p, 20);
    assert_eq!(which(&e), 2);
}

#[test]
fn slider_steps_with_arrow_keys() {
    let src = format!(
        "{SYSTEM}{}",
        r#"
static LEVEL: Setting<Int<0, 100>> = setting(50);

fn Title() -> RootPage {
    RootPage(column![slider!{value = LEVEL.get(), on_change = |v| LEVEL.set(v), step = 10}])
}
"#
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    e.update(&mut p, 0);
    key(&mut e, &mut p, Key::Down, 10);
    key(&mut e, &mut p, Key::Right, 10);
    e.update(&mut p, 10);
    assert!(matches!(setting(&e, "crate::LEVEL"), Value::Int(60)));
}

fn level(e: &Engine) -> i128 {
    match setting(e, "crate::LEVEL") {
        Value::Int(n) => n,
        other => panic!("LEVEL is a {}", other.tag()),
    }
}

fn pointer(e: &mut Engine, p: &mut TestPlatform, x: f32, now: u64) {
    e.input(InputEvent::MouseMove { x, y: 12.0 }, p, now);
}

fn left(e: &mut Engine, p: &mut TestPlatform, pressed: bool, now: u64) {
    e.input(
        InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
        },
        p,
        now,
    );
}

/// A 200 px slider with 50 px of padding on each side: its track spans x 50..150.
const PADDED_SLIDER: &str = r#"
use std::screen::insets;
use std::ui::spacer;

static LEVEL: Setting<Int<0, 100>> = setting(50);

fn Title() -> RootPage {
    let _ = Over;
    RootPage(column![
        slider!{value = LEVEL.get(), on_change = |v| LEVEL.set(v)}
            .width(200)
            .height(24)
            .padding(insets(0, 50, 0, 50)),
    ])
}

fn Over() -> Overlay {
    Overlay(spacer())
}
"#;

#[test]
fn a_slider_maps_the_pointer_inside_its_padding() {
    let mut p = TestPlatform::default();
    let mut e = boot(&format!("{SYSTEM}{PADDED_SLIDER}"), &mut p);
    e.update(&mut p, 0);
    // The thumb centre at the track's left end (x 50 + half the 16 px thumb) is the minimum.
    pointer(&mut e, &mut p, 58.0, 10);
    left(&mut e, &mut p, true, 10);
    e.update(&mut p, 10);
    assert_eq!(level(&e), 0);
    left(&mut e, &mut p, false, 20);
}

#[test]
fn a_release_under_another_mode_ends_the_drag() {
    let mut p = TestPlatform::default();
    let mut e = boot(&format!("{SYSTEM}{PADDED_SLIDER}"), &mut p);
    e.update(&mut p, 0);
    pointer(&mut e, &mut p, 100.0, 10);
    left(&mut e, &mut p, true, 10);
    e.update(&mut p, 10);
    let over = e.state.program.find_function("crate::Over").unwrap();
    e.state.pending_nav.push(NavOp::Call(Value::Func(over)));
    e.update(&mut p, 20);
    assert_eq!(e.state.system.modes.len(), 2);
    left(&mut e, &mut p, false, 30);
    e.state.pending_nav.push(NavOp::Back);
    e.update(&mut p, 40);
    assert_eq!(e.state.system.modes.len(), 1);
    let before = level(&e);
    pointer(&mut e, &mut p, 140.0, 50);
    e.update(&mut p, 50);
    assert_eq!(level(&e), before);
}
