//! Const evaluation (compilation stage 4): constants and statics, the constant parts of
//! story bodies, tabulated `const fn` calls from the story, and the renders steps use.
//!
//! See `docs/syntax/src/compilation.md` (stage 4), `docs/syntax/src/kinds.md`,
//! `docs/syntax/src/presentation.md` and `docs/std/src/checks.md#const-evaluation-errors`.

mod anim;
mod builtins;
pub(crate) mod const_exprs;
pub(crate) mod domain;
pub(crate) mod interp;
mod num;
pub(crate) mod screen;
pub(crate) mod std_items;
mod value;

#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, Def, Resolution, def_maps};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hir::{ArrayExpr, Expr, ExprId, ExprStore, Literal, file_hir};
use crate::infer::ValueRef;
use crate::input::{Program, SourceFile};
use crate::item_tree::{FnKind, ItemId, ItemKind, ItemParent, item_tree};
use crate::resolve::file_resolution;
use crate::signatures::{FnSig, ItemSig, program_signatures};
use crate::ty::{FnDefId, FnRef, Ty};
use kagari_syntax::TextRange;

pub use const_exprs::{ConstExprs, ConstLocal, FileConstExprs, const_exprs, file_const_exprs};
pub use interp::MAX_CALL_DEPTH;
pub use num::{Decimal, Rat};
pub use screen::{
    Camera, Channel, ChannelId, Cursor, CursorAnchor, Entity, EntityProp, Hud, LayerTarget,
    LogicalScreen, PendingChange, PropTarget, Scheduled, ScheduledKind, Screen, ScreenObj,
    StepState, Target,
};
pub use value::{AssetKind, ConstValue, HandleKind, Text, display_value};

use domain::DomainCtx;
use interp::{BodyRef, ClosureValue, Ctx, ErrorKind, EvalError, Frame, GenericEnv, Machine, Value};
use std_items::{Builtin, StdTable};

/// The number of evaluation steps one top-level evaluation may take (*tentative*: a
/// compiler setting). Running out is `eval/budget-exceeded`.
pub const EVAL_STEP_BUDGET: u64 = 1_000_000;

/// The largest number of argument combinations a tabulated call may have (*tentative*: a
/// compiler setting). More is `eval/domain-too-large`.
pub const TABLE_DOMAIN_CAP: u64 = 65_536;

/// A const-evaluation error: its diagnostic code and message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fault {
    pub code: DiagnosticCode,
    pub message: String,
}

impl Fault {
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Fault {
        Fault {
            code,
            message: message.into(),
        }
    }
}

/// The results of const evaluation for a whole program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstEval {
    /// Every non-generic `const` item of the program and std.
    pub consts: FxHashMap<ItemId, ConstValue>,
    /// Initial values of story variables and the default values of handles (`static`).
    pub statics: FxHashMap<ItemId, ConstValue>,
    /// Maximal constant subexpressions of `pushdown fn` bodies that are not plain literals
    /// or paths (e.g. `fade(ms(500))`, `t![…]` texts, step records' constant parts).
    /// Expressions of type `()` are not recorded.
    pub folded: FxHashMap<(ItemId, ExprId), ConstValue>,
    /// Tabulated `const fn` calls (incl. `to_text`) in `pushdown fn` bodies.
    pub tables: FxHashMap<(ItemId, ExprId), Table>,
    /// Every render used by a step.
    pub renders: FxHashMap<RenderId, RenderInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A render: a named function or an inline closure (the item whose body contains it and
/// the closure expression).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderId {
    Fn(ItemId),
    Closure(ItemId, ExprId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderInfo {
    /// The render's output does not depend on its input screen.
    pub keyframe: bool,
}

/// A `const fn` call from the story evaluated on every combination of its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub func: FnRef,
    /// One row per combination: the value of every argument in order (the receiver first
    /// for a method call; the referenced value for a reference argument), and the output.
    /// When `writes` is non-empty the output is the tuple of the return value followed by
    /// the final value of each `&mut` argument listed in `writes`.
    pub rows: Vec<(Vec<ConstValue>, ConstValue)>,
    /// The indices of the `&mut` arguments whose final values follow the return value.
    pub writes: Vec<usize>,
}

#[salsa::tracked(returns(ref))]
pub fn const_eval(db: &dyn Db, program: Program) -> ConstEval {
    let Some(std) = StdTable::new(db, program) else {
        return ConstEval::default();
    };
    let sigs = program_signatures(db, program);
    let ctx = Ctx::new(db, program, &std, sigs);
    let mut driver = Driver {
        db,
        program,
        ctx: &ctx,
        out: ConstEval::default(),
        texts: Vec::new(),
        text_set: FxHashSet::default(),
    };
    driver.run();
    let mut out = driver.out;
    out.diagnostics.sort_by(|a, b| {
        (&a.file, a.range.start(), a.code.as_str(), &a.message).cmp(&(
            &b.file,
            b.range.start(),
            b.code.as_str(),
            &b.message,
        ))
    });
    out.diagnostics.dedup();
    out
}

/// A candidate render of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RenderCand {
    Fn(FnRef),
    Closure(ItemId, ExprId),
}

