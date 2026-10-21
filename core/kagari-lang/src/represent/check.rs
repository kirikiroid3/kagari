//! Stage 3 entry points: item rules, signatures and bodies of `.kix`/`.kirp` files, inline
//! `<r>` code, `<m voice>` references, and recursion/const cycles.

use kagari_syntax::TextRange;
use kagari_syntax::represent::ast;
use kagari_syntax::script::ast::{Document, Element, decode_content};
use rowan::ast::AstNode;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Span};
use crate::input::{FilePath, Program, SourceFile};
use crate::kis::hir::{Block, ChoiceItem, Render, StmtKind};
use crate::kis::program_hir;
use crate::kis::value::check_name;
use crate::parse::parse_kis;
use crate::represent::infer::{Body, RefEdge};
use crate::represent::item_tree::{DefId, Item, ItemKind, item_tree};
use crate::represent::scope::{ValueDef, item_scope, value_location};
use crate::represent::ty::{Ty, TyCtx, TyEnv};

/// Diagnostics and reference edges of one checked file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BodyChecks {
    pub diagnostics: Vec<Diagnostic>,
    /// Calls to non-builtin fns, from the calling fn.
    pub calls: Vec<RefEdge>,
    /// Paths naming consts, from the referring fn or const.
    pub const_refs: Vec<RefEdge>,
}

impl BodyChecks {
    fn absorb(&mut self, body: Body<'_, '_>) {
        let (diagnostics, calls, const_refs) = body.finish();
        self.diagnostics.extend(diagnostics);
        self.calls.extend(calls);
        self.const_refs.extend(const_refs);
    }

    fn error(
        &mut self,
        db: &dyn Db,
        file: SourceFile,
        range: TextRange,
        code: DiagnosticCode,
        message: String,
    ) {
        self.diagnostics
            .push(Diagnostic::new(file.path(db).clone(), range, code, message));
    }
}

#[salsa::tracked(returns(ref))]
pub fn check_kix_file(db: &dyn Db, program: Program, file: SourceFile) -> BodyChecks {
    let tcx = TyCtx::new(db, item_scope(db, program));
    let tree = item_tree(db, file);
    let is_std = matches!(file.path(db), FilePath::Std(_));
    let no_kirp_fns = FxHashMap::default();
    let mut out = BodyChecks::default();
    for (index, item) in (0u32..).zip(&tree.items) {
        let def = DefId { file, index };
        let parent_kind = item
            .parent
            .and_then(|parent| tree.items.get(parent as usize))
            .map(|parent| &parent.kind);
        if matches!(parent_kind, Some(ItemKind::Impl { .. }))
            && !matches!(item.kind, ItemKind::Fn { .. } | ItemKind::Const)
        {
            let message = format!(
                "{} items are not allowed inside an impl",
                item.kind.describe()
            );
            out.error(
                db,
                file,
                item.name_range,
                DiagnosticCode::KixItemNotAllowed,
                message,
            );
            continue;
        }
        if item.parent.is_none()
            && item.name == "render"
            && matches!(item.kind, ItemKind::Fn { .. })
        {
            let message = "`render` belongs in a `.kirp` file, not in a `.kix` file".to_owned();
            out.error(
                db,
                file,
                item.name_range,
                DiagnosticCode::KixItemNotAllowed,
                message,
            );
            continue;
        }
        check_attrs(db, file, item, &mut out);
        check_fn_body_rules(db, file, item, is_std, &mut out);
        lower_item_types(&tcx, def, item, &mut out);
        check_bodies(&tcx, def, &no_kirp_fns, &mut out);
    }
    out
}

