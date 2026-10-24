//! The load-time verifier. It checks what the VM relies on without run-time checks: every
//! index is in range, constants are acyclic, the call rules of function kinds hold, registers
//! exist and code cannot fall off its end. Value types are not checked statically: values
//! carry tags and a mismatch is a VM panic.

use std::fmt;

use crate::instr::{Instr, RandDomain};
use crate::module::{
    ConstId, Constant, FnKind, Function, IntType, Module, StaticId, StaticKind, TypeId,
};

/// A rule the module breaks: where, and which.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    /// The function index, for errors inside a function.
    pub function: Option<u32>,
    /// The instruction index inside `function`, for errors at one instruction.
    pub instr: Option<u32>,
    pub kind: VerifyErrorKind,
}

/// The rule a module breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyErrorKind {
    /// An index into `table` (e.g. `"string"`, `"constant"`) is past its end.
    IndexOutOfRange {
        table: &'static str,
        index: u32,
    },
    /// Int type 0 is not `i128`.
    FirstIntType,
    /// An int type has `lo > hi`, or `bits` outside `1..=128`.
    BadIntType {
        index: u32,
    },
    /// An import has kind `Scene`.
    SceneImport {
        import: u32,
    },
    /// Constant `constant` refers to `child`, which does not have a lower index.
    ForwardConstant {
        constant: u32,
        child: u32,
    },
    /// A type has no variant `variant`.
    BadVariant {
        ty: u32,
        variant: u32,
    },
    /// A value of a variant is built with the wrong number of fields.
    FieldCount {
        expected: usize,
        found: usize,
    },
    /// A Handle constant names a static that is not a setting or meta.
    BadHandle {
        var: u32,
    },
    /// No function has the `main` flag.
    MissingMain,
    /// More than one function has the `main` flag.
    MultipleMain,
    /// The `main` function is not a `fn` without parameters.
    BadMain,
    /// An `entry` function is not a scene.
    EntryNotScene,
    /// The Debug section does not have one entry per function.
    DebugFunctions {
        expected: usize,
        found: usize,
    },
    /// `params > regs`.
    TooManyParams {
        params: u16,
        regs: u16,
    },
    EmptyCode,
    /// The last instruction is not RET, JMP or UNREACHABLE.
    FallThrough,
    /// A jump target past the end of the code.
    BadJumpTarget {
        target: u32,
    },
    RegisterOutOfRange {
        reg: u16,
        regs: u16,
    },
    /// The register range `first..first+count` does not fit in `regs`.
    RangeOutOfRange {
        first: u16,
        count: u16,
        regs: u16,
    },
    /// A call passes `found` arguments to a callee with `expected` parameters.
    ArgCount {
        expected: u16,
        found: u16,
    },
    /// A closure captures more values than its function has parameters.
    TooManyCaptures {
        params: u16,
        count: u16,
    },
    /// CLOSURE names a function without the `closure` flag.
    NotAClosure {
        func: u32,
    },
    /// A function of kind `caller` calls one of kind `callee`.
    CallRule {
        caller: FnKind,
        callee: FnKind,
    },
    /// STEP or RAND outside a scene.
    SceneOnly {
        instr: &'static str,
    },
    /// REFSTAT outside a scene or instant procedure.
    StoryOnly {
        caller: FnKind,
    },
    /// REFSTAT on a static that is not a story variable.
    NotStoryVariable {
        var: u32,
    },
    /// RAND draws from a type that is not a fieldless enum with at least one variant.
    BadRandEnum {
        ty: u32,
    },
}

