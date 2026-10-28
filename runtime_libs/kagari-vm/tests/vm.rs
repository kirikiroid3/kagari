#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::rc::Rc;

use common::{HandFn, TestHost, compile, func, hand, int};
use kagari_bytecode::{CmpOp, ImportId, Instr, IntBinOp, IntType, IntTypeId, IntUnOp, Label};
use kagari_vm::{AdtTy, Decimal, FuncId, Rat, Text, TrapKind, Value, Vm};

fn ints(values: &[i128]) -> Vec<Value> {
    values.iter().map(|&v| Value::Int(v)).collect()
}

#[test]
fn overflow_on_a_bounded_int() {
    let int_0_3 = IntType {
        lo: 0,
        hi: 3,
        bits: 2,
        signed: false,
    };
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
                ty: IntTypeId(1),
            },
            Instr::Ret { src: 2 },
        ],
    );
    let program = hand(&[int_0_3], &[], vec![add], 16);
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    let trap = vm.call(&mut host, FuncId(0), ints(&[3, 1])).unwrap_err();
    assert_eq!(trap.kind, TrapKind::Overflow);
    assert_eq!(trap.backtrace, vec![(FuncId(0), 0)]);
    let sum = vm.call(&mut host, FuncId(0), ints(&[1, 2])).unwrap();
    assert_eq!(int(&sum), 3);
}

#[test]
fn shifts_and_not_on_u8() {
    let u8_ty = IntType {
        lo: 0,
        hi: 255,
        bits: 8,
        signed: false,
    };
    let shl = HandFn::new(
        "t::shl",
        2,
        3,
        vec![
            Instr::IntBin {
                op: IntBinOp::Shl,
                dst: 2,
                lhs: 0,
                rhs: 1,
                ty: IntTypeId(1),
            },
            Instr::Ret { src: 2 },
        ],
    );
    let not = HandFn::new(
        "t::not",
        1,
        2,
        vec![
            Instr::IntUn {
                op: IntUnOp::Not,
                dst: 1,
                src: 0,
                ty: IntTypeId(1),
            },
            Instr::Ret { src: 1 },
        ],
    );
    let program = hand(&[u8_ty], &[], vec![shl, not], 16);
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    assert_eq!(
        int(&vm.call(&mut host, FuncId(0), ints(&[3, 7])).unwrap()),
        128
    );
    let trap = vm.call(&mut host, FuncId(0), ints(&[1, 8])).unwrap_err();
    assert_eq!(trap.kind, TrapKind::Overflow);
    assert_eq!(
        int(&vm.call(&mut host, FuncId(1), ints(&[0])).unwrap()),
        255
    );
}

#[test]
fn u8_overflow_compiled() {
    let program =
        compile("fn add_u8(a: u8, b: u8) -> u8 { a + b }\nfn keep() { let _ = add_u8(1, 2); }");
    let add = func(&program, "crate::add_u8");
    let mut vm = Vm::new(program.clone());
    let mut host = TestHost::default();
    let trap = vm.call(&mut host, add, ints(&[200, 100])).unwrap_err();
    assert_eq!(trap.kind, TrapKind::Overflow);
    let rendered = trap.render(&program);
    assert!(
        rendered
            .starts_with("vm panic: integer overflow or result out of range at crate::add_u8 ("),
        "{rendered}"
    );
    assert!(rendered.contains("main.kix@"), "{rendered}");
    assert_eq!(
        int(&vm.call(&mut host, add, ints(&[200, 55])).unwrap()),
        255
    );
}

#[test]
fn compound_assignment_saturates() {
    let program = compile(
        "fn up(x: Int<0, 5>) -> Int<0, 5> { let mut n = x; n += 3; n }\n\
         fn down(x: Int<0, 5>) -> Int<0, 5> { let mut n = x; n -= 9; n }\n\
         fn keep() { let _ = (up(1), down(1)); }",
    );
    let (up, down) = (func(&program, "crate::up"), func(&program, "crate::down"));
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    assert_eq!(int(&vm.call(&mut host, up, ints(&[4])).unwrap()), 5);
    assert_eq!(int(&vm.call(&mut host, up, ints(&[1])).unwrap()), 4);
    assert_eq!(int(&vm.call(&mut host, down, ints(&[4])).unwrap()), 0);
}

fn dec(mantissa: i128, scale: u32) -> Value {
    Value::Decimal(Decimal::new(mantissa, scale))
}

