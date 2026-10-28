use std::rc::Rc;

use kagari_vm::{Adt, AdtTy, Text, Value};

use crate::Reveal;
use crate::screen::runtime::MarkTimes;
use crate::story::reveal::{RevealPlan, RevealRun, RevealState, Speeds};

/// An ADT whose type the reveal never checks.
fn adt(variant: u32, fields: Vec<Value>) -> Value {
    Value::Adt(Rc::new(Adt {
        ty: AdtTy::Synthetic(0),
        variant,
        fields,
    }))
}

fn none() -> Value {
    adt(0, vec![])
}

fn some(v: Value) -> Value {
    adt(1, vec![v])
}

/// An empty text under a `TextEffect` with only `pause` and `mark` set.
fn effect(pause: Value, mark: Value) -> Text {
    let mut fields = vec![none(); 9];
    fields.push(pause);
    fields.push(mark);
    Text::Effect(Rc::new((Text::Plain(Rc::from("")), adt(0, fields))))
}

fn text(middle: Text) -> Text {
    Text::Concat(Rc::new(vec![
        Text::Plain(Rc::from("Ab")),
        middle,
        Text::Plain(Rc::from("c")),
    ]))
}

fn run(text: &Text) -> RevealRun {
    RevealRun::new(
        RevealPlan::build(Some(text), Speeds { text_speed_ms: 10 }),
        0,
    )
}

#[test]
fn reveal_pause_mark_and_click() {
    let pause = some(adt(0, vec![adt(0, vec![Value::Int(100)])]));
    let mark = some(adt(0, vec![Value::Int(1)]));
    let r = run(&text(effect(pause, mark)));
    assert_eq!(r.mark_ms(1), Some(120));
    assert_eq!(r.mark_ms(9), Some(130));

    let mut r = run(&text(effect(some(adt(1, vec![])), none())));
    assert_eq!(r.state(25), RevealState::WaitingClick { since: 20 });
    assert_eq!(r.reveal(25), Reveal::Units(2));
    assert_eq!(r.mark_ms(9), None);
    r.release(40);
    assert_eq!(
        r.reveal(45),
        Reveal::UnitsWithFade {
            full: 2,
            partial: 0.5
        }
    );
    assert_eq!(r.reveal(50), Reveal::All);
}

#[test]
fn a_completed_segment_keeps_its_forced_end() {
    // "Ab" | click | "c" at 10 ms per unit: the first segment ends naturally at 20.
    let mut r = run(&text(effect(some(adt(1, vec![])), none())));
    r.complete_segment(5);
    assert_eq!(r.state(5), RevealState::WaitingClick { since: 5 });
    assert_eq!(r.state(30), RevealState::WaitingClick { since: 5 });
    r.release(40);
    r.complete_segment(42);
    assert_eq!(r.state(100), RevealState::Done { at: 42 });
}
