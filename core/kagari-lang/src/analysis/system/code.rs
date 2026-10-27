//! The code of each mode: per-body summaries of navigation operations, calls and bindings,
//! and their transitive closure.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, DefMaps, TypeKey};
use crate::hir::{Expr, ExprId, ExprStore, Name};
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{FnKind, ItemId, ItemKind, ItemParent, item_tree};
use crate::signatures::{FnSig, ProgramSignatures};
use crate::ty::{FnDefId, Ty, TyKind};

use super::summary::{Summary, system_summary};
use super::{ModeKind, OpKind};

/// An `Action` built somewhere in the code `main` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BindingAction {
    /// `Action::Open(x)`: an overlay openable from every mode.
    Open(Option<ItemId>),
    /// `Action::OpenInGame(x)`: a game overlay openable from the game and game overlays.
    OpenInGame(Option<ItemId>),
    Back,
}

/// The std items the checks recognize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SystemStd {
    pub(super) ops: FxHashMap<ItemId, OpKind>,
    root_page: ItemId,
    overlay: ItemId,
    game_overlay: ItemId,
    dialog: ItemId,
    narrative: ItemId,
    system: ItemId,
    pub(super) action: Option<ActionVariants>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActionVariants {
    pub(super) item: ItemId,
    pub(super) open: Option<u32>,
    pub(super) open_in_game: Option<u32>,
    pub(super) back: Option<u32>,
}

impl SystemStd {
    pub(super) fn new(db: &dyn Db, maps: &DefMaps, sigs: &ProgramSignatures) -> Option<SystemStd> {
        let std_files: FxHashMap<&str, SourceFile> = maps
            .files
            .iter()
            .filter(|(_, krate)| *krate == CrateId::Std)
            .filter_map(|(file, _)| match file.path(db) {
                FilePath::Std(name) => Some((&**name, *file)),
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
        let is = |ty: &Ty, item: ItemId| matches!(ty.kind(), TyKind::Adt { item: adt, args } if *adt == item && args.is_empty());
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
    pub(super) std: &'db SystemStd,
    /// Each trait to the impls of it, built on first use.
    trait_impls: Option<FxHashMap<ItemId, Vec<ItemId>>>,
}

/// The operations of one mode's code.
#[derive(Clone, Debug, Default)]
pub(super) struct Code {
    pub(super) ops: Vec<super::Op>,
    /// The operations of the exit handlers it passes to `nav::start`: game-mode code.
    pub(super) exit_ops: Vec<super::Op>,
}

impl<'db> Ctx<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        program: Program,
        maps: &'db DefMaps,
        std: &'db SystemStd,
    ) -> Ctx<'db> {
        Ctx {
            db,
            program,
            maps,
            std,
            trait_impls: None,
        }
    }

    pub(super) fn item_name(&self, item: ItemId) -> Option<Name> {
        item_tree(self.db, item.file).item(item.index)?.name.clone()
    }

    /// The summary of an item's body.
    fn summary(&self, item: ItemId) -> &'db Summary {
        system_summary(self.db, self.program, item)
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
        let main = main_fn(self.db, self.program)?;
        sigs.fn_sig(main)?;
        let body = crate::hir::body_of(self.db, main)?;
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
pub(super) fn fn_target(
    store: &ExprStore,
    infer: &InferenceResult,
    expr: ExprId,
) -> Option<ItemId> {
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