#[salsa::tracked(returns(ref))]
pub fn check_kirp_file(db: &dyn Db, program: Program, file: SourceFile) -> BodyChecks {
    let scope = item_scope(db, program);
    let tcx = TyCtx::new(db, scope);
    let tree = item_tree(db, file);
    let mut out = BodyChecks::default();
    let mut kirp_fns: FxHashMap<SmolStr, DefId> = FxHashMap::default();
    let mut fns: Vec<DefId> = Vec::new();
    let mut renders: Vec<DefId> = Vec::new();
    for &index in &tree.top {
        let def = DefId { file, index };
        let Some(item) = tree.items.get(index as usize) else {
            continue;
        };
        if !matches!(item.kind, ItemKind::Fn { .. }) {
            let message = format!(
                "{} items are not allowed in a `.kirp` file; only functions are",
                item.kind.describe()
            );
            out.error(
                db,
                file,
                item.name_range,
                DiagnosticCode::KixItemNotAllowed,
                message,
            );
            continue;
        }
        fns.push(def);
        if item.name == "render" {
            if !renders.is_empty() {
                let message = "`render` is defined more than once".to_owned();
                out.error(
                    db,
                    file,
                    item.name_range,
                    DiagnosticCode::KirpDuplicateRender,
                    message,
                );
            }
            renders.push(def);
            continue;
        }
        let first = kirp_fns
            .get(&item.name)
            .map(|first| ValueDef::Item(*first))
            .or_else(|| scope.root.values.get(&item.name).copied());
        match first {
            Some(first) => {
                let mut diag = Diagnostic::new(
                    file.path(db).clone(),
                    item.name_range,
                    DiagnosticCode::KixDuplicateDefinition,
                    format!("`{}` is already defined", item.name),
                );
                if let Some((first_file, range)) = value_location(db, first) {
                    diag =
                        diag.with_related(first_file.path(db).clone(), range, "first defined here");
                }
                out.diagnostics.push(diag);
            }
            None => {
                kirp_fns.insert(item.name.clone(), def);
            }
        }
    }
    match renders.first() {
        None => {
            let message =
                "a `.kirp` file must define `fn render(screen: Screen) -> Screen`".to_owned();
            out.error(
                db,
                file,
                TextRange::default(),
                DiagnosticCode::KirpMissingRender,
                message,
            );
        }
        Some(&render) => check_render_signature(&tcx, render, &mut out),
    }
    for def in fns {
        let Some(item) = tree.items.get(def.index as usize) else {
            continue;
        };
        check_attrs(db, file, item, &mut out);
        check_fn_body_rules(db, file, item, false, &mut out);
        lower_item_types(&tcx, def, item, &mut out);
        check_bodies(&tcx, def, &kirp_fns, &mut out);
    }
    out
}

fn check_render_signature(tcx: &TyCtx<'_>, render: DefId, out: &mut BodyChecks) {
    let Some(item) = tcx.item(render) else {
        return;
    };
    let screen = tcx.prelude_ty(|defs| defs.screen);
    if screen == Ty::Error {
        return;
    }
    let sig = tcx.fn_sig(render);
    let ok = !sig.has_self
        && item.kind.generics().is_empty()
        && sig.params.len() == 1
        && sig
            .params
            .first()
            .is_some_and(|param| *param == screen || *param == Ty::Error)
        && (sig.ret == screen || sig.ret == Ty::Error);
    if !ok {
        let message =
            "`render` must have the signature `fn render(screen: Screen) -> Screen`".to_owned();
        out.error(
            tcx.db,
            render.file,
            item.name_range,
            DiagnosticCode::KirpBadRenderSignature,
            message,
        );
    }
}

/// `#[builtin]` only on fns, `#[must_use]` only on structs/enums, neither with arguments.
fn check_attrs(db: &dyn Db, file: SourceFile, item: &Item, out: &mut BodyChecks) {
    for (name, range, has_args) in &item.attrs {
        let valid_on = match name.as_str() {
            "builtin" => matches!(item.kind, ItemKind::Fn { .. }),
            "must_use" => matches!(item.kind, ItemKind::Struct { .. } | ItemKind::Enum { .. }),
            _ => {
                let message = format!("unknown attribute `{name}`");
                out.error(
                    db,
                    file,
                    *range,
                    DiagnosticCode::KixUnknownAttribute,
                    message,
                );
                continue;
            }
        };
        if !valid_on {
            let message = format!("`#[{name}]` is not allowed on a {}", item.kind.describe());
            out.error(db, file, *range, DiagnosticCode::KixBadAttribute, message);
        } else if *has_args {
            let message = format!("`#[{name}]` takes no arguments");
            out.error(db, file, *range, DiagnosticCode::KixBadAttribute, message);
        }
    }
}

