//! Building MIR from THIR, one body at a time, in the shape of rustc's MIR builder: every
//! lowering function takes the block control is in and returns the block where it
//! continues, with its result ([`BlockAnd`]).
//!
//! - `place.rs`: expressions as places;
//! - `operand.rs`: expressions as operands and temporaries;
//! - `rvalue.rs`: expressions as rvalues (arithmetic, aggregates, borrows);
//! - `into.rs`: expressions written into a destination (blocks, `if`, `&&`/`||`,
//!   assignments);
//! - `control.rs`: loops, `break`/`continue`/`return`, `?`;
//! - `call.rs`: calls and suspensions;
//! - `matches.rs`: patterns, as decision trees;
//! - `closure.rs`: the bodies of an item, its closures and its promoted constants;
//! - `cleanup.rs`: the finished CFG (dead blocks, straight-line merges, cells).

/// The block control continues in, and a result.
pub(super) struct BlockAnd<T>(BasicBlock, T);

/// `unpack!(block = expr)`: assigns the block of the `BlockAnd` `expr` to `block` and
/// yields its result.
macro_rules! unpack {
    ($block:ident = $c:expr) => {{
        let block_and = $c;
        $block = block_and.0;
        block_and.1
    }};
}

mod call;
mod cleanup;
mod closure;
mod control;
mod into;
mod matches;
mod operand;
mod place;
mod rvalue;

use la_arena::{Arena, RawIdx};
use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::eval::ConstValue;
use crate::eval::std_items::{StdTable, std_table};
use crate::hir::{ExprId, Pat, body_of};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId};
use crate::resolve::LocalId;
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::thir::{Thir, ThirExprId, ThirExprKind, ThirPatKind, thir};
use crate::ty::Ty;

use super::{
    BasicBlock, BlockData, Body, BodyKind, ConstOperand, ItemMir, Local, LocalDecl, LocalKind,
    LoopInfo, MirSource, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
};

/// The MIR of `item`'s body, its closures and its promoted constants; `None` exactly when
/// the item has no THIR.
#[salsa::tracked(returns(ref))]
pub fn item_mir(db: &dyn Db, program: Program, item: ItemId) -> Option<ItemMir> {
    let thir = thir(db, program, item).as_ref()?;
    let std = std_table(db, program).as_ref()?;
    let sigs = program_signatures(db, program);
    let cx = ItemCx::new(db, item, thir, sigs, std);
    Some(cx.build())
}

/// What every body of one item shares.
pub(super) struct ItemCx<'a> {
    item: ItemId,
    thir: &'a Thir,
    sigs: &'a ProgramSignatures,
    std: &'a StdTable,
    /// The declaration of every local the THIR binds: its type and mutability.
    bindings: FxHashMap<LocalId, LocalDecl>,
    /// The type of each closure, by index.
    closure_tys: FxHashMap<u32, Ty>,
}

impl<'a> ItemCx<'a> {
    fn new(
        db: &'a dyn Db,
        item: ItemId,
        thir: &'a Thir,
        sigs: &'a ProgramSignatures,
        std: &'a StdTable,
    ) -> ItemCx<'a> {
        let hir = body_of(db, item);
        let mut bindings = FxHashMap::default();
        for (_, pat) in thir.pats.iter() {
            let ThirPatKind::Binding { local, by_ref } = pat.kind else {
                continue;
            };
            let ty = match by_ref {
                Some(mutable) => Ty::reference(mutable, pat.ty.clone()),
                None => pat.ty.clone(),
            };
            let mutable = matches!(
                hir.map(|body| body.store.pat(pat.src)),
                Some(Pat::Bind { mutable: true, .. })
            );
            bindings.insert(
                local,
                LocalDecl {
                    ty,
                    mutable,
                    kind: LocalKind::User(local),
                },
            );
        }
        if let Some(local) = thir.self_param {
            let sig = sigs.fn_sig(item);
            bindings.insert(
                local,
                LocalDecl {
                    ty: sig
                        .and_then(|sig| sig.params.first().cloned())
                        .unwrap_or_else(Ty::error),
                    mutable: sig.is_some_and(|sig| sig.self_mutable),
                    kind: LocalKind::User(local),
                },
            );
        }
        let closure_tys = thir
            .exprs
            .iter()
            .filter_map(|(_, expr)| match expr.kind {
                ThirExprKind::Closure { index } => Some((index, expr.ty.clone())),
                _ => None,
            })
            .collect();
        ItemCx {
            item,
            thir,
            sigs,
            std,
            bindings,
            closure_tys,
        }
    }

    /// The kind of the item's own body.
    fn kind(&self) -> BodyKind {
        match self.sigs.fn_sig(self.item) {
            Some(sig) => match sig.kind {
                FnKind::Plain => BodyKind::Fn,
                FnKind::Const => BodyKind::ConstFn,
                FnKind::Pushdown if sig.scene => BodyKind::Scene,
                FnKind::Pushdown => BodyKind::Instant,
            },
            None => BodyKind::Const,
        }
    }
}

