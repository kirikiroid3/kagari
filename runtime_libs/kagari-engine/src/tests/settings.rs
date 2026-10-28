use kagari_vm::Value;

use super::{TestPlatform, at_choice, boot, key, line, static_id, story};
use crate::story::PlayMode;
use crate::story::driver::set_mode;
use crate::system::imports::SystemImport;
use crate::values::StdTy;
use crate::{Engine, Key, Reveal};

fn mode(e: &Engine) -> PlayMode {
    e.state.game.as_ref().unwrap().mode
}

fn skip(e: &mut Engine, p: &mut TestPlatform) {
    e.with_ctx(p, 0, |ctx, _| {
        set_mode(ctx, PlayMode::Skip);
        Ok(())
    });
}

fn set_setting(e: &mut Engine, p: &mut TestPlatform, path: &str, value: Value) {
    let id = static_id(e, path);
    e.with_ctx(p, 0, |ctx, vm| {
        SystemImport::SettingSet
            .call(ctx, vm, vec![Value::Handle(id), value])
            .map(|_| ())
    });
    e.update(p, 0);
}

#[test]
fn skip_stops_at_unread() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    key(&mut e, &mut p, Key::Space, 10);
    key(&mut e, &mut p, Key::Space, 20);
    e.update(&mut p, 21);

    let mut e = boot(&story(), &mut p);
    skip(&mut e, &mut p);
    for now in [0, 50, 100, 150] {
        e.update(&mut p, now);
    }
    assert_eq!(line(&e).as_deref(), Some("Two."));
    assert_eq!(mode(&e), PlayMode::Normal);

    set_setting(&mut e, &mut p, "crate::SKIP_UNREAD", Value::Bool(true));
    let mut e = boot(&story(), &mut p);
    skip(&mut e, &mut p);
    for now in [0, 50, 100, 150, 200] {
        e.update(&mut p, now);
    }
    assert!(at_choice(&e));
    assert_eq!(mode(&e), PlayMode::Normal);
}

#[test]
fn settings_persist_across_boots() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let duration = e.state.std.make(StdTy::Duration, 0, vec![Value::Int(45)]);
    set_setting(&mut e, &mut p, "crate::TEXT_SPEED", duration);
    let e = boot(&story(), &mut p);
    let view = e.state.game.as_ref().unwrap().message(100).unwrap();
    assert_eq!(
        view.reveal,
        Reveal::UnitsWithFade {
            full: 2,
            partial: 10.0 / 45.0
        }
    );
    p.stores
        .insert("settings.kisave".to_owned(), b"not a save".to_vec());
    let e = boot(&story(), &mut p);
    let view = e.state.game.as_ref().unwrap().message(100).unwrap();
    assert_eq!(
        view.reveal,
        Reveal::UnitsWithFade {
            full: 3,
            partial: 10.0 / 30.0
        }
    );
}

#[test]
fn plain_text_round_trips_through_a_store() {
    use std::rc::Rc;

    use kagari_archive::save::Save;
    use kagari_vm::Text;

    use crate::store::value::{Names, decode, encode_field};

    let mut p = TestPlatform::default();
    let e = boot(&story(), &mut p);
    let names = Names {
        program: &e.state.program,
        types: e.vm.types(),
    };
    let text = Value::Text(Text::Plain(Rc::from("hi")));
    let tuple = Value::Tuple(Rc::new(vec![text.clone()]));
    let mut save = Save::new();
    encode_field(&names, &mut save.root_mut(), "text", &text).unwrap();
    encode_field(&names, &mut save.root_mut(), "tuple", &tuple).unwrap();
    let root = save.root();
    let back = decode(&e.state.program, root.get("text").unwrap()).unwrap();
    assert!(matches!(&back, Value::Text(Text::Plain(s)) if &**s == "hi"));
    let Value::Tuple(items) = decode(&e.state.program, root.get("tuple").unwrap()).unwrap() else {
        panic!("not a tuple");
    };
    assert!(matches!(&items[0], Value::Text(Text::Plain(s)) if &**s == "hi"));
}
