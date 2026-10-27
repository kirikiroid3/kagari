//! Tabulated `const fn` calls: every call of a `const fn` from a story body with a
//! non-constant argument, evaluated on every combination of its arguments' values.
//!
//! The calls are the `Call` statements of the story item's MIR; which arguments are
//! constant, their types and the shape of composite arguments come from the call's HIR
//! expression (`const_exprs`, inference).

use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Anchor, DiagnosticCode, NodeId, Owner};
use crate::hir::{ArrayExpr, Expr, ExprId};
use crate::infer::ValueRef;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::mir::{ItemMir, StatementKind, item_mir};
use crate::signatures::FnSig;
use crate::ty::{FnDefId, FnRef, Ty, TyKind};

use super::const_exprs::{ConstExprs, const_exprs};
use super::ctx::BodyRef;
use super::domain::{self, DomainCtx};
use super::machine::value::Value;
use super::machine::{self, CallSite, ErrorKind, EvalError};
use super::value::{AssetKind, ConstValue, display_value};
use super::{Driver, Fault, TABLE_DOMAIN_CAP, Table};

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
        at: None,
    }
}

/// A story body with what tabulation reads.
#[derive(Clone, Copy)]
struct Story<'db> {
    body: BodyRef<'db>,
    consts: &'db ConstExprs,
    mir: &'db ItemMir,
}

impl<'db> Driver<'db> {
    /// Tabulates every `const fn` call of a story body with a non-constant argument,
    /// inner calls first so that outer calls range over their results.
    pub(super) fn tabulate(
        &mut self,
        item: ItemId,
        game: bool,
        assets: &FxHashMap<AssetKind, Vec<ConstValue>>,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        let (Some(body), Some(consts), Some(mir)) = (
            self.ctx.body(item),
            const_exprs(self.db, self.program, item),
            item_mir(self.db, self.program, item).as_ref(),
        ) else {
            return;
        };
        let story = Story { body, consts, mir };
        let mut seen = FxHashSet::default();
        let bodies = std::iter::once(&mir.main).chain(mir.closures.iter().map(|(_, body)| body));
        for mir_body in bodies {
            for (_, block) in mir_body.blocks.iter() {
                for stmt in &block.stmts {
                    let (StatementKind::Call { .. }, Some(expr)) = (&stmt.kind, stmt.src) else {
                        continue;
                    };
                    if seen.insert(expr) {
                        self.tabulate_call(story, game, expr, assets, functions);
                    }
                }
            }
        }
    }

