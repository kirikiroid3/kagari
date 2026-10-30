//! The diagnostic text of types.

use crate::db::Db;
use crate::def_map::{Def, DefMaps};
use crate::hir::{ClosureKind, FnPtrKind, signature_of};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId};
use crate::paths;
use crate::ty::{Const, FnDefId, FnRef, GenericArg, InferVar, Ty, TyKind};

use super::lower::item_name;
use super::subst::{Subst, subst_ty};
use super::{ProgramSignatures, program_signatures};

/// The diagnostic text of a type: `Int<0, 5>`, `Option<Text>`, `&mut Screen`, `[T; 4]`,
/// `const fn(Screen) -> Screen`, `pushdown Fn(u32)`, `(bool, Route)`, `()`, `!`; `_` for an
/// inference variable. A named function shows as `fn(u32) -> bool {name}`.
pub fn display_ty(db: &dyn Db, program: Program, ty: &Ty) -> String {
    let sigs = program_signatures(db, program);
    display_ty_with(db, Some(sigs), ty, &|_| false)
}

/// Like [`display_ty`]; `is_literal` tells which inference variables are unresolved integer
/// literals, shown as `{integer}`. Without `sigs`, named functions show only their name.
pub(crate) fn display_ty_with(
    db: &dyn Db,
    sigs: Option<&ProgramSignatures>,
    ty: &Ty,
    is_literal: &dyn Fn(InferVar) -> bool,
) -> String {
    let mut out = String::new();
    Printer {
        db,
        sigs,
        maps: None,
        is_literal,
    }
    .ty(ty, &mut out);
    out
}

/// A type with every item named by its full path: `crate::story::Route`,
/// `std::Option<crate::Mood>`, `fn(u32) {crate::pick}`. Unlike [`display_ty`] two distinct
/// types never print the same, so it names the type arguments of monomorphized instances.
pub(crate) fn display_ty_qualified(
    db: &dyn Db,
    sigs: &ProgramSignatures,
    maps: &DefMaps,
    ty: &Ty,
) -> String {
    let mut out = String::new();
    Printer {
        db,
        sigs: Some(sigs),
        maps: Some(maps),
        is_literal: &|_| false,
    }
    .ty(ty, &mut out);
    out
}

fn param_name(db: &dyn Db, owner: ItemId, index: u32) -> String {
    signature_of(db, owner)
        .and_then(|signature| signature.generics.get(index as usize))
        .map_or_else(|| "_".to_owned(), |param| param.name.to_string())
}

/// The kind, parameters and return type of a named function or tuple constructor.
fn fn_def_signature(sigs: &ProgramSignatures, fn_ref: &FnRef) -> Option<(FnPtrKind, Vec<Ty>, Ty)> {
    match fn_ref.def {
        FnDefId::Item(item) => {
            let sig = sigs.fn_sig(item)?;
            let mut owners = Vec::new();
            if let Some(parent) = sig.generics.parent {
                owners.push((parent, 0));
            }
            owners.push((item, sig.generics.parent_count as usize));
            let subst = Subst {
                owners: &owners,
                args: &fn_ref.args,
                self_ty: fn_ref.self_ty.as_ref(),
            };
            let kind = match sig.kind {
                FnKind::Plain => FnPtrKind::Plain,
                FnKind::Const => FnPtrKind::Const,
                FnKind::Pushdown if sig.scene => FnPtrKind::Scene,
                FnKind::Pushdown => FnPtrKind::Pushdown,
            };
            Some((
                kind,
                sig.params
                    .iter()
                    .map(|param| subst_ty(param, &subst))
                    .collect(),
                subst_ty(&sig.ret, &subst),
            ))
        }
        FnDefId::StructCtor(adt) | FnDefId::VariantCtor(adt, _) => {
            let variant = match fn_ref.def {
                FnDefId::VariantCtor(_, index) => index as usize,
                _ => 0,
            };
            let subst = Subst {
                owners: &[(adt, 0)],
                args: &fn_ref.args,
                self_ty: None,
            };
            let fields = &sigs.adt(adt)?.variants.get(variant)?.fields;
            Some((
                FnPtrKind::Const,
                fields
                    .iter()
                    .map(|field| subst_ty(&field.ty, &subst))
                    .collect(),
                Ty::adt(adt, fn_ref.args.clone()),
            ))
        }
    }
}

struct Printer<'a> {
    db: &'a dyn Db,
    sigs: Option<&'a ProgramSignatures>,
    /// `Some`: name items by their full paths.
    maps: Option<&'a DefMaps>,
    is_literal: &'a dyn Fn(InferVar) -> bool,
}

