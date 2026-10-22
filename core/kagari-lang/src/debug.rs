//! A textual dump of name resolution, for tests and the `check` example.

use std::fmt::Write as _;
use std::path::Path as FsPath;

use kagari_syntax::{TextRange, TextSize};

use crate::db::Db;
use crate::def_map::{CrateId, Def, DefMaps, ModuleId, Resolution, TypeKey, def_maps};
use crate::diagnostic::line_col;
use crate::hir::{Body, Expr, ExprStore, Name, Pat, PathKind, TypeRef, file_hir};
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, ItemParent, item_tree};
use crate::resolve::{BodyResolution, StoreResolution, file_resolution};

/// For each game-crate file (sorted by path) and each resolved path, in source order:
/// `{file}:{line}:{col} {path} -> {description}`, plus one line per `break`/`continue` naming
/// its loop. Paths synthesized by form desugaring are not shown.
pub fn resolution_dump(db: &dyn Db, program: Program, base: Option<&FsPath>) -> String {
    let maps = def_maps(db, program);
    let mut files: Vec<SourceFile> = maps
        .files
        .iter()
        .filter(|(_, krate)| *krate == CrateId::Game)
        .map(|(file, _)| *file)
        .collect();
    files.sort_by_key(|file| file.path(db).clone());
    let mut out = String::new();
    for file in files {
        let dumper = Dumper { db, maps, file };
        let mut lines: Vec<(TextSize, String)> = Vec::new();
        let hir = file_hir(db, file);
        let resolution = file_resolution(db, program, file);
        for (index, item) in resolution.items.iter().enumerate() {
            let Some(item) = item else {
                continue;
            };
            if let Some(Some(signature)) = hir.signatures.get(index) {
                dumper.store_lines(&signature.store, &item.signature, None, &mut lines);
            }
            if let (Some(Some(body)), Some(resolved)) = (hir.bodies.get(index), &item.body) {
                dumper.store_lines(
                    &body.store,
                    &resolved.store,
                    Some((body, resolved)),
                    &mut lines,
                );
                for (&jump, &target) in &resolved.loop_targets {
                    let what = match body.store.expr(jump) {
                        Expr::Continue { .. } => "continue",
                        _ => "break",
                    };
                    let range = body.store.expr_range(jump);
                    let target = dumper.position(body.store.expr_range(target).start());
                    lines.push((range.start(), format!("{what} -> loop@{target}")));
                }
            }
        }
        lines.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let shown = file.path(db).display(base);
        for (offset, line) in lines {
            let _ = writeln!(out, "{shown}:{} {line}", dumper.position(offset));
        }
    }
    out
}

struct Dumper<'a> {
    db: &'a dyn Db,
    maps: &'a DefMaps,
    file: SourceFile,
}