    fn tabulate_call(
        &mut self,
        story: Story<'db>,
        game: bool,
        expr: ExprId,
        assets: &FxHashMap<AssetKind, Vec<ConstValue>>,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        let item = story.body.item;
        if story.consts.contains(expr) {
            return;
        }
        let Some((fn_ref, args)) = self.const_call(story.body, expr) else {
            return;
        };
        if args.iter().all(|arg| story.consts.contains(arg.expr)) {
            return;
        }
        let result = {
            let domain = self.domain(assets, functions);
            self.table(story, &domain, expr, &fn_ref, &args)
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
                let at = Anchor::Node(Owner::Body(item), NodeId::Expr(expr));
                self.report(game, at, &error, suffix.as_deref());
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
        story: Story<'db>,
        domain: &DomainCtx<'_>,
        expr: ExprId,
        ty: &Ty,
        direct: bool,
    ) -> Result<Option<Vec<ConstValue>>, TooLarge> {
        let body = story.body;
        if story.consts.contains(expr) {
            return Ok(self
                .const_expr_value(body, story.consts, story.mir, expr)
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
                    let direct = !matches!(item_ty.kind(), TyKind::Ref { .. });
                    let item_ty = match item_ty.kind() {
                        TyKind::Ref { inner, .. } => inner,
                        _ => item_ty,
                    };
                    let Some(values) = self.arg_values(story, domain, *item, item_ty, direct)?
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
                            .map(|items| ConstValue::Tuple(items.into()))
                            .collect()
                    }));
                }
                Expr::Array(ArrayExpr::List(items)) => {
                    return Ok(parts(items)?.map(|parts| {
                        domain::product(&parts)
                            .into_iter()
                            .map(|items| ConstValue::Array(items.into()))
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
                                    fields: fields.into(),
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
    fn const_call(&self, body: BodyRef<'db>, expr: ExprId) -> Option<(FnRef, Vec<CallArg>)> {
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
            Some(match ty.kind() {
                TyKind::Ref { mutable, inner } => CallArg {
                    expr: arg,
                    ty: inner.clone(),
                    by_ref: Some(*mutable),
                    direct: false,
                },
                _ => CallArg {
                    expr: arg,
                    ty: ty.clone(),
                    by_ref: None,
                    direct: true,
                },
            })
        };
        match body.body.store.expr(expr) {
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
                    ty = match ty.kind() {
                        TyKind::Ref { inner, .. } => inner.clone(),
                        _ => return None,
                    };
                }
                let receiver_arg = match (call.receiver.autoref, ty.kind()) {
                    (Some(mutable), _) => CallArg {
                        expr: *receiver,
                        ty: ty.clone(),
                        by_ref: Some(mutable),
                        direct,
                    },
                    (None, TyKind::Ref { mutable, inner }) => CallArg {
                        expr: *receiver,
                        ty: inner.clone(),
                        by_ref: Some(*mutable),
                        direct: false,
                    },
                    (None, _) => CallArg {
                        expr: *receiver,
                        ty: ty.clone(),
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

    /// The value of a constant argument; a reference argument `&x` gives the value of `x`.
    fn const_arg_value(&self, story: Story<'db>, arg: &CallArg) -> Option<ConstValue> {
        let body = story.body;
        match (arg.by_ref, body.body.store.expr(arg.expr)) {
            (Some(_), Expr::Ref { expr: inner, .. }) => self
                .const_expr_value(body, story.consts, story.mir, *inner)
                .or_else(|| self.promoted_pointee(story.mir, arg.expr)),
            _ => self.const_expr_value(body, story.consts, story.mir, arg.expr),
        }
    }

    /// The table of one call; `Ok(None)` when it cannot be built without a new diagnostic.
    #[allow(clippy::type_complexity)]
    fn table(
        &self,
        story: Story<'db>,
        domain: &DomainCtx<'_>,
        expr: ExprId,
        fn_ref: &FnRef,
        args: &[CallArg],
    ) -> Result<Option<Table>, (EvalError, Option<String>)> {
        let body = story.body;
        let mut domains = Vec::with_capacity(args.len());
        let mut size: u64 = 1;
        for arg in args {
            let values = if story.consts.contains(arg.expr) {
                match self.const_arg_value(story, arg) {
                    Some(value) => vec![value],
                    None => return Ok(None),
                }
            } else {
                match self.arg_values(story, domain, arg.expr, &arg.ty, arg.direct) {
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
        let at = Anchor::Node(Owner::Body(body.item), NodeId::Expr(expr));
        let ty = body.infer.expr_types.get(expr).cloned();
        let mut rows = Vec::new();
        for inputs in domain::product(&domains) {
            let mut machine = self.ctx.machine();
            let mut values = Vec::with_capacity(inputs.len());
            let mut cells = Vec::with_capacity(inputs.len());
            for (input, arg) in inputs.iter().zip(args) {
                let value = machine.thaw(input);
                if arg.by_ref.is_some() {
                    let cell = machine.alloc(value);
                    cells.push(Some(cell));
                    values.push(Value::Ref(Box::new(machine::Place::cell(cell))));
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
            let site = CallSite {
                at: Some(at),
                ty: ty.clone(),
            };
            let result = machine
                .call_value(site, &callee, values)
                .and_then(|value| machine.freeze_result(&value));
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
                    match machine.freeze_result(&value) {
                        Ok(value) => parts.push(value),
                        Err(error) => return Err((error, shown())),
                    }
                }
                ConstValue::Tuple(parts.into())
            };
            rows.push((inputs, output));
        }
        Ok(Some(Table {
            func: fn_ref.clone(),
            rows,
            writes,
        }))
    }
}