/// Body presence rules for fns.
fn check_fn_body_rules(
    db: &dyn Db,
    file: SourceFile,
    item: &Item,
    is_std: bool,
    out: &mut BodyChecks,
) {
    let ItemKind::Fn { has_body, .. } = item.kind else {
        return;
    };
    let builtin = item.attrs.iter().find(|(name, _, _)| name == "builtin");
    match builtin {
        Some((_, range, _)) => {
            if !is_std {
                let message = "`#[builtin]` is only allowed in bundled std files".to_owned();
                out.error(
                    db,
                    file,
                    *range,
                    DiagnosticCode::KixBuiltinOutsideStd,
                    message,
                );
            } else if has_body {
                let message = format!("builtin fn `{}` must not have a body", item.name);
                out.error(
                    db,
                    file,
                    item.name_range,
                    DiagnosticCode::KixBuiltinWithBody,
                    message,
                );
            }
        }
        None if !has_body => {
            let message = format!("fn `{}` has no body", item.name);
            out.error(
                db,
                file,
                item.name_range,
                DiagnosticCode::KixMissingBody,
                message,
            );
        }
        None => {}
    }
}

/// Lower every type written in an item's signature, reporting resolution errors.
fn lower_item_types(tcx: &TyCtx<'_>, def: DefId, item: &Item, out: &mut BodyChecks) {
    let env = tcx.env_for(def);
    let sink = &mut out.diagnostics;
    match &item.kind {
        ItemKind::Struct { .. } => {
            if let Some(list) = tcx
                .node::<ast::Struct>(def)
                .and_then(|s| s.record_field_list())
            {
                for field in list.fields() {
                    tcx.lower_opt(&env, field.ty(), sink);
                }
            }
        }
        ItemKind::Enum { .. } => {
            if let Some(enm) = tcx.node::<ast::Enum>(def) {
                for list in enm
                    .variants()
                    .filter_map(|variant| variant.tuple_field_list())
                {
                    for ty in list.types() {
                        tcx.lower_type(&env, &ty, sink);
                    }
                }
            }
        }
        ItemKind::TypeAlias { .. } => {
            if tcx.alias_is_cyclic(def) {
                sink.push(Diagnostic::new(
                    def.file.path(tcx.db).clone(),
                    item.name_range,
                    DiagnosticCode::KixAliasCycle,
                    format!("type alias `{}` refers to itself", item.name),
                ));
            } else if let Some(alias) = tcx.node::<ast::TypeAlias>(def) {
                tcx.lower_opt(&env, alias.ty(), sink);
            }
        }
        ItemKind::Const => {
            if let Some(konst) = tcx.node::<ast::Const>(def) {
                tcx.lower_opt(&env, konst.ty(), sink);
            }
        }
        ItemKind::Fn { .. } => {
            let Some(function) = tcx.node::<ast::Fn>(def) else {
                return;
            };
            if let Some(list) = function.param_list() {
                for param in list.params() {
                    tcx.lower_opt(&env, param.ty(), sink);
                }
            }
            if let Some(ret) = function.ret_type() {
                tcx.lower_opt(&env, ret.ty(), sink);
            }
        }
        ItemKind::AssetImport { path } => {
            let Some(import) = tcx.node::<ast::AssetImport>(def) else {
                return;
            };
            if path.is_empty() && import.path_token().is_some() {
                let range = import
                    .path_token()
                    .map(|token| token.text_range())
                    .unwrap_or(item.name_range);
                sink.push(Diagnostic::new(
                    def.file.path(tcx.db).clone(),
                    range,
                    DiagnosticCode::KixBadAssetPath,
                    "the asset path is empty".to_owned(),
                ));
            }
            let Some(written) = import.ty() else {
                return;
            };
            let ty = tcx.lower_type(&env, &written, sink);
            let Some(defs) = tcx.scope.prelude else {
                return;
            };
            let is_asset = matches!(&ty, Ty::Error)
                || matches!(&ty, Ty::Adt { def, args } if args.is_empty()
                    && [defs.image_asset, defs.audio_asset, defs.voice_asset].contains(def));
            if !is_asset {
                sink.push(Diagnostic::new(
                    def.file.path(tcx.db).clone(),
                    written.syntax().text_range(),
                    DiagnosticCode::KixBadAssetType,
                    "an asset import must be `as ImageAsset`, `as AudioAsset` or `as VoiceAsset`"
                        .to_owned(),
                ));
            }
        }
        ItemKind::Mod { .. } | ItemKind::Impl { .. } => {}
    }
}

