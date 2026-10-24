//! The code of each mode: per-body summaries of navigation operations, calls and bindings,
//! and their transitive closure.

use kagari_syntax::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, DefMaps, TypeKey};
use crate::hir::{Expr, ExprId, ExprStore, Name, file_hir};
use crate::infer::walk::expr_children;
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{FnKind, ItemId, ItemKind, ItemParent, item_tree};
use crate::signatures::{FnSig, ProgramSignatures};
use crate::ty::{FnDefId, Ty};

use super::{ModeKind, Op, OpKind};

/// An `Action` built somewhere in the code `main` runs.
#[derive(Clone, Copy, Debug)]
pub(super) enum BindingAction {
    /// `Action::Open(x)`: an overlay openable from every mode.
    Open(Option<ItemId>),
    /// `Action::OpenInGame(x)`: a game overlay openable from the game and game overlays.
    OpenInGame(Option<ItemId>),
    Back,
}

/// What one part of a body (its own code or the exit handlers it passes) does directly.
#[derive(Clone, Debug, Default)]
struct Region {
    ops: Vec<Op>,
    /// Items called, and constants and statics read.
    calls: Vec<ItemId>,
    /// Functions referred to as values (handlers, pointers passed on).
    values: Vec<ItemId>,
}

#[derive(Clone, Debug, Default)]
struct Summary {
    own: Region,
    /// Code inside the exit handlers of `nav::start`: it runs in the game mode.
    exit: Region,
    bindings: Vec<BindingAction>,
}

/// The std items the checks recognize.
pub(super) struct SystemStd {
    ops: FxHashMap<ItemId, OpKind>,
    root_page: ItemId,
    overlay: ItemId,
    game_overlay: ItemId,
    dialog: ItemId,
    narrative: ItemId,
    system: ItemId,
    action: Option<ActionVariants>,
}

struct ActionVariants {
    item: ItemId,
    open: Option<u32>,
    open_in_game: Option<u32>,
    back: Option<u32>,
}

impl SystemStd {
    pub(super) fn new(db: &dyn Db, maps: &DefMaps, sigs: &ProgramSignatures) -> Option<SystemStd> {
        let std_files: FxHashMap<&str, SourceFile> = maps
            .files
            .iter()
            .filter(|(_, krate)| *krate == CrateId::Std)
            .filter_map(|(file, _)| match file.path(db) {
                FilePath::Std(name) => Some((name.as_str(), *file)),
                FilePath::Disk(_) => None,
            })
            .collect();
        let find = |file: &str, name: &str| -> Option<ItemId> {
            let source = *std_files.get(file)?;
            let tree = item_tree(db, source);
            tree.top.iter().find_map(|&index| {
                let item = tree.item(index)?;
                (item.name.as_deref() == Some(name)).then_some(ItemId {
                    file: source,
                    index,
                })
            })
        };
        let narrative = sigs.std.narrative?;
        let mut ops = FxHashMap::default();
        for (name, op) in [
            ("call", OpKind::Call),
            ("dialog", OpKind::Dialog),
            ("back", OpKind::Back),
            ("goto", OpKind::Goto),
            ("start", OpKind::Start),
            ("load", OpKind::Load),
            ("quit", OpKind::Quit),
        ] {
            if let Some(item) = find("system/nav.kis", name) {
                ops.insert(item, op);
            }
        }
        if let Some(&call) = maps
            .inherent
            .get(&TypeKey::Item(narrative))
            .and_then(|methods| methods.get("call"))
        {
            ops.insert(call, OpKind::GameCall);
        }
        let action = find("system.kis", "Action").map(|item| {
            let variant = |name: &str| {
                sigs.adt(item)?
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .and_then(|index| u32::try_from(index).ok())
            };
            ActionVariants {
                item,
                open: variant("Open"),
                open_in_game: variant("OpenInGame"),
                back: variant("Back"),
            }
        });
        Some(SystemStd {
            ops,
            root_page: find("system.kis", "RootPage")?,
            overlay: find("system.kis", "Overlay")?,
            game_overlay: find("system.kis", "GameOverlay")?,
            dialog: find("system.kis", "Dialog")?,
            narrative,
            system: find("system.kis", "System")?,
            action,
        })
    }