impl Printer<'_> {
    fn ty(&self, ty: &Ty, out: &mut String) {
        match ty.kind() {
            TyKind::Error => out.push_str("{unknown}"),
            TyKind::Never => out.push('!'),
            TyKind::Bool => out.push_str("bool"),
            TyKind::Int(int) => out.push_str(int.name()),
            TyKind::Bounded { lo, hi } => {
                out.push_str("Int<");
                self.constant(lo, out);
                out.push_str(", ");
                self.constant(hi, out);
                out.push('>');
            }
            TyKind::Tuple(fields) => {
                out.push('(');
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    self.ty(field, out);
                }
                if fields.len() == 1 {
                    out.push(',');
                }
                out.push(')');
            }
            TyKind::Array { elem, len } => {
                out.push('[');
                self.ty(elem, out);
                out.push_str("; ");
                self.constant(len, out);
                out.push(']');
            }
            TyKind::Adt { item, args } => {
                match self.maps {
                    Some(maps) => out.push_str(&paths::item_path(self.db, maps, *item)),
                    None => out.push_str(&item_name(self.db, *item)),
                }
                self.args(args, out);
            }
            TyKind::Ref { mutable, inner } => {
                out.push_str(if *mutable { "&mut " } else { "&" });
                self.ty(inner, out);
            }
            TyKind::FnPtr { kind, params, ret } => {
                out.push_str(match kind {
                    FnPtrKind::Plain => "fn",
                    FnPtrKind::Const => "const fn",
                    FnPtrKind::Pushdown => "pushdown fn",
                    FnPtrKind::Scene => "scene fn",
                });
                self.signature(params, ret, out);
            }
            TyKind::Closure { kind, params, ret } => {
                out.push_str(match kind {
                    ClosureKind::Plain => "Fn",
                    ClosureKind::Const => "const Fn",
                    ClosureKind::Pushdown => "pushdown Fn",
                });
                self.signature(params, ret, out);
            }
            TyKind::FnDef(fn_ref) => {
                let name = match (self.maps, fn_ref.def) {
                    (Some(maps), FnDefId::Item(item)) => {
                        let mut name = paths::assoc_path(self.db, maps, item);
                        if !fn_ref.args.is_empty() {
                            name.push_str("::");
                            self.args(&fn_ref.args, &mut name);
                        }
                        name
                    }
                    (Some(maps), FnDefId::StructCtor(item)) => {
                        paths::def_path(self.db, maps, Def::Item(item))
                    }
                    (Some(maps), FnDefId::VariantCtor(enum_item, index)) => {
                        paths::def_path(self.db, maps, Def::Variant { enum_item, index })
                    }
                    (None, FnDefId::Item(item) | FnDefId::StructCtor(item)) => {
                        item_name(self.db, item).to_string()
                    }
                    (None, FnDefId::VariantCtor(item, index)) => format!(
                        "{}::{}",
                        item_name(self.db, item),
                        variant_name(self.db, item, index)
                    ),
                };
                match self.sigs.and_then(|sigs| fn_def_signature(sigs, fn_ref)) {
                    Some((kind, params, ret)) => {
                        self.ty(
                            &Ty::new(TyKind::FnPtr {
                                kind,
                                params: params.into_boxed_slice(),
                                ret,
                            }),
                            out,
                        );
                        out.push_str(" {");
                        out.push_str(&name);
                        out.push('}');
                    }
                    None => {
                        out.push_str("fn ");
                        out.push_str(&name);
                    }
                }
            }
            TyKind::Param { owner, index } => out.push_str(&param_name(self.db, *owner, *index)),
            TyKind::TraitSelf { .. } => out.push_str("Self"),
            TyKind::Projection { base, name, .. } => {
                self.ty(base, out);
                out.push_str("::");
                out.push_str(name);
            }
            TyKind::Infer(var) => {
                if (self.is_literal)(*var) {
                    out.push_str("{integer}");
                } else {
                    out.push('_');
                }
            }
        }
    }

    fn signature(&self, params: &[Ty], ret: &Ty, out: &mut String) {
        out.push('(');
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            self.ty(param, out);
        }
        out.push(')');
        if !ret.is_unit() {
            out.push_str(" -> ");
            self.ty(ret, out);
        }
    }

    fn args(&self, args: &[GenericArg], out: &mut String) {
        if args.is_empty() {
            return;
        }
        out.push('<');
        for (index, arg) in args.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            match arg {
                GenericArg::Type(ty) => self.ty(ty, out),
                GenericArg::Const(c) => self.constant(c, out),
            }
        }
        out.push('>');
    }

    fn constant(&self, c: &Const, out: &mut String) {
        match c {
            Const::Value(value) => out.push_str(&value.to_string()),
            Const::Param { owner, index } => out.push_str(&param_name(self.db, *owner, *index)),
            Const::Infer(_) => out.push('_'),
            Const::Error => out.push_str("{unknown}"),
        }
    }
}

fn variant_name(db: &dyn Db, item: ItemId, index: u32) -> String {
    match crate::item_tree::item_tree(db, item.file)
        .item(item.index)
        .map(|data| &data.kind)
    {
        Some(crate::item_tree::ItemKind::Enum { variants }) => variants
            .get(index as usize)
            .map_or_else(String::new, |(name, _)| name.to_string()),
        _ => String::new(),
    }
}
