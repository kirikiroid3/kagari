#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::arithmetic_side_effects)]

use std::collections::BTreeSet;

use crate::*;

fn s(i: u32) -> StrId {
    StrId(i)
}

fn c(i: u32) -> ConstId {
    ConstId(i)
}

const U8: IntType = IntType {
    lo: 0,
    hi: 255,
    bits: 8,
    signed: false,
};

const SMALL: IntType = IntType {
    lo: 0,
    hi: 5,
    bits: 128,
    signed: true,
};

fn function(path: u32, kind: FnKind, params: u16, regs: u16, code: Vec<Instr>) -> Function {
    Function {
        path: s(path),
        kind,
        flags: FnFlags::default(),
        params,
        regs,
        code,
    }
}

/// `main`'s code: every opcode a `fn` may use, with jumps forward and backward.
fn main_code() -> Vec<Instr> {
    let mut code = vec![
        Instr::Move { dst: 0, src: 1 },
        Instr::Const {
            dst: 0,
            value: c(14),
        },
        Instr::Bool {
            dst: 1,
            value: true,
        },
        Instr::Int { dst: 2, value: -7 },
        Instr::Func {
            dst: 3,
            func: FuncId(2),
        },
        Instr::Clear { reg: 3 },
    ];
    for op in [
        IntBinOp::Add,
        IntBinOp::Sub,
        IntBinOp::Mul,
        IntBinOp::Div,
        IntBinOp::Rem,
        IntBinOp::And,
        IntBinOp::Or,
        IntBinOp::Xor,
        IntBinOp::Shl,
        IntBinOp::Shr,
    ] {
        code.push(Instr::IntBin {
            op,
            dst: 0,
            lhs: 1,
            rhs: 2,
            ty: IntTypeId(1),
        });
    }
    for op in [IntUnOp::Neg, IntUnOp::Not, IntUnOp::Clamp] {
        code.push(Instr::IntUn {
            op,
            dst: 0,
            src: 1,
            ty: IntTypeId(2),
        });
    }
    code.push(Instr::Not { dst: 0, src: 1 });
    for op in [BoolBinOp::And, BoolBinOp::Or, BoolBinOp::Xor] {
        code.push(Instr::BoolBin {
            op,
            dst: 0,
            lhs: 1,
            rhs: 2,
        });
    }
    for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div] {
        code.push(Instr::DecBin {
            op,
            dst: 0,
            lhs: 1,
            rhs: 2,
        });
        code.push(Instr::RatBin {
            op,
            dst: 0,
            lhs: 1,
            rhs: 2,
        });
    }
    code.push(Instr::DecNeg { dst: 0, src: 1 });
    code.push(Instr::IntDec { dst: 0, src: 1 });
    code.push(Instr::RatNeg { dst: 0, src: 1 });
    code.push(Instr::RatNew {
        dst: 0,
        num: 1,
        den: 2,
    });
    code.push(Instr::RatDec { dst: 0, src: 1 });
    for op in [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Lt,
        CmpOp::Le,
        CmpOp::Gt,
        CmpOp::Ge,
    ] {
        code.push(Instr::Cmp {
            op,
            dst: 0,
            lhs: 1,
            rhs: 2,
        });
    }
    let back = Label(3);
    let here = u32::try_from(code.len()).unwrap();
    code.extend([
        // A forward jump over the UNREACHABLE.
        Instr::Jmp {
            target: Label(here + 2),
        },
        Instr::Unreachable,
        Instr::JmpIf {
            cond: 1,
            target: back,
        },
        Instr::JmpNot {
            cond: 1,
            target: Label(here),
        },
        Instr::Call {
            dst: 0,
            args: 1,
            argc: 1,
            func: FuncId(2),
        },
        Instr::CallImp {
            dst: 0,
            args: 1,
            argc: 2,
            import: ImportId(0),
        },
        Instr::CallVal {
            dst: 0,
            args: 1,
            argc: 1,
            callee: 3,
            kind: FnKind::Fn,
        },
        Instr::Closure {
            dst: 4,
            captures: 1,
            count: 2,
            func: FuncId(3),
        },
        Instr::Tuple {
            dst: 0,
            first: 1,
            count: 3,
        },
        Instr::Array {
            dst: 0,
            first: 1,
            count: 0,
        },
        Instr::Repeat {
            dst: 0,
            value: 1,
            count: 3,
        },
        Instr::Adt {
            dst: 0,
            first: 1,
            count: 2,
            ty: TypeId(2),
            variant: 0,
        },
        Instr::Field {
            dst: 0,
            src: 1,
            index: 1,
        },
        Instr::IsVariant {
            dst: 0,
            src: 1,
            ty: TypeId(1),
            variant: 1,
        },
        Instr::Index {
            dst: 0,
            base: 1,
            index: 2,
        },
        Instr::Len { dst: 0, src: 1 },
        Instr::Cell { dst: 5, init: 1 },
        Instr::RefField {
            dst: 6,
            base: 5,
            index: 0,
        },
        Instr::RefIndex {
            dst: 6,
            base: 5,
            index: 2,
        },
        Instr::Load { dst: 0, src: 6 },
        Instr::Store { dst: 6, value: 0 },
        Instr::VecNew { dst: 7 },
        Instr::VecGet {
            dst: 0,
            vec: 7,
            index: 2,
            option: TypeId(1),
        },
        Instr::VecSet {
            vec: 5,
            index: 2,
            value: 0,
        },
        Instr::VecPush { vec: 5, value: 0 },
        Instr::Ruby {
            dst: 0,
            base: 1,
            reading: 2,
        },
        Instr::MonoRuby {
            dst: 0,
            base: 1,
            readings: 2,
        },
        Instr::Effect {
            dst: 0,
            base: 1,
            effect: 2,
        },
        Instr::Concat { dst: 0, parts: 1 },
        Instr::ToText { dst: 0, src: 1 },
        Instr::Ret { src: 0 },
    ]);
    code
}