    /// The mode kind of a component signature.
    pub(super) fn mode_kind(&self, sig: &FnSig) -> Option<ModeKind> {
        if !sig.generics.params.is_empty()
            || sig.generics.parent_count != 0
            || sig.self_param.is_some()
            || sig.kind != FnKind::Plain
        {
            return None;
        }
        let is = |ty: &Ty, item: ItemId| matches!(ty, Ty::Adt { item: adt, args } if *adt == item && args.is_empty());
        match sig.params.as_slice() {
            [] if is(&sig.ret, self.root_page) => Some(ModeKind::RootPage),
            [] if is(&sig.ret, self.overlay) => Some(ModeKind::Overlay),
            [] if is(&sig.ret, self.dialog) => Some(ModeKind::Dialog),
            [param] if is(param, self.narrative) && is(&sig.ret, self.game_overlay) => {
                Some(ModeKind::GameOverlay)
            }
            _ => None,
        }
    }
}

/// `System::start` and `System::game` as written in `main`.
pub(super) struct SystemRoot {
    pub(super) start: ItemId,
    pub(super) game: ItemId,
    pub(super) bindings: Vec<BindingAction>,
}

pub(super) struct Ctx<'db> {
    pub(super) db: &'db dyn Db,
    program: Program,
    pub(super) maps: &'db DefMaps,
    pub(super) std: SystemStd,
    summaries: FxHashMap<ItemId, Summary>,
    /// Each trait to the impls of it, built on first use.
    trait_impls: Option<FxHashMap<ItemId, Vec<ItemId>>>,
}

/// The operations of one mode's code.
#[derive(Clone, Debug, Default)]
pub(super) struct Code {
    pub(super) ops: Vec<Op>,
    /// The operations of the exit handlers it passes to `nav::start`: game-mode code.
    pub(super) exit_ops: Vec<Op>,
}

