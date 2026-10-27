//! The MIR executor: runs MIR bodies on the [`Machine`], with an explicit stack of frames
//! (no host recursion) and one heap cell per MIR local.

use crate::diagnostic::{Anchor, DiagnosticCode, NodeId, Owner};
use crate::hir::{BinaryOp, ExprId};
use crate::item_tree::FnKind;
use crate::mir::{
    self, AggregateKind, BasicBlock, Body, Callee, ConstOperand, Local, MirSource, Operand,
    PlaceBase, Rvalue, StatementKind, Terminator, item_mir,
};
use crate::ty::{Const, FnDefId, FnRef, GenericArg, GenericArgs, IntTy, Ty, TyKind};

use super::super::num;
use super::super::queries::{StaticValue, const_value, promoted_value, static_value};
use super::super::std_items::Builtin;
use super::super::{EVAL_STEP_BUDGET, value::ConstValue};
use super::value::{ClosureValue, GenericEnv, Value, int_range, subst_arg, subst_fn_ref, subst_ty};
use super::{
    CallSite, ErrorKind, EvalError, MAX_CALL_DEPTH, Machine, Place, Proj, budget_exceeded, fault,
    overflow, recursion_too_deep,
};

type Res<T> = Result<T, EvalError>;

/// One activation of a MIR body.
pub(crate) struct Frame<'db> {
    pub body: &'db Body,
    pub env: GenericEnv,
    /// The heap cell of every local of `body`.
    pub cells: Vec<usize>,
    pub block: BasicBlock,
    pub stmt: usize,
}

/// What a call does: produce its value at once, or run a body.
enum Called<'db> {
    Value(Value),
    Enter(Frame<'db>),
}

/// What executing one statement or terminator leads to.
pub(super) enum Next<'db> {
    Continue,
    Enter(Frame<'db>),
    Return(Value),
}

fn error(kind: ErrorKind, at: Option<Anchor>) -> EvalError {
    EvalError { kind, at }
}

fn abort() -> EvalError {
    EvalError::abort()
}

fn local_index(local: Local) -> usize {
    local.into_raw().into_u32() as usize
}

/// The anchor of a statement or terminator of `body`.
fn anchor(body: &Body, src: Option<ExprId>) -> Option<Anchor> {
    src.map(|expr| Anchor::Node(Owner::Body(body.source.owner()), NodeId::Expr(expr)))
}

impl<'db> Machine<'db> {
    /// Runs `body` with `args` bound to its argument locals; returns its `_0`.
    pub fn run_body(
        &mut self,
        body: &'db Body,
        env: GenericEnv,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        let frame = self.new_frame(body, env, args)?;
        self.execute(frame)
    }

    /// Calls a function value (a named function or a closure) from outside any body: the
    /// renders of steps and tabulated calls.
    pub fn call_value(
        &mut self,
        site: CallSite,
        callee: &Value,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        let at = site.at;
        match self.call_callee(site, callee, args)? {
            Called::Value(value) => Ok(value),
            Called::Enter(frame) => {
                self.push_depth(at)?;
                let result = self.execute(frame);
                self.depth = self.depth.saturating_sub(1);
                result
            }
        }
    }