/// A valid module that uses every section, every constant tag and every opcode.
fn full() -> Module {
    let strings = [
        "crate::main",                 // 0
        "crate::Start",                // 1
        "crate::Coin",                 // 2
        "Heads",                       // 3
        "Tails",                       // 4
        "std::option::Option",         // 5
        "None",                        // 6
        "Some",                        // 7
        "crate::Point",                // 8
        "x",                           // 9
        "y",                           // 10
        "std::narrative::Meta::write", // 11
        "bg.png",                      // 12
        "Hello",                       // 13
        "かな",                        // 14
        "crate::SEEN",                 // 15
        "crate::VOLUME",               // 16
        "crate::helper",               // 17
        "crate::main::{closure#0}",    // 18
        "crate::instant",              // 19
        "main.kis",                    // 20
        "ext.feature",                 // 21
        "crate::calc",                 // 22
    ];
    let mut main = function(0, FnKind::Fn, 0, 8, main_code());
    main.flags.main = true;
    let mut start = function(
        1,
        FnKind::Scene,
        0,
        2,
        vec![
            Instr::Step {
                dst: 0,
                record: 1,
                id: 7,
            },
            Instr::Rand {
                dst: 0,
                domain: RandDomain::Enum(TypeId(0)),
            },
            Instr::Rand {
                dst: 1,
                domain: RandDomain::Int(IntTypeId(2)),
            },
            Instr::RefStatic {
                dst: 1,
                var: StaticId(0),
            },
            Instr::Call {
                dst: 0,
                args: 0,
                argc: 0,
                func: FuncId(4),
            },
            Instr::Call {
                dst: 0,
                args: 0,
                argc: 0,
                func: FuncId(5),
            },
            Instr::Ret { src: 0 },
        ],
    );
    start.flags.entry = true;
    let mut closure = function(18, FnKind::Fn, 2, 2, vec![Instr::Ret { src: 0 }]);
    closure.flags.closure = true;
    let mut calc = function(
        22,
        FnKind::Const,
        0,
        1,
        vec![Instr::Int { dst: 0, value: 1 }, Instr::Ret { src: 0 }],
    );
    calc.flags.synthetic = true;
    Module {
        strings: strings.iter().map(|s| (*s).to_owned()).collect(),
        features: vec![Feature {
            name: s(21),
            required: true,
        }],
        types: vec![
            AdtType {
                path: s(2),
                is_enum: true,
                variants: vec![
                    Variant {
                        name: s(3),
                        fields: vec![],
                    },
                    Variant {
                        name: s(4),
                        fields: vec![],
                    },
                ],
            },
            AdtType {
                path: s(5),
                is_enum: true,
                variants: vec![
                    Variant {
                        name: s(6),
                        fields: vec![],
                    },
                    Variant {
                        name: s(7),
                        fields: vec![None],
                    },
                ],
            },
            AdtType {
                path: s(8),
                is_enum: false,
                variants: vec![Variant {
                    name: s(8),
                    fields: vec![Some(s(9)), Some(s(10))],
                }],
            },
        ],
        int_types: vec![IntType::I128, U8, SMALL],
        imports: vec![Import {
            path: s(11),
            kind: FnKind::Fn,
            params: 2,
        }],
        assets: vec![Asset {
            kind: AssetKind::Image,
            path: s(12),
        }],
        constants: vec![
            Constant::Bool(true),
            Constant::Int(-5),
            Constant::Decimal {
                mantissa: 5,
                scale: 1,
            },
            Constant::Rat { num: 1, den: 3 },
            Constant::String(s(13)),
            Constant::Text(s(13)),
            Constant::Text(s(14)),
            Constant::Ruby {
                base: c(5),
                reading: c(6),
            },
            Constant::MonoRuby {
                base: c(5),
                readings: vec![c(6), c(6), c(6)],
            },
            Constant::Effect {
                base: c(5),
                effect: c(0),
            },
            Constant::Concat(vec![c(5), c(7)]),
            Constant::Tuple(vec![]),
            Constant::Array(vec![c(1), c(1)]),
            Constant::Vec(vec![c(1)]),
            Constant::Adt {
                ty: TypeId(2),
                variant: 0,
                fields: vec![c(1), c(1)],
            },
            Constant::Asset(AssetId(0)),
            Constant::Function(FuncId(1)),
            Constant::Handle(StaticId(1)),
            Constant::Adt {
                ty: TypeId(0),
                variant: 1,
                fields: vec![],
            },
        ],
        statics: vec![
            Static {
                path: s(15),
                kind: StaticKind::StoryVariable,
                init: c(0),
            },
            Static {
                path: s(16),
                kind: StaticKind::Setting,
                init: c(1),
            },
        ],
        functions: vec![
            main,
            start,
            function(17, FnKind::Fn, 1, 1, vec![Instr::Ret { src: 0 }]),
            closure,
            function(
                19,
                FnKind::Instant,
                0,
                1,
                vec![
                    Instr::RefStatic {
                        dst: 0,
                        var: StaticId(0),
                    },
                    Instr::Ret { src: 0 },
                ],
            ),
            calc,
        ],
        options: Options {
            max_call_depth: 2048,
        },
        debug: Some(Debug {
            files: vec![s(20)],
            functions: vec![
                FnDebug {
                    file: Some(0),
                    lines: vec![(0, 0), (2, 10)],
                },
                FnDebug {
                    file: Some(0),
                    lines: vec![(0, 40)],
                },
                FnDebug::default(),
                FnDebug::default(),
                FnDebug::default(),
                FnDebug::default(),
            ],
        }),
    }
}