impl<'db> Ctx<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        program: Program,
        maps: &'db DefMaps,
        std: SystemStd,
    ) -> Ctx<'db> {
        Ctx {
            db,
            program,
            maps,
            std,
            summaries: FxHashMap::default(),
            trait_impls: None,
        }
    }

    pub(super) fn item_name(&self, item: ItemId) -> Option<(Name, TextRange)> {
        let data = item_tree(self.db, item.file).item(item.index)?;
        Some((data.name.clone()?, data.name_range))
    }

    /// The summary of an item's body, computed once.
    fn summary(&mut self, item: ItemId) -> &Summary {
        if !self.summaries.contains_key(&item) {
            let summary = self.compute_summary(item);
            self.summaries.insert(item, summary);
        }
        self.summaries.entry(item).or_default()
    }

    fn compute_summary(&self, item: ItemId) -> Summary {
        let tree = item_tree(self.db, item.file);
        let follows = match tree.item(item.index).map(|data| &data.kind) {
            // A pushdown fn cannot call a `fn`, so it never runs system code.
            Some(ItemKind::Fn { kind, has_body, .. }) => *has_body && *kind != FnKind::Pushdown,
            Some(ItemKind::Const { has_value }) => *has_value,
            Some(ItemKind::Static { .. }) => true,
            _ => false,
        };
        if !follows {
            return Summary::default();
        }
        let Some(body) = file_hir(self.db, item.file)
            .bodies
            .get(item.index as usize)
            .and_then(Option::as_ref)
        else {
            return Summary::default();
        };
        let Some(infer) = inference(self.db, self.program, item) else {
            return Summary::default();
        };
        let mut walker = Walker {
            store: &body.store,
            infer,
            file: item.file,
            std: &self.std,
            summary: Summary::default(),
        };
        walker.expr(body.value, false);
        walker.summary
    }

    /// `item`, and for a method declared in a trait, the methods of the same name in every
    /// impl of the trait (the call may dispatch to any of them).
    fn expand(&mut self, item: ItemId) -> Vec<ItemId> {
        let mut out = vec![item];
        let tree = item_tree(self.db, item.file);
        let Some(data) = tree.item(item.index) else {
            return out;
        };
        let ItemParent::Item(parent) = data.parent else {
            return out;
        };
        if !matches!(
            tree.item(parent).map(|parent| &parent.kind),
            Some(ItemKind::Trait { .. })
        ) {
            return out;
        }
        let Some(name) = data.name.clone() else {
            return out;
        };
        let trait_item = ItemId {
            file: item.file,
            index: parent,
        };
        let db = self.db;
        let maps = self.maps;
        let impls = self.trait_impls.get_or_insert_with(|| {
            let mut impls: FxHashMap<ItemId, Vec<ItemId>> = FxHashMap::default();
            for (&impl_item, &trait_item) in &maps.impl_traits {
                impls.entry(trait_item).or_default().push(impl_item);
            }
            impls
        });
        for impl_item in impls.get(&trait_item).into_iter().flatten() {
            let impl_tree = item_tree(db, impl_item.file);
            let Some(ItemKind::Impl { items, .. }) =
                impl_tree.item(impl_item.index).map(|data| &data.kind)
            else {
                continue;
            };
            for &index in items {
                if impl_tree
                    .item(index)
                    .is_some_and(|method| method.name.as_ref() == Some(&name))
                {
                    out.push(ItemId {
                        file: impl_item.file,
                        index,
                    });
                }
            }
        }
        out
    }

    /// The operations of the code that runs from `roots`.
    pub(super) fn code(&mut self, roots: &[ItemId]) -> Code {
        let mut code = Code::default();
        let mut seen = FxHashSet::default();
        let mut stack = roots.to_vec();
        let mut exit_roots = Vec::new();
        while let Some(item) = stack.pop() {
            for item in self.expand(item) {
                if !seen.insert(item) {
                    continue;
                }
                let summary = self.summary(item);
                code.ops.extend(summary.own.ops.iter().copied());
                code.exit_ops.extend(summary.exit.ops.iter().copied());
                stack.extend(summary.own.calls.iter().copied());
                stack.extend(summary.own.values.iter().copied());
                exit_roots.extend(summary.exit.calls.iter().copied());
                exit_roots.extend(summary.exit.values.iter().copied());
            }
        }
        let mut seen = FxHashSet::default();
        let mut stack = exit_roots;
        while let Some(item) = stack.pop() {
            for item in self.expand(item) {
                if !seen.insert(item) {
                    continue;
                }
                let summary = self.summary(item);
                for region in [&summary.own, &summary.exit] {
                    code.exit_ops.extend(region.ops.iter().copied());
                    stack.extend(region.calls.iter().copied());
                    stack.extend(region.values.iter().copied());
                }
            }
        }
        code
    }

    /// The bindings built by the code `main` calls.
    fn bindings(&mut self, main: ItemId) -> Vec<BindingAction> {
        let mut bindings = Vec::new();
        let mut seen = FxHashSet::default();
        let mut stack = vec![main];
        while let Some(item) = stack.pop() {
            for item in self.expand(item) {
                if !seen.insert(item) {
                    continue;
                }
                let summary = self.summary(item);
                bindings.extend(summary.bindings.iter().copied());
                stack.extend(summary.own.calls.iter().copied());
                stack.extend(summary.exit.calls.iter().copied());
            }
        }
        bindings
    }

    /// `fn main` of the root module and the `System` record it returns, when its `start`
    /// and `game` fields name functions.
    pub(super) fn system_root(&mut self, sigs: &ProgramSignatures) -> Option<SystemRoot> {
        let root = *self.program.root(self.db);
        let main = main_fn(self.db, self.program)?;
        sigs.fn_sig(main)?;
        let body = file_hir(self.db, root)
            .bodies
            .get(main.index as usize)
            .and_then(Option::as_ref)?;
        let infer = inference(self.db, self.program, main)?;
        let system = self.std.system;
        let fields = body
            .store
            .exprs
            .iter()
            .find_map(|(expr, data)| match data {
                Expr::Record { fields, .. }
                    if infer
                        .records
                        .get(&expr)
                        .is_some_and(|ctor| ctor.adt == system) =>
                {
                    Some(fields)
                }
                _ => None,
            })?;
        let field = |name: &str| {
            let (_, value) = fields.iter().find(|(field, _)| field == name)?;
            fn_target(&body.store, infer, *value)
        };
        let start = field("start")?;
        let game = field("game")?;
        let bindings = self.bindings(main);
        Some(SystemRoot {
            start,
            game,
            bindings,
        })
    }
}

