//! The kind and story rules of compilation stage 3 (`docs/syntax/src/kinds.md`,
//! `values.md`, `narrative/*.md`, std book chapter *Checks*), on top of inference:
//!
//! - the call rules between `fn`, `pushdown fn` and `const fn`, for direct calls, methods
//!   (std builtins included) and calls through pointers and closures, whose types carry the
//!   kind;
//! - scenes: only a `#[scene]` runs steps (`push_step`) and calls scenes; instant
//!   procedures (other `pushdown fn` and closures in the story) have no `loop`/`while` and
//!   do not recurse (a program-wide cycle check over direct calls);
//! - `for` in the story over arrays and constant ranges only;
//! - story variables only in `pushdown` code, with finite types and constant initializers;
//!   handles (`Setting<T>`, `Meta<T>`) as the only immutable statics, constant
//!   initializers, no references in item types, no `Meta` in `const` code;
//! - finite types for everything the story holds at runtime (constant expressions are
//!   folded and exempt), and entry scenes without `Text`;
//! - second-class references and closures in the story, and exclusive `&mut` arguments;
//! - mutability of assigned and mutably borrowed places, in all code;
//! - placement of `#[scene]`, `#[entry]` and `#[must_use]`;
//! - `choice!` ids and arms, mono ruby readings, constant text effects;
//! - unused `#[must_use]` values (a warning).
//!
//! The context kind of code is the kind of its `fn`; a closure body has the kind its
//! inference recorded (`ClosureInfo::kind`); `const` and `static` initializers are `const`.
//! Only a `#[scene]` `pushdown fn` body is a scene; closures never are. Expressions whose
//! type is erroneous are skipped, so nothing cascades from inference errors.

mod body;
mod finite;
mod forms;
mod places;

use kagari_syntax::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::eval::const_exprs;
use crate::hir::{Signature, SignatureKind, file_hir};
use crate::infer::inference;
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{FnKind, Item, ItemId, ItemKind, item_tree};
use crate::resolve::file_resolution;
use crate::signatures::{ItemSig, ProgramSignatures, assoc_parent, display_ty, program_signatures};
use crate::ty::Ty;

/// The kind checks of one file, plus what the program-wide recursion check of instant
/// procedures needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileKindChecks {
    pub diagnostics: Vec<Diagnostic>,
    /// Edges from the body of an instant procedure: direct calls of instant procedures and
    /// instant procedures used as values (which the callee may call).
    pub instant_calls: Vec<InstantCall>,
    /// Calls through a `pushdown fn` pointer or a `pushdown` closure value in the body of an
    /// instant procedure: the caller and the call.
    pub indirect_calls: Vec<(ItemId, TextRange)>,
    /// Instant procedures used as values anywhere, and whether the value may escape: be
    /// stored, returned or folded into a constant, rather than passed straight to a
    /// function of the story as an argument.
    pub instant_values: Vec<(ItemId, bool)>,
    /// Some code stores a runtime `pushdown fn` pointer (not a constant) into a place, so
    /// any instant procedure passed around may come back out of storage.
    pub stores_pointers: bool,
}

/// An edge of the call graph of instant procedures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstantCall {
    pub caller: ItemId,
    pub callee: ItemId,
    pub range: TextRange,
    pub kind: EdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// `callee(…)` or `x.callee(…)`.
    Direct,
    /// `callee` used as a value (a pointer or a closure) in the caller's body.
    Value,
    /// A call through a pointer or closure value that may hold `callee`.
    Indirect,
}

#[salsa::tracked(returns(ref))]
pub fn file_kind_checks(db: &dyn Db, program: Program, file: SourceFile) -> FileKindChecks {
    let mut out = FileKindChecks::default();
    if !def_maps(db, program).file_modules.contains_key(&file) {
        return out;
    }
    let env = Env::new(db, program);
    let tree = item_tree(db, file);
    let hir = file_hir(db, file);
    let resolution = file_resolution(db, program, file);
    let path = file.path(db);
    for (index, data) in tree.items.iter().enumerate() {
        let Ok(index_u32) = u32::try_from(index) else {
            continue;
        };
        let item = ItemId {
            file,
            index: index_u32,
        };
        env.check_attributes(path, data, &mut out.diagnostics);
        if let Some(Some(signature)) = hir.signatures.get(index) {
            env.check_item_types(item, path, data, signature, &mut out.diagnostics);
        }
        let body = hir.bodies.get(index).and_then(Option::as_ref);
        let res = resolution
            .items
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|res| res.body.as_ref());
        let (Some(body), Some(res), Some(infer), Some(consts)) = (
            body,
            res,
            inference(db, program, item),
            const_exprs(db, program, item),
        ) else {
            continue;
        };
        body::check_body(&env, item, data, body, res, infer, consts, &mut out);
    }
    out
}

