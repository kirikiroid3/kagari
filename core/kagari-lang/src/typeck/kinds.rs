//! The kind and story rules of compilation stage 3 (`docs/dev/src/compiler/typeck.md`; for
//! users, `docs/reference/en/src/concepts/*.md` and `syntax/values.md`), on top of inference:
//!
//! - the call rules between `fn`, `pushdown fn` and `const fn`, for direct calls, methods
//!   (std builtins included) and calls through pointers and closures, whose types carry the
//!   kind;
//! - scenes: only a `#[scene]` runs steps (`push_step`) and calls scenes; instant
//!   procedures (other `pushdown fn` and closures in the story) have no `loop`/`while` and
//!   do not recurse (a program-wide cycle check over the calls in their MIR, `instant`);
//! - `for` in the story over arrays and constant ranges only;
//! - story variables only in `pushdown` code, with finite types and constant initializers;
//!   handles (`Setting<T>`, `Meta<T>`) as the only immutable statics, constant
//!   initializers, no references in item types, no `Meta` in `const` code;
//! - finite types for everything the story holds at runtime (constant expressions are
//!   folded and exempt), and entry scenes without `Text`;
//! - second-class references and closures in the story, and exclusive `&mut` arguments;
//! - mutability of assigned and mutably borrowed places, in all code;
//! - placement of `#[scene]`, `#[entry]`, `#[must_use]` and `#[shader_arg]`, and the shape and
//!   field types of `#[shader_arg]` structs;
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
mod instant;
mod places;

pub use instant::{InstantEdges, instant_edges, instant_recursion};

use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};

use crate::db::Db;
use crate::def_map::def_maps;
use crate::eval::const_exprs;
use crate::hir::{Signature, SignatureKind, body_of, signature_of};
use crate::infer::inference;
use crate::input::{FilePath, Program};
use crate::item_tree::{FnKind, Item, ItemId, ItemKind, item_tree};
use crate::resolve::body_resolution;
use crate::signatures::{ItemSig, ProgramSignatures, assoc_parent, display_ty, program_signatures};
use crate::ty::{Ty, TyKind};

/// The kind checks of one item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindChecks {
    pub diagnostics: Vec<Diag>,
}

