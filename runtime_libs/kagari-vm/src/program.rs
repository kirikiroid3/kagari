//! A loaded program: the decoded and verified module, its constants as values, the path
//! indexes and source locations.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use kagari_bytecode::{
    ConstId, Constant, DecodeError, FuncId, Module, StrId, TypeId, VerifyError, VerifyErrorKind,
    decode, verify, word_offsets,
};
use kagari_num::{Decimal, Rat};
use rustc_hash::FxHashMap;

use crate::trap::StepId;
use crate::value::{Adt, AdtTy, Proj, Text, Value};

/// Why a program cannot be loaded.
#[derive(Debug)]
pub enum LoadError {
    Decode(DecodeError),
    Verify(VerifyError),
    /// A feature the module requires and this VM lacks.
    RequiredFeature(String),
    BadConstant {
        index: u32,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Decode(e) => write!(f, "cannot decode the bytecode: {e}"),
            LoadError::Verify(e) => write!(f, "the bytecode does not verify: {e}"),
            LoadError::RequiredFeature(name) => {
                write!(
                    f,
                    "the bytecode requires the unsupported engine feature `{name}`"
                )
            }
            LoadError::BadConstant { index } => write!(f, "constant {index} is malformed"),
        }
    }
}

impl std::error::Error for LoadError {}

/// One function's `(instruction index, byte offset)` pairs.
type FnLines = Box<[(u32, u32)]>;

/// `std::Option` as the module interned it.
#[derive(Debug)]
pub(crate) struct OptionTy {
    pub ty: TypeId,
    /// The cached `None`.
    pub none: Value,
    pub some: u32,
}

/// A decoded, verified module ready to run.
#[derive(Debug)]
pub struct Program {
    module: Module,
    constants: Box<[Value]>,
    functions: FxHashMap<Box<str>, FuncId>,
    types: FxHashMap<Box<str>, TypeId>,
    main: FuncId,
    /// Per function, `(instruction index, byte offset)`, ascending.
    lines: Box<[FnLines]>,
    pub(crate) unit: Value,
    pub(crate) empty_array: Value,
    pub(crate) empty_vec: Value,
    pub(crate) empty_path: Rc<Vec<Proj>>,
    pub(crate) option: Option<OptionTy>,
}

impl Program {
    /// Decodes and verifies `bytes`, then precomputes what running needs.
    pub fn load(bytes: &[u8]) -> Result<Rc<Program>, LoadError> {
        let module = decode(bytes).map_err(LoadError::Decode)?;
        verify(&module).map_err(LoadError::Verify)?;
        if let Some(feature) = module.features.iter().find(|f| f.required) {
            return Err(LoadError::RequiredFeature(
                string(&module, feature.name).to_owned(),
            ));
        }
        let constants = constants(&module)?;
        let mut functions = FxHashMap::default();
        for (index, function) in module.functions.iter().enumerate() {
            functions
                .entry(string(&module, function.path).into())
                .or_insert(FuncId(index_u32(index)));
        }
        let mut types = FxHashMap::default();
        for (index, ty) in module.types.iter().enumerate() {
            types
                .entry(string(&module, ty.path).into())
                .or_insert(TypeId(index_u32(index)));
        }
        let main = module
            .functions
            .iter()
            .position(|f| f.flags.main)
            .map(|index| FuncId(index_u32(index)))
            .ok_or(LoadError::Verify(VerifyError {
                function: None,
                instr: None,
                kind: VerifyErrorKind::MissingMain,
            }))?;
        let lines = module
            .functions
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let Some(fn_debug) = module.debug.as_ref().and_then(|d| d.functions.get(index))
                else {
                    return Box::default();
                };
                let offsets = word_offsets(&f.code);
                fn_debug
                    .lines
                    .iter()
                    .filter_map(|&(word, offset)| {
                        let instr = offsets.binary_search(&word).ok()?;
                        (instr < f.code.len()).then(|| (index_u32(instr), offset))
                    })
                    .collect()
            })
            .collect();
        let option = types.get("std::Option").and_then(|&ty| {
            let adt = module.types.get(ty.0 as usize)?;
            let variant = |name: &str| {
                adt.variants
                    .iter()
                    .position(|v| string(&module, v.name) == name)
                    .map(index_u32)
            };
            Some(OptionTy {
                ty,
                none: Value::Adt(Rc::new(Adt {
                    ty: AdtTy::Module(ty),
                    variant: variant("None")?,
                    fields: Vec::new(),
                })),
                some: variant("Some")?,
            })
        });
        Ok(Rc::new(Program {
            module,
            constants,
            functions,
            types,
            main,
            lines,
            unit: Value::unit(),
            empty_array: Value::Array(Rc::new(Vec::new())),
            empty_vec: Value::Vec(Rc::new(Vec::new())),
            empty_path: Rc::new(Vec::new()),
            option,
        }))
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    /// The string `id`; `""` when out of range.
    pub fn string(&self, id: StrId) -> &str {
        string(&self.module, id)
    }

    /// The path of `f`; `"<unknown function>"` when out of range.
    pub fn function_path(&self, f: FuncId) -> &str {
        match self.module.functions.get(f.0 as usize) {
            Some(function) => self.string(function.path),
            None => "<unknown function>",
        }
    }

    /// The function at `path`; the lowest id when several share it.
    pub fn find_function(&self, path: &str) -> Option<FuncId> {
        self.functions.get(path).copied()
    }

    pub fn find_type(&self, path: &str) -> Option<TypeId> {
        self.types.get(path).copied()
    }

    /// The function with the `main` flag.
    pub fn main(&self) -> FuncId {
        self.main
    }

    /// `"{file}@{byte_offset}"` of an instruction: the Debug section maps code words to byte
    /// offsets, not lines, and the runtime has no sources. `None` without a Debug entry.
    pub fn location(&self, f: FuncId, instr: u32) -> Option<String> {
        let lines = self.lines.get(f.0 as usize)?;
        let at = lines.partition_point(|&(index, _)| index <= instr);
        let &(_, offset) = lines.get(at.checked_sub(1)?)?;
        let debug = self.module.debug.as_ref()?;
        let file = debug.functions.get(f.0 as usize)?.file?;
        let path = debug.files.get(file as usize)?;
        Some(format!("{}@{offset}", self.string(*path)))
    }

    /// The precomputed value of a module constant; `None` out of range.
    pub fn constant(&self, id: ConstId) -> Option<&Value> {
        self.constants.get(id.0 as usize)
    }

    /// The step ID `{function path}#{id}`: how saves, logs and messages name a step.
    pub fn step_path(&self, step: StepId) -> String {
        format!("{}#{}", self.function_path(step.func), step.id)
    }

    pub(crate) fn function(&self, f: FuncId) -> Option<&kagari_bytecode::Function> {
        self.module.functions.get(f.0 as usize)
    }
}