impl fmt::Display for VerifyErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyErrorKind::IndexOutOfRange { table, index } => {
                write!(f, "{table} index {index} out of range")
            }
            VerifyErrorKind::FirstIntType => f.write_str("int type 0 is not i128"),
            VerifyErrorKind::BadIntType { index } => {
                write!(f, "int type {index} has lo > hi or bits outside 1..=128")
            }
            VerifyErrorKind::SceneImport { import } => {
                write!(f, "import {import} has kind scene")
            }
            VerifyErrorKind::ForwardConstant { constant, child } => write!(
                f,
                "constant {constant} refers to constant {child}, which is not lower"
            ),
            VerifyErrorKind::BadVariant { ty, variant } => {
                write!(f, "type {ty} has no variant {variant}")
            }
            VerifyErrorKind::FieldCount { expected, found } => {
                write!(f, "{found} fields given, the variant has {expected}")
            }
            VerifyErrorKind::BadHandle { var } => {
                write!(f, "handle to static {var}, which is not a setting or meta")
            }
            VerifyErrorKind::MissingMain => f.write_str("no function is `main`"),
            VerifyErrorKind::MultipleMain => f.write_str("more than one function is `main`"),
            VerifyErrorKind::BadMain => f.write_str("`main` is not a fn without parameters"),
            VerifyErrorKind::EntryNotScene => f.write_str("an entry function is not a scene"),
            VerifyErrorKind::DebugFunctions { expected, found } => write!(
                f,
                "debug info has {found} function entries, the module has {expected} functions"
            ),
            VerifyErrorKind::TooManyParams { params, regs } => {
                write!(f, "{params} parameters exceed {regs} registers")
            }
            VerifyErrorKind::EmptyCode => f.write_str("empty code"),
            VerifyErrorKind::FallThrough => {
                f.write_str("code ends without RET, JMP or UNREACHABLE")
            }
            VerifyErrorKind::BadJumpTarget { target } => {
                write!(f, "jump target @{target:04} past the end of the code")
            }
            VerifyErrorKind::RegisterOutOfRange { reg, regs } => {
                write!(f, "register r{reg} out of range (regs = {regs})")
            }
            VerifyErrorKind::RangeOutOfRange { first, count, regs } => write!(
                f,
                "register range r{first} + {count} out of range (regs = {regs})"
            ),
            VerifyErrorKind::ArgCount { expected, found } => {
                write!(f, "{found} arguments given, the callee takes {expected}")
            }
            VerifyErrorKind::TooManyCaptures { params, count } => {
                write!(
                    f,
                    "{count} captures exceed the closure's {params} parameters"
                )
            }
            VerifyErrorKind::NotAClosure { func } => {
                write!(f, "CLOSURE of function {func}, which is not a closure")
            }
            VerifyErrorKind::CallRule { caller, callee } => write!(
                f,
                "a {} function cannot call a {} function",
                caller.name(),
                callee.name()
            ),
            VerifyErrorKind::SceneOnly { instr } => write!(f, "{instr} outside a scene"),
            VerifyErrorKind::StoryOnly { caller } => {
                write!(f, "REFSTAT in a {} function", caller.name())
            }
            VerifyErrorKind::NotStoryVariable { var } => {
                write!(f, "REFSTAT of static {var}, which is not a story variable")
            }
            VerifyErrorKind::BadRandEnum { ty } => write!(
                f,
                "RAND of type {ty}, which is not a fieldless enum with variants"
            ),
        }
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.function, self.instr) {
            (Some(func), Some(instr)) => write!(f, "function {func}, instruction {instr}: ")?,
            (Some(func), None) => write!(f, "function {func}: ")?,
            _ => {}
        }
        self.kind.fmt(f)
    }
}

impl std::error::Error for VerifyError {}

fn err(kind: VerifyErrorKind) -> VerifyError {
    VerifyError {
        function: None,
        instr: None,
        kind,
    }
}

fn index_u32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// Checks that `index < len`.
fn in_range(table: &'static str, index: u32, len: usize) -> Result<(), VerifyErrorKind> {
    if usize::try_from(index).is_ok_and(|i| i < len) {
        Ok(())
    } else {
        Err(VerifyErrorKind::IndexOutOfRange { table, index })
    }
}

fn get<T>(items: &[T], index: u32) -> Option<&T> {
    items.get(usize::try_from(index).ok()?)
}

/// Verifies `module`, reporting the first rule it breaks.
pub fn verify(module: &Module) -> Result<(), VerifyError> {
    tables(module).map_err(err)?;
    constants(module)?;
    main_fn(module).map_err(err)?;
    code(module)
}

/// Verifies everything [`verify`] does except that the module has a `main`: for checking
/// the code of a program that is not (yet) runnable.
pub fn verify_code(module: &Module) -> Result<(), VerifyError> {
    tables(module).map_err(err)?;
    constants(module)?;
    code(module)
}

/// Rule 3's entries and rules 4–7.
fn code(module: &Module) -> Result<(), VerifyError> {
    entries(module).map_err(err)?;
    for (index, function) in module.functions.iter().enumerate() {
        let at = |instr: Option<usize>, kind| VerifyError {
            function: Some(index_u32(index)),
            instr: instr.map(index_u32),
            kind,
        };
        header(function).map_err(|kind| at(None, kind))?;
        for (i, instr) in function.code.iter().enumerate() {
            instruction(module, function, instr).map_err(|kind| at(Some(i), kind))?;
        }
    }
    Ok(())
}