/// A block under construction.
#[derive(Default)]
struct PendingBlock {
    stmts: Vec<Statement>,
    term: Option<(Terminator, Option<ExprId>)>,
}

/// An enclosing `loop`, `while` or `for`.
struct LoopScope {
    /// The loop expression `break` and `continue` name.
    target: ExprId,
    brk: BasicBlock,
    cont: BasicBlock,
    /// Where `break` writes its value (`None`: discarded).
    dest: Option<Place>,
}

/// The builder of one body.
pub(super) struct Builder<'a> {
    cx: &'a ItemCx<'a>,
    thir: &'a Thir,
    source: MirSource,
    kind: BodyKind,
    locals: Arena<LocalDecl>,
    arg_count: u32,
    blocks: Vec<PendingBlock>,
    scopes: Vec<LoopScope>,
    local_map: FxHashMap<LocalId, Local>,
    /// The HIR expression being lowered.
    src: Option<ExprId>,
    /// The `loop`s and `while`s lowered so far (kept for `main` only).
    loops: Vec<LoopInfo>,
}

impl<'a> Builder<'a> {
    /// A builder whose return place has type `ret`; block 0 is the entry.
    fn new(cx: &'a ItemCx<'a>, source: MirSource, kind: BodyKind, ret: Ty) -> Builder<'a> {
        let mut locals = Arena::default();
        locals.alloc(LocalDecl {
            ty: ret,
            mutable: true,
            kind: LocalKind::Return,
        });
        let mut builder = Builder {
            cx,
            thir: cx.thir,
            source,
            kind,
            locals,
            arg_count: 0,
            blocks: Vec::new(),
            scopes: Vec::new(),
            local_map: FxHashMap::default(),
            src: None,
            loops: Vec::new(),
        };
        builder.new_block();
        builder
    }

    fn entry() -> BasicBlock {
        BasicBlock::from_raw(RawIdx::from_u32(0))
    }

    pub(super) fn new_block(&mut self) -> BasicBlock {
        let index = u32::try_from(self.blocks.len()).unwrap_or(u32::MAX);
        self.blocks.push(PendingBlock::default());
        BasicBlock::from_raw(RawIdx::from_u32(index))
    }

    fn pending(&mut self, block: BasicBlock) -> Option<&mut PendingBlock> {
        self.blocks.get_mut(block.into_raw().into_u32() as usize)
    }

    pub(super) fn push(&mut self, block: BasicBlock, kind: StatementKind) {
        let src = self.src;
        if let Some(pending) = self.pending(block) {
            pending.stmts.push(Statement { kind, src });
        }
    }

    pub(super) fn assign(&mut self, block: BasicBlock, place: Place, rvalue: Rvalue) {
        self.push(block, StatementKind::Assign(place, rvalue));
    }

    /// Ends `block` with `term`; a block keeps its first terminator.
    pub(super) fn terminate(&mut self, block: BasicBlock, term: Terminator) {
        let src = self.src;
        if let Some(pending) = self.pending(block)
            && pending.term.is_none()
        {
            pending.term = Some((term, src));
        }
    }

    pub(super) fn goto(&mut self, from: BasicBlock, to: BasicBlock) {
        self.terminate(from, Terminator::Goto(to));
    }

    /// Ends `block` as unreachable (an abort at run time) and returns a fresh block for
    /// the code after it, which nothing reaches.
    pub(super) fn abort(&mut self, block: BasicBlock) -> BlockAnd<()> {
        self.terminate(block, Terminator::Unreachable);
        BlockAnd(self.new_block(), ())
    }

    pub(super) fn temp(&mut self, ty: Ty) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            mutable: false,
            kind: LocalKind::Temp,
        })
    }

    /// A temporary assigned more than once (loop counters, `&&`/`||` results).
    pub(super) fn temp_mut(&mut self, ty: Ty) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            mutable: true,
            kind: LocalKind::Temp,
        })
    }

    /// The MIR local of the source local `id`, declared on first use.
    pub(super) fn local(&mut self, id: LocalId) -> Local {
        if let Some(local) = self.local_map.get(&id) {
            return *local;
        }
        let decl = self.cx.bindings.get(&id).cloned().unwrap_or(LocalDecl {
            ty: Ty::error(),
            mutable: false,
            kind: LocalKind::User(id),
        });
        let local = self.locals.alloc(decl);
        self.local_map.insert(id, local);
        local
    }

    /// Declares an argument: the source local `id` itself, or an anonymous one.
    fn arg(&mut self, id: Option<LocalId>, ty: Ty) -> Local {
        let decl = match id.and_then(|id| self.cx.bindings.get(&id)) {
            Some(decl) => LocalDecl { ty, ..decl.clone() },
            None => LocalDecl {
                ty,
                mutable: false,
                kind: id.map_or(LocalKind::Arg, LocalKind::User),
            },
        };
        let local = self.locals.alloc(decl);
        if let Some(id) = id {
            self.local_map.insert(id, local);
        }
        self.arg_count = self.arg_count.saturating_add(1);
        local
    }

    /// Declares the arguments for parameter patterns `params` of types `tys`: a plain
    /// by-value binding is its own argument; any other pattern gets an anonymous argument,
    /// returned with the pattern to destructure it once all arguments are declared.
    fn params(
        &mut self,
        params: &[crate::thir::ThirPatId],
        tys: &[Ty],
    ) -> Vec<(Local, crate::thir::ThirPatId)> {
        let mut destructure = Vec::new();
        for (index, &pat) in params.iter().enumerate() {
            let data = self.thir.pat(pat);
            let ty = tys.get(index).cloned().unwrap_or_else(|| data.ty.clone());
            match data.kind {
                ThirPatKind::Binding {
                    local,
                    by_ref: None,
                } => {
                    self.arg(Some(local), ty);
                }
                _ => {
                    let arg = self.arg(None, ty);
                    destructure.push((arg, pat));
                }
            }
        }
        destructure
    }

    /// Runs `f` with `expr`'s HIR expression as the source of what it emits.
    pub(super) fn with_src<T>(&mut self, expr: ThirExprId, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.src.replace(self.thir.expr(expr).src);
        let out = f(self);
        self.src = saved;
        out
    }

    pub(super) fn const_operand(value: ConstValue) -> Operand {
        Operand::Const(ConstOperand::Value(value))
    }

    pub(super) fn unit() -> Rvalue {
        Rvalue::Use(Self::const_operand(ConstValue::unit()))
    }

    /// Writes `()` to `dest`, if any.
    pub(super) fn assign_unit(&mut self, block: BasicBlock, dest: Option<Place>) {
        if let Some(dest) = dest {
            self.assign(block, dest, Self::unit());
        }
    }

    /// The finished body.
    fn finish(self) -> (Body, Vec<LoopInfo>) {
        let blocks: Vec<BlockData> = self
            .blocks
            .into_iter()
            .map(|pending| {
                let (term, term_src) = pending.term.unwrap_or((Terminator::Unreachable, None));
                BlockData {
                    stmts: pending.stmts,
                    term,
                    term_src,
                }
            })
            .collect();
        let (blocks, loops) = cleanup::simplify(blocks, self.loops);
        let mut arena = Arena::default();
        for block in blocks {
            arena.alloc(block);
        }
        let needs_cell = cleanup::needs_cell(&arena);
        let body = Body {
            source: self.source,
            kind: self.kind,
            locals: self.locals,
            arg_count: self.arg_count,
            blocks: arena,
            entry: Self::entry(),
            needs_cell,
        };
        (body, loops)
    }
}
