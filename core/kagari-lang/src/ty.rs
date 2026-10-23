//! Semantic types (compilation stage 3), shared by type inference, the stage 3 checks and
//! const evaluation.
//!
//! Integer values and bounds are `i128` throughout the compiler; `u128` values above
//! `i128::MAX` are out of range at compile time.

use crate::hir::{ClosureKind, FnPtrKind, Name};
use crate::item_tree::ItemId;

/// Rust's integer types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IntTy {
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
}

impl IntTy {
    pub const ALL: [IntTy; 12] = [
        IntTy::U8,
        IntTy::U16,
        IntTy::U32,
        IntTy::U64,
        IntTy::U128,
        IntTy::Usize,
        IntTy::I8,
        IntTy::I16,
        IntTy::I32,
        IntTy::I64,
        IntTy::I128,
        IntTy::Isize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            IntTy::U8 => "u8",
            IntTy::U16 => "u16",
            IntTy::U32 => "u32",
            IntTy::U64 => "u64",
            IntTy::U128 => "u128",
            IntTy::Usize => "usize",
            IntTy::I8 => "i8",
            IntTy::I16 => "i16",
            IntTy::I32 => "i32",
            IntTy::I64 => "i64",
            IntTy::I128 => "i128",
            IntTy::Isize => "isize",
        }
    }

    pub fn from_suffix(suffix: &str) -> Option<IntTy> {
        IntTy::ALL.into_iter().find(|ty| ty.name() == suffix)
    }

    /// The smallest value. `usize`/`isize` are 64-bit.
    pub fn min(self) -> i128 {
        match self {
            IntTy::U8 | IntTy::U16 | IntTy::U32 | IntTy::U64 | IntTy::U128 | IntTy::Usize => 0,
            IntTy::I8 => i128::from(i8::MIN),
            IntTy::I16 => i128::from(i16::MIN),
            IntTy::I32 => i128::from(i32::MIN),
            IntTy::I64 | IntTy::Isize => i128::from(i64::MIN),
            IntTy::I128 => i128::MIN,
        }
    }

    /// The largest value; `u128` is capped at `i128::MAX` (see the module docs).
    pub fn max(self) -> i128 {
        match self {
            IntTy::U8 => i128::from(u8::MAX),
            IntTy::U16 => i128::from(u16::MAX),
            IntTy::U32 => i128::from(u32::MAX),
            IntTy::U64 | IntTy::Usize => i128::from(u64::MAX),
            IntTy::U128 | IntTy::I128 => i128::MAX,
            IntTy::I8 => i128::from(i8::MAX),
            IntTy::I16 => i128::from(i16::MAX),
            IntTy::I32 => i128::from(i32::MAX),
            IntTy::I64 | IntTy::Isize => i128::from(i64::MAX),
        }
    }
}

/// An inference variable; only appears while a body is being inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InferVar(pub u32);

/// A type-level constant: an array length or a bound of `Int<lo, hi>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Const {
    Value(i128),
    /// A `const` generic parameter: the owner item and its index in the owner's generics.
    Param {
        owner: ItemId,
        index: u32,
    },
    Infer(InferVar),
    Error,
}

impl Const {
    pub fn value(&self) -> Option<i128> {
        match self {
            Const::Value(value) => Some(*value),
            _ => None,
        }
    }
}

/// A generic argument: a type or a constant.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GenericArg {
    Type(Ty),
    Const(Const),
}

/// A function-like definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnDefId {
    /// A `fn` item: free, associated (inherent impl) or declared in a trait.
    Item(ItemId),
    /// The constructor of a tuple struct.
    StructCtor(ItemId),
    /// The constructor of a tuple variant: the enum and the variant index.
    VariantCtor(ItemId, u32),
}

/// A function with its generic arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnRef {
    pub def: FnDefId,
    /// For a method declared in a trait (`Integer::into`): the type the trait is used on.
    pub self_ty: Option<Ty>,
    /// The generics of the parent impl (or of the ADT, for constructors) first, then the
    /// function's own, in declaration order.
    pub args: Vec<GenericArg>,
}