fn string(module: &Module, id: StrId) -> &str {
    module.strings.get(id.0 as usize).map_or("", String::as_str)
}

pub(crate) fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Every constant as a value, in index order; children are clones of earlier entries.
fn constants(module: &Module) -> Result<Box<[Value]>, LoadError> {
    let mut out: Vec<Value> = Vec::with_capacity(module.constants.len());
    for (index, constant) in module.constants.iter().enumerate() {
        let value = constant_value(module, &out, constant).ok_or(LoadError::BadConstant {
            index: index_u32(index),
        })?;
        out.push(value);
    }
    Ok(out.into_boxed_slice())
}

/// One constant, given the values of the constants before it; `None` when malformed.
fn constant_value(module: &Module, built: &[Value], constant: &Constant) -> Option<Value> {
    let child = |id: ConstId| built.get(id.0 as usize).cloned();
    let text = |id: ConstId| match child(id) {
        Some(Value::Text(text)) => Some(text),
        _ => None,
    };
    let items = |ids: &[ConstId]| ids.iter().map(|&id| child(id)).collect::<Option<Vec<_>>>();
    let texts = |ids: &[ConstId]| ids.iter().map(|&id| text(id)).collect::<Option<Vec<_>>>();
    Some(match constant {
        Constant::Bool(b) => Value::Bool(*b),
        Constant::Int(i) => Value::Int(*i),
        Constant::Decimal { mantissa, scale } => Value::Decimal(Decimal::new(*mantissa, *scale)),
        Constant::Rat { num, den } => Value::Rat(Rat::new(*num, *den)?),
        Constant::String(s) => Value::String(Rc::from(string(module, *s))),
        Constant::Text(s) => Value::Text(Text::Plain(Rc::from(string(module, *s)))),
        Constant::Ruby { base, reading } => {
            Value::Text(Text::Ruby(Rc::new((text(*base)?, text(*reading)?))))
        }
        Constant::MonoRuby { base, readings } => {
            Value::Text(Text::MonoRuby(Rc::new((text(*base)?, texts(readings)?))))
        }
        Constant::Effect { base, effect } => {
            Value::Text(Text::Effect(Rc::new((text(*base)?, child(*effect)?))))
        }
        Constant::Concat(parts) => Value::Text(Text::Concat(Rc::new(texts(parts)?))),
        Constant::Tuple(ids) => Value::Tuple(Rc::new(items(ids)?)),
        Constant::Array(ids) => Value::Array(Rc::new(items(ids)?)),
        Constant::Vec(ids) => Value::Vec(Rc::new(items(ids)?)),
        Constant::Adt {
            ty,
            variant,
            fields,
        } => Value::Adt(Rc::new(Adt {
            ty: AdtTy::Module(*ty),
            variant: *variant,
            fields: items(fields)?,
        })),
        Constant::Asset(a) => Value::Asset(*a),
        Constant::Function(f) => Value::Func(*f),
        Constant::Handle(s) => Value::Handle(*s),
    })
}

/// A cell holding `value`.
pub(crate) fn new_cell(value: Value) -> Rc<RefCell<Value>> {
    Rc::new(RefCell::new(value))
}