/// Cycles of calls among instant procedures, each reported once at a call inside it.
///
/// Calls through pointers and closures count too, since an instant procedure must finish
/// without input. A value reaches a call either down the stack, as an argument, from the
/// code that names the function: then naming it is an edge from that code (`h(k)` inside
/// `k` is the edge `k → k`); or out of storage (a story variable, a constant, a returned
/// value): then every call through a value may call it. Functions named only as direct
/// arguments of story calls cannot come back out of storage unless some code stores a
/// runtime pointer, in which case every instant procedure used as a value may.
#[salsa::tracked(returns(ref))]
pub fn instant_recursion(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let maps = def_maps(db, program);
    let mut calls: Vec<InstantCall> = Vec::new();
    let mut indirect: Vec<(ItemId, TextRange)> = Vec::new();
    let mut escaping: Vec<ItemId> = Vec::new();
    let mut all_values: Vec<ItemId> = Vec::new();
    let mut stores_pointers = false;
    for &(file, _) in &maps.files {
        let checks = file_kind_checks(db, program, file);
        calls.extend(checks.instant_calls.iter().copied());
        indirect.extend(checks.indirect_calls.iter().copied());
        for &(item, escapes) in &checks.instant_values {
            if !all_values.contains(&item) {
                all_values.push(item);
            }
            if escapes && !escaping.contains(&item) {
                escaping.push(item);
            }
        }
        stores_pointers |= checks.stores_pointers;
    }
    let stored = if stores_pointers {
        &all_values
    } else {
        &escaping
    };
    for &(caller, range) in &indirect {
        calls.extend(stored.iter().map(|&callee| InstantCall {
            caller,
            callee,
            range,
            kind: EdgeKind::Indirect,
        }));
    }
    let mut graph: FxHashMap<ItemId, Vec<ItemId>> = FxHashMap::default();
    let mut kinds: FxHashMap<(ItemId, ItemId), EdgeKind> = FxHashMap::default();
    for call in &calls {
        let callees = graph.entry(call.caller).or_default();
        if !callees.contains(&call.callee) {
            callees.push(call.callee);
        }
        let kind = kinds.entry((call.caller, call.callee)).or_insert(call.kind);
        *kind = (*kind).min(call.kind);
    }
    let components = strongly_connected(&calls, &graph);
    let env = Env::new(db, program);
    let mut reported: FxHashSet<usize> = FxHashSet::default();
    let mut out = Vec::new();
    // Report at a direct call where the cycle has one.
    let mut ordered: Vec<&InstantCall> = calls.iter().collect();
    ordered.sort_by_key(|call| call.kind);
    for call in ordered {
        let (Some(&from), Some(&to)) = (components.get(&call.caller), components.get(&call.callee))
        else {
            continue;
        };
        if from != to || !reported.insert(from) {
            continue;
        }
        // The cycle: the caller, then the shortest way back from the callee.
        let mut path = vec![call.caller];
        path.extend(shortest_path(
            &graph,
            call.callee,
            call.caller,
            &components,
            from,
        ));
        let mut cycle = format!("`{}`", env.fn_path(call.caller));
        for pair in path.windows(2) {
            let [a, b] = pair else {
                continue;
            };
            let how = match kinds.get(&(*a, *b)) {
                Some(EdgeKind::Value) => " (passed as a value to code that may call it)",
                Some(EdgeKind::Indirect) => " (called through a pointer or closure)",
                _ => "",
            };
            cycle.push_str(&format!(" → `{}`{how}", env.fn_path(*b)));
        }
        out.push(Diagnostic::new(
            call.caller.file.path(db).clone(),
            call.range,
            DiagnosticCode::KindInstantRecursion,
            format!(
                "instant procedures cannot recurse, but these calls form a cycle: {cycle}; \
                 mark one of them `#[scene]` and pass through a step"
            ),
        ));
    }
    out
}

