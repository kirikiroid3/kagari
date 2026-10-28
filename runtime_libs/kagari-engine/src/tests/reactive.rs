//! §4.11 Reactive: signals, `each`, `effect` and `show` through a booted engine.

use std::rc::Rc;

use kagari_vm::Value;

use super::{SYSTEM, TestPlatform, boot, key, static_id};
use crate::{Engine, Key};

/// Activates the root's first button: the first focus key shows focus, Enter clicks.
fn click_first(e: &mut Engine, p: &mut TestPlatform, now: u64) {
    key(e, p, Key::Tab, now);
    key(e, p, Key::Enter, now);
    e.update(p, now);
}

fn meta(e: &Engine, path: &str) -> Value {
    let id = static_id(e, path);
    e.state.system.stores.meta[&id].clone()
}

fn log_len(e: &Engine) -> usize {
    match meta(e, "crate::LOG") {
        Value::Vec(items) => items.len(),
        other => panic!("LOG is a {}", other.tag()),
    }
}

const LOG: &str = r#"
use std::ui::{each, effect, label, show};

static LOG: Meta<Vec<u32>> = meta(Vec::new());
static MARK: Meta<bool> = meta(false);

fn record(x: u32) {
    let mut v = LOG.get();
    v.push(x);
    LOG.set(v);
}
"#;

fn source(title: &str) -> String {
    format!("{SYSTEM}{LOG}{title}")
}

#[test]
fn setting_an_equal_value_does_not_rerun() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let (get, set) = signal::<u32>(1);
    effect(|| {
        let _ = get();
        MARK.set(true);
    });
    RootPage(column![button!{on_click = || set(1), label(t!["Same"])}.width(100).height(40)])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    assert!(matches!(meta(&e, "crate::MARK"), Value::Bool(true)));
    let mark = static_id(&e, "crate::MARK");
    e.state.system.stores.meta.insert(mark, Value::Bool(false));
    click_first(&mut e, &mut p, 10);
    assert_eq!(e.crash(), None);
    assert!(matches!(meta(&e, "crate::MARK"), Value::Bool(false)));
}

#[test]
fn each_keeps_equal_rows() {
    let src = source(
        r#"
fn three(first: u32) -> Vec<u32> {
    let mut v = Vec::new();
    v.push(first);
    v.push(first + 1);
    v.push(first + 2);
    v
}

fn item(x: u32) -> Element {
    record(x);
    label(t!["item"])
}

fn Title() -> RootPage {
    let (items, set_items) = signal::<Vec<u32>>(three(1));
    RootPage(column![
        button!{on_click = || set_items(three(2)), label(t!["Next"])}.width(100).height(40),
        each(|| items(), |x| item(x)),
    ])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    assert_eq!(log_len(&e), 3);
    click_first(&mut e, &mut p, 10);
    assert_eq!(e.crash(), None);
    assert_eq!(log_len(&e), 4);
    let Value::Vec(items) = meta(&e, "crate::LOG") else {
        panic!()
    };
    assert!(matches!(items.last(), Some(Value::Int(4))));
    let _: &Rc<Vec<Value>> = &items;
}

#[test]
fn an_effect_that_feeds_itself_does_not_settle() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let (get, set) = signal::<u32>(0);
    effect(|| set(get() + 1));
    RootPage(column![])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    e.update(&mut p, 10);
    let crash = e.crash().expect("the update must stop the game");
    assert!(crash.contains("reactive update did not settle"), "{crash}");
}

#[test]
fn show_runs_its_branch_once_while_true() {
    let src = source(
        r#"
fn branch() -> Element {
    record(1);
    label(t!["shown"])
}

fn Title() -> RootPage {
    let (n, set_n) = signal::<u32>(0);
    RootPage(column![
        button!{on_click = || set_n(n() + 1), label(t!["More"])}.width(100).height(40),
        show(|| n() > 0, || branch()),
    ])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    assert_eq!(log_len(&e), 0);
    click_first(&mut e, &mut p, 10);
    assert_eq!(log_len(&e), 1);
    key(&mut e, &mut p, Key::Enter, 20);
    e.update(&mut p, 20);
    assert_eq!(e.crash(), None);
    assert_eq!(log_len(&e), 1);
}

/// Boots `src` and returns the crash message `boot` would assert away.
fn boot_crash(src: &str) -> Option<String> {
    let mut p = TestPlatform::default();
    let e = Engine::boot(&crate::test_support::compile(src), &mut p, 0).unwrap();
    e.crash().map(str::to_owned)
}

#[test]
fn an_element_moves_once_its_parent_is_gone() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let (open, set_open) = signal::<bool>(true);
    let title = label(t!["Title"]);
    RootPage(column![
        button!{on_click = || set_open(!open()), label(t!["Toggle"])}.width(100).height(40),
        show(|| open(), || column![title]),
    ])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    click_first(&mut e, &mut p, 10);
    key(&mut e, &mut p, Key::Enter, 20);
    e.update(&mut p, 20);
    assert_eq!(e.crash(), None);
}

#[test]
fn each_rows_cannot_share_an_element() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let shared = label(t!["Shared"]);
    let mut items: Vec<u32> = Vec::new();
    items.push(1);
    items.push(2);
    RootPage(column![each(|| items, |_x| shared)])
}
"#,
    );
    let crash = boot_crash(&src).expect("two rows showing one element must stop the game");
    assert!(
        crash.contains("an element can be shown in one place only"),
        "{crash}"
    );
}

#[test]
fn a_select_label_shown_elsewhere_traps() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let on = label(t!["On"]);
    RootPage(column![
        on,
        select!{
            value = true,
            std::ui::option(true, on),
            std::ui::option(false, label(t!["Off"])),
        },
    ])
}
"#,
    );
    let crash = boot_crash(&src).expect("a label shown twice must stop the game");
    assert!(
        crash.contains("an element can be shown in one place only"),
        "{crash}"
    );
}

fn has_faded_group(nodes: &[crate::frame::Node]) -> bool {
    nodes.iter().any(|node| match node {
        crate::frame::Node::Group {
            opacity, children, ..
        } => *opacity < 1.0 || has_faded_group(children),
        _ => false,
    })
}

#[test]
fn a_spliced_fragment_ignores_its_opacity() {
    let src = source(
        r#"
fn Title() -> RootPage {
    let mut items: Vec<u32> = Vec::new();
    items.push(1);
    items.push(2);
    RootPage(row![each(|| items, |_x| label(t!["x"])).opacity(0.5)])
}
"#,
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&src, &mut p);
    e.update(&mut p, 0);
    let frame = e.frame(0);
    assert!(!has_faded_group(&frame.nodes));
}
