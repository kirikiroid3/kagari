//! Type inference of bodies (compilation stage 3): the type of every expression, pattern and
//! local, plus what each path, method call, field access and struct expression refers to.
//! The stage 3 checks and const evaluation read these results.

mod call;
mod ctx;
mod expr;
mod finish;
mod ops;
mod pat;
mod path;
mod table;
pub(crate) mod walk;

use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::Diag;
use crate::hir::{Body, ClosureKind, ExprId, GenericParamKind, PatId, body_of, signature_of};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::resolve::{BodyResolution, LocalId, body_resolution};
use crate::signatures::lower::TyLowerer;
use crate::signatures::{BoundEnv, ItemSig, ProgramSignatures, assoc_parent, program_signatures};
use crate::ty::{CtorRef, FnRef, GenericArg, Ty};

use ctx::{Expect, InferCtx};

/// The inference result of one item's body, with the diagnostics it reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InferOutput {
    pub result: InferenceResult,
    pub diagnostics: Vec<Diag>,
}

/// Infers one item's body: `Some` for each `fn` with a body, `const` and `static` with
/// an initializer, in a file of the program.
#[salsa::tracked(returns(ref))]
pub fn infer(db: &dyn Db, program: Program, item: ItemId) -> Option<InferOutput> {
    if !def_maps(db, program).file_modules.contains_key(&item.file) {
        return None;
    }
    let tree = item_tree(db, item.file);
    let data = tree.item(item.index)?;
    if !matches!(
        data.kind,
        ItemKind::Fn { .. } | ItemKind::Const { .. } | ItemKind::Static { .. }
    ) {
        return None;
    }
    let body = body_of(db, item)?;
    let res = body_resolution(db, program, item).body.as_ref()?;
    let sigs = program_signatures(db, program);
    let mut diagnostics = Vec::new();
    let result = infer_body(db, program, sigs, item, body, res, &mut diagnostics)?;
    Some(InferOutput {
        result,
        diagnostics,
    })
}

fn infer_body(
    db: &dyn Db,
    program: Program,
    sigs: &ProgramSignatures,
    item: ItemId,
    body: &Body,
    res: &BodyResolution,
    diagnostics: &mut Vec<Diag>,
) -> Option<InferenceResult> {
    let maps = def_maps(db, program);
    let (fn_kind, declared) = match sigs.get(item)? {
        ItemSig::Fn(sig) => (
            match sig.kind {
                FnKind::Plain => ClosureKind::Plain,
                FnKind::Const => ClosureKind::Const,
                FnKind::Pushdown => ClosureKind::Pushdown,
            },
            sig.ret.clone(),
        ),
        ItemSig::Const { ty, .. } | ItemSig::Static { ty, .. } => (ClosureKind::Const, ty.clone()),
        _ => return None,
    };
    let self_owner = assoc_parent(db, item);
    let mut generic_scope = Vec::new();
    for owner in self_owner.into_iter().chain(std::iter::once(item)) {
        if let Some(signature) = signature_of(db, owner) {
            for (index, param) in signature.generics.iter().enumerate() {
                generic_scope.push((
                    param.name.clone(),
                    owner,
                    u32::try_from(index).unwrap_or(u32::MAX),
                    matches!(param.kind, GenericParamKind::Const { .. }),
                ));
            }
        }
    }
    let mut cx = InferCtx {
        db,
        program,
        sigs,
        maps,
        item,
        store: &body.store,
        res,
        env: BoundEnv::of_item(sigs, item),
        lowerer: TyLowerer::new(db, program),
        table: table::Table::default(),
        result: InferenceResult::default(),
        diags: Vec::new(),
        fn_kind,
        closure_kinds: Vec::new(),
        returns: vec![declared.clone()],
        loops: FxHashMap::default(),
        modules: Vec::new(),
        self_owner,
        generic_scope,
        projections: Vec::new(),
        links: Vec::new(),
        obligations: Vec::new(),
        into_checks: Vec::new(),
        pat_fits: Vec::new(),
    };
    cx.result.return_ty = declared.clone();
    if let Some(ItemSig::Fn(sig)) = sigs.get(item) {
        let mut params = sig.params.iter();
        if body.has_self {
            let self_ty = params.next().cloned().unwrap_or(Ty::error());
            if let Some((local, _)) = res.locals.iter().find(|(_, local)| local.pat.is_none()) {
                cx.result.local_types.insert(local, self_ty);
            }
        }
        for &pat in &body.params {
            let ty = params.next().cloned().unwrap_or(Ty::error());
            cx.check_pat(pat, &ty);
        }
    }
    cx.infer_expr(body.value, Expect::Coerce(&declared));
    let (result, diags) = cx.finish();
    diagnostics.extend(diags);
    Some(result)
}