/// The strongly connected component of every node that lies on a cycle (a component of
/// several nodes, or one node calling itself), numbered from 0.
fn strongly_connected(
    calls: &[InstantCall],
    graph: &FxHashMap<ItemId, Vec<ItemId>>,
) -> FxHashMap<ItemId, usize> {
    // Kosaraju with explicit stacks: finishing order on the graph, then components on the
    // reversed graph in reverse finishing order.
    let mut nodes: Vec<ItemId> = Vec::new();
    let mut seen_nodes: FxHashSet<ItemId> = FxHashSet::default();
    for call in calls {
        for node in [call.caller, call.callee] {
            if seen_nodes.insert(node) {
                nodes.push(node);
            }
        }
    }
    let mut reverse: FxHashMap<ItemId, Vec<ItemId>> = FxHashMap::default();
    for (from, tos) in graph {
        for to in tos {
            reverse.entry(*to).or_default().push(*from);
        }
    }
    let mut visited: FxHashSet<ItemId> = FxHashSet::default();
    let mut order: Vec<ItemId> = Vec::new();
    for &start in &nodes {
        if !visited.insert(start) {
            continue;
        }
        let mut stack: Vec<(ItemId, usize)> = vec![(start, 0)];
        while let Some((node, next)) = stack.pop() {
            let successors = graph.get(&node).map_or(&[][..], Vec::as_slice);
            if let Some(&succ) = successors.get(next) {
                stack.push((node, next.saturating_add(1)));
                if visited.insert(succ) {
                    stack.push((succ, 0));
                }
            } else {
                order.push(node);
            }
        }
    }
    let mut assigned: FxHashSet<ItemId> = FxHashSet::default();
    let mut component: FxHashMap<ItemId, usize> = FxHashMap::default();
    let mut count = 0usize;
    for &start in order.iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            members.push(node);
            for &pred in reverse.get(&node).map_or(&[][..], Vec::as_slice) {
                if assigned.insert(pred) {
                    stack.push(pred);
                }
            }
        }
        let cyclic = members.len() > 1
            || graph
                .get(&start)
                .is_some_and(|succs| succs.contains(&start));
        if cyclic {
            for member in members {
                component.insert(member, count);
            }
            count = count.saturating_add(1);
        }
    }
    component
}

/// The nodes of a shortest path from `from` to `to` inside component `id`, both included.
fn shortest_path(
    graph: &FxHashMap<ItemId, Vec<ItemId>>,
    from: ItemId,
    to: ItemId,
    components: &FxHashMap<ItemId, usize>,
    id: usize,
) -> Vec<ItemId> {
    let mut parent: FxHashMap<ItemId, ItemId> = FxHashMap::default();
    let mut queue = std::collections::VecDeque::from([from]);
    let mut seen: FxHashSet<ItemId> = FxHashSet::from_iter([from]);
    while let Some(node) = queue.pop_front() {
        if node == to {
            break;
        }
        for &succ in graph.get(&node).map_or(&[][..], Vec::as_slice) {
            if components.get(&succ) == Some(&id) && seen.insert(succ) {
                parent.insert(succ, node);
                queue.push_back(succ);
            }
        }
    }
    let mut path = vec![to];
    let mut node = to;
    while node != from {
        let Some(&prev) = parent.get(&node) else {
            break;
        };
        path.push(prev);
        node = prev;
    }
    path.reverse();
    path
}

/// What the checks read about the program besides one body.
pub(super) struct Env<'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub sigs: &'db ProgramSignatures,
}

