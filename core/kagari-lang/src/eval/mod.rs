//! Const evaluation (compilation stage 4): constants and statics, the constant parts of
//! story bodies, tabulated `const fn` calls from the story, and the renders steps use.
//!
//! See `docs/syntax/src/compilation.md` (stage 4), `docs/syntax/src/kinds.md`,
//! `docs/syntax/src/presentation.md` and `docs/std/src/checks.md#const-evaluation-errors`.

pub(crate) mod const_exprs;
pub(crate) mod ctx;
pub(crate) mod domain;
pub(crate) mod machine;
mod num;
pub(crate) mod queries;
mod renders;
pub(crate) mod std_items;
mod tabulate;
mod value;

#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, Def, Resolution, def_maps};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::hir::{Expr, ExprId, Literal, Pat, body_of, signature_of};
use crate::infer::ValueRef;
use crate::input::{Program, SourceFile};
use crate::item_tree::ItemIndex;
use crate::item_tree::{FnKind, ItemId, ItemKind, ItemParent, item_tree};
use crate::mir::{ItemMir, item_mir};
use crate::resolve::body_resolution;
use crate::signatures::{FnSig, ItemSig, program_signatures};
use crate::thir::{PromotedId, literal_value};
use crate::ty::{FnRef, GenericArgs};

pub use const_exprs::{ConstExprs, ConstLocal, const_exprs};
pub use machine::MAX_CALL_DEPTH;
pub use machine::screen::{
    Camera, Channel, ChannelId, Cursor, CursorAnchor, Entity, EntityProp, Hud, LayerTarget,
    LogicalScreen, PendingChange, PropTarget, Scheduled, ScheduledKind, Screen, ScreenObj,
    StepState, Target,
};
pub use num::{Decimal, Rat};
pub(crate) use num::{int_bits, is_signed};
pub use queries::{const_value, promoted_value};
pub use value::{AssetKind, ConstValue, HandleKind, Text, display_value};

use ctx::{BodyRef, Ctx};
use domain::DomainCtx;
use machine::value::GenericEnv;
use machine::{ErrorKind, EvalError};
use queries::{StaticValue, static_value};
use std_items::{Builtin, std_table};

/// The number of evaluation steps one top-level evaluation may take (*tentative*: a
/// compiler setting). Running out is `E4003`.
pub const EVAL_STEP_BUDGET: u64 = 1_000_000;

/// The largest number of argument combinations a tabulated call may have (*tentative*: a
/// compiler setting). More is `E4004`.
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
    /// Tabulated `const fn` calls (incl. `to_text`) in `pushdown fn` bodies.
    pub tables: FxHashMap<(ItemId, ExprId), Table>,
    /// Every render used by a step.
    pub renders: FxHashMap<RenderId, RenderInfo>,
    pub diagnostics: Vec<Diag>,
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
    let Some(std) = std_table(db, program).as_ref() else {
        return ConstEval::default();
    };
    let sigs = program_signatures(db, program);
    let mut driver = Driver {
        db,
        program,
        ctx: Ctx::new(db, program, std, sigs),
        out: ConstEval::default(),
        texts: Vec::new(),
        text_set: FxHashSet::default(),
    };
    driver.run();
    driver.out
}

struct Driver<'db> {
    db: &'db dyn Db,
    program: Program,
    ctx: Ctx<'db>,
    out: ConstEval,
    texts: Vec<Text>,
    text_set: FxHashSet<Text>,
}