/// The smallest valid module: `fn main() { }`.
fn minimal() -> Module {
    let mut main = function(0, FnKind::Fn, 0, 1, vec![Instr::Ret { src: 0 }]);
    main.flags.main = true;
    Module {
        strings: vec!["crate::main".to_owned(), "crate::f".to_owned()],
        int_types: vec![IntType::I128],
        functions: vec![main],
        options: Options {
            max_call_depth: 2048,
        },
        ..Module::default()
    }
}

/// `(id, start, end)` of every section, `end` including padding.
fn sections(bytes: &[u8]) -> Vec<(u16, usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 16;
    while pos < bytes.len() {
        let id = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        let len = u64::from_le_bytes(bytes[pos + 8..pos + 16].try_into().unwrap()) as usize;
        let end = pos + 16 + len.div_ceil(8) * 8;
        out.push((id, pos, end));
        pos = end;
    }
    out
}

fn section_bytes(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(payload);
    while out.len() % 8 != 0 {
        out.push(0);
    }
    out
}

#[test]
fn full_module_uses_every_opcode_and_tag() {
    let m = full();
    let used: BTreeSet<u16> = m
        .functions
        .iter()
        .flat_map(|f| f.code.iter().map(Instr::opcode))
        .collect();
    let known: BTreeSet<u16> = (0..=u16::MAX)
        .filter(|&op| {
            !matches!(
                Instr::decode(&[u64::from(op), 0]),
                Err(InstrError::UnknownOpcode(_))
            )
        })
        .collect();
    assert_eq!(known.len(), 76);
    assert_eq!(used, known);
    let tags: BTreeSet<u32> = m.constants.iter().map(Constant::tag).collect();
    assert_eq!(tags, (0..=16).collect());
}