struct Driver<'c, 'db> {
    db: &'db dyn Db,
    program: Program,
    ctx: &'c Ctx<'db>,
    out: ConstEval,
    texts: Vec<Text>,
    text_set: FxHashSet<Text>,
}

/// Whether `index` or its parent impl or trait has generic parameters.
fn is_generic(db: &dyn Db, file: SourceFile, index: u32) -> bool {
    let hir = file_hir(db, file);
    let has_generics = |index: u32| {
        hir.signatures
            .get(index as usize)
            .and_then(Option::as_ref)
            .is_some_and(|signature| !signature.generics.is_empty())
    };
    if has_generics(index) {
        return true;
    }
    match item_tree(db, file).item(index).map(|item| item.parent) {
        Some(ItemParent::Item(parent)) => has_generics(parent),
        _ => false,
    }
}

impl<'db> Driver<'_, 'db> {
    fn run(&mut self) {
        let maps = def_maps(self.db, self.program);
        let mut story = Vec::new();
        for &(file, krate) in &maps.files {
            let game = krate == CrateId::Game;
            let tree = item_tree(self.db, file);
            for (index, item) in tree.items.iter().enumerate() {
                let Ok(index) = u32::try_from(index) else {
                    continue;
                };
                let id = ItemId { file, index };
                match item.kind {
                    ItemKind::Const { has_value: true } if !is_generic(self.db, file, index) => {
                        match self.ctx.const_value(id, Vec::new()) {
                            Ok(value) => {
                                self.out.consts.insert(id, value);
                            }
                            Err(error) => {
                                self.report(game, file, item.name_range, &error, None);
                            }
                        }
                    }
                    ItemKind::Static { .. } => match self.ctx.static_value(id) {
                        Ok(value) => {
                            self.out.statics.insert(id, value.value);
                        }
                        Err(error) => self.report(game, file, item.name_range, &error, None),
                    },
                    ItemKind::Fn {
                        kind: FnKind::Pushdown,
                        has_body: true,
                        ..
                    } => story.push((id, game)),
                    _ => {}
                }
                if game {
                    self.collect_literal_texts(id);
                }
            }
        }
        let const_texts: Vec<ConstValue> = self.out.consts.values().cloned().collect();
        for value in &const_texts {
            self.collect_texts(value);
        }
        for &(item, game) in &story {
            self.fold(item, game);
        }
        let folded: Vec<ConstValue> = self.out.folded.values().cloned().collect();
        for value in &folded {
            self.collect_texts(value);
        }
        let assets = self.assets();
        let functions = self.functions();
        for &(item, game) in &story {
            self.tabulate(item, game, &assets, &functions);
        }
        for &(item, game) in &story {
            self.renders(item, game, &functions);
        }
    }

    /// Reports `error` at `range` of `file` (the code that triggered the evaluation) with a
    /// note at the failing site. Only game code reports; aborts and probes never do.
    fn report(
        &mut self,
        game: bool,
        file: SourceFile,
        range: TextRange,
        error: &EvalError,
        suffix: Option<&str>,
    ) {
        let ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        if !game {
            return;
        }
        let mut fault = fault.clone();
        if fault.code == DiagnosticCode::EvalOverflow
            && error
                .span
                .is_some_and(|span| span.file == self.ctx.std.time_file)
        {
            fault = screen::duration_overflow();
        }
        let message = match suffix {
            Some(suffix) => format!("{}{suffix}", fault.message),
            None => fault.message,
        };
        let mut diagnostic =
            Diagnostic::new(file.path(self.db).clone(), range, fault.code, message);
        if let Some(span) = error.span
            && (span.file != file || span.range != range)
        {
            diagnostic = diagnostic.with_related(
                span.file.path(self.db).clone(),
                span.range,
                "evaluation failed here",
            );
        }
        self.out.diagnostics.push(diagnostic);
    }

    fn add_text(&mut self, text: &Text) {
        if self.text_set.insert(text.clone()) {
            self.texts.push(text.clone());
        }
    }

    /// Every `Text` inside `value`.
    fn collect_texts(&mut self, value: &ConstValue) {
        let mut found = Vec::new();
        value.walk(&mut |value| {
            if let ConstValue::Text(text) = value {
                found.push(text.clone());
            }
        });
        for text in &found {
            self.add_text(text);
        }
    }

    /// The string literals of `item`'s body, except asset paths.
    fn collect_literal_texts(&mut self, item: ItemId) {
        let Some(Some(body)) = file_hir(self.db, item.file).bodies.get(item.index as usize) else {
            return;
        };
        let resolution = file_resolution(self.db, self.program, item.file)
            .items
            .get(item.index as usize)
            .and_then(Option::as_ref)
            .and_then(|resolution| resolution.body.as_ref());
        let is_asset_fn = |callee: ExprId| {
            matches!(
                resolution.and_then(|res| res.store.exprs.get(&callee)),
                Some(Resolution::Def(Def::Item(item)))
                    if self.ctx.std.builtin(*item) == Some(Builtin::Asset)
            )
        };
        let mut asset_paths = FxHashSet::default();
        for (_, expr) in body.store.exprs.iter() {
            if let Expr::Call { callee, args } = expr
                && is_asset_fn(*callee)
            {
                asset_paths.extend(args.iter().copied());
            }
        }
        let mut found = Vec::new();
        for (id, expr) in body.store.exprs.iter() {
            if let Expr::Literal(Literal::String(text)) = expr
                && !asset_paths.contains(&id)
            {
                found.push(Text::plain(text.clone()));
            }
        }
        for text in &found {
            self.add_text(text);
        }
    }

    fn story_frame(&self, item: ItemId) -> Option<(BodyRef<'db>, &'db ConstExprs)> {
        let body = self.ctx.body(item)?;
        let consts = const_exprs(self.db, self.program, item)?;
        Some((body, consts))
    }

    fn frame(&self, body: BodyRef<'db>, consts: &'db ConstExprs) -> Frame<'db> {
        let mut frame = Frame::new(body, self.ctx.env_for(body.item, Vec::new()));
        frame.lazy = Some(consts);
        frame
    }

    /// Evaluates a constant expression of a story body.
    fn eval_constant(
        &self,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        expr: ExprId,
    ) -> Result<ConstValue, EvalError> {
        let mut machine = Machine::new(self.ctx);
        let mut frame = self.frame(body, consts);
        let value = machine.run(&mut frame, expr)?;
        machine
            .freeze(&value)
            .map_err(|kind| EvalError { kind, span: None })
    }

    /// Folds the maximal constant subexpressions of a story body.
    fn fold(&mut self, item: ItemId, game: bool) {
        if let Some((body, consts)) = self.story_frame(item) {
            self.fold_body(game, body, consts);
        }
    }

    fn fold_body(&mut self, game: bool, body: BodyRef<'db>, consts: &'db ConstExprs) {
        let item = body.item;
        let store = &body.body.store;
        let mut stack = vec![body.body.value];
        while let Some(expr) = stack.pop() {
            if !consts.contains(expr) {
                stack.extend(const_exprs::children(store.expr(expr)));
                continue;
            }
            let trivial = matches!(store.expr(expr), Expr::Literal(_) | Expr::Path(_))
                || matches!(
                    body.infer.expr_types.get(expr),
                    Some(ty) if ty.is_unit() || *ty == Ty::Never
                );
            if trivial {
                continue;
            }
            match self.eval_constant(body, consts, expr) {
                Ok(value) => {
                    self.out.folded.insert((item, expr), value);
                }
                Err(error) => {
                    self.report(game, item.file, store.expr_range(expr), &error, None);
                }
            }
        }
    }

    /// The asset constants of the program by kind.
    fn assets(&self) -> FxHashMap<AssetKind, Vec<ConstValue>> {
        let maps = def_maps(self.db, self.program);
        let mut out: FxHashMap<AssetKind, Vec<ConstValue>> = FxHashMap::default();
        let mut seen = FxHashSet::default();
        for &(file, _) in &maps.files {
            for index in 0..item_tree(self.db, file).items.len() {
                let Ok(index) = u32::try_from(index) else {
                    continue;
                };
                if let Some(value @ ConstValue::Asset { kind, .. }) =
                    self.out.consts.get(&ItemId { file, index })
                    && seen.insert(value.clone())
                {
                    out.entry(*kind).or_default().push(value.clone());
                }
            }
        }
        out
    }

    /// The named non-generic functions of the program, in load order.
    fn functions(&self) -> Vec<(ItemId, &'db FnSig)> {
        let maps = def_maps(self.db, self.program);
        let mut out = Vec::new();
        for &(file, _) in &maps.files {
            for index in 0..item_tree(self.db, file).items.len() {
                let Ok(index) = u32::try_from(index) else {
                    continue;
                };
                let item = ItemId { file, index };
                if let Some(ItemSig::Fn(sig)) = self.ctx.sigs.get(item)
                    && sig.generics.params.is_empty()
                    && sig.generics.parent_count == 0
                {
                    out.push((item, sig));
                }
            }
        }
        out
    }

    fn domain<'a>(
        &'a self,
        assets: &'a FxHashMap<AssetKind, Vec<ConstValue>>,
        functions: &'a [(ItemId, &'a FnSig)],
    ) -> DomainCtx<'a> {
        DomainCtx {
            sigs: self.ctx.sigs,
            std: self.ctx.std,
            texts: &self.texts,
            assets,
            functions,
        }
    }

    /// Tabulates every `const fn` call of a story body with a non-constant argument,
    /// inner calls first so that outer calls range over their results.
    fn tabulate(
        &mut self,
        item: ItemId,
        game: bool,
        assets: &FxHashMap<AssetKind, Vec<ConstValue>>,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        if let Some((body, consts)) = self.story_frame(item) {
            self.tabulate_body(game, body, consts, assets, functions);
        }
    }

    fn tabulate_body(
        &mut self,
        game: bool,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        assets: &FxHashMap<AssetKind, Vec<ConstValue>>,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        let item = body.item;
        let store = &body.body.store;
        for expr in post_order(store, body.body.value) {
            if consts.contains(expr) {
                continue;
            }
            let Some((fn_ref, args)) = self.const_call(body, expr, store.expr(expr)) else {
                continue;
            };
            if args.iter().all(|arg| consts.contains(arg.expr)) {
                continue;
            }
            let result = {
                let domain = self.domain(assets, functions);
                self.table(body, consts, &domain, expr, &fn_ref, &args)
            };
            match result {
                Ok(Some(table)) => {
                    let outputs: Vec<ConstValue> = table
                        .rows
                        .iter()
                        .map(|(_, output)| output.clone())
                        .collect();
                    self.out.tables.insert((item, expr), table);
                    // Texts computed by the story are texts of the program too.
                    for output in &outputs {
                        self.collect_texts(output);
                    }
                }
                Ok(None) => {}
                Err((error, suffix)) => {
                    let range = store.expr_range(expr);
                    self.report(game, item.file, range, &error, suffix.as_deref());
                }
            }
        }
    }

    /// The values a non-constant argument `expr` of type `ty` can take: the outputs of a
    /// tabulated call, the product of the parts of a tuple, array or constructor call
    /// built from such, otherwise every value of `ty`. Constant parts are evaluated.
    /// `direct` is false when the argument is passed by reference (`expr` is then not
    /// the value itself). `Ok(None)` when `ty` is not finite.
    fn arg_values(
        &self,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        domain: &DomainCtx<'_>,
        expr: ExprId,
        ty: &Ty,
        direct: bool,
    ) -> Result<Option<Vec<ConstValue>>, TooLarge> {
        if consts.contains(expr) {
            return Ok(self
                .eval_constant(body, consts, expr)
                .ok()
                .map(|value| vec![value]));
        }
        if direct {
            if let Some(table) = self.out.tables.get(&(body.item, expr)) {
                let mut seen = FxHashSet::default();
                let mut values = Vec::new();
                for (_, output) in &table.rows {
                    let value = if table.writes.is_empty() {
                        output
                    } else {
                        match output.field(0) {
                            Some(value) => value,
                            None => return Ok(None),
                        }
                    };
                    if seen.insert(value.clone()) {
                        values.push(value.clone());
                    }
                }
                return Ok(Some(values));
            }
            let parts = |items: &[ExprId]| -> Result<Option<Vec<Vec<ConstValue>>>, TooLarge> {
                let mut size: u64 = 1;
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let Some(item_ty) = body.infer.expr_types.get(*item) else {
                        return Ok(None);
                    };
                    let direct = !matches!(item_ty, Ty::Ref { .. });
                    let item_ty = match item_ty {
                        Ty::Ref { inner, .. } => inner,
                        ty => ty,
                    };
                    let Some(values) =
                        self.arg_values(body, consts, domain, *item, item_ty, direct)?
                    else {
                        return Ok(None);
                    };
                    size = size.saturating_mul(u64::try_from(values.len()).unwrap_or(u64::MAX));
                    if size > TABLE_DOMAIN_CAP {
                        return Err(TooLarge);
                    }
                    out.push(values);
                }
                Ok(Some(out))
            };
            match body.body.store.expr(expr) {
                Expr::Tuple(items) => {
                    return Ok(parts(items)?.map(|parts| {
                        domain::product(&parts)
                            .into_iter()
                            .map(ConstValue::Tuple)
                            .collect()
                    }));
                }
                Expr::Array(ArrayExpr::List(items)) => {
                    return Ok(parts(items)?.map(|parts| {
                        domain::product(&parts)
                            .into_iter()
                            .map(ConstValue::Array)
                            .collect()
                    }));
                }
                Expr::Call { callee, args } => {
                    let ctor = match body.infer.paths.get(callee) {
                        Some(ValueRef::Fn(FnRef {
                            def: FnDefId::StructCtor(item),
                            ..
                        })) => Some((*item, 0)),
                        Some(ValueRef::Fn(FnRef {
                            def: FnDefId::VariantCtor(item, variant),
                            ..
                        })) => Some((*item, *variant)),
                        _ => None,
                    };
                    if let Some((item, variant)) = ctor {
                        return Ok(parts(args)?.map(|parts| {
                            domain::product(&parts)
                                .into_iter()
                                .map(|fields| ConstValue::Adt {
                                    item,
                                    variant,
                                    fields,
                                })
                                .collect()
                        }));
                    }
                }
                _ => {}
            }
        }
        let Some(count) = domain.size(ty) else {
            return Ok(None);
        };
        if count > TABLE_DOMAIN_CAP {
            return Err(TooLarge);
        }
        Ok(domain.values(ty, TABLE_DOMAIN_CAP))
    }

    /// A call of a `const fn` (not a constructor) with its arguments, the receiver first.
    fn const_call(
        &self,
        body: BodyRef<'db>,
        expr: ExprId,
        data: &Expr,
    ) -> Option<(FnRef, Vec<CallArg>)> {
        let is_const = |fn_ref: &FnRef| match fn_ref.def {
            FnDefId::Item(item) => matches!(
                item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Fn {
                    kind: FnKind::Const,
                    ..
                })
            ),
            _ => false,
        };
        let types = &body.infer.expr_types;
        let plain_arg = |arg: ExprId| {
            let ty = types.get(arg)?.clone();
            Some(match ty {
                Ty::Ref { mutable, inner } => CallArg {
                    expr: arg,
                    ty: *inner,
                    by_ref: Some(mutable),
                    direct: false,
                },
                ty => CallArg {
                    expr: arg,
                    ty,
                    by_ref: None,
                    direct: true,
                },
            })
        };
        match data {
            Expr::Call { callee, args } => {
                let Some(ValueRef::Fn(fn_ref)) = body.infer.paths.get(callee) else {
                    return None;
                };
                if !is_const(fn_ref) {
                    return None;
                }
                let args = args
                    .iter()
                    .map(|arg| plain_arg(*arg))
                    .collect::<Option<Vec<_>>>()?;
                Some((fn_ref.clone(), args))
            }
            Expr::MethodCall { receiver, args, .. } => {
                let call = body.infer.method_calls.get(&expr)?;
                if !is_const(&call.func) {
                    return None;
                }
                let direct = call.receiver.derefs == 0;
                let mut ty = types.get(*receiver)?.clone();
                for _ in 0..call.receiver.derefs {
                    ty = match ty {
                        Ty::Ref { inner, .. } => *inner,
                        _ => return None,
                    };
                }
                let receiver_arg = match (call.receiver.autoref, ty) {
                    (Some(mutable), ty) => CallArg {
                        expr: *receiver,
                        ty,
                        by_ref: Some(mutable),
                        direct,
                    },
                    (None, Ty::Ref { mutable, inner }) => CallArg {
                        expr: *receiver,
                        ty: *inner,
                        by_ref: Some(mutable),
                        direct: false,
                    },
                    (None, ty) => CallArg {
                        expr: *receiver,
                        ty,
                        by_ref: None,
                        direct,
                    },
                };
                let mut out = vec![receiver_arg];
                for arg in args {
                    out.push(plain_arg(*arg)?);
                }
                Some((call.func.clone(), out))
            }
            _ => None,
        }
    }

    /// The table of one call; `Ok(None)` when it cannot be built without a new diagnostic.
    #[allow(clippy::type_complexity)]
    fn table(
        &self,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        domain: &DomainCtx<'_>,
        expr: ExprId,
        fn_ref: &FnRef,
        args: &[CallArg],
    ) -> Result<Option<Table>, (EvalError, Option<String>)> {
        let mut domains = Vec::with_capacity(args.len());
        let mut size: u64 = 1;
        for arg in args {
            let values = if consts.contains(arg.expr) {
                let inner = match (arg.by_ref, body.body.store.expr(arg.expr)) {
                    (Some(_), Expr::Ref { expr: inner, .. }) => *inner,
                    _ => arg.expr,
                };
                match self.eval_constant(body, consts, inner) {
                    Ok(value) => vec![value],
                    Err(_) => return Ok(None),
                }
            } else {
                match self.arg_values(body, consts, domain, arg.expr, &arg.ty, arg.direct) {
                    Ok(Some(values)) => values,
                    Ok(None) => return Ok(None),
                    Err(TooLarge) => return Err((domain_too_large(), None)),
                }
            };
            size = size.saturating_mul(u64::try_from(values.len()).unwrap_or(u64::MAX));
            if size > TABLE_DOMAIN_CAP {
                return Err((domain_too_large(), None));
            }
            domains.push(values);
        }
        let writes: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, arg)| arg.by_ref == Some(true))
            .map(|(index, _)| index)
            .collect();
        let callee = Value::Fn(fn_ref.clone());
        let mut rows = Vec::new();
        for inputs in domain::product(&domains) {
            let mut machine = Machine::new(self.ctx);
            let site = self.frame(body, consts);
            let mut values = Vec::with_capacity(inputs.len());
            let mut cells = Vec::with_capacity(inputs.len());
            for (input, arg) in inputs.iter().zip(args) {
                let value = machine.thaw(input);
                if arg.by_ref.is_some() {
                    let cell = machine.alloc(value);
                    cells.push(Some(cell));
                    values.push(Value::Ref(Box::new(interp::Place {
                        cell,
                        path: Vec::new(),
                        obj: None,
                    })));
                } else {
                    cells.push(None);
                    values.push(value);
                }
            }
            let shown = || {
                let shown: Vec<String> = inputs
                    .iter()
                    .map(|input| display_value(self.db, input))
                    .collect();
                Some(format!(" (with arguments `({})`)", shown.join(", ")))
            };
            let result = machine
                .call_value(&site, expr, &callee, values)
                .and_then(|value| {
                    machine
                        .freeze(&value)
                        .map_err(|kind| EvalError { kind, span: None })
                });
            let output = match result {
                Ok(output) => output,
                Err(error) => return Err((error, shown())),
            };
            let output = if writes.is_empty() {
                output
            } else {
                let mut parts = vec![output];
                for &index in &writes {
                    let Some(Some(cell)) = cells.get(index) else {
                        return Ok(None);
                    };
                    let Some(value) = machine.heap.get(*cell).cloned() else {
                        return Ok(None);
                    };
                    match machine.freeze(&value) {
                        Ok(value) => parts.push(value),
                        Err(kind) => return Err((EvalError { kind, span: None }, shown())),
                    }
                }
                ConstValue::Tuple(parts)
            };
            rows.push((inputs, output));
        }
        Ok(Some(Table {
            func: fn_ref.clone(),
            rows,
            writes,
        }))
    }

    /// Evaluates the render of every step of a story body.
    fn renders(&mut self, item: ItemId, game: bool, functions: &[(ItemId, &'db FnSig)]) {
        if let Some((body, consts)) = self.story_frame(item) {
            self.renders_body(game, body, consts, functions);
        }
    }

    fn renders_body(
        &mut self,
        game: bool,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        let item = body.item;
        let store = &body.body.store;
        for (expr, data) in store.exprs.iter() {
            let Expr::Call { callee, args } = data else {
                continue;
            };
            let is_push_step = matches!(
                body.infer.paths.get(callee),
                Some(ValueRef::Fn(FnRef { def: FnDefId::Item(func), .. }))
                    if *func == self.ctx.std.push_step
            );
            let [step] = args.as_slice() else {
                continue;
            };
            if !is_push_step {
                continue;
            }
            let mut cands = Vec::new();
            let trigger = match store.expr(*step) {
                Expr::Record { fields, .. } => {
                    match fields.iter().find(|(name, _)| name == "render") {
                        Some((_, render)) => {
                            self.render_cands(body, consts, *render, false, functions, &mut cands);
                            self.render_site(body, *render)
                        }
                        None => continue,
                    }
                }
                _ if consts.contains(*step) => {
                    let value = self
                        .out
                        .folded
                        .get(&(item, *step))
                        .cloned()
                        .or_else(|| self.eval_constant(body, consts, *step).ok());
                    if let Some(value) = value {
                        let std = self.ctx.std;
                        let render = match &value {
                            ConstValue::Adt { item: adt, .. } if *adt == std.say.item => {
                                std.say.field("render")
                            }
                            ConstValue::Adt { item: adt, .. } if *adt == std.choice.item => {
                                std.choice.field("render")
                            }
                            _ => None,
                        };
                        if let Some(Some(Some(render))) = render
                            .and_then(|index| value.field(index))
                            .map(|render| std.as_option(render))
                        {
                            push_value(render, &mut cands);
                        }
                    }
                    *step
                }
                _ => {
                    self.all_renders(functions, &mut cands);
                    *step
                }
            };
            for cand in cands {
                self.render(body, consts, expr, trigger, game, cand);
            }
        }
    }

    /// Where a step's render errors are reported: the `x` of `render: Some(x)` (what
    /// `render = x` desugars to), else the whole `render` value.
    fn render_site(&self, body: BodyRef<'db>, render: ExprId) -> ExprId {
        let std = self.ctx.std;
        match body.body.store.expr(render) {
            Expr::Call { callee, args } => match (body.infer.paths.get(callee), args.as_slice()) {
                (
                    Some(ValueRef::Fn(FnRef {
                        def: FnDefId::VariantCtor(adt, variant),
                        ..
                    })),
                    [inner],
                ) if *adt == std.option.item && std.option.variant("Some") == Some(*variant) => {
                    *inner
                }
                _ => render,
            },
            _ => render,
        }
    }

    /// The renders `expr` (an `Option<Render>`, or a `Render` when `inner`) can yield.
    fn render_cands(
        &self,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        expr: ExprId,
        inner: bool,
        functions: &[(ItemId, &'db FnSig)],
        out: &mut Vec<RenderCand>,
    ) {
        let std = self.ctx.std;
        if consts.contains(expr) {
            let value = self
                .out
                .folded
                .get(&(body.item, expr))
                .cloned()
                .or_else(|| self.eval_constant(body, consts, expr).ok());
            match (inner, value) {
                (true, Some(value)) => push_value(&value, out),
                (false, Some(value)) => {
                    if let Some(Some(render)) = std.as_option(&value) {
                        push_value(render, out);
                    }
                }
                (_, None) => {}
            }
            return;
        }
        match body.body.store.expr(expr) {
            Expr::Call { callee, args } if !inner => {
                let is_some = matches!(
                    body.infer.paths.get(callee),
                    Some(ValueRef::Fn(FnRef { def: FnDefId::VariantCtor(adt, variant), .. }))
                        if *adt == std.option.item && std.option.variant("Some") == Some(*variant)
                );
                match args.as_slice() {
                    [render] if is_some => {
                        self.render_cands(body, consts, *render, true, functions, out);
                    }
                    _ => self.all_renders(functions, out),
                }
            }
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.render_cands(body, consts, *then_branch, inner, functions, out);
                if let Some(else_branch) = else_branch {
                    self.render_cands(body, consts, *else_branch, inner, functions, out);
                }
            }
            Expr::Match { arms, .. } => {
                for arm in arms {
                    self.render_cands(body, consts, arm.body, inner, functions, out);
                }
            }
            Expr::Block {
                tail: Some(tail), ..
            } => self.render_cands(body, consts, *tail, inner, functions, out),
            Expr::Closure { .. } if inner => out.push(RenderCand::Closure(body.item, expr)),
            _ => self.all_renders(functions, out),
        }
    }

    /// Every named `const fn(Screen) -> Screen`: the values a runtime `Render` can hold.
    fn all_renders(&self, functions: &[(ItemId, &'db FnSig)], out: &mut Vec<RenderCand>) {
        let screen = Ty::Adt {
            item: self.ctx.std.screen,
            args: Vec::new(),
        };
        for (item, sig) in functions {
            if sig.kind == FnKind::Const && sig.params == [screen.clone()] && sig.ret == screen {
                out.push(RenderCand::Fn(FnRef {
                    def: FnDefId::Item(*item),
                    self_ty: None,
                    args: Vec::new(),
                }));
            }
        }
    }

    /// Evaluates a render once: on the probe screen to decide whether it is a keyframe, and
    /// on `Screen::empty()` for its errors when it inspects its input.
    fn render(
        &mut self,
        body: BodyRef<'db>,
        consts: &'db ConstExprs,
        call: ExprId,
        trigger: ExprId,
        game: bool,
        cand: RenderCand,
    ) {
        let (id, callee) = match cand {
            RenderCand::Fn(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => (RenderId::Fn(item), Value::Fn(fn_ref)),
                _ => return,
            },
            RenderCand::Closure(owner, expr) => (
                RenderId::Closure(owner, expr),
                Value::Closure(Box::new(ClosureValue {
                    owner,
                    expr,
                    env: GenericEnv::default(),
                    captures: Vec::new(),
                })),
            ),
        };
        if self.out.renders.contains_key(&id) {
            return;
        }
        let site = self.frame(body, consts);
        let mut machine = Machine::new(self.ctx);
        let probe = machine.call_value(&site, call, &callee, vec![Value::Probe]);
        let (keyframe, error) = match probe {
            Ok(output) => (machine.freeze(&output).is_ok(), None),
            Err(error) if error.kind == ErrorKind::Inspected => {
                let mut machine = Machine::new(self.ctx);
                let empty = Value::Screen(Box::new(Screen::empty(self.ctx.std)));
                (
                    false,
                    machine.call_value(&site, call, &callee, vec![empty]).err(),
                )
            }
            Err(error) => (false, Some(error)),
        };
        self.out.renders.insert(id, RenderInfo { keyframe });
        if let Some(error) = error {
            let range = body.body.store.expr_range(trigger);
            self.report(game, body.item.file, range, &error, None);
        }
    }
}

fn push_value(value: &ConstValue, out: &mut Vec<RenderCand>) {
    match value {
        ConstValue::Fn(fn_ref) => out.push(RenderCand::Fn(fn_ref.clone())),
        ConstValue::Closure { owner, expr } => out.push(RenderCand::Closure(*owner, *expr)),
        _ => {}
    }
}

/// An argument of a tabulated call: the type its values range over (references stripped),
/// whether it is passed by reference (`Some(mutable)`), and whether `expr`'s own value is
/// the argument (not a reference to it).
struct CallArg {
    expr: ExprId,
    ty: Ty,
    by_ref: Option<bool>,
    direct: bool,
}

/// An argument with more values than [`TABLE_DOMAIN_CAP`].
struct TooLarge;

fn domain_too_large() -> EvalError {
    EvalError {
        kind: ErrorKind::Fault(Fault::new(
            DiagnosticCode::EvalDomainTooLarge,
            format!(
                "this call has too many argument combinations to tabulate: more than the limit of {TABLE_DOMAIN_CAP}"
            ),
        )),
        span: None,
    }
}

/// The expressions of a body from `root`, every expression after its subexpressions.
fn post_order(store: &ExprStore, root: ExprId) -> Vec<ExprId> {
    let mut out = Vec::new();
    let mut stack = vec![(root, false)];
    while let Some((expr, expanded)) = stack.pop() {
        if expanded {
            out.push(expr);
            continue;
        }
        stack.push((expr, true));
        for child in const_exprs::children(store.expr(expr)).into_iter().rev() {
            stack.push((child, false));
        }
    }
    out
}
