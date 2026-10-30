#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Debugger introspection: fiber frames, the draw log, the instruction hook and the program's
//! code-position helpers.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{HandFn, TestHost, compile, func, hand, int};
use kagari_bytecode::{Instr, IntBinOp, IntTypeId};
use kagari_vm::{
    FnKind, FuncId, HookStop, InstrHook, Program, StaticId, StoryFiber, StoryStatics, Suspend,
    Value, Vm,
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

fn instr(program: &Program, f: FuncId, pc: u32) -> Instr {
    program.module().functions[f.0 as usize].code[pc as usize]
}

/// The instruction index of the first STEP of `f`.
fn first_step(program: &Program, f: FuncId) -> u32 {
    let code = &program.module().functions[f.0 as usize].code;
    let at = code
        .iter()
        .position(|i| matches!(i, Instr::Step { .. }))
        .expect("a STEP");
    u32::try_from(at).unwrap()
}

fn expect_step(suspend: Suspend) -> (FuncId, u32) {
    match suspend {
        Suspend::Step { step, .. } => (step.func, step.id),
        Suspend::Returned(value) => panic!("expected a step, the scene returned {value:?}"),
    }
}

const NESTED: &str = r#"static mut GOT: Int<0, 9> = 0;

#[scene]
#[entry]
pushdown fn Outer() {
    s!(t!["one"]);
    GOT = 4;
    Inner(2);
    s!(t!["three"]);
}

#[scene]
pushdown fn Inner(n: Int<0, 9>) {
    s!(t!["two"]);
    GOT = n;
}

fn keep() {}
"#;

#[test]
fn frames_of_a_suspended_fiber() {
    let program = compile(NESTED);
    let outer = func(&program, "crate::Outer");
    let inner = func(&program, "crate::Inner");
    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber =
        StoryFiber::start(&vm, outer, vec![], StoryStatics::fresh(&program), 0).unwrap();

    let fresh = fiber.frames();
    assert_eq!(fresh.len(), 1);
    assert_eq!((fresh[0].func, fresh[0].pc), (outer, 0));

    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (outer, 0)
    );
    let frames = fiber.frames();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].func, outer);
    assert!(matches!(
        instr(&program, outer, frames[0].pc),
        Instr::Step { id: 0, .. }
    ));

    assert_eq!(
        expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap()),
        (inner, 0)
    );
    let frames = fiber.frames();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].func, outer);
    assert!(
        matches!(instr(&program, outer, frames[0].pc), Instr::Call { func, .. } if func == inner)
    );
    assert_eq!(frames[1].func, inner);
    assert_eq!(frames[1].pc, first_step(&program, inner));
    assert_eq!(int(&frames[1].regs[0]), 2);

    expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap());
    assert!(matches!(
        fiber.resume(&mut vm, &mut host, Value::unit()).unwrap(),
        Suspend::Returned(_)
    ));
    assert!(fiber.frames().is_empty());
}

#[test]
fn draws_of_the_last_resume() {
    let program = compile(
        r#"static mut A: Int<0, 9> = 0;
static mut B: Int<0, 9> = 0;

#[scene]
#[entry]
pushdown fn Draw() {
    A = rng_choose::<Int<0, 9>>();
    B = rng_choose::<Int<0, 9>>();
    s!(t!["d"]);
    s!(t!["e"]);
}

fn keep() {}
"#,
    );
    let draw = func(&program, "crate::Draw");
    let (a, b) = (
        static_id(&program, "crate::A"),
        static_id(&program, "crate::B"),
    );
    let mut vm = Vm::new(Rc::clone(&program));
    let mut host = TestHost::default();
    let mut fiber = StoryFiber::start(&vm, draw, vec![], StoryStatics::fresh(&program), 0).unwrap();
    assert!(fiber.draws().is_empty());
    expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap());
    let read = |id| int(&fiber.statics().cell(id).unwrap().borrow());
    let draws: Vec<i128> = fiber.draws().iter().map(int).collect();
    // Seed 0: the first draw is 7 (see `rand_with_seed_zero`).
    assert_eq!(draws, [read(a), read(b)]);
    assert_eq!(draws[0], 7);
    expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap());
    assert!(fiber.draws().is_empty());
}

/// What a recording hook saw at one stop: per frame, the function, pc and the int registers.
#[derive(Debug, PartialEq)]
struct Seen {
    frames: Vec<(FuncId, u32, Vec<Option<i128>>)>,
    got: Option<i128>,
    depth: u32,
}

/// Stops at `at`, recording what it sees; counts every check.
struct Recorder {
    at: Option<(FuncId, u32)>,
    got: Option<StaticId>,
    checks: Rc<RefCell<u64>>,
    seen: Rc<RefCell<Vec<Seen>>>,
}

impl InstrHook for Recorder {
    fn check(&mut self, func: FuncId, pc: u32, _depth: u32) -> bool {
        *self.checks.borrow_mut() += 1;
        self.at == Some((func, pc))
    }

    fn stop(&mut self, stop: &HookStop<'_>) {
        let got = match (stop.statics, self.got) {
            (Some(statics), Some(id)) => Some(int(&statics.cell(id).unwrap().borrow())),
            _ => None,
        };
        let top = stop.frames.last().unwrap();
        assert_eq!(Some((top.func, top.pc)), self.at);
        assert_ne!(stop.program.function_path(top.func), "<unknown function>");
        self.seen.borrow_mut().push(Seen {
            frames: stop
                .frames
                .iter()
                .map(|f| {
                    let ints = f.regs.iter().map(|v| match v {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    });
                    (f.func, f.pc, ints.collect())
                })
                .collect(),
            got,
            depth: stop.depth,
        });
    }
}

