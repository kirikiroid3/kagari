//! A human-readable listing of a module's functions. It does not assume the module is
//! verified: out-of-range indices print as `?<table><index>`.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::instr::{Instr, Label, RandDomain, Reg};
use crate::module::{ConstId, Constant, FnFlags, IntType, Module, StaticId, StrId, TypeId};

/// Constant lists longer than this print their first items and `…`.
const MAX_ITEMS: usize = 4;
/// Constants nested deeper than this print as `…`.
const MAX_DEPTH: u32 = 3;

/// Lists every function whose path passes `filter`, then the imports and statics those
/// functions refer to, then the options.
pub fn disassemble(module: &Module, filter: &dyn Fn(&str) -> bool) -> String {
    let d = Disasm { m: module };
    let mut out = String::new();
    let mut imports = BTreeSet::new();
    let mut statics = BTreeSet::new();
    for f in &module.functions {
        let path = d.string(f.path);
        if !filter(&path) {
            continue;
        }
        let _ = writeln!(
            out,
            "fn {path} [{}] params={} regs={}",
            kind_and_flags(f.kind.name(), f.flags),
            f.params,
            f.regs
        );
        for (index, instr) in f.code.iter().enumerate() {
            let operands = d.operands(instr, &path);
            let _ = write!(out, "  {index:04} {}", instr.mnemonic());
            if !operands.is_empty() {
                let _ = write!(out, " {}", operands.join(", "));
            }
            out.push('\n');
            match *instr {
                Instr::CallImp { import, .. } => {
                    imports.insert(import.0);
                }
                Instr::RefStatic { var, .. } => {
                    statics.insert(var.0);
                }
                Instr::Const { value, .. } => d.handles(value, &mut statics, &mut BTreeSet::new()),
                _ => {}
            }
        }
        out.push('\n');
    }
    for index in imports {
        match module
            .imports
            .get(usize::try_from(index).unwrap_or(usize::MAX))
        {
            Some(import) => {
                let _ = writeln!(
                    out,
                    "import {} [{}] params={}",
                    d.string(import.path),
                    import.kind.name(),
                    import.params
                );
            }
            None => {
                let _ = writeln!(out, "import ?import{index}");
            }
        }
    }
    for index in statics {
        match module
            .statics
            .get(usize::try_from(index).unwrap_or(usize::MAX))
        {
            Some(s) => {
                let _ = writeln!(
                    out,
                    "static {} [{}] = {}",
                    d.string(s.path),
                    s.kind.name(),
                    d.constant(s.init, 0)
                );
            }
            None => {
                let _ = writeln!(out, "static ?static{index}");
            }
        }
    }
    let _ = writeln!(
        out,
        "options max_call_depth={}",
        module.options.max_call_depth
    );
    out
}

fn kind_and_flags(kind: &str, flags: FnFlags) -> String {
    let mut out = kind.to_owned();
    for (set, name) in [
        (flags.entry, "entry"),
        (flags.main, "main"),
        (flags.closure, "closure"),
        (flags.synthetic, "synthetic"),
    ] {
        if set {
            out.push_str(", ");
            out.push_str(name);
        }
    }
    out
}

fn get<T>(items: &[T], index: u32) -> Option<&T> {
    items.get(usize::try_from(index).ok()?)
}

fn reg(r: Reg) -> String {
    format!("r{r}")
}

fn label(l: Label) -> String {
    format!("@{:04}", l.0)
}

/// The registers `first..first+count`, as `(r2, r3)`.
fn regs(first: Reg, count: u16) -> String {
    let items: Vec<String> = (0..count)
        .map(|i| format!("r{}", u32::from(first).saturating_add(u32::from(i))))
        .collect();
    format!("({})", items.join(", "))
}

/// The name of an int type: a Rust integer type when the range is exactly one, else
/// `Int<lo,hi>`.
fn int_type_name(t: &IntType) -> String {
    const RUST: [(&str, u32, bool); 10] = [
        ("u8", 8, false),
        ("u16", 16, false),
        ("u32", 32, false),
        ("u64", 64, false),
        ("u128", 128, false),
        ("i8", 8, true),
        ("i16", 16, true),
        ("i32", 32, true),
        ("i64", 64, true),
        ("i128", 128, true),
    ];
    for (name, bits, signed) in RUST {
        let (lo, hi) = match (bits, signed) {
            // `u128` is capped at `i128::MAX`, as in the compiler.
            (128, false) => (0, i128::MAX),
            (128, true) => (i128::MIN, i128::MAX),
            (bits, false) => (0, 1i128.checked_shl(bits).unwrap_or(0).saturating_sub(1)),
            (bits, true) => {
                let half = 1i128.checked_shl(bits.saturating_sub(1)).unwrap_or(0);
                (half.saturating_neg(), half.saturating_sub(1))
            }
        };
        if *t
            == (IntType {
                lo,
                hi,
                bits,
                signed,
            })
        {
            return name.to_owned();
        }
    }
    format!("Int<{},{}>", t.lo, t.hi)
}

