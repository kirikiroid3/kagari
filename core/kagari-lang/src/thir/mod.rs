//! Typed HIR: one item's body as a tree with names resolved, types attached and the
//! implicit operations of inference made explicit (coercions, auto-derefs and auto-borrows
//! of receivers and field bases, constructor and range literals). MIR building reads only
//! this tree.

mod build;

use kagari_num::Decimal;
use la_arena::{Arena, Idx};

use crate::callee::CalleeKind;
use crate::eval::std_items::Builtin;
use crate::eval::{ConstValue, Text};
use crate::hir::{BinaryOp, ClosureKind, ExprId, Literal, PatId, UnaryOp};
use crate::infer::Coercion;
use crate::item_tree::ItemId;
use crate::resolve::LocalId;
use crate::ty::{CtorRef, FnRef, GenericArgs, Ty};

pub use build::thir;

pub type ThirExprId = Idx<ThirExpr>;
pub type ThirPatId = Idx<ThirPat>;

crate::item_tree::index_newtype!(
    /// An index into [`Thir::promoted`].
    PromotedId
);

/// The typed tree of one item's body (a `fn`, or the initializer of a `const` or `static`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thir {
    pub exprs: Arena<ThirExpr>,
    pub pats: Arena<ThirPat>,
    /// The `self` parameter of a method: always the first argument, bound to this local.
    pub self_param: Option<LocalId>,
    /// The other parameters' patterns, in order.
    pub params: Vec<ThirPatId>,
    pub root: ThirExprId,
    /// Every closure of the body; [`ThirExprKind::Closure`] indexes this list.
    pub closures: Vec<ThirClosure>,
    /// The maximal constant subexpressions of a `pushdown fn` body that const evaluation
    /// computes ahead of time; [`ThirExprKind::Promoted`] indexes this list.
    pub promoted: Vec<ThirPromoted>,
}

impl Thir {
    pub fn expr(&self, id: ThirExprId) -> &ThirExpr {
        &self.exprs[id]
    }

    pub fn pat(&self, id: ThirPatId) -> &ThirPat {
        &self.pats[id]
    }

    pub fn closure(&self, index: u32) -> Option<&ThirClosure> {
        self.closures.get(usize::try_from(index).ok()?)
    }

