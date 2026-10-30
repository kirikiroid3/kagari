//! Human-readable paths of modules and items: `crate::scenes::Start`, `std::screen::Screen`,
//! `std::narrative::Meta::write`. Block-scoped items print a `{block}` segment.

use crate::db::Db;
use crate::def_map::{CrateId, Def, DefMaps, ModuleId, TypeKey};
use crate::hir::{Expr, ExprId};
use crate::item_tree::{ItemId, ItemKind, ItemParent, item_tree};
use crate::signatures::{ProgramSignatures, display_ty_qualified};
use crate::ty::{Const, GenericArg};

/// `crate::a::b`, `std::screen`.
pub fn module_path(_db: &dyn Db, maps: &DefMaps, module: ModuleId) -> String {
    let mut parts = Vec::new();
    let mut current = Some(module);
    while let Some(id) = current {
        let data = maps.module(id);
        match (&data.name, data.parent) {
            (_, None) => parts.push(match id.krate {
                CrateId::Game => "crate".to_owned(),
                CrateId::Std => "std".to_owned(),
            }),
            (Some(name), Some(_)) => parts.push(name.to_string()),
            (None, Some(_)) => parts.push("{block}".to_owned()),
        }
        current = data.parent;
    }
    parts.reverse();
    parts.join("::")
}

/// The path of a module, an item or an enum variant (`{enum path}::{variant}`).
pub fn def_path(db: &dyn Db, maps: &DefMaps, def: Def) -> String {
    match def {
        Def::Module(module) => module_path(db, maps, module),
        Def::Item(item) => item_path(db, maps, item),
        Def::Variant { enum_item, index } => {
            let tree = item_tree(db, enum_item.file);
            let variant = match tree.item(enum_item.index).map(|data| &data.kind) {
                Some(ItemKind::Enum { variants }) => variants
                    .get(index as usize)
                    .map_or_else(|| "?".to_owned(), |(name, _)| name.to_string()),
                _ => "?".to_owned(),
            };
            format!("{}::{variant}", item_path(db, maps, enum_item))
        }
    }
}

/// `{module path}::{name}`.
pub fn item_path(db: &dyn Db, maps: &DefMaps, item: ItemId) -> String {
    let tree = item_tree(db, item.file);
    let name = tree
        .item(item.index)
        .and_then(|data| data.name.clone())
        .unwrap_or_default();
    let module = maps.module_of_item(db, item);
    format!("{}::{name}", module_path(db, maps, module))
}

/// `{type path}::{name}` for impl items, `{trait path}::{name}` for trait items, and the
/// item path for any other item.
pub fn assoc_path(db: &dyn Db, maps: &DefMaps, item: ItemId) -> String {
    let tree = item_tree(db, item.file);
    let Some(data) = tree.item(item.index) else {
        return "?".to_owned();
    };
    let name = data.name.clone().unwrap_or_default();
    let ItemParent::Item(parent) = data.parent else {
        return item_path(db, maps, item);
    };
    let parent = ItemId {
        file: item.file,
        index: parent,
    };
    let owner = match tree.item(parent.index).map(|data| &data.kind) {
        Some(ItemKind::Trait { .. }) => item_path(db, maps, parent),
        Some(ItemKind::Impl { .. }) => match maps.impl_keys.get(&parent) {
            Some(TypeKey::Item(target)) => item_path(db, maps, *target),
            Some(TypeKey::Prim(prim)) => prim.name().to_owned(),
            None => "?".to_owned(),
        },
        _ => return item_path(db, maps, item),
    };
    format!("{owner}::{name}")
}

/// `::<…>` with an instance's generic arguments, every type named by its full path, so that
/// two instances of one function never share a path: a monomorphized scene has its own step
/// IDs and read flags. Empty without arguments.
pub fn generic_args_suffix(
    db: &dyn Db,
    sigs: &ProgramSignatures,
    maps: &DefMaps,
    args: &[GenericArg],
) -> String {
    if args.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = args
        .iter()
        .map(|arg| match arg {
            GenericArg::Type(ty) => display_ty_qualified(db, sigs, maps, ty),
            GenericArg::Const(Const::Value(value)) => value.to_string(),
            GenericArg::Const(_) => "_".to_owned(),
        })
        .collect();
    format!("::<{}>", shown.join(", "))
}

/// The number `N` of a closure in its path `{owner}::{closure#N}`: its position among the
/// closure expressions of the owner's body.
pub fn closure_number(db: &dyn Db, owner: ItemId, expr: ExprId) -> usize {
    crate::hir::body_of(db, owner)
        .and_then(|body| {
            body.store
                .exprs
                .iter()
                .filter(|(_, data)| matches!(data, Expr::Closure { .. }))
                .position(|(id, _)| id == expr)
        })
        .unwrap_or(0)
}