/// `fn main` of the root module: the first top-level item of the root file named `main`
/// that is a plain `fn` with a body.
pub(crate) fn main_fn(db: &dyn Db, program: Program) -> Option<ItemId> {
    let root = *program.root(db);
    let tree = item_tree(db, root);
    tree.top.iter().find_map(|&index| {
        let data = tree.item(index)?;
        let is_main = data.name.as_deref() == Some("main")
            && matches!(
                data.kind,
                ItemKind::Fn {
                    kind: FnKind::Plain,
                    has_body: true,
                    ..
                }
            );
        is_main.then_some(ItemId { file: root, index })
    })
}

/// The function a path expression names.
fn fn_target(store: &ExprStore, infer: &InferenceResult, expr: ExprId) -> Option<ItemId> {
    if !matches!(store.expr(expr), Expr::Path(_)) {
        return None;
    }
    match infer.paths.get(&expr) {
        Some(ValueRef::Fn(fn_ref)) => match fn_ref.def {
            FnDefId::Item(item) => Some(item),
            _ => None,
        },
        _ => None,
    }
}

/// Collects the [`Summary`] of one body.
struct Walker<'a> {
    store: &'a ExprStore,
    infer: &'a InferenceResult,
    file: SourceFile,
    std: &'a SystemStd,
    summary: Summary,
}

