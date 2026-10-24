//! Scene bodies lowered to control-flow graphs whose suspension points are the steps and
//! the scene calls.
//!
//! Only what needs it is lowered: an expression that contains a step, a scene call, a
//! `loop` or a `while` (loops are kept in the graph for the termination check). Every other
//! expression is *atomic*: the const-evaluation interpreter runs it in story mode. When an
//! atomic expression has a lowered operand (`foo(x, Confession())`), the operands up to the
//! last lowered one are evaluated first into temporaries, in evaluation order, and the
//! interpreter reads them back through `Frame::overrides`. Operands evaluated as places
//! (assignment targets, `&` operands, borrowed receivers) and closures are left in place.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eval::ConstValue;
use crate::eval::const_exprs::children;
use crate::eval::interp::BodyRef;
use crate::hir::{BinaryOp, Condition, Expr, ExprId, FnPtrKind, Pat, PatId, Stmt, UnaryOp};
use crate::infer::ValueRef;
use crate::item_tree::{FnKind, ItemId};
use crate::resolve::LocalId;
use crate::signatures::ProgramSignatures;
use crate::ty::{FnDefId, FnRef, Ty};

pub(super) type BlockId = usize;
pub(super) type Slot = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotKind {
    /// A local of the body. `by_ref` (`Some(mutable)`) for a reference parameter: the slot
    /// holds the value it refers to, which the scene receives as a copy and, for `&mut`,
    /// hands back on return (`docs/syntax/src/values.md#references-in-the-story`).
    Local {
        local: LocalId,
        by_ref: Option<bool>,
    },
    /// The value of a subexpression evaluated before its parent.
    Temp,
}

#[derive(Clone, Debug)]
pub(super) enum Instr {
    /// Evaluates `expr` into `dest`; `overrides` are its operands already evaluated.
    Eval {
        expr: ExprId,
        dest: Option<Slot>,
        overrides: Vec<(ExprId, Slot)>,
    },
    /// Evaluates `expr`, a reference, into the value it refers to: a reference argument of
    /// a scene call, copied in.
    EvalDeref {
        expr: ExprId,
        dest: Slot,
        overrides: Vec<(ExprId, Slot)>,
    },
    /// The `self` argument of a scene method: `expr` dereferenced `derefs` times, read from
    /// the place it denotes when the call borrows it (`by_place`).
    Receiver {
        expr: ExprId,
        dest: Slot,
        derefs: u32,
        by_place: bool,
        overrides: Vec<(ExprId, Slot)>,
    },
    /// Binds the irrefutable `pat` to the value in `src`.
    Bind {
        pat: PatId,
        src: Slot,
    },
    /// `let pat;`.
    Declare {
        pat: PatId,
    },
    Const {
        dest: Slot,
        value: ConstValue,
    },
    /// Turns the range or array in `slot` into the array of its items.
    ForInit {
        slot: Slot,
    },
    /// Writes `src` back to the place of a `&mut` argument of a scene call, the place
    /// resolved with the same `overrides` as when it was copied in.
    WriteBack {
        expr: ExprId,
        src: Slot,
        mode: WriteMode,
        overrides: Vec<(ExprId, Slot)>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum WriteMode {
    /// Through the reference `expr` evaluates to.
    ThroughRef,
    /// Into the place `expr` denotes, dereferenced `derefs` times (a borrowed receiver).
    Place { derefs: u32 },
}

#[derive(Clone, Debug)]
pub(super) enum Callee {
    Direct(FnRef),
    /// A `scene fn` pointer value and its type.
    Value {
        slot: Slot,
        ty: Ty,
    },
}

#[derive(Clone, Debug)]
pub(super) enum Term {
    Goto(BlockId),
    If {
        cond: Slot,
        then: BlockId,
        other: BlockId,
    },
    /// Matches the refutable `pat` against `src`, binding its locals on success.
    Test {
        pat: PatId,
        src: Slot,
        then: BlockId,
        other: BlockId,
    },
    /// The next item of a `for` loop (`iter` holds the remaining ones), bound to `pat`.
    ForNext {
        iter: Slot,
        pat: PatId,
        body: BlockId,
        exit: BlockId,
    },
    /// `push_step(arg)`, the call `expr`; resumes at `next` with the output in `dest`.
    Step {
        expr: ExprId,
        arg: Slot,
        dest: Option<Slot>,
        next: BlockId,
    },
    /// A scene call; resumes at `next` with the return value in `dest` and the final
    /// values of the callee's `&mut` parameters in `outs`.
    Call {
        expr: ExprId,
        callee: Callee,
        args: Vec<Slot>,
        outs: Vec<Slot>,
        dest: Option<Slot>,
        next: BlockId,
    },
    Return {
        value: Slot,
    },
    Unreachable,
}

#[derive(Clone, Debug)]
pub(super) struct Block {
    pub instrs: Vec<Instr>,
    /// Per instruction, the blocks an atomic `break`, `continue`, `return` or `?` inside it
    /// can jump to: edges of the graph like the terminator's.
    pub exits: Vec<Vec<BlockId>>,
    pub term: Term,
}

/// Where control goes when an atomic expression leaves a lowered loop.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopExit {
    pub brk: BlockId,
    pub cont: BlockId,
    pub dest: Option<Slot>,
}

/// A set of slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SlotSet(Vec<u64>);

impl SlotSet {
    pub fn with_len(len: usize) -> SlotSet {
        SlotSet(vec![0; len.div_ceil(64)])
    }

