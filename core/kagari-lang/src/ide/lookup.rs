//! Reference sites: every place in a file that names something (a resolved path or path
//! segment, a local binding, a method call, a field, a `use` path or the name of a definition),
//! and the one under a text offset. Covers the sites `debug::resolution_dump` enumerates.

use kagari_syntax::ast::{self, AstNode, HasName};
use kagari_syntax::{SyntaxKind, SyntaxNode, TextRange, TextSize};
use rowan::NodeOrToken;

use crate::db::Db;
use crate::def_map::{Def, ModuleId, Namespace, Resolution, def_maps, resolve_module_path};
use crate::diagnostic::{Anchor, NodeId};
use crate::hir::{
    Expr, ExprStore, FieldName, Pat, PatBound, Path, PathKind, StoreSourceMap, TypeRef, body_of,
    body_source_map_of, signature_of, signature_source_map_of,
};
use crate::infer::{InferenceResult, inference};
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, Shape, item_source_map, item_tree};
use crate::parse_query::parse;
use crate::resolve::{
    BodyResolution, LocalId, StoreResolution, body_resolution, signature_resolution,
};
use crate::ty::{FnDefId, TyKind};

/// What a reference site denotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    Res(Resolution),
    /// The method a method call calls.
    Method(ItemId),
    /// The name of a definition itself.
    DefSite(Def),
    /// A named field, where it is declared or used.
    Field(FieldId),
}

/// A named field of a struct (`variant: None`) or of an enum variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FieldId {
    pub adt: ItemId,
    pub variant: Option<u32>,
    pub index: u32,
}

/// A place in a file that names something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    /// The name as written: one identifier.
    pub range: TextRange,
    pub found: Found,
    /// For a field written in shorthand (`S { x }`), where one name is both the field and the
    /// value or binding: the whole field.
    pub shorthand: Option<TextRange>,
}

impl Site {
    fn new(range: TextRange, found: Found) -> Site {
        Site {
            range,
            found,
            shorthand: None,
        }
    }
}

/// A definition, independent of where it is referred to from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Symbol {
    Item(ItemId),
    Variant {
        enum_item: ItemId,
        index: u32,
    },
    Module(ModuleId),
    /// A local of `owner`'s body.
    Local {
        owner: ItemId,
        local: LocalId,
    },
    GenericParam {
        owner: ItemId,
        index: u32,
    },
    Field(FieldId),
}

impl Symbol {
    fn of_def(def: Def) -> Symbol {
        match def {
            Def::Item(item) => Symbol::Item(item),
            Def::Variant { enum_item, index } => Symbol::Variant { enum_item, index },
            Def::Module(module) => Symbol::Module(module),
        }
    }
}

impl Found {
    /// The definition this denotes, seen from a site inside `owner`. `None` for primitive
    /// types and paths that did not resolve.
    pub(crate) fn symbol(&self, owner: ItemId) -> Option<Symbol> {
        Some(match self {
            Found::Res(Resolution::Def(def)) | Found::DefSite(def) => Symbol::of_def(*def),
            Found::Res(Resolution::Assoc(item) | Resolution::SelfType(item))
            | Found::Method(item) => Symbol::Item(*item),
            Found::Res(Resolution::Local(local)) => Symbol::Local {
                owner,
                local: *local,
            },
            Found::Res(Resolution::GenericParam { owner, index }) => Symbol::GenericParam {
                owner: *owner,
                index: *index,
            },
            Found::Field(field) => Symbol::Field(*field),
            Found::Res(Resolution::Prim(_) | Resolution::Deferred { .. } | Resolution::Error) => {
                return None;
            }
        })
    }
}

