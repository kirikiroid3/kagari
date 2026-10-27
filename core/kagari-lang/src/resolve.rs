//! Name resolution of signatures and bodies (compilation stage 2): paths, locals, generic
//! parameters, `Self`, loop labels and asset paths.

use kagari_syntax::TextRange;
use la_arena::{Arena, Idx};
use rustc_hash::FxHashMap;

use crate::db::Db;
pub use crate::def_map::Resolution;
use crate::def_map::{
    Def, DefMaps, ModuleId, Namespace, PathResolution, def_maps, resolve_first_segment,
    resolve_module_path, resolve_rest,
};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::hir::{
    ArrayExpr, Condition, Expr, ExprId, ExprStore, GenericArg, GenericArgs, GenericParamKind,
    LangItem, Literal, Name, Pat, PatBound, PatId, Path, PathKind, Signature, SignatureKind, Stmt,
    TypeRef, TypeRefId, body_of, signature_of,
};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, ItemParent, Shape, item_tree};

pub type LocalId = Idx<Local>;

/// A local binding. `pat` is `None` for `self`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub name: Name,
    pub pat: Option<PatId>,
    pub mutable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignatureResolution {
    pub store: StoreResolution,
    pub diagnostics: Vec<Diag>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyResolutionOut {
    pub body: Option<BodyResolution>,
    pub diagnostics: Vec<Diag>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreResolution {
    pub exprs: FxHashMap<ExprId, Resolution>,
    pub pats: FxHashMap<PatId, Resolution>,
    pub types: FxHashMap<TypeRefId, Resolution>,
    /// Generic type arguments that name a value (`Choice<A, N>`).
    pub const_args: FxHashMap<TypeRefId, Resolution>,
    /// Paths in range patterns: `(pat, 0)` for the start, `(pat, 1)` for the end.
    pub range_bounds: FxHashMap<(PatId, u8), Resolution>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyResolution {
    pub store: StoreResolution,
    pub locals: Arena<Local>,
    pub bindings: FxHashMap<PatId, LocalId>,
    /// `break`/`continue` → the loop it leaves or continues.
    pub loop_targets: FxHashMap<ExprId, ExprId>,
}

/// Resolution of an item's signature; empty for files outside the program.
#[salsa::tracked(returns(ref))]
pub fn signature_resolution(db: &dyn Db, program: Program, item: ItemId) -> SignatureResolution {
    let mut out = SignatureResolution::default();
    let Some(cx) = ItemCx::new(db, program, item) else {
        return out;
    };
    if let Some(signature) = signature_of(db, item) {
        let mut resolver = cx.resolver(
            db,
            Owner::Signature(item),
            &signature.store,
            &mut out.diagnostics,
        );
        resolver.resolve_signature(signature);
        out.store = resolver.out;
    }
    out
}

/// Resolution of an item's body; empty for files outside the program.
#[salsa::tracked(returns(ref))]
pub fn body_resolution(db: &dyn Db, program: Program, item: ItemId) -> BodyResolutionOut {
    let mut out = BodyResolutionOut::default();
    let Some(cx) = ItemCx::new(db, program, item) else {
        return out;
    };
    if let Some(body) = body_of(db, item) {
        let mut resolver = cx.resolver(db, Owner::Body(item), &body.store, &mut out.diagnostics);
        if body.has_self {
            let local = resolver.locals.alloc(Local {
                name: Name::new_static("self"),
                pat: None,
                mutable: body.self_mutable,
            });
            resolver
                .scopes
                .push(vec![(Name::new_static("self"), local)]);
        } else {
            resolver.scopes.push(Vec::new());
        }
        let mut binds = Vec::new();
        for &param in &body.params {
            resolver.walk_pat(param, &mut BindMode::New(&mut binds));
        }
        resolver.push_binds(binds);
        resolver.resolve_expr(body.value);
        out.body = Some(BodyResolution {
            store: resolver.out,
            locals: resolver.locals,
            bindings: resolver.bindings,
            loop_targets: resolver.loop_targets,
        });
    }
    out
}

/// Generic parameters in scope: `(name, owner, index, is_const)`.
type GenericScope = Vec<(Name, ItemId, u32, bool)>;

/// The scope shared by an item's signature and body.
struct ItemCx<'db> {
    maps: &'db DefMaps,
    item: ItemId,
    file_path: FilePath,
    asset_root: FilePath,
    module: ModuleId,
    generics: GenericScope,
    self_owner: Option<ItemId>,
    asset_fn: Option<Def>,
}

impl<'db> ItemCx<'db> {
    /// `None` for items in files outside the program.
    fn new(db: &'db dyn Db, program: Program, item: ItemId) -> Option<Self> {
        let maps = def_maps(db, program);
        if !maps.file_modules.contains_key(&item.file) {
            return None;
        }
        let asset_fn = match resolve_module_path(
            maps,
            db,
            maps.std_root(),
            &Path::lang(LangItem::Asset),
            Namespace::Values,
            // Not written in source; the diagnostics are dropped.
            &|_| Anchor::Text(item.file, TextRange::default()),
        )
        .resolved
        {
            Some(Resolution::Def(def)) => Some(def),
            _ => None,
        };
        let tree = item_tree(db, item.file);
        let module = maps.module_of_item(db, item);

        // Generic parameters and `Self`: the item's own, plus its impl's or trait's.
        let mut generics: GenericScope = Vec::new();
        let mut self_owner = None;
        let data = tree.item(item.index);
        if matches!(
            data.map(|data| &data.kind),
            Some(ItemKind::Impl { .. } | ItemKind::Trait { .. })
        ) {
            self_owner = Some(item);
        }
        if let Some(ItemParent::Item(parent)) = data.map(|data| data.parent)
            && matches!(
                tree.item(parent).map(|data| &data.kind),
                Some(ItemKind::Impl { .. } | ItemKind::Trait { .. })
            )
        {
            let parent = ItemId {
                file: item.file,
                index: parent,
            };
            self_owner = Some(parent);
            push_generics(&mut generics, db, parent);
        }
        push_generics(&mut generics, db, item);
        Some(Self {
            maps,
            item,
            file_path: item.file.path(db).clone(),
            asset_root: program.asset_root(db).clone(),
            module,
            generics,
            self_owner,
            asset_fn,
        })
    }

    fn resolver<'a>(
        &'a self,
        db: &'a dyn Db,
        owner: Owner,
        store: &'a ExprStore,
        diagnostics: &'a mut Vec<Diag>,
    ) -> Resolver<'a> {
        Resolver::new(
            db,
            self.maps,
            self.item.file,
            self.file_path.clone(),
            self.asset_root.clone(),
            self.module,
            owner,
            store,
            &self.generics,
            self.self_owner,
            self.asset_fn,
            diagnostics,
        )
    }
}

