//! Moving values from one program to another (scene hot swap): functions, types, assets and
//! statics are indices into a program's tables, so a value built by one program is rebuilt for
//! the other by path, as saves store them. Cells keep their aliasing: every reference to one
//! old cell refers to one new cell.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use kagari_bytecode::{AssetId, FuncId, StaticId};
use rustc_hash::FxHashMap;

use crate::program::{Program, new_cell};
use crate::types::TypeTable;
use crate::value::{Adt, AdtTy, Closure, Ref, RefRoot, Text, Value};

/// Why a value cannot move to the new program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemapError {
    /// No function at this path, or one of another kind or arity.
    Function(String),
    /// A closure whose function's code changed (closure paths are positional, so another
    /// closure may have the path now).
    Closure(String),
    /// The type is gone or its definition changed.
    Type(String),
    /// The asset is gone or is of another kind.
    Asset(String),
    /// The static is gone or is of another kind.
    Static(String),
    /// The story variables differ: one was added or removed.
    StoryVariable(String),
    /// A frame's function no longer has the same code layout.
    Frame(String),
    /// A host object or host function that cannot move (`type_name`).
    Host(String),
}

impl fmt::Display for RemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemapError::Function(p) => write!(f, "the function `{p}` is gone or changed kind"),
            RemapError::Closure(p) => write!(f, "the closure `{p}` changed"),
            RemapError::Type(p) => write!(f, "the type `{p}` is gone or its definition changed"),
            RemapError::Asset(p) => write!(f, "the asset `{p}` is gone"),
            RemapError::Static(p) => write!(f, "the static `{p}` is gone or changed kind"),
            RemapError::StoryVariable(m) => write!(f, "{m}"),
            RemapError::Frame(p) => write!(f, "the code of `{p}` no longer lines up"),
            RemapError::Host(t) => write!(f, "a `{t}` value cannot move to the new program"),
        }
    }
}

impl std::error::Error for RemapError {}

/// Rebuilds values of `old` for `new`.
pub struct Remap<'a> {
    old: &'a Program,
    old_types: &'a TypeTable,
    new: &'a Program,
    new_types: &'a mut TypeTable,
    /// Whether an old function and the new one at its path have the same code.
    same_code: &'a dyn Fn(FuncId, FuncId) -> bool,
    assets: FxHashMap<String, AssetId>,
    statics: FxHashMap<String, StaticId>,
    types: FxHashMap<AdtTy, AdtTy>,
    cells: FxHashMap<*const RefCell<Value>, Rc<RefCell<Value>>>,
}

