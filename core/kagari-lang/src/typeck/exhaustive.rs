//! `match` exhaustiveness, unreachable arms and pattern refutability (compilation stage 3).
//!
//! Maranget-style usefulness with constructor splitting, as in rustc's
//! `pattern/usefulness`: HIR patterns are lowered to deconstructed patterns (a constructor
//! with sub-patterns), the arms form a matrix, and a pattern is useful when some value it
//! matches is matched by no earlier row. An arm that is not useful is unreachable; the
//! values the whole matrix misses are the witnesses of a non-exhaustive `match` or of a
//! refutable pattern.
//!
//! Constructors by type: `bool` has `true`/`false`; Rust integers and `Int<lo, hi>` have
//! integer ranges, split at the boundaries of the ranges written in the column; enums have
//! their variants; structs, tuples and fixed-length arrays have one constructor; `&T` is
//! matched through. `!` and enums without inhabited variants have no constructor. Every
//! other type (`Text`, `String`, `Decimal`, generic parameters, …) is an infinite domain:
//! a literal covers only itself, and only wildcards cover the rest. Constants used as
//! patterns are read from their initializer syntactically; an initializer that is not a
//! literal, a constructor or a tuple/record of those becomes an opaque constructor that
//! covers only itself.

use crate::db::Db;
use crate::def_map::Def;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hir::{
    self, ArrayExpr, Body, Expr, ExprId, Literal, MatchArm, Name, Pat, PatBound, PatId, Stmt,
    UnaryOp, file_hir,
};
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, Shape, item_tree};
use crate::resolve::{BodyResolution, Resolution, file_resolution};
use crate::signatures::{AdtDef, StdItems, fold_const_expr, program_signatures};
use crate::ty::{Const, FnDefId, GenericArg, IntTy, Ty};

/// Exhaustiveness and refutability diagnostics of one body.
pub fn check_body(
    db: &dyn Db,
    program: Program,
    item: ItemId,
    body: &hir::Body,
    resolution: &BodyResolution,
    inference: &InferenceResult,
) -> Vec<Diagnostic> {
    let env = DbEnv {
        db,
        program,
        signatures: program_signatures(db, program),
    };
    check_body_in(
        &env,
        item.file.path(db).clone(),
        body,
        resolution,
        inference,
    )
}

/// Exhaustiveness and refutability diagnostics of every item of `file` with a body and an
/// inference result.
#[salsa::tracked(returns(ref))]
pub fn file_pattern_checks(db: &dyn Db, program: Program, file: SourceFile) -> Vec<Diagnostic> {
    let hir = file_hir(db, file);
    let resolution = file_resolution(db, program, file);
    let mut out = Vec::new();
    for (index, body) in hir.bodies.iter().enumerate() {
        let Some(body) = body else {
            continue;
        };
        let Ok(index_u32) = u32::try_from(index) else {
            continue;
        };
        let item = ItemId {
            file,
            index: index_u32,
        };
        let Some(body_resolution) = resolution
            .items
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|item| item.body.as_ref())
        else {
            continue;
        };
        let Some(result) = inference(db, program, item) else {
            continue;
        };
        out.extend(check_body(db, program, item, body, body_resolution, result));
    }
    out
}

// ---- program access ----

/// What the checker needs to know about the program besides the body itself.
trait PatEnv {
    fn adt(&self, item: ItemId) -> Option<&AdtDef>;
    fn std_items(&self) -> &StdItems;
    fn item_name(&self, item: ItemId) -> Option<Name>;
    /// The initializer of a `const` item; `None` for anything else.
    fn const_body(&self, item: ItemId) -> Option<ConstBody<'_>>;
    /// The integer `expr` of a constant's initializer folds to; the folder's own
    /// diagnostics are dropped (signatures and const evaluation report them).
    fn fold_int(&self, init: ConstBody<'_>, expr: ExprId) -> Option<i128>;
}

/// The initializer of a `const` item, with its resolution and inference result.
#[derive(Clone, Copy)]
struct ConstBody<'a> {
    /// The `const` item.
    item: ItemId,
    body: &'a Body,
    resolution: &'a BodyResolution,
    inference: Option<&'a InferenceResult>,
}

struct DbEnv<'db> {
    db: &'db dyn Db,
    program: Program,
    signatures: &'db crate::signatures::ProgramSignatures,
}

impl PatEnv for DbEnv<'_> {
    fn adt(&self, item: ItemId) -> Option<&AdtDef> {
        self.signatures.adt(item)
    }

    fn std_items(&self) -> &StdItems {
        &self.signatures.std
    }

    fn item_name(&self, item: ItemId) -> Option<Name> {
        item_tree(self.db, item.file).item(item.index)?.name.clone()
    }

    fn fold_int(&self, init: ConstBody<'_>, expr: ExprId) -> Option<i128> {
        let mut diagnostics = Vec::new();
        fold_const_expr(
            self.db,
            self.program,
            init.item,
            &init.body.store,
            &init.resolution.store,
            expr,
            &mut diagnostics,
        )
        .value()
    }

    fn const_body(&self, item: ItemId) -> Option<ConstBody<'_>> {
        let data = item_tree(self.db, item.file).item(item.index)?;
        if !matches!(data.kind, ItemKind::Const { .. }) {
            return None;
        }
        let index = item.index as usize;
        let body = file_hir(self.db, item.file).bodies.get(index)?.as_ref()?;
        let resolution = file_resolution(self.db, self.program, item.file)
            .items
            .get(index)?
            .as_ref()?
            .body
            .as_ref()?;
        Some(ConstBody {
            item,
            body,
            resolution,
            inference: inference(self.db, self.program, item),
        })
    }
}

// ---- deconstructed patterns ----

/// An inclusive integer range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct IntRange {
    lo: i128,
    hi: i128,
}

impl IntRange {
    fn contains(self, other: IntRange) -> bool {
        self.lo <= other.lo && other.hi <= self.hi
    }
}