    /// A fresh frame of `body`: every local in a new uninitialized cell, the arguments in
    /// theirs.
    fn new_frame(&mut self, body: &'db Body, env: GenericEnv, args: Vec<Value>) -> Res<Frame<'db>> {
        if args.len() != body.arg_count as usize {
            return Err(abort());
        }
        let mut cells = Vec::with_capacity(body.locals.len());
        let mut args = args.into_iter();
        for (index, _) in body.locals.iter().enumerate() {
            let value = match index {
                0 => Value::Uninit,
                _ => args.next().unwrap_or(Value::Uninit),
            };
            cells.push(self.alloc(value));
        }
        Ok(Frame {
            body,
            env,
            cells,
            block: body.entry,
            stmt: 0,
        })
    }

    /// Counts one more nested call.
    pub(super) fn push_depth(&mut self, at: Option<Anchor>) -> Res<()> {
        if self.depth >= MAX_CALL_DEPTH {
            return Err(error(recursion_too_deep(), at));
        }
        self.depth = self.depth.saturating_add(1);
        Ok(())
    }

    fn tick_at(&mut self, count: u64, at: Option<Anchor>) -> Res<()> {
        self.steps = self.steps.saturating_add(count);
        if self.steps > EVAL_STEP_BUDGET {
            return Err(error(budget_exceeded(), at));
        }
        Ok(())
    }

    /// Runs `first` and every frame it calls until `first` returns.
    pub(super) fn execute(&mut self, first: Frame<'db>) -> Res<Value> {
        let base = self.depth;
        let mut stack = vec![first];
        let result = self.execute_stack(&mut stack);
        self.depth = base;
        result
    }

    fn execute_stack(&mut self, stack: &mut Vec<Frame<'db>>) -> Res<Value> {
        loop {
            let Some(frame) = stack.last_mut() else {
                return Err(abort());
            };
            match self.step(frame)? {
                Next::Continue => {}
                Next::Enter(callee) => {
                    let at = {
                        let block = &frame.body.blocks[frame.block];
                        let src = block.stmts.get(frame.stmt).and_then(|stmt| stmt.src);
                        anchor(frame.body, src)
                    };
                    self.push_depth(at)?;
                    stack.push(callee);
                }
                Next::Return(value) => {
                    stack.pop();
                    let Some(caller) = stack.last_mut() else {
                        return Ok(value);
                    };
                    self.depth = self.depth.saturating_sub(1);
                    let block = &caller.body.blocks[caller.block];
                    let Some(stmt) = block.stmts.get(caller.stmt) else {
                        return Err(abort());
                    };
                    let StatementKind::Call { dest, .. } = &stmt.kind else {
                        return Err(abort());
                    };
                    let at = anchor(caller.body, stmt.src);
                    let place = self.place(caller, dest, at)?;
                    self.write_place(&place, value)
                        .map_err(|kind| error(kind, at))?;
                    caller.stmt = caller.stmt.saturating_add(1);
                }
            }
        }
    }

    /// Executes the next statement or the terminator of `frame`.
    pub(super) fn step(&mut self, frame: &mut Frame<'db>) -> Res<Next<'db>> {
        let body = frame.body;
        let block = &body.blocks[frame.block];
        if let Some(stmt) = block.stmts.get(frame.stmt) {
            let at = anchor(body, stmt.src);
            self.tick_at(1, at)?;
            match &stmt.kind {
                StatementKind::Assign(place, rvalue) => {
                    let value = self.rvalue(frame, rvalue, at)?;
                    let place = self.place(frame, place, at)?;
                    self.write_place(&place, value)
                        .map_err(|kind| error(kind, at))?;
                }
                StatementKind::Call { dest, callee, args } => {
                    match self.call_stmt(frame, dest, callee, args, at)? {
                        Called::Value(value) => {
                            let place = self.place(frame, dest, at)?;
                            self.write_place(&place, value)
                                .map_err(|kind| error(kind, at))?;
                        }
                        Called::Enter(callee) => return Ok(Next::Enter(callee)),
                    }
                }
            }
            frame.stmt = frame.stmt.saturating_add(1);
            return Ok(Next::Continue);
        }
        let at = anchor(body, block.term_src);
        self.tick_at(1, at)?;
        let target = match &block.term {
            Terminator::Goto(target) => *target,
            Terminator::SwitchBool {
                cond, then, other, ..
            } => {
                let value = self.operand(frame, cond, at)?;
                match self.deref_value(value).map_err(|kind| error(kind, at))? {
                    Value::Bool(true) => *then,
                    Value::Bool(false) => *other,
                    _ => return Err(abort()),
                }
            }
            Terminator::SwitchVariant {
                place,
                targets,
                otherwise,
                ..
            } => {
                let place = self.place(frame, place, at)?;
                let variant = match self.place_mut(&place).map_err(|kind| error(kind, at))? {
                    Value::Adt { variant, .. } => *variant,
                    Value::Probe => return Err(error(ErrorKind::Inspected, at)),
                    _ => return Err(abort()),
                };
                match targets.iter().find(|(target, _)| *target == variant) {
                    Some((_, block)) => *block,
                    None => otherwise.ok_or_else(abort)?,
                }
            }
            Terminator::Return => {
                let cell = frame.cells.first().copied().ok_or_else(abort)?;
                let value = self
                    .read_place(&Place::cell(cell))
                    .map_err(|kind| error(kind, at))?;
                return Ok(Next::Return(value));
            }
            // Suspensions run only in the story analysis.
            Terminator::Unreachable
            | Terminator::Step { .. }
            | Terminator::Rng { .. }
            | Terminator::SceneCall { .. } => return Err(abort()),
        };
        frame.block = target;
        frame.stmt = 0;
        Ok(Next::Continue)
    }

    /// The machine place of a MIR place.
    pub(super) fn place(
        &mut self,
        frame: &Frame<'db>,
        place: &mir::Place,
        at: Option<Anchor>,
    ) -> Res<Place> {
        let mut out = match place.base {
            PlaceBase::Local(local) => Place::cell(self.cell_of(frame, local)?),
            PlaceBase::StoryVar(item) => Place::cell(self.story_cell(item).ok_or_else(abort)?),
        };
        for proj in &place.proj {
            out = match proj {
                mir::Proj::Field(index) => {
                    self.projectable_at(&out, at)?;
                    out.project(Proj::Field(*index))
                }
                mir::Proj::Downcast(_) => out,
                mir::Proj::Deref => match self.place_mut(&out) {
                    Ok(Value::Ref(next)) => (**next).clone(),
                    Ok(Value::Probe) | Err(ErrorKind::Inspected) => {
                        return Err(error(ErrorKind::Inspected, at));
                    }
                    _ => return Err(abort()),
                },
                mir::Proj::Index(local) => {
                    let cell = self.cell_of(frame, *local)?;
                    let index = self
                        .read_place(&Place::cell(cell))
                        .map_err(|kind| error(kind, at))?;
                    let index = self.int_at(index, at)?;
                    self.index_place(out, index, at)?
                }
            };
        }
        Ok(out)
    }

    fn projectable_at(&mut self, place: &Place, at: Option<Anchor>) -> Res<()> {
        match self.place_mut(place) {
            Ok(Value::Probe) | Err(ErrorKind::Inspected) => Err(error(ErrorKind::Inspected, at)),
            _ => Ok(()),
        }
    }

    /// `place[index]`, following references at `place`.
    fn index_place(&mut self, mut place: Place, index: i128, at: Option<Anchor>) -> Res<Place> {
        loop {
            let len = match self.place_mut(&place) {
                Ok(Value::Array(items) | Value::Vec(items)) => items.len(),
                Ok(Value::Ref(next)) => {
                    place = (**next).clone();
                    continue;
                }
                Ok(Value::Probe) => return Err(error(ErrorKind::Inspected, at)),
                Ok(_) => return Err(abort()),
                Err(kind) => return Err(error(kind, at)),
            };
            let Some(position) = usize::try_from(index).ok().filter(|i| *i < len) else {
                return Err(error(
                    fault(
                        DiagnosticCode::EvalIndexOutOfRange,
                        format!("index {index} is out of range for length {len}"),
                    ),
                    at,
                ));
            };
            return Ok(place.project(Proj::Index(position)));
        }
    }

    fn cell_of(&self, frame: &Frame<'db>, local: Local) -> Res<usize> {
        frame
            .cells
            .get(local_index(local))
            .copied()
            .ok_or_else(abort)
    }

    fn int_at(&mut self, value: Value, at: Option<Anchor>) -> Res<i128> {
        match self.deref_value(value) {
            Ok(Value::Int(value)) => Ok(value),
            Ok(_) => Err(abort()),
            Err(kind) => Err(error(kind, at)),
        }
    }

    pub(super) fn operand(
        &mut self,
        frame: &Frame<'db>,
        operand: &Operand,
        at: Option<Anchor>,
    ) -> Res<Value> {
        match operand {
            Operand::Copy(place) => {
                let place = self.place(frame, place, at)?;
                self.read_place(&place).map_err(|kind| error(kind, at))
            }
            Operand::Const(constant) => self.constant(frame, constant, at),
        }
    }

    fn operands(
        &mut self,
        frame: &Frame<'db>,
        operands: &[Operand],
        at: Option<Anchor>,
    ) -> Res<Vec<Value>> {
        operands
            .iter()
            .map(|operand| self.operand(frame, operand, at))
            .collect()
    }

    fn constant(
        &mut self,
        frame: &Frame<'db>,
        constant: &ConstOperand,
        at: Option<Anchor>,
    ) -> Res<Value> {
        match constant {
            ConstOperand::Value(value) => Ok(self.thaw(value)),
            ConstOperand::Item { item, args } => {
                let args: GenericArgs = args.iter().map(|arg| subst_arg(arg, &frame.env)).collect();
                let generic = !args.is_empty();
                match const_value(self.db, self.program, *item, args) {
                    Ok(value) => Ok(self.thaw(value)),
                    // A non-generic constant reports its own errors.
                    Err(_) if !generic => Err(abort()),
                    Err(failure) => Err(EvalError {
                        kind: failure.kind.clone(),
                        at: failure.at.or(at),
                    }),
                }
            }
            ConstOperand::Param { owner, index } => match frame.env.arg(*owner, *index) {
                Some(GenericArg::Const(Const::Value(value))) => Ok(Value::Int(*value)),
                _ => Err(abort()),
            },
            ConstOperand::Promoted(index) => {
                // The driver reports the errors of promoted constants.
                match promoted_value(self.db, self.program, frame.body.source.owner(), *index) {
                    Ok(value) => Ok(self.thaw(value)),
                    Err(_) => Err(abort()),
                }
            }
            ConstOperand::Fn(fn_ref) => Ok(Value::Fn(subst_fn_ref(fn_ref, &frame.env))),
            ConstOperand::Handle(item) | ConstOperand::Static(item) => {
                match static_value(self.db, self.program, *item) {
                    Ok(StaticValue {
                        handle: Some(kind), ..
                    }) => Ok(Value::Handle {
                        kind: *kind,
                        item: *item,
                    }),
                    Ok(StaticValue {
                        handle: None,
                        value,
                    }) => Ok(self.thaw(value)),
                    Err(_) => Err(abort()),
                }
            }
        }
    }

    fn rvalue(&mut self, frame: &Frame<'db>, rvalue: &Rvalue, at: Option<Anchor>) -> Res<Value> {
        let lift = |kind: ErrorKind| error(kind, at);
        match rvalue {
            Rvalue::Use(operand) => self.operand(frame, operand, at),
            Rvalue::IntBin { op, lhs, rhs, ty } => {
                let ty = subst_ty(ty, &frame.env);
                self.mir_binary(frame, *op, lhs, rhs, &ty, at)
            }
            Rvalue::BoolBin { op, lhs, rhs } => {
                self.mir_binary(frame, *op, lhs, rhs, &Ty::bool(), at)
            }
            Rvalue::DecBin { op, lhs, rhs }
            | Rvalue::RatBin { op, lhs, rhs }
            | Rvalue::Cmp { op, lhs, rhs } => {
                self.mir_binary(frame, *op, lhs, rhs, &Ty::error(), at)
            }
            Rvalue::IntNeg { operand, ty } => {
                let ty = subst_ty(ty, &frame.env);
                let value = self.operand(frame, operand, at)?;
                let value = self.int_at(value, at)?;
                match value.checked_neg() {
                    Some(negated) => self.check_int(negated, &ty).map(Value::Int).map_err(lift),
                    None => Err(lift(overflow(None, &ty).into())),
                }
            }
            Rvalue::IntNot { operand, ty } => {
                let ty = subst_ty(ty, &frame.env);
                let value = self.operand(frame, operand, at)?;
                let value = self.int_at(value, at)?;
                let int = match ty.kind() {
                    TyKind::Int(int) => *int,
                    _ => IntTy::I128,
                };
                num::bit_not(value, int)
                    .map(Value::Int)
                    .ok_or_else(|| lift(overflow(None, &ty).into()))
            }
            Rvalue::IntClamp { operand, ty } => {
                let ty = subst_ty(ty, &frame.env);
                let value = self.operand(frame, operand, at)?;
                let value = self.int_at(value, at)?;
                Ok(Value::Int(match int_range(&ty) {
                    Some((lo, hi)) => value.clamp(lo, hi),
                    None => value,
                }))
            }
            Rvalue::BoolNot(operand) => {
                let value = self.operand(frame, operand, at)?;
                match self.deref_value(value).map_err(lift)? {
                    Value::Bool(value) => Ok(Value::Bool(!value)),
                    _ => Err(abort()),
                }
            }
            Rvalue::DecNeg(operand) | Rvalue::RatNeg(operand) => {
                let value = self.operand(frame, operand, at)?;
                let negated = match self.deref_value(value).map_err(lift)? {
                    Value::Decimal(value) => value.checked_neg().map(Value::Decimal),
                    Value::Rat(value) => value.checked_neg().map(Value::Rat),
                    _ => return Err(abort()),
                };
                negated.ok_or_else(|| lift(overflow(None, &Ty::error()).into()))
            }
            Rvalue::Aggregate { kind, ops } => {
                let values = self.operands(frame, ops, at)?;
                Ok(match kind {
                    AggregateKind::Tuple => Value::Tuple(values.into()),
                    AggregateKind::Array => Value::Array(values.into()),
                    AggregateKind::Adt(ctor) => Value::Adt {
                        item: ctor.adt,
                        variant: ctor.variant.unwrap_or(0),
                        fields: values.into(),
                    },
                })
            }
            Rvalue::Repeat { operand, count } => {
                let value = self.operand(frame, operand, at)?;
                self.tick_at(*count, at)?;
                let len = usize::try_from(*count).unwrap_or(usize::MAX);
                Ok(Value::Array(imbl::Vector::from(vec![value; len])))
            }
            Rvalue::Ref { place, .. } => {
                let place = self.place(frame, place, at)?;
                Ok(Value::Ref(Box::new(place)))
            }
            Rvalue::Len(place) => {
                let mut place = self.place(frame, place, at)?;
                loop {
                    match self.place_mut(&place).map_err(lift)? {
                        Value::Array(items) | Value::Vec(items) => {
                            let len = i128::try_from(items.len()).map_err(|_| abort())?;
                            return Ok(Value::Int(len));
                        }
                        Value::Ref(next) => place = (**next).clone(),
                        Value::Probe => return Err(lift(ErrorKind::Inspected)),
                        _ => return Err(abort()),
                    }
                }
            }
            Rvalue::Closure { expr, captures } => {
                let captures = self.operands(frame, captures, at)?;
                Ok(Value::Closure(Box::new(ClosureValue {
                    source: MirSource::Closure {
                        owner: frame.body.source.owner(),
                        expr: *expr,
                    },
                    env: frame.env.clone(),
                    captures,
                })))
            }
            Rvalue::Cast { operand, ty } => {
                let value = self.operand(frame, operand, at)?;
                let value = self.int_at(value, at)?;
                match subst_ty(ty, &frame.env).kind() {
                    &TyKind::Int(int) => match num::wrap_to(value, int) {
                        Some(value) => Ok(Value::Int(value)),
                        None => Err(lift(overflow(None, &Ty::int(int)).into())),
                    },
                    _ => Ok(Value::Int(value)),
                }
            }
        }
    }

    /// A binary operator; `ty` is the result type, which is also the left operand's for
    /// the integer operators.
    fn mir_binary(
        &mut self,
        frame: &Frame<'db>,
        op: BinaryOp,
        lhs: &Operand,
        rhs: &Operand,
        ty: &Ty,
        at: Option<Anchor>,
    ) -> Res<Value> {
        let left = self.operand(frame, lhs, at)?;
        let right = self.operand(frame, rhs, at)?;
        self.binop(op, left, right, ty, ty)
            .map_err(|kind| error(kind, at))
    }

    fn call_stmt(
        &mut self,
        frame: &Frame<'db>,
        dest: &mir::Place,
        callee: &Callee,
        args: &[Operand],
        at: Option<Anchor>,
    ) -> Res<Called<'db>> {
        let callee = match callee {
            Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => {
                Value::Fn(subst_fn_ref(fn_ref, &frame.env))
            }
            Callee::Value(operand) => {
                let value = self.operand(frame, operand, at)?;
                self.deref_value(value).map_err(|kind| error(kind, at))?
            }
        };
        let args = self.operands(frame, args, at)?;
        let ty = dest
            .as_local()
            .map(|local| subst_ty(&frame.body.locals[local].ty, &frame.env));
        self.call_callee(CallSite { at, ty }, &callee, args)
    }

    fn call_callee(
        &mut self,
        site: CallSite,
        callee: &Value,
        args: Vec<Value>,
    ) -> Res<Called<'db>> {
        match callee {
            Value::Fn(fn_ref) => self.call_fn(site, fn_ref, args),
            Value::Closure(closure) => {
                let source = closure.source;
                let Some(body) = item_mir(self.db, self.program, source.owner())
                    .as_ref()
                    .and_then(|mir| mir.body(source))
                else {
                    return Err(abort());
                };
                let mut values = closure.captures.clone();
                values.extend(args);
                self.new_frame(body, closure.env.clone(), values)
                    .map(Called::Enter)
            }
            Value::Probe => Err(error(ErrorKind::Inspected, site.at)),
            _ => Err(abort()),
        }
    }