#[test]
fn decimal_division() {
    let program = compile(
        "fn ddiv(a: Decimal, b: Decimal) -> Decimal { a / b }\nfn keep() { let _ = ddiv(1.0, 2.0); }",
    );
    let ddiv = func(&program, "crate::ddiv");
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    let third = vm
        .call(&mut host, ddiv, vec![dec(1, 0), dec(3, 0)])
        .unwrap();
    assert!(matches!(third, Value::Decimal(d) if d == Decimal::new(3_333_333, 7)));
    let neg = vm
        .call(&mut host, ddiv, vec![dec(-2, 0), dec(3, 0)])
        .unwrap();
    assert!(matches!(neg, Value::Decimal(d) if d == Decimal::new(-6_666_666, 7)));
    let trap = vm
        .call(&mut host, ddiv, vec![dec(1, 0), dec(0, 0)])
        .unwrap_err();
    assert_eq!(trap.kind, TrapKind::DivisionByZero);
}

fn plain(value: &Value) -> &str {
    match value {
        Value::Text(Text::Plain(s)) => s,
        other => panic!("expected plain text, found {other:?}"),
    }
}

#[test]
fn to_text_of_numbers() {
    let program = compile(
        "use std::narrative::{Text, text::to_text};\n\
         fn td(x: Decimal) -> Text { to_text(x) }\n\
         fn tr(r: Rat) -> Text { to_text(r) }\n\
         fn keep() { let _ = (td(1.0), tr(Rat::new(1, 2))); }",
    );
    let (td, tr) = (func(&program, "crate::td"), func(&program, "crate::tr"));
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    assert_eq!(
        plain(&vm.call(&mut host, td, vec![dec(3, 0)]).unwrap()),
        "3.0"
    );
    let r = Value::Rat(Rat::new(-3, 4).unwrap());
    assert_eq!(plain(&vm.call(&mut host, tr, vec![r]).unwrap()), "-3/4");
}

#[test]
fn closures_capture_by_value() {
    let program = compile(
        "fn capture() -> i32 { let mut n: i32 = 1; let r = &mut n; let f = || n; *r = 5; f() + n }\n\
         fn keep() { let _ = capture(); }",
    );
    let capture = func(&program, "crate::capture");
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    assert_eq!(int(&vm.call(&mut host, capture, vec![]).unwrap()), 6);
}

#[test]
fn a_vec_through_a_reference_does_not_alias_a_copy() {
    let program = compile(
        "fn push_via(v: &mut Vec<i32>) { v.push(3); }\n\
         fn vec_cow() -> (usize, usize) { let mut xs: Vec<i32> = Vec::new(); xs.push(1); let copy = xs; push_via(&mut xs); (xs.len(), copy.len()) }\n\
         fn keep() { let _ = vec_cow(); }",
    );
    let vec_cow = func(&program, "crate::vec_cow");
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    match vm.call(&mut host, vec_cow, vec![]).unwrap() {
        Value::Tuple(items) => {
            assert_eq!(items.iter().map(int).collect::<Vec<_>>(), vec![2, 1]);
        }
        other => panic!("expected a tuple, found {other:?}"),
    }
}

#[test]
fn call_depth_compiled() {
    let program = compile(
        "fn deep(n: u32) -> u32 { if n == 0 { 0 } else { deep(n - 1) + 1 } }\n\
         fn keep() { let _ = deep(1); }",
    );
    let deep = func(&program, "crate::deep");
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    assert_eq!(int(&vm.call(&mut host, deep, ints(&[2047])).unwrap()), 2047);
    let trap = vm.call(&mut host, deep, ints(&[2048])).unwrap_err();
    assert_eq!(trap.kind, TrapKind::CallDepth);
    assert_eq!(int(&vm.call(&mut host, deep, ints(&[1])).unwrap()), 1);
    // No depth leaked: the deepest call that fits still fits.
    assert_eq!(int(&vm.call(&mut host, deep, ints(&[2047])).unwrap()), 2047);
}

/// f0 imports `t::reenter`, which calls f1 through the VM; f1 → f2 → f3.
fn reentrant(max_call_depth: u32) -> Result<Value, kagari_vm::Trap> {
    let call = |func: u32| Instr::Call {
        dst: 0,
        args: 0,
        argc: 0,
        func: FuncId(func),
    };
    let functions = vec![
        HandFn::new(
            "t::f0",
            0,
            1,
            vec![
                Instr::CallImp {
                    dst: 0,
                    args: 0,
                    argc: 0,
                    import: ImportId(0),
                },
                Instr::Ret { src: 0 },
            ],
        ),
        HandFn::new("t::f1", 0, 1, vec![call(2), Instr::Ret { src: 0 }]),
        HandFn::new("t::f2", 0, 1, vec![call(3), Instr::Ret { src: 0 }]),
        HandFn::new(
            "t::f3",
            0,
            1,
            vec![Instr::Int { dst: 0, value: 1 }, Instr::Ret { src: 0 }],
        ),
    ];
    let program = hand(&[], &[("t::reenter", 0)], functions, max_call_depth);
    let mut vm = Vm::new(program);
    let mut host = TestHost {
        reenter_target: Some(FuncId(1)),
        ..TestHost::default()
    };
    vm.call(&mut host, FuncId(0), vec![])
}