impl Dumper<'_> {
    fn position(&self, offset: TextSize) -> String {
        let (line, col) = line_col(self.db, self.file.path(self.db), offset);
        format!("{line}:{col}")
    }

    fn store_lines(
        &self,
        store: &ExprStore,
        resolved: &StoreResolution,
        body: Option<(&Body, &BodyResolution)>,
        lines: &mut Vec<(TextSize, String)>,
    ) {
        let mut push = |range: TextRange, text: String, resolution: &Resolution| {
            lines.push((
                range.start(),
                format!("{text} -> {}", self.describe(resolution, store, body)),
            ));
        };
        for (&id, resolution) in &resolved.exprs {
            let path = match store.expr(id) {
                Expr::Path(path) | Expr::Record { path, .. } => path,
                _ => continue,
            };
            if matches!(path.kind, PathKind::Lang(_)) {
                continue;
            }
            push(store.expr_range(id), path.display(), resolution);
        }
        for (&id, resolution) in &resolved.types {
            if let TypeRef::Path(path) = store.type_ref(id) {
                push(store.type_range(id), path.display(), resolution);
            }
        }
        for (&id, resolution) in &resolved.const_args {
            if let TypeRef::Path(path) = store.type_ref(id) {
                push(store.type_range(id), path.display(), resolution);
            }
        }
        for (&id, resolution) in &resolved.pats {
            let text = match store.pat(id) {
                Pat::Path(path) | Pat::TupleStruct { path, .. } | Pat::Record { path, .. } => {
                    path.display()
                }
                Pat::Bind { name, .. } => name.to_string(),
                _ => continue,
            };
            push(store.pat_range(id), text, resolution);
        }
        for (&(id, slot), resolution) in &resolved.range_bounds {
            if let Pat::Range { start, end, .. } = store.pat(id) {
                let bound = if slot == 0 { start } else { end };
                if let Some(crate::hir::PatBound::Path(path)) = bound {
                    let range = path
                        .segments
                        .first()
                        .map_or_else(|| store.pat_range(id), |segment| segment.range);
                    push(range, path.display(), resolution);
                }
            }
        }
        if let Some((_, body_resolution)) = body {
            for (&pat, &local) in &body_resolution.bindings {
                if let Pat::Bind { name, .. } = store.pat(pat) {
                    push(
                        store.pat_range(pat),
                        name.to_string(),
                        &Resolution::Local(local),
                    );
                }
            }
        }
    }

    fn describe(
        &self,
        resolution: &Resolution,
        store: &ExprStore,
        body: Option<(&Body, &BodyResolution)>,
    ) -> String {
        match resolution {
            Resolution::Local(local) => {
                let Some((_, resolved)) = body else {
                    return "local".to_owned();
                };
                let local = &resolved.locals[*local];
                match local.pat {
                    Some(pat) => format!(
                        "local {}@{}",
                        local.name,
                        self.position(store.pat_range(pat).start())
                    ),
                    None => format!("local {}", local.name),
                }
            }
            Resolution::Def(def) => self.describe_def(*def),
            Resolution::Prim(prim) => format!("prim {}", prim.name()),
            Resolution::GenericParam { owner, index } => {
                format!("generic {}", self.generic_name(*owner, *index))
            }
            Resolution::SelfType(_) => "Self".to_owned(),
            Resolution::Assoc(item) => {
                let kind = self.item_kind(*item);
                format!("{kind} {}", self.assoc_path(*item))
            }
            Resolution::Deferred { base, rest } => {
                let base = match base.as_ref() {
                    Resolution::GenericParam { owner, index } => self.generic_name(*owner, *index),
                    Resolution::SelfType(_) => Name::new_static("Self"),
                    Resolution::Def(def) => Name::new(self.def_path(*def)),
                    _ => Name::new_static("?"),
                };
                let rest: Vec<&str> = rest.iter().map(Name::as_str).collect();
                format!("deferred {base}::{}", rest.join("::"))
            }
            Resolution::Error => "unresolved".to_owned(),
        }
    }

    fn describe_def(&self, def: Def) -> String {
        let kind = match def {
            Def::Module(_) => "module",
            Def::Variant { .. } => "variant",
            Def::Item(item) => self.item_kind(item),
        };
        format!("{kind} {}", self.def_path(def))
    }

    fn item_kind(&self, item: ItemId) -> &'static str {
        item_tree(self.db, item.file)
            .item(item.index)
            .map_or("item", |data| data.kind_name())
    }

    fn generic_name(&self, owner: ItemId, index: u32) -> Name {
        file_hir(self.db, owner.file)
            .signatures
            .get(owner.index as usize)
            .and_then(Option::as_ref)
            .and_then(|signature| signature.generics.get(index as usize))
            .map_or_else(|| Name::new_static("?"), |param| param.name.clone())
    }

    fn module_path(&self, module: ModuleId) -> String {
        let mut parts = Vec::new();
        let mut current = Some(module);
        while let Some(id) = current {
            let data = self.maps.module(id);
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

    fn def_path(&self, def: Def) -> String {
        match def {
            Def::Module(module) => self.module_path(module),
            Def::Item(item) => self.item_path(item),
            Def::Variant { enum_item, index } => {
                let tree = item_tree(self.db, enum_item.file);
                let variant = match tree.item(enum_item.index).map(|data| &data.kind) {
                    Some(ItemKind::Enum { variants }) => variants
                        .get(index as usize)
                        .map_or_else(|| "?".to_owned(), |(name, _, _)| name.to_string()),
                    _ => "?".to_owned(),
                };
                format!("{}::{variant}", self.item_path(enum_item))
            }
        }
    }

    fn item_path(&self, item: ItemId) -> String {
        let tree = item_tree(self.db, item.file);
        let name = tree
            .item(item.index)
            .and_then(|data| data.name.clone())
            .unwrap_or_default();
        let module = self.maps.module_of_item(self.db, item);
        format!("{}::{name}", self.module_path(module))
    }

    /// `{type path}::{name}` for impl items, `{trait path}::{name}` for trait items.
    fn assoc_path(&self, item: ItemId) -> String {
        let tree = item_tree(self.db, item.file);
        let Some(data) = tree.item(item.index) else {
            return "?".to_owned();
        };
        let name = data.name.clone().unwrap_or_default();
        let ItemParent::Item(parent) = data.parent else {
            return self.item_path(item);
        };
        let parent = ItemId {
            file: item.file,
            index: parent,
        };
        let owner = match tree.item(parent.index).map(|data| &data.kind) {
            Some(ItemKind::Trait { .. }) => self.item_path(parent),
            _ => match self.maps.impl_keys.get(&parent) {
                Some(TypeKey::Item(target)) => self.item_path(*target),
                Some(TypeKey::Prim(prim)) => prim.name().to_owned(),
                None => "?".to_owned(),
            },
        };
        format!("{owner}::{name}")
    }
}
