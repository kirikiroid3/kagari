//! Mid-level IR: a control-flow graph of basic blocks over locals and places, built from THIR
//! per body. Const evaluation, the story analyses and codegen all run on it.
//!
//! One [`Body`] type covers Turing-complete and pushdown code; its [`BodyKind`] says which.
//! Suspensions (`Step`, `Rng`, `SceneCall`) are block terminators and are legal only in
//! `Scene` bodies (see `validate`).

mod build;
pub mod liveness;
pub mod pretty;
pub(crate) mod validate;

pub use build::item_mir;

use la_arena::{Arena, Idx};
use smallvec::SmallVec;

use crate::eval::ConstValue;
use crate::eval::std_items::Builtin;
use crate::hir::{BinaryOp, ExprId};
use crate::item_tree::ItemId;
use crate::resolve::LocalId;
use crate::thir::PromotedId;
use crate::ty::{CtorRef, FnRef, GenericArgs, Ty};

/// What kind of code a body is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyKind {
    Fn,
    ConstFn,
    /// A `pushdown fn` that is not a scene: an instant procedure.
    Instant,
    Scene,
    /// A `const`/`static` initializer, or a promoted constant.
    Const,
}

/// Where a body comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MirSource {
    Item(ItemId),
    Closure { owner: ItemId, expr: ExprId },
    Promoted { owner: ItemId, index: PromotedId },
}

impl MirSource {
    /// The item whose HIR store the body's `ExprId`s belong to.
    pub fn owner(self) -> ItemId {
        match self {
            MirSource::Item(item)
            | MirSource::Closure { owner: item, .. }
            | MirSource::Promoted { owner: item, .. } => item,
        }
    }
}

/// The MIR of one item: its body, the bodies of its closures and of its promoted constants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemMir {
    pub main: Body,
    /// Sorted by `ExprId`.
    pub closures: Vec<(ExprId, Body)>,
    pub promoted: Vec<Body>,
    /// The root HIR expression of each promoted constant, in the order of `promoted`.
    pub promoted_src: Vec<ExprId>,
    /// The `loop` and `while` loops of `main` (not `for`).
    pub loops: Vec<LoopInfo>,
}

impl ItemMir {
    pub fn closure(&self, expr: ExprId) -> Option<&Body> {
        let index = self
            .closures
            .binary_search_by_key(&expr, |(src, _)| *src)
            .ok()?;
        self.closures.get(index).map(|(_, body)| body)
    }

    pub fn promoted(&self, index: PromotedId) -> Option<&Body> {
        self.promoted.get(index.idx())
    }

    /// The body `source` names, when it is one of this item's.
    pub fn body(&self, source: MirSource) -> Option<&Body> {
        let body = match source {
            MirSource::Item(_) => &self.main,
            MirSource::Closure { expr, .. } => self.closure(expr)?,
            MirSource::Promoted { index, .. } => self.promoted(index)?,
        };
        (body.source == source).then_some(body)
    }

    /// The promoted constant whose root is `expr`.
    pub fn promoted_of(&self, expr: ExprId) -> Option<PromotedId> {
        let index = self.promoted_src.iter().position(|src| *src == expr)?;
        PromotedId::from_usize(index)
    }

    /// Every body: `main`, then the closures, then the promoted constants.
    pub fn bodies(&self) -> impl Iterator<Item = &Body> {
        std::iter::once(&self.main)
            .chain(self.closures.iter().map(|(_, body)| body))
            .chain(self.promoted.iter())
    }
}

/// A `loop` or `while` of a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopInfo {
    pub src: ExprId,
    pub header: BasicBlock,
    /// The header plus every block allocated while lowering the loop's condition and body.
    pub region: Vec<BasicBlock>,
}

/// One body. Local `0` is the return place; locals `1..=arg_count` are the arguments (a
/// closure's captures first, then its parameters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub source: MirSource,
    pub kind: BodyKind,
    pub locals: Arena<LocalDecl>,
    pub arg_count: u32,
    pub blocks: Arena<BlockData>,
    pub entry: BasicBlock,
    /// Locals whose place is borrowed or written through a projection: they live in a heap
    /// cell at run time.
    pub needs_cell: LocalSet,
}