/// What an opaque constructor is: equal keys denote the same value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OpaqueKey {
    Str(String),
    Decimal(String),
    Int(i128),
    Bool(bool),
    /// A constant whose initializer is not a pattern.
    Const(ItemId),
    /// A pattern equal to no other (a range over an infinite domain, an unknown path).
    Unique(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Ctor {
    Wildcard,
    /// Alternatives, kept in `fields`.
    Or,
    Bool(bool),
    Int(IntRange),
    /// An enum variant by index.
    Variant(u32),
    /// The only constructor of a struct, tuple or array.
    Single,
    Opaque(OpaqueKey),
    /// Values of an infinite domain that no listed constructor covers; also selects the
    /// rows of the default matrix. Printed `_`.
    NonExhaustive,
}

/// A constructor applied to sub-patterns (one per field of the constructor).
#[derive(Clone, Debug, PartialEq, Eq)]
struct DPat {
    ctor: Ctor,
    fields: Vec<DPat>,
}

impl DPat {
    fn leaf(ctor: Ctor) -> DPat {
        DPat {
            ctor,
            fields: Vec::new(),
        }
    }

    fn wild() -> DPat {
        DPat::leaf(Ctor::Wildcard)
    }

    fn wilds(count: usize) -> Vec<DPat> {
        vec![DPat::wild(); count]
    }

    fn is_wild(&self) -> bool {
        self.ctor == Ctor::Wildcard
    }
}

/// The constructors of a type.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CtorSet {
    Bool,
    Int(IntRange),
    /// The variants of an enum, by whether each is inhabited. Empty for `!`.
    Variants(Vec<bool>),
    Single,
    /// An infinite or unknown domain.
    Opaque,
}

/// The constructors of a column, split so that each is covered entirely or not at all by
/// every row head.
struct Split {
    present: Vec<Ctor>,
    missing: Vec<Ctor>,
}

/// More witnesses than this are dropped.
const WITNESS_CAP: usize = 256;
/// Arrays longer than this are an opaque domain.
const MAX_ARRAY_FIELDS: i128 = 1024;
/// Constants referring to constants deeper than this are opaque.
const MAX_CONST_DEPTH: u32 = 32;
/// Inhabitedness is not looked for deeper than this.
const MAX_INHABITED_DEPTH: u32 = 8;

fn peel(mut ty: &Ty) -> &Ty {
    while let Ty::Ref { inner, .. } = ty {
        ty = inner;
    }
    ty
}

/// `ty` with the generic parameters of `owner` replaced by `args`.
fn subst(ty: &Ty, owner: ItemId, args: &[GenericArg]) -> Ty {
    let types = |tys: &[Ty]| tys.iter().map(|ty| subst(ty, owner, args)).collect();
    match ty {
        Ty::Param { owner: o, index } if *o == owner => match args.get(*index as usize) {
            Some(GenericArg::Type(arg)) => arg.clone(),
            _ => Ty::Error,
        },
        Ty::Bounded { lo, hi } => Ty::Bounded {
            lo: subst_const(lo, owner, args),
            hi: subst_const(hi, owner, args),
        },
        Ty::Tuple(fields) => Ty::Tuple(types(fields)),
        Ty::Array { elem, len } => Ty::Array {
            elem: Box::new(subst(elem, owner, args)),
            len: subst_const(len, owner, args),
        },
        Ty::Adt { item, args: inner } => Ty::Adt {
            item: *item,
            args: inner
                .iter()
                .map(|arg| subst_arg(arg, owner, args))
                .collect(),
        },
        Ty::Ref { mutable, inner } => Ty::Ref {
            mutable: *mutable,
            inner: Box::new(subst(inner, owner, args)),
        },
        Ty::FnPtr { kind, params, ret } => Ty::FnPtr {
            kind: *kind,
            params: types(params),
            ret: Box::new(subst(ret, owner, args)),
        },
        Ty::Closure { kind, params, ret } => Ty::Closure {
            kind: *kind,
            params: types(params),
            ret: Box::new(subst(ret, owner, args)),
        },
        Ty::Projection {
            base,
            trait_item,
            name,
        } => Ty::Projection {
            base: Box::new(subst(base, owner, args)),
            trait_item: *trait_item,
            name: name.clone(),
        },
        Ty::Error
        | Ty::Never
        | Ty::Bool
        | Ty::Int(_)
        | Ty::FnDef(_)
        | Ty::Param { .. }
        | Ty::TraitSelf { .. }
        | Ty::Infer(_) => ty.clone(),
    }
}

fn subst_const(value: &Const, owner: ItemId, args: &[GenericArg]) -> Const {
    match value {
        Const::Param { owner: o, index } if *o == owner => match args.get(*index as usize) {
            Some(GenericArg::Const(arg)) => arg.clone(),
            _ => Const::Error,
        },
        _ => value.clone(),
    }
}

fn subst_arg(arg: &GenericArg, owner: ItemId, args: &[GenericArg]) -> GenericArg {
    match arg {
        GenericArg::Type(ty) => GenericArg::Type(subst(ty, owner, args)),
        GenericArg::Const(value) => GenericArg::Const(subst_const(value, owner, args)),
    }
}

/// Splits `range` at the boundaries of `heads`: every segment lies inside or outside each
/// head. Returns the segments in order, with whether some head covers them.
fn split_int(range: IntRange, heads: &[IntRange]) -> Vec<(IntRange, bool)> {
    let mut bounds = vec![range.lo];
    for head in heads {
        let lo = head.lo.max(range.lo);
        let hi = head.hi.min(range.hi);
        if lo > hi {
            continue;
        }
        if lo > range.lo {
            bounds.push(lo);
        }
        if let Some(next) = hi.checked_add(1)
            && next <= range.hi
        {
            bounds.push(next);
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    let ends = bounds
        .iter()
        .skip(1)
        .map(|next| next.saturating_sub(1))
        .chain([range.hi]);
    bounds
        .iter()
        .zip(ends)
        .map(|(&lo, hi)| {
            let segment = IntRange { lo, hi };
            let covered = heads.iter().any(|head| head.contains(segment));
            (segment, covered)
        })
        .collect()
}

/// Replaces rows whose head is an or-pattern by one row per alternative.
fn expand_or(rows: &[Vec<DPat>]) -> Vec<Vec<DPat>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        expand_row(row, &mut out);
    }
    out
}

fn expand_row(row: &[DPat], out: &mut Vec<Vec<DPat>>) {
    match row.split_first() {
        Some((head, rest)) if head.ctor == Ctor::Or => {
            for alt in &head.fields {
                let mut expanded = Vec::with_capacity(row.len());
                expanded.push(alt.clone());
                expanded.extend_from_slice(rest);
                expand_row(&expanded, out);
            }
        }
        _ => out.push(row.to_vec()),
    }
}

/// Whether a row head with constructor `head` matches every value of `ctor`.
fn covers(head: &Ctor, ctor: &Ctor) -> bool {
    match (head, ctor) {
        (Ctor::Wildcard, _) => true,
        (Ctor::Int(head), Ctor::Int(ctor)) => head.contains(*ctor),
        (Ctor::NonExhaustive, _) | (_, Ctor::NonExhaustive) => false,
        (head, ctor) => head == ctor,
    }
}