impl Walker<'_> {
    fn region(&mut self, exit: bool) -> &mut Region {
        if exit {
            &mut self.summary.exit
        } else {
            &mut self.summary.own
        }
    }

    fn op_item(&self, expr: ExprId) -> Option<OpKind> {
        let item = fn_target(self.store, self.infer, expr)?;
        self.std.ops.get(&item).copied()
    }

    fn record_op(&mut self, kind: OpKind, target: Option<ItemId>, expr: ExprId, exit: bool) {
        let op = Op {
            kind,
            target,
            file: self.file,
            range: self.store.expr_range(expr),
        };
        self.region(exit).ops.push(op);
    }

    /// The `Action` variant a callee path names: `Open` or `OpenInGame`.
    fn action_ctor(&self, expr: ExprId) -> Option<fn(Option<ItemId>) -> BindingAction> {
        let action = self.std.action.as_ref()?;
        let Some(ValueRef::Fn(fn_ref)) = self.infer.paths.get(&expr) else {
            return None;
        };
        let FnDefId::VariantCtor(adt, variant) = fn_ref.def else {
            return None;
        };
        if adt != action.item {
            return None;
        }
        if Some(variant) == action.open {
            Some(BindingAction::Open)
        } else if Some(variant) == action.open_in_game {
            Some(BindingAction::OpenInGame)
        } else {
            None
        }
    }

    fn expr(&mut self, expr: ExprId, exit: bool) {
        match self.store.expr(expr) {
            Expr::Call { callee, args } => {
                let (callee, args) = (*callee, args.as_slice());
                if let Some(op) = self.op_item(callee) {
                    // `Narrative::call(&game, o)` names the receiver first.
                    let target_index = usize::from(op == OpKind::GameCall);
                    self.op_call(expr, op, args, target_index, exit);
                    return;
                }
                if let Some(ctor) = self.action_ctor(callee) {
                    let target = args
                        .first()
                        .and_then(|&arg| fn_target(self.store, self.infer, arg));
                    self.summary.bindings.push(ctor(target));
                } else if matches!(self.store.expr(callee), Expr::Path(_)) {
                    self.path(callee, exit, true);
                } else {
                    self.expr(callee, exit);
                }
                for &arg in args {
                    self.expr(arg, exit);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                let (receiver, args) = (*receiver, args.as_slice());
                let method =
                    self.infer
                        .method_calls
                        .get(&expr)
                        .and_then(|call| match call.func.def {
                            FnDefId::Item(item) => Some(item),
                            _ => None,
                        });
                self.expr(receiver, exit);
                if let Some(method) = method {
                    if let Some(&op) = self.std.ops.get(&method) {
                        self.op_call(expr, op, args, 0, exit);
                        return;
                    }
                    self.region(exit).calls.push(method);
                }
                for &arg in args {
                    self.expr(arg, exit);
                }
            }
            Expr::Path(_) => self.path(expr, exit, false),
            data => {
                let mut children = Vec::new();
                expr_children(data, &mut |child| children.push(child), &mut |_| {});
                for child in children {
                    self.expr(child, exit);
                }
            }
        }
    }

    /// A call of a navigation operation. `target_index` is the argument naming the mode.
    fn op_call(
        &mut self,
        expr: ExprId,
        op: OpKind,
        args: &[ExprId],
        target_index: usize,
        exit: bool,
    ) {
        let target = match op.target_kind() {
            Some(ModeKind::Game) | None => None,
            Some(_) => args
                .get(target_index)
                .and_then(|&arg| fn_target(self.store, self.infer, arg)),
        };
        self.record_op(op, target, expr, exit);
        for (index, &arg) in args.iter().enumerate() {
            if index == target_index && target.is_some() {
                // A named target is an edge of the mode graph, not code of this mode.
                continue;
            }
            // `nav::start(entry, args, on_exit)`: the exit handler runs in the game mode.
            let arg_exit = exit || (op == OpKind::Start && index == 2);
            self.expr(arg, arg_exit);
        }
    }

    fn path(&mut self, expr: ExprId, exit: bool, callee: bool) {
        match self.infer.paths.get(&expr) {
            Some(ValueRef::Fn(fn_ref)) => match fn_ref.def {
                FnDefId::Item(item) => {
                    if let Some(&op) = self.std.ops.get(&item) {
                        // An operation used as a value (`on_click = nav::back`): it may run
                        // with any argument.
                        self.record_op(op, None, expr, exit);
                    } else if callee {
                        self.region(exit).calls.push(item);
                    } else {
                        self.region(exit).values.push(item);
                    }
                }
                FnDefId::VariantCtor(..) => {
                    if let Some(ctor) = self.action_ctor(expr) {
                        self.summary.bindings.push(ctor(None));
                    }
                }
                FnDefId::StructCtor(_) => {}
            },
            Some(ValueRef::Ctor(ctor)) => {
                if let Some(action) = &self.std.action
                    && ctor.adt == action.item
                    && ctor.variant.is_some()
                    && ctor.variant == action.back
                {
                    self.summary.bindings.push(BindingAction::Back);
                }
            }
            Some(ValueRef::Const { item, .. } | ValueRef::Static(item)) => {
                let item = *item;
                self.region(exit).calls.push(item);
            }
            _ => {}
        }
        let mut children = Vec::new();
        expr_children(
            self.store.expr(expr),
            &mut |child| children.push(child),
            &mut |_| {},
        );
        for child in children {
            self.expr(child, exit);
        }
    }
}