/// A struct or enum variant with the ADT's generic arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CtorRef {
    pub adt: ItemId,
    /// `None` for a struct.
    pub variant: Option<u32>,
    pub args: Vec<GenericArg>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ty {
    /// An error was reported; the type is unknown and checks on it are skipped.
    #[default]
    Error,
    /// The type of `return`, `break`, `continue` and `loop` without `break`.
    Never,
    Bool,
    /// A Rust integer.
    Int(IntTy),
    /// `Int<lo, hi>`.
    Bounded {
        lo: Const,
        hi: Const,
    },
    /// `()` is the empty tuple.
    Tuple(Vec<Ty>),
    Array {
        elem: Box<Ty>,
        len: Const,
    },
    /// A struct or enum, user-declared or from std (including `#[builtin]` ones such as
    /// `Text`, `Screen`, `Vec<T>`, `Decimal`).
    Adt {
        item: ItemId,
        args: Vec<GenericArg>,
    },
    Ref {
        mutable: bool,
        inner: Box<Ty>,
    },
    FnPtr {
        kind: FnPtrKind,
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// `Fn(A) -> B`, `pushdown Fn(A) -> B`, `const Fn(A) -> B`.
    Closure {
        kind: ClosureKind,
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// A named function or tuple constructor used as a value, before it coerces to a pointer.
    FnDef(Box<FnRef>),
    /// A generic type parameter.
    Param {
        owner: ItemId,
        index: u32,
    },
    /// `Self` inside a trait declaration.
    TraitSelf {
        trait_item: ItemId,
    },
    /// An associated type that cannot be normalized yet: `S::Output` for a parameter `S`.
    Projection {
        base: Box<Ty>,
        trait_item: ItemId,
        name: Name,
    },
    Infer(InferVar),
}

impl Ty {
    pub fn unit() -> Ty {
        Ty::Tuple(Vec::new())
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, Ty::Tuple(fields) if fields.is_empty())
    }

    pub fn bounded(lo: i128, hi: i128) -> Ty {
        Ty::Bounded {
            lo: Const::Value(lo),
            hi: Const::Value(hi),
        }
    }

    /// Whether the type contains `Ty::Error` or `Const::Error` anywhere.
    pub fn references_error(&self) -> bool {
        let mut found = false;
        self.walk(&mut |node| {
            found |= matches!(node, TyNode::Ty(Ty::Error) | TyNode::Const(Const::Error));
        });
        found
    }

    /// Whether the type contains inference variables.
    pub fn has_infer(&self) -> bool {
        let mut found = false;
        self.walk(&mut |node| {
            found |= matches!(
                node,
                TyNode::Ty(Ty::Infer(_)) | TyNode::Const(Const::Infer(_))
            );
        });
        found
    }

    /// Visits every type and constant inside `self`, `self` included, outermost first.
    pub fn walk(&self, visit: &mut dyn FnMut(TyNode<'_>)) {
        visit(TyNode::Ty(self));
        match self {
            Ty::Error
            | Ty::Never
            | Ty::Bool
            | Ty::Int(_)
            | Ty::Param { .. }
            | Ty::TraitSelf { .. }
            | Ty::Infer(_) => {}
            Ty::Bounded { lo, hi } => {
                visit(TyNode::Const(lo));
                visit(TyNode::Const(hi));
            }
            Ty::Tuple(fields) => {
                for field in fields {
                    field.walk(visit);
                }
            }
            Ty::Array { elem, len } => {
                elem.walk(visit);
                visit(TyNode::Const(len));
            }
            Ty::Adt { args, .. } => walk_args(args, visit),
            Ty::Ref { inner, .. } => inner.walk(visit),
            Ty::FnPtr { params, ret, .. } | Ty::Closure { params, ret, .. } => {
                for param in params {
                    param.walk(visit);
                }
                ret.walk(visit);
            }
            Ty::FnDef(fn_ref) => {
                if let Some(self_ty) = &fn_ref.self_ty {
                    self_ty.walk(visit);
                }
                walk_args(&fn_ref.args, visit);
            }
            Ty::Projection { base, .. } => base.walk(visit),
        }
    }
}

/// A node visited by [`Ty::walk`].
#[derive(Clone, Copy, Debug)]
pub enum TyNode<'a> {
    Ty(&'a Ty),
    Const(&'a Const),
}

fn walk_args(args: &[GenericArg], visit: &mut dyn FnMut(TyNode<'_>)) {
    for arg in args {
        match arg {
            GenericArg::Type(ty) => ty.walk(visit),
            GenericArg::Const(c) => visit(TyNode::Const(c)),
        }
    }
}