/// Rule 1, outside code: every index of the tables is in range; int types are well formed.
fn tables(m: &Module) -> Result<(), VerifyErrorKind> {
    let strings = m.strings.len();
    let string = |id: u32| in_range("string", id, strings);
    for f in &m.features {
        string(f.name.0)?;
    }
    for t in &m.types {
        string(t.path.0)?;
        for v in &t.variants {
            string(v.name.0)?;
            for name in v.fields.iter().flatten() {
                string(name.0)?;
            }
        }
    }
    match m.int_types.first() {
        Some(&IntType::I128) => {}
        _ => return Err(VerifyErrorKind::FirstIntType),
    }
    for (i, t) in m.int_types.iter().enumerate() {
        if t.lo > t.hi || !(1..=128).contains(&t.bits) {
            return Err(VerifyErrorKind::BadIntType {
                index: index_u32(i),
            });
        }
    }
    for (i, import) in m.imports.iter().enumerate() {
        string(import.path.0)?;
        if import.kind == FnKind::Scene {
            return Err(VerifyErrorKind::SceneImport {
                import: index_u32(i),
            });
        }
    }
    for a in &m.assets {
        string(a.path.0)?;
    }
    for c in &m.constants {
        match c {
            Constant::String(s) | Constant::Text(s) => string(s.0)?,
            Constant::Adt { ty, .. } => in_range("type", ty.0, m.types.len())?,
            Constant::Asset(a) => in_range("asset", a.0, m.assets.len())?,
            Constant::Function(f) => in_range("function", f.0, m.functions.len())?,
            Constant::Handle(s) => in_range("static", s.0, m.statics.len())?,
            _ => {}
        }
        for child in c.children() {
            in_range("constant", child.0, m.constants.len())?;
        }
    }
    for s in &m.statics {
        string(s.path.0)?;
        in_range("constant", s.init.0, m.constants.len())?;
    }
    for f in &m.functions {
        string(f.path.0)?;
    }
    if let Some(debug) = &m.debug {
        for file in &debug.files {
            string(file.0)?;
        }
        if debug.functions.len() != m.functions.len() {
            return Err(VerifyErrorKind::DebugFunctions {
                expected: m.functions.len(),
                found: debug.functions.len(),
            });
        }
        for f in &debug.functions {
            if let Some(file) = f.file {
                in_range("debug file", file, debug.files.len())?;
            }
        }
    }
    Ok(())
}

/// The field count of variant `variant` of type `ty`.
fn variant_fields(m: &Module, ty: TypeId, variant: u32) -> Result<usize, VerifyErrorKind> {
    let adt = get(&m.types, ty.0).ok_or(VerifyErrorKind::IndexOutOfRange {
        table: "type",
        index: ty.0,
    })?;
    get(&adt.variants, variant)
        .map(|v| v.fields.len())
        .ok_or(VerifyErrorKind::BadVariant { ty: ty.0, variant })
}

/// Rule 2: constants refer only to lower constants; Adt variants and Handle statics fit.
fn constants(m: &Module) -> Result<(), VerifyError> {
    for (i, c) in m.constants.iter().enumerate() {
        let constant = index_u32(i);
        for child in c.children() {
            if child.0 >= constant {
                return Err(err(VerifyErrorKind::ForwardConstant {
                    constant,
                    child: child.0,
                }));
            }
        }
        match c {
            Constant::Adt {
                ty,
                variant,
                fields,
            } => {
                let expected = variant_fields(m, *ty, *variant).map_err(err)?;
                if expected != fields.len() {
                    return Err(err(VerifyErrorKind::FieldCount {
                        expected,
                        found: fields.len(),
                    }));
                }
            }
            Constant::Handle(var) => handle(m, *var).map_err(err)?,
            _ => {}
        }
    }
    Ok(())
}

fn handle(m: &Module, var: StaticId) -> Result<(), VerifyErrorKind> {
    match get(&m.statics, var.0).map(|s| s.kind) {
        Some(StaticKind::Setting | StaticKind::Meta) => Ok(()),
        _ => Err(VerifyErrorKind::BadHandle { var: var.0 }),
    }
}

/// Rule 3: exactly one `main`, a parameterless `fn`.
fn main_fn(m: &Module) -> Result<(), VerifyErrorKind> {
    let mut mains = m.functions.iter().filter(|f| f.flags.main);
    let main = mains.next().ok_or(VerifyErrorKind::MissingMain)?;
    if mains.next().is_some() {
        return Err(VerifyErrorKind::MultipleMain);
    }
    if main.kind != FnKind::Fn || main.params != 0 {
        return Err(VerifyErrorKind::BadMain);
    }
    Ok(())
}

/// Rule 3: entries are scenes.
fn entries(m: &Module) -> Result<(), VerifyErrorKind> {
    if m.functions
        .iter()
        .any(|f| f.flags.entry && f.kind != FnKind::Scene)
    {
        return Err(VerifyErrorKind::EntryNotScene);
    }
    Ok(())
}

/// Rule 4, per function: parameters fit, code is non-empty and cannot fall through.
fn header(f: &Function) -> Result<(), VerifyErrorKind> {
    if f.params > f.regs {
        return Err(VerifyErrorKind::TooManyParams {
            params: f.params,
            regs: f.regs,
        });
    }
    match f.code.last() {
        None => Err(VerifyErrorKind::EmptyCode),
        Some(Instr::Ret { .. } | Instr::Jmp { .. } | Instr::Unreachable) => Ok(()),
        Some(_) => Err(VerifyErrorKind::FallThrough),
    }
}