/// `mantissa / 10^scale` as exact decimal text.
fn decimal(mantissa: i128, scale: u32) -> String {
    if scale == 0 {
        return format!("{mantissa}.0");
    }
    // An i128 has at most 39 digits; a larger scale is not worth expanding.
    if scale > 39 {
        return format!("{mantissa}e-{scale}");
    }
    let digits = mantissa.unsigned_abs().to_string();
    let scale = usize::try_from(scale).unwrap_or(39);
    let padded = format!("{digits:0>width$}", width = scale.saturating_add(1));
    let (int, frac) = padded.split_at(padded.len().saturating_sub(scale));
    let sign = if mantissa < 0 { "-" } else { "" };
    format!("{sign}{int}.{frac}")
}

struct Disasm<'a> {
    m: &'a Module,
}

impl Disasm<'_> {
    fn string(&self, id: StrId) -> String {
        get(&self.m.strings, id.0)
            .cloned()
            .unwrap_or_else(|| format!("?str{}", id.0))
    }

    fn func(&self, id: u32) -> String {
        get(&self.m.functions, id).map_or_else(|| format!("?fn{id}"), |f| self.string(f.path))
    }

    fn import(&self, id: u32) -> String {
        get(&self.m.imports, id).map_or_else(|| format!("?import{id}"), |i| self.string(i.path))
    }

    fn static_path(&self, id: StaticId) -> String {
        get(&self.m.statics, id.0)
            .map_or_else(|| format!("?static{}", id.0), |s| self.string(s.path))
    }

    fn ty(&self, id: TypeId) -> String {
        get(&self.m.types, id.0).map_or_else(|| format!("?type{}", id.0), |t| self.string(t.path))
    }

    /// `Type::Variant` for an enum, `Type` for a struct.
    fn variant(&self, ty: TypeId, variant: u32) -> String {
        let Some(adt) = get(&self.m.types, ty.0) else {
            return format!("?type{}::{variant}", ty.0);
        };
        let path = self.string(adt.path);
        if !adt.is_enum {
            return path;
        }
        match get(&adt.variants, variant) {
            Some(v) => format!("{path}::{}", self.string(v.name)),
            None => format!("{path}::?{variant}"),
        }
    }

    fn int_type(&self, id: u32) -> String {
        get(&self.m.int_types, id).map_or_else(|| format!("?int{id}"), int_type_name)
    }

    /// The plain string of a Text constant.
    fn plain(&self, id: ConstId, depth: u32) -> Option<String> {
        if depth > MAX_DEPTH.saturating_mul(4) {
            return None;
        }
        let next = depth.saturating_add(1);
        match get(&self.m.constants, id.0)? {
            Constant::Text(s) => get(&self.m.strings, s.0).cloned(),
            Constant::Ruby { base, .. }
            | Constant::MonoRuby { base, .. }
            | Constant::Effect { base, .. } => self.plain(*base, next),
            Constant::Concat(parts) => parts.iter().map(|p| self.plain(*p, next)).collect(),
            _ => None,
        }
    }

    fn list(&self, items: &[ConstId], depth: u32) -> String {
        let mut parts: Vec<String> = items
            .iter()
            .take(MAX_ITEMS)
            .map(|c| self.constant(*c, depth.saturating_add(1)))
            .collect();
        if items.len() > MAX_ITEMS {
            parts.push("…".to_owned());
        }
        parts.join(", ")
    }

    /// A short form of a constant's value.
    fn constant(&self, id: ConstId, depth: u32) -> String {
        if depth > MAX_DEPTH {
            return "…".to_owned();
        }
        let Some(c) = get(&self.m.constants, id.0) else {
            return format!("?const{}", id.0);
        };
        match c {
            Constant::Bool(b) => b.to_string(),
            Constant::Int(v) => v.to_string(),
            Constant::Decimal { mantissa, scale } => decimal(*mantissa, *scale),
            Constant::Rat { num, den } => format!("{num}/{den}"),
            Constant::String(s) => format!("str {:?}", self.string(*s)),
            Constant::Text(_)
            | Constant::Ruby { .. }
            | Constant::MonoRuby { .. }
            | Constant::Effect { .. }
            | Constant::Concat(_) => match self.plain(id, 0) {
                Some(text) => format!("{text:?}"),
                None => "text ?".to_owned(),
            },
            Constant::Tuple(items) => format!("({})", self.list(items, depth)),
            Constant::Array(items) => format!("[{}]", self.list(items, depth)),
            Constant::Vec(items) => format!("vec[{}]", self.list(items, depth)),
            Constant::Adt {
                ty,
                variant,
                fields,
            } => {
                let name = self.variant(*ty, *variant);
                if fields.is_empty() {
                    name
                } else {
                    format!("{name} {{{}}}", self.list(fields, depth))
                }
            }
            Constant::Asset(a) => match get(&self.m.assets, a.0) {
                Some(asset) => format!("{} {:?}", asset.kind.name(), self.string(asset.path)),
                None => format!("?asset{}", a.0),
            },
            Constant::Function(f) => self.func(f.0),
            Constant::Handle(s) => self.static_path(*s),
        }
    }

    /// Collects the statics of the Handle constants inside constant `id`.
    fn handles(&self, id: ConstId, out: &mut BTreeSet<u32>, seen: &mut BTreeSet<u32>) {
        if !seen.insert(id.0) {
            return;
        }
        let Some(c) = get(&self.m.constants, id.0) else {
            return;
        };
        if let Constant::Handle(s) = c {
            out.insert(s.0);
        }
        for child in c.children() {
            self.handles(child, out, seen);
        }
    }

    /// The operands of `instr`, an instruction of the function at `path`.
    fn operands(&self, instr: &Instr, path: &str) -> Vec<String> {
        match *instr {
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
            } => vec![reg(dst), reg(src)],
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
            } => vec![reg(dst), reg(lhs), reg(rhs)],
            Instr::Const { dst, value } => {
                vec![
                    reg(dst),
                    format!("#{} {}", value.0, self.constant(value, 0)),
                ]
            }
            Instr::Bool { dst, value } => vec![reg(dst), value.to_string()],
            Instr::Int { dst, value } => vec![reg(dst), value.to_string()],
            Instr::Func { dst, func } => vec![reg(dst), self.func(func.0)],
            Instr::Clear { reg: r } | Instr::Ret { src: r } | Instr::VecNew { dst: r } => {
                vec![reg(r)]
            }
            Instr::IntBin {
                dst, lhs, rhs, ty, ..
            } => vec![reg(dst), reg(lhs), reg(rhs), self.int_type(ty.0)],
            Instr::IntUn { dst, src, ty, .. } => vec![reg(dst), reg(src), self.int_type(ty.0)],
            Instr::Jmp { target } => vec![label(target)],
            Instr::JmpIf { cond, target } | Instr::JmpNot { cond, target } => {
                vec![reg(cond), label(target)]
            }
            Instr::Unreachable => Vec::new(),
            // The step ID in its full form, as the runtime names it.
            Instr::Step { dst, record, id } => vec![reg(dst), reg(record), format!("{path}#{id}")],
            Instr::Call {
                dst,
                args,
                argc,
                func,
            } => vec![reg(dst), regs(args, argc), self.func(func.0)],
            Instr::CallImp {
                dst,
                args,
                argc,
                import,
            } => vec![reg(dst), regs(args, argc), self.import(import.0)],
            Instr::CallVal {
                dst,
                args,
                argc,
                callee,
                kind,
            } => vec![
                reg(dst),
                regs(args, argc),
                reg(callee),
                kind.name().to_owned(),
            ],
            Instr::Closure {
                dst,
                captures,
                count,
                func,
            } => vec![reg(dst), regs(captures, count), self.func(func.0)],
            Instr::Rand { dst, domain } => vec![
                reg(dst),
                match domain {
                    RandDomain::Enum(ty) => self.ty(ty),
                    RandDomain::Int(ty) => self.int_type(ty.0),
                },
            ],
            Instr::Tuple { dst, first, count } | Instr::Array { dst, first, count } => {
                vec![reg(dst), regs(first, count)]
            }
            Instr::Repeat { dst, value, count } => vec![reg(dst), reg(value), count.to_string()],
            Instr::Adt {
                dst,
                first,
                count,
                ty,
                variant,
            } => vec![reg(dst), regs(first, count), self.variant(ty, variant)],
            Instr::Field { dst, src, index }
            | Instr::RefField {
                dst,
                base: src,
                index,
            } => vec![reg(dst), reg(src), index.to_string()],
            Instr::IsVariant {
                dst,
                src,
                ty,
                variant,
            } => vec![reg(dst), reg(src), self.variant(ty, variant)],
            Instr::RefStatic { dst, var } => vec![reg(dst), self.static_path(var)],
            Instr::VecGet {
                dst,
                vec,
                index,
                option,
            } => vec![reg(dst), reg(vec), reg(index), self.ty(option)],
        }
    }
}