/// The row specialized to `ctor`: the head replaced by its `arity` fields, or `None` when
/// the head does not cover `ctor`.
fn specialize_row(row: &[DPat], ctor: &Ctor, arity: usize) -> Option<Vec<DPat>> {
    let (head, rest) = row.split_first()?;
    if !covers(&head.ctor, ctor) {
        return None;
    }
    let mut out = Vec::with_capacity(arity.saturating_add(rest.len()));
    if head.is_wild() {
        out.extend(DPat::wilds(arity));
    } else {
        out.extend(head.fields.iter().cloned());
    }
    out.extend_from_slice(rest);
    Some(out)
}

fn specialize(rows: &[Vec<DPat>], ctor: &Ctor, arity: usize) -> Vec<Vec<DPat>> {
    rows.iter()
        .filter_map(|row| specialize_row(row, ctor, arity))
        .collect()
}

/// Sort key of a constructor, for witnesses in declaration order.
fn ctor_order(ctor: &Ctor) -> (u8, i128) {
    match ctor {
        Ctor::Bool(value) => (0, i128::from(*value)),
        Ctor::Int(range) => (0, range.lo),
        Ctor::Variant(index) => (0, i128::from(*index)),
        Ctor::Single => (0, 0),
        Ctor::Opaque(_) => (1, 0),
        Ctor::Wildcard | Ctor::Or | Ctor::NonExhaustive => (2, 0),
    }
}

// ---- the usefulness algorithm ----

struct Cx<'a> {
    env: &'a dyn PatEnv,
}

