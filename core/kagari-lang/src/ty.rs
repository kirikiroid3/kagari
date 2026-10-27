//! Semantic types (compilation stage 3), shared by type inference, the stage 3 checks and
//! const evaluation.
//!
//! Integer values and bounds are `i128` throughout the compiler; `u128` values above
//! `i128::MAX` are out of range at compile time.

use std::ops::Deref;
use std::sync::LazyLock;

use crate::hir::{ClosureKind, FnPtrKind, Name};
use crate::intern::{Interned, internable};
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
    fn flags(&self) -> TyFlags {
        match self {
            Const::Value(_) => TyFlags::EMPTY,
            Const::Param { .. } => TyFlags::HAS_PARAM,
            Const::Infer(_) => TyFlags::HAS_INFER,
            Const::Error => TyFlags::HAS_ERROR,
        }
    }

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

impl GenericArg {
    fn flags(&self) -> TyFlags {
        match self {
            GenericArg::Type(ty) => ty.flags(),
            GenericArg::Const(c) => c.flags(),
        }
    }
}

internable!(Box<[GenericArg]>);

/// An interned list of generic arguments.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GenericArgs(Interned<Box<[GenericArg]>>);

static EMPTY_ARGS: LazyLock<GenericArgs> =
    LazyLock::new(|| GenericArgs(Interned::new(Box::default())));

impl GenericArgs {
    pub fn new(args: Vec<GenericArg>) -> GenericArgs {
        if args.is_empty() {
            return GenericArgs::empty();
        }
        GenericArgs(Interned::new(args.into_boxed_slice()))
    }

    pub fn empty() -> GenericArgs {
        EMPTY_ARGS.clone()
    }

    fn flags(&self) -> TyFlags {
        self.iter()
            .fold(TyFlags::EMPTY, |flags, arg| flags.union(arg.flags()))
    }
}

impl Default for GenericArgs {
    fn default() -> Self {
        GenericArgs::empty()
    }
}

impl Deref for GenericArgs {
    type Target = [GenericArg];

    fn deref(&self) -> &[GenericArg] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a GenericArgs {
    type Item = &'a GenericArg;
    type IntoIter = std::slice::Iter<'a, GenericArg>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FromIterator<GenericArg> for GenericArgs {
    fn from_iter<I: IntoIterator<Item = GenericArg>>(iter: I) -> Self {
        GenericArgs::new(iter.into_iter().collect())
    }
}

impl std::fmt::Debug for GenericArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
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
    pub args: GenericArgs,
}

/// A struct or enum variant with the ADT's generic arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CtorRef {
    pub adt: ItemId,
    /// `None` for a struct.
    pub variant: Option<u32>,
    pub args: GenericArgs,
}

/// The shape of a type; wrap it with [`Ty::new`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TyKind {
    /// An error was reported; the type is unknown and checks on it are skipped.
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
    Tuple(Box<[Ty]>),
    Array {
        elem: Ty,
        len: Const,
    },
    /// A struct or enum, user-declared or from std (including `#[builtin]` ones such as
    /// `Text`, `Screen`, `Vec<T>`, `Decimal`).
    Adt {
        item: ItemId,
        args: GenericArgs,
    },
    Ref {
        mutable: bool,
        inner: Ty,
    },
    FnPtr {
        kind: FnPtrKind,
        params: Box<[Ty]>,
        ret: Ty,
    },
    /// `Fn(A) -> B`, `pushdown Fn(A) -> B`, `const Fn(A) -> B`.
    Closure {
        kind: ClosureKind,
        params: Box<[Ty]>,
        ret: Ty,
    },
    /// A named function or tuple constructor used as a value, before it coerces to a pointer.
    FnDef(FnRef),
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
        base: Ty,
        trait_item: ItemId,
        name: Name,
    },
    Infer(InferVar),
}

/// What a type contains, computed once when it is interned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TyFlags(u8);

impl TyFlags {
    pub const EMPTY: TyFlags = TyFlags(0);
    /// `Error` or `Const::Error`.
    pub const HAS_ERROR: TyFlags = TyFlags(1);
    /// `Infer` or `Const::Infer`.
    pub const HAS_INFER: TyFlags = TyFlags(2);
    /// `Param`, `TraitSelf` or `Const::Param`.
    pub const HAS_PARAM: TyFlags = TyFlags(4);

    pub fn union(self, other: TyFlags) -> TyFlags {
        TyFlags(self.0 | other.0)
    }

    pub fn contains(self, other: TyFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

/// An interned type with its flags.
#[derive(Clone, Debug)]
pub struct TyData {
    kind: TyKind,
    flags: TyFlags,
}

impl PartialEq for TyData {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for TyData {}

impl std::hash::Hash for TyData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

internable!(TyData);

/// A semantic type: an interned [`TyKind`]. Equality is a pointer comparison.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Ty(Interned<TyData>);

static ERROR: LazyLock<Ty> = LazyLock::new(|| Ty::new(TyKind::Error));
static NEVER: LazyLock<Ty> = LazyLock::new(|| Ty::new(TyKind::Never));
static BOOL: LazyLock<Ty> = LazyLock::new(|| Ty::new(TyKind::Bool));
static UNIT: LazyLock<Ty> = LazyLock::new(|| Ty::new(TyKind::Tuple(Box::default())));

impl Default for Ty {
    fn default() -> Self {
        Ty::error()
    }
}

impl std::fmt::Debug for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind().fmt(f)
    }
}

impl Ty {
    pub fn new(kind: TyKind) -> Ty {
        let flags = kind_flags(&kind);
        Ty(Interned::new(TyData { kind, flags }))
    }

