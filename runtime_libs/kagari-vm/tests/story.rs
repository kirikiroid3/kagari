#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::rc::Rc;

use common::{TestHost, compile, func, int};
use kagari_vm::{
    Adt, AdtTy, Program, StaticId, StepId, StoryFiber, StoryStatics, Suspend, TrapKind, Value, Vm,
};

fn static_id(program: &Program, path: &str) -> StaticId {
    let index = program
        .module()
        .statics
        .iter()
        .position(|s| program.string(s.path) == path)
        .unwrap_or_else(|| panic!("no static `{path}`"));
    StaticId(u32::try_from(index).unwrap())
}

fn read_static(fiber: &StoryFiber, id: StaticId) -> Value {
    fiber
        .statics()
        .cell(id)
        .expect("a story variable")
        .borrow()
        .clone()
}

fn expect_step(suspend: Suspend) -> (kagari_vm::FuncId, u32) {
    match suspend {
        Suspend::Step { step, .. } => (step.func, step.id),
        Suspend::Returned(value) => panic!("expected a step, the scene returned {value:?}"),
    }
}

/// The step ids of the STEPs of `f`, in code order.
fn code_order_ids(program: &Program, f: kagari_vm::FuncId) -> Vec<u32> {
    program.module().functions[f.0 as usize]
        .code
        .iter()
        .filter_map(|instr| match instr {
            kagari_bytecode::Instr::Step { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn step_ids_follow_source_order_not_code_layout() {
    let program = compile(
        r#"enum Pick { A, B }

#[scene]
#[entry]
pushdown fn Main() {
    let p = choice!{ id = "pick", t!["A"] => Pick::A, t!["B"] => Pick::B };
    match p {
        Pick::A => s!(t!["a"]),
        Pick::B => s!(t!["b"]),
    }
    let mut i: Int<0, 3> = 0;
    // The early `break` leads to the step after the loop, which is laid out first.
    loop {
        if i == 1 {
            break;
        }
        s!(t!["lap"]);
        i = 1;
    }
    s!(t!["end"]);
}

fn keep() {}
"#,
    );
    let main = func(&program, "crate::Main");
    let pick = program.find_type("crate::Pick").expect("Pick interned");
    // The blocks are laid out in reverse postorder: the step after the loop comes before the
    // loop's step.
    assert_eq!(code_order_ids(&program, main), [0, 1, 2, 4, 3]);

    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber = StoryFiber::start(&vm, main, vec![], StoryStatics::fresh(&program), 0).unwrap();
    let mut resume = |input: Value| expect_step(fiber.resume(&mut vm, &mut host, input).unwrap());
    assert_eq!(resume(Value::unit()), (main, 0));
    let b = Value::Adt(Rc::new(Adt {
        ty: AdtTy::Module(pick),
        variant: 1,
        fields: Vec::new(),
    }));
    assert_eq!(resume(b), (main, 2));
    assert_eq!(resume(Value::unit()), (main, 3));
    assert_eq!(resume(Value::unit()), (main, 4));
}

#[test]
fn trap_names_the_last_step_reached() {
    let program = compile(
        r#"enum Pick { A, B }
static mut GOT: Int<0, 2> = 0;

#[scene]
#[entry]
pushdown fn Main() {
    s!(t!["one"]);
    let p = choice!{ id = "pick", t!["A"] => Pick::A, t!["B"] => Pick::B };
    match p {
        Pick::A => GOT = 1,
        Pick::B => GOT = 2,
    }
}

fn keep() {}
"#,
    );
    let main = func(&program, "crate::Main");
    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber = StoryFiber::start(&vm, main, vec![], StoryStatics::fresh(&program), 0).unwrap();
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (main, 0)
    );
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (main, 1)
    );
    // Not a `Pick`: the `match` panics.
    let trap = fiber.resume(&mut vm, &mut host, Value::unit()).unwrap_err();
    assert_eq!(trap.step, Some(StepId { func: main, id: 1 }));
    let rendered = trap.render(&program);
    assert!(
        rendered.contains("at crate::Main (") && rendered.contains("), after step crate::Main#1"),
        "{rendered}"
    );
}

#[test]
fn steps_across_nested_scenes() {
    let program = compile(
        r#"enum Pick { A, B }
static mut GOT: Int<0, 2> = 0;

#[scene]
#[entry]
pushdown fn Outer() {
    s!(t!["one"]);
    Inner();
    s!(t!["three"]);
}

#[scene]
pushdown fn Inner() {
    let p = choice!{ id = "pick", t!["A"] => Pick::A, t!["B"] => Pick::B };
    match p {
        Pick::A => GOT = 1,
        Pick::B => GOT = 2,
    }
}

fn keep() {}
"#,
    );
    let outer = func(&program, "crate::Outer");
    let inner = func(&program, "crate::Inner");
    let pick = program.find_type("crate::Pick").expect("Pick interned");
    let got = static_id(&program, "crate::GOT");
    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber =
        StoryFiber::start(&vm, outer, vec![], StoryStatics::fresh(&program), 0).unwrap();
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (outer, 0)
    );
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (inner, 0)
    );
    let b = Value::Adt(Rc::new(Adt {
        ty: AdtTy::Module(pick),
        variant: 1,
        fields: Vec::new(),
    }));
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, b).unwrap()),
        (outer, 1)
    );
    assert_eq!(int(&read_static(&fiber, got)), 2);
    match fiber.resume(&mut vm, &mut host, Value::unit()).unwrap() {
        Suspend::Returned(Value::Tuple(items)) => assert!(items.is_empty()),
        other => panic!("expected the scene to return unit, found {other:?}"),
    }
    let trap = fiber.resume(&mut vm, &mut host, Value::unit()).unwrap_err();
    assert_eq!(trap.kind, TrapKind::Unreachable);
}

#[test]
fn rand_with_seed_zero() {
    let program = compile(
        r#"static mut DRAW: Int<0, 9> = 0;

#[scene]
#[entry]
pushdown fn Draw() {
    DRAW = rng_choose::<Int<0, 9>>();
    s!(t!["d"]);
}

fn keep() {}
"#,
    );
    let draw = func(&program, "crate::Draw");
    let draw_static = static_id(&program, "crate::DRAW");
    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber = StoryFiber::start(&vm, draw, vec![], StoryStatics::fresh(&program), 0).unwrap();
    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (draw, 0)
    );
    // 0x6F68E1E7E2646EE1 mod 10 = 7; t = 2^64 mod 10 = 6, and r ≥ t.
    assert_eq!(int(&read_static(&fiber, draw_static)), 7);
}
