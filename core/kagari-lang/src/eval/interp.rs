//! The const-evaluation interpreter over HIR, name resolution and inference results.
//!
//! Values follow `docs/syntax/src/values.md`: every value is copied on assignment and
//! argument passing; a reference is a pointer to a place (a heap cell plus a projection
//! path), so writes through `&mut` are visible through every alias. The screen parts
//! builtins hand out (`&mut Entity`, `&mut Prop<T>`, …) are places with a [`ScreenObj`].

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{Def, Resolution};
use crate::diagnostic::{DiagnosticCode, Span};
use crate::hir::{
    ArrayExpr, BinaryOp, Body, Condition, Expr, ExprId, Literal, Pat, PatBound, PatId, Stmt,
    UnaryOp, file_hir,
};
use crate::infer::{InferenceResult, ReceiverAdjust, ValueRef, inference};
use crate::input::{FilePath, Program};
use crate::item_tree::{FnKind, ItemId, ItemKind, Shape, item_tree};
use crate::resolve::{BodyResolution, LocalId, file_resolution};
use crate::signatures::{Generics, ItemSig, ProgramSignatures};
use crate::ty::{Const, FnDefId, FnRef, GenericArg, IntTy, Ty};

use super::const_exprs::ConstExprs;
use super::num::{self, Decimal, Rat};
use super::screen::{PendingChange, Screen, ScreenObj};
use super::std_items::{Builtin, StdTable};
use super::value::{AssetKind, ConstValue, HandleKind, Text, adt_variants, item_name};
use super::{EVAL_STEP_BUDGET, Fault};

/// Interpreted calls nested deeper than this, counted across the constants an evaluation
/// needs, are `E4003` ("recursion too deep").
pub const MAX_CALL_DEPTH: u32 = 10_000;

/// When less native stack than this is left, evaluation continues on a new segment.
const STACK_RED_ZONE: usize = 100 * 1024;
/// The size of each new stack segment.
const STACK_GROWTH: usize = 1024 * 1024;

/// Why an evaluation did not produce a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Something already reported or outside const evaluation (`Expr::Missing`, a
    /// `Ty::Error`, a missing inference result): stop without a diagnostic.
    Abort,
    /// A render inspected its input screen (see [`Value::Probe`]).
    Inspected,
    /// A const-evaluation error.
    Fault(Fault),
}

