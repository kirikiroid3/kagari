//! Instructions. A code word is a `u64`: `op: u16 | a: u16 << 16 | b: u16 << 32 | c: u16 << 48`.
//! A 2-word instruction is followed by an operand word `x`. `a`, `b` and `c` are registers
//! unless noted otherwise; value operands auto-deref (a reference is followed to the value at
//! the end of the chain), operands marked *raw* are taken as they are.

use crate::module::{ConstId, FnKind, FuncId, ImportId, IntTypeId, StaticId, TypeId};

/// A register, `r0..regs`.
pub type Reg = u16;

/// A jump target: an index into the function's instructions (not a code word offset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label(pub u32);

/// `I*` binary operators on integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

/// Unary operators on integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntUnOp {
    /// Checked negation.
    Neg,
    /// Bitwise not within `bits`/`signed`.
    Not,
    /// Clamp to `[lo, hi]`: saturation, never panics.
    Clamp,
}

/// Non-short-circuit `bool` operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoolBinOp {
    And,
    Or,
    Xor,
}

/// Arithmetic on `Decimal` and `Rat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Comparisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// What `RAND` draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandDomain {
    /// A variant of a fieldless enum, uniformly.
    Enum(TypeId),
    /// A value of `lo..=hi` of the int type, uniformly.
    Int(IntTypeId),
}