/// Whether `index` or its parent impl or trait has generic parameters.
fn is_generic(db: &dyn Db, file: SourceFile, index: ItemIndex) -> bool {
    let has_generics = |index: ItemIndex| {
        signature_of(db, ItemId { file, index })
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

impl<'db> Driver<'db> {
    fn run(&mut self) {
        let maps = def_maps(self.db, self.program);
        let mut story = Vec::new();
        for &(file, krate) in &maps.files {
            let game = krate == CrateId::Game;
            let tree = item_tree(self.db, file);
            for (index, item) in tree.items.iter().enumerate() {
                let Some(index) = ItemIndex::from_usize(index) else {
                    continue;
                };
                let id = ItemId { file, index };
                match item.kind {
                    ItemKind::Const { has_value: true } if !is_generic(self.db, file, index) => {
                        match const_value(self.db, self.program, id, GenericArgs::empty()) {
                            Ok(value) => {
                                self.out.consts.insert(id, value.clone());
                            }
                            Err(error) => {
                                self.report(game, Anchor::ItemName(id), error, None);
                            }
                        }
                    }
                    ItemKind::Static { .. } => match static_value(self.db, self.program, id) {
                        Ok(value) => {
                            self.out.statics.insert(id, value.value.clone());
                        }
                        Err(error) => self.report(game, Anchor::ItemName(id), error, None),
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
            self.promoted(item, game);
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

    /// Reports `error` at `anchor` (the code that triggered the evaluation) with a note at
    /// the failing site. Only game code reports; aborts and probes never do.
    fn report(&mut self, game: bool, anchor: Anchor, error: &EvalError, suffix: Option<&str>) {
        let ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        if !game {
            return;
        }
        let mut fault = fault.clone();
        if fault.code == DiagnosticCode::EvalOverflow
            && error
                .at
                .is_some_and(|at| at.file() == self.ctx.std.time_file)
        {
            fault = machine::screen::duration_overflow();
        }
        let message = match suffix {
            Some(suffix) => format!("{}{suffix}", fault.message),
            None => fault.message,
        };
        let mut diagnostic = Diag::new(anchor, fault.code, message);
        if let Some(at) = error.at
            && at != anchor
        {
            diagnostic = diagnostic.with_related(at, "evaluation failed here");
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

    /// The string literals of `item`'s body, except asset and system font paths.
    fn collect_literal_texts(&mut self, item: ItemId) {
        let Some(body) = body_of(self.db, item) else {
            return;
        };
        let resolution = body_resolution(self.db, self.program, item).body.as_ref();
        let is_asset_fn = |callee: ExprId| {
            matches!(
                resolution.and_then(|res| res.store.exprs.get(&callee)),
                Some(Resolution::Def(Def::Item(item)))
                    if matches!(
                        self.ctx.std.builtin(*item),
                        Some(Builtin::Asset | Builtin::SystemFont)
                    )
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

    /// Evaluates the promoted constants of a story body: reports their errors and
    /// collects their texts.
    fn promoted(&mut self, item: ItemId, game: bool) {
        let Some(mir) = item_mir(self.db, self.program, item).as_ref() else {
            return;
        };
        for (index, src) in mir.promoted_src.iter().enumerate() {
            let Some(id) = PromotedId::from_usize(index) else {
                continue;
            };
            match promoted_value(self.db, self.program, item, id) {
                Ok(value) => self.collect_texts(value),
                Err(error) => {
                    let at = Anchor::Node(Owner::Body(item), NodeId::Expr(*src));
                    self.report(game, at, error, None);
                }
            }
        }
    }

    /// The value of the constant expression `expr` of a story body: its promoted
    /// constant, or a literal, a path or `()`, which are not promoted.
    fn const_expr_value(
        &self,
        body: BodyRef<'db>,
        consts: &ConstExprs,
        mir: &ItemMir,
        expr: ExprId,
    ) -> Option<ConstValue> {
        if let Some(index) = mir.promoted_of(expr) {
            return promoted_value(self.db, self.program, body.item, index)
                .as_ref()
                .ok()
                .cloned();
        }
        let store = &body.body.store;
        match store.expr(expr) {
            Expr::Literal(literal) => literal_value(literal, false),
            Expr::Path(_) => match body.infer.paths.get(&expr)? {
                ValueRef::Const { item, args } => {
                    const_value(self.db, self.program, *item, GenericArgs::new(args.clone()))
                        .as_ref()
                        .ok()
                        .cloned()
                }
                ValueRef::Fn(fn_ref) => Some(ConstValue::Fn(fn_ref.clone())),
                ValueRef::Ctor(ctor) => Some(ConstValue::Adt {
                    item: ctor.adt,
                    variant: ctor.variant.unwrap_or(0),
                    fields: imbl::Vector::new(),
                }),
                ValueRef::Static(item) => match static_value(self.db, self.program, *item) {
                    Ok(StaticValue {
                        handle: Some(kind), ..
                    }) => Some(ConstValue::Handle {
                        kind: *kind,
                        item: *item,
                    }),
                    Ok(StaticValue {
                        handle: None,
                        value,
                    }) => Some(value.clone()),
                    Err(_) => None,
                },
                ValueRef::Local(local) => {
                    // A constant local bound by a whole `let x = source;`.
                    let source = consts.locals.get(local)?;
                    let whole = matches!(store.pat(source.pat), Pat::Bind { .. })
                        && body.res.bindings.get(&source.pat) == Some(local);
                    if !whole {
                        return None;
                    }
                    self.const_expr_value(body, consts, mir, source.source)
                }
                ValueRef::ConstParam { .. } | ValueRef::Error => None,
            },
            _ if body
                .infer
                .expr_types
                .get(expr)
                .is_some_and(|ty| ty.is_unit()) =>
            {
                Some(ConstValue::Tuple(imbl::Vector::new()))
            }
            _ => None,
        }
    }

    /// The value a promoted reference `&x` (the root of promoted constant of `expr`)
    /// points to.
    fn promoted_pointee(&self, mir: &'db ItemMir, expr: ExprId) -> Option<ConstValue> {
        let index = mir.promoted_of(expr)?;
        let body = mir.promoted(index)?;
        let env = GenericEnv::for_item(self.ctx.sigs, body.source.owner(), GenericArgs::empty());
        let mut machine = self.ctx.machine();
        let value = machine.run_body(body, env, Vec::new()).ok()?;
        let value = machine.deref_value(value).ok()?;
        machine.freeze(&value).ok()
    }

    /// The asset constants of the program by kind.
    fn assets(&self) -> FxHashMap<AssetKind, Vec<ConstValue>> {
        let maps = def_maps(self.db, self.program);
        let mut out: FxHashMap<AssetKind, Vec<ConstValue>> = FxHashMap::default();
        let mut seen = FxHashSet::default();
        for &(file, _) in &maps.files {
            for index in 0..item_tree(self.db, file).items.len() {
                let Some(index) = ItemIndex::from_usize(index) else {
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
                let Some(index) = ItemIndex::from_usize(index) else {
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
}