/// Rules 1 and 4–7 for one instruction.
fn instruction(m: &Module, f: &Function, instr: &Instr) -> Result<(), VerifyErrorKind> {
    // Registers and ranges.
    for reg in instr.registers() {
        if reg >= f.regs {
            return Err(VerifyErrorKind::RegisterOutOfRange { reg, regs: f.regs });
        }
    }
    if let Some((first, count)) = instr.range()
        && first.checked_add(count).is_none_or(|end| end > f.regs)
    {
        return Err(VerifyErrorKind::RangeOutOfRange {
            first,
            count,
            regs: f.regs,
        });
    }
    if let Some(target) = instr.target()
        && usize::try_from(target.0).is_ok_and(|t| t >= f.code.len())
    {
        return Err(VerifyErrorKind::BadJumpTarget { target: target.0 });
    }

    let call = |callee: FnKind| {
        if f.kind.may_call(callee) {
            Ok(())
        } else {
            Err(VerifyErrorKind::CallRule {
                caller: f.kind,
                callee,
            })
        }
    };
    let arity = |expected: u16, found: u16| {
        if expected == found {
            Ok(())
        } else {
            Err(VerifyErrorKind::ArgCount { expected, found })
        }
    };
    let scene_only = || {
        if f.kind == FnKind::Scene {
            Ok(())
        } else {
            Err(VerifyErrorKind::SceneOnly {
                instr: instr.mnemonic(),
            })
        }
    };
    let constant = |c: ConstId| in_range("constant", c.0, m.constants.len());
    let int_type = |t: u32| in_range("int type", t, m.int_types.len());
    let ty = |t: TypeId| in_range("type", t.0, m.types.len());
    let function = |func: u32| {
        get(&m.functions, func).ok_or(VerifyErrorKind::IndexOutOfRange {
            table: "function",
            index: func,
        })
    };

    match *instr {
        Instr::Const { value, .. } => constant(value)?,
        Instr::Func { func, .. } => {
            function(func.0)?;
        }
        Instr::IntBin { ty: t, .. } | Instr::IntUn { ty: t, .. } => int_type(t.0)?,
        Instr::Call { argc, func, .. } => {
            let callee = function(func.0)?;
            arity(callee.params, argc)?;
            call(callee.kind)?;
        }
        Instr::CallImp { argc, import, .. } => {
            let callee = get(&m.imports, import.0).ok_or(VerifyErrorKind::IndexOutOfRange {
                table: "import",
                index: import.0,
            })?;
            arity(callee.params, argc)?;
            call(callee.kind)?;
        }
        Instr::CallVal { kind, .. } => call(kind)?,
        Instr::Closure { count, func, .. } => {
            let callee = function(func.0)?;
            if !callee.flags.closure {
                return Err(VerifyErrorKind::NotAClosure { func: func.0 });
            }
            if count > callee.params {
                return Err(VerifyErrorKind::TooManyCaptures {
                    params: callee.params,
                    count,
                });
            }
        }
        Instr::Step { .. } => scene_only()?,
        Instr::Rand { domain, .. } => {
            match domain {
                RandDomain::Enum(t) => {
                    ty(t)?;
                    let fieldless = get(&m.types, t.0).is_some_and(|adt| {
                        adt.is_enum
                            && !adt.variants.is_empty()
                            && adt.variants.iter().all(|v| v.fields.is_empty())
                    });
                    if !fieldless {
                        return Err(VerifyErrorKind::BadRandEnum { ty: t.0 });
                    }
                }
                RandDomain::Int(t) => int_type(t.0)?,
            }
            scene_only()?;
        }
        Instr::Adt {
            count,
            ty: t,
            variant,
            ..
        } => {
            let expected = variant_fields(m, t, variant)?;
            if expected != usize::from(count) {
                return Err(VerifyErrorKind::FieldCount {
                    expected,
                    found: usize::from(count),
                });
            }
        }
        Instr::IsVariant { ty: t, variant, .. } => {
            variant_fields(m, t, variant)?;
        }
        Instr::RefStatic { var, .. } => {
            in_range("static", var.0, m.statics.len())?;
            if !matches!(f.kind, FnKind::Scene | FnKind::Instant) {
                return Err(VerifyErrorKind::StoryOnly { caller: f.kind });
            }
            if get(&m.statics, var.0).map(|s| s.kind) != Some(StaticKind::StoryVariable) {
                return Err(VerifyErrorKind::NotStoryVariable { var: var.0 });
            }
        }
        Instr::VecGet { option, .. } => ty(option)?,
        _ => {}
    }
    Ok(())
}