    /// Calls `fn_ref` with evaluated arguments (the receiver first for methods).
    fn call_fn(&mut self, site: CallSite, fn_ref: &FnRef, args: Vec<Value>) -> Res<Called<'db>> {
        let item = match fn_ref.def {
            FnDefId::StructCtor(item) => {
                return Ok(Called::Value(Value::Adt {
                    item,
                    variant: 0,
                    fields: args.into(),
                }));
            }
            FnDefId::VariantCtor(item, variant) => {
                return Ok(Called::Value(Value::Adt {
                    item,
                    variant,
                    fields: args.into(),
                }));
            }
            FnDefId::Item(item) => item,
        };
        if let Some(builtin) = self.std().builtin(item) {
            // In story mode the only pushdown builtin besides `push_step` (which the story
            // analysis runs itself) is `Meta::write`: the story never reads meta back, so
            // it changes nothing the analysis tracks.
            if builtin == Builtin::Runtime
                && self.story.is_some()
                && item != self.std().push_step
                && self.sigs.fn_sig(item).map(|sig| sig.kind) == Some(FnKind::Pushdown)
            {
                return Ok(Called::Value(Value::unit()));
            }
            let at = site.at;
            return self
                .builtin(site, builtin, fn_ref, args)
                .map(Called::Value)
                .map_err(|kind| error(kind, at));
        }
        let Some(sig) = self.sigs.fn_sig(item) else {
            return Err(abort());
        };
        // Calling a `fn` or `pushdown fn` from const code is a call-rule error of stage 3;
        // in story mode instant procedures run, scenes never (the analysis runs them).
        let allowed = match sig.kind {
            FnKind::Const => true,
            FnKind::Pushdown => self.story.is_some() && !sig.scene,
            FnKind::Plain => false,
        };
        if !allowed {
            return Err(abort());
        }
        let Some(mir) = item_mir(self.db, self.program, item).as_ref() else {
            return Err(abort());
        };
        let env = GenericEnv::for_item(self.sigs, item, fn_ref.args.clone());
        self.new_frame(&mir.main, env, args).map(Called::Enter)
    }

    /// Freezes the result of an evaluation.
    pub(crate) fn freeze_result(&mut self, value: &Value) -> Result<ConstValue, EvalError> {
        self.freeze(value)
            .map_err(|kind| EvalError { kind, at: None })
    }
}