impl<'a> Remap<'a> {
    pub fn new(
        old: &'a Program,
        old_types: &'a TypeTable,
        new: &'a Program,
        new_types: &'a mut TypeTable,
        same_code: &'a dyn Fn(FuncId, FuncId) -> bool,
    ) -> Remap<'a> {
        let module = new.module();
        let assets = module
            .assets
            .iter()
            .enumerate()
            .map(|(i, a)| (new.string(a.path).to_owned(), AssetId(index(i))))
            .collect();
        let statics = module
            .statics
            .iter()
            .enumerate()
            .map(|(i, s)| (new.string(s.path).to_owned(), StaticId(index(i))))
            .collect();
        Remap {
            old,
            old_types,
            new,
            new_types,
            same_code,
            assets,
            statics,
            types: FxHashMap::default(),
            cells: FxHashMap::default(),
        }
    }

    pub fn old(&self) -> &'a Program {
        self.old
    }

    pub fn new_program(&self) -> &'a Program {
        self.new
    }

    /// The new function at the path of `f`, of the same kind and arity.
    pub fn func(&mut self, f: FuncId) -> Result<FuncId, RemapError> {
        let path = self.old.function_path(f);
        let fits = |new: FuncId| {
            self.old.kind(f) == self.new.kind(new) && self.old.params(f) == self.new.params(new)
        };
        self.new
            .find_function(path)
            .filter(|&new| fits(new))
            .ok_or_else(|| RemapError::Function(path.to_owned()))
    }

    /// The new type at the path of `ty`, whose definition must be the same.
    pub fn adt_ty(&mut self, ty: AdtTy) -> Result<AdtTy, RemapError> {
        if let Some(&new) = self.types.get(&ty) {
            return Ok(new);
        }
        let path = self.old_types.path(ty).to_owned();
        let shape = self
            .old_types
            .shape(ty)
            .ok_or_else(|| RemapError::Type(path.clone()))?;
        let variants: Vec<&str> = shape.variants.iter().map(String::as_str).collect();
        let new = self.new_types.resolve(&path, &variants, shape.is_enum);
        let agrees = self
            .new_types
            .shape(new)
            .is_some_and(|other| other.agrees(&shape));
        if !agrees {
            return Err(RemapError::Type(path));
        }
        self.types.insert(ty, new);
        Ok(new)
    }

    /// The new asset at the path of `a`, of the same kind.
    pub fn asset(&self, a: AssetId) -> Result<AssetId, RemapError> {
        let old = self.old.module().assets.get(a.0 as usize);
        let path = old.map_or("<unknown asset>", |old| self.old.string(old.path));
        self.assets
            .get(path)
            .copied()
            .filter(|new| {
                let kind = self.new.module().assets.get(new.0 as usize).map(|n| n.kind);
                kind.is_some() && kind == old.map(|o| o.kind)
            })
            .ok_or_else(|| RemapError::Asset(path.to_owned()))
    }

    /// The new static at the path of `s`, of the same kind.
    pub fn static_id(&self, s: StaticId) -> Result<StaticId, RemapError> {
        let old = self.old.module().statics.get(s.0 as usize);
        let path = old.map_or("<unknown static>", |old| self.old.string(old.path));
        self.statics
            .get(path)
            .copied()
            .filter(|new| {
                let kind = self
                    .new
                    .module()
                    .statics
                    .get(new.0 as usize)
                    .map(|n| n.kind);
                kind.is_some() && kind == old.map(|o| o.kind)
            })
            .ok_or_else(|| RemapError::Static(path.to_owned()))
    }

    /// The new static by path, of any kind.
    pub(crate) fn static_by_path(&self, path: &str) -> Option<StaticId> {
        self.statics.get(path).copied()
    }

    /// Makes `new` the cell that every reference to `old` refers to from now on.
    pub(crate) fn seed_cell(&mut self, old: &Rc<RefCell<Value>>, new: Rc<RefCell<Value>>) {
        self.cells.insert(Rc::as_ptr(old), new);
    }

    /// The new cell of `old`; its value is rebuilt the first time it is met.
    pub fn cell(&mut self, old: &Rc<RefCell<Value>>) -> Result<Rc<RefCell<Value>>, RemapError> {
        if let Some(new) = self.cells.get(&Rc::as_ptr(old)) {
            return Ok(new.clone());
        }
        // Registered before its value is rebuilt: a cell may refer to itself.
        let new = new_cell(Value::Uninit);
        self.cells.insert(Rc::as_ptr(old), new.clone());
        let value = old
            .try_borrow()
            .map_err(|_| RemapError::Host("borrowed cell".to_owned()))?
            .clone();
        let value = self.value(&value)?;
        *new.try_borrow_mut()
            .map_err(|_| RemapError::Host("borrowed cell".to_owned()))? = value;
        Ok(new)
    }

    pub fn text(&mut self, text: &Text) -> Result<Text, RemapError> {
        Ok(match text {
            Text::Plain(s) => Text::Plain(s.clone()),
            Text::Ruby(pair) => Text::Ruby(Rc::new((self.text(&pair.0)?, self.text(&pair.1)?))),
            Text::MonoRuby(pair) => {
                let readings = pair
                    .1
                    .iter()
                    .map(|t| self.text(t))
                    .collect::<Result<_, _>>()?;
                Text::MonoRuby(Rc::new((self.text(&pair.0)?, readings)))
            }
            Text::Effect(pair) => {
                Text::Effect(Rc::new((self.text(&pair.0)?, self.value(&pair.1)?)))
            }
            Text::Concat(parts) => Text::Concat(Rc::new(
                parts
                    .iter()
                    .map(|t| self.text(t))
                    .collect::<Result<_, _>>()?,
            )),
        })
    }

    fn items(&mut self, items: &[Value]) -> Result<Rc<Vec<Value>>, RemapError> {
        Ok(Rc::new(
            items
                .iter()
                .map(|v| self.value(v))
                .collect::<Result<_, _>>()?,
        ))
    }

    /// `value` rebuilt for the new program.
    pub fn value(&mut self, value: &Value) -> Result<Value, RemapError> {
        Ok(match value {
            Value::Uninit
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Decimal(_)
            | Value::Rat(_)
            | Value::String(_) => value.clone(),
            Value::Text(text) => Value::Text(self.text(text)?),
            Value::Tuple(items) => Value::Tuple(self.items(items)?),
            Value::Array(items) => Value::Array(self.items(items)?),
            Value::Vec(items) => Value::Vec(self.items(items)?),
            Value::Adt(adt) => Value::Adt(Rc::new(Adt {
                ty: self.adt_ty(adt.ty)?,
                variant: adt.variant,
                fields: adt
                    .fields
                    .iter()
                    .map(|v| self.value(v))
                    .collect::<Result<_, _>>()?,
            })),
            Value::Asset(a) => Value::Asset(self.asset(*a)?),
            Value::Func(f) => Value::Func(self.func(*f)?),
            Value::Closure(c) => {
                let func = self.func(c.func)?;
                if !(self.same_code)(c.func, func) {
                    return Err(RemapError::Closure(
                        self.old.function_path(c.func).to_owned(),
                    ));
                }
                Value::Closure(Rc::new(Closure {
                    func,
                    captures: c
                        .captures
                        .iter()
                        .map(|v| self.value(v))
                        .collect::<Result<_, _>>()?,
                }))
            }
            Value::Handle(s) => Value::Handle(self.static_id(*s)?),
            Value::Ref(r) => Value::Ref(self.reference(r)?),
            Value::Host(obj) => obj
                .0
                .remap(self)?
                .map(Value::Host)
                .ok_or_else(|| RemapError::Host(obj.0.type_name().to_owned()))?,
            Value::HostFn(_) => return Err(RemapError::Host("host function".to_owned())),
        })
    }

    /// A reference to the new cell; a host part cannot move.
    pub fn reference(&mut self, r: &Ref) -> Result<Ref, RemapError> {
        match &r.root {
            RefRoot::Cell(cell) => Ok(Ref {
                root: RefRoot::Cell(self.cell(cell)?),
                path: r.path.clone(),
            }),
            RefRoot::Host(obj) => Err(RemapError::Host(obj.0.type_name().to_owned())),
        }
    }
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