#[test]
fn hook_stops_at_an_instruction() {
    let program = compile(NESTED);
    let outer = func(&program, "crate::Outer");
    let inner = func(&program, "crate::Inner");
    let got = static_id(&program, "crate::GOT");
    let step = first_step(&program, inner);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut vm = Vm::new(Rc::clone(&program));
    assert!(
        vm.set_hook(Some(Box::new(Recorder {
            at: Some((inner, step)),
            got: Some(got),
            checks: Rc::default(),
            seen: Rc::clone(&seen),
        })))
        .is_none()
    );
    let mut host = TestHost::default();
    let mut fiber =
        StoryFiber::start(&vm, outer, vec![], StoryStatics::fresh(&program), 0).unwrap();
    expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap());
    assert!(seen.borrow().is_empty());
    expect_step(fiber.resume(&mut vm, &mut host, Value::unit()).unwrap());
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1);
    let stop = &seen[0];
    assert_eq!(stop.got, Some(4));
    assert_eq!(stop.depth, 2);
    assert_eq!(stop.frames.len(), 2);
    let (f0, pc0, _) = &stop.frames[0];
    assert_eq!(*f0, outer);
    assert!(matches!(instr(&program, outer, *pc0), Instr::Call { func, .. } if func == inner));
    let (f1, pc1, regs) = &stop.frames[1];
    assert_eq!((*f1, *pc1), (inner, step));
    assert_eq!(regs[0], Some(2));
    // The stop saw what the suspended fiber shows.
    let frames = fiber.frames();
    assert_eq!(
        frames.iter().map(|f| (f.func, f.pc)).collect::<Vec<_>>(),
        [(outer, *pc0), (inner, step)]
    );
    assert!(vm.set_hook(None).is_some());
}

/// Runs `NESTED` to its end: the step ids and the final `GOT`.
fn run_nested(vm: &mut Vm, program: &Program) -> (Vec<(FuncId, u32)>, i128) {
    let outer = func(program, "crate::Outer");
    let got = static_id(program, "crate::GOT");
    let mut host = TestHost::default();
    let mut fiber = StoryFiber::start(vm, outer, vec![], StoryStatics::fresh(program), 0).unwrap();
    let mut steps = Vec::new();
    while let Suspend::Step { step, .. } = fiber.resume(vm, &mut host, Value::unit()).unwrap() {
        steps.push((step.func, step.id));
    }
    let value = int(&fiber.statics().cell(got).unwrap().borrow());
    (steps, value)
}

#[test]
fn a_hook_that_never_stops_changes_nothing() {
    let program = compile(NESTED);
    let plain = run_nested(&mut Vm::new(Rc::clone(&program)), &program);
    let checks = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut vm = Vm::new(Rc::clone(&program));
    vm.set_hook(Some(Box::new(Recorder {
        at: None,
        got: None,
        checks: Rc::clone(&checks),
        seen: Rc::clone(&seen),
    })));
    let hooked = run_nested(&mut vm, &program);
    assert_eq!(hooked, plain);
    assert_eq!(plain.1, 2);
    assert!(*checks.borrow() > 0);
    assert!(seen.borrow().is_empty());
}

#[test]
fn hook_in_a_plain_call() {
    let add = HandFn::new(
        "t::add",
        2,
        3,
        vec![
            Instr::IntBin {
                op: IntBinOp::Add,
                dst: 2,
                lhs: 0,
                rhs: 1,
                ty: IntTypeId(0),
            },
            Instr::Ret { src: 2 },
        ],
    );
    let program = hand(&[], &[], vec![add], 16);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut vm = Vm::new(program);
    vm.set_hook(Some(Box::new(Recorder {
        at: Some((FuncId(0), 1)),
        got: None,
        checks: Rc::default(),
        seen: Rc::clone(&seen),
    })));
    let mut host = TestHost::default();
    let sum = vm
        .call(&mut host, FuncId(0), vec![Value::Int(1), Value::Int(2)])
        .unwrap();
    assert_eq!(int(&sum), 3);
    assert_eq!(
        *seen.borrow(),
        [Seen {
            frames: vec![(FuncId(0), 1, vec![Some(1), Some(2), Some(3)])],
            got: None,
            depth: 1,
        }]
    );
}

#[test]
fn code_positions() {
    let program = compile(NESTED);
    let inner = func(&program, "crate::Inner");
    assert_eq!(program.kind(inner), Some(FnKind::Scene));
    assert_eq!(program.params(inner), Some(1));
    assert_eq!(program.kind(FuncId(u32::MAX)), None);
    let code = &program.module().functions[inner.0 as usize].code;
    let mut word = 0;
    for (index, ins) in code.iter().enumerate() {
        let index = u32::try_from(index).unwrap();
        assert_eq!(program.word_of_instr(inner, index), Some(word));
        assert_eq!(program.instr_at_word(inner, word), Some(index));
        if ins.words() == 2 {
            assert_eq!(program.instr_at_word(inner, word + 1), None);
        }
        word += ins.words();
    }
    let len = u32::try_from(code.len()).unwrap();
    assert_eq!(program.word_of_instr(inner, len), None);
    assert_eq!(program.instr_at_word(inner, word), None);
}