    pub fn kind(&self) -> &TyKind {
        &self.0.kind
    }

    pub fn flags(&self) -> TyFlags {
        self.0.flags
    }

    pub fn error() -> Ty {
        ERROR.clone()
    }

    pub fn never() -> Ty {
        NEVER.clone()
    }

    pub fn bool() -> Ty {
        BOOL.clone()
    }

    pub fn unit() -> Ty {
        UNIT.clone()
    }

    pub fn int(int: IntTy) -> Ty {
        Ty::new(TyKind::Int(int))
    }

    pub fn tuple(fields: Vec<Ty>) -> Ty {
        Ty::new(TyKind::Tuple(fields.into_boxed_slice()))
    }

    pub fn adt(item: ItemId, args: GenericArgs) -> Ty {
        Ty::new(TyKind::Adt { item, args })
    }

    pub fn reference(mutable: bool, inner: Ty) -> Ty {
        Ty::new(TyKind::Ref { mutable, inner })
    }

    pub fn infer(var: InferVar) -> Ty {
        Ty::new(TyKind::Infer(var))
    }

    pub fn is_unit(&self) -> bool {
        matches!(self.kind(), TyKind::Tuple(fields) if fields.is_empty())
    }

    pub fn is_error(&self) -> bool {
        matches!(self.kind(), TyKind::Error)
    }

    pub fn is_never(&self) -> bool {
        matches!(self.kind(), TyKind::Never)
    }

    pub fn bounded(lo: i128, hi: i128) -> Ty {
        Ty::new(TyKind::Bounded {
            lo: Const::Value(lo),
            hi: Const::Value(hi),
        })
    }

    /// Whether the type contains `TyKind::Error` or `Const::Error` anywhere.
    pub fn references_error(&self) -> bool {
        self.flags().contains(TyFlags::HAS_ERROR)
    }

    /// Whether the type contains inference variables.
    pub fn has_infer(&self) -> bool {
        self.flags().contains(TyFlags::HAS_INFER)
    }

    /// Whether the type contains generic parameters (`Param`, `TraitSelf`, `Const::Param`).
    pub fn has_param(&self) -> bool {
        self.flags().contains(TyFlags::HAS_PARAM)
    }

    /// Visits every type and constant inside `self`, `self` included, outermost first.
    pub fn walk(&self, visit: &mut dyn FnMut(TyNode<'_>)) {
        visit(TyNode::Ty(self));
        match self.kind() {
            TyKind::Error
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Param { .. }
            | TyKind::TraitSelf { .. }
            | TyKind::Infer(_) => {}
            TyKind::Bounded { lo, hi } => {
                visit(TyNode::Const(lo));
                visit(TyNode::Const(hi));
            }
            TyKind::Tuple(fields) => {
                for field in fields {
                    field.walk(visit);
                }
            }
            TyKind::Array { elem, len } => {
                elem.walk(visit);
                visit(TyNode::Const(len));
            }
            TyKind::Adt { args, .. } => walk_args(args, visit),
            TyKind::Ref { inner, .. } => inner.walk(visit),
            TyKind::FnPtr { params, ret, .. } | TyKind::Closure { params, ret, .. } => {
                for param in params {
                    param.walk(visit);
                }
                ret.walk(visit);
            }
            TyKind::FnDef(fn_ref) => {
                if let Some(self_ty) = &fn_ref.self_ty {
                    self_ty.walk(visit);
                }
                walk_args(&fn_ref.args, visit);
            }
            TyKind::Projection { base, .. } => base.walk(visit),
        }
    }
}

fn kind_flags(kind: &TyKind) -> TyFlags {
    let tys = |tys: &[Ty]| {
        tys.iter()
            .fold(TyFlags::EMPTY, |flags, ty| flags.union(ty.flags()))
    };
    match kind {
        TyKind::Error => TyFlags::HAS_ERROR,
        TyKind::Infer(_) => TyFlags::HAS_INFER,
        TyKind::Param { .. } | TyKind::TraitSelf { .. } => TyFlags::HAS_PARAM,
        TyKind::Never | TyKind::Bool | TyKind::Int(_) => TyFlags::EMPTY,
        TyKind::Bounded { lo, hi } => lo.flags().union(hi.flags()),
        TyKind::Tuple(fields) => tys(fields),
        TyKind::Array { elem, len } => elem.flags().union(len.flags()),
        TyKind::Adt { args, .. } => args.flags(),
        TyKind::Ref { inner, .. } => inner.flags(),
        TyKind::FnPtr { params, ret, .. } | TyKind::Closure { params, ret, .. } => {
            tys(params).union(ret.flags())
        }
        TyKind::FnDef(fn_ref) => fn_ref
            .self_ty
            .as_ref()
            .map_or(TyFlags::EMPTY, Ty::flags)
            .union(fn_ref.args.flags()),
        TyKind::Projection { base, .. } => base.flags(),
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