fn push_generics(scope: &mut GenericScope, db: &dyn Db, owner: ItemId) {
    let Some(signature) = signature_of(db, owner) else {
        return;
    };
    for (index, param) in signature.generics.iter().enumerate() {
        scope.push((
            param.name.clone(),
            owner,
            u32::try_from(index).unwrap_or(u32::MAX),
            matches!(param.kind, GenericParamKind::Const { .. }),
        ));
    }
}

enum BindMode<'a> {
    /// Bindings create new locals, collected here.
    New(&'a mut Vec<(Name, LocalId)>),
    /// A later alternative of an or-pattern: bindings map to the first alternative's locals.
    Alt {
        existing: &'a [(Name, LocalId)],
        seen: Vec<Name>,
    },
}

struct Resolver<'a> {
    db: &'a dyn Db,
    maps: &'a DefMaps,
    file: SourceFile,
    file_path: FilePath,
    asset_root: FilePath,
    module: ModuleId,
    src: Owner,
    store: &'a ExprStore,
    generics: &'a GenericScope,
    self_owner: Option<ItemId>,
    asset_fn: Option<Def>,
    diagnostics: &'a mut Vec<Diag>,
    out: StoreResolution,
    locals: Arena<Local>,
    bindings: FxHashMap<PatId, LocalId>,
    loop_targets: FxHashMap<ExprId, ExprId>,
    scopes: Vec<Vec<(Name, LocalId)>>,
    loops: Vec<(Option<Name>, ExprId)>,
}