    pub fn contains(&self, slot: Slot) -> bool {
        self.0
            .get(slot / 64)
            .is_some_and(|word| word & (1 << (slot % 64)) != 0)
    }

    pub fn insert(&mut self, slot: Slot) {
        if let Some(word) = self.0.get_mut(slot / 64) {
            *word |= 1 << (slot % 64);
        }
    }

    pub fn remove(&mut self, slot: Slot) {
        if let Some(word) = self.0.get_mut(slot / 64) {
            *word &= !(1 << (slot % 64));
        }
    }

    pub fn union(&mut self, other: &SlotSet) {
        for (word, other) in self.0.iter_mut().zip(&other.0) {
            *word |= other;
        }
    }
}

/// A scene body as a control-flow graph.
#[derive(Clone, Debug)]
pub(super) struct StoryBody {
    pub slots: Vec<SlotKind>,
    pub blocks: Vec<Block>,
    pub entry: BlockId,
    /// The slots the arguments go to, in order (`self` first).
    pub params: Vec<Slot>,
    /// The slots of the `&mut` parameters, in order: what a return hands back.
    pub out_params: Vec<Slot>,
    /// Where `return` stores its value, and the block that returns it.
    pub ret: Slot,
    pub ret_block: BlockId,
    pub loop_exits: FxHashMap<ExprId, LoopExit>,
    /// Every `loop` and `while` with its header block and the blocks of its body (the
    /// header included, the exit not).
    pub loops: Vec<(ExprId, BlockId, FxHashSet<BlockId>)>,
    /// The slots live when the story is suspended at a step or call block.
    pub kept: FxHashMap<BlockId, SlotSet>,
}

impl StoryBody {
    /// Every block a terminator can go to.
    pub fn successors(term: &Term) -> Vec<BlockId> {
        match term {
            Term::Goto(next) => vec![*next],
            Term::If { then, other, .. } | Term::Test { then, other, .. } => vec![*then, *other],
            Term::ForNext { body, exit, .. } => vec![*body, *exit],
            Term::Step { next, .. } | Term::Call { next, .. } => vec![*next],
            Term::Return { .. } | Term::Unreachable => Vec::new(),
        }
    }
}

/// The scene `callee` of a call expression, if it calls one.
#[derive(Clone, Debug)]
enum SceneCall {
    Direct(FnRef),
    Pointer(ExprId),
    Method(FnRef),
}

pub(super) fn lower(b: BodyRef<'_>, sigs: &ProgramSignatures, push_step: ItemId) -> StoryBody {
    let mut lowerer = Lowerer {
        b,
        sigs,
        push_step,
        structural: FxHashSet::default(),
        body: StoryBody {
            slots: Vec::new(),
            blocks: Vec::new(),
            entry: 0,
            params: Vec::new(),
            out_params: Vec::new(),
            ret: 0,
            ret_block: 0,
            loop_exits: FxHashMap::default(),
            loops: Vec::new(),
            kept: FxHashMap::default(),
        },
        local_slots: FxHashMap::default(),
        cur: None,
    };
    lowerer.mark_structural(b.body.value);
    lowerer.lower_body();
    let mut body = lowerer.body;
    let uses = Uses {
        b,
        local_slots: &lowerer.local_slots,
    };
    body.kept = liveness(&body, &uses);
    body
}

struct Lowerer<'a, 'db> {
    b: BodyRef<'db>,
    sigs: &'a ProgramSignatures,
    push_step: ItemId,
    /// Expressions that contain a step, a scene call, a `loop` or a `while`.
    structural: FxHashSet<ExprId>,
    body: StoryBody,
    local_slots: FxHashMap<LocalId, Slot>,
    cur: Option<BlockId>,
}

impl Lowerer<'_, '_> {
    fn expr(&self, expr: ExprId) -> &Expr {
        self.b.body.store.expr(expr)
    }

    fn mark_structural(&mut self, root: ExprId) {
        // Post-order without recursion: bodies can nest deeply.
        let mut stack = vec![(root, false)];
        while let Some((expr, expanded)) = stack.pop() {
            let data = self.expr(expr);
            if matches!(data, Expr::Closure { .. }) {
                // A closure in a scene is an instant procedure or a render.
                continue;
            }
            if !expanded {
                stack.push((expr, true));
                for child in children(data) {
                    stack.push((child, false));
                }
                continue;
            }
            let own = matches!(data, Expr::Loop { .. } | Expr::While { .. })
                || self.is_step(expr)
                || self.scene_call(expr).is_some();
            if own || children(data).iter().any(|c| self.structural.contains(c)) {
                self.structural.insert(expr);
            }
        }
    }