/// Check the body of a fn or the initializer of a const.
fn check_bodies(
    tcx: &TyCtx<'_>,
    def: DefId,
    kirp_fns: &FxHashMap<SmolStr, DefId>,
    out: &mut BodyChecks,
) {
    match tcx.item(def).map(|item| &item.kind) {
        Some(ItemKind::Fn { has_body: true, .. }) => {
            if let Some(function) = tcx.node::<ast::Fn>(def) {
                let mut body = Body::new(tcx, tcx.env_for(def), Some(def), kirp_fns, None);
                body.check_fn(def, &function);
                out.absorb(body);
            }
        }
        Some(ItemKind::Const) => {
            if let Some(konst) = tcx.node::<ast::Const>(def) {
                let mut body = Body::new(tcx, tcx.env_for(def), Some(def), kirp_fns, None);
                body.check_const(def, &konst);
                out.absorb(body);
            }
        }
        _ => {}
    }
}

#[salsa::tracked(returns(ref))]
pub fn check_inline_renders(db: &dyn Db, program: Program, file: SourceFile) -> BodyChecks {
    let mut out = BodyChecks::default();
    let Some(root) = Document::cast(parse_kis(db, file).syntax_node()).and_then(|doc| doc.root())
    else {
        return out;
    };
    let game_root: Option<Element> = if file == *program.entry(db) {
        if root.is("kagari") {
            root.child_elements().find(|child| child.is("game"))
        } else {
            None
        }
    } else if root.is("kagari-game") {
        Some(root)
    } else {
        None
    };
    let Some(game_root) = game_root else {
        return out;
    };
    let tcx = TyCtx::new(db, item_scope(db, program));
    let screen = tcx.prelude_ty(|defs| defs.screen);
    let no_kirp_fns = FxHashMap::default();
    let path = file.path(db);
    for element in game_root.syntax().descendants().filter_map(Element::cast) {
        if !element.is("r") {
            continue;
        }
        let decoded = decode_content(element.content());
        let parse = kagari_syntax::represent::parse_inline(&decoded.text);
        for error in parse.errors() {
            out.diagnostics.push(Diagnostic::new(
                path.clone(),
                decoded.map.range_to_source(error.range),
                DiagnosticCode::Syntax,
                error.message.clone(),
            ));
        }
        let Some(inline) = ast::InlineRender::cast(parse.syntax_node()) else {
            continue;
        };
        let mut body = Body::new(
            &tcx,
            TyEnv::bare(file),
            None,
            &no_kirp_fns,
            Some(&decoded.map),
        );
        body.bind(SmolStr::new("screen"), screen.clone());
        body.check_stmts(inline.stmts());
        let last = body.local("screen").unwrap_or(Ty::Error);
        let range = element.name_range().unwrap_or(element.start_tag_range());
        body.expect_ty(&last, &screen, range, true);
        out.absorb(body);
    }
    out
}