/// One instruction. Each variant (with its operator, for the operator families) is one
/// opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Instr {
    /// 0x01 `MOVE a=dst, b=src (raw)`.
    Move { dst: Reg, src: Reg },
    /// 0x02 `CONST a=dst, x=ConstId`.
    Const { dst: Reg, value: ConstId },
    /// 0x03 `BOOL a=dst, b=0/1`.
    Bool { dst: Reg, value: bool },
    /// 0x04 `INT a=dst, x=i64`, sign-extended to i128.
    Int { dst: Reg, value: i64 },
    /// 0x05 `FUNC a=dst, x=FuncId`: a function pointer.
    Func { dst: Reg, func: FuncId },
    /// 0x06 `CLEAR a`: the register becomes Uninit.
    Clear { reg: Reg },
    /// 0x10–0x19 `IADD … ISHR a=dst, b, c, x=IntTypeId`.
    IntBin {
        op: IntBinOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        ty: IntTypeId,
    },
    /// 0x1A–0x1C `INEG INOT ICLAMP a=dst, b, x=IntTypeId`.
    IntUn {
        op: IntUnOp,
        dst: Reg,
        src: Reg,
        ty: IntTypeId,
    },
    /// 0x20 `NOT a, b`.
    Not { dst: Reg, src: Reg },
    /// 0x21–0x23 `BAND BOR BXOR a, b, c`.
    BoolBin {
        op: BoolBinOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    /// 0x28–0x2B `DADD DSUB DMUL DDIV a, b, c`.
    DecBin {
        op: ArithOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    /// 0x2C `DNEG a, b`.
    DecNeg { dst: Reg, src: Reg },
    /// 0x30–0x33 `RADD RSUB RMUL RDIV a, b, c`.
    RatBin {
        op: ArithOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    /// 0x34 `RNEG a, b`.
    RatNeg { dst: Reg, src: Reg },
    /// 0x35 `RATNEW a, b=num, c=den`.
    RatNew { dst: Reg, num: Reg, den: Reg },
    /// 0x36 `RATDEC a, b`: Rat → Decimal, truncated to 7 digits.
    RatDec { dst: Reg, src: Reg },
    /// 0x40–0x45 `EQ NE LT LE GT GE a, b, c`.
    Cmp {
        op: CmpOp,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    /// 0x50 `JMP x=target`.
    Jmp { target: Label },
    /// 0x51 `JMPIF a=cond, x=target`.
    JmpIf { cond: Reg, target: Label },
    /// 0x52 `JMPNOT a=cond, x=target`.
    JmpNot { cond: Reg, target: Label },
    /// 0x53 `RET a (raw)`.
    Ret { src: Reg },
    /// 0x54 `UNREACHABLE`: panics.
    Unreachable,
    /// 0x60 `CALL a=dst, b=first arg, c=argc, x=FuncId`; args are raw registers `b..b+c`.
    Call {
        dst: Reg,
        args: Reg,
        argc: u16,
        func: FuncId,
    },
    /// 0x61 `CALLIMP a, b, c, x=ImportId`.
    CallImp {
        dst: Reg,
        args: Reg,
        argc: u16,
        import: ImportId,
    },
    /// 0x62 `CALLVAL a, b, c, x = callee reg (bits 0-15) | expected FnKind (bits 16-23)`.
    CallVal {
        dst: Reg,
        args: Reg,
        argc: u16,
        callee: Reg,
        kind: FnKind,
    },
    /// 0x63 `CLOSURE a=dst, b=first capture, c=count, x=FuncId`.
    Closure {
        dst: Reg,
        captures: Reg,
        count: u16,
        func: FuncId,
    },
    /// 0x71 `RAND a=dst, x = kind (bits 32-39) | index (bits 0-31)`.
    Rand { dst: Reg, domain: RandDomain },
    /// 0x72 `STEP a=dst, b=step record, x=step id`. The step id is the step's ordinal among
    /// the step forms of the function's item, in source order (0x70, a STEP without an id,
    /// is retired).
    Step { dst: Reg, record: Reg, id: u32 },
    /// 0x80 `TUPLE a, b=first, c=count`.
    Tuple { dst: Reg, first: Reg, count: u16 },
    /// 0x81 `ARRAY a, b, c`.
    Array { dst: Reg, first: Reg, count: u16 },
    /// 0x82 `REPEAT a, b=value, x=count`.
    Repeat { dst: Reg, value: Reg, count: u64 },
    /// 0x83 `ADT a, b, c=count, x = TypeId (bits 0-31) | variant (bits 32-63)`.
    Adt {
        dst: Reg,
        first: Reg,
        count: u16,
        ty: TypeId,
        variant: u32,
    },
    /// 0x84 `FIELD a, b, c=field index (immediate)`.
    Field { dst: Reg, src: Reg, index: u16 },
    /// 0x85 `ISVAR a, b, x = TypeId | variant << 32`.
    IsVariant {
        dst: Reg,
        src: Reg,
        ty: TypeId,
        variant: u32,
    },
    /// 0x86 `INDEX a, b=array or Vec, c=index`.
    Index { dst: Reg, base: Reg, index: Reg },
    /// 0x87 `LEN a, b`.
    Len { dst: Reg, src: Reg },
    /// 0x90 `CELL a=dst, b=init (raw)`: a new heap cell; `dst` is a reference to it.
    Cell { dst: Reg, init: Reg },
    /// 0x91 `REFSTAT a, x=StaticId`.
    RefStatic { dst: Reg, var: StaticId },
    /// 0x92 `REFFIELD a, b=ref, c=field (immediate)`.
    RefField { dst: Reg, base: Reg, index: u16 },
    /// 0x93 `REFINDEX a, b=ref, c=index reg`.
    RefIndex { dst: Reg, base: Reg, index: Reg },
    /// 0x94 `LOAD a, b=ref`.
    Load { dst: Reg, src: Reg },
    /// 0x95 `STORE a=ref, b=value (raw)`.
    Store { dst: Reg, value: Reg },
    /// 0xA0 `VECNEW a`.
    VecNew { dst: Reg },
    /// 0xA1 `VECGET a, b, c=index, x=TypeId of std::Option`.
    VecGet {
        dst: Reg,
        vec: Reg,
        index: Reg,
        option: TypeId,
    },
    /// 0xA2 `VECSET a=ref, b=index, c=value (raw)`.
    VecSet { vec: Reg, index: Reg, value: Reg },
    /// 0xA3 `VECPUSH a=ref, b=value (raw)`.
    VecPush { vec: Reg, value: Reg },
    /// 0xB0 `RUBY a, b=base, c=reading`.
    Ruby { dst: Reg, base: Reg, reading: Reg },
    /// 0xB1 `MRUBY a, b=base, c=array of readings`.
    MonoRuby { dst: Reg, base: Reg, readings: Reg },
    /// 0xB2 `EFFECT a, b=base, c=effect`.
    Effect { dst: Reg, base: Reg, effect: Reg },
    /// 0xB3 `CONCAT a, b=array of Text`.
    Concat { dst: Reg, parts: Reg },
    /// 0xB4 `TOTEXT a, b`.
    ToText { dst: Reg, src: Reg },
}

/// A malformed instruction while decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrError {
    UnknownOpcode(u16),
    /// An operand that is not valid for the opcode (a bool other than 0/1, an unknown
    /// `FnKind` or `RAND` kind, nonzero unused bits).
    BadOperand,
    /// The code ends inside a 2-word instruction.
    Truncated,
}

fn word(op: u16, a: u16, b: u16, c: u16) -> u64 {
    u64::from(op)
        | u64::from(a).wrapping_shl(16)
        | u64::from(b).wrapping_shl(32)
        | u64::from(c).wrapping_shl(48)
}

fn field(word: u64, index: u32) -> u16 {
    (word.wrapping_shr(index.wrapping_mul(16)) & 0xFFFF) as u16
}

fn lo32(x: u64) -> u32 {
    (x & 0xFFFF_FFFF) as u32
}

fn hi32(x: u64) -> u32 {
    x.wrapping_shr(32) as u32
}

fn pair(lo: u32, hi: u32) -> u64 {
    u64::from(lo) | u64::from(hi).wrapping_shl(32)
}

const INT_BIN: [IntBinOp; 10] = [
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
];
const INT_UN: [IntUnOp; 3] = [IntUnOp::Neg, IntUnOp::Not, IntUnOp::Clamp];
const BOOL_BIN: [BoolBinOp; 3] = [BoolBinOp::And, BoolBinOp::Or, BoolBinOp::Xor];
const ARITH: [ArithOp; 4] = [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div];
const CMP: [CmpOp; 6] = [
    CmpOp::Eq,
    CmpOp::Ne,
    CmpOp::Lt,
    CmpOp::Le,
    CmpOp::Gt,
    CmpOp::Ge,
];

fn position<T: PartialEq>(table: &[T], op: T) -> u16 {
    table
        .iter()
        .position(|entry| *entry == op)
        .and_then(|index| u16::try_from(index).ok())
        .unwrap_or(0)
}

fn op_in<T: Copy>(table: &[T], base: u16, op: u16) -> Option<T> {
    table.get(usize::from(op.checked_sub(base)?)).copied()
}

impl Instr {
    /// The opcode.
    pub fn opcode(&self) -> u16 {
        match *self {
            Instr::Move { .. } => 0x01,
            Instr::Const { .. } => 0x02,
            Instr::Bool { .. } => 0x03,
            Instr::Int { .. } => 0x04,
            Instr::Func { .. } => 0x05,
            Instr::Clear { .. } => 0x06,
            Instr::IntBin { op, .. } => 0x10u16.wrapping_add(position(&INT_BIN, op)),
            Instr::IntUn { op, .. } => 0x1Au16.wrapping_add(position(&INT_UN, op)),
            Instr::Not { .. } => 0x20,
            Instr::BoolBin { op, .. } => 0x21u16.wrapping_add(position(&BOOL_BIN, op)),
            Instr::DecBin { op, .. } => 0x28u16.wrapping_add(position(&ARITH, op)),
            Instr::DecNeg { .. } => 0x2C,
            Instr::RatBin { op, .. } => 0x30u16.wrapping_add(position(&ARITH, op)),
            Instr::RatNeg { .. } => 0x34,
            Instr::RatNew { .. } => 0x35,
            Instr::RatDec { .. } => 0x36,
            Instr::Cmp { op, .. } => 0x40u16.wrapping_add(position(&CMP, op)),
            Instr::Jmp { .. } => 0x50,
            Instr::JmpIf { .. } => 0x51,
            Instr::JmpNot { .. } => 0x52,
            Instr::Ret { .. } => 0x53,
            Instr::Unreachable => 0x54,
            Instr::Call { .. } => 0x60,
            Instr::CallImp { .. } => 0x61,
            Instr::CallVal { .. } => 0x62,
            Instr::Closure { .. } => 0x63,
            Instr::Rand { .. } => 0x71,
            Instr::Step { .. } => 0x72,
            Instr::Tuple { .. } => 0x80,
            Instr::Array { .. } => 0x81,
            Instr::Repeat { .. } => 0x82,
            Instr::Adt { .. } => 0x83,
            Instr::Field { .. } => 0x84,
            Instr::IsVariant { .. } => 0x85,
            Instr::Index { .. } => 0x86,
            Instr::Len { .. } => 0x87,
            Instr::Cell { .. } => 0x90,
            Instr::RefStatic { .. } => 0x91,
            Instr::RefField { .. } => 0x92,
            Instr::RefIndex { .. } => 0x93,
            Instr::Load { .. } => 0x94,
            Instr::Store { .. } => 0x95,
            Instr::VecNew { .. } => 0xA0,
            Instr::VecGet { .. } => 0xA1,
            Instr::VecSet { .. } => 0xA2,
            Instr::VecPush { .. } => 0xA3,
            Instr::Ruby { .. } => 0xB0,
            Instr::MonoRuby { .. } => 0xB1,
            Instr::Effect { .. } => 0xB2,
            Instr::Concat { .. } => 0xB3,
            Instr::ToText { .. } => 0xB4,
        }
    }

    /// The mnemonic, as the disassembler prints it.
    pub fn mnemonic(&self) -> &'static str {
        const INT_BIN_NAMES: [&str; 10] = [
            "IADD", "ISUB", "IMUL", "IDIV", "IREM", "IAND", "IOR", "IXOR", "ISHL", "ISHR",
        ];
        const INT_UN_NAMES: [&str; 3] = ["INEG", "INOT", "ICLAMP"];
        const BOOL_NAMES: [&str; 3] = ["BAND", "BOR", "BXOR"];
        const DEC_NAMES: [&str; 4] = ["DADD", "DSUB", "DMUL", "DDIV"];
        const RAT_NAMES: [&str; 4] = ["RADD", "RSUB", "RMUL", "RDIV"];
        const CMP_NAMES: [&str; 6] = ["EQ", "NE", "LT", "LE", "GT", "GE"];
        let pick = |names: &[&'static str], index: u16| -> &'static str {
            names.get(usize::from(index)).copied().unwrap_or("?")
        };
        match *self {
            Instr::Move { .. } => "MOVE",
            Instr::Const { .. } => "CONST",
            Instr::Bool { .. } => "BOOL",
            Instr::Int { .. } => "INT",
            Instr::Func { .. } => "FUNC",
            Instr::Clear { .. } => "CLEAR",
            Instr::IntBin { op, .. } => pick(&INT_BIN_NAMES, position(&INT_BIN, op)),
            Instr::IntUn { op, .. } => pick(&INT_UN_NAMES, position(&INT_UN, op)),
            Instr::Not { .. } => "NOT",
            Instr::BoolBin { op, .. } => pick(&BOOL_NAMES, position(&BOOL_BIN, op)),
            Instr::DecBin { op, .. } => pick(&DEC_NAMES, position(&ARITH, op)),
            Instr::DecNeg { .. } => "DNEG",
            Instr::RatBin { op, .. } => pick(&RAT_NAMES, position(&ARITH, op)),
            Instr::RatNeg { .. } => "RNEG",
            Instr::RatNew { .. } => "RATNEW",
            Instr::RatDec { .. } => "RATDEC",
            Instr::Cmp { op, .. } => pick(&CMP_NAMES, position(&CMP, op)),
            Instr::Jmp { .. } => "JMP",
            Instr::JmpIf { .. } => "JMPIF",
            Instr::JmpNot { .. } => "JMPNOT",
            Instr::Ret { .. } => "RET",
            Instr::Unreachable => "UNREACHABLE",
            Instr::Call { .. } => "CALL",
            Instr::CallImp { .. } => "CALLIMP",
            Instr::CallVal { .. } => "CALLVAL",
            Instr::Closure { .. } => "CLOSURE",
            Instr::Step { .. } => "STEP",
            Instr::Rand { .. } => "RAND",
            Instr::Tuple { .. } => "TUPLE",
            Instr::Array { .. } => "ARRAY",
            Instr::Repeat { .. } => "REPEAT",
            Instr::Adt { .. } => "ADT",
            Instr::Field { .. } => "FIELD",
            Instr::IsVariant { .. } => "ISVAR",
            Instr::Index { .. } => "INDEX",
            Instr::Len { .. } => "LEN",
            Instr::Cell { .. } => "CELL",
            Instr::RefStatic { .. } => "REFSTAT",
            Instr::RefField { .. } => "REFFIELD",
            Instr::RefIndex { .. } => "REFINDEX",
            Instr::Load { .. } => "LOAD",
            Instr::Store { .. } => "STORE",
            Instr::VecNew { .. } => "VECNEW",
            Instr::VecGet { .. } => "VECGET",
            Instr::VecSet { .. } => "VECSET",
            Instr::VecPush { .. } => "VECPUSH",
            Instr::Ruby { .. } => "RUBY",
            Instr::MonoRuby { .. } => "MRUBY",
            Instr::Effect { .. } => "EFFECT",
            Instr::Concat { .. } => "CONCAT",
            Instr::ToText { .. } => "TOTEXT",
        }
    }

    /// The number of code words: 1, or 2 with an operand word.
    pub fn words(&self) -> u32 {
        match self {
            Instr::Const { .. }
            | Instr::Int { .. }
            | Instr::Func { .. }
            | Instr::IntBin { .. }
            | Instr::IntUn { .. }
            | Instr::Jmp { .. }
            | Instr::JmpIf { .. }
            | Instr::JmpNot { .. }
            | Instr::Call { .. }
            | Instr::CallImp { .. }
            | Instr::CallVal { .. }
            | Instr::Closure { .. }
            | Instr::Rand { .. }
            | Instr::Step { .. }
            | Instr::Repeat { .. }
            | Instr::Adt { .. }
            | Instr::IsVariant { .. }
            | Instr::RefStatic { .. }
            | Instr::VecGet { .. } => 2,
            _ => 1,
        }
    }

    /// The jump target, if this is a jump.
    pub fn target(&self) -> Option<Label> {
        match *self {
            Instr::Jmp { target } | Instr::JmpIf { target, .. } | Instr::JmpNot { target, .. } => {
                Some(target)
            }
            _ => None,
        }
    }

    /// Every register operand (the `b..b+c` ranges of calls, aggregates and closures are
    /// given by their first register and checked separately).
    pub fn registers(&self) -> Vec<Reg> {
        match *self {
            Instr::Move { dst, src }
            | Instr::IntUn { dst, src, .. }
            | Instr::Not { dst, src }
            | Instr::DecNeg { dst, src }
            | Instr::RatNeg { dst, src }
            | Instr::RatDec { dst, src }
            | Instr::Field { dst, src, .. }
            | Instr::IsVariant { dst, src, .. }
            | Instr::Len { dst, src }
            | Instr::Load { dst, src }
            | Instr::ToText { dst, src }
            | Instr::Cell { dst, init: src }
            | Instr::RefField { dst, base: src, .. }
            | Instr::Store { dst, value: src }
            | Instr::Step {
                dst, record: src, ..
            }
            | Instr::Concat { dst, parts: src }
            | Instr::Repeat {
                dst, value: src, ..
            }
            | Instr::VecPush {
                vec: dst,
                value: src,
            } => vec![dst, src],
            Instr::IntBin { dst, lhs, rhs, .. }
            | Instr::BoolBin { dst, lhs, rhs, .. }
            | Instr::DecBin { dst, lhs, rhs, .. }
            | Instr::RatBin { dst, lhs, rhs, .. }
            | Instr::Cmp { dst, lhs, rhs, .. }
            | Instr::RatNew {
                dst,
                num: lhs,
                den: rhs,
            }
            | Instr::Index {
                dst,
                base: lhs,
                index: rhs,
            }
            | Instr::RefIndex {
                dst,
                base: lhs,
                index: rhs,
            }
            | Instr::VecGet {
                dst,
                vec: lhs,
                index: rhs,
                ..
            }
            | Instr::VecSet {
                vec: dst,
                index: lhs,
                value: rhs,
            }
            | Instr::Ruby {
                dst,
                base: lhs,
                reading: rhs,
            }
            | Instr::MonoRuby {
                dst,
                base: lhs,
                readings: rhs,
            }
            | Instr::Effect {
                dst,
                base: lhs,
                effect: rhs,
            } => vec![dst, lhs, rhs],
            Instr::Const { dst, .. }
            | Instr::Bool { dst, .. }
            | Instr::Int { dst, .. }
            | Instr::Func { dst, .. }
            | Instr::Rand { dst, .. }
            | Instr::RefStatic { dst, .. }
            | Instr::VecNew { dst }
            | Instr::Clear { reg: dst }
            | Instr::Ret { src: dst }
            | Instr::JmpIf { cond: dst, .. }
            | Instr::JmpNot { cond: dst, .. } => vec![dst],
            Instr::Call { dst, .. }
            | Instr::CallImp { dst, .. }
            | Instr::Closure { dst, .. }
            | Instr::Tuple { dst, .. }
            | Instr::Array { dst, .. }
            | Instr::Adt { dst, .. } => vec![dst],
            Instr::CallVal { dst, callee, .. } => vec![dst, callee],
            Instr::Jmp { .. } | Instr::Unreachable => Vec::new(),
        }
    }

    /// The register range `first..first+count` this instruction reads, if any.
    pub fn range(&self) -> Option<(Reg, u16)> {
        match *self {
            Instr::Call { args, argc, .. }
            | Instr::CallImp { args, argc, .. }
            | Instr::CallVal { args, argc, .. } => Some((args, argc)),
            Instr::Closure {
                captures, count, ..
            } => Some((captures, count)),
            Instr::Tuple { first, count, .. }
            | Instr::Array { first, count, .. }
            | Instr::Adt { first, count, .. } => Some((first, count)),
            _ => None,
        }
    }

    /// The code words, with the jump target already converted to a word offset by
    /// `target_word`.
    pub fn encode(&self, target_word: impl Fn(Label) -> u32) -> (u64, Option<u64>) {
        let op = self.opcode();
        let w = |a: u16, b: u16, c: u16| word(op, a, b, c);
        match *self {
            Instr::Move { dst, src }
            | Instr::Not { dst, src }
            | Instr::DecNeg { dst, src }
            | Instr::RatNeg { dst, src }
            | Instr::RatDec { dst, src }
            | Instr::Len { dst, src }
            | Instr::Load { dst, src }
            | Instr::ToText { dst, src }
            | Instr::Cell { dst, init: src }
            | Instr::Store { dst, value: src }
            | Instr::Concat { dst, parts: src }
            | Instr::VecPush {
                vec: dst,
                value: src,
            } => (w(dst, src, 0), None),
            Instr::BoolBin { dst, lhs, rhs, .. }
            | Instr::DecBin { dst, lhs, rhs, .. }
            | Instr::RatBin { dst, lhs, rhs, .. }
            | Instr::Cmp { dst, lhs, rhs, .. }
            | Instr::RatNew {
                dst,
                num: lhs,
                den: rhs,
            }
            | Instr::Index {
                dst,
                base: lhs,
                index: rhs,
            }
            | Instr::RefIndex {
                dst,
                base: lhs,
                index: rhs,
            }
            | Instr::VecSet {
                vec: dst,
                index: lhs,
                value: rhs,
            }
            | Instr::Ruby {
                dst,
                base: lhs,
                reading: rhs,
            }
            | Instr::MonoRuby {
                dst,
                base: lhs,
                readings: rhs,
            }
            | Instr::Effect {
                dst,
                base: lhs,
                effect: rhs,
            } => (w(dst, lhs, rhs), None),
            Instr::Tuple { dst, first, count } | Instr::Array { dst, first, count } => {
                (w(dst, first, count), None)
            }
            Instr::Field { dst, src, index }
            | Instr::RefField {
                dst,
                base: src,
                index,
            } => (w(dst, src, index), None),
            Instr::Bool { dst, value } => (w(dst, u16::from(value), 0), None),
            Instr::Clear { reg } => (w(reg, 0, 0), None),
            Instr::VecNew { dst } => (w(dst, 0, 0), None),
            Instr::Ret { src } => (w(src, 0, 0), None),
            Instr::Unreachable => (w(0, 0, 0), None),
            Instr::Const { dst, value } => (w(dst, 0, 0), Some(u64::from(value.0))),
            Instr::Int { dst, value } => (w(dst, 0, 0), Some(value as u64)),
            Instr::Func { dst, func } => (w(dst, 0, 0), Some(u64::from(func.0))),
            Instr::IntBin {
                dst, lhs, rhs, ty, ..
            } => (w(dst, lhs, rhs), Some(u64::from(ty.0))),
            Instr::IntUn { dst, src, ty, .. } => (w(dst, src, 0), Some(u64::from(ty.0))),
            Instr::Jmp { target } => (w(0, 0, 0), Some(u64::from(target_word(target)))),
            Instr::JmpIf { cond, target } | Instr::JmpNot { cond, target } => {
                (w(cond, 0, 0), Some(u64::from(target_word(target))))
            }
            Instr::Call {
                dst,
                args,
                argc,
                func,
            } => (w(dst, args, argc), Some(u64::from(func.0))),
            Instr::CallImp {
                dst,
                args,
                argc,
                import,
            } => (w(dst, args, argc), Some(u64::from(import.0))),
            Instr::CallVal {
                dst,
                args,
                argc,
                callee,
                kind,
            } => (
                w(dst, args, argc),
                Some(u64::from(callee) | u64::from(kind.code()).wrapping_shl(16)),
            ),
            Instr::Closure {
                dst,
                captures,
                count,
                func,
            } => (w(dst, captures, count), Some(u64::from(func.0))),
            Instr::Rand { dst, domain } => {
                let x = match domain {
                    RandDomain::Enum(ty) => pair(ty.0, 0),
                    RandDomain::Int(ty) => pair(ty.0, 1),
                };
                (w(dst, 0, 0), Some(x))
            }
            Instr::Step { dst, record, id } => (w(dst, record, 0), Some(u64::from(id))),
            Instr::Repeat { dst, value, count } => (w(dst, value, 0), Some(count)),
            Instr::Adt {
                dst,
                first,
                count,
                ty,
                variant,
            } => (w(dst, first, count), Some(pair(ty.0, variant))),
            Instr::IsVariant {
                dst,
                src,
                ty,
                variant,
            } => (w(dst, src, 0), Some(pair(ty.0, variant))),
            Instr::RefStatic { dst, var } => (w(dst, 0, 0), Some(u64::from(var.0))),
            Instr::VecGet {
                dst,
                vec,
                index,
                option,
            } => (w(dst, vec, index), Some(u64::from(option.0))),
        }
    }

    /// Decodes the instruction starting at `code[0]`; jump targets come back as the raw
    /// word offset in a `Label`, for the caller to map to an instruction index. Returns the
    /// instruction and the number of words it used.
    pub fn decode(code: &[u64]) -> Result<(Instr, usize), InstrError> {
        let first = *code.first().ok_or(InstrError::Truncated)?;
        let op = field(first, 0);
        let (a, b, c) = (field(first, 1), field(first, 2), field(first, 3));
        let x = || code.get(1).copied().ok_or(InstrError::Truncated);
        let id32 = |x: u64| u32::try_from(x).map_err(|_| InstrError::BadOperand);
        let instr = match op {
            0x01 => Instr::Move { dst: a, src: b },
            0x02 => Instr::Const {
                dst: a,
                value: ConstId(id32(x()?)?),
            },
            0x03 => Instr::Bool {
                dst: a,
                value: match b {
                    0 => false,
                    1 => true,
                    _ => return Err(InstrError::BadOperand),
                },
            },
            0x04 => Instr::Int {
                dst: a,
                value: x()? as i64,
            },
            0x05 => Instr::Func {
                dst: a,
                func: FuncId(id32(x()?)?),
            },
            0x06 => Instr::Clear { reg: a },
            0x10..=0x19 => Instr::IntBin {
                op: op_in(&INT_BIN, 0x10, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                lhs: b,
                rhs: c,
                ty: IntTypeId(id32(x()?)?),
            },
            0x1A..=0x1C => Instr::IntUn {
                op: op_in(&INT_UN, 0x1A, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                src: b,
                ty: IntTypeId(id32(x()?)?),
            },
            0x20 => Instr::Not { dst: a, src: b },
            0x21..=0x23 => Instr::BoolBin {
                op: op_in(&BOOL_BIN, 0x21, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                lhs: b,
                rhs: c,
            },
            0x28..=0x2B => Instr::DecBin {
                op: op_in(&ARITH, 0x28, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                lhs: b,
                rhs: c,
            },
            0x2C => Instr::DecNeg { dst: a, src: b },
            0x30..=0x33 => Instr::RatBin {
                op: op_in(&ARITH, 0x30, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                lhs: b,
                rhs: c,
            },
            0x34 => Instr::RatNeg { dst: a, src: b },
            0x35 => Instr::RatNew {
                dst: a,
                num: b,
                den: c,
            },
            0x36 => Instr::RatDec { dst: a, src: b },
            0x40..=0x45 => Instr::Cmp {
                op: op_in(&CMP, 0x40, op).ok_or(InstrError::UnknownOpcode(op))?,
                dst: a,
                lhs: b,
                rhs: c,
            },
            0x50 => Instr::Jmp {
                target: Label(id32(x()?)?),
            },
            0x51 => Instr::JmpIf {
                cond: a,
                target: Label(id32(x()?)?),
            },
            0x52 => Instr::JmpNot {
                cond: a,
                target: Label(id32(x()?)?),
            },
            0x53 => Instr::Ret { src: a },
            0x54 => Instr::Unreachable,
            0x60 => Instr::Call {
                dst: a,
                args: b,
                argc: c,
                func: FuncId(id32(x()?)?),
            },
            0x61 => Instr::CallImp {
                dst: a,
                args: b,
                argc: c,
                import: ImportId(id32(x()?)?),
            },
            0x62 => {
                let x = x()?;
                if x.wrapping_shr(24) != 0 {
                    return Err(InstrError::BadOperand);
                }
                Instr::CallVal {
                    dst: a,
                    args: b,
                    argc: c,
                    callee: field(x, 0),
                    kind: FnKind::from_code((x.wrapping_shr(16) & 0xFF) as u8)
                        .ok_or(InstrError::BadOperand)?,
                }
            }
            0x63 => Instr::Closure {
                dst: a,
                captures: b,
                count: c,
                func: FuncId(id32(x()?)?),
            },
            0x71 => {
                let x = x()?;
                let index = lo32(x);
                let domain = match hi32(x) {
                    0 => RandDomain::Enum(TypeId(index)),
                    1 => RandDomain::Int(IntTypeId(index)),
                    _ => return Err(InstrError::BadOperand),
                };
                Instr::Rand { dst: a, domain }
            }
            0x72 => Instr::Step {
                dst: a,
                record: b,
                id: id32(x()?)?,
            },
            0x80 => Instr::Tuple {
                dst: a,
                first: b,
                count: c,
            },
            0x81 => Instr::Array {
                dst: a,
                first: b,
                count: c,
            },
            0x82 => Instr::Repeat {
                dst: a,
                value: b,
                count: x()?,
            },
            0x83 => {
                let x = x()?;
                Instr::Adt {
                    dst: a,
                    first: b,
                    count: c,
                    ty: TypeId(lo32(x)),
                    variant: hi32(x),
                }
            }
            0x84 => Instr::Field {
                dst: a,
                src: b,
                index: c,
            },
            0x85 => {
                let x = x()?;
                Instr::IsVariant {
                    dst: a,
                    src: b,
                    ty: TypeId(lo32(x)),
                    variant: hi32(x),
                }
            }
            0x86 => Instr::Index {
                dst: a,
                base: b,
                index: c,
            },
            0x87 => Instr::Len { dst: a, src: b },
            0x90 => Instr::Cell { dst: a, init: b },
            0x91 => Instr::RefStatic {
                dst: a,
                var: StaticId(id32(x()?)?),
            },
            0x92 => Instr::RefField {
                dst: a,
                base: b,
                index: c,
            },
            0x93 => Instr::RefIndex {
                dst: a,
                base: b,
                index: c,
            },
            0x94 => Instr::Load { dst: a, src: b },
            0x95 => Instr::Store { dst: a, value: b },
            0xA0 => Instr::VecNew { dst: a },
            0xA1 => Instr::VecGet {
                dst: a,
                vec: b,
                index: c,
                option: TypeId(id32(x()?)?),
            },
            0xA2 => Instr::VecSet {
                vec: a,
                index: b,
                value: c,
            },
            0xA3 => Instr::VecPush { vec: a, value: b },
            0xB0 => Instr::Ruby {
                dst: a,
                base: b,
                reading: c,
            },
            0xB1 => Instr::MonoRuby {
                dst: a,
                base: b,
                readings: c,
            },
            0xB2 => Instr::Effect {
                dst: a,
                base: b,
                effect: c,
            },
            0xB3 => Instr::Concat { dst: a, parts: b },
            0xB4 => Instr::ToText { dst: a, src: b },
            _ => return Err(InstrError::UnknownOpcode(op)),
        };
        // Operand fields an opcode does not use must be zero, so that every instruction
        // has exactly one encoding.
        let (expected, _) = instr.encode(|label| label.0);
        if expected != first {
            return Err(InstrError::BadOperand);
        }
        Ok((instr, instr.words() as usize))
    }
}

/// The word offset of every instruction of `code`, plus the total word count at the end.
pub fn word_offsets(code: &[Instr]) -> Vec<u32> {
    let mut out = Vec::with_capacity(code.len().saturating_add(1));
    let mut offset: u32 = 0;
    for instr in code {
        out.push(offset);
        offset = offset.saturating_add(instr.words());
    }
    out.push(offset);
    out
}