impl<'db> Env<'db> {
    fn new(db: &'db dyn Db, program: Program) -> Env<'db> {
        Env {
            db,
            program,
            sigs: program_signatures(db, program),
        }
    }

    pub(super) fn show(&self, ty: &Ty) -> String {
        display_ty(self.db, self.program, ty)
    }

    pub(super) fn item_name(&self, item: ItemId) -> String {
        item_tree(self.db, item.file)
            .item(item.index)
            .and_then(|data| data.name.as_ref())
            .map_or_else(|| "?".to_owned(), ToString::to_string)
    }

    /// `name`, or `Type::name` / `Trait::name` for an associated function.
    pub(super) fn fn_path(&self, item: ItemId) -> String {
        let name = self.item_name(item);
        let Some(parent) = assoc_parent(self.db, item) else {
            return name;
        };
        let owner = match self.sigs.get(parent) {
            Some(ItemSig::Impl {
                self_ty: Ty::Adt { item: adt, .. },
                ..
            }) => self.item_name(*adt),
            Some(ItemSig::Impl { self_ty, .. }) => self.show(self_ty),
            _ => self.item_name(parent),
        };
        format!("{owner}::{name}")
    }

    pub(super) fn is_std(&self, item: ItemId) -> bool {
        matches!(item.file.path(self.db), FilePath::Std(_))
    }

    /// `diag` with a note at the declaration of `item`, unless it is a std item.
    pub(super) fn declared_here(&self, diag: Diagnostic, item: ItemId) -> Diagnostic {
        if self.is_std(item) {
            return diag;
        }
        let Some(data) = item_tree(self.db, item.file).item(item.index) else {
            return diag;
        };
        diag.with_related(
            item.file.path(self.db).clone(),
            data.name_range,
            format!("`{}` is declared here", self.item_name(item)),
        )
    }

    /// `#[scene]` only on a `pushdown fn`, `#[entry]` only with `#[scene]`, `#[must_use]`
    /// only on a struct or an enum.
    fn check_attributes(&self, path: &FilePath, data: &Item, out: &mut Vec<Diagnostic>) {
        for (attr, range) in &data.attrs {
            let message = match attr.as_str() {
                "scene" => match data.kind {
                    ItemKind::Fn {
                        kind: FnKind::Pushdown,
                        ..
                    } => continue,
                    ItemKind::Fn { kind, .. } => format!(
                        "`#[scene]` is only allowed on a `pushdown fn`, but this is a `{}`",
                        fn_kind_word(kind)
                    ),
                    _ => format!(
                        "`#[scene]` is only allowed on a `pushdown fn`, but this is a `{}`",
                        data.kind_name()
                    ),
                },
                "entry" if !data.has_attr("scene") => {
                    "`#[entry]` needs `#[scene]`: an entry scene is a `#[scene]` `pushdown fn`"
                        .to_owned()
                }
                "must_use" => match data.kind {
                    ItemKind::Struct { .. } | ItemKind::Enum { .. } => continue,
                    _ => format!(
                        "`#[must_use]` is only allowed on a struct or an enum, but this is a `{}`",
                        data.kind_name()
                    ),
                },
                _ => continue,
            };
            out.push(Diagnostic::new(
                path.clone(),
                *range,
                DiagnosticCode::KindMisplacedAttribute,
                message,
            ));
        }
    }

    /// The types an item declares: finite parameters and return type of a `pushdown fn`
    /// (references and closures only as parameters, a reference result only from a method
    /// with a reference receiver), entry scenes without `Text`; no reference in the type of
    /// a `const` or `static`; finite story variables; handles as the only immutable statics.
    fn check_item_types(
        &self,
        item: ItemId,
        path: &FilePath,
        data: &Item,
        signature: &Signature,
        out: &mut Vec<Diagnostic>,
    ) {
        let store = &signature.store;
        let diag = |range: TextRange, code: DiagnosticCode, message: String| {
            Diagnostic::new(path.clone(), range, code, message)
        };
        match (self.sigs.get(item), &signature.kind) {
            (
                Some(ItemSig::Fn(sig)),
                SignatureKind::Fn {
                    params: param_refs,
                    ret: ret_ref,
                    ..
                },
                // A `#[builtin]` function is the engine's interface: it has no frame in the
                // story, and its handle receivers (`Meta::write`) are constants at every call.
            ) if sig.kind == FnKind::Pushdown && !sig.builtin => {
                let entry = sig.entry && sig.scene;
                let what = if sig.scene {
                    "a scene"
                } else {
                    "an instant procedure"
                };
                let has_self = sig.self_param.is_some();
                for (index, ty) in sig.params.iter().enumerate() {
                    let range = match index.checked_sub(usize::from(has_self)) {
                        Some(param) => param_refs
                            .get(param)
                            .map_or(data.name_range, |&ty| store.type_range(ty)),
                        None => data.name_range,
                    };
                    if ty.references_error() || matches!(ty, Ty::Closure { .. }) {
                        continue;
                    }
                    if !self.is_finite(ty, true) {
                        out.push(diag(
                            range,
                            DiagnosticCode::KindNotFinite,
                            format!(
                                "parameter type `{}` is not finite: {what} can hold only \
                                 finite types",
                                self.show(ty)
                            ),
                        ));
                    } else if entry && self.contains_text(ty) {
                        out.push(diag(
                            range,
                            DiagnosticCode::KindTextInEntry,
                            format!(
                                "the parameter types of an entry scene cannot contain `Text`, \
                                 but this is `{}`: `fn` code can build runtime text",
                                self.show(ty)
                            ),
                        ));
                    }
                }
                let ret = &sig.ret;
                let range = ret_ref.map_or(data.name_range, |ty| store.type_range(ty));
                if ret.references_error() {
                    return;
                }
                match ret {
                    Ty::Ref { .. }
                        if !matches!(
                            sig.self_param,
                            Some(
                                crate::hir::SelfParamKind::Ref | crate::hir::SelfParamKind::RefMut
                            )
                        ) =>
                    {
                        out.push(diag(
                            range,
                            DiagnosticCode::KindSecondClassReference,
                            format!(
                                "{what} cannot return a reference: references in the story \
                                 are second-class, and only a method whose receiver is a \
                                 reference may return one"
                            ),
                        ));
                    }
                    // Reported at the returned closure value.
                    Ty::Closure { .. } => {}
                    _ if !self.is_finite(ret, true) => out.push(diag(
                        range,
                        DiagnosticCode::KindNotFinite,
                        format!(
                            "return type `{}` is not finite: {what} can hold only finite types",
                            self.show(ret)
                        ),
                    )),
                    _ if entry && self.contains_text(ret) => out.push(diag(
                        range,
                        DiagnosticCode::KindTextInEntry,
                        format!(
                            "the return type of an entry scene cannot contain `Text`, but \
                             this is `{}`",
                            self.show(ret)
                        ),
                    )),
                    _ => {}
                }
            }
            (Some(ItemSig::Const { ty, .. }), SignatureKind::Const { ty: ty_ref }) => {
                if self.contains_ref(ty) {
                    out.push(diag(
                        store.type_range(*ty_ref),
                        DiagnosticCode::KindReferenceInItem,
                        format!(
                            "the type of a `const` cannot contain a reference, but this is `{}`",
                            self.show(ty)
                        ),
                    ));
                }
            }
            (Some(ItemSig::Static { ty, mutable }), SignatureKind::Static { ty: ty_ref }) => {
                let range = store.type_range(*ty_ref);
                if ty.references_error() {
                    return;
                }
                let what = if *mutable {
                    "a story variable"
                } else {
                    "a handle"
                };
                if self.contains_ref(ty) {
                    out.push(diag(
                        range,
                        DiagnosticCode::KindReferenceInItem,
                        format!(
                            "the type of {what} cannot contain a reference, but this is `{}`",
                            self.show(ty)
                        ),
                    ));
                } else if *mutable && !self.is_finite(ty, false) {
                    out.push(diag(
                        range,
                        DiagnosticCode::KindNotFinite,
                        format!(
                            "the type of a story variable must be finite, but `{}` is not",
                            self.show(ty)
                        ),
                    ));
                }
                if !*mutable && !self.is_handle(ty) {
                    out.push(diag(
                        range,
                        DiagnosticCode::KindBadHandle,
                        format!(
                            "an immutable `static` must hold a `Setting<T>` or `Meta<T>` handle, \
                             but this is `{}`; use `const` for a constant or `static mut` for a \
                             story variable",
                            self.show(ty)
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    /// `Setting<T>` or `Meta<T>`.
    fn is_handle(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Adt { item, .. }
            if Some(*item) == self.sigs.std.setting || Some(*item) == self.sigs.std.meta)
    }

    pub(super) fn is_meta(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Adt { item, .. } if Some(*item) == self.sigs.std.meta)
    }
}

pub(super) fn fn_kind_word(kind: FnKind) -> &'static str {
    match kind {
        FnKind::Plain => "fn",
        FnKind::Const => "const fn",
        FnKind::Pushdown => "pushdown fn",
    }
}