/// The reference under `offset`: the item it is in, the range to highlight and what it
/// denotes.
pub(crate) fn reference_at(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Option<(ItemId, TextRange, Found)> {
    if let Some((item, site)) = def_sites(db, file)
        .into_iter()
        .find(|(_, site)| site.range.contains_inclusive(offset))
    {
        return Some((item, site.range, site.found));
    }
    let owner = item_at(db, file, offset)?;
    let site = smallest_at(item_sites(db, program, owner), offset)?;
    Some((owner, site.range, site.found))
}

/// The smallest item whose syntax contains `offset`.
pub(crate) fn item_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<ItemId> {
    let tree = item_tree(db, file);
    let map = item_source_map(db, file);
    let mut owner: Option<(ItemId, TextRange)> = None;
    for index in tree.indices() {
        let Some(src) = map.items.get(index.idx()) else {
            continue;
        };
        let range = src.ptr.text_range();
        if range.contains_inclusive(offset)
            && owner.is_none_or(|(_, best)| range.len() < best.len())
        {
            owner = Some((ItemId { file, index }, range));
        }
    }
    owner.map(|(item, _)| item)
}

/// Among the sites containing `offset` (inclusive at both ends), the smallest; the first of
/// equal ones.
fn smallest_at(sites: Vec<Site>, offset: TextSize) -> Option<Site> {
    let mut best: Option<Site> = None;
    for site in sites {
        if site.range.contains_inclusive(offset)
            && best
                .as_ref()
                .is_none_or(|best| site.range.len() < best.range.len())
        {
            best = Some(site);
        }
    }
    best
}

/// The definition names of a file: items, enum variants and named fields, each with the item
/// it belongs to.
pub(crate) fn def_sites(db: &dyn Db, file: SourceFile) -> Vec<(ItemId, Site)> {
    let tree = item_tree(db, file);
    let map = item_source_map(db, file);
    let root = parse(db, file).syntax_node();
    let mut out = Vec::new();
    for index in tree.indices() {
        let (Some(item), Some(src)) = (tree.item(index), map.items.get(index.idx())) else {
            continue;
        };
        let id = ItemId { file, index };
        if item.name.is_some() {
            out.push((id, Site::new(src.name, Found::DefSite(Def::Item(id)))));
        }
        if matches!(item.kind, ItemKind::Enum { .. }) {
            for (variant, range) in src.variants.iter().enumerate() {
                let Ok(index) = u32::try_from(variant) else {
                    continue;
                };
                let def = Def::Variant {
                    enum_item: id,
                    index,
                };
                out.push((id, Site::new(*range, Found::DefSite(def))));
            }
        }
        if matches!(item.kind, ItemKind::Struct { .. } | ItemKind::Enum { .. }) {
            let node = src.ptr.to_node(&root);
            for (field, range) in field_decls(&node, &src.variants) {
                let found = Found::Field(FieldId {
                    adt: id,
                    variant: field.0,
                    index: field.1,
                });
                out.push((id, Site::new(range, found)));
            }
        }
    }
    out
}

/// The named fields declared by a `STRUCT` or `ENUM` node: `((variant, index), name range)`.
/// `variant_names` are the item source map's variant name ranges.
pub(crate) fn field_decls(
    node: &SyntaxNode,
    variant_names: &[TextRange],
) -> Vec<((Option<u32>, u32), TextRange)> {
    let mut out = Vec::new();
    // Indexed like `Shape::Record`, which skips fields without a name.
    let mut push_fields = |variant: Option<u32>, list: Option<ast::RecordFieldList>| {
        let names = list
            .iter()
            .flat_map(|list| list.fields())
            .filter_map(|field| field.name());
        for (index, name) in names.enumerate() {
            if let Ok(index) = u32::try_from(index) {
                out.push(((variant, index), name.syntax().text_range()));
            }
        }
    };
    if let Some(item) = ast::Struct::cast(node.clone()) {
        push_fields(None, item.record_field_list());
    } else if let Some(item) = ast::Enum::cast(node.clone()) {
        for variant in item.variant_list().iter().flat_map(|list| list.variants()) {
            let Some(name) = variant.name() else {
                continue;
            };
            let name = name.syntax().text_range();
            let Some(index) = variant_names.iter().position(|range| *range == name) else {
                continue;
            };
            if let Ok(index) = u32::try_from(index) {
                push_fields(Some(index), variant.record_field_list());
            }
        }
    }
    out
}

/// Every reference site inside `item` (not in items nested in it): paths and path segments of
/// its signature and body, bindings, method calls, fields and, for a `use` item, its paths.
pub(crate) fn item_sites(db: &dyn Db, program: Program, item: ItemId) -> Vec<Site> {
    let root = parse(db, item.file).syntax_node();
    let mut out: Vec<Site> = Vec::new();
    if let (Some(signature), Some(map)) =
        (signature_of(db, item), signature_source_map_of(db, item))
    {
        let resolved = &signature_resolution(db, program, item).store;
        store_sites(&root, &signature.store, map, resolved, None, &mut out);
        for (index, range) in map.generic_params.iter().enumerate() {
            if let Ok(index) = u32::try_from(index) {
                let found = Found::Res(Resolution::GenericParam { owner: item, index });
                out.push(Site::new(*range, found));
            }
        }
    }
    if let (Some(body), Some(map), Some(resolved)) = (
        body_of(db, item),
        body_source_map_of(db, item),
        &body_resolution(db, program, item).body,
    ) {
        store_sites(
            &root,
            &body.store,
            map,
            &resolved.store,
            Some(resolved),
            &mut out,
        );
        if let Some(result) = inference(db, program, item) {
            for (&expr, call) in &result.method_calls {
                let FnDefId::Item(method) = call.func.def else {
                    continue;
                };
                let name = node_at(&root, map.expr(expr), SyntaxKind::METHOD_CALL_EXPR)
                    .and_then(|node| child_of_kind(&node, SyntaxKind::NAME_REF));
                if let Some(name) = name {
                    out.push(Site::new(name.text_range(), Found::Method(method)));
                }
            }
            field_sites(db, &root, &body.store, map, result, &mut out);
        }
    }
    import_sites(db, program, item, &mut out);
    out
}

/// The range of the last segment of `path` owned by `node`, else `fallback`.
fn last_segment(map: &StoreSourceMap, node: NodeId, path: &Path, fallback: TextRange) -> TextRange {
    path.segments
        .len()
        .checked_sub(1)
        .and_then(|last| map.segment(node, last))
        .unwrap_or(fallback)
}

fn store_sites(
    root: &SyntaxNode,
    store: &ExprStore,
    map: &StoreSourceMap,
    resolved: &StoreResolution,
    body: Option<&BodyResolution>,
    out: &mut Vec<Site>,
) {
    for (&id, resolution) in &resolved.exprs {
        let path = match store.expr(id) {
            Expr::Path(path) | Expr::Record { path, .. } => path,
            _ => continue,
        };
        if matches!(path.kind, PathKind::Lang(_)) {
            continue;
        }
        let range = last_segment(map, NodeId::Expr(id), path, map.expr(id));
        out.push(Site::new(range, Found::Res(resolution.clone())));
    }
    for (&id, resolution) in resolved.types.iter().chain(&resolved.const_args) {
        if let TypeRef::Path(path) = store.type_ref(id) {
            let range = last_segment(map, NodeId::Type(id), path, map.type_ref(id));
            out.push(Site::new(range, Found::Res(resolution.clone())));
        }
    }
    for (&id, resolution) in &resolved.pats {
        let range = match store.pat(id) {
            Pat::Path(path) | Pat::TupleStruct { path, .. } | Pat::Record { path, .. } => {
                last_segment(map, NodeId::Pat(id), path, map.pat(id))
            }
            Pat::Bind { .. } => binding_name(root, map.pat(id)),
            _ => continue,
        };
        out.push(Site::new(range, Found::Res(resolution.clone())));
    }
    for (&(id, slot), resolution) in &resolved.range_bounds {
        if let Pat::Range { start, end, .. } = store.pat(id) {
            let bound = if slot == 0 { start } else { end };
            if let Some(PatBound::Path(path)) = bound {
                let range = last_segment(map, NodeId::PatBound(id, slot), path, map.pat(id));
                out.push(Site::new(range, Found::Res(resolution.clone())));
            }
        }
    }
    for (&(node, index), resolution) in &resolved.prefixes {
        let segment = usize::try_from(index)
            .ok()
            .and_then(|index| map.segment(node, index));
        if let Some(range) = segment {
            out.push(Site::new(range, Found::Res(resolution.clone())));
        }
    }
    if let Some(body) = body {
        for (&pat, &local) in &body.bindings {
            if let Pat::Bind { .. } = store.pat(pat) {
                let range = binding_name(root, map.pat(pat));
                out.push(Site::new(range, Found::Res(Resolution::Local(local))));
            }
        }
    }
}

/// The name of the `IDENT_PAT` at `range` (`x` of `mut x`), else `range`.
pub(crate) fn binding_name(root: &SyntaxNode, range: TextRange) -> TextRange {
    node_at(root, range, SyntaxKind::IDENT_PAT)
        .and_then(|node| child_of_kind(&node, SyntaxKind::NAME))
        .map_or(range, |name| name.text_range())
}

/// Field accesses, the fields of struct expressions and of struct patterns.
fn field_sites(
    db: &dyn Db,
    root: &SyntaxNode,
    store: &ExprStore,
    map: &StoreSourceMap,
    result: &InferenceResult,
    out: &mut Vec<Site>,
) {
    let mut shorthands: Vec<(TextRange, TextRange)> = Vec::new();
    for (id, expr) in store.exprs.iter() {
        match expr {
            Expr::Field {
                base,
                name: FieldName::Named(_),
            } => {
                let Some(field) = result.fields.get(&id) else {
                    continue;
                };
                let Some(mut ty) = result.expr_types.get(*base).cloned() else {
                    continue;
                };
                for _ in 0..field.derefs {
                    let TyKind::Ref { inner, .. } = ty.kind() else {
                        break;
                    };
                    ty = inner.clone();
                }
                let TyKind::Adt { item, .. } = ty.kind() else {
                    continue;
                };
                let name = node_at(root, map.expr(id), SyntaxKind::FIELD_EXPR)
                    .and_then(|node| child_of_kind(&node, SyntaxKind::NAME_REF));
                if let Some(name) = name {
                    let found = Found::Field(FieldId {
                        adt: *item,
                        variant: None,
                        index: field.index,
                    });
                    out.push(Site::new(name.text_range(), found));
                }
            }
            Expr::Record { .. } => {
                let Some(ctor) = result.records.get(&id) else {
                    continue;
                };
                let Some(record) = node_at(root, map.expr(id), SyntaxKind::RECORD_EXPR)
                    .and_then(ast::RecordExpr::cast)
                else {
                    continue;
                };
                let fields = record.record_expr_field_list();
                for field in fields.iter().flat_map(|list| list.fields()) {
                    let Some(name) = field.name_ref() else {
                        continue;
                    };
                    let range = name.syntax().text_range();
                    let Some(index) = field_index(db, ctor.adt, ctor.variant, &name.text()) else {
                        continue;
                    };
                    let found = Found::Field(FieldId {
                        adt: ctor.adt,
                        variant: ctor.variant,
                        index,
                    });
                    out.push(Site::new(range, found));
                    if field.expr().is_none() {
                        shorthands.push((range, field.syntax().text_range()));
                    }
                }
            }
            _ => {}
        }
    }
    for (id, pat) in store.pats.iter() {
        if !matches!(pat, Pat::Record { .. }) {
            continue;
        }
        let Some(ctor) = result.pat_ctors.get(&id) else {
            continue;
        };
        let Some(record) =
            node_at(root, map.pat(id), SyntaxKind::RECORD_PAT).and_then(ast::RecordPat::cast)
        else {
            continue;
        };
        let fields = record.record_pat_field_list();
        for field in fields.iter().flat_map(|list| list.fields()) {
            let (name, range, shorthand) = match field.name_ref() {
                Some(name) => (name.text(), name.syntax().text_range(), false),
                None => match field.pat() {
                    Some(ast::Pat::IdentPat(ident)) => match ident.name() {
                        Some(name) => (name.text(), name.syntax().text_range(), true),
                        None => continue,
                    },
                    _ => continue,
                },
            };
            let Some(index) = field_index(db, ctor.adt, ctor.variant, &name) else {
                continue;
            };
            let found = Found::Field(FieldId {
                adt: ctor.adt,
                variant: ctor.variant,
                index,
            });
            out.push(Site::new(range, found));
            if shorthand {
                shorthands.push((range, field.syntax().text_range()));
            }
        }
    }
    for site in out.iter_mut() {
        if let Some((_, whole)) = shorthands.iter().find(|(name, _)| *name == site.range) {
            site.shorthand = Some(*whole);
        }
    }
}

/// The index of the named field `name` of a struct or of variant `variant` of an enum.
pub(crate) fn field_index(
    db: &dyn Db,
    adt: ItemId,
    variant: Option<u32>,
    name: &str,
) -> Option<u32> {
    let names = record_field_names(db, adt, variant)?;
    u32::try_from(names.iter().position(|field| field == name)?).ok()
}

/// The field names of a record struct or record variant.
pub(crate) fn record_field_names(
    db: &dyn Db,
    adt: ItemId,
    variant: Option<u32>,
) -> Option<&[crate::hir::Name]> {
    let shape = match (&item_tree(db, adt.file).item(adt.index)?.kind, variant) {
        (ItemKind::Struct { shape }, None) => shape,
        (ItemKind::Enum { variants }, Some(index)) => {
            &variants.get(usize::try_from(index).ok()?)?.1
        }
        _ => return None,
    };
    match shape {
        Shape::Record(names) => Some(names),
        Shape::Unit | Shape::Tuple(_) => None,
    }
}

/// The segments of the paths of a `use` item, resolved from the module it is in.
fn import_sites(db: &dyn Db, program: Program, item: ItemId, out: &mut Vec<Site>) {
    let tree = item_tree(db, item.file);
    let Some(ItemKind::Use { imports }) = tree.item(item.index).map(|data| &data.kind) else {
        return;
    };
    let Some(src) = item_source_map(db, item.file).items.get(item.index.idx()) else {
        return;
    };
    let maps = def_maps(db, program);
    if !maps.file_modules.contains_key(&item.file) {
        return;
    }
    let module = maps.module_of_item(db, item);
    let at = |_| Anchor::Text(item.file, TextRange::default());
    for (import, (_, ranges)) in imports.iter().zip(&src.imports) {
        let namespaces: &[Namespace] = if import.glob {
            &[Namespace::Types]
        } else {
            &[Namespace::Types, Namespace::Values]
        };
        for &ns in namespaces {
            let result = resolve_module_path(maps, db, module, &import.path, ns, &at);
            for (prefix, range) in result.prefixes.into_iter().zip(ranges) {
                out.push(Site::new(*range, Found::Res(prefix)));
            }
            let last = import.path.segments.len().checked_sub(1);
            if let (Some(resolved), Some(range)) =
                (result.resolved, last.and_then(|last| ranges.get(last)))
            {
                out.push(Site::new(*range, Found::Res(resolved)));
            }
        }
    }
}

/// The node of `kind` whose range is exactly `range`.
pub(crate) fn node_at(root: &SyntaxNode, range: TextRange, kind: SyntaxKind) -> Option<SyntaxNode> {
    if !root.text_range().contains_range(range) {
        return None;
    }
    let start = match root.covering_element(range) {
        NodeOrToken::Node(node) => node,
        NodeOrToken::Token(token) => token.parent()?,
    };
    start
        .ancestors()
        .find(|node| node.kind() == kind && node.text_range() == range)
}

/// The first child node of `kind`.
pub(crate) fn child_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}
