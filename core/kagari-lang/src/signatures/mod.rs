//! Item signatures lowered to semantic types (compilation stage 3): generics with bounds,
//! function signatures, struct and enum definitions, the types of constants and statics,
//! type aliases and impl headers, plus the std items the checker and evaluator refer to.
//!
//! Computed once for the whole program, so aliases and constants used across files never
//! form query cycles.

mod collect;
mod consts;
mod display;
pub(crate) mod lower;
pub(crate) mod subst;
mod traits;

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::diagnostic::Diagnostic;
use crate::hir::{Name, SelfParamKind};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, Shape};
use crate::ty::Ty;

pub(crate) use collect::assoc_parent;
pub use consts::fold_const_expr;
pub use display::display_ty;
pub(crate) use display::display_ty_with;
pub use traits::{BoundEnv, implements};
pub(crate) use traits::{TraitSolver, normalize_projection, type_key};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramSignatures {
    /// Every item of every loaded file (game and std) that has a signature.
    pub items: FxHashMap<ItemId, ItemSig>,
    pub std: StdItems,
    pub diagnostics: Vec<Diagnostic>,
}

impl ProgramSignatures {
    pub fn get(&self, item: ItemId) -> Option<&ItemSig> {
        self.items.get(&item)
    }

    pub fn fn_sig(&self, item: ItemId) -> Option<&FnSig> {
        match self.items.get(&item) {
            Some(ItemSig::Fn(sig)) => Some(sig),
            _ => None,
        }
    }

    pub fn adt(&self, item: ItemId) -> Option<&AdtDef> {
        match self.items.get(&item) {
            Some(ItemSig::Adt(adt)) => Some(adt),
            _ => None,
        }
    }

    /// The generics of a function, ADT, constant, type alias, impl or trait.
    pub fn generics(&self, item: ItemId) -> Option<&Generics> {
        match self.items.get(&item)? {
            ItemSig::Fn(sig) => Some(&sig.generics),
            ItemSig::Adt(adt) => Some(&adt.generics),
            ItemSig::Const { generics, .. }
            | ItemSig::TypeAlias { generics, .. }
            | ItemSig::Impl { generics, .. }
            | ItemSig::Trait { generics } => Some(generics),
            ItemSig::Static { .. } => None,
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn program_signatures(db: &dyn Db, program: Program) -> ProgramSignatures {
    collect::collect(db, program)
}

/// The generic parameters of an item. Generic arguments are laid out as the parent's
/// parameters followed by the item's own (see [`crate::ty::FnRef::args`]); a
/// [`Ty::Param`] names its owner (the parent or the item itself) and its index in that
/// owner's own `params`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Generics {
    /// The impl or trait of an associated item.
    pub parent: Option<ItemId>,
    /// The number of parameters the parent contributes first.
    pub parent_count: u32,
    /// The item's own parameters.
    pub params: Vec<GenericParamDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericParamDef {
    pub name: Name,
    pub kind: GenericParamDefKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenericParamDefKind {
    /// A type parameter and the traits it is bounded by.
    Type { bounds: Vec<ItemId> },
    /// A `const` parameter and its type.
    Const { ty: Ty },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemSig {
    Fn(FnSig),
    Adt(AdtDef),
    /// A `const` item; associated constants carry their impl's generics as parent.
    Const {
        generics: Generics,
        ty: Ty,
    },
    Static {
        ty: Ty,
        mutable: bool,
    },
    /// `ty` is `None` for an associated type declared in a trait (`type Output;`).
    TypeAlias {
        generics: Generics,
        ty: Option<Ty>,
    },
    Impl {
        generics: Generics,
        self_ty: Ty,
        trait_item: Option<ItemId>,
    },
    Trait {
        generics: Generics,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnSig {
    pub generics: Generics,
    pub kind: FnKind,
    pub scene: bool,
    pub entry: bool,
    pub builtin: bool,
    pub self_param: Option<SelfParamKind>,
    /// Whether the `self` parameter is `mut self`.
    pub self_mutable: bool,
    /// Parameter types; the `self` parameter comes first when there is one, as its full
    /// type (`&mut Screen` for `&mut self` in `impl Screen`).
    pub params: Vec<Ty>,
    /// `()` when the return type is omitted.
    pub ret: Ty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdtDef {
    pub generics: Generics,
    pub is_enum: bool,
    pub builtin: bool,
    pub must_use: bool,
    /// The variants of an enum, in order; a struct has exactly one, named like the struct.
    pub variants: Vec<VariantDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantDef {
    pub name: Name,
    pub shape: Shape,
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDef {
    /// `None` for tuple fields.
    pub name: Option<Name>,
    pub ty: Ty,
}

/// Std items by role. `None` only when the std item is missing (already reported by
/// resolution as `resolve/missing-std-item` where forms use it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StdItems {
    // Types.
    pub text: Option<ItemId>,
    pub string: Option<ItemId>,
    pub decimal: Option<ItemId>,
    pub rat: Option<ItemId>,
    pub vec: Option<ItemId>,
    pub range: Option<ItemId>,
    pub option: Option<ItemId>,
    pub result: Option<ItemId>,
    pub screen: Option<ItemId>,
    pub narrative: Option<ItemId>,
    pub setting: Option<ItemId>,
    pub meta: Option<ItemId>,
    pub say: Option<ItemId>,
    pub choice: Option<ItemId>,
    pub choice_arm: Option<ItemId>,
    pub element: Option<ItemId>,
    /// `ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`.
    pub assets: Vec<ItemId>,
    // Traits.
    pub eq: Option<ItemId>,
    pub integer: Option<ItemId>,
    pub asset: Option<ItemId>,
    pub step: Option<ItemId>,
    pub to_text: Option<ItemId>,
    // Functions.
    pub push_step: Option<ItemId>,
    pub asset_fn: Option<ItemId>,
    pub to_text_fn: Option<ItemId>,
    pub concat: Option<ItemId>,
    pub ruby: Option<ItemId>,
    pub mono_ruby: Option<ItemId>,
    pub effect: Option<ItemId>,
    pub nav_start: Option<ItemId>,
    pub signal: Option<ItemId>,
    /// `Integer::into` and `Integer::saturate`.
    pub integer_into: Option<ItemId>,
    pub integer_saturate: Option<ItemId>,
}