#[test]
fn step_id_is_a_u32_operand_and_0x70_is_retired() {
    let step = Instr::Step {
        dst: 1,
        record: 2,
        id: u32::MAX,
    };
    let (first, x) = step.encode(|label| label.0);
    let x = x.unwrap();
    assert_eq!(Instr::decode(&[first, x]), Ok((step, 2)));
    assert_eq!(Instr::decode(&[first, x + 1]), Err(InstrError::BadOperand));
    assert_eq!(
        Instr::decode(&[0x70, 0]),
        Err(InstrError::UnknownOpcode(0x70))
    );
}

#[test]
fn roundtrip() {
    let m = full();
    assert_eq!(verify(&m), Ok(()));
    let bytes = encode(&m);
    assert_eq!(&bytes[..16], b"\0\0\0\0KAGARI\0\0V000");
    assert_eq!(bytes.len() % 8, 0);
    let ids: Vec<u16> = sections(&bytes).iter().map(|s| s.0).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0x80]);
    assert_eq!(decode(&bytes), Ok(m.clone()));

    let mut bare = m;
    bare.debug = None;
    assert_eq!(decode(&encode(&bare)), Ok(bare));
    assert_eq!(decode(&encode(&minimal())), Ok(minimal()));
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = encode(&minimal());
    bytes[4] = b'X';
    assert_eq!(decode(&bytes), Err(DecodeError::BadMagic));
    assert_eq!(decode(b"PNG"), Err(DecodeError::BadMagic));
    assert_eq!(decode(&bytes[..0]), Err(DecodeError::Truncated));
}

#[test]
fn rejects_other_version() {
    let mut bytes = encode(&minimal());
    bytes[12..16].copy_from_slice(b"V001");
    assert_eq!(decode(&bytes), Err(DecodeError::UnsupportedVersion));
}

#[test]
fn rejects_unknown_critical_section() {
    let bytes = encode(&full());
    let (_, debug_start, _) = *sections(&bytes).last().unwrap();
    let mut patched = bytes[..debug_start].to_vec();
    patched.extend(section_bytes(0x0B, &[]));
    patched.extend_from_slice(&bytes[debug_start..]);
    assert_eq!(
        decode(&patched),
        Err(DecodeError::UnknownCriticalSection(0x0B))
    );
}

#[test]
fn rejects_sections_out_of_order() {
    let bytes = encode(&minimal());
    let secs = sections(&bytes);
    let (_, a0, a1) = secs[0];
    let (_, b0, b1) = secs[1];
    let mut patched = bytes[..a0].to_vec();
    patched.extend_from_slice(&bytes[b0..b1]);
    patched.extend_from_slice(&bytes[a0..a1]);
    patched.extend_from_slice(&bytes[b1..]);
    assert_eq!(decode(&patched), Err(DecodeError::SectionOrder));
}

#[test]
fn rejects_missing_section() {
    let bytes = encode(&minimal());
    let secs = sections(&bytes);
    let (_, a0, a1) = secs[2];
    let mut patched = bytes[..a0].to_vec();
    patched.extend_from_slice(&bytes[a1..]);
    assert_eq!(decode(&patched), Err(DecodeError::MissingSection(0x03)));
}

#[test]
fn rejects_trailing_and_truncated() {
    let bytes = encode(&minimal());
    let mut long = bytes.clone();
    long.extend_from_slice(&[0; 3]);
    assert!(decode(&long).is_err());
    for len in 16..bytes.len() {
        assert!(decode(&bytes[..len]).is_err(), "prefix of {len} bytes");
    }
}

