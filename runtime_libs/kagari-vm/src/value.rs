//! Run-time values. Aggregates share their storage through `Rc` and are copied on write, so
//! every `Value` has value semantics; only references (`Ref`) alias.

use std::cell::RefCell;
use std::rc::Rc;

use kagari_bytecode::{AssetId, FuncId, StaticId, TypeId};
use kagari_num::{Decimal, Rat};

use crate::host::{HostFn, HostObj};

/// A value in a register, a cell, a static or an aggregate.
#[derive(Clone, Debug)]
pub enum Value {
    Uninit,
    Bool(bool),
    Int(i128),
    Decimal(Decimal),
    Rat(Rat),
    Text(Text),
    String(Rc<str>),
    Tuple(Rc<Vec<Value>>),
    Array(Rc<Vec<Value>>),
    Vec(Rc<Vec<Value>>),
    Adt(Rc<Adt>),
    Asset(AssetId),
    Func(FuncId),
    Closure(Rc<Closure>),
    Handle(StaticId),
    Ref(Ref),
    Host(HostObj),
    HostFn(HostFn),
}

impl Value {
    /// A fresh unit: the tuple of no items.
    pub fn unit() -> Value {
        Value::Tuple(Rc::new(Vec::new()))
    }

    /// The name of this value's tag, as traps report it.
    pub fn tag(&self) -> &'static str {
        match self {
            Value::Uninit => "Uninit",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Decimal(_) => "Decimal",
            Value::Rat(_) => "Rat",
            Value::Text(_) => "Text",
            Value::String(_) => "String",
            Value::Tuple(_) => "Tuple",
            Value::Array(_) => "Array",
            Value::Vec(_) => "Vec",
            Value::Adt(_) => "Adt",
            Value::Asset(_) => "Asset",
            Value::Func(_) => "Func",
            Value::Closure(_) => "Closure",
            Value::Handle(_) => "Handle",
            Value::Ref(_) => "Ref",
            Value::Host(_) => "Host",
            Value::HostFn(_) => "HostFn",
        }
    }
}

/// A struct or enum value.
#[derive(Clone, Debug)]
pub struct Adt {
    pub ty: AdtTy,
    pub variant: u32,
    pub fields: Vec<Value>,
}

/// A module type, or a std type the module never interned (see [`crate::TypeTable`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AdtTy {
    Module(TypeId),
    Synthetic(u32),
}

/// A closure: its function and the captured values, which precede the call's arguments.
#[derive(Clone, Debug)]
pub struct Closure {
    pub func: FuncId,
    pub captures: Vec<Value>,
}

/// Rich text, as the text constants and the text instructions build it.
#[derive(Clone, Debug)]
pub enum Text {
    Plain(Rc<str>),
    Ruby(Rc<(Text, Text)>),
    MonoRuby(Rc<(Text, Vec<Text>)>),
    /// The `Value` is a `std::narrative::text::TextEffect` Adt.
    Effect(Rc<(Text, Value)>),
    Concat(Rc<Vec<Text>>),
}

/// A reference: a root and a path of projections from it.
#[derive(Clone, Debug)]
pub struct Ref {
    pub root: RefRoot,
    pub path: Rc<Vec<Proj>>,
}

/// What a reference starts from: a heap cell (also a story static), or a host part.
#[derive(Clone, Debug)]
pub enum RefRoot {
    Cell(Rc<RefCell<Value>>),
    Host(HostObj),
}

/// One projection: a Tuple or Adt field, or an Array or Vec element.
#[derive(Clone, Copy, Debug)]
pub enum Proj {
    Field(u16),
    Index(u64),
}