impl<'a> Resolver<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        db: &'a dyn Db,
        maps: &'a DefMaps,
        file: SourceFile,
        file_path: FilePath,
        asset_root: FilePath,
        module: ModuleId,
        src: Owner,
        store: &'a ExprStore,
        generics: &'a GenericScope,
        self_owner: Option<ItemId>,
        asset_fn: Option<Def>,
        diagnostics: &'a mut Vec<Diag>,
    ) -> Self {
        Resolver {
            db,
            maps,
            file,
            file_path,
            asset_root,
            module,
            src,
            store,
            generics,
            self_owner,
            asset_fn,
            diagnostics,
            out: StoreResolution::default(),
            locals: Arena::new(),
            bindings: FxHashMap::default(),
            loop_targets: FxHashMap::default(),
            scopes: Vec::new(),
            loops: Vec::new(),
        }
    }

    fn diag(&mut self, node: NodeId, code: DiagnosticCode, message: String) {
        self.diagnostics
            .push(Diag::new(Anchor::Node(self.src, node), code, message));
    }

    // ---- paths ----

    fn lookup_local(&self, name: &Name) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .find(|(local, _)| local == name)
            .map(|(_, id)| *id)
    }

    /// Resolves `path`, owned by `node`, with the full lexical chain; diagnostics are
    /// returned, not reported.
    fn resolve_path(&mut self, path: &Path, ns: Namespace, node: NodeId) -> PathResolution {
        let src = self.src;
        let at = move |segment| Anchor::Segment(src, node, segment);
        if path.kind != PathKind::Plain {
            return resolve_module_path(self.maps, self.db, self.module, path, ns, &at);
        }
        let Some(first) = path.segments.first() else {
            return PathResolution::default();
        };
        let single = path.segments.len() == 1;
        let first_ns = if single { ns } else { Namespace::Types };
        let name = &first.name;
        let base = if first_ns == Namespace::Values
            && let Some(local) = self.lookup_local(name)
        {
            Some(Resolution::Local(local))
        } else if let Some((_, owner, index, _)) =
            self.generics.iter().rev().find(|(param, _, _, is_const)| {
                param == name && *is_const == (first_ns == Namespace::Values)
            })
        {
            Some(Resolution::GenericParam {
                owner: *owner,
                index: *index,
            })
        } else if name == "Self" {
            match self.self_owner {
                Some(owner) => Some(Resolution::SelfType(owner)),
                None => {
                    let mut result = PathResolution::default();
                    result.diagnostics.push(Diag::new(
                        at(0),
                        DiagnosticCode::ResolveSelfOutsideImpl,
                        "`Self` is only available in impls and traits",
                    ));
                    result.failed_segment = Some(0);
                    return result;
                }
            }
        } else if name == "self" && first_ns == Namespace::Values {
            let mut result = PathResolution::default();
            result.diagnostics.push(Diag::new(
                at(0),
                DiagnosticCode::ResolveUnresolvedName,
                "`self` is only available in methods with a `self` parameter",
            ));
            result.failed_segment = Some(0);
            return result;
        } else {
            None
        };
        match base {
            Some(base) => resolve_rest(self.maps, self.db, self.module, base, path, 1, ns, &at),
            None => resolve_module_path(self.maps, self.db, self.module, path, ns, &at),
        }
    }

    /// Resolves a path owned by `node`, reports its diagnostics and resolves its generic
    /// arguments.
    fn resolve_path_reported(&mut self, path: &Path, ns: Namespace, node: NodeId) -> Resolution {
        let result = self.resolve_path(path, ns, node);
        self.diagnostics.extend(result.diagnostics);
        self.resolve_path_args(path);
        result.resolved.unwrap_or(Resolution::Error)
    }

    fn resolve_path_args(&mut self, path: &Path) {
        for segment in &path.segments {
            if let Some(args) = &segment.args {
                self.resolve_generic_args(args);
            }
        }
    }

    fn resolve_generic_args(&mut self, args: &GenericArgs) {
        for arg in &args.0 {
            match *arg {
                GenericArg::Type(ty) => self.resolve_type_arg(ty),
                GenericArg::Const(expr) => self.resolve_expr(expr),
            }
        }
    }

    /// A generic type argument; a bare single-segment path that is not a type may name a value.
    fn resolve_type_arg(&mut self, ty: TypeRefId) {
        let store = self.store;
        if let TypeRef::Path(path) = store.type_ref(ty)
            && path.kind == PathKind::Plain
            && path.segments.len() == 1
        {
            let as_type = self.resolve_path(path, Namespace::Types, NodeId::Type(ty));
            if as_type.resolved.is_none() {
                let as_value = self.resolve_path(path, Namespace::Values, NodeId::Type(ty));
                if let Some(resolved) = as_value.resolved {
                    self.diagnostics.extend(as_value.diagnostics);
                    self.out.const_args.insert(ty, resolved);
                    return;
                }
            }
        }
        self.resolve_type(ty);
    }

    // ---- types and signatures ----

    fn resolve_type(&mut self, ty: TypeRefId) {
        let store = self.store;
        let type_ref = store.type_ref(ty);
        match type_ref {
            TypeRef::Missing | TypeRef::Infer => {}
            TypeRef::Path(path) => {
                let resolved = self.resolve_path_reported(path, Namespace::Types, NodeId::Type(ty));
                self.out.types.insert(ty, resolved);
            }
            TypeRef::Tuple(fields) => {
                for &field in fields {
                    self.resolve_type(field);
                }
            }
            TypeRef::Array { elem, len } => {
                self.resolve_type(*elem);
                self.resolve_expr(*len);
            }
            TypeRef::Ref { inner, .. } => self.resolve_type(*inner),
            TypeRef::FnPtr { params, ret, .. } | TypeRef::Closure { params, ret, .. } => {
                for &param in params {
                    self.resolve_type(param);
                }
                if let Some(ret) = ret {
                    self.resolve_type(*ret);
                }
            }
        }
    }

    fn resolve_signature(&mut self, signature: &Signature) {
        for param in &signature.generics {
            match &param.kind {
                GenericParamKind::Type { bounds } => {
                    for &bound in bounds {
                        self.resolve_type(bound);
                    }
                }
                GenericParamKind::Const { ty } => self.resolve_type(*ty),
            }
        }
        match &signature.kind {
            SignatureKind::Fn { params, ret, .. } => {
                for &param in params {
                    self.resolve_type(param);
                }
                if let Some(ret) = ret {
                    self.resolve_type(*ret);
                }
            }
            SignatureKind::Struct { fields } => {
                for &(_, ty) in fields {
                    self.resolve_type(ty);
                }
            }
            SignatureKind::Enum { variants } => {
                for &(_, ty) in variants.iter().flatten() {
                    self.resolve_type(ty);
                }
            }
            SignatureKind::Const { ty } | SignatureKind::Static { ty } => self.resolve_type(*ty),
            SignatureKind::TypeAlias { bounds, value } => {
                for &bound in bounds {
                    self.resolve_type(bound);
                }
                if let Some(value) = value {
                    self.resolve_type(*value);
                }
            }
            SignatureKind::Impl { trait_ref, self_ty } => {
                if let Some(trait_ref) = trait_ref {
                    self.resolve_type(*trait_ref);
                }
                self.resolve_type(*self_ty);
            }
            SignatureKind::Trait { bounds } => {
                for &bound in bounds {
                    self.resolve_type(bound);
                }
            }
            SignatureKind::None => {}
        }
    }

    // ---- patterns ----

    fn push_binds(&mut self, binds: Vec<(Name, LocalId)>) {
        match self.scopes.last_mut() {
            Some(scope) => scope.extend(binds),
            None => self.scopes.push(binds),
        }
    }

    /// Whether an identifier pattern names a unit struct, unit variant, `const` or `static`.
    fn ident_as_path(&self, name: &Name) -> Option<Resolution> {
        let (resolution, _) =
            resolve_first_segment(self.maps, self.db, self.module, name, Namespace::Values)?;
        let Resolution::Def(def) = &resolution else {
            return None;
        };
        let is_path = match *def {
            Def::Item(item) => matches!(
                item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(
                    ItemKind::Struct { shape: Shape::Unit }
                        | ItemKind::Const { .. }
                        | ItemKind::Static { .. }
                )
            ),
            Def::Variant { enum_item, index } => matches!(
                item_tree(self.db, enum_item.file)
                    .item(enum_item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Enum { variants })
                    if matches!(variants.get(index as usize), Some((_, Shape::Unit)))
            ),
            Def::Module(_) => false,
        };
        is_path.then_some(resolution)
    }

    fn walk_pat(&mut self, pat: PatId, mode: &mut BindMode<'_>) {
        let store = self.store;
        let data = store.pat(pat);
        match data {
            Pat::Missing | Pat::Wild | Pat::Lit { .. } | Pat::Rest => {}
            Pat::Bind { name, mutable } => {
                if let Some(resolution) = self.ident_as_path(name) {
                    self.out.pats.insert(pat, resolution);
                    return;
                }
                self.bind(pat, name, *mutable, mode);
            }
            Pat::Path(path) => {
                let resolved =
                    self.resolve_path_reported(path, Namespace::Values, NodeId::Pat(pat));
                self.out.pats.insert(pat, resolved);
            }
            Pat::Range { start, end, .. } => {
                for (slot, bound) in [(0u8, start), (1u8, end)] {
                    if let Some(PatBound::Path(path)) = bound {
                        let resolved = self.resolve_path_reported(
                            path,
                            Namespace::Values,
                            NodeId::PatBound(pat, slot),
                        );
                        self.out.range_bounds.insert((pat, slot), resolved);
                    }
                }
            }
            Pat::Tuple(pats) => {
                for &sub in pats {
                    self.walk_pat(sub, mode);
                }
            }
            Pat::TupleStruct { path, pats } => {
                let resolved =
                    self.resolve_path_reported(path, Namespace::Values, NodeId::Pat(pat));
                self.out.pats.insert(pat, resolved);
                for &sub in pats {
                    self.walk_pat(sub, mode);
                }
            }
            Pat::Record { path, fields } => {
                let resolved = self.resolve_path_reported(path, Namespace::Types, NodeId::Pat(pat));
                self.out.pats.insert(pat, resolved);
                for &(_, sub) in fields {
                    self.walk_pat(sub, mode);
                }
            }
            Pat::Or(alternatives) => {
                let Some((&first, rest)) = alternatives.split_first() else {
                    return;
                };
                match mode {
                    BindMode::New(binds) => {
                        let start = binds.len();
                        self.walk_pat(first, &mut BindMode::New(binds));
                        let bound: Vec<(Name, LocalId)> =
                            binds.get(start..).map(<[_]>::to_vec).unwrap_or_default();
                        for &alternative in rest {
                            let mut alt = BindMode::Alt {
                                existing: &bound,
                                seen: Vec::new(),
                            };
                            self.walk_pat(alternative, &mut alt);
                            let BindMode::Alt { seen, .. } = alt else {
                                continue;
                            };
                            for (name, _) in &bound {
                                if !seen.contains(name) {
                                    self.diag(
                                        NodeId::Pat(alternative),
                                        DiagnosticCode::ResolveInconsistentBinding,
                                        format!("variable `{name}` is not bound in all patterns"),
                                    );
                                }
                            }
                        }
                    }
                    BindMode::Alt { .. } => {
                        for &alternative in alternatives {
                            self.walk_pat(alternative, mode);
                        }
                    }
                }
            }
        }
    }

    fn bind(&mut self, pat: PatId, name: &Name, mutable: bool, mode: &mut BindMode<'_>) {
        match mode {
            BindMode::New(binds) => {
                if binds.iter().any(|(bound, _)| bound == name) {
                    self.diag(
                        NodeId::Pat(pat),
                        DiagnosticCode::ResolveDuplicateBinding,
                        format!("identifier `{name}` is bound more than once in the same pattern"),
                    );
                }
                let local = self.locals.alloc(Local {
                    name: name.clone(),
                    pat: Some(pat),
                    mutable,
                });
                self.bindings.insert(pat, local);
                binds.push((name.clone(), local));
            }
            BindMode::Alt { existing, seen } => {
                if seen.contains(name) {
                    self.diag(
                        NodeId::Pat(pat),
                        DiagnosticCode::ResolveDuplicateBinding,
                        format!("identifier `{name}` is bound more than once in the same pattern"),
                    );
                }
                seen.push(name.clone());
                match existing.iter().find(|(bound, _)| bound == name) {
                    Some((_, local)) => {
                        self.bindings.insert(pat, *local);
                    }
                    None => {
                        self.diag(
                            NodeId::Pat(pat),
                            DiagnosticCode::ResolveInconsistentBinding,
                            format!("variable `{name}` is not bound in all patterns"),
                        );
                        let local = self.locals.alloc(Local {
                            name: name.clone(),
                            pat: Some(pat),
                            mutable,
                        });
                        self.bindings.insert(pat, local);
                    }
                }
            }
        }
    }

    /// Resolves a pattern and returns the locals it binds.
    fn pattern(&mut self, pat: PatId) -> Vec<(Name, LocalId)> {
        let mut binds = Vec::new();
        self.walk_pat(pat, &mut BindMode::New(&mut binds));
        binds
    }

    // ---- expressions ----

    fn with_scope(&mut self, binds: Vec<(Name, LocalId)>, f: impl FnOnce(&mut Self)) {
        self.scopes.push(binds);
        f(self);
        self.scopes.pop();
    }

    fn resolve_condition(&mut self, cond: Condition, then: impl FnOnce(&mut Self)) {
        match cond {
            Condition::Expr(expr) => {
                self.resolve_expr(expr);
                then(self);
            }
            Condition::Let { pat, expr } => {
                self.resolve_expr(expr);
                let binds = self.pattern(pat);
                self.with_scope(binds, then);
            }
        }
    }

    fn with_loop(&mut self, label: Option<Name>, id: ExprId, f: impl FnOnce(&mut Self)) {
        self.loops.push((label, id));
        f(self);
        self.loops.pop();
    }

    fn loop_target(&mut self, id: ExprId, label: Option<&Name>, what: &str) {
        let range = NodeId::Expr(id);
        let target = match label {
            Some(label) => {
                let found = self
                    .loops
                    .iter()
                    .rev()
                    .find(|(name, _)| name.as_ref() == Some(label))
                    .map(|(_, target)| *target);
                if found.is_none() {
                    self.diag(
                        range,
                        DiagnosticCode::ResolveUndeclaredLabel,
                        format!("use of undeclared label `'{label}`"),
                    );
                }
                found
            }
            None => {
                let found = self.loops.last().map(|(_, target)| *target);
                if found.is_none() {
                    self.diag(
                        range,
                        DiagnosticCode::ResolveOutsideLoop,
                        format!("`{what}` outside of a loop"),
                    );
                }
                found
            }
        };
        if let Some(target) = target {
            self.loop_targets.insert(id, target);
        }
    }

    fn resolve_exprs(&mut self, exprs: &[ExprId]) {
        for &expr in exprs {
            self.resolve_expr(expr);
        }
    }

    fn resolve_expr(&mut self, id: ExprId) {
        let store = self.store;
        let expr = store.expr(id);
        match expr {
            Expr::Missing | Expr::Literal(_) => {}
            Expr::Path(path) => {
                let resolved =
                    self.resolve_path_reported(path, Namespace::Values, NodeId::Expr(id));
                self.out.exprs.insert(id, resolved);
            }
            Expr::Block { stmts, tail, scope } => {
                let saved = self.module;
                if let Some(block) = scope
                    && let Some(&module) = self.maps.block_modules.get(&(self.file, *block))
                {
                    self.module = module;
                }
                self.scopes.push(Vec::new());
                for stmt in stmts {
                    match *stmt {
                        Stmt::Let { pat, ty, init } => {
                            if let Some(ty) = ty {
                                self.resolve_type(ty);
                            }
                            if let Some(init) = init {
                                self.resolve_expr(init);
                            }
                            let binds = self.pattern(pat);
                            self.push_binds(binds);
                        }
                        Stmt::Expr { expr, .. } => self.resolve_expr(expr),
                        Stmt::Item(_) => {}
                    }
                }
                if let Some(tail) = tail {
                    self.resolve_expr(*tail);
                }
                self.scopes.pop();
                self.module = saved;
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let then_branch = *then_branch;
                self.resolve_condition(*cond, |this| this.resolve_expr(then_branch));
                if let Some(else_branch) = else_branch {
                    self.resolve_expr(*else_branch);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.resolve_expr(*scrutinee);
                for arm in arms {
                    let binds = self.pattern(arm.pat);
                    self.with_scope(binds, |this| {
                        if let Some(guard) = arm.guard {
                            this.resolve_expr(guard);
                        }
                        this.resolve_expr(arm.body);
                    });
                }
            }
            Expr::Loop { label, body } => {
                let body = *body;
                self.with_loop(label.clone(), id, |this| this.resolve_expr(body));
            }
            Expr::While { label, cond, body } => {
                let (label, body) = (label.clone(), *body);
                self.resolve_condition(*cond, |this| {
                    this.with_loop(label, id, |this| this.resolve_expr(body));
                });
            }
            Expr::For {
                label,
                pat,
                iterable,
                body,
            } => {
                self.resolve_expr(*iterable);
                let binds = self.pattern(*pat);
                let (label, body) = (label.clone(), *body);
                self.with_scope(binds, |this| {
                    this.with_loop(label, id, |this| this.resolve_expr(body));
                });
            }
            Expr::Break { label, value } => {
                if let Some(value) = value {
                    self.resolve_expr(*value);
                }
                self.loop_target(id, label.as_ref(), "break");
            }
            Expr::Continue { label } => self.loop_target(id, label.as_ref(), "continue"),
            Expr::Return { value } => {
                if let Some(value) = value {
                    self.resolve_expr(*value);
                }
            }
            Expr::Call { callee, args } => {
                self.resolve_expr(*callee);
                self.resolve_exprs(args);
                self.check_asset(*callee, args);
            }
            Expr::MethodCall {
                receiver,
                args_generic,
                args,
                ..
            } => {
                self.resolve_expr(*receiver);
                if let Some(generic) = args_generic {
                    self.resolve_generic_args(generic);
                }
                self.resolve_exprs(args);
            }
            Expr::Field { base, .. } => self.resolve_expr(*base),
            Expr::Index { base, index } => {
                self.resolve_expr(*base);
                self.resolve_expr(*index);
            }
            Expr::Try(inner) | Expr::Unary { expr: inner, .. } | Expr::Ref { expr: inner, .. } => {
                self.resolve_expr(*inner);
            }
            Expr::Cast { expr, ty } => {
                self.resolve_expr(*expr);
                self.resolve_type(*ty);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(*lhs);
                self.resolve_expr(*rhs);
            }
            Expr::Assign { target, value, .. } => {
                self.resolve_expr(*target);
                self.resolve_expr(*value);
            }
            Expr::Range { start, end, .. } => {
                if let Some(start) = start {
                    self.resolve_expr(*start);
                }
                if let Some(end) = end {
                    self.resolve_expr(*end);
                }
            }
            Expr::Tuple(exprs) | Expr::Array(ArrayExpr::List(exprs)) => self.resolve_exprs(exprs),
            Expr::Array(ArrayExpr::Repeat { value, count }) => {
                self.resolve_expr(*value);
                self.resolve_expr(*count);
            }
            Expr::Record { path, fields } => {
                let resolved = self.resolve_path_reported(path, Namespace::Types, NodeId::Expr(id));
                self.out.exprs.insert(id, resolved);
                for &(_, value) in fields {
                    self.resolve_expr(value);
                }
            }
            Expr::Closure { params, ret, body } => {
                let mut binds = Vec::new();
                for &(pat, ty) in params {
                    if let Some(ty) = ty {
                        self.resolve_type(ty);
                    }
                    self.walk_pat(pat, &mut BindMode::New(&mut binds));
                }
                if let Some(ret) = ret {
                    self.resolve_type(*ret);
                }
                let body = *body;
                let loops = std::mem::take(&mut self.loops);
                self.with_scope(binds, |this| this.resolve_expr(body));
                self.loops = loops;
            }
        }
    }

    /// `asset("path")` in a disk file: the path must be non-empty and name an existing file
    /// inside the program's asset directory.
    fn check_asset(&mut self, callee: ExprId, args: &[ExprId]) {
        let Some(asset_fn) = self.asset_fn else {
            return;
        };
        if self.out.exprs.get(&callee) != Some(&Resolution::Def(asset_fn)) {
            return;
        }
        if !matches!(self.file_path, FilePath::Disk(_)) {
            return;
        }
        let Some(&arg) = args.first() else {
            return;
        };
        let Expr::Literal(Literal::String(path)) = self.store.expr(arg) else {
            return;
        };
        let range = NodeId::Expr(arg);
        if path.is_empty() {
            self.diag(
                range,
                DiagnosticCode::ResolveEmptyAssetPath,
                "asset path must not be empty".to_owned(),
            );
            return;
        }
        let asset_path = self.asset_root.join(path);
        let inside = matches!(
            (&asset_path, &self.asset_root),
            (FilePath::Disk(file), FilePath::Disk(root))
                if file.as_ref() != root.as_ref() && file.starts_with(root)
        );
        if !inside {
            self.diag(
                range,
                DiagnosticCode::ResolveAssetOutsideRoot,
                format!("asset path `{path}` leaves the asset directory"),
            );
            return;
        }
        let asset = self.db.asset_file(asset_path);
        if !asset.exists(self.db) {
            self.diag(
                range,
                DiagnosticCode::ResolveAssetNotFound,
                format!("asset file `{path}` not found"),
            );
        }
    }
}