#[salsa::tracked(returns(ref))]
pub fn kind_checks(db: &dyn Db, program: Program, item: ItemId) -> KindChecks {
    let mut out = KindChecks::default();
    if !def_maps(db, program).file_modules.contains_key(&item.file) {
        return out;
    }
    let Some(data) = item_tree(db, item.file).item(item.index) else {
        return out;
    };
    let env = Env::new(db, program);
    env.check_attributes(item, data, &mut out.diagnostics);
    if let Some(signature) = signature_of(db, item) {
        env.check_item_types(item, signature, &mut out.diagnostics);
    }
    let (Some(body), Some(res), Some(infer), Some(consts)) = (
        body_of(db, item),
        body_resolution(db, program, item).body.as_ref(),
        inference(db, program, item),
        const_exprs(db, program, item),
    ) else {
        return out;
    };
    body::check_body(&env, item, data, body, res, infer, consts, &mut out);
    out
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
            Some(ItemSig::Impl { self_ty, .. }) => match self_ty.kind() {
                TyKind::Adt { item: adt, .. } => self.item_name(*adt),
                _ => self.show(self_ty),
            },
            _ => self.item_name(parent),
        };
        format!("{owner}::{name}")
    }

    pub(super) fn is_std(&self, item: ItemId) -> bool {
        matches!(item.file.path(self.db), FilePath::Std(_))
    }

    /// `diag` with a note at the declaration of `item`, unless it is a std item.
    pub(super) fn declared_here(&self, diag: Diag, item: ItemId) -> Diag {
        if self.is_std(item) || item_tree(self.db, item.file).item(item.index).is_none() {
            return diag;
        }
        diag.with_related(
            Anchor::ItemName(item),
            format!("`{}` is declared here", self.item_name(item)),
        )
    }

    /// `#[scene]` only on a `pushdown fn`, `#[entry]` only with `#[scene]`, `#[must_use]`
    /// only on a struct or an enum.
    fn check_attributes(&self, item: ItemId, data: &Item, out: &mut Vec<Diag>) {
        for (index, attr) in data.attrs.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
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
                "shader_arg" => match data.kind {
                    ItemKind::Struct { .. } => continue,
                    _ => "`#[shader_arg]` applies only to structs".to_owned(),
                },
                _ => continue,
            };
            out.push(Diag::new(
                Anchor::ItemAttr(item, index),
                DiagnosticCode::KindMisplacedAttribute,
                message,
            ));
        }
    }

    /// The types an item declares: finite parameters and return type of a `pushdown fn`
    /// (references and closures only as parameters, a reference result only from a method
    /// with a reference receiver), entry scenes without `Text`; no reference in the type of
    /// a `const` or `static`; finite story variables; handles as the only immutable statics.
    fn check_item_types(&self, item: ItemId, signature: &Signature, out: &mut Vec<Diag>) {
        let name = Anchor::ItemName(item);
        let ty_anchor = |ty| Anchor::Node(Owner::Signature(item), NodeId::Type(ty));
        let diag = |anchor: Anchor, code: DiagnosticCode, message: String| {
            Diag::new(anchor, code, message)
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
                        Some(param) => param_refs.get(param).map_or(name, |&ty| ty_anchor(ty)),
                        None => name,
                    };
                    if ty.references_error() || matches!(ty.kind(), TyKind::Closure { .. }) {
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
                let range = ret_ref.map_or(name, ty_anchor);
                if ret.references_error() {
                    return;
                }
                match ret.kind() {
                    TyKind::Ref { .. }
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
                    TyKind::Closure { .. } => {}
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
                        ty_anchor(*ty_ref),
                        DiagnosticCode::KindReferenceInItem,
                        format!(
                            "the type of a `const` cannot contain a reference, but this is `{}`",
                            self.show(ty)
                        ),
                    ));
                }
            }
            (Some(ItemSig::Static { ty, mutable }), SignatureKind::Static { ty: ty_ref }) => {
                let range = ty_anchor(*ty_ref);
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
            (Some(ItemSig::Adt(adt)), SignatureKind::Struct { fields }) if adt.shader_arg => {
                self.check_shader_arg(item, adt, signature, fields, out);
            }
            _ => {}
        }
    }

    /// A `#[shader_arg]` struct: named fields, no type parameters, and only field types a
    /// shader can read.
    fn check_shader_arg(
        &self,
        item: ItemId,
        adt: &crate::signatures::AdtDef,
        signature: &Signature,
        fields: &[(Option<crate::hir::Name>, crate::hir::TypeRefId)],
        out: &mut Vec<Diag>,
    ) {
        let named = adt.variants.first().is_some_and(|variant| {
            matches!(&variant.shape, crate::item_tree::Shape::Record(names) if !names.is_empty())
        });
        let type_params = signature
            .generics
            .iter()
            .any(|param| matches!(param.kind, crate::hir::GenericParamKind::Type { .. }));
        if !named || type_params {
            let attr = item_tree(self.db, item.file)
                .item(item.index)
                .and_then(|data| {
                    data.attrs
                        .iter()
                        .position(|attr| attr.as_str() == "shader_arg")
                })
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(0);
            out.push(Diag::new(
                Anchor::ItemAttr(item, attr),
                DiagnosticCode::KindShaderArgShape,
                "a `#[shader_arg]` struct needs named fields and no type parameters",
            ));
            return;
        }
        let Some(variant) = adt.variants.first() else {
            return;
        };
        for (field, (_, ty_ref)) in variant.fields.iter().zip(fields) {
            let name = field
                .name
                .as_ref()
                .map_or_else(|| "?".to_owned(), ToString::to_string);
            let message = match self.shader_field(&field.ty) {
                ShaderField::Ok => continue,
                ShaderField::Bad => format!(
                    "field `{name}` has type `{}`, which a shader cannot read",
                    self.show(&field.ty)
                ),
                ShaderField::EmptyArray => format!("field `{name}` is an array of length 0"),
            };
            out.push(Diag::new(
                Anchor::Node(Owner::Signature(item), NodeId::Type(*ty_ref)),
                DiagnosticCode::KindShaderArgField,
                message,
            ));
        }
    }

    /// Whether a shader can read a field of type `ty`.
    fn shader_field(&self, ty: &Ty) -> ShaderField {
        use crate::ty::IntTy;
        let std = &self.sigs.std;
        match ty.kind() {
            TyKind::Error => ShaderField::Ok,
            TyKind::Bool | TyKind::Int(IntTy::I32 | IntTy::U32) => ShaderField::Ok,
            TyKind::Array { elem, len } => {
                if len.value() == Some(0) {
                    return ShaderField::EmptyArray;
                }
                self.shader_field(elem)
            }
            TyKind::Adt { item, .. } => {
                let item = *item;
                if Some(item) == std.decimal
                    || Some(item) == std.rat
                    || std.shader_arg_std.contains(&item)
                {
                    return ShaderField::Ok;
                }
                let ok = self.sigs.adt(item).is_some_and(|adt| {
                    adt.shader_arg
                        || (adt.is_enum
                            && !adt.builtin
                            && adt.generics.params.is_empty()
                            && !adt.variants.is_empty()
                            && adt.variants.iter().all(|variant| variant.fields.is_empty()))
                });
                if ok {
                    ShaderField::Ok
                } else {
                    ShaderField::Bad
                }
            }
            _ => ShaderField::Bad,
        }
    }

    /// `Setting<T>` or `Meta<T>`.
    fn is_handle(&self, ty: &Ty) -> bool {
        matches!(ty.kind(), TyKind::Adt { item, .. }
            if Some(*item) == self.sigs.std.setting || Some(*item) == self.sigs.std.meta)
    }

    pub(super) fn is_meta(&self, ty: &Ty) -> bool {
        matches!(ty.kind(), TyKind::Adt { item, .. } if Some(*item) == self.sigs.std.meta)
    }
}

pub(super) fn fn_kind_word(kind: FnKind) -> &'static str {
    match kind {
        FnKind::Plain => "fn",
        FnKind::Const => "const fn",
        FnKind::Pushdown => "pushdown fn",
    }
}

/// Whether a shader can read a `#[shader_arg]` field type.
enum ShaderField {
    Ok,
    Bad,
    EmptyArray,
}