#[salsa::tracked(returns(ref))]
pub fn check_voices(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let hir = program_hir(db, program);
    let tcx = TyCtx::new(db, item_scope(db, program));
    let mut voices: Vec<(String, Span)> = Vec::new();
    for (_, scene) in hir.scenes.iter() {
        collect_voices(&scene.body, &mut voices);
    }
    let mut seen: FxHashSet<Span> = FxHashSet::default();
    let mut out = Vec::new();
    for (text, span) in voices {
        if !seen.insert(span) {
            continue;
        }
        if !is_voice(&tcx, &text) {
            out.push(span.diagnostic(
                db,
                DiagnosticCode::KisBadVoice,
                format!("`{text}` is not an asset import or constant of type `VoiceAsset`"),
            ));
        }
    }
    out
}

fn collect_voices(block: &Block, out: &mut Vec<(String, Span)>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Say { messages, .. } => {
                for message in messages {
                    if let Some((text, span)) = &message.voice {
                        out.push((text.clone(), *span));
                    }
                }
            }
            StmtKind::Choice { items, .. } => collect_choice_voices(items, out),
            StmtKind::Match(matched) => {
                for arm in &matched.arms {
                    collect_voices(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_choice_voices(items: &[ChoiceItem], out: &mut Vec<(String, Span)>) {
    for item in items {
        match item {
            ChoiceItem::Arm(arm) => collect_voices(&arm.body, out),
            ChoiceItem::Match(matched) => {
                for arm in &matched.arms {
                    collect_choice_voices(&arm.body, out);
                }
            }
        }
    }
}

/// Whether `text` (`a::b::NAME`) names a `VoiceAsset` asset import or const.
fn is_voice(tcx: &TyCtx<'_>, text: &str) -> bool {
    let segments: Vec<&str> = text.split("::").collect();
    if !segments.iter().all(|segment| check_name(segment)) {
        return false;
    }
    let Some((last, prefix)) = segments.split_last() else {
        return false;
    };
    let mut module: Option<DefId> = None;
    for segment in prefix {
        let Some(&def) = tcx
            .scope
            .module(module)
            .and_then(|scope| scope.types.get(*segment))
        else {
            return false;
        };
        if !matches!(
            tcx.item(def).map(|item| &item.kind),
            Some(ItemKind::Mod { .. })
        ) {
            return false;
        }
        module = Some(def);
    }
    let Some(&ValueDef::Item(def)) = tcx
        .scope
        .module(module)
        .and_then(|scope| scope.values.get(*last))
    else {
        return false;
    };
    if !matches!(
        tcx.item(def).map(|item| &item.kind),
        Some(ItemKind::Const | ItemKind::AssetImport { .. })
    ) {
        return false;
    }
    let voice = tcx.prelude_ty(|defs| defs.voice_asset);
    voice != Ty::Error && tcx.value_ty(def) == voice
}

#[salsa::tracked(returns(ref))]
pub fn check_cycles(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let collected = crate::collect::collect(db, program);
    let hir = program_hir(db, program);
    let mut checks: Vec<&BodyChecks> = collected
        .kix_files
        .iter()
        .map(|&file| check_kix_file(db, program, file))
        .collect();
    let mut kirp_seen = FxHashSet::default();
    for (_, render) in hir.renders.iter() {
        if let Render::File { kirp, .. } = render
            && kirp_seen.insert(*kirp)
        {
            checks.push(check_kirp_file(db, program, *kirp));
        }
    }
    let calls: Vec<&RefEdge> = checks.iter().flat_map(|checks| &checks.calls).collect();
    let const_refs: Vec<&RefEdge> = checks
        .iter()
        .flat_map(|checks| &checks.const_refs)
        .collect();
    let mut out = cycles(
        db,
        &calls,
        DiagnosticCode::KixRecursion,
        "recursive functions",
    );
    out.extend(cycles(
        db,
        &const_refs,
        DiagnosticCode::KixConstCycle,
        "cyclic constants",
    ));
    out
}

/// One diagnostic per cyclic strongly connected component of the edge graph, at its smallest
/// edge by `(file path, start)`.
fn cycles(db: &dyn Db, edges: &[&RefEdge], code: DiagnosticCode, what: &str) -> Vec<Diagnostic> {
    let mut nodes: Vec<DefId> = Vec::new();
    let mut index_of: FxHashMap<DefId, usize> = FxHashMap::default();
    for edge in edges {
        for def in [edge.from, edge.to] {
            index_of.entry(def).or_insert_with(|| {
                nodes.push(def);
                nodes.len().saturating_sub(1)
            });
        }
    }
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for edge in edges {
        if let (Some(&from), Some(&to)) = (index_of.get(&edge.from), index_of.get(&edge.to))
            && let Some(list) = adjacency.get_mut(from)
        {
            list.push(to);
        }
    }
    let components = tarjan(&adjacency);
    let mut out = Vec::new();
    for component in components {
        let members: FxHashSet<usize> = component.iter().copied().collect();
        let inside: Vec<&&RefEdge> = edges
            .iter()
            .filter(|edge| {
                index_of
                    .get(&edge.from)
                    .is_some_and(|from| members.contains(from))
                    && index_of
                        .get(&edge.to)
                        .is_some_and(|to| members.contains(to))
            })
            .collect();
        let Some(first) = inside.iter().min_by(|a, b| {
            (a.span.file.path(db), a.span.range.start())
                .cmp(&(b.span.file.path(db), b.span.range.start()))
        }) else {
            continue;
        };
        let mut names: Vec<String> = component
            .iter()
            .filter_map(|index| nodes.get(*index))
            .filter_map(|def| item_tree(db, def.file).items.get(def.index as usize))
            .map(|item| format!("`{}`", item.name))
            .collect();
        names.sort();
        names.dedup();
        out.push(
            first
                .span
                .diagnostic(db, code, format!("{what}: {}", names.join(", "))),
        );
    }
    out
}

/// Strongly connected components that contain a cycle (size > 1, or a self-edge).
pub(crate) fn tarjan(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct State<'a> {
        adjacency: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }

    fn visit(state: &mut State<'_>, node: usize) {
        let number = state.next;
        state.next = state.next.saturating_add(1);
        if let Some(slot) = state.index.get_mut(node) {
            *slot = Some(number);
        }
        if let Some(slot) = state.lowlink.get_mut(node) {
            *slot = number;
        }
        state.stack.push(node);
        if let Some(flag) = state.on_stack.get_mut(node) {
            *flag = true;
        }
        let successors = state.adjacency.get(node).cloned().unwrap_or_default();
        for next in successors {
            match state.index.get(next).copied().flatten() {
                None => {
                    visit(state, next);
                    let low = state.lowlink.get(next).copied().unwrap_or(usize::MAX);
                    if let Some(slot) = state.lowlink.get_mut(node) {
                        *slot = (*slot).min(low);
                    }
                }
                Some(next_index) if state.on_stack.get(next).copied().unwrap_or(false) => {
                    if let Some(slot) = state.lowlink.get_mut(node) {
                        *slot = (*slot).min(next_index);
                    }
                }
                Some(_) => {}
            }
        }
        if state.lowlink.get(node).copied() == Some(number) {
            let mut component = Vec::new();
            while let Some(member) = state.stack.pop() {
                if let Some(flag) = state.on_stack.get_mut(member) {
                    *flag = false;
                }
                component.push(member);
                if member == node {
                    break;
                }
            }
            let self_edge = state
                .adjacency
                .get(node)
                .is_some_and(|successors| successors.contains(&node));
            if component.len() > 1 || self_edge {
                component.sort_unstable();
                state.out.push(component);
            }
        }
    }

    let count = adjacency.len();
    let mut state = State {
        adjacency,
        index: vec![None; count],
        lowlink: vec![0; count],
        on_stack: vec![false; count],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for node in 0..count {
        if state.index.get(node).copied().flatten().is_none() {
            visit(&mut state, node);
        }
    }
    state.out
}