    fn is_step(&self, expr: ExprId) -> bool {
        let Expr::Call { callee, args } = self.expr(expr) else {
            return false;
        };
        args.len() == 1
            && matches!(
                self.b.infer.paths.get(callee),
                Some(ValueRef::Fn(FnRef { def: FnDefId::Item(item), .. })) if *item == self.push_step
            )
    }

    fn is_scene(&self, item: ItemId) -> bool {
        self.sigs
            .fn_sig(item)
            .is_some_and(|sig| sig.scene && sig.kind == FnKind::Pushdown)
    }

    fn scene_call(&self, expr: ExprId) -> Option<SceneCall> {
        match self.expr(expr) {
            Expr::Call { callee, .. } => match self.b.infer.paths.get(callee) {
                Some(ValueRef::Fn(fn_ref)) => match fn_ref.def {
                    FnDefId::Item(item) if self.is_scene(item) => {
                        Some(SceneCall::Direct(fn_ref.clone()))
                    }
                    _ => None,
                },
                _ => match self.b.infer.expr_types.get(*callee) {
                    Some(Ty::FnPtr {
                        kind: FnPtrKind::Scene,
                        ..
                    }) => Some(SceneCall::Pointer(*callee)),
                    Some(Ty::FnDef(fn_ref)) => match fn_ref.def {
                        FnDefId::Item(item) if self.is_scene(item) => {
                            Some(SceneCall::Direct((**fn_ref).clone()))
                        }
                        _ => None,
                    },
                    _ => None,
                },
            },
            Expr::MethodCall { .. } => {
                let call = self.b.infer.method_calls.get(&expr)?;
                match call.func.def {
                    FnDefId::Item(item) if self.is_scene(item) => {
                        Some(SceneCall::Method(call.func.clone()))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // ----- Building blocks ----------------------------------------------------------

    fn new_block(&mut self) -> BlockId {
        self.body.blocks.push(Block {
            instrs: Vec::new(),
            exits: Vec::new(),
            term: Term::Unreachable,
        });
        self.body.blocks.len().saturating_sub(1)
    }

    /// The block being filled; after a jump, a fresh unreachable one.
    fn cur(&mut self) -> BlockId {
        match self.cur {
            Some(block) => block,
            None => {
                let block = self.new_block();
                self.cur = Some(block);
                block
            }
        }
    }

    fn emit(&mut self, instr: Instr) {
        let exits = self.exits(&instr);
        let block = self.cur();
        if let Some(block) = self.body.blocks.get_mut(block) {
            block.instrs.push(instr);
            block.exits.push(exits);
        }
    }

    /// Records a lowered `loop` or `while`: every block created since its header belongs to
    /// it, except its exit.
    fn record_loop(&mut self, expr: ExprId, header: BlockId, exit: BlockId) {
        let region = (header..self.body.blocks.len())
            .filter(|block| *block != exit)
            .collect();
        self.body.loops.push((expr, header, region));
    }

    /// The blocks an atomic instruction can leave to through `break`, `continue`,
    /// `return` or `?`: enclosing lowered loops and the function's return.
    fn exits(&self, instr: &Instr) -> Vec<BlockId> {
        let (root, skip) = match instr {
            Instr::Eval {
                expr, overrides, ..
            }
            | Instr::EvalDeref {
                expr, overrides, ..
            }
            | Instr::Receiver {
                expr, overrides, ..
            }
            | Instr::WriteBack {
                expr, overrides, ..
            } => (*expr, overrides.as_slice()),
            _ => return Vec::new(),
        };
        let store = &self.b.body.store;
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(expr) = stack.pop() {
            if skip.iter().any(|(skipped, _)| *skipped == expr) {
                continue;
            }
            let data = store.expr(expr);
            let target = match data {
                Expr::Closure { .. } => continue,
                Expr::Break { .. } | Expr::Continue { .. } => self
                    .b
                    .res
                    .loop_targets
                    .get(&expr)
                    .and_then(|target| self.body.loop_exits.get(target))
                    .map(|exit| {
                        if matches!(data, Expr::Break { .. }) {
                            exit.brk
                        } else {
                            exit.cont
                        }
                    }),
                Expr::Return { .. } | Expr::Try(_) => Some(self.body.ret_block),
                _ => None,
            };
            if let Some(target) = target
                && !out.contains(&target)
            {
                out.push(target);
            }
            stack.extend(children(data));
        }
        out
    }

    fn terminate(&mut self, term: Term) {
        let block = self.cur();
        if let Some(block) = self.body.blocks.get_mut(block) {
            block.term = term;
        }
        self.cur = None;
    }

    fn goto(&mut self, target: BlockId) {
        self.terminate(Term::Goto(target));
    }

    fn temp(&mut self) -> Slot {
        self.body.slots.push(SlotKind::Temp);
        self.body.slots.len().saturating_sub(1)
    }

    fn local_slot(&mut self, local: LocalId, by_ref: Option<bool>) -> Slot {
        if let Some(slot) = self.local_slots.get(&local) {
            return *slot;
        }
        self.body.slots.push(SlotKind::Local { local, by_ref });
        let slot = self.body.slots.len().saturating_sub(1);
        self.local_slots.insert(local, slot);
        slot
    }

    /// Gives every local `pat` binds a slot.
    fn bind_slots(&mut self, pat: PatId) {
        for local in pat_locals(self.b, pat) {
            self.local_slot(local, None);
        }
    }

    fn set_unit(&mut self, dest: Option<Slot>) {
        if let Some(dest) = dest {
            self.emit(Instr::Const {
                dest,
                value: ConstValue::unit(),
            });
        }
    }

    // ----- Lowering -----------------------------------------------------------------

    fn lower_body(&mut self) {
        let entry = self.new_block();
        self.body.entry = entry;
        self.cur = Some(entry);
        let item = self.b.item;
        let param_tys: Vec<Ty> = self
            .sigs
            .fn_sig(item)
            .map(|sig| sig.params.clone())
            .unwrap_or_default();
        let by_ref = |index: usize| match param_tys.get(index) {
            Some(Ty::Ref { mutable, .. }) => Some(*mutable),
            _ => None,
        };
        let mut index = 0usize;
        if self.b.body.has_self
            && let Some(local) = self
                .b
                .res
                .locals
                .iter()
                .find(|(_, local)| local.pat.is_none())
                .map(|(id, _)| id)
        {
            let slot = self.local_slot(local, by_ref(index));
            self.body.params.push(slot);
            if by_ref(index) == Some(true) {
                self.body.out_params.push(slot);
            }
            index = index.saturating_add(1);
        }
        let params = self.b.body.params.clone();
        for pat in params {
            let param_ref = by_ref(index);
            let slot = match (self.b.body.store.pat(pat), self.b.res.bindings.get(&pat)) {
                (Pat::Bind { .. }, Some(local)) => self.local_slot(*local, param_ref),
                _ => {
                    let temp = self.temp();
                    self.bind_slots(pat);
                    self.emit(Instr::Bind { pat, src: temp });
                    temp
                }
            };
            self.body.params.push(slot);
            if param_ref == Some(true) {
                self.body.out_params.push(slot);
            }
            index = index.saturating_add(1);
        }
        let ret = self.temp();
        self.body.ret = ret;
        let ret_block = self.new_block();
        self.body.ret_block = ret_block;
        if let Some(block) = self.body.blocks.get_mut(ret_block) {
            block.term = Term::Return { value: ret };
        }
        self.lower_expr(self.b.body.value, Some(ret));
        self.goto(ret_block);
    }

    fn atomic(&mut self, expr: ExprId, dest: Option<Slot>) {
        self.emit(Instr::Eval {
            expr,
            dest,
            overrides: Vec::new(),
        });
        // An expression that always leaves (`break 1`, `{ return; }`, …) never falls
        // through: its exits are the only edges out.
        if self.diverges(expr) {
            self.terminate(Term::Unreachable);
        }
    }

    /// Whether `expr` always leaves through `break`, `continue` or `return`.
    fn diverges(&self, expr: ExprId) -> bool {
        if matches!(self.b.infer.expr_types.get(expr), Some(Ty::Never)) {
            return true;
        }
        match self.expr(expr) {
            Expr::Break { .. } | Expr::Continue { .. } | Expr::Return { .. } => true,
            Expr::Block { stmts, tail, .. } => {
                stmts.iter().any(|stmt| match stmt {
                    Stmt::Expr { expr, .. } => self.diverges(*expr),
                    Stmt::Let {
                        init: Some(init), ..
                    } => self.diverges(*init),
                    _ => false,
                }) || tail.is_some_and(|tail| self.diverges(tail))
            }
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => self.diverges(*then_branch) && self.diverges(*else_branch),
            Expr::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.diverges(arm.body))
            }
            _ => false,
        }
    }

    fn lower_expr(&mut self, expr: ExprId, dest: Option<Slot>) {
        if !self.structural.contains(&expr) {
            self.atomic(expr, dest);
            return;
        }
        let data = self.expr(expr).clone();
        match data {
            Expr::Block { stmts, tail, .. } => {
                for stmt in &stmts {
                    self.lower_stmt(stmt);
                }
                match tail {
                    Some(tail) => self.lower_expr(tail, dest),
                    None => self.set_unit(dest),
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join = self.new_block();
                self.condition(&cond, then_block, else_block);
                self.cur = Some(then_block);
                self.lower_expr(then_branch, dest);
                self.goto(join);
                self.cur = Some(else_block);
                match else_branch {
                    Some(else_branch) => self.lower_expr(else_branch, dest),
                    None => self.set_unit(dest),
                }
                self.goto(join);
                self.cur = Some(join);
            }
            Expr::Match { scrutinee, arms } => {
                let src = self.temp();
                self.lower_expr(scrutinee, Some(src));
                let join = self.new_block();
                for arm in &arms {
                    let body = self.new_block();
                    let next = self.new_block();
                    self.bind_slots(arm.pat);
                    match arm.guard {
                        Some(guard) => {
                            let guard_block = self.new_block();
                            self.terminate(Term::Test {
                                pat: arm.pat,
                                src,
                                then: guard_block,
                                other: next,
                            });
                            self.cur = Some(guard_block);
                            let cond = self.temp();
                            self.lower_expr(guard, Some(cond));
                            self.terminate(Term::If {
                                cond,
                                then: body,
                                other: next,
                            });
                        }
                        None => self.terminate(Term::Test {
                            pat: arm.pat,
                            src,
                            then: body,
                            other: next,
                        }),
                    }
                    self.cur = Some(body);
                    self.lower_expr(arm.body, dest);
                    self.goto(join);
                    self.cur = Some(next);
                }
                // Stage 3 checked exhaustiveness.
                self.terminate(Term::Unreachable);
                self.cur = Some(join);
            }
            Expr::Loop { body, .. } => {
                let header = self.new_block();
                let exit = self.new_block();
                self.body.loop_exits.insert(
                    expr,
                    LoopExit {
                        brk: exit,
                        cont: header,
                        dest,
                    },
                );
                self.goto(header);
                self.cur = Some(header);
                self.lower_expr(body, None);
                self.goto(header);
                self.record_loop(expr, header, exit);
                self.cur = Some(exit);
            }
            Expr::While { cond, body, .. } => {
                let header = self.new_block();
                let body_block = self.new_block();
                let exit = self.new_block();
                self.body.loop_exits.insert(
                    expr,
                    LoopExit {
                        brk: exit,
                        cont: header,
                        dest: None,
                    },
                );
                self.goto(header);
                self.cur = Some(header);
                self.condition(&cond, body_block, exit);
                self.cur = Some(body_block);
                self.lower_expr(body, None);
                self.goto(header);
                self.record_loop(expr, header, exit);
                self.cur = Some(exit);
                self.set_unit(dest);
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => {
                let iter = self.temp();
                self.lower_expr(iterable, Some(iter));
                self.emit(Instr::ForInit { slot: iter });
                let header = self.new_block();
                let body_block = self.new_block();
                let exit = self.new_block();
                self.body.loop_exits.insert(
                    expr,
                    LoopExit {
                        brk: exit,
                        cont: header,
                        dest: None,
                    },
                );
                self.goto(header);
                self.cur = Some(header);
                self.bind_slots(pat);
                self.terminate(Term::ForNext {
                    iter,
                    pat,
                    body: body_block,
                    exit,
                });
                self.cur = Some(body_block);
                self.lower_expr(body, None);
                self.goto(header);
                self.cur = Some(exit);
                self.set_unit(dest);
            }
            Expr::Break { value, .. } => {
                let exit = self
                    .b
                    .res
                    .loop_targets
                    .get(&expr)
                    .and_then(|target| self.body.loop_exits.get(target))
                    .copied();
                let Some(exit) = exit else {
                    self.terminate(Term::Unreachable);
                    return;
                };
                match value {
                    Some(value) => self.lower_expr(value, exit.dest),
                    None => self.set_unit(exit.dest),
                }
                self.goto(exit.brk);
            }
            Expr::Return { value } => {
                let ret = self.body.ret;
                match value {
                    Some(value) => self.lower_expr(value, Some(ret)),
                    None => self.set_unit(Some(ret)),
                }
                let ret_block = self.body.ret_block;
                self.goto(ret_block);
            }
            Expr::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                lhs,
                rhs,
            } if self.structural.contains(&rhs) => {
                let left = self.temp();
                self.lower_expr(lhs, Some(left));
                let rhs_block = self.new_block();
                let short = self.new_block();
                let join = self.new_block();
                let (then, other) = if op == BinaryOp::And {
                    (rhs_block, short)
                } else {
                    (short, rhs_block)
                };
                self.terminate(Term::If {
                    cond: left,
                    then,
                    other,
                });
                self.cur = Some(rhs_block);
                self.lower_expr(rhs, dest);
                self.goto(join);
                self.cur = Some(short);
                if let Some(dest) = dest {
                    self.emit(Instr::Const {
                        dest,
                        value: ConstValue::Bool(op == BinaryOp::Or),
                    });
                }
                self.goto(join);
                self.cur = Some(join);
            }
            Expr::Call { ref args, .. } if self.is_step(expr) => {
                let arg = self.temp();
                if let Some(value) = args.first() {
                    self.lower_expr(*value, Some(arg));
                }
                let next = self.new_block();
                self.terminate(Term::Step {
                    expr,
                    arg,
                    dest,
                    next,
                });
                self.cur = Some(next);
            }
            _ => match self.scene_call(expr) {
                Some(call) => self.lower_scene_call(expr, call, dest),
                None => self.lower_hoisted(expr, dest),
            },
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let { pat, init, .. } => {
                self.bind_slots(pat);
                match init {
                    Some(init) => {
                        let src = self.temp();
                        self.lower_expr(init, Some(src));
                        self.emit(Instr::Bind { pat, src });
                    }
                    None => self.emit(Instr::Declare { pat }),
                }
            }
            Stmt::Expr { expr, .. } => self.lower_expr(expr, None),
            Stmt::Item(_) => {}
        }
    }

    fn condition(&mut self, cond: &Condition, then: BlockId, other: BlockId) {
        match *cond {
            Condition::Expr(expr) => {
                let slot = self.temp();
                self.lower_expr(expr, Some(slot));
                self.terminate(Term::If {
                    cond: slot,
                    then,
                    other,
                });
            }
            Condition::Let { pat, expr } => {
                let src = self.temp();
                self.lower_expr(expr, Some(src));
                self.bind_slots(pat);
                self.terminate(Term::Test {
                    pat,
                    src,
                    then,
                    other,
                });
            }
        }
    }

    fn lower_scene_call(&mut self, expr: ExprId, call: SceneCall, dest: Option<Slot>) {
        let (arg_exprs, param_tys, callee, adjust) = match (&call, self.expr(expr).clone()) {
            (SceneCall::Direct(fn_ref), Expr::Call { args, .. }) => {
                let params = self.callee_params(fn_ref);
                (args, params, Callee::Direct(fn_ref.clone()), None)
            }
            (SceneCall::Pointer(callee), Expr::Call { args, .. }) => {
                let ty = self
                    .b
                    .infer
                    .expr_types
                    .get(*callee)
                    .cloned()
                    .unwrap_or(Ty::Error);
                let params = match &ty {
                    Ty::FnPtr { params, .. } => params.clone(),
                    _ => Vec::new(),
                };
                let slot = self.temp();
                self.lower_expr(*callee, Some(slot));
                (args, params, Callee::Value { slot, ty }, None)
            }
            (SceneCall::Method(fn_ref), Expr::MethodCall { receiver, args, .. }) => {
                let params = self.callee_params(fn_ref);
                let adjust = self
                    .b
                    .infer
                    .method_calls
                    .get(&expr)
                    .map(|call| call.receiver);
                let mut all = vec![receiver];
                all.extend(args);
                (all, params, Callee::Direct(fn_ref.clone()), adjust)
            }
            _ => {
                self.lower_hoisted(expr, dest);
                return;
            }
        };
        // Arguments in order. A reference argument is copied in when the call happens, so
        // the value it refers to is read after every argument; the operands of its place
        // (indices) are evaluated in order, and kept to write the result back to the same
        // place.
        let mut args = Vec::with_capacity(arg_exprs.len());
        let mut copies = Vec::new();
        let mut writebacks = Vec::new();
        for (index, arg) in arg_exprs.iter().enumerate() {
            let slot = self.temp();
            args.push(slot);
            let param_ref = match param_tys.get(index) {
                Some(Ty::Ref { mutable, .. }) => Some(*mutable),
                _ => None,
            };
            if index == 0
                && let Some(adjust) = adjust
                && !self.structural.contains(arg)
                && (adjust.autoref.is_some() || adjust.derefs > 0)
            {
                let by_place = adjust.autoref.is_some();
                let overrides = if by_place {
                    self.lower_place(*arg)
                } else {
                    Vec::new()
                };
                if adjust.autoref == Some(true) {
                    writebacks.push((
                        *arg,
                        WriteMode::Place {
                            derefs: adjust.derefs,
                        },
                        overrides.clone(),
                    ));
                }
                copies.push(Instr::Receiver {
                    expr: *arg,
                    dest: slot,
                    derefs: adjust.derefs,
                    by_place,
                    overrides,
                });
                continue;
            }
            if param_ref.is_some() && !self.structural.contains(arg) {
                let overrides = match self.expr(*arg) {
                    Expr::Ref { expr: inner, .. } => {
                        let inner = *inner;
                        self.lower_place(inner)
                    }
                    _ => Vec::new(),
                };
                if param_ref == Some(true) {
                    writebacks.push((*arg, WriteMode::ThroughRef, overrides.clone()));
                }
                copies.push(Instr::EvalDeref {
                    expr: *arg,
                    dest: slot,
                    overrides,
                });
                continue;
            }
            self.lower_expr(*arg, Some(slot));
        }
        for copy in copies {
            self.emit(copy);
        }
        let outs: Vec<Slot> = writebacks.iter().map(|_| self.temp()).collect();
        let next = self.new_block();
        self.terminate(Term::Call {
            expr,
            callee,
            args,
            outs: outs.clone(),
            dest,
            next,
        });
        self.cur = Some(next);
        for ((expr, mode, overrides), src) in writebacks.into_iter().zip(outs) {
            self.emit(Instr::WriteBack {
                expr,
                src,
                mode,
                overrides,
            });
        }
    }

    fn callee_params(&self, fn_ref: &FnRef) -> Vec<Ty> {
        match fn_ref.def {
            FnDefId::Item(item) => self
                .sigs
                .fn_sig(item)
                .map(|sig| sig.params.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Evaluates `expr` into a new temporary, as an operand of a later expression.
    fn hoist(&mut self, expr: ExprId) -> (ExprId, Slot) {
        let slot = self.temp();
        self.lower_expr(expr, Some(slot));
        (expr, slot)
    }

    /// Evaluates the value operands of the place `expr` denotes (indices, a temporary
    /// base), so that the place is resolved later exactly as it would be now. A local or
    /// story variable at its root stays where it is.
    fn lower_place(&mut self, expr: ExprId) -> Vec<(ExprId, Slot)> {
        match self.expr(expr).clone() {
            Expr::Path(_) => Vec::new(),
            Expr::Field { base, .. } => self.lower_place(base),
            Expr::Index { base, index } => {
                let mut out = self.lower_place(base);
                out.push(self.hoist(index));
                out
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } if matches!(self.expr(inner), Expr::Path(_)) => Vec::new(),
            _ => vec![self.hoist(expr)],
        }
    }

    /// An atomic expression with lowered operands: every operand up to the last lowered
    /// one is evaluated first, in the interpreter's order. A place operand keeps its place
    /// and has only the operands of the place evaluated; so does the place under a `&`.
    fn lower_hoisted(&mut self, expr: ExprId, dest: Option<Slot>) {
        let operands = self.operands(expr);
        let Some(last) = operands
            .iter()
            .rposition(|(child, _)| self.structural.contains(child))
        else {
            self.atomic(expr, dest);
            return;
        };
        let mut overrides = Vec::new();
        for (child, place) in operands.into_iter().take(last.saturating_add(1)) {
            if place {
                overrides.extend(self.lower_place(child));
                continue;
            }
            match self.expr(child).clone() {
                Expr::Ref { expr: inner, .. } => overrides.extend(self.lower_place(inner)),
                _ => overrides.push(self.hoist(child)),
            }
        }
        self.emit(Instr::Eval {
            expr,
            dest,
            overrides,
        });
    }

    /// The operands of an atomic expression in the interpreter's evaluation order, each
    /// with whether the interpreter evaluates it as a place.
    fn operands(&self, expr: ExprId) -> Vec<(ExprId, bool)> {
        let data = self.expr(expr);
        match data {
            Expr::Call { callee, args } => {
                let mut out = Vec::new();
                if !matches!(
                    (self.expr(*callee), self.b.infer.paths.get(callee)),
                    (Expr::Path(_), Some(ValueRef::Fn(_)))
                ) {
                    out.push((*callee, false));
                }
                out.extend(args.iter().map(|arg| (*arg, false)));
                out
            }
            Expr::MethodCall { receiver, args, .. } => {
                let autoref = self
                    .b
                    .infer
                    .method_calls
                    .get(&expr)
                    .is_some_and(|call| call.receiver.autoref.is_some());
                let mut out = vec![(*receiver, autoref)];
                out.extend(args.iter().map(|arg| (*arg, false)));
                out
            }
            Expr::Assign { target, value, .. } => vec![(*value, false), (*target, true)],
            Expr::Field { base, .. } => vec![(*base, true)],
            Expr::Index { base, index } => vec![(*base, true), (*index, false)],
            Expr::Ref { expr: inner, .. } => vec![(*inner, true)],
            _ => children(data)
                .into_iter()
                .map(|child| (child, false))
                .collect(),
        }
    }
}

/// The locals `pat` binds.
pub(super) fn pat_locals(b: BodyRef<'_>, pat: PatId) -> Vec<LocalId> {
    let store = &b.body.store;
    let mut out = Vec::new();
    let mut stack = vec![pat];
    while let Some(pat) = stack.pop() {
        match store.pat(pat) {
            Pat::Bind { .. } => {
                if let Some(local) = b.res.bindings.get(&pat) {
                    out.push(*local);
                }
            }
            Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
                stack.extend(pats.iter().copied());
            }
            Pat::Record { fields, .. } => stack.extend(fields.iter().map(|(_, pat)| *pat)),
            _ => {}
        }
    }
    out
}

// ----- Liveness -----------------------------------------------------------------------

struct Uses<'a, 'db> {
    b: BodyRef<'db>,
    local_slots: &'a FxHashMap<LocalId, Slot>,
}

impl Uses<'_, '_> {
    /// The local slots `expr` mentions, except inside `skip` (operands evaluated before).
    fn locals_in(&self, expr: ExprId, skip: &[(ExprId, Slot)], out: &mut SlotSet) {
        let store = &self.b.body.store;
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            if skip.iter().any(|(skipped, _)| *skipped == expr) {
                continue;
            }
            if let Some(ValueRef::Local(local)) = self.b.infer.paths.get(&expr)
                && let Some(slot) = self.local_slots.get(local)
            {
                out.insert(*slot);
            }
            stack.extend(children(store.expr(expr)));
        }
    }

    fn pat_slots(&self, pat: PatId, out: &mut Vec<Slot>) {
        for local in pat_locals(self.b, pat) {
            if let Some(slot) = self.local_slots.get(&local) {
                out.push(*slot);
            }
        }
    }

    /// `live` before `instr`, given `live` after it.
    fn instr(&self, instr: &Instr, live: &mut SlotSet) {
        match instr {
            Instr::Eval {
                expr,
                dest,
                overrides,
            } => {
                if let Some(dest) = dest {
                    live.remove(*dest);
                }
                let store = &self.b.body.store;
                match store.expr(*expr) {
                    Expr::Assign {
                        op: None,
                        target,
                        value,
                    } if matches!(store.expr(*target), Expr::Path(_)) && overrides.is_empty() => {
                        if let Some(ValueRef::Local(local)) = self.b.infer.paths.get(target)
                            && let Some(slot) = self.local_slots.get(local)
                        {
                            live.remove(*slot);
                        }
                        self.locals_in(*value, overrides, live);
                    }
                    _ => self.locals_in(*expr, overrides, live),
                }
                for (_, slot) in overrides {
                    live.insert(*slot);
                }
            }
            Instr::EvalDeref {
                expr,
                dest,
                overrides,
            }
            | Instr::Receiver {
                expr,
                dest,
                overrides,
                ..
            } => {
                live.remove(*dest);
                self.locals_in(*expr, overrides, live);
                for (_, slot) in overrides {
                    live.insert(*slot);
                }
            }
            Instr::Bind { pat, src } => {
                let mut bound = Vec::new();
                self.pat_slots(*pat, &mut bound);
                for slot in bound {
                    live.remove(slot);
                }
                live.insert(*src);
            }
            Instr::Declare { pat } => {
                let mut bound = Vec::new();
                self.pat_slots(*pat, &mut bound);
                for slot in bound {
                    live.remove(slot);
                }
            }
            Instr::Const { dest, .. } => live.remove(*dest),
            Instr::ForInit { slot } => live.insert(*slot),
            Instr::WriteBack {
                expr,
                src,
                overrides,
                ..
            } => {
                live.insert(*src);
                self.locals_in(*expr, overrides, live);
                for (_, slot) in overrides {
                    live.insert(*slot);
                }
            }
        }
    }
}

/// Live slots per block, and the slots kept at each suspension.
fn liveness(body: &StoryBody, uses: &Uses<'_, '_>) -> FxHashMap<BlockId, SlotSet> {
    let len = body.slots.len();
    let mut live_in = vec![SlotSet::with_len(len); body.blocks.len()];
    let get = |live_in: &[SlotSet], block: BlockId| {
        live_in
            .get(block)
            .cloned()
            .unwrap_or_else(|| SlotSet::with_len(len))
    };
    loop {
        let mut changed = false;
        for (index, block) in body.blocks.iter().enumerate().rev() {
            let mut live = match &block.term {
                Term::Goto(next) => get(&live_in, *next),
                Term::If { cond, then, other } => {
                    let mut live = get(&live_in, *then);
                    live.union(&get(&live_in, *other));
                    live.insert(*cond);
                    live
                }
                Term::Test {
                    pat,
                    src,
                    then,
                    other,
                } => {
                    let mut live = get(&live_in, *then);
                    let mut bound = Vec::new();
                    uses.pat_slots(*pat, &mut bound);
                    for slot in bound {
                        live.remove(slot);
                    }
                    live.union(&get(&live_in, *other));
                    live.insert(*src);
                    live
                }
                Term::ForNext {
                    iter,
                    pat,
                    body: loop_body,
                    exit,
                } => {
                    let mut live = get(&live_in, *loop_body);
                    let mut bound = Vec::new();
                    uses.pat_slots(*pat, &mut bound);
                    for slot in bound {
                        live.remove(slot);
                    }
                    live.union(&get(&live_in, *exit));
                    live.insert(*iter);
                    live
                }
                Term::Step {
                    arg, dest, next, ..
                } => {
                    let mut live = get(&live_in, *next);
                    if let Some(dest) = dest {
                        live.remove(*dest);
                    }
                    live.insert(*arg);
                    live
                }
                Term::Call {
                    callee,
                    args,
                    outs,
                    dest,
                    next,
                    ..
                } => {
                    let mut live = get(&live_in, *next);
                    if let Some(dest) = dest {
                        live.remove(*dest);
                    }
                    for out in outs {
                        live.remove(*out);
                    }
                    for arg in args {
                        live.insert(*arg);
                    }
                    if let Callee::Value { slot, .. } = callee {
                        live.insert(*slot);
                    }
                    live
                }
                Term::Return { value } => {
                    let mut live = SlotSet::with_len(len);
                    live.insert(*value);
                    for slot in &body.out_params {
                        live.insert(*slot);
                    }
                    live
                }
                Term::Unreachable => SlotSet::with_len(len),
            };
            for (position, instr) in block.instrs.iter().enumerate().rev() {
                uses.instr(instr, &mut live);
                for exit in block
                    .exits
                    .get(position)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                {
                    live.union(&get(&live_in, *exit));
                }
            }
            if let Some(slot) = live_in.get_mut(index)
                && *slot != live
            {
                *slot = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // A suspension keeps what lives after it, and what the step or call itself consumes:
    // states that differ in the step shown or the scene called are different states.
    let mut kept = FxHashMap::default();
    for (index, block) in body.blocks.iter().enumerate() {
        match &block.term {
            Term::Step {
                arg, dest, next, ..
            } => {
                let mut live = get(&live_in, *next);
                if let Some(dest) = dest {
                    live.remove(*dest);
                }
                live.insert(*arg);
                kept.insert(index, live);
            }
            Term::Call {
                callee,
                args,
                dest,
                outs,
                next,
                ..
            } => {
                let mut live = get(&live_in, *next);
                if let Some(dest) = dest {
                    live.remove(*dest);
                }
                for out in outs {
                    live.remove(*out);
                }
                for arg in args {
                    live.insert(*arg);
                }
                if let Callee::Value { slot, .. } = callee {
                    live.insert(*slot);
                }
                kept.insert(index, live);
            }
            _ => {}
        }
    }
    kept
}