/// The inference result of one item's body.
pub fn inference(db: &dyn Db, program: Program, item: ItemId) -> Option<&InferenceResult> {
    infer(db, program, item).as_ref().map(|out| &out.result)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InferenceResult {
    /// The type of every expression, before the coercion recorded in `coercions`. No
    /// inference variables remain: unresolved ones become `Ty::Error` (and are reported).
    pub expr_types: ArenaMap<ExprId, Ty>,
    pub pat_types: ArenaMap<PatId, Ty>,
    /// Every local of the body's `BodyResolution`, `self` and closure parameters included.
    pub local_types: ArenaMap<LocalId, Ty>,
    /// A function's declared return type; the declared type of a `const` or `static`.
    pub return_ty: Ty,
    /// What each `Expr::Path` denotes, including the callee paths of calls.
    pub paths: FxHashMap<ExprId, ValueRef>,
    /// Each `Expr::MethodCall`.
    pub method_calls: FxHashMap<ExprId, MethodCall>,
    /// Each `Expr::Field`.
    pub fields: FxHashMap<ExprId, FieldRef>,
    /// The struct or variant each `Expr::Record` builds.
    pub records: FxHashMap<ExprId, CtorRef>,
    /// The struct or variant matched by each `Pat::Path`, `Pat::TupleStruct`,
    /// `Pat::Record`, and each `Pat::Bind` that names a unit struct or unit variant.
    pub pat_ctors: FxHashMap<PatId, CtorRef>,
    /// Each `Expr::Closure`.
    pub closures: FxHashMap<ExprId, ClosureInfo>,
    /// Expressions converted at a coercion site (argument, `let` with a type, field,
    /// return value, assignment, array element, arm of `if`/`match`).
    pub coercions: FxHashMap<ExprId, (Coercion, Ty)>,
}

/// What a path expression denotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueRef {
    Local(LocalId),
    /// A function or a tuple constructor, with its generic arguments.
    Fn(FnRef),
    /// A unit struct or unit variant.
    Ctor(CtorRef),
    /// A `const` item; `args` are the parent impl's generic arguments for an associated
    /// constant (`Self::CELLS` in `impl<const N: u64> Square<N>`), empty otherwise.
    Const {
        item: ItemId,
        args: Vec<GenericArg>,
    },
    /// A `static` or `static mut` item.
    Static(ItemId),
    /// A `const` generic parameter used as a value.
    ConstParam {
        owner: ItemId,
        index: u32,
    },
    /// Already reported.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodCall {
    /// The method with its generic arguments; the receiver is its first parameter.
    pub func: FnRef,
    pub receiver: ReceiverAdjust,
}

/// How the receiver expression becomes the method's `self` argument: dereferenced
/// `derefs` times, then borrowed when `autoref` is `Some(mutable)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiverAdjust {
    pub derefs: u32,
    pub autoref: Option<bool>,
}

/// A field access: the base dereferenced `derefs` times, then field `index` of the struct,
/// tuple or tuple struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldRef {
    pub derefs: u32,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureInfo {
    /// The kind the body is checked with: the kind of the expected pointer or closure type,
    /// otherwise the kind of the enclosing function.
    pub kind: ClosureKind,
    /// Locals of enclosing scopes the body uses, in order of first use.
    pub captures: Vec<LocalId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coercion {
    /// A named function or tuple constructor to a function pointer of its kind.
    FnDefToPtr,
    /// A named function or function pointer to the closure type of the same kind.
    ToClosure,
    /// A closure that captures nothing to a function pointer of its kind.
    ClosureToPtr,
    /// `&mut T` to `&T`.
    MutToShared,
    /// An integer literal or `Int` value to an `Int` type whose range contains it, or a
    /// literal to a Rust integer type.
    IntWiden,
    /// `!` to any type.
    NeverToAny,
}