impl Body {
    pub const RETURN: Local = Local::from_raw(la_arena::RawIdx::from_u32(0));

    /// The argument locals, in order.
    pub fn args(&self) -> impl Iterator<Item = Local> + use<> {
        (1..=self.arg_count).map(|index| Local::from_raw(la_arena::RawIdx::from_u32(index)))
    }

    /// The declaration of `local`, if it exists.
    pub fn local_decl(&self, local: Local) -> Option<&LocalDecl> {
        ((local.into_raw().into_u32() as usize) < self.locals.len()).then(|| &self.locals[local])
    }

    /// The data of `block`, if it exists.
    pub fn block(&self, block: BasicBlock) -> Option<&BlockData> {
        ((block.into_raw().into_u32() as usize) < self.blocks.len()).then(|| &self.blocks[block])
    }
}

pub type Local = Idx<LocalDecl>;
pub type BasicBlock = Idx<BlockData>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDecl {
    pub ty: Ty,
    pub mutable: bool,
    pub kind: LocalKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalKind {
    Return,
    Arg,
    /// A local of the source; for a parameter pattern's bindings too.
    User(LocalId),
    Temp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockData {
    pub stmts: Vec<Statement>,
    pub term: Terminator,
    /// The HIR expression being lowered when the terminator was emitted.
    pub term_src: Option<ExprId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    /// The HIR expression being lowered when the statement was emitted.
    pub src: Option<ExprId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatementKind {
    Assign(Place, Rvalue),
    Call {
        dest: Place,
        callee: Callee,
        args: SmallVec<[Operand; 4]>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Callee {
    Fn(FnRef),
    Builtin(Builtin, FnRef),
    Value(Operand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub base: PlaceBase,
    pub proj: SmallVec<[Proj; 2]>,
}

impl Place {
    pub fn local(local: Local) -> Place {
        Place {
            base: PlaceBase::Local(local),
            proj: SmallVec::new(),
        }
    }

    /// The local at the base of the place, if any.
    pub fn base_local(&self) -> Option<Local> {
        match self.base {
            PlaceBase::Local(local) => Some(local),
            PlaceBase::StoryVar(_) => None,
        }
    }

    /// The local the place is, when it has no projection.
    pub fn as_local(&self) -> Option<Local> {
        if self.proj.is_empty() {
            self.base_local()
        } else {
            None
        }
    }

    pub fn project(mut self, proj: Proj) -> Place {
        self.proj.push(proj);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceBase {
    Local(Local),
    /// A `static mut` of the story.
    StoryVar(ItemId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Proj {
    Deref,
    Field(u32),
    Index(Local),
    /// The place as variant `v` of its enum, for reading the variant's fields.
    Downcast(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Copy(Place),
    Const(ConstOperand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstOperand {
    Value(ConstValue),
    Item {
        item: ItemId,
        args: GenericArgs,
    },
    /// A `const` generic parameter of `owner`.
    Param {
        owner: ItemId,
        index: u32,
    },
    Promoted(PromotedId),
    Fn(FnRef),
    /// A `Setting`/`Meta` handle: the immutable `static` it is the value of.
    Handle(ItemId),
    /// A plain immutable `static`.
    Static(ItemId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    /// Integer arithmetic of type `ty` (`Int` or `Int<lo, hi>`).
    IntBin {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
        ty: Ty,
    },
    IntNeg {
        operand: Operand,
        ty: Ty,
    },
    IntNot {
        operand: Operand,
        ty: Ty,
    },
    /// Saturates an integer to the range of `ty`.
    IntClamp {
        operand: Operand,
        ty: Ty,
    },
    BoolNot(Operand),
    BoolBin {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    DecBin {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    DecNeg(Operand),
    RatBin {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    RatNeg(Operand),
    /// A comparison: `BinaryOp::{Eq, Ne, Lt, Le, Gt, Ge}`.
    Cmp {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    Aggregate {
        kind: AggregateKind,
        ops: SmallVec<[Operand; 4]>,
    },
    Repeat {
        operand: Operand,
        count: u64,
    },
    Ref {
        mutable: bool,
        place: Place,
    },
    /// The length of an array or `Vec`.
    Len(Place),
    /// A closure value; the captures in the order of the closure body's leading arguments.
    Closure {
        expr: ExprId,
        captures: SmallVec<[Operand; 4]>,
    },
    Cast {
        operand: Operand,
        ty: Ty,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    Tuple,
    Array,
    Adt(CtorRef),
}

// One terminator per block, so the size of `SceneCall` does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Goto(BasicBlock),
    /// `short_circuit` is the result local of `&&`/`||`: the branch only decides that value.
    SwitchBool {
        cond: Operand,
        then: BasicBlock,
        other: BasicBlock,
        short_circuit: Option<Local>,
    },
    SwitchVariant {
        place: Place,
        adt: ItemId,
        targets: SmallVec<[(u32, BasicBlock); 4]>,
        otherwise: Option<BasicBlock>,
    },
    Return,
    /// Cannot happen in a well-typed program.
    Unreachable,
    Step {
        arg: Operand,
        dest: Place,
        next: BasicBlock,
    },
    Rng {
        ty: Ty,
        dest: Place,
        next: BasicBlock,
    },
    SceneCall {
        callee: Callee,
        args: SmallVec<[Operand; 4]>,
        dest: Place,
        next: BasicBlock,
    },
}

impl Terminator {
    /// The blocks control may continue at.
    pub fn successors(&self) -> SmallVec<[BasicBlock; 4]> {
        match self {
            Terminator::Goto(target) => SmallVec::from_elem(*target, 1),
            Terminator::SwitchBool { then, other, .. } => SmallVec::from_slice(&[*then, *other]),
            Terminator::SwitchVariant {
                targets, otherwise, ..
            } => targets
                .iter()
                .map(|(_, block)| *block)
                .chain(*otherwise)
                .collect(),
            Terminator::Return | Terminator::Unreachable => SmallVec::new(),
            Terminator::Step { next, .. }
            | Terminator::Rng { next, .. }
            | Terminator::SceneCall { next, .. } => SmallVec::from_elem(*next, 1),
        }
    }

    /// Whether the terminator suspends the story (legal only in `Scene` bodies).
    pub fn is_suspension(&self) -> bool {
        matches!(
            self,
            Terminator::Step { .. } | Terminator::Rng { .. } | Terminator::SceneCall { .. }
        )
    }
}

/// A set of locals as a bitset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LocalSet(Vec<u64>);

impl LocalSet {
    fn split(local: Local) -> (usize, u32) {
        let index = local.into_raw().into_u32();
        ((index / 64) as usize, index % 64)
    }

    /// Adds `local`; returns whether it was new.
    pub fn insert(&mut self, local: Local) -> bool {
        let (word, bit) = Self::split(local);
        if self.0.len() <= word {
            self.0.resize(word.saturating_add(1), 0);
        }
        let Some(slot) = self.0.get_mut(word) else {
            return false;
        };
        let mask = 1u64 << bit;
        let new = *slot & mask == 0;
        *slot |= mask;
        new
    }

    pub fn remove(&mut self, local: Local) {
        let (word, bit) = Self::split(local);
        if let Some(slot) = self.0.get_mut(word) {
            *slot &= !(1u64 << bit);
        }
    }

    pub fn contains(&self, local: Local) -> bool {
        let (word, bit) = Self::split(local);
        self.0
            .get(word)
            .is_some_and(|slot| slot & (1u64 << bit) != 0)
    }

    /// Adds every local of `other`; returns whether `self` changed.
    pub fn union(&mut self, other: &LocalSet) -> bool {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        let mut changed = false;
        for (slot, bits) in self.0.iter_mut().zip(&other.0) {
            let next = *slot | bits;
            changed |= next != *slot;
            *slot = next;
        }
        changed
    }

    /// The locals in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = Local> + '_ {
        self.0.iter().enumerate().flat_map(|(word, bits)| {
            (0..64u32)
                .filter(move |bit| bits & (1u64 << bit) != 0)
                .filter_map(move |bit| {
                    let index = u32::try_from(word)
                        .ok()?
                        .checked_mul(64)?
                        .checked_add(bit)?;
                    Some(Local::from_raw(la_arena::RawIdx::from_u32(index)))
                })
        })
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|bits| *bits == 0)
    }
}