impl Cx<'_> {
    fn ctor_set(&self, ty: &Ty) -> CtorSet {
        match peel(ty) {
            Ty::Bool => CtorSet::Bool,
            Ty::Int(int) => CtorSet::Int(IntRange {
                lo: IntTy::min(*int),
                hi: IntTy::max(*int),
            }),
            Ty::Bounded {
                lo: Const::Value(lo),
                hi: Const::Value(hi),
            } if lo <= hi => CtorSet::Int(IntRange { lo: *lo, hi: *hi }),
            Ty::Never => CtorSet::Variants(Vec::new()),
            Ty::Tuple(_) => CtorSet::Single,
            Ty::Array {
                len: Const::Value(len),
                ..
            } if (0..=MAX_ARRAY_FIELDS).contains(len) => CtorSet::Single,
            Ty::Adt { item, args } => match self.env.adt(*item) {
                Some(adt) if !adt.builtin && adt.is_enum => CtorSet::Variants(
                    adt.variants
                        .iter()
                        .map(|variant| {
                            !variant
                                .fields
                                .iter()
                                .any(|field| self.is_uninhabited(&subst(&field.ty, *item, args), 0))
                        })
                        .collect(),
                ),
                Some(adt) if !adt.builtin => CtorSet::Single,
                _ => CtorSet::Opaque,
            },
            _ => CtorSet::Opaque,
        }
    }

    /// Whether `ty` certainly has no value: `!`, enums without inhabited variants, and
    /// tuples and structs with such a field. References are inhabited.
    fn is_uninhabited(&self, ty: &Ty, depth: u32) -> bool {
        if depth > MAX_INHABITED_DEPTH {
            return false;
        }
        let depth = depth.saturating_add(1);
        match ty {
            Ty::Never => true,
            Ty::Tuple(fields) => fields.iter().any(|field| self.is_uninhabited(field, depth)),
            Ty::Array {
                elem,
                len: Const::Value(len),
            } => *len > 0 && self.is_uninhabited(elem, depth),
            Ty::Adt { item, args } => match self.env.adt(*item) {
                Some(adt) if !adt.builtin => {
                    let variant_empty = |variant: &crate::signatures::VariantDef| {
                        variant
                            .fields
                            .iter()
                            .any(|field| self.is_uninhabited(&subst(&field.ty, *item, args), depth))
                    };
                    if adt.is_enum {
                        adt.variants.iter().all(variant_empty)
                    } else {
                        adt.variants.first().is_some_and(variant_empty)
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// The types of the fields of `ctor` on `ty`.
    fn field_tys(&self, ctor: &Ctor, ty: &Ty) -> Vec<Ty> {
        match (ctor, peel(ty)) {
            (Ctor::Single, Ty::Tuple(fields)) => fields.clone(),
            (
                Ctor::Single,
                Ty::Array {
                    elem,
                    len: Const::Value(len),
                },
            ) => {
                let count = usize::try_from(*len).unwrap_or(0);
                vec![elem.as_ref().clone(); count]
            }
            (Ctor::Single, Ty::Adt { item, args }) => self.variant_field_tys(*item, 0, args),
            (Ctor::Variant(index), Ty::Adt { item, args }) => {
                self.variant_field_tys(*item, *index, args)
            }
            _ => Vec::new(),
        }
    }

    fn variant_field_tys(&self, item: ItemId, index: u32, args: &[GenericArg]) -> Vec<Ty> {
        self.env
            .adt(item)
            .and_then(|adt| adt.variants.get(index as usize))
            .map(|variant| {
                variant
                    .fields
                    .iter()
                    .map(|field| subst(&field.ty, item, args))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Splits the constructors of `ty` by the non-wildcard row heads `heads`.
    fn split(&self, ty: &Ty, heads: &[&Ctor]) -> Split {
        let mut present: Vec<Ctor> = Vec::new();
        let mut missing: Vec<Ctor> = Vec::new();
        match self.ctor_set(ty) {
            CtorSet::Bool => {
                for value in [false, true] {
                    let ctor = Ctor::Bool(value);
                    if heads.contains(&&ctor) {
                        present.push(ctor);
                    } else {
                        missing.push(ctor);
                    }
                }
            }
            CtorSet::Int(range) => {
                let ranges: Vec<IntRange> = heads
                    .iter()
                    .filter_map(|head| match head {
                        Ctor::Int(range) => Some(*range),
                        _ => None,
                    })
                    .collect();
                for (segment, covered) in split_int(range, &ranges) {
                    if covered {
                        present.push(Ctor::Int(segment));
                    } else {
                        missing.push(Ctor::Int(segment));
                    }
                }
            }
            CtorSet::Variants(inhabited) => {
                for (index, inhabited) in inhabited.into_iter().enumerate() {
                    let Ok(index) = u32::try_from(index) else {
                        break;
                    };
                    let ctor = Ctor::Variant(index);
                    if heads.contains(&&ctor) {
                        present.push(ctor);
                    } else if inhabited {
                        missing.push(ctor);
                    }
                }
            }
            CtorSet::Single => {
                if heads.contains(&&Ctor::Single) {
                    present.push(Ctor::Single);
                } else if !self.is_uninhabited(peel(ty), 0) {
                    missing.push(Ctor::Single);
                }
            }
            CtorSet::Opaque => missing.push(Ctor::NonExhaustive),
        }
        for head in heads {
            if matches!(head, Ctor::Opaque(_)) && !present.contains(head) {
                present.push((*head).clone());
            }
        }
        Split { present, missing }
    }

    /// Whether some value matched by `row` is matched by none of `rows`. `tys` are the
    /// column types.
    fn is_useful(&self, rows: &[Vec<DPat>], row: &[DPat], tys: &[Ty]) -> bool {
        let (Some(head), Some(ty)) = (row.first(), tys.first()) else {
            return rows.is_empty();
        };
        let rest = row.get(1..).unwrap_or_default();
        let rest_tys = tys.get(1..).unwrap_or_default();
        if head.ctor == Ctor::Or {
            return head.fields.iter().any(|alt| {
                let mut expanded = Vec::with_capacity(row.len());
                expanded.push(alt.clone());
                expanded.extend_from_slice(rest);
                self.is_useful(rows, &expanded, tys)
            });
        }
        let rows = expand_or(rows);
        let heads: Vec<&Ctor> = rows
            .iter()
            .filter_map(|row| row.first())
            .map(|pat| &pat.ctor)
            .filter(|ctor| **ctor != Ctor::Wildcard)
            .collect();
        let ctors = match &head.ctor {
            Ctor::Wildcard => {
                let split = self.split(ty, &heads);
                if split.missing.is_empty() && !split.present.is_empty() {
                    split.present
                } else {
                    let default = specialize(&rows, &Ctor::NonExhaustive, 0);
                    return self.is_useful(&default, rest, rest_tys);
                }
            }
            Ctor::Int(range) => {
                let ranges: Vec<IntRange> = heads
                    .iter()
                    .filter_map(|head| match head {
                        Ctor::Int(range) => Some(*range),
                        _ => None,
                    })
                    .collect();
                split_int(*range, &ranges)
                    .into_iter()
                    .map(|(segment, _)| Ctor::Int(segment))
                    .collect()
            }
            ctor => vec![ctor.clone()],
        };
        ctors.iter().any(|ctor| {
            let mut field_tys = self.field_tys(ctor, ty);
            let arity = field_tys.len();
            let Some(specialized_row) = specialize_row(row, ctor, arity) else {
                return false;
            };
            let specialized = specialize(&rows, ctor, arity);
            field_tys.extend_from_slice(rest_tys);
            self.is_useful(&specialized, &specialized_row, &field_tys)
        })
    }

    /// Value stacks (one pattern per column of `tys`) that no row of `rows` matches.
    /// `top` is set for the scrutinee column: its missing constructors are listed one by
    /// one even when none is present.
    fn witnesses(&self, rows: &[Vec<DPat>], tys: &[Ty], top: bool) -> Vec<Vec<DPat>> {
        let Some(ty) = tys.first() else {
            return if rows.is_empty() {
                vec![Vec::new()]
            } else {
                Vec::new()
            };
        };
        let rest_tys = tys.get(1..).unwrap_or_default();
        let rows = expand_or(rows);
        let heads: Vec<&Ctor> = rows
            .iter()
            .filter_map(|row| row.first())
            .map(|pat| &pat.ctor)
            .filter(|ctor| **ctor != Ctor::Wildcard)
            .collect();
        let split = self.split(ty, &heads);
        let mut out: Vec<Vec<DPat>> = Vec::new();
        for ctor in &split.present {
            let mut field_tys = self.field_tys(ctor, ty);
            let arity = field_tys.len();
            let specialized = specialize(&rows, ctor, arity);
            field_tys.extend_from_slice(rest_tys);
            for mut witness in self.witnesses(&specialized, &field_tys, false) {
                let rest = witness.split_off(arity.min(witness.len()));
                let mut stack = Vec::with_capacity(rest.len().saturating_add(1));
                stack.push(DPat {
                    ctor: ctor.clone(),
                    fields: witness,
                });
                stack.extend(rest);
                out.push(stack);
                if out.len() >= WITNESS_CAP {
                    return out;
                }
            }
        }
        if !split.missing.is_empty() {
            let default = specialize(&rows, &Ctor::NonExhaustive, 0);
            let rest_witnesses = self.witnesses(&default, rest_tys, false);
            if !rest_witnesses.is_empty() {
                let heads: Vec<DPat> = if top || !split.present.is_empty() {
                    split
                        .missing
                        .iter()
                        .map(|ctor| DPat {
                            ctor: ctor.clone(),
                            fields: DPat::wilds(self.field_tys(ctor, ty).len()),
                        })
                        .collect()
                } else {
                    vec![DPat::wild()]
                };
                for head in heads {
                    for rest in &rest_witnesses {
                        let mut stack = Vec::with_capacity(rest.len().saturating_add(1));
                        stack.push(head.clone());
                        stack.extend(rest.iter().cloned());
                        out.push(stack);
                        if out.len() >= WITNESS_CAP {
                            return out;
                        }
                    }
                }
            }
        }
        out.sort_by_key(|stack| stack.first().map(|pat| ctor_order(&pat.ctor)));
        out
    }

    // ---- witness printing ----

    fn print(&self, pat: &DPat, ty: &Ty) -> String {
        match &pat.ctor {
            Ctor::Wildcard | Ctor::Or | Ctor::NonExhaustive => "_".to_owned(),
            Ctor::Bool(value) => value.to_string(),
            Ctor::Int(range) => print_int(*range, peel(ty)),
            Ctor::Opaque(key) => match key {
                OpaqueKey::Str(text) => format!("{text:?}"),
                OpaqueKey::Decimal(text) => text.clone(),
                OpaqueKey::Int(value) => value.to_string(),
                OpaqueKey::Bool(value) => value.to_string(),
                OpaqueKey::Const(item) => self
                    .env
                    .item_name(*item)
                    .map_or_else(|| "_".to_owned(), |name| name.to_string()),
                OpaqueKey::Unique(_) => "_".to_owned(),
            },
            Ctor::Single => match peel(ty) {
                Ty::Tuple(tys) => {
                    let fields = self.print_fields(&pat.fields, tys);
                    if fields.len() == 1 {
                        format!("({},)", fields.join(", "))
                    } else {
                        format!("({})", fields.join(", "))
                    }
                }
                Ty::Array { .. } => {
                    let tys = self.field_tys(&pat.ctor, ty);
                    format!("[{}]", self.print_fields(&pat.fields, &tys).join(", "))
                }
                Ty::Adt { item, .. } => self.print_variant(*item, 0, false, pat, ty),
                _ => "_".to_owned(),
            },
            Ctor::Variant(index) => match peel(ty) {
                Ty::Adt { item, .. } => self.print_variant(*item, *index, true, pat, ty),
                _ => "_".to_owned(),
            },
        }
    }

    fn print_fields(&self, fields: &[DPat], tys: &[Ty]) -> Vec<String> {
        fields
            .iter()
            .enumerate()
            .map(|(index, field)| match tys.get(index) {
                Some(ty) => self.print(field, ty),
                None => "_".to_owned(),
            })
            .collect()
    }

    fn print_variant(
        &self,
        item: ItemId,
        index: u32,
        is_enum: bool,
        pat: &DPat,
        ty: &Ty,
    ) -> String {
        let adt_name = self
            .env
            .item_name(item)
            .map_or_else(|| "_".to_owned(), |name| name.to_string());
        let Some(variant) = self
            .env
            .adt(item)
            .and_then(|adt| adt.variants.get(index as usize))
        else {
            return "_".to_owned();
        };
        let std = self.env.std_items();
        let name = if !is_enum {
            adt_name
        } else if std.option == Some(item) || std.result == Some(item) {
            variant.name.to_string()
        } else {
            format!("{adt_name}::{}", variant.name)
        };
        let tys = self.field_tys(&pat.ctor, ty);
        let fields = self.print_fields(&pat.fields, &tys);
        match &variant.shape {
            Shape::Unit => name,
            Shape::Tuple(_) => format!("{name}({})", fields.join(", ")),
            Shape::Record(names) => {
                let shown: Vec<String> = names
                    .iter()
                    .zip(&pat.fields)
                    .zip(&fields)
                    .filter(|((_, pat), _)| !pat.is_wild())
                    .map(|((field, _), text)| format!("{field}: {text}"))
                    .collect();
                if shown.is_empty() {
                    format!("{name} {{ .. }}")
                } else if shown.len() < names.len() {
                    format!("{name} {{ {}, .. }}", shown.join(", "))
                } else {
                    format!("{name} {{ {} }}", shown.join(", "))
                }
            }
        }
    }
}

/// `5`, `6..=10`, `i32::MIN..=-1`, `6..=i32::MAX`.
fn print_int(range: IntRange, ty: &Ty) -> String {
    let bound = |value: i128| match ty {
        Ty::Int(int) if value == IntTy::max(*int) => format!("{}::MAX", int.name()),
        Ty::Int(int) if value == IntTy::min(*int) && value != 0 => {
            format!("{}::MIN", int.name())
        }
        _ => value.to_string(),
    };
    if range.lo == range.hi {
        bound(range.lo)
    } else {
        format!("{}..={}", bound(range.lo), bound(range.hi))
    }
}

/// "`a`", "`a` and `b`", "`a`, `b` and `c`", "`a`, `b`, `c` and 2 more".
fn witness_list(witnesses: &[String]) -> String {
    const SHOWN: usize = 3;
    let quoted: Vec<String> = witnesses
        .iter()
        .take(SHOWN)
        .map(|witness| format!("`{witness}`"))
        .collect();
    let more = witnesses.len().saturating_sub(SHOWN);
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, init)) if more == 0 => format!("{} and {last}", init.join(", ")),
        Some(_) => format!("{} and {more} more", quoted.join(", ")),
    }
}

// ---- checking a body ----

/// Where an irrefutable pattern is required.
#[derive(Clone, Copy)]
enum Irrefutable {
    Let,
    FnParam,
    ClosureParam,
    For,
}

impl Irrefutable {
    fn describe(self) -> &'static str {
        match self {
            Irrefutable::Let => "local binding",
            Irrefutable::FnParam => "function argument",
            Irrefutable::ClosureParam => "closure argument",
            Irrefutable::For => "`for` loop binding",
        }
    }
}

/// A range bound read from a pattern.
enum BoundValue {
    Value(i128),
    /// A constant that does not fold: the range covers only itself.
    Opaque,
    /// Already reported.
    Error,
}

fn check_body_in(
    env: &dyn PatEnv,
    file: FilePath,
    body: &Body,
    resolution: &BodyResolution,
    inference: &InferenceResult,
) -> Vec<Diagnostic> {
    let mut checker = Checker {
        cx: Cx { env },
        file,
        body,
        resolution,
        inference,
        next_unique: 0,
        diagnostics: Vec::new(),
    };
    checker.run();
    checker.diagnostics
}

struct Checker<'a> {
    cx: Cx<'a>,
    file: FilePath,
    body: &'a Body,
    resolution: &'a BodyResolution,
    inference: &'a InferenceResult,
    next_unique: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    fn run(&mut self) {
        let body: &'a Body = self.body;
        for &param in &body.params {
            self.check_irrefutable(param, Irrefutable::FnParam);
        }
        for (_, expr) in body.store.exprs.iter() {
            match expr {
                Expr::Match { scrutinee, arms } => self.check_match(*scrutinee, arms),
                Expr::Block { stmts, .. } => {
                    for stmt in stmts {
                        if let Stmt::Let { pat, .. } = stmt {
                            self.check_irrefutable(*pat, Irrefutable::Let);
                        }
                    }
                }
                Expr::For { pat, .. } => self.check_irrefutable(*pat, Irrefutable::For),
                Expr::Closure { params, .. } => {
                    for &(param, _) in params {
                        self.check_irrefutable(param, Irrefutable::ClosureParam);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_match(&mut self, scrutinee: ExprId, arms: &[MatchArm]) {
        let Some(ty) = self.inference.expr_types.get(scrutinee) else {
            return;
        };
        if ty.references_error() || ty.has_infer() {
            return;
        }
        let mut lowered = Vec::with_capacity(arms.len());
        for arm in arms {
            let Some(pat) = self.lower_pat(arm.pat, ty) else {
                return;
            };
            lowered.push(pat);
        }
        let tys = [ty.clone()];
        let mut rows: Vec<Vec<DPat>> = Vec::new();
        for (arm, pat) in arms.iter().zip(lowered) {
            let row = vec![pat];
            if !self.cx.is_useful(&rows, &row, &tys) {
                self.diagnostics.push(Diagnostic::new(
                    self.file.clone(),
                    self.body.store.pat_range(arm.pat),
                    DiagnosticCode::TypeUnreachablePattern,
                    "unreachable pattern",
                ));
            }
            if arm.guard.is_none() {
                rows.push(row);
            }
        }
        if let Some(list) = self.missing(&rows, ty) {
            self.diagnostics.push(Diagnostic::new(
                self.file.clone(),
                self.body.store.expr_range(scrutinee),
                DiagnosticCode::TypeNonExhaustiveMatch,
                format!("non-exhaustive patterns: {list} not covered"),
            ));
        }
    }

    fn check_irrefutable(&mut self, pat: PatId, place: Irrefutable) {
        let Some(ty) = self.inference.pat_types.get(pat) else {
            return;
        };
        let Some(lowered) = self.lower_pat(pat, ty) else {
            return;
        };
        if let Some(list) = self.missing(&[vec![lowered]], ty) {
            self.diagnostics.push(Diagnostic::new(
                self.file.clone(),
                self.body.store.pat_range(pat),
                DiagnosticCode::TypeRefutablePattern,
                format!(
                    "refutable pattern in {}: {list} not covered",
                    place.describe()
                ),
            ));
        }
    }

    /// The witness list of the values `rows` misses, if any.
    fn missing(&self, rows: &[Vec<DPat>], ty: &Ty) -> Option<String> {
        let mut printed: Vec<String> = Vec::new();
        for witness in self.cx.witnesses(rows, std::slice::from_ref(ty), true) {
            let Some(pat) = witness.first() else {
                continue;
            };
            let text = self.cx.print(pat, ty);
            if !printed.contains(&text) {
                printed.push(text);
            }
        }
        (!printed.is_empty()).then(|| witness_list(&printed))
    }

    fn unique(&mut self) -> DPat {
        let key = self.next_unique;
        self.next_unique = self.next_unique.saturating_add(1);
        DPat::leaf(Ctor::Opaque(OpaqueKey::Unique(key)))
    }

    // ---- lowering HIR patterns ----

    /// The deconstructed pattern of `pat` matched against `ty`; `None` when the pattern or
    /// its type has an error (the construct is skipped).
    fn lower_pat(&mut self, pat: PatId, ty: &Ty) -> Option<DPat> {
        if ty.references_error() || ty.has_infer() {
            return None;
        }
        if let Some(pat_ty) = self.inference.pat_types.get(pat)
            && (pat_ty.references_error() || pat_ty.has_infer())
        {
            return None;
        }
        let store: &'a hir::ExprStore = &self.body.store;
        match store.pat(pat) {
            Pat::Missing => None,
            Pat::Wild | Pat::Rest => Some(DPat::wild()),
            Pat::Bind { .. } => {
                if self.inference.pat_ctors.contains_key(&pat)
                    || self.resolution.store.pats.contains_key(&pat)
                {
                    self.lower_path_pat(pat, ty)
                } else {
                    Some(DPat::wild())
                }
            }
            Pat::Path(_) => self.lower_path_pat(pat, ty),
            Pat::Lit { lit, negated } => self.cx.lower_lit(lit, *negated, ty),
            Pat::Range {
                start,
                end,
                inclusive,
            } => self.lower_range(pat, start.as_ref(), end.as_ref(), *inclusive, ty),
            Pat::Tuple(pats) => {
                let Ty::Tuple(tys) = peel(ty) else {
                    return None;
                };
                let fields = self.lower_seq(pats, tys)?;
                Some(DPat {
                    ctor: Ctor::Single,
                    fields,
                })
            }
            Pat::TupleStruct { pats, .. } => {
                let (ctor, tys) = self.pat_ctor(pat, ty)?;
                let fields = self.lower_seq(pats, &tys)?;
                Some(DPat { ctor, fields })
            }
            Pat::Record { fields, .. } => {
                let (ctor, tys) = self.pat_ctor(pat, ty)?;
                let names = self.ctor_field_names(&ctor, ty)?;
                let mut lowered = DPat::wilds(tys.len());
                for (name, sub) in fields {
                    let index = names.iter().position(|field| field == name)?;
                    let sub_ty = tys.get(index)?;
                    let sub = self.lower_pat(*sub, sub_ty)?;
                    *lowered.get_mut(index)? = sub;
                }
                Some(DPat {
                    ctor,
                    fields: lowered,
                })
            }
            Pat::Or(alternatives) => {
                if alternatives.is_empty() {
                    return None;
                }
                let mut fields = Vec::with_capacity(alternatives.len());
                for &alternative in alternatives {
                    let lowered = self.lower_pat(alternative, ty)?;
                    if lowered.ctor == Ctor::Or {
                        fields.extend(lowered.fields);
                    } else {
                        fields.push(lowered);
                    }
                }
                Some(DPat {
                    ctor: Ctor::Or,
                    fields,
                })
            }
        }
    }

    /// Sub-patterns of a tuple or tuple struct, with at most one `..`.
    fn lower_seq(&mut self, pats: &[PatId], tys: &[Ty]) -> Option<Vec<DPat>> {
        let store: &'a hir::ExprStore = &self.body.store;
        let is_rest = |pat: &PatId| matches!(store.pat(*pat), Pat::Rest);
        let (prefix, suffix) = match pats.iter().position(is_rest) {
            Some(position) => {
                let prefix = pats.get(..position)?;
                let suffix = pats.get(position.checked_add(1)?..)?;
                if suffix.iter().any(is_rest) || prefix.len().checked_add(suffix.len())? > tys.len()
                {
                    return None;
                }
                (prefix, suffix)
            }
            None if pats.len() == tys.len() => (pats, &[][..]),
            None => return None,
        };
        let suffix_start = tys.len().checked_sub(suffix.len())?;
        let mut fields = DPat::wilds(tys.len());
        let positions = (0..prefix.len()).chain(suffix_start..tys.len());
        for (index, &sub) in positions.zip(prefix.iter().chain(suffix)) {
            let sub_ty = tys.get(index)?;
            let lowered = self.lower_pat(sub, sub_ty)?;
            *fields.get_mut(index)? = lowered;
        }
        Some(fields)
    }

    /// The constructor a struct/variant pattern names, checked against `ty`, with its field
    /// types.
    fn pat_ctor(&self, pat: PatId, ty: &Ty) -> Option<(Ctor, Vec<Ty>)> {
        let (adt, variant) = match self.inference.pat_ctors.get(&pat) {
            Some(ctor) => (ctor.adt, ctor.variant),
            None => match self.resolution.store.pats.get(&pat)? {
                Resolution::Def(Def::Variant { enum_item, index }) => (*enum_item, Some(*index)),
                Resolution::Def(Def::Item(item)) => (*item, None),
                _ => return None,
            },
        };
        self.cx.ctor_for(adt, variant, ty)
    }

    fn ctor_field_names(&self, ctor: &Ctor, ty: &Ty) -> Option<Vec<Name>> {
        let Ty::Adt { item, .. } = peel(ty) else {
            return None;
        };
        let index = match ctor {
            Ctor::Variant(index) => *index,
            _ => 0,
        };
        let variant = self.cx.env.adt(*item)?.variants.get(index as usize)?;
        variant
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect()
    }

    /// A path pattern, or a binding that names a unit struct, unit variant or constant.
    fn lower_path_pat(&mut self, pat: PatId, ty: &Ty) -> Option<DPat> {
        if self.inference.pat_ctors.contains_key(&pat) {
            let (ctor, tys) = self.pat_ctor(pat, ty)?;
            return tys.is_empty().then(|| DPat::leaf(ctor));
        }
        match self.resolution.store.pats.get(&pat)? {
            Resolution::Def(Def::Variant { .. }) => {
                let (ctor, tys) = self.pat_ctor(pat, ty)?;
                tys.is_empty().then(|| DPat::leaf(ctor))
            }
            Resolution::Def(Def::Item(item)) if self.cx.env.adt(*item).is_some() => {
                let (ctor, tys) = self.pat_ctor(pat, ty)?;
                tys.is_empty().then(|| DPat::leaf(ctor))
            }
            Resolution::Def(Def::Item(item)) | Resolution::Assoc(item) => {
                Some(match self.cx.env.const_body(*item) {
                    Some(_) => self.cx.lower_const_item(*item, ty, 0),
                    None => self.unique(),
                })
            }
            Resolution::Error => None,
            _ => Some(self.unique()),
        }
    }

    fn lower_range(
        &mut self,
        pat: PatId,
        start: Option<&PatBound>,
        end: Option<&PatBound>,
        inclusive: bool,
        ty: &Ty,
    ) -> Option<DPat> {
        let CtorSet::Int(domain) = self.cx.ctor_set(ty) else {
            return Some(self.unique());
        };
        let lo = match start {
            None => BoundValue::Value(domain.lo),
            Some(bound) => self.bound_value(pat, 0, bound),
        };
        let hi = match end {
            None => BoundValue::Value(domain.hi),
            Some(bound) => match self.bound_value(pat, 1, bound) {
                BoundValue::Value(value) if !inclusive => match value.checked_sub(1) {
                    Some(value) => BoundValue::Value(value),
                    None => BoundValue::Error,
                },
                other => other,
            },
        };
        let (lo, hi) = match (lo, hi) {
            (BoundValue::Error, _) | (_, BoundValue::Error) => return None,
            (BoundValue::Opaque, _) | (_, BoundValue::Opaque) => return Some(self.unique()),
            (BoundValue::Value(lo), BoundValue::Value(hi)) => (lo, hi),
        };
        let range = IntRange { lo, hi };
        (lo <= hi && domain.contains(range)).then(|| DPat::leaf(Ctor::Int(range)))
    }

    fn bound_value(&self, pat: PatId, slot: u8, bound: &PatBound) -> BoundValue {
        match bound {
            PatBound::Lit {
                lit: Literal::Int { value, .. },
                negated,
            } => match int_value(*value, *negated) {
                Some(value) => BoundValue::Value(value),
                None => BoundValue::Error,
            },
            PatBound::Lit { .. } => BoundValue::Error,
            PatBound::Path(_) => match self.resolution.store.range_bounds.get(&(pat, slot)) {
                Some(Resolution::Def(Def::Item(item)) | Resolution::Assoc(item)) => {
                    match self.cx.const_int(*item) {
                        Some(value) => BoundValue::Value(value),
                        None => BoundValue::Opaque,
                    }
                }
                Some(Resolution::Error) | None => BoundValue::Error,
                Some(_) => BoundValue::Opaque,
            },
        }
    }
}

fn int_value(value: u128, negated: bool) -> Option<i128> {
    if negated {
        // `-170141183460469231731687303715884105728` is `i128::MIN`.
        0i128.checked_sub_unsigned(value)
    } else {
        i128::try_from(value).ok()
    }
}

impl Cx<'_> {
    /// The constructor of struct `adt` (`variant` `None`) or of its variant, when `ty` is
    /// that ADT, with the constructor's field types.
    fn ctor_for(&self, adt: ItemId, variant: Option<u32>, ty: &Ty) -> Option<(Ctor, Vec<Ty>)> {
        let Ty::Adt { item, .. } = peel(ty) else {
            return None;
        };
        if *item != adt {
            return None;
        }
        let def = self.env.adt(adt)?;
        if def.builtin {
            return None;
        }
        let ctor = match (def.is_enum, variant) {
            (true, Some(index)) if (index as usize) < def.variants.len() => Ctor::Variant(index),
            (false, None) => Ctor::Single,
            _ => return None,
        };
        let tys = self.field_tys(&ctor, ty);
        Some((ctor, tys))
    }

    /// A literal pattern against `ty`; `None` when the literal does not fit the type.
    fn lower_lit(&self, lit: &Literal, negated: bool, ty: &Ty) -> Option<DPat> {
        let ctor = match (self.ctor_set(ty), lit) {
            (CtorSet::Bool, Literal::Bool(value)) if !negated => Ctor::Bool(*value),
            (CtorSet::Int(domain), Literal::Int { value, .. }) => {
                let value = int_value(*value, negated)?;
                let range = IntRange {
                    lo: value,
                    hi: value,
                };
                if !domain.contains(range) {
                    return None;
                }
                Ctor::Int(range)
            }
            (CtorSet::Opaque, lit) => Ctor::Opaque(match lit {
                Literal::Int { value, .. } => OpaqueKey::Int(int_value(*value, negated)?),
                Literal::Decimal(text) if negated => OpaqueKey::Decimal(format!("-{text}")),
                Literal::Decimal(text) => OpaqueKey::Decimal(text.to_string()),
                Literal::String(text) if !negated => OpaqueKey::Str(text.clone()),
                Literal::Bool(value) if !negated => OpaqueKey::Bool(*value),
                Literal::String(_) | Literal::Bool(_) => return None,
            }),
            _ => return None,
        };
        Some(DPat::leaf(ctor))
    }

    // ---- constants as patterns ----

    /// The pattern equivalent to `const item` against `ty`: integers folded by the stage 3
    /// constant folder, other initializers read syntactically, or an opaque constructor
    /// covering only this constant.
    fn lower_const_item(&self, item: ItemId, ty: &Ty, depth: u32) -> DPat {
        let opaque = DPat::leaf(Ctor::Opaque(OpaqueKey::Const(item)));
        if depth > MAX_CONST_DEPTH {
            return opaque;
        }
        let Some(init) = self.env.const_body(item) else {
            return opaque;
        };
        self.lower_const_expr(init, init.body.value, ty, depth)
            .unwrap_or(opaque)
    }

    fn lower_const_expr(
        &self,
        init: ConstBody<'_>,
        expr: ExprId,
        ty: &Ty,
        depth: u32,
    ) -> Option<DPat> {
        if let CtorSet::Int(domain) = self.ctor_set(ty) {
            let value = self.env.fold_int(init, expr)?;
            let range = IntRange {
                lo: value,
                hi: value,
            };
            return domain.contains(range).then(|| DPat::leaf(Ctor::Int(range)));
        }
        let store = &init.body.store;
        match store.expr(expr) {
            Expr::Literal(lit) => self.lower_lit(lit, false, ty),
            Expr::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } => match store.expr(*inner) {
                Expr::Literal(lit @ Literal::Int { .. }) => self.lower_lit(lit, true, ty),
                _ => None,
            },
            Expr::Block {
                stmts,
                tail: Some(tail),
                ..
            } if stmts.is_empty() => self.lower_const_expr(init, *tail, ty, depth),
            Expr::Path(_) => {
                let target = self.const_path_target(init, expr)?;
                match target {
                    PathTarget::Ctor(adt, variant) => {
                        let (ctor, tys) = self.ctor_for(adt, variant, ty)?;
                        tys.is_empty().then(|| DPat::leaf(ctor))
                    }
                    PathTarget::Const(item) => {
                        Some(self.lower_const_item(item, ty, depth.saturating_add(1)))
                    }
                }
            }
            Expr::Tuple(elems) => {
                let Ty::Tuple(tys) = peel(ty) else {
                    return None;
                };
                let fields = self.lower_const_fields(init, elems, tys, depth)?;
                Some(DPat {
                    ctor: Ctor::Single,
                    fields,
                })
            }
            Expr::Array(ArrayExpr::List(elems)) => {
                if !matches!(peel(ty), Ty::Array { .. }) || self.ctor_set(ty) != CtorSet::Single {
                    return None;
                }
                let tys = self.field_tys(&Ctor::Single, ty);
                let fields = self.lower_const_fields(init, elems, &tys, depth)?;
                Some(DPat {
                    ctor: Ctor::Single,
                    fields,
                })
            }
            Expr::Call { callee, args } => {
                let (adt, variant) = self.const_callee(init, *callee)?;
                let (ctor, tys) = self.ctor_for(adt, variant, ty)?;
                let fields = self.lower_const_fields(init, args, &tys, depth)?;
                Some(DPat { ctor, fields })
            }
            Expr::Record { fields, .. } => {
                let (adt, variant) = match init.inference.and_then(|inf| inf.records.get(&expr)) {
                    Some(ctor) => (ctor.adt, ctor.variant),
                    None => match init.resolution.store.exprs.get(&expr)? {
                        Resolution::Def(Def::Variant { enum_item, index }) => {
                            (*enum_item, Some(*index))
                        }
                        Resolution::Def(Def::Item(item)) => (*item, None),
                        _ => return None,
                    },
                };
                let (ctor, tys) = self.ctor_for(adt, variant, ty)?;
                let index = match ctor {
                    Ctor::Variant(index) => index,
                    _ => 0,
                };
                let variant = self.env.adt(adt)?.variants.get(index as usize)?;
                let mut lowered: Vec<Option<DPat>> = vec![None; tys.len()];
                for (name, value) in fields {
                    let position = variant
                        .fields
                        .iter()
                        .position(|field| field.name.as_ref() == Some(name))?;
                    let field_ty = tys.get(position)?;
                    let pat = self.lower_const_expr(init, *value, field_ty, depth)?;
                    *lowered.get_mut(position)? = Some(pat);
                }
                let fields = lowered.into_iter().collect::<Option<Vec<DPat>>>()?;
                Some(DPat { ctor, fields })
            }
            _ => None,
        }
    }

    fn lower_const_fields(
        &self,
        init: ConstBody<'_>,
        exprs: &[ExprId],
        tys: &[Ty],
        depth: u32,
    ) -> Option<Vec<DPat>> {
        if exprs.len() != tys.len() {
            return None;
        }
        exprs
            .iter()
            .zip(tys)
            .map(|(&expr, ty)| self.lower_const_expr(init, expr, ty, depth))
            .collect()
    }

    /// What a path in a constant's initializer denotes, if a unit constructor or a const.
    fn const_path_target(&self, init: ConstBody<'_>, expr: ExprId) -> Option<PathTarget> {
        if let Some(value) = init.inference.and_then(|inf| inf.paths.get(&expr)) {
            return match value {
                ValueRef::Ctor(ctor) => Some(PathTarget::Ctor(ctor.adt, ctor.variant)),
                ValueRef::Const { item, .. } => Some(PathTarget::Const(*item)),
                _ => None,
            };
        }
        match init.resolution.store.exprs.get(&expr)? {
            Resolution::Def(Def::Variant { enum_item, index }) => {
                Some(PathTarget::Ctor(*enum_item, Some(*index)))
            }
            Resolution::Def(Def::Item(item)) if self.env.adt(*item).is_some() => {
                Some(PathTarget::Ctor(*item, None))
            }
            Resolution::Def(Def::Item(item)) | Resolution::Assoc(item) => {
                self.env.const_body(*item).map(|_| PathTarget::Const(*item))
            }
            _ => None,
        }
    }

    /// The tuple struct or tuple variant a call in a constant's initializer builds.
    fn const_callee(&self, init: ConstBody<'_>, callee: ExprId) -> Option<(ItemId, Option<u32>)> {
        if let Some(value) = init.inference.and_then(|inf| inf.paths.get(&callee)) {
            return match value {
                ValueRef::Fn(func) => match func.def {
                    FnDefId::StructCtor(item) => Some((item, None)),
                    FnDefId::VariantCtor(item, index) => Some((item, Some(index))),
                    FnDefId::Item(_) => None,
                },
                _ => None,
            };
        }
        match init.resolution.store.exprs.get(&callee)? {
            Resolution::Def(Def::Variant { enum_item, index }) => Some((*enum_item, Some(*index))),
            Resolution::Def(Def::Item(item)) if self.env.adt(*item).is_some() => {
                Some((*item, None))
            }
            _ => None,
        }
    }

    /// The integer value of `const item`, folded by the stage 3 constant folder.
    fn const_int(&self, item: ItemId) -> Option<i128> {
        let init = self.env.const_body(item)?;
        self.env.fold_int(init, init.body.value)
    }
}

/// What a path in a constant's initializer denotes.
enum PathTarget {
    Ctor(ItemId, Option<u32>),
    Const(ItemId),
}

#[cfg(test)]
mod tests;