impl From<Fault> for ErrorKind {
    fn from(fault: Fault) -> ErrorKind {
        ErrorKind::Fault(fault)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalError {
    pub kind: ErrorKind,
    /// The expression that failed, in whatever body it is (std included).
    pub span: Option<Span>,
}

impl EvalError {
    pub fn abort() -> EvalError {
        EvalError {
            kind: ErrorKind::Abort,
            span: None,
        }
    }
}

/// How control leaves an expression other than by producing a value.
pub(crate) enum Exit {
    Break(ExprId, Value),
    Continue(ExprId),
    Return(Value),
    Error(EvalError),
}

pub(crate) type R<T> = Result<T, Exit>;

/// A value during evaluation.
#[derive(Clone, Debug)]
pub(crate) enum Value {
    Bool(bool),
    Int(i128),
    Decimal(Decimal),
    Rat(Rat),
    Text(Text),
    String(String),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
    Vec(Vec<Value>),
    Adt {
        item: ItemId,
        variant: u32,
        fields: Vec<Value>,
    },
    Asset {
        kind: AssetKind,
        path: FilePath,
    },
    Fn(FnRef),
    Closure(Box<ClosureValue>),
    /// A path to a handle `static`.
    Handle {
        kind: HandleKind,
        item: ItemId,
    },
    /// `setting(init)` / `meta(init)`: a handle being created by a `static` initializer.
    NewHandle {
        kind: HandleKind,
        init: Box<Value>,
    },
    Screen(Box<Screen>),
    Ref(Box<Place>),
    /// A `Change<T>`, `Swap` or `ChannelChange` waiting for `apply_animation`.
    Pending(Box<Pending>),
    /// The opaque input screen of a render's keyframe test: inspecting it in any way aborts
    /// with [`ErrorKind::Inspected`].
    Probe,
    /// A `let` without initializer.
    Uninit,
}

impl Value {
    pub fn unit() -> Value {
        Value::Tuple(Vec::new())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ClosureValue {
    pub owner: ItemId,
    pub expr: ExprId,
    pub env: GenericEnv,
    pub captures: Vec<(LocalId, Value)>,
}

/// A place: a heap cell, a projection path into it, and optionally the part of the screen
/// at that path a builtin returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub cell: usize,
    pub path: Vec<Proj>,
    pub obj: Option<ScreenObj>,
}

impl Place {
    fn cell(cell: usize) -> Place {
        Place {
            cell,
            path: Vec::new(),
            obj: None,
        }
    }

    fn project(&self, proj: Proj) -> Place {
        let mut place = self.clone();
        place.path.push(proj);
        place
    }

    /// The place of the screen itself.
    pub fn without_obj(&self) -> Place {
        Place {
            cell: self.cell,
            path: self.path.clone(),
            obj: None,
        }
    }

    pub fn with_obj(&self, obj: ScreenObj) -> Place {
        Place {
            cell: self.cell,
            path: self.path.clone(),
            obj: Some(obj),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Proj {
    Field(u32),
    Index(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct Pending {
    /// The screen the change applies to.
    pub screen: Place,
    pub change: PendingChange,
}

/// The generic arguments of the function or constant being evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GenericEnv {
    pub item: Option<ItemId>,
    pub parent: Option<ItemId>,
    pub parent_count: u32,
    pub args: Vec<GenericArg>,
}

impl GenericEnv {
    fn arg(&self, owner: ItemId, index: u32) -> Option<&GenericArg> {
        let position = if Some(owner) == self.item {
            self.parent_count.checked_add(index)?
        } else if Some(owner) == self.parent {
            index
        } else {
            return None;
        };
        self.args.get(position as usize)
    }
}

pub(crate) fn subst_const(value: &Const, env: &GenericEnv) -> Const {
    match value {
        Const::Param { owner, index } => match env.arg(*owner, *index) {
            Some(GenericArg::Const(value)) => value.clone(),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

pub(crate) fn subst_arg(arg: &GenericArg, env: &GenericEnv) -> GenericArg {
    match arg {
        GenericArg::Type(ty) => GenericArg::Type(subst_ty(ty, env)),
        GenericArg::Const(value) => GenericArg::Const(subst_const(value, env)),
    }
}

pub(crate) fn subst_fn_ref(fn_ref: &FnRef, env: &GenericEnv) -> FnRef {
    if env.args.is_empty() {
        return fn_ref.clone();
    }
    FnRef {
        def: fn_ref.def,
        self_ty: fn_ref.self_ty.as_ref().map(|ty| subst_ty(ty, env)),
        args: fn_ref.args.iter().map(|arg| subst_arg(arg, env)).collect(),
    }
}

/// `ty` with the generic parameters of `env` replaced by their arguments.
pub(crate) fn subst_ty(ty: &Ty, env: &GenericEnv) -> Ty {
    if env.args.is_empty() {
        return ty.clone();
    }
    let all = |tys: &[Ty]| tys.iter().map(|ty| subst_ty(ty, env)).collect::<Vec<_>>();
    match ty {
        Ty::Param { owner, index } => match env.arg(*owner, *index) {
            Some(GenericArg::Type(arg)) => arg.clone(),
            _ => ty.clone(),
        },
        Ty::Bounded { lo, hi } => Ty::Bounded {
            lo: subst_const(lo, env),
            hi: subst_const(hi, env),
        },
        Ty::Tuple(fields) => Ty::Tuple(all(fields)),
        Ty::Array { elem, len } => Ty::Array {
            elem: Box::new(subst_ty(elem, env)),
            len: subst_const(len, env),
        },
        Ty::Adt { item, args } => Ty::Adt {
            item: *item,
            args: args.iter().map(|arg| subst_arg(arg, env)).collect(),
        },
        Ty::Ref { mutable, inner } => Ty::Ref {
            mutable: *mutable,
            inner: Box::new(subst_ty(inner, env)),
        },
        Ty::FnPtr { kind, params, ret } => Ty::FnPtr {
            kind: *kind,
            params: all(params),
            ret: Box::new(subst_ty(ret, env)),
        },
        Ty::Closure { kind, params, ret } => Ty::Closure {
            kind: *kind,
            params: all(params),
            ret: Box::new(subst_ty(ret, env)),
        },
        Ty::FnDef(fn_ref) => Ty::FnDef(Box::new(subst_fn_ref(fn_ref, env))),
        Ty::Projection {
            base,
            trait_item,
            name,
        } => Ty::Projection {
            base: Box::new(subst_ty(base, env)),
            trait_item: *trait_item,
            name: name.clone(),
        },
        Ty::Error | Ty::Never | Ty::Bool | Ty::Int(_) | Ty::TraitSelf { .. } | Ty::Infer(_) => {
            ty.clone()
        }
    }
}

/// The `[lo, hi]` range of an integer type, when known.
pub(crate) fn int_range(ty: &Ty) -> Option<(i128, i128)> {
    match ty {
        Ty::Int(int) => Some((IntTy::min(*int), IntTy::max(*int))),
        Ty::Bounded { lo, hi } => Some((lo.value()?, hi.value()?)),
        _ => None,
    }
}

/// `u32`, `Int<0, 5>`, … for messages.
pub(crate) fn int_ty_name(ty: &Ty) -> String {
    match ty {
        Ty::Int(int) => int.name().to_owned(),
        Ty::Bounded { lo, hi } => match (lo.value(), hi.value()) {
            (Some(lo), Some(hi)) => format!("Int<{lo}, {hi}>"),
            _ => "Int".to_owned(),
        },
        _ => "integer".to_owned(),
    }
}

fn overflow(value: Option<i128>, ty: &Ty) -> Fault {
    let message = match value {
        Some(value) => format!(
            "arithmetic overflow: {value} does not fit in `{}`",
            int_ty_name(ty)
        ),
        None => "arithmetic overflow".to_owned(),
    };
    Fault::new(DiagnosticCode::EvalOverflow, message)
}

fn fault(code: DiagnosticCode, message: impl Into<String>) -> ErrorKind {
    ErrorKind::Fault(Fault::new(code, message))
}

/// A cached result of evaluating a constant or a static initializer.
enum Slot<T> {
    InProgress,
    Done(Result<T, EvalError>),
}

/// The evaluated initializer of a `static`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticValue {
    /// `Some` for a `Setting`/`Meta` handle; `value` is then its default value.
    pub handle: Option<HandleKind>,
    pub value: ConstValue,
}

/// Evaluated constants by item and parent generic arguments.
type ConstCache = FxHashMap<(ItemId, Vec<GenericArg>), Slot<ConstValue>>;

/// What every evaluation of one program shares: the database, std items, signatures and
/// the caches of constants and statics.
pub(crate) struct Ctx<'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub std: &'db StdTable,
    pub sigs: &'db ProgramSignatures,
    consts: RefCell<ConstCache>,
    statics: RefCell<FxHashMap<ItemId, Slot<StaticValue>>>,
    /// Non-generic constants and statics whose evaluation reached themselves. A use inside
    /// the cycle aborts (a non-generic item reports its own errors); the item whose
    /// evaluation closed the cycle reports it once, at its own name.
    cycles: RefCell<FxHashSet<ItemId>>,
    /// The nesting of interpreted calls, across machines.
    depth: Cell<u32>,
}

/// The body of an item with everything evaluation reads.
#[derive(Clone, Copy)]
pub(crate) struct BodyRef<'db> {
    pub item: ItemId,
    pub body: &'db Body,
    pub res: &'db BodyResolution,
    pub infer: &'db InferenceResult,
}

impl<'db> Ctx<'db> {
    pub fn new(
        db: &'db dyn Db,
        program: Program,
        std: &'db StdTable,
        sigs: &'db ProgramSignatures,
    ) -> Ctx<'db> {
        Ctx {
            db,
            program,
            std,
            sigs,
            consts: RefCell::default(),
            statics: RefCell::default(),
            cycles: RefCell::default(),
            depth: Cell::new(0),
        }
    }

    pub fn body(&self, item: ItemId) -> Option<BodyRef<'db>> {
        let index = item.index as usize;
        let body = file_hir(self.db, item.file).bodies.get(index)?.as_ref()?;
        let res = file_resolution(self.db, self.program, item.file)
            .items
            .get(index)?
            .as_ref()?
            .body
            .as_ref()?;
        let infer = inference(self.db, self.program, item)?;
        Some(BodyRef {
            item,
            body,
            res,
            infer,
        })
    }

    fn generics(&self, item: ItemId) -> Option<&'db Generics> {
        match self.sigs.get(item)? {
            ItemSig::Fn(sig) => Some(&sig.generics),
            ItemSig::Const { generics, .. } => Some(generics),
            _ => None,
        }
    }

    pub fn env_for(&self, item: ItemId, args: Vec<GenericArg>) -> GenericEnv {
        let generics = self.generics(item);
        GenericEnv {
            item: Some(item),
            parent: generics.and_then(|generics| generics.parent),
            parent_count: generics.map_or(0, |generics| generics.parent_count),
            args,
        }
    }

    /// The value of a `const` item with its parent impl's generic arguments.
    pub fn const_value(
        &self,
        item: ItemId,
        args: Vec<GenericArg>,
    ) -> Result<ConstValue, EvalError> {
        let key = (item, args);
        match self.consts.borrow().get(&key) {
            Some(Slot::Done(result)) => return result.clone(),
            Some(Slot::InProgress) => {
                if key.1.is_empty() {
                    self.cycles.borrow_mut().insert(item);
                }
                return Err(self.cycle_error(item));
            }
            None => {}
        }
        self.consts
            .borrow_mut()
            .insert(key.clone(), Slot::InProgress);
        let result = self
            .eval_initializer(item, key.1.clone())
            .and_then(|value| {
                let mut machine = Machine::new(self);
                machine
                    .freeze(&value)
                    .map_err(|kind| EvalError { kind, span: None })
            });
        let result = if key.1.is_empty() && self.cycles.borrow_mut().remove(&item) {
            Err(self.cycle_error(item))
        } else {
            result
        };
        let mut consts = self.consts.borrow_mut();
        if self.nested_budget_failure(&result) {
            consts.remove(&key);
        } else {
            consts.insert(key, Slot::Done(result.clone()));
        }
        result
    }

    /// The error of a constant or static whose value depends on itself: it cannot be
    /// computed at compile time.
    fn cycle_error(&self, item: ItemId) -> EvalError {
        EvalError {
            kind: fault(
                DiagnosticCode::EvalConstCycle,
                format!(
                    "the value of `{}` depends on itself, so it cannot be computed at compile \
                     time",
                    item_name(self.db, item)
                ),
            ),
            span: None,
        }
    }

    /// The evaluated initializer of a `static`.
    pub fn static_value(&self, item: ItemId) -> Result<StaticValue, EvalError> {
        match self.statics.borrow().get(&item) {
            Some(Slot::Done(result)) => return result.clone(),
            Some(Slot::InProgress) => {
                self.cycles.borrow_mut().insert(item);
                return Err(self.cycle_error(item));
            }
            None => {}
        }
        self.statics.borrow_mut().insert(item, Slot::InProgress);
        let result = self.eval_initializer(item, Vec::new()).and_then(|value| {
            let mut machine = Machine::new(self);
            let (handle, value) = match value {
                Value::NewHandle { kind, init } => (Some(kind), *init),
                value => (None, value),
            };
            let value = machine
                .freeze(&value)
                .map_err(|kind| EvalError { kind, span: None })?;
            Ok(StaticValue { handle, value })
        });
        let result = if self.cycles.borrow_mut().remove(&item) {
            Err(self.cycle_error(item))
        } else {
            result
        };
        let mut statics = self.statics.borrow_mut();
        if self.nested_budget_failure(&result) {
            statics.remove(&item);
        } else {
            statics.insert(item, Slot::Done(result.clone()));
        }
        result
    }

    /// Whether `result` ran out of budget while nested in another evaluation: the failure
    /// may be the outer evaluation's call depth, so it is not the value's own and is not
    /// cached.
    fn nested_budget_failure<T>(&self, result: &Result<T, EvalError>) -> bool {
        self.depth.get() > 0
            && matches!(
                result,
                Err(EvalError { kind: ErrorKind::Fault(fault), .. })
                    if fault.code == DiagnosticCode::EvalBudgetExceeded
            )
    }

    fn eval_initializer(&self, item: ItemId, args: Vec<GenericArg>) -> Result<Value, EvalError> {
        let Some(body) = self.body(item) else {
            return Err(EvalError::abort());
        };
        let env = self.env_for(item, args);
        let mut machine = Machine::new(self);
        let mut frame = Frame::new(body, env);
        frame.initializer = true;
        machine.run(&mut frame, body.body.value)
    }
}

/// One activation: a function, closure, constant or the constant part of a story body.
pub(crate) struct Frame<'db> {
    pub b: BodyRef<'db>,
    pub env: GenericEnv,
    pub locals: FxHashMap<LocalId, usize>,
    /// The constant locals of a `pushdown fn` body, evaluated when first read.
    pub lazy: Option<&'db ConstExprs>,
    /// A `const` or `static` initializer: `?` has no function to return from.
    pub initializer: bool,
    /// Subexpressions whose value is already computed, in these heap cells: the story
    /// analysis evaluates the steps and scene calls inside an expression first.
    pub overrides: FxHashMap<ExprId, usize>,
}

impl<'db> Frame<'db> {
    pub fn new(b: BodyRef<'db>, env: GenericEnv) -> Frame<'db> {
        Frame {
            b,
            env,
            locals: FxHashMap::default(),
            lazy: None,
            initializer: false,
            overrides: FxHashMap::default(),
        }
    }

    pub fn span(&self, expr: ExprId) -> Span {
        Span::new(self.b.item.file, self.b.body.store.expr_range(expr))
    }

    fn expr(&self, expr: ExprId) -> &'db Expr {
        self.b.body.store.expr(expr)
    }

    /// The type of `expr` with the frame's generic arguments substituted.
    pub fn ty(&self, expr: ExprId) -> Option<Ty> {
        let ty = self.b.infer.expr_types.get(expr)?;
        Some(subst_ty(ty, &self.env))
    }
}

/// Matching state: a value, or a place reached through a reference (bindings then bind
/// references, as Rust's default binding modes do).
enum Scrut {
    Val(Value),
    Place(Place),
}

/// The head of a scrutinee, enough to test a pattern without copying the whole value.
enum Head {
    Adt { item: ItemId, variant: u32 },
    Tuple,
    Leaf(Value),
}

/// One evaluation with its own heap and step budget.
pub(crate) struct Machine<'c, 'db> {
    pub ctx: &'c Ctx<'db>,
    pub heap: Vec<Value>,
    steps: u64,
    /// Story mode (stage 5): the heap cells of the story variables. Story variables can
    /// then be read and written, and instant procedures called.
    pub story: Option<FxHashMap<ItemId, usize>>,
}

impl<'c, 'db> Machine<'c, 'db> {
    pub fn new(ctx: &'c Ctx<'db>) -> Machine<'c, 'db> {
        Machine {
            ctx,
            heap: Vec::new(),
            steps: 0,
            story: None,
        }
    }

    pub fn std(&self) -> &'db StdTable {
        self.ctx.std
    }

    /// Evaluates `expr` as the body of `frame`, catching `return`.
    pub fn run(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> Result<Value, EvalError> {
        match self.eval(frame, expr) {
            Ok(value) | Err(Exit::Return(value)) => Ok(value),
            Err(Exit::Break(..) | Exit::Continue(_)) => Err(EvalError::abort()),
            Err(Exit::Error(error)) => Err(error),
        }
    }

    pub fn alloc(&mut self, value: Value) -> usize {
        self.heap.push(value);
        self.heap.len().saturating_sub(1)
    }

    fn fail(frame: &Frame<'_>, expr: ExprId, kind: ErrorKind) -> Exit {
        Exit::Error(EvalError {
            kind,
            span: Some(frame.span(expr)),
        })
    }

    fn tick(&mut self, frame: &Frame<'_>, expr: ExprId, count: u64) -> R<()> {
        self.steps = self.steps.saturating_add(count);
        if self.steps > EVAL_STEP_BUDGET {
            return Err(Self::fail(frame, expr, budget_exceeded()));
        }
        Ok(())
    }

    // ----- Places -------------------------------------------------------------------

    /// The value at `place` (ignoring `obj`).
    pub fn place_mut(&mut self, place: &Place) -> Result<&mut Value, ErrorKind> {
        let mut value = self.heap.get_mut(place.cell).ok_or(ErrorKind::Abort)?;
        for proj in &place.path {
            value = match (proj, value) {
                (_, Value::Probe) => return Err(ErrorKind::Inspected),
                (Proj::Field(index), Value::Tuple(fields) | Value::Adt { fields, .. }) => {
                    fields.get_mut(*index as usize).ok_or(ErrorKind::Abort)?
                }
                (Proj::Index(index), Value::Array(items) | Value::Vec(items)) => {
                    items.get_mut(*index).ok_or(ErrorKind::Abort)?
                }
                _ => return Err(ErrorKind::Abort),
            };
        }
        Ok(value)
    }

    pub fn read_place(&mut self, place: &Place) -> Result<Value, ErrorKind> {
        if place.obj.is_some() {
            return Err(ErrorKind::Abort);
        }
        let value = self.place_mut(place)?;
        if matches!(value, Value::Uninit) {
            return Err(ErrorKind::Abort);
        }
        Ok(value.clone())
    }

    pub(crate) fn write_place(&mut self, place: &Place, value: Value) -> Result<(), ErrorKind> {
        if place.obj.is_some() {
            return Err(ErrorKind::Abort);
        }
        *self.place_mut(place)? = value;
        Ok(())
    }

    /// The place a reference points to, following references stored at that place.
    pub fn ref_place(&mut self, value: &Value) -> Result<Place, ErrorKind> {
        let Value::Ref(place) = value else {
            return match value {
                Value::Probe => Err(ErrorKind::Inspected),
                _ => Err(ErrorKind::Abort),
            };
        };
        let mut place = (**place).clone();
        while place.obj.is_none() {
            match self.place_mut(&place)? {
                Value::Ref(next) => place = (**next).clone(),
                _ => break,
            }
        }
        Ok(place)
    }

    /// `value` with references followed.
    pub fn deref_value(&mut self, value: Value) -> Result<Value, ErrorKind> {
        match value {
            Value::Ref(_) => {
                let place = self.ref_place(&value)?;
                self.read_place(&place)
            }
            Value::Probe => Err(ErrorKind::Inspected),
            value => Ok(value),
        }
    }

    /// The screen at `place`.
    pub fn screen_at(&mut self, place: &Place) -> Result<&mut Screen, ErrorKind> {
        match self.place_mut(&place.without_obj())? {
            Value::Screen(screen) => Ok(screen),
            Value::Probe => Err(ErrorKind::Inspected),
            _ => Err(ErrorKind::Abort),
        }
    }

    fn local_cell(&mut self, frame: &mut Frame<'db>, local: LocalId) -> R<usize> {
        if let Some(cell) = frame.locals.get(&local) {
            return Ok(*cell);
        }
        let Some(lazy) = frame.lazy else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let Some(source) = lazy.locals.get(&local).copied() else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let value = self.eval(frame, source.source)?;
        if !self.match_pat(frame, source.pat, Scrut::Val(value))? {
            return Err(Exit::Error(EvalError::abort()));
        }
        frame
            .locals
            .get(&local)
            .copied()
            .ok_or_else(|| Exit::Error(EvalError::abort()))
    }

    /// The place `expr` denotes; a temporary for an expression that is not a place.
    pub(crate) fn eval_place(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Place> {
        if let Some(cell) = frame.overrides.get(&expr) {
            return Ok(Place::cell(*cell));
        }
        match frame.expr(expr) {
            Expr::Path(_) => match frame.b.infer.paths.get(&expr) {
                Some(ValueRef::Local(local)) => {
                    let cell = self.local_cell(frame, *local)?;
                    return Ok(Place::cell(cell));
                }
                Some(ValueRef::Static(item)) => {
                    if let Some(cell) = self.story.as_ref().and_then(|story| story.get(item)) {
                        return Ok(Place::cell(*cell));
                    }
                }
                _ => {}
            },
            Expr::Field { base, .. } => {
                let Some(field) = frame.b.infer.fields.get(&expr).copied() else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let mut place = self.eval_place(frame, *base)?;
                for _ in 0..field.derefs {
                    place = self.deref_place(frame, expr, &place)?;
                }
                self.check_projectable(frame, expr, &place)?;
                return Ok(place.project(Proj::Field(field.index)));
            }
            Expr::Index { base, index } => {
                let mut place = self.eval_place(frame, *base)?;
                let index_value = self.eval(frame, *index)?;
                let index_value = self.int(frame, *index, index_value)?;
                loop {
                    let len = match self.place_mut(&place) {
                        Ok(Value::Array(items) | Value::Vec(items)) => items.len(),
                        Ok(Value::Ref(_)) => {
                            place = self.deref_place(frame, expr, &place)?;
                            continue;
                        }
                        Ok(_) => return Err(Exit::Error(EvalError::abort())),
                        Err(kind) => return Err(Self::fail(frame, expr, kind)),
                    };
                    let Some(position) = usize::try_from(index_value).ok().filter(|i| *i < len)
                    else {
                        return Err(Self::fail(
                            frame,
                            expr,
                            fault(
                                DiagnosticCode::EvalIndexOutOfRange,
                                format!("index {index_value} is out of range for length {len}"),
                            ),
                        ));
                    };
                    return Ok(place.project(Proj::Index(position)));
                }
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => {
                let value = self.eval(frame, *inner)?;
                return self
                    .ref_place(&value)
                    .map_err(|kind| Self::fail(frame, expr, kind));
            }
            _ => {}
        }
        let value = self.eval(frame, expr)?;
        Ok(Place::cell(self.alloc(value)))
    }

    pub(crate) fn deref_place(
        &mut self,
        frame: &Frame<'_>,
        expr: ExprId,
        place: &Place,
    ) -> R<Place> {
        match self.place_mut(place) {
            Ok(Value::Ref(next)) => Ok((**next).clone()),
            Ok(Value::Probe) | Err(ErrorKind::Inspected) => {
                Err(Self::fail(frame, expr, ErrorKind::Inspected))
            }
            _ => Err(Exit::Error(EvalError::abort())),
        }
    }

    fn check_projectable(&mut self, frame: &Frame<'_>, expr: ExprId, place: &Place) -> R<()> {
        match self.place_mut(place) {
            Ok(Value::Probe) | Err(ErrorKind::Inspected) => {
                Err(Self::fail(frame, expr, ErrorKind::Inspected))
            }
            _ => Ok(()),
        }
    }

    // ----- Expressions --------------------------------------------------------------

    /// Evaluates `expr`, growing the native stack on demand so that nesting is bounded only
    /// by the step budget and [`MAX_CALL_DEPTH`].
    pub fn eval(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        stacker::maybe_grow(STACK_RED_ZONE, STACK_GROWTH, || {
            self.eval_inner(frame, expr)
        })
    }

    /// Every arm lives in its own non-inlined method so that this frame, which recursion
    /// stacks up, stays small.
    #[inline(never)]
    fn eval_inner(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        self.tick(frame, expr, 1)?;
        if let Some(cell) = frame.overrides.get(&expr) {
            return self
                .read_place(&Place::cell(*cell))
                .map_err(|kind| Self::fail(frame, expr, kind));
        }
        if matches!(frame.b.infer.expr_types.get(expr), Some(Ty::Error)) {
            return Err(Exit::Error(EvalError::abort()));
        }
        match frame.expr(expr) {
            Expr::Missing => Err(Exit::Error(EvalError::abort())),
            Expr::Literal(literal) => self.literal(frame, expr, literal),
            Expr::Path(_) => self.path(frame, expr),
            Expr::Block { stmts, tail, .. } => self.block(frame, stmts, *tail),
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => self.if_expr(frame, cond, *then_branch, *else_branch),
            Expr::Match { scrutinee, arms } => self.match_expr(frame, *scrutinee, arms),
            Expr::Loop { body, .. } => self.loop_expr(frame, expr, *body),
            Expr::While { cond, body, .. } => self.while_expr(frame, expr, cond, *body),
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => self.for_loop(frame, expr, *pat, *iterable, *body),
            Expr::Break { .. } | Expr::Continue { .. } | Expr::Return { .. } => {
                self.jump(frame, expr)
            }
            Expr::Call { callee, args } => self.call_expr(frame, expr, *callee, args),
            Expr::MethodCall { receiver, args, .. } => {
                self.method_call(frame, expr, *receiver, args)
            }
            Expr::Field { .. } | Expr::Index { .. } => self.read_expr_place(frame, expr),
            Expr::Try(inner) => self.try_expr(frame, *inner),
            Expr::Cast { expr: inner, .. } => self.cast(frame, expr, *inner),
            Expr::Unary { op, expr: inner } => self.unary(frame, expr, *op, *inner),
            Expr::Ref { expr: inner, .. } => self.ref_expr(frame, *inner),
            Expr::Binary { op, lhs, rhs } => self.binary(frame, expr, *op, *lhs, *rhs),
            Expr::Assign { op, target, value } => self.assign(frame, expr, *op, *target, *value),
            Expr::Range {
                start,
                end,
                inclusive,
            } => self.range_expr(frame, *start, *end, *inclusive),
            Expr::Tuple(items) => self.list(frame, items).map(Value::Tuple),
            Expr::Array(ArrayExpr::List(items)) => self.list(frame, items).map(Value::Array),
            Expr::Array(ArrayExpr::Repeat { value, count }) => {
                self.repeat(frame, expr, *value, *count)
            }
            Expr::Record { fields, .. } => self.record(frame, expr, fields),
            Expr::Closure { .. } => self.closure(frame, expr),
        }
    }

    #[inline(never)]
    fn block(&mut self, frame: &mut Frame<'db>, stmts: &[Stmt], tail: Option<ExprId>) -> R<Value> {
        for stmt in stmts {
            self.stmt(frame, stmt)?;
        }
        match tail {
            Some(tail) => self.eval(frame, tail),
            None => Ok(Value::unit()),
        }
    }

    #[inline(never)]
    fn if_expr(
        &mut self,
        frame: &mut Frame<'db>,
        cond: &Condition,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    ) -> R<Value> {
        if self.condition(frame, cond)? {
            self.eval(frame, then_branch)
        } else if let Some(else_branch) = else_branch {
            self.eval(frame, else_branch)
        } else {
            Ok(Value::unit())
        }
    }

    #[inline(never)]
    fn match_expr(
        &mut self,
        frame: &mut Frame<'db>,
        scrutinee: ExprId,
        arms: &[crate::hir::MatchArm],
    ) -> R<Value> {
        let value = self.eval(frame, scrutinee)?;
        for arm in arms {
            if !self.match_pat(frame, arm.pat, Scrut::Val(value.clone()))? {
                continue;
            }
            if let Some(guard) = arm.guard {
                let guard_value = self.eval(frame, guard)?;
                if !self.bool(frame, guard, guard_value)? {
                    continue;
                }
            }
            return self.eval(frame, arm.body);
        }
        // Stage 3 reports non-exhaustive matches.
        Err(Exit::Error(EvalError::abort()))
    }

    #[inline(never)]
    fn loop_expr(&mut self, frame: &mut Frame<'db>, expr: ExprId, body: ExprId) -> R<Value> {
        loop {
            match self.eval(frame, body) {
                Ok(_) => {}
                Err(Exit::Break(target, value)) if target == expr => return Ok(value),
                Err(Exit::Continue(target)) if target == expr => {}
                Err(exit) => return Err(exit),
            }
        }
    }

    #[inline(never)]
    fn while_expr(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        cond: &Condition,
        body: ExprId,
    ) -> R<Value> {
        while self.condition(frame, cond)? {
            match self.eval(frame, body) {
                Ok(_) => {}
                Err(Exit::Break(target, _)) if target == expr => break,
                Err(Exit::Continue(target)) if target == expr => {}
                Err(exit) => return Err(exit),
            }
        }
        Ok(Value::unit())
    }

    /// `break`, `continue` and `return`.
    #[inline(never)]
    fn jump(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        let (Expr::Break { value, .. } | Expr::Return { value }) = frame.expr(expr) else {
            return match frame.b.res.loop_targets.get(&expr).copied() {
                Some(target) => Err(Exit::Continue(target)),
                None => Err(Exit::Error(EvalError::abort())),
            };
        };
        let value = match value {
            Some(value) => self.eval(frame, *value)?,
            None => Value::unit(),
        };
        if matches!(frame.expr(expr), Expr::Return { .. }) {
            return Err(Exit::Return(value));
        }
        match frame.b.res.loop_targets.get(&expr).copied() {
            Some(target) => Err(Exit::Break(target, value)),
            None => Err(Exit::Error(EvalError::abort())),
        }
    }

    #[inline(never)]
    fn method_call(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        receiver: ExprId,
        args: &[ExprId],
    ) -> R<Value> {
        let Some(call) = frame.b.infer.method_calls.get(&expr) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let fn_ref = subst_fn_ref(&call.func, &frame.env);
        let receiver = self.receiver(frame, expr, receiver, call.receiver)?;
        let mut values = vec![receiver];
        for arg in args {
            values.push(self.eval(frame, *arg)?);
        }
        self.call(frame, expr, &fn_ref, values)
    }

    #[inline(never)]
    fn read_expr_place(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        let place = self.eval_place(frame, expr)?;
        self.read_place(&place)
            .map_err(|kind| Self::fail(frame, expr, kind))
    }

    #[inline(never)]
    fn cast(&mut self, frame: &mut Frame<'db>, expr: ExprId, inner: ExprId) -> R<Value> {
        let value = self.eval(frame, inner)?;
        let value = self.int(frame, inner, value)?;
        match frame.ty(expr) {
            Some(Ty::Int(int)) => match num::wrap_to(value, int) {
                Some(value) => Ok(Value::Int(value)),
                None => Err(Self::fail(
                    frame,
                    expr,
                    overflow(None, &Ty::Int(int)).into(),
                )),
            },
            _ => Ok(Value::Int(value)),
        }
    }

    #[inline(never)]
    fn ref_expr(&mut self, frame: &mut Frame<'db>, inner: ExprId) -> R<Value> {
        let place = self.eval_place(frame, inner)?;
        Ok(Value::Ref(Box::new(place)))
    }

    #[inline(never)]
    fn assign(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        op: Option<BinaryOp>,
        target: ExprId,
        value: ExprId,
    ) -> R<Value> {
        let value = self.eval(frame, value)?;
        let place = self.eval_place(frame, target)?;
        let value = match op {
            Some(op) => {
                let current = self
                    .read_place(&place)
                    .map_err(|kind| Self::fail(frame, expr, kind))?;
                let ty = frame.ty(target);
                self.compound(frame, expr, op, current, value, ty.as_ref())?
            }
            None => value,
        };
        self.write_place(&place, value)
            .map_err(|kind| Self::fail(frame, expr, kind))?;
        Ok(Value::unit())
    }

    #[inline(never)]
    fn range_expr(
        &mut self,
        frame: &mut Frame<'db>,
        start: Option<ExprId>,
        end: Option<ExprId>,
        inclusive: bool,
    ) -> R<Value> {
        let (Some(start), Some(end)) = (start, end) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let start = self.eval(frame, start)?;
        let end = self.eval(frame, end)?;
        Ok(self.range_value(start, end, inclusive))
    }

    #[inline(never)]
    fn list(&mut self, frame: &mut Frame<'db>, items: &[ExprId]) -> R<Vec<Value>> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.eval(frame, *item)?);
        }
        Ok(values)
    }

    #[inline(never)]
    fn repeat(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        value: ExprId,
        count: ExprId,
    ) -> R<Value> {
        let value = self.eval(frame, value)?;
        let count_value = self.eval(frame, count)?;
        let count_value = self.int(frame, count, count_value)?;
        let Ok(count_value) = u64::try_from(count_value) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        self.tick(frame, expr, count_value)?;
        let len = usize::try_from(count_value).unwrap_or(usize::MAX);
        Ok(Value::Array(vec![value; len]))
    }

    #[inline(never)]
    fn closure(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        let Some(info) = frame.b.infer.closures.get(&expr) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let mut captures = Vec::with_capacity(info.captures.len());
        for local in &info.captures {
            let cell = self.local_cell(frame, *local)?;
            let value = self
                .read_place(&Place::cell(cell))
                .map_err(|kind| Self::fail(frame, expr, kind))?;
            captures.push((*local, value));
        }
        Ok(Value::Closure(Box::new(ClosureValue {
            owner: frame.b.item,
            expr,
            env: frame.env.clone(),
            captures,
        })))
    }

    fn stmt(&mut self, frame: &mut Frame<'db>, stmt: &Stmt) -> R<()> {
        match stmt {
            Stmt::Let { pat, init, .. } => match init {
                Some(init) => {
                    let value = self.eval(frame, *init)?;
                    self.bind(frame, *pat, value)
                }
                None => {
                    self.declare(frame, *pat);
                    Ok(())
                }
            },
            Stmt::Expr { expr, .. } => self.eval(frame, *expr).map(drop),
            Stmt::Item(_) => Ok(()),
        }
    }

    /// Allocates uninitialized cells for every binding of `pat`.
    pub(crate) fn declare(&mut self, frame: &mut Frame<'db>, pat: PatId) {
        let store = &frame.b.body.store;
        let mut stack = vec![pat];
        while let Some(pat) = stack.pop() {
            match store.pat(pat) {
                Pat::Bind { .. } => {
                    if let Some(local) = frame.b.res.bindings.get(&pat).copied() {
                        let cell = self.alloc(Value::Uninit);
                        frame.locals.insert(local, cell);
                    }
                }
                Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
                    stack.extend(pats.iter().copied());
                }
                Pat::Record { fields, .. } => stack.extend(fields.iter().map(|(_, pat)| *pat)),
                _ => {}
            }
        }
    }

    /// Binds an irrefutable pattern (stage 3 rejects refutable ones, so a mismatch aborts
    /// silently).
    pub(crate) fn bind(&mut self, frame: &mut Frame<'db>, pat: PatId, value: Value) -> R<()> {
        if self.match_pat(frame, pat, Scrut::Val(value))? {
            Ok(())
        } else {
            Err(Exit::Error(EvalError::abort()))
        }
    }

    fn condition(&mut self, frame: &mut Frame<'db>, cond: &Condition) -> R<bool> {
        match cond {
            Condition::Expr(expr) => {
                let value = self.eval(frame, *expr)?;
                self.bool(frame, *expr, value)
            }
            Condition::Let { pat, expr } => {
                let value = self.eval(frame, *expr)?;
                self.match_pat(frame, *pat, Scrut::Val(value))
            }
        }
    }

    fn bool(&mut self, frame: &Frame<'_>, expr: ExprId, value: Value) -> R<bool> {
        match self.deref_value(value) {
            Ok(Value::Bool(value)) => Ok(value),
            Ok(_) => Err(Exit::Error(EvalError::abort())),
            Err(kind) => Err(Self::fail(frame, expr, kind)),
        }
    }

    pub(crate) fn int(&mut self, frame: &Frame<'_>, expr: ExprId, value: Value) -> R<i128> {
        match self.deref_value(value) {
            Ok(Value::Int(value)) => Ok(value),
            Ok(_) => Err(Exit::Error(EvalError::abort())),
            Err(kind) => Err(Self::fail(frame, expr, kind)),
        }
    }

    fn literal(&mut self, frame: &Frame<'_>, expr: ExprId, literal: &Literal) -> R<Value> {
        match literal {
            Literal::Int { value, .. } => match i128::try_from(*value) {
                Ok(value) => Ok(Value::Int(value)),
                Err(_) => Err(Self::fail(
                    frame,
                    expr,
                    fault(
                        DiagnosticCode::EvalOverflow,
                        "integer literal does not fit in `i128`",
                    ),
                )),
            },
            Literal::Decimal(text) => match Decimal::parse(text) {
                Some(value) => Ok(Value::Decimal(value)),
                None => Err(Self::fail(
                    frame,
                    expr,
                    fault(
                        DiagnosticCode::EvalOverflow,
                        "decimal literal has too many digits",
                    ),
                )),
            },
            Literal::String(text) => Ok(Value::Text(Text::plain(text.clone()))),
            Literal::Bool(value) => Ok(Value::Bool(*value)),
        }
    }

    fn path(&mut self, frame: &mut Frame<'db>, expr: ExprId) -> R<Value> {
        let Some(path) = frame.b.infer.paths.get(&expr) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        match path {
            ValueRef::Local(local) => {
                let cell = self.local_cell(frame, *local)?;
                self.read_place(&Place::cell(cell))
                    .map_err(|kind| Self::fail(frame, expr, kind))
            }
            ValueRef::Fn(fn_ref) => Ok(Value::Fn(subst_fn_ref(fn_ref, &frame.env))),
            ValueRef::Ctor(ctor) => Ok(Value::Adt {
                item: ctor.adt,
                variant: ctor.variant.unwrap_or(0),
                fields: Vec::new(),
            }),
            ValueRef::Const { item, args } => {
                let args: Vec<GenericArg> =
                    args.iter().map(|arg| subst_arg(arg, &frame.env)).collect();
                let generic = !args.is_empty();
                match self.ctx.const_value(*item, args) {
                    Ok(value) => Ok(self.thaw(&value)),
                    // A non-generic constant reports its own errors.
                    Err(_) if !generic => Err(Exit::Error(EvalError::abort())),
                    Err(error) => Err(Exit::Error(EvalError {
                        span: error.span.or(Some(frame.span(expr))),
                        kind: error.kind,
                    })),
                }
            }
            ValueRef::Static(item) => {
                let mutable = matches!(
                    item_tree(self.ctx.db, item.file)
                        .item(item.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Static { mutable: true })
                );
                if mutable {
                    // Story variables in const code are reported by stage 3.
                    let Some(cell) = self.story.as_ref().and_then(|story| story.get(item)) else {
                        return Err(Exit::Error(EvalError::abort()));
                    };
                    let cell = *cell;
                    return self
                        .read_place(&Place::cell(cell))
                        .map_err(|kind| Self::fail(frame, expr, kind));
                }
                match self.ctx.static_value(*item) {
                    Ok(StaticValue {
                        handle: Some(kind), ..
                    }) => Ok(Value::Handle { kind, item: *item }),
                    Ok(StaticValue {
                        handle: None,
                        value,
                    }) => Ok(self.thaw(&value)),
                    Err(_) => Err(Exit::Error(EvalError::abort())),
                }
            }
            ValueRef::ConstParam { owner, index } => match frame.env.arg(*owner, *index) {
                Some(GenericArg::Const(Const::Value(value))) => Ok(Value::Int(*value)),
                _ => Err(Exit::Error(EvalError::abort())),
            },
            ValueRef::Error => Err(Exit::Error(EvalError::abort())),
        }
    }

    fn for_loop(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        pat: PatId,
        iterable: ExprId,
        body: ExprId,
    ) -> R<Value> {
        let value = self.eval(frame, iterable)?;
        let value = self
            .deref_value(value)
            .map_err(|kind| Self::fail(frame, iterable, kind))?;
        let range = self.std().range.clone();
        match value {
            Value::Adt { item, fields, .. } if item == range.item => {
                let field = |name: &str| range.field(name).and_then(|index| fields.get(index));
                let (Some(Value::Int(start)), Some(Value::Int(end)), Some(Value::Bool(inclusive))) =
                    (field("start"), field("end"), field("inclusive"))
                else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let (start, end, inclusive) = (*start, *end, *inclusive);
                let mut current = start;
                while current < end || (inclusive && current == end) {
                    self.bind(frame, pat, Value::Int(current))?;
                    match self.eval(frame, body) {
                        Ok(_) => {}
                        Err(Exit::Break(target, _)) if target == expr => break,
                        Err(Exit::Continue(target)) if target == expr => {}
                        Err(exit) => return Err(exit),
                    }
                    match current.checked_add(1) {
                        Some(next) => current = next,
                        None => break,
                    }
                }
                Ok(Value::unit())
            }
            Value::Array(items) | Value::Vec(items) => {
                for item in items {
                    self.bind(frame, pat, item)?;
                    match self.eval(frame, body) {
                        Ok(_) => {}
                        Err(Exit::Break(target, _)) if target == expr => break,
                        Err(Exit::Continue(target)) if target == expr => {}
                        Err(exit) => return Err(exit),
                    }
                }
                Ok(Value::unit())
            }
            _ => Err(Exit::Error(EvalError::abort())),
        }
    }

    pub(crate) fn range_value(&self, start: Value, end: Value, inclusive: bool) -> Value {
        let range = &self.std().range;
        let mut fields = vec![Value::unit(), Value::unit(), Value::Bool(inclusive)];
        for (name, value) in [
            ("start", start),
            ("end", end),
            ("inclusive", Value::Bool(inclusive)),
        ] {
            if let Some(slot) = range.field(name).and_then(|index| fields.get_mut(index)) {
                *slot = value;
            }
        }
        Value::Adt {
            item: range.item,
            variant: 0,
            fields,
        }
    }

    fn record(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        fields: &[(crate::hir::Name, ExprId)],
    ) -> R<Value> {
        let Some(ctor) = frame.b.infer.records.get(&expr) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let variant = ctor.variant.unwrap_or(0);
        let shape = adt_variants(self.ctx.db, ctor.adt)
            .into_iter()
            .nth(variant as usize)
            .map(|(_, shape)| shape);
        let Some(Shape::Record(names)) = shape else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let mut values: Vec<Option<Value>> = vec![None; names.len()];
        for (name, value) in fields {
            let value = self.eval(frame, *value)?;
            let Some(slot) = names
                .iter()
                .position(|field| field == name)
                .and_then(|index| values.get_mut(index))
            else {
                return Err(Exit::Error(EvalError::abort()));
            };
            *slot = Some(value);
        }
        let Some(values) = values.into_iter().collect::<Option<Vec<_>>>() else {
            return Err(Exit::Error(EvalError::abort()));
        };
        Ok(Value::Adt {
            item: ctor.adt,
            variant,
            fields: values,
        })
    }

    fn try_expr(&mut self, frame: &mut Frame<'db>, inner: ExprId) -> R<Value> {
        let value = self.eval(frame, inner)?;
        let value = self
            .deref_value(value)
            .map_err(|kind| Self::fail(frame, inner, kind))?;
        let std = self.std();
        let Value::Adt {
            item,
            variant,
            mut fields,
        } = value
        else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let success = if item == std.option.item {
            std.option.variant("Some")
        } else if item == std.result.item {
            std.result.variant("Ok")
        } else {
            return Err(Exit::Error(EvalError::abort()));
        };
        if Some(variant) == success {
            return fields.pop().ok_or_else(|| Exit::Error(EvalError::abort()));
        }
        if frame.initializer {
            // Stage 3 rejects `?` outside a function returning `Option`/`Result`.
            return Err(Exit::Error(EvalError::abort()));
        }
        Err(Exit::Return(Value::Adt {
            item,
            variant,
            fields,
        }))
    }

    fn unary(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        op: UnaryOp,
        inner: ExprId,
    ) -> R<Value> {
        if op == UnaryOp::Deref {
            let place = self.eval_place(frame, expr)?;
            return self
                .read_place(&place)
                .map_err(|kind| Self::fail(frame, expr, kind));
        }
        let value = self.eval(frame, inner)?;
        let value = self
            .deref_value(value)
            .map_err(|kind| Self::fail(frame, inner, kind))?;
        let ty = frame.ty(expr).unwrap_or(Ty::Error);
        let result = match (op, value) {
            (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
            (UnaryOp::Not, Value::Int(value)) => {
                let int = match ty {
                    Ty::Int(int) => int,
                    _ => IntTy::I128,
                };
                num::bit_not(value, int)
                    .map(Value::Int)
                    .ok_or_else(|| overflow(None, &ty).into())
            }
            (UnaryOp::Neg, Value::Int(value)) => match value.checked_neg() {
                Some(negated) => self.check_int(negated, &ty).map(Value::Int),
                None => Err(overflow(None, &ty).into()),
            },
            (UnaryOp::Neg, Value::Decimal(value)) => value
                .checked_neg()
                .map(Value::Decimal)
                .ok_or_else(|| overflow(None, &ty).into()),
            (UnaryOp::Neg, Value::Rat(value)) => value
                .checked_neg()
                .map(Value::Rat)
                .ok_or_else(|| overflow(None, &ty).into()),
            _ => Err(ErrorKind::Abort),
        };
        result.map_err(|kind| Self::fail(frame, expr, kind))
    }

    fn check_int(&self, value: i128, ty: &Ty) -> Result<i128, ErrorKind> {
        match int_range(ty) {
            Some((lo, hi)) if value < lo || value > hi => Err(overflow(Some(value), ty).into()),
            _ => Ok(value),
        }
    }

    fn binary(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> R<Value> {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let left = self.eval(frame, lhs)?;
            let left = self.bool(frame, lhs, left)?;
            if (op == BinaryOp::And) != left {
                return Ok(Value::Bool(left));
            }
            let right = self.eval(frame, rhs)?;
            return self.bool(frame, rhs, right).map(Value::Bool);
        }
        let left = self.eval(frame, lhs)?;
        let right = self.eval(frame, rhs)?;
        let ty = frame.ty(expr).unwrap_or(Ty::Error);
        let lhs_ty = frame.ty(lhs).unwrap_or(Ty::Error);
        self.binop(op, left, right, &ty, &lhs_ty)
            .map_err(|kind| Self::fail(frame, expr, kind))
    }

    fn compound(
        &mut self,
        frame: &Frame<'_>,
        expr: ExprId,
        op: BinaryOp,
        current: Value,
        value: Value,
        ty: Option<&Ty>,
    ) -> R<Value> {
        let ty = ty.cloned().unwrap_or(Ty::Error);
        if let Ty::Bounded { .. } = ty {
            // `Int` compound assignment saturates at the bounds of the place.
            let result = self.binop(op, current, value, &Ty::Error, &ty);
            return match (result, int_range(&ty)) {
                (Ok(Value::Int(value)), Some((lo, hi))) => Ok(Value::Int(value.clamp(lo, hi))),
                (result, _) => result.map_err(|kind| Self::fail(frame, expr, kind)),
            };
        }
        self.binop(op, current, value, &ty, &ty)
            .map_err(|kind| Self::fail(frame, expr, kind))
    }

    /// A binary operator on evaluated operands; `ty` is the result type (for overflow
    /// checks), `lhs_ty` the left operand's (for shifts).
    pub(crate) fn binop(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
        ty: &Ty,
        lhs_ty: &Ty,
    ) -> Result<Value, ErrorKind> {
        match op {
            BinaryOp::Eq => return self.value_eq(&left, &right).map(Value::Bool),
            BinaryOp::Ne => return self.value_eq(&left, &right).map(|eq| Value::Bool(!eq)),
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let ordering = self.value_cmp(left, right)?;
                return Ok(Value::Bool(match op {
                    BinaryOp::Lt => ordering == Ordering::Less,
                    BinaryOp::Le => ordering != Ordering::Greater,
                    BinaryOp::Gt => ordering == Ordering::Greater,
                    _ => ordering != Ordering::Less,
                }));
            }
            _ => {}
        }
        let left = self.deref_value(left)?;
        let right = self.deref_value(right)?;
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                let int_of = |ty: &Ty| match ty {
                    Ty::Int(int) => *int,
                    _ => IntTy::I128,
                };
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div | BinaryOp::Rem => {
                        if b == 0 {
                            return Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            ));
                        }
                        if op == BinaryOp::Div {
                            a.checked_div(b)
                        } else {
                            a.checked_rem(b)
                        }
                    }
                    BinaryOp::BitAnd => Some(a & b),
                    BinaryOp::BitOr => Some(a | b),
                    BinaryOp::BitXor => Some(a ^ b),
                    BinaryOp::Shl => num::shl(a, b, int_of(lhs_ty)),
                    BinaryOp::Shr => num::shr(a, b, int_of(lhs_ty)),
                    _ => return Err(ErrorKind::Abort),
                };
                match result {
                    Some(value) => self.check_int(value, ty).map(Value::Int),
                    None => Err(overflow(None, ty).into()),
                }
            }
            (Value::Bool(a), Value::Bool(b)) => match op {
                BinaryOp::BitAnd => Ok(Value::Bool(a & b)),
                BinaryOp::BitOr => Ok(Value::Bool(a | b)),
                BinaryOp::BitXor => Ok(Value::Bool(a ^ b)),
                _ => Err(ErrorKind::Abort),
            },
            (Value::Decimal(a), Value::Decimal(b)) => {
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div => {
                        return match a.checked_div(b) {
                            Ok(value) => Ok(Value::Decimal(value)),
                            Err(true) => Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            )),
                            Err(false) => Err(fault(
                                DiagnosticCode::EvalOverflow,
                                format!("{a} / {b} overflows `Decimal`"),
                            )),
                        };
                    }
                    _ => return Err(ErrorKind::Abort),
                };
                result
                    .map(Value::Decimal)
                    .ok_or_else(|| overflow(None, ty).into())
            }
            (Value::Rat(a), Value::Rat(b)) => {
                let result = match op {
                    BinaryOp::Add => a.checked_add(b),
                    BinaryOp::Sub => a.checked_sub(b),
                    BinaryOp::Mul => a.checked_mul(b),
                    BinaryOp::Div => {
                        if b.is_zero() {
                            return Err(fault(
                                DiagnosticCode::EvalDivisionByZero,
                                "attempt to divide by zero",
                            ));
                        }
                        a.checked_div(b)
                    }
                    _ => return Err(ErrorKind::Abort),
                };
                result
                    .map(Value::Rat)
                    .ok_or_else(|| overflow(None, ty).into())
            }
            (Value::Probe, _) | (_, Value::Probe) => Err(ErrorKind::Inspected),
            _ => Err(ErrorKind::Abort),
        }
    }

    /// Structural equality; references compare the values they point to.
    pub(crate) fn value_eq(&mut self, left: &Value, right: &Value) -> Result<bool, ErrorKind> {
        let left = self.deref_value(left.clone())?;
        let right = self.deref_value(right.clone())?;
        Ok(match (&left, &right) {
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Decimal(a), Value::Decimal(b)) => a == b,
            (Value::Rat(a), Value::Rat(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Asset { kind: k1, path: p1 }, Value::Asset { kind: k2, path: p2 }) => {
                k1 == k2 && p1 == p2
            }
            (Value::Fn(a), Value::Fn(b)) => a == b,
            (Value::Handle { item: a, .. }, Value::Handle { item: b, .. }) => a == b,
            (Value::Screen(a), Value::Screen(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b))
            | (Value::Array(a), Value::Array(b))
            | (Value::Vec(a), Value::Vec(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !self.value_eq(a, b)? {
                        return Ok(false);
                    }
                }
                true
            }
            (
                Value::Adt {
                    item: i1,
                    variant: v1,
                    fields: f1,
                },
                Value::Adt {
                    item: i2,
                    variant: v2,
                    fields: f2,
                },
            ) => {
                if i1 != i2 || v1 != v2 || f1.len() != f2.len() {
                    return Ok(false);
                }
                for (a, b) in f1.iter().zip(f2) {
                    if !self.value_eq(a, b)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Closure(_) | Value::Pending(_) | Value::Uninit, _)
            | (_, Value::Closure(_) | Value::Pending(_) | Value::Uninit) => {
                return Err(ErrorKind::Abort);
            }
            _ => false,
        })
    }

    fn value_cmp(&mut self, left: Value, right: Value) -> Result<Ordering, ErrorKind> {
        let left = self.deref_value(left)?;
        let right = self.deref_value(right)?;
        let ordering = match (left, right) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(&b)),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(&b)),
            (Value::Decimal(a), Value::Decimal(b)) => a.checked_cmp(b),
            (Value::Rat(a), Value::Rat(b)) => a.checked_cmp(b),
            (Value::Probe, _) | (_, Value::Probe) => return Err(ErrorKind::Inspected),
            _ => return Err(ErrorKind::Abort),
        };
        ordering.ok_or_else(|| overflow(None, &Ty::Error).into())
    }

    // ----- Calls --------------------------------------------------------------------

    fn receiver(
        &mut self,
        frame: &mut Frame<'db>,
        call: ExprId,
        receiver: ExprId,
        adjust: ReceiverAdjust,
    ) -> R<Value> {
        if adjust.autoref.is_some() {
            let mut place = self.eval_place(frame, receiver)?;
            for _ in 0..adjust.derefs {
                place = self.deref_place(frame, call, &place)?;
            }
            return Ok(Value::Ref(Box::new(place)));
        }
        let mut value = self.eval(frame, receiver)?;
        for _ in 0..adjust.derefs {
            let place = self
                .ref_place(&value)
                .map_err(|kind| Self::fail(frame, call, kind))?;
            value = self
                .read_place(&place)
                .map_err(|kind| Self::fail(frame, call, kind))?;
        }
        Ok(value)
    }

    fn call_expr(
        &mut self,
        frame: &mut Frame<'db>,
        expr: ExprId,
        callee: ExprId,
        args: &[ExprId],
    ) -> R<Value> {
        let direct = match (frame.expr(callee), frame.b.infer.paths.get(&callee)) {
            (Expr::Path(_), Some(ValueRef::Fn(fn_ref))) => Some(subst_fn_ref(fn_ref, &frame.env)),
            _ => None,
        };
        let callee_value = match direct {
            Some(fn_ref) => Value::Fn(fn_ref),
            None => {
                let value = self.eval(frame, callee)?;
                self.deref_value(value)
                    .map_err(|kind| Self::fail(frame, callee, kind))?
            }
        };
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval(frame, *arg)?);
        }
        match callee_value {
            Value::Fn(fn_ref) => self.call(frame, expr, &fn_ref, values),
            Value::Closure(closure) => self.call_closure(&closure, values),
            _ => Err(Exit::Error(EvalError::abort())),
        }
    }

    /// Calls `fn_ref` with evaluated arguments (the receiver first for methods).
    pub(crate) fn call(
        &mut self,
        frame: &Frame<'db>,
        expr: ExprId,
        fn_ref: &FnRef,
        args: Vec<Value>,
    ) -> R<Value> {
        let item = match fn_ref.def {
            FnDefId::StructCtor(item) => {
                return Ok(Value::Adt {
                    item,
                    variant: 0,
                    fields: args,
                });
            }
            FnDefId::VariantCtor(item, variant) => {
                return Ok(Value::Adt {
                    item,
                    variant,
                    fields: args,
                });
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
                && self.ctx.sigs.fn_sig(item).map(|sig| sig.kind) == Some(FnKind::Pushdown)
            {
                return Ok(Value::unit());
            }
            return self
                .builtin(frame, expr, builtin, fn_ref, args)
                .map_err(|kind| Self::fail(frame, expr, kind));
        }
        let Some(sig) = self.ctx.sigs.fn_sig(item) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        // Calling a `fn` or `pushdown fn` from const code is a call-rule error of stage 3;
        // in story mode instant procedures run, scenes never (the analysis runs them).
        let allowed = match sig.kind {
            FnKind::Const => true,
            FnKind::Pushdown => self.story.is_some() && !sig.scene,
            FnKind::Plain => false,
        };
        if !allowed {
            return Err(Exit::Error(EvalError::abort()));
        }
        let Some(body) = self.ctx.body(item) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let env = self.ctx.env_for(item, fn_ref.args.clone());
        let mut callee = Frame::new(body, env);
        let mut args = args.into_iter();
        if body.body.has_self {
            let Some(self_local) = body
                .res
                .locals
                .iter()
                .find(|(_, local)| local.pat.is_none())
                .map(|(id, _)| id)
            else {
                return Err(Exit::Error(EvalError::abort()));
            };
            let Some(receiver) = args.next() else {
                return Err(Exit::Error(EvalError::abort()));
            };
            let cell = self.alloc(receiver);
            callee.locals.insert(self_local, cell);
        }
        for (&pat, arg) in body.body.params.iter().zip(args) {
            self.bind(&mut callee, pat, arg)?;
        }
        self.enter(&mut callee, body.body.value)
    }

    /// Evaluates a callee's body, catching `return`.
    fn enter(&mut self, callee: &mut Frame<'db>, body: ExprId) -> R<Value> {
        let depth = self.ctx.depth.get();
        if depth >= MAX_CALL_DEPTH {
            return Err(Self::fail(callee, body, recursion_too_deep()));
        }
        self.ctx.depth.set(depth.saturating_add(1));
        let result = self.eval(callee, body);
        self.ctx.depth.set(depth);
        match result {
            Ok(value) | Err(Exit::Return(value)) => Ok(value),
            Err(Exit::Break(..) | Exit::Continue(_)) => Err(Exit::Error(EvalError::abort())),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn call_closure(&mut self, closure: &ClosureValue, args: Vec<Value>) -> R<Value> {
        let Some(body) = self.ctx.body(closure.owner) else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let Expr::Closure {
            params,
            body: closure_body,
            ..
        } = body.body.store.expr(closure.expr)
        else {
            return Err(Exit::Error(EvalError::abort()));
        };
        let mut callee = Frame::new(body, closure.env.clone());
        for (local, value) in &closure.captures {
            let cell = self.alloc(value.clone());
            callee.locals.insert(*local, cell);
        }
        for ((pat, _), arg) in params.iter().zip(args) {
            self.bind(&mut callee, *pat, arg)?;
        }
        self.enter(&mut callee, *closure_body)
    }

    /// Calls a function value (a named function or a closure) from outside any body: the
    /// renders of steps and tabulated calls.
    pub(crate) fn call_value(
        &mut self,
        site: &Frame<'db>,
        expr: ExprId,
        callee: &Value,
        args: Vec<Value>,
    ) -> Result<Value, EvalError> {
        let result = match callee {
            Value::Fn(fn_ref) => self.call(site, expr, fn_ref, args),
            Value::Closure(closure) => self.call_closure(closure, args),
            _ => return Err(EvalError::abort()),
        };
        match result {
            Ok(value) | Err(Exit::Return(value)) => Ok(value),
            Err(Exit::Error(error)) => Err(error),
            Err(_) => Err(EvalError::abort()),
        }
    }

    // ----- Patterns -----------------------------------------------------------------

    fn head(&mut self, scrut: &Scrut) -> Result<Head, ErrorKind> {
        let value = match scrut {
            Scrut::Val(value) => value,
            Scrut::Place(place) => &*self.place_mut(place)?,
        };
        Ok(match value {
            Value::Adt { item, variant, .. } => Head::Adt {
                item: *item,
                variant: *variant,
            },
            Value::Tuple(_) => Head::Tuple,
            Value::Probe => return Err(ErrorKind::Inspected),
            Value::Uninit => return Err(ErrorKind::Abort),
            value => Head::Leaf(value.clone()),
        })
    }

    fn sub(&mut self, scrut: &Scrut, index: usize) -> Result<Scrut, ErrorKind> {
        match scrut {
            Scrut::Val(Value::Tuple(fields) | Value::Adt { fields, .. }) => fields
                .get(index)
                .cloned()
                .map(Scrut::Val)
                .ok_or(ErrorKind::Abort),
            Scrut::Val(_) => Err(ErrorKind::Abort),
            Scrut::Place(place) => Ok(Scrut::Place(place.project(Proj::Field(
                u32::try_from(index).map_err(|_| ErrorKind::Abort)?,
            )))),
        }
    }

    fn len(&mut self, scrut: &Scrut) -> Result<usize, ErrorKind> {
        let value = match scrut {
            Scrut::Val(value) => value,
            Scrut::Place(place) => &*self.place_mut(place)?,
        };
        match value {
            Value::Tuple(fields) | Value::Adt { fields, .. } => Ok(fields.len()),
            _ => Err(ErrorKind::Abort),
        }
    }

    /// Follows references at the scrutinee, switching to place mode.
    fn auto_deref(&mut self, scrut: Scrut) -> Result<Scrut, ErrorKind> {
        let mut scrut = scrut;
        loop {
            let next = match &scrut {
                Scrut::Val(Value::Ref(place)) => (**place).clone(),
                Scrut::Val(_) => return Ok(scrut),
                Scrut::Place(place) => match self.place_mut(place)? {
                    Value::Ref(next) => (**next).clone(),
                    _ => return Ok(scrut),
                },
            };
            scrut = Scrut::Place(next);
        }
    }

    /// Whether `value` matches the refutable `pat`, binding its locals when it does.
    pub(crate) fn test_pat(&mut self, frame: &mut Frame<'db>, pat: PatId, value: Value) -> R<bool> {
        self.match_pat(frame, pat, Scrut::Val(value))
    }

    /// Matches `pat`, binding its locals; errors without a location get the pattern's.
    fn match_pat(&mut self, frame: &mut Frame<'db>, pat: PatId, scrut: Scrut) -> R<bool> {
        self.match_pat_inner(frame, pat, scrut)
            .map_err(|exit| match exit {
                Exit::Error(EvalError { kind, span: None }) if kind != ErrorKind::Abort => {
                    Exit::Error(EvalError {
                        kind,
                        span: Some(Span::new(
                            frame.b.item.file,
                            frame.b.body.store.pat_range(pat),
                        )),
                    })
                }
                exit => exit,
            })
    }

    fn match_pat_inner(&mut self, frame: &mut Frame<'db>, pat: PatId, scrut: Scrut) -> R<bool> {
        let lift = |kind: ErrorKind| Exit::Error(EvalError { kind, span: None });
        let store = &frame.b.body.store;
        let ctor = frame.b.infer.pat_ctors.get(&pat).cloned();
        match store.pat(pat) {
            Pat::Missing => Err(Exit::Error(EvalError::abort())),
            Pat::Wild | Pat::Rest => Ok(true),
            Pat::Bind { .. } if ctor.is_none() => {
                let Some(local) = frame.b.res.bindings.get(&pat).copied() else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let value = match scrut {
                    Scrut::Val(value) => value,
                    Scrut::Place(place) => Value::Ref(Box::new(place)),
                };
                let cell = self.alloc(value);
                frame.locals.insert(local, cell);
                Ok(true)
            }
            Pat::Or(pats) => {
                let pats = pats.clone();
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                for sub in pats {
                    let copy = match &scrut {
                        Scrut::Val(value) => Scrut::Val(value.clone()),
                        Scrut::Place(place) => Scrut::Place(place.clone()),
                    };
                    if self.match_pat_inner(frame, sub, copy)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Pat::Lit { lit, negated } => {
                let (lit, negated) = (lit.clone(), *negated);
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                let Head::Leaf(value) = self.head(&scrut).map_err(lift)? else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let expected =
                    literal_value(&lit, negated).ok_or_else(|| lift(ErrorKind::Abort))?;
                self.value_eq(&value, &expected).map_err(lift)
            }
            Pat::Range {
                start,
                end,
                inclusive,
            } => {
                let (start, end, inclusive) = (start.clone(), end.clone(), *inclusive);
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                let Head::Leaf(value) = self.head(&scrut).map_err(lift)? else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                if let Some(start) = start {
                    let bound = self.pat_bound(frame, pat, 0, &start)?;
                    if self.value_cmp(value.clone(), bound).map_err(lift)? == Ordering::Less {
                        return Ok(false);
                    }
                }
                if let Some(end) = end {
                    let bound = self.pat_bound(frame, pat, 1, &end)?;
                    let ordering = self.value_cmp(value, bound).map_err(lift)?;
                    if ordering == Ordering::Greater || (!inclusive && ordering == Ordering::Equal)
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Pat::Tuple(pats) => {
                let pats = pats.clone();
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                if !matches!(self.head(&scrut).map_err(lift)?, Head::Tuple) {
                    return Err(Exit::Error(EvalError::abort()));
                }
                self.match_fields(frame, &pats, &scrut)
            }
            Pat::TupleStruct { pats, .. } => {
                let pats = pats.clone();
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                if !self.ctor_matches(ctor.as_ref(), &scrut).map_err(lift)? {
                    return Ok(false);
                }
                self.match_fields(frame, &pats, &scrut)
            }
            Pat::Record { fields, .. } => {
                let fields = fields.clone();
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                if !self.ctor_matches(ctor.as_ref(), &scrut).map_err(lift)? {
                    return Ok(false);
                }
                let Some(ctor) = ctor else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let shape = adt_variants(self.ctx.db, ctor.adt)
                    .into_iter()
                    .nth(ctor.variant.unwrap_or(0) as usize)
                    .map(|(_, shape)| shape);
                let Some(Shape::Record(names)) = shape else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                for (name, sub) in fields {
                    let Some(index) = names.iter().position(|field| *field == name) else {
                        return Err(Exit::Error(EvalError::abort()));
                    };
                    let sub_scrut = self.sub(&scrut, index).map_err(lift)?;
                    if !self.match_pat_inner(frame, sub, sub_scrut)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Pat::Path(_) | Pat::Bind { .. } => {
                let scrut = self.auto_deref(scrut).map_err(lift)?;
                if ctor.is_some() {
                    return self.ctor_matches(ctor.as_ref(), &scrut).map_err(lift);
                }
                // A path to a constant.
                let Some(Resolution::Def(Def::Item(item))) = frame.b.res.store.pats.get(&pat)
                else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                let expected = self
                    .ctx
                    .const_value(*item, Vec::new())
                    .map_err(|_| Exit::Error(EvalError::abort()))?;
                let expected = self.thaw(&expected);
                let value = match scrut {
                    Scrut::Val(value) => value,
                    Scrut::Place(place) => self.read_place(&place).map_err(lift)?,
                };
                self.value_eq(&value, &expected).map_err(lift)
            }
        }
    }

    fn ctor_matches(
        &mut self,
        ctor: Option<&crate::ty::CtorRef>,
        scrut: &Scrut,
    ) -> Result<bool, ErrorKind> {
        let Some(ctor) = ctor else {
            return Err(ErrorKind::Abort);
        };
        match self.head(scrut)? {
            Head::Adt { item, variant } => {
                Ok(item == ctor.adt && variant == ctor.variant.unwrap_or(0))
            }
            _ => Err(ErrorKind::Abort),
        }
    }

    /// Matches `pats` (with at most one `..`) against the fields of `scrut`.
    fn match_fields(&mut self, frame: &mut Frame<'db>, pats: &[PatId], scrut: &Scrut) -> R<bool> {
        let lift = |kind: ErrorKind| Exit::Error(EvalError { kind, span: None });
        let len = self.len(scrut).map_err(lift)?;
        let store = &frame.b.body.store;
        let rest = pats
            .iter()
            .position(|pat| matches!(store.pat(*pat), Pat::Rest));
        let indexed: Vec<(usize, PatId)> = match rest {
            None => pats.iter().copied().enumerate().collect(),
            Some(rest) => {
                let after = pats.len().saturating_sub(rest).saturating_sub(1);
                let mut out: Vec<(usize, PatId)> =
                    pats.iter().copied().take(rest).enumerate().collect();
                let skip = len.saturating_sub(after);
                out.extend(
                    pats.iter()
                        .copied()
                        .skip(rest.saturating_add(1))
                        .enumerate()
                        .map(|(index, pat)| (skip.saturating_add(index), pat)),
                );
                out
            }
        };
        for (index, pat) in indexed {
            let sub = self.sub(scrut, index).map_err(lift)?;
            if !self.match_pat_inner(frame, pat, sub)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn pat_bound(
        &mut self,
        frame: &Frame<'db>,
        pat: PatId,
        which: u8,
        bound: &PatBound,
    ) -> R<Value> {
        match bound {
            PatBound::Lit { lit, negated } => {
                literal_value(lit, *negated).ok_or_else(|| Exit::Error(EvalError::abort()))
            }
            PatBound::Path(_) => {
                let Some(Resolution::Def(Def::Item(item))) =
                    frame.b.res.store.range_bounds.get(&(pat, which))
                else {
                    return Err(Exit::Error(EvalError::abort()));
                };
                match self.ctx.const_value(*item, Vec::new()) {
                    Ok(value) => Ok(self.thaw(&value)),
                    Err(_) => Err(Exit::Error(EvalError::abort())),
                }
            }
        }
    }

    // ----- Conversions --------------------------------------------------------------

    /// `value` as plain data. References and pending changes cannot be constants;
    /// the probe screen reports that the render inspected it.
    pub(crate) fn freeze(&mut self, value: &Value) -> Result<ConstValue, ErrorKind> {
        let list = |machine: &mut Self, items: &[Value]| {
            items
                .iter()
                .map(|item| machine.freeze(item))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(match value {
            Value::Bool(value) => ConstValue::Bool(*value),
            Value::Int(value) => ConstValue::Int(*value),
            Value::Decimal(value) => ConstValue::Decimal(*value),
            Value::Rat(value) => ConstValue::Rat(*value),
            Value::Text(text) => ConstValue::Text(text.clone()),
            Value::String(text) => ConstValue::String(text.clone()),
            Value::Tuple(items) => ConstValue::Tuple(list(self, items)?),
            Value::Array(items) => ConstValue::Array(list(self, items)?),
            Value::Vec(items) => ConstValue::Vec(list(self, items)?),
            Value::Adt {
                item,
                variant,
                fields,
            } => ConstValue::Adt {
                item: *item,
                variant: *variant,
                fields: list(self, fields)?,
            },
            Value::Asset { kind, path } => ConstValue::Asset {
                kind: *kind,
                path: path.clone(),
            },
            Value::Fn(fn_ref) => ConstValue::Fn(fn_ref.clone()),
            Value::Closure(closure) if closure.captures.is_empty() => ConstValue::Closure {
                owner: closure.owner,
                expr: closure.expr,
            },
            Value::Handle { kind, item } => ConstValue::Handle {
                kind: *kind,
                item: *item,
            },
            Value::NewHandle { .. } => {
                return Err(fault(
                    DiagnosticCode::EvalHandleOutsideStatic,
                    "a `Setting` or `Meta` handle can only be the initializer of a `static`",
                ));
            }
            Value::Screen(screen) => ConstValue::Screen(screen.clone()),
            Value::Probe => return Err(ErrorKind::Inspected),
            Value::Closure(_) | Value::Ref(_) | Value::Pending(_) | Value::Uninit => {
                return Err(ErrorKind::Abort);
            }
        })
    }

    pub(crate) fn thaw(&self, value: &ConstValue) -> Value {
        let list = |items: &[ConstValue]| items.iter().map(|item| self.thaw(item)).collect();
        match value {
            ConstValue::Bool(value) => Value::Bool(*value),
            ConstValue::Int(value) => Value::Int(*value),
            ConstValue::Decimal(value) => Value::Decimal(*value),
            ConstValue::Rat(value) => Value::Rat(*value),
            ConstValue::Text(text) => Value::Text(text.clone()),
            ConstValue::String(text) => Value::String(text.clone()),
            ConstValue::Tuple(items) => Value::Tuple(list(items)),
            ConstValue::Array(items) => Value::Array(list(items)),
            ConstValue::Vec(items) => Value::Vec(list(items)),
            ConstValue::Adt {
                item,
                variant,
                fields,
            } => Value::Adt {
                item: *item,
                variant: *variant,
                fields: list(fields),
            },
            ConstValue::Asset { kind, path } => Value::Asset {
                kind: *kind,
                path: path.clone(),
            },
            ConstValue::Fn(fn_ref) => Value::Fn(fn_ref.clone()),
            ConstValue::Closure { owner, expr } => Value::Closure(Box::new(ClosureValue {
                owner: *owner,
                expr: *expr,
                env: GenericEnv::default(),
                captures: Vec::new(),
            })),
            ConstValue::Handle { kind, item } => Value::Handle {
                kind: *kind,
                item: *item,
            },
            ConstValue::Screen(screen) => Value::Screen(screen.clone()),
        }
    }
}

fn literal_value(literal: &Literal, negated: bool) -> Option<Value> {
    Some(match literal {
        Literal::Int { value, .. } => {
            let value = i128::try_from(*value).ok()?;
            Value::Int(if negated { value.checked_neg()? } else { value })
        }
        Literal::Decimal(text) => {
            let value = Decimal::parse(text)?;
            Value::Decimal(if negated { value.checked_neg()? } else { value })
        }
        Literal::String(text) => Value::Text(Text::plain(text.clone())),
        Literal::Bool(value) => Value::Bool(*value),
    })
}

pub(crate) fn budget_exceeded() -> ErrorKind {
    fault(
        DiagnosticCode::EvalBudgetExceeded,
        format!("constant evaluation exceeded its budget of {EVAL_STEP_BUDGET} steps"),
    )
}

fn recursion_too_deep() -> ErrorKind {
    fault(
        DiagnosticCode::EvalBudgetExceeded,
        format!(
            "recursion too deep: more than {MAX_CALL_DEPTH} nested calls during constant evaluation"
        ),
    )
}
