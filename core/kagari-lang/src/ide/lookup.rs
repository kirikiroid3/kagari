//! From a text offset to what is there: a resolved path, a local binding, a method call or
//! the name of a definition. Mirrors the sites `debug::resolution_dump` enumerates.

use kagari_syntax::{SyntaxKind, SyntaxNode, TextRange, TextSize};
use rowan::NodeOrToken;

use crate::db::Db;
use crate::def_map::{Def, Resolution};
use crate::diagnostic::NodeId;
use crate::hir::{
    Expr, ExprStore, Pat, PatBound, Path, PathKind, StoreSourceMap, TypeRef, body_of,
    body_source_map_of, signature_of, signature_source_map_of,
};
use crate::infer::inference;
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, item_source_map, item_tree};
use crate::parse_query::parse;
use crate::resolve::{BodyResolution, StoreResolution, body_resolution, signature_resolution};
use crate::ty::FnDefId;

/// What a reference site denotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    Res(Resolution),
    /// The method a method call calls.
    Method(ItemId),
    /// The name of a definition itself.
    DefSite(Def),
}

/// The reference under `offset`: the item it is in, the range to highlight and what it
/// denotes.
pub(crate) fn reference_at(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Option<(ItemId, TextRange, Found)> {
    let tree = item_tree(db, file);
    let map = item_source_map(db, file);
    let mut owner: Option<(ItemId, TextRange)> = None;
    for index in tree.indices() {
        let (Some(item), Some(src)) = (tree.item(index), map.items.get(index.idx())) else {
            continue;
        };
        let id = ItemId { file, index };
        if item.name.is_some() && src.name.contains_inclusive(offset) {
            return Some((id, src.name, Found::DefSite(Def::Item(id))));
        }
        if matches!(item.kind, ItemKind::Enum { .. }) {
            for (variant, range) in src.variants.iter().enumerate() {
                if range.contains_inclusive(offset) {
                    let index = u32::try_from(variant).ok()?;
                    let def = Def::Variant {
                        enum_item: id,
                        index,
                    };
                    return Some((id, *range, Found::DefSite(def)));
                }
            }
        }
        let range = src.ptr.text_range();
        if range.contains_inclusive(offset)
            && owner.is_none_or(|(_, best)| range.len() < best.len())
        {
            owner = Some((id, range));
        }
    }
    let (owner, _) = owner?;

    let mut candidates: Vec<(TextRange, Found)> = Vec::new();
    if let (Some(signature), Some(map)) =
        (signature_of(db, owner), signature_source_map_of(db, owner))
    {
        let resolved = &signature_resolution(db, program, owner).store;
        store_candidates(&signature.store, map, resolved, None, &mut candidates);
    }
    if let (Some(body), Some(map), Some(resolved)) = (
        body_of(db, owner),
        body_source_map_of(db, owner),
        &body_resolution(db, program, owner).body,
    ) {
        store_candidates(
            &body.store,
            map,
            &resolved.store,
            Some(resolved),
            &mut candidates,
        );
        if let Some(result) = inference(db, program, owner) {
            let root = parse(db, file).syntax_node();
            for (&expr, call) in &result.method_calls {
                let FnDefId::Item(method) = call.func.def else {
                    continue;
                };
                if let Some(range) = method_name_range(&root, map.expr(expr)) {
                    candidates.push((range, Found::Method(method)));
                }
            }
        }
    }

    let mut best: Option<(TextRange, Found)> = None;
    for (range, found) in candidates {
        if range.contains_inclusive(offset)
            && best
                .as_ref()
                .is_none_or(|(best, _)| range.len() < best.len())
        {
            best = Some((range, found));
        }
    }
    best.map(|(range, found)| (owner, range, found))
}

/// The range of the last segment of `path` owned by `node`, else `fallback`.
fn last_segment(map: &StoreSourceMap, node: NodeId, path: &Path, fallback: TextRange) -> TextRange {
    path.segments
        .len()
        .checked_sub(1)
        .and_then(|last| map.segment(node, last))
        .unwrap_or(fallback)
}

fn store_candidates(
    store: &ExprStore,
    map: &StoreSourceMap,
    resolved: &StoreResolution,
    body: Option<&BodyResolution>,
    out: &mut Vec<(TextRange, Found)>,
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
        out.push((range, Found::Res(resolution.clone())));
    }
    for (&id, resolution) in resolved.types.iter().chain(&resolved.const_args) {
        if let TypeRef::Path(path) = store.type_ref(id) {
            let range = last_segment(map, NodeId::Type(id), path, map.type_ref(id));
            out.push((range, Found::Res(resolution.clone())));
        }
    }
    for (&id, resolution) in &resolved.pats {
        let range = match store.pat(id) {
            Pat::Path(path) | Pat::TupleStruct { path, .. } | Pat::Record { path, .. } => {
                last_segment(map, NodeId::Pat(id), path, map.pat(id))
            }
            Pat::Bind { .. } => map.pat(id),
            _ => continue,
        };
        out.push((range, Found::Res(resolution.clone())));
    }
    for (&(id, slot), resolution) in &resolved.range_bounds {
        if let Pat::Range { start, end, .. } = store.pat(id) {
            let bound = if slot == 0 { start } else { end };
            if let Some(PatBound::Path(path)) = bound {
                let range = last_segment(map, NodeId::PatBound(id, slot), path, map.pat(id));
                out.push((range, Found::Res(resolution.clone())));
            }
        }
    }
    if let Some(body) = body {
        for (&pat, &local) in &body.bindings {
            if let Pat::Bind { .. } = store.pat(pat) {
                out.push((map.pat(pat), Found::Res(Resolution::Local(local))));
            }
        }
    }
}

/// The method name of the `METHOD_CALL_EXPR` whose range is `range`.
fn method_name_range(root: &SyntaxNode, range: TextRange) -> Option<TextRange> {
    if !root.text_range().contains_range(range) {
        return None;
    }
    let start = match root.covering_element(range) {
        NodeOrToken::Node(node) => node,
        NodeOrToken::Token(token) => token.parent()?,
    };
    let call = start
        .ancestors()
        .find(|node| node.kind() == SyntaxKind::METHOD_CALL_EXPR && node.text_range() == range)?;
    call.children()
        .find(|child| child.kind() == SyntaxKind::NAME_REF)
        .map(|name| name.text_range())
}