#[test]
fn rejects_jump_into_two_word_instruction() {
    let mut m = minimal();
    m.functions[0].code = vec![
        Instr::Int {
            dst: 0,
            value: 0x1234_5678,
        },
        Instr::Jmp { target: Label(0) },
    ];
    let mut bytes = encode(&m);
    let jmp: [u8; 16] = [0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let at = bytes.windows(16).position(|w| w == jmp).unwrap();
    // The target word 1 is the operand word of INT.
    bytes[at + 8] = 1;
    assert_eq!(decode(&bytes), Err(DecodeError::BadJumpTarget));
}

#[test]
fn skips_unknown_ancillary_section() {
    let m = full();
    let mut bytes = encode(&m);
    bytes.extend(section_bytes(0x81, &[1, 2, 3, 4, 5]));
    assert_eq!(decode(&bytes), Ok(m));
}

fn verify_kind(m: &Module) -> VerifyErrorKind {
    verify(m).unwrap_err().kind
}

#[test]
fn verify_rejects_register_out_of_range() {
    let mut m = minimal();
    m.functions[0].code = vec![Instr::Ret { src: 5 }];
    let error = verify(&m).unwrap_err();
    assert_eq!(
        error,
        VerifyError {
            function: Some(0),
            instr: Some(0),
            kind: VerifyErrorKind::RegisterOutOfRange { reg: 5, regs: 1 },
        }
    );
    assert_eq!(
        error.to_string(),
        "function 0, instruction 0: register r5 out of range (regs = 1)"
    );
}

#[test]
fn verify_rejects_fall_through() {
    let mut m = minimal();
    m.functions[0].code = vec![Instr::Int { dst: 0, value: 1 }];
    assert_eq!(verify_kind(&m), VerifyErrorKind::FallThrough);
}

#[test]
fn verify_rejects_wrong_argc() {
    let mut m = minimal();
    m.functions
        .push(function(1, FnKind::Fn, 1, 1, vec![Instr::Ret { src: 0 }]));
    m.functions[0].code = vec![
        Instr::Call {
            dst: 0,
            args: 0,
            argc: 0,
            func: FuncId(1),
        },
        Instr::Ret { src: 0 },
    ];
    assert_eq!(
        verify_kind(&m),
        VerifyErrorKind::ArgCount {
            expected: 1,
            found: 0
        }
    );
}

#[test]
fn verify_rejects_const_calling_fn() {
    let mut m = minimal();
    m.functions.push(function(
        1,
        FnKind::Const,
        0,
        1,
        vec![
            Instr::Call {
                dst: 0,
                args: 0,
                argc: 0,
                func: FuncId(0),
            },
            Instr::Ret { src: 0 },
        ],
    ));
    let error = verify(&m).unwrap_err();
    assert_eq!((error.function, error.instr), (Some(1), Some(0)));
    assert_eq!(
        error.kind,
        VerifyErrorKind::CallRule {
            caller: FnKind::Const,
            callee: FnKind::Fn
        }
    );
}

#[test]
fn verify_rejects_step_outside_scene() {
    let mut m = minimal();
    m.functions.push(function(
        1,
        FnKind::Instant,
        0,
        1,
        vec![
            Instr::Step {
                dst: 0,
                record: 0,
                id: 0,
            },
            Instr::Ret { src: 0 },
        ],
    ));
    assert_eq!(
        verify_kind(&m),
        VerifyErrorKind::SceneOnly { instr: "STEP" }
    );
}

#[test]
fn verify_rejects_refstat_in_fn() {
    let mut m = minimal();
    m.constants.push(Constant::Int(0));
    m.statics.push(Static {
        path: s(1),
        kind: StaticKind::StoryVariable,
        init: c(0),
    });
    m.functions[0].code = vec![
        Instr::RefStatic {
            dst: 0,
            var: StaticId(0),
        },
        Instr::Ret { src: 0 },
    ];
    assert_eq!(
        verify_kind(&m),
        VerifyErrorKind::StoryOnly { caller: FnKind::Fn }
    );
}

#[test]
fn verify_rejects_missing_main() {
    let mut m = minimal();
    m.functions[0].flags.main = false;
    assert_eq!(verify_kind(&m), VerifyErrorKind::MissingMain);
    // The code of a module without `main` is still checked.
    assert_eq!(verify_code(&m), Ok(()));
    m.functions[0].code = vec![Instr::Int { dst: 0, value: 1 }];
    assert_eq!(
        verify_code(&m).map_err(|error| error.kind),
        Err(VerifyErrorKind::FallThrough)
    );
}

#[test]
fn verify_rejects_forward_constant() {
    let mut m = minimal();
    m.constants = vec![Constant::Tuple(vec![c(1)]), Constant::Int(0)];
    assert_eq!(
        verify_kind(&m),
        VerifyErrorKind::ForwardConstant {
            constant: 0,
            child: 1
        }
    );
}

#[test]
fn disassembles() {
    let m = full();
    let text = disassemble(&m, &|path| path == "crate::Start" || path == "crate::main");
    let start = "\
fn crate::Start [scene, entry] params=0 regs=2
  0000 STEP r0, r1, crate::Start#7
  0001 RAND r0, crate::Coin
  0002 RAND r1, Int<0,5>
  0003 REFSTAT r1, crate::SEEN
  0004 CALL r0, (), crate::instant
  0005 CALL r0, (), crate::calc
  0006 RET r0
";
    assert!(text.contains(start), "{text}");
    for line in [
        "fn crate::main [fn, main] params=0 regs=8\n",
        "  0000 MOVE r0, r1\n",
        "  0001 CONST r0, #14 crate::Point {-5, -5}\n",
        "  0006 IADD r0, r1, r2, u8\n",
        "  0016 INEG r0, r1, Int<0,5>\n",
        "  0032 INTDEC r0, r1\n",
        "  0047 CALLIMP r0, (r1, r2), std::narrative::Meta::write\n",
        "  0049 CLOSURE r4, (r1, r2), crate::main::{closure#0}\n",
        "  0053 ADT r0, (r1, r2), crate::Point\n",
        "  0055 ISVAR r0, r1, std::option::Option::Some\n",
    ] {
        assert!(text.contains(line), "missing {line:?} in\n{text}");
    }
    assert!(text.ends_with(
        "import std::narrative::Meta::write [fn] params=2\n\
         static crate::SEEN [story] = true\n\
         options max_call_depth=2048\n"
    ));
}

#[test]
fn disassembles_constants() {
    let m = full();
    let mut with_consts = m.clone();
    let code: Vec<Instr> = (0..m.constants.len() as u32)
        .map(|i| Instr::Const {
            dst: 0,
            value: c(i),
        })
        .chain([Instr::Ret { src: 0 }])
        .collect();
    with_consts.functions[0].code = code;
    let text = disassemble(&with_consts, &|path| path == "crate::main");
    let expected = [
        "#0 true",
        "#1 -5",
        "#2 0.5",
        "#3 1/3",
        "#4 str \"Hello\"",
        "#5 \"Hello\"",
        "#6 \"かな\"",
        "#7 \"Hello\"",
        "#8 \"Hello\"",
        "#9 \"Hello\"",
        "#10 \"HelloHello\"",
        "#11 ()",
        "#12 [-5, -5]",
        "#13 vec[-5]",
        "#14 crate::Point {-5, -5}",
        "#15 image \"bg.png\"",
        "#16 crate::Start",
        "#17 crate::VOLUME",
        "#18 crate::Coin::Tails",
    ];
    for (i, value) in expected.iter().enumerate() {
        let line = format!("  {i:04} CONST r0, {value}\n");
        assert!(text.contains(&line), "missing {line:?} in\n{text}");
    }
    assert!(
        text.contains("static crate::VOLUME [setting] = -5\n"),
        "{text}"
    );
}

#[test]
fn decode_never_panics_on_corruption() {
    let bytes = encode(&full());
    for i in 16..bytes.len() {
        for flip in [0x01u8, 0x80, 0xFF] {
            let mut patched = bytes.clone();
            patched[i] ^= flip;
            if let Ok(m) = decode(&patched) {
                let _ = verify(&m);
                let _ = disassemble(&m, &|_| true);
            }
        }
    }
}