    pub fn promoted(&self, id: PromotedId) -> Option<&ThirPromoted> {
        self.promoted.get(id.idx())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThirExpr {
    pub kind: ThirExprKind,
    pub ty: Ty,
    /// The HIR expression this node comes from; synthesized nodes (adjustments, the
    /// `inclusive` field of a range) carry the expression they adjust or belong to.
    pub src: ExprId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThirClosure {
    pub src: ExprId,
    pub kind: ClosureKind,
    /// The captured locals of enclosing scopes, in order of first use.
    pub captures: Vec<LocalId>,
    pub params: Vec<ThirPatId>,
    pub body: ThirExprId,
    pub ret: Ty,
}

/// A promoted constant: `root` evaluated after binding `const_locals`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThirPromoted {
    pub src: ExprId,
    pub root: ThirExprId,
    /// `let pat = source;` for every constant local of the enclosing body that `root`
    /// uses, in dependency order (a local's sources come before its own binding). The
    /// pattern may be refutable (an `if let` or arm pattern): a mismatch cannot happen,
    /// since the local is only reachable where it matched.
    pub const_locals: Vec<(ThirPatId, ThirExprId)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThirCallee {
    Direct(FnRef),
    /// A `#[builtin]` function (const or runtime).
    Builtin(Builtin, FnRef),
    /// A function pointer, closure or function value computed by an expression.
    Value(ThirExprId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThirCond {
    Expr(ThirExprId),
    Let { pat: ThirPatId, expr: ThirExprId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForKind {
    Array,
    Vec,
    /// `inclusive` is `Some` when the iterable is a range expression; otherwise the range
    /// value's `inclusive` field decides at runtime.
    Range {
        inclusive: Option<bool>,
    },
}

/// `?` on an `Option` or `Result`: the variant indices of success and failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TryKind {
    Option { some: u32, none: u32 },
    Result { ok: u32, err: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThirArm {
    pub pat: ThirPatId,
    pub guard: Option<ThirExprId>,
    pub body: ThirExprId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThirExprKind {
    Literal(ConstValue),
    /// A `const` item; `args` are the parent impl's generic arguments of an associated
    /// constant.
    ConstItem {
        item: ItemId,
        args: GenericArgs,
    },
    ConstParam {
        owner: ItemId,
        index: u32,
    },
    /// A named function or tuple constructor used as a value.
    FnItem(FnRef),
    /// A `Setting`/`Meta` handle `static`: its value is its identity.
    Handle(ItemId),
    /// Any other immutable `static`.
    StaticValue(ItemId),
    /// A `static mut` (a story variable): a place.
    StoryVar(ItemId),
    Local(LocalId),
    Promoted(PromotedId),
    Block {
        stmts: Vec<ThirStmt>,
        tail: Option<ThirExprId>,
    },
    If {
        cond: ThirCond,
        then: ThirExprId,
        else_: Option<ThirExprId>,
    },
    Match {
        scrutinee: ThirExprId,
        arms: Vec<ThirArm>,
    },
    Loop {
        body: ThirExprId,
    },
    While {
        cond: ThirCond,
        body: ThirExprId,
    },
    For {
        pat: ThirPatId,
        iterable: ThirExprId,
        body: ThirExprId,
        kind: ForKind,
    },
    /// `target` is the `loop`, `while` or `for` expression left.
    Break {
        target: ExprId,
        value: Option<ThirExprId>,
    },
    Continue {
        target: ExprId,
    },
    Return {
        value: Option<ThirExprId>,
    },
    /// A call; a method call's receiver is the first argument, already adjusted.
    Call {
        callee: ThirCallee,
        args: Vec<ThirExprId>,
        kind: CalleeKind,
    },
    Field {
        base: ThirExprId,
        index: u32,
    },
    Index {
        base: ThirExprId,
        index: ThirExprId,
    },
    Deref(ThirExprId),
    Borrow {
        mutable: bool,
        place: ThirExprId,
    },
    /// `-` or `!`; `*` is [`ThirExprKind::Deref`].
    Unary {
        op: UnaryOp,
        expr: ThirExprId,
    },
    /// Every binary operator but `&&` and `||`.
    Binary {
        op: BinaryOp,
        lhs: ThirExprId,
        rhs: ThirExprId,
    },
    /// `&&` (`and`) or `||`.
    Logical {
        and: bool,
        lhs: ThirExprId,
        rhs: ThirExprId,
    },
    Assign {
        target: ThirExprId,
        value: ThirExprId,
    },
    /// `target op= value`; `saturate` when the target is an `Int<lo, hi>`, whose compound
    /// assignment clamps to its bounds.
    AssignOp {
        op: BinaryOp,
        target: ThirExprId,
        value: ThirExprId,
        saturate: bool,
    },
    Try {
        expr: ThirExprId,
        kind: TryKind,
    },
    Tuple(Vec<ThirExprId>),
    Array(Vec<ThirExprId>),
    Repeat {
        value: ThirExprId,
        count: u64,
    },
    /// A struct or variant built from fields: `(field index, value)` in source order.
    Adt {
        ctor: CtorRef,
        fields: Vec<(u32, ThirExprId)>,
    },
    /// [`Thir::closures`]`[index]`.
    Closure {
        index: u32,
    },
    /// `expr as ty`.
    Cast {
        expr: ThirExprId,
        ty: Ty,
    },
    /// A value-preserving conversion to this node's type.
    Coerce {
        kind: Coercion,
        expr: ThirExprId,
    },
    /// Something already reported, or that cannot be evaluated.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThirStmt {
    Let {
        pat: ThirPatId,
        init: Option<ThirExprId>,
    },
    Expr(ThirExprId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThirPat {
    pub kind: ThirPatKind,
    /// The type the pattern matches once `derefs` references are followed (for a binding,
    /// a wildcard or an or-pattern: the type at its position).
    pub ty: Ty,
    /// The pattern of the source; for the wildcards a `..` expands to, the `..`.
    pub src: PatId,
    /// The references followed from the matched value before the test: the `TyKind::Ref`
    /// layers of the type at this position. 0 for wildcards, bindings and or-patterns
    /// (whose alternatives follow references themselves).
    pub derefs: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThirPatKind {
    Wild,
    /// `by_ref` is `Some(mutable)` when the binding is under a pattern that followed a
    /// reference: it binds a reference to the matched place.
    Binding {
        local: LocalId,
        by_ref: Option<bool>,
    },
    Lit(ConstValue),
    /// A path naming a `const` item: equal to its value.
    Const(ItemId),
    Range {
        lo: Option<RangeBound>,
        hi: Option<RangeBound>,
        inclusive: bool,
    },
    /// Exactly one pattern per field.
    Tuple(Vec<ThirPatId>),
    /// A struct (`variant: None`) or enum variant; `(field index, pattern)` pairs.
    Adt {
        adt: ItemId,
        variant: Option<u32>,
        fields: Vec<(u32, ThirPatId)>,
    },
    Or(Vec<ThirPatId>),
    /// A missing or unresolved pattern: matching it aborts.
    Error,
}

/// A bound of a range pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeBound {
    Value(ConstValue),
    Const(ItemId),
}

/// The value of a literal, `negated` for a negative literal pattern; `None` when it does
/// not fit.
pub(crate) fn literal_value(literal: &Literal, negated: bool) -> Option<ConstValue> {
    Some(match literal {
        Literal::Int { value, .. } => {
            let value = i128::try_from(*value).ok()?;
            ConstValue::Int(if negated { value.checked_neg()? } else { value })
        }
        Literal::Decimal(text) => {
            let value = Decimal::parse(text)?;
            ConstValue::Decimal(if negated { value.checked_neg()? } else { value })
        }
        Literal::String(text) => ConstValue::Text(Text::plain(text.clone())),
        Literal::Bool(value) => ConstValue::Bool(*value),
    })
}