#[test]
fn depth_counts_across_nested_calls() {
    let trap = reentrant(3).unwrap_err();
    assert_eq!(trap.kind, TrapKind::CallDepth);
    assert_eq!(
        trap.backtrace,
        vec![(FuncId(2), 0), (FuncId(1), 0), (FuncId(0), 0)]
    );
    assert_eq!(int(&reentrant(4).unwrap()), 1);
}

#[test]
fn fuel_runs_out_per_run() {
    let functions = vec![
        HandFn::new("t::spin", 0, 1, vec![Instr::Jmp { target: Label(0) }]),
        HandFn::new(
            "t::one",
            0,
            1,
            vec![Instr::Int { dst: 0, value: 1 }, Instr::Ret { src: 0 }],
        ),
    ];
    let program = hand(&[], &[], functions, 16);
    let mut vm = Vm::new(program);
    vm.set_fuel_per_run(1000);
    let mut host = TestHost::default();
    let trap = vm.call(&mut host, FuncId(0), vec![]).unwrap_err();
    assert_eq!(trap.kind, TrapKind::FuelExhausted);
    assert_eq!(int(&vm.call(&mut host, FuncId(1), vec![]).unwrap()), 1);
}

/// f0 burns `before` instructions, imports `t::fresh` (three runs of the 61-instruction f1),
/// then burns `after`; the budget per run is 100.
fn fresh_budget(before: usize, after: usize) -> Result<Value, kagari_vm::Trap> {
    let burn = |n: usize| std::iter::repeat_n(Instr::Int { dst: 0, value: 1 }, n);
    let mut f0: Vec<Instr> = burn(before).collect();
    f0.push(Instr::CallImp {
        dst: 0,
        args: 0,
        argc: 0,
        import: ImportId(0),
    });
    f0.extend(burn(after));
    f0.push(Instr::Ret { src: 0 });
    let mut f1: Vec<Instr> = burn(60).collect();
    f1.push(Instr::Ret { src: 0 });
    let functions = vec![
        HandFn::new("t::f0", 0, 1, f0),
        HandFn::new("t::f1", 0, 1, f1),
    ];
    let program = hand(&[], &[("t::fresh", 0)], functions, 16);
    let mut vm = Vm::new(program);
    vm.set_fuel_per_run(100);
    let mut host = TestHost {
        reenter_target: Some(FuncId(1)),
        fresh_runs: 3,
        ..TestHost::default()
    };
    vm.call(&mut host, FuncId(0), vec![])
}

#[test]
fn fresh_budget_runs_on_their_own_fuel() {
    // The three nested runs need 183 instructions together: each gets its own 100.
    assert!(fresh_budget(10, 10).is_ok());
    // The caller's fuel is restored, not refilled: 40 + 1 + 60 exceeds its 100.
    let trap = fresh_budget(40, 60).unwrap_err();
    assert_eq!(trap.kind, TrapKind::FuelExhausted);
}

#[test]
fn closures_cannot_be_compared() {
    let mut body = HandFn::new(
        "t::closure",
        0,
        1,
        vec![Instr::Int { dst: 0, value: 0 }, Instr::Ret { src: 0 }],
    );
    body.closure = true;
    let compare = HandFn::new(
        "t::compare",
        0,
        3,
        vec![
            Instr::Closure {
                dst: 0,
                captures: 0,
                count: 0,
                func: FuncId(0),
            },
            Instr::Move { dst: 1, src: 0 },
            Instr::Cmp {
                op: CmpOp::Eq,
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            Instr::Ret { src: 2 },
        ],
    );
    let program = hand(&[], &[], vec![body, compare], 16);
    let mut vm = Vm::new(program);
    let mut host = TestHost::default();
    let trap = vm.call(&mut host, FuncId(1), vec![]).unwrap_err();
    assert_eq!(trap.kind, TrapKind::ClosureEquality);
}

#[test]
fn type_table_resolves_module_and_synthetic_types() {
    let program = compile("fn keep() {}");
    let option = program
        .find_type("std::Option")
        .expect("std::Option interned");
    let mut vm = Vm::new(Rc::clone(&program));
    let types = vm.types_mut();
    assert_eq!(types.resolve("std::Option", &[]), AdtTy::Module(option));
    assert_eq!(types.resolve("x::Y", &["A", "B"]), AdtTy::Synthetic(0));
    assert_eq!(types.resolve("x::Y", &["A", "B"]), AdtTy::Synthetic(0));
    assert_eq!(types.variant_name(AdtTy::Synthetic(0), 1), "B");
    assert_eq!(types.path(AdtTy::Synthetic(0)), "x::Y");
}
