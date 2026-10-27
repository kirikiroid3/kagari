//! Computes [`ProgramSignatures`]: every item signature of every loaded file.

use kagari_syntax::TextRange;

use super::lower::{Scope, Sink, TyLowerer, item_signature, trait_assoc};
use super::{
    AdtDef, FieldDef, FnSig, GenericParamDef, GenericParamDefKind, Generics, ItemSig,
    ProgramSignatures, StdItems, VariantDef,
};
use crate::db::Db;
use crate::def_map::{Def, DefMaps, Namespace, Resolution, def_maps, resolve_module_path};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::hir::{
    GenericParamKind, Name, Path, PathKind, PathSegment, SelfParamKind, Signature, SignatureKind,
};
use crate::input::Program;
use crate::item_tree::ItemIndex;
use crate::item_tree::{Item, ItemId, ItemKind, ItemParent, item_tree};
use crate::ty::{Ty, TyKind};

pub(super) fn collect(db: &dyn Db, program: Program) -> ProgramSignatures {
    let maps = def_maps(db, program);
    let mut lowerer = TyLowerer::new(db, program);
    let mut out = ProgramSignatures::default();
    for &(file, _) in &maps.files {
        let tree = item_tree(db, file);
        for (index, data) in tree.items.iter().enumerate() {
            let Some(item_index) = ItemIndex::from_usize(index) else {
                continue;
            };
            let item = ItemId {
                file,
                index: item_index,
            };
            let Some((signature, res)) = item_signature(db, program, item) else {
                continue;
            };
            let scope = Scope {
                src: Owner::Signature(item),
                store: &signature.store,
                res,
            };
            let mut sink = Sink::loud();
            let sig = item_sig(&mut lowerer, maps, item, data, signature, scope, &mut sink);
            out.diagnostics.extend(sink.diags);
            if let Some(sig) = sig {
                out.items.insert(item, sig);
            }
        }
    }
    out.diagnostics.append(&mut lowerer.cycle_diags);
    out.std = std_items(db, maps);
    out
}

/// The impl or trait an associated item belongs to.
pub(crate) fn assoc_parent(db: &dyn Db, item: ItemId) -> Option<ItemId> {
    let tree = item_tree(db, item.file);
    let ItemParent::Item(parent) = tree.item(item.index)?.parent else {
        return None;
    };
    matches!(
        tree.item(parent)?.kind,
        ItemKind::Impl { .. } | ItemKind::Trait { .. }
    )
    .then_some(ItemId {
        file: item.file,
        index: parent,
    })
}

fn item_sig(
    lowerer: &mut TyLowerer<'_>,
    maps: &DefMaps,
    item: ItemId,
    data: &Item,
    signature: &Signature,
    scope: Scope<'_>,
    sink: &mut Sink<'_>,
) -> Option<ItemSig> {
    let db = lowerer.db;
    Some(match (&data.kind, &signature.kind) {
        (
            ItemKind::Fn { kind, .. },
            SignatureKind::Fn {
                params,
                self_param,
                ret,
            },
        ) => {
            let generics = generics(lowerer, item, signature, scope, sink);
            let mut tys = Vec::new();
            if let Some(self_param) = self_param {
                let self_ty = match generics.parent {
                    Some(parent) => match item_tree(db, parent.file)
                        .item(parent.index)
                        .map(|data| &data.kind)
                    {
                        Some(ItemKind::Trait { .. }) => {
                            Ty::new(TyKind::TraitSelf { trait_item: parent })
                        }
                        _ => lowerer.impl_self_ty(parent),
                    },
                    None => Ty::error(),
                };
                tys.push(match self_param {
                    SelfParamKind::Value => self_ty,
                    SelfParamKind::Ref => Ty::reference(false, self_ty),
                    SelfParamKind::RefMut => Ty::reference(true, self_ty),
                });
            }
            tys.extend(
                params
                    .iter()
                    .map(|&param| lowerer.lower(scope, param, sink)),
            );
            let ret = match ret {
                Some(ret) => lowerer.lower(scope, *ret, sink),
                None => Ty::unit(),
            };
            let self_mutable = signature.self_mutable;
            ItemSig::Fn(FnSig {
                generics,
                kind: *kind,
                scene: data.has_attr("scene"),
                entry: data.has_attr("entry"),
                builtin: data.has_attr("builtin"),
                self_param: *self_param,
                self_mutable,
                params: tys,
                ret,
            })
        }
        (ItemKind::Struct { shape }, SignatureKind::Struct { fields }) => {
            let generics = generics(lowerer, item, signature, scope, sink);
            let fields = fields
                .iter()
                .map(|(name, ty)| FieldDef {
                    name: name.clone(),
                    ty: lowerer.lower(scope, *ty, sink),
                })
                .collect();
            ItemSig::Adt(AdtDef {
                generics,
                is_enum: false,
                builtin: data.has_attr("builtin"),
                must_use: data.has_attr("must_use"),
                variants: vec![VariantDef {
                    name: data.name.clone().unwrap_or_default(),
                    shape: shape.clone(),
                    fields,
                }],
            })
        }
        (ItemKind::Enum { variants }, SignatureKind::Enum { variants: fields }) => {
            let generics = generics(lowerer, item, signature, scope, sink);
            let variants = variants
                .iter()
                .zip(fields)
                .map(|((name, shape), fields)| VariantDef {
                    name: name.clone(),
                    shape: shape.clone(),
                    fields: fields
                        .iter()
                        .map(|(name, ty)| FieldDef {
                            name: name.clone(),
                            ty: lowerer.lower(scope, *ty, sink),
                        })
                        .collect(),
                })
                .collect();
            ItemSig::Adt(AdtDef {
                generics,
                is_enum: true,
                builtin: data.has_attr("builtin"),
                must_use: data.has_attr("must_use"),
                variants,
            })
        }
        (ItemKind::Const { .. }, SignatureKind::Const { ty }) => ItemSig::Const {
            generics: generics(lowerer, item, signature, scope, sink),
            ty: lowerer.lower(scope, *ty, sink),
        },
        (ItemKind::Static { mutable }, SignatureKind::Static { ty }) => ItemSig::Static {
            ty: lowerer.lower(scope, *ty, sink),
            mutable: *mutable,
        },
        (ItemKind::TypeAlias { .. }, SignatureKind::TypeAlias { value, .. }) => {
            let generics = generics(lowerer, item, signature, scope, sink);
            let ty = value.map(|value| lowerer.lower_alias_item(item, scope, value, sink));
            ItemSig::TypeAlias { generics, ty }
        }
        (ItemKind::Impl { .. }, SignatureKind::Impl { self_ty, .. }) => ItemSig::Impl {
            generics: generics(lowerer, item, signature, scope, sink),
            self_ty: lowerer.lower(scope, *self_ty, sink),
            trait_item: maps.impl_traits.get(&item).copied(),
        },
        (ItemKind::Trait { .. }, SignatureKind::Trait { .. }) => ItemSig::Trait {
            generics: generics(lowerer, item, signature, scope, sink),
        },
        _ => return None,
    })
}

fn generics(
    lowerer: &mut TyLowerer<'_>,
    item: ItemId,
    signature: &Signature,
    scope: Scope<'_>,
    sink: &mut Sink<'_>,
) -> Generics {
    let db = lowerer.db;
    let parent = assoc_parent(db, item);
    let parent_count = parent
        .and_then(|parent| item_signature(db, lowerer.program, parent))
        .map_or(0, |(signature, _)| {
            u32::try_from(signature.generics.len()).unwrap_or(u32::MAX)
        });
    let params = signature
        .generics
        .iter()
        .map(|param| GenericParamDef {
            name: param.name.clone(),
            kind: match &param.kind {
                GenericParamKind::Type { bounds } => GenericParamDefKind::Type {
                    bounds: bounds
                        .iter()
                        .filter_map(|&bound| {
                            let range = scope.node(NodeId::Type(bound));
                            trait_bound(lowerer, scope.res.types.get(&bound), range, sink)
                        })
                        .collect(),
                },
                GenericParamKind::Const { ty } => GenericParamDefKind::Const {
                    ty: lowerer.lower(scope, *ty, sink),
                },
            },
        })
        .collect();
    Generics {
        parent,
        parent_count,
        params,
    }
}

fn trait_bound(
    lowerer: &TyLowerer<'_>,
    resolution: Option<&Resolution>,
    range: Anchor,
    sink: &mut Sink<'_>,
) -> Option<ItemId> {
    let db = lowerer.db;
    match resolution? {
        Resolution::Def(Def::Item(item))
            if matches!(
                item_tree(db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Trait { .. })
            ) =>
        {
            Some(*item)
        }
        Resolution::Error => None,
        _ => {
            sink.diags.push(Diag::new(
                range,
                DiagnosticCode::TypeExpectedTrait,
                "expected a trait",
            ));
            None
        }
    }
}

fn std_path(db: &dyn Db, maps: &DefMaps, path: &[&str], ns: Namespace) -> Option<ItemId> {
    let path = Path {
        kind: PathKind::Crate,
        segments: path
            .iter()
            .map(|name| PathSegment {
                name: Name::new(name),
                args: None,
            })
            .collect(),
    };
    // The path is not written in source; its diagnostics are dropped.
    let file = maps.module_file(maps.std_root());
    let at = |_| Anchor::Text(file, TextRange::default());
    match resolve_module_path(maps, db, maps.std_root(), &path, ns, &at).resolved {
        Some(Resolution::Def(Def::Item(item))) => Some(item),
        _ => None,
    }
}

fn std_items(db: &dyn Db, maps: &DefMaps) -> StdItems {
    let ty = |path: &[&str]| std_path(db, maps, path, Namespace::Types);
    let value = |path: &[&str]| std_path(db, maps, path, Namespace::Values);
    let integer = ty(&["Integer"]);
    StdItems {
        text: ty(&["narrative", "text", "Text"]),
        string: ty(&["String"]),
        decimal: ty(&["Decimal"]),
        rat: ty(&["Rat"]),
        vec: ty(&["Vec"]),
        range: ty(&["Range"]),
        option: ty(&["Option"]),
        result: ty(&["Result"]),
        screen: ty(&["screen", "Screen"]),
        narrative: ty(&["narrative", "Narrative"]),
        setting: ty(&["system", "Setting"]),
        meta: ty(&["narrative", "Meta"]),
        say: ty(&["narrative", "Say"]),
        choice: ty(&["narrative", "Choice"]),
        choice_arm: ty(&["narrative", "ChoiceArm"]),
        element: ty(&["ui", "Element"]),
        assets: [
            "ImageAsset",
            "AudioAsset",
            "VoiceAsset",
            "VideoAsset",
            "FontAsset",
            "ShaderAsset",
        ]
        .iter()
        .filter_map(|name| ty(&[name]))
        .collect(),
        eq: ty(&["Eq"]),
        integer,
        asset: ty(&["Asset"]),
        step: ty(&["narrative", "Step"]),
        rand_choose: ty(&["RandChoose"]),
        to_text: ty(&["narrative", "text", "ToText"]),
        push_step: value(&["narrative", "push_step"]),
        rng_choose: value(&["narrative", "rng_choose"]),
        asset_fn: value(&["asset"]),
        system_font_fn: value(&["system_font"]),
        to_text_fn: value(&["narrative", "text", "to_text"]),
        concat: value(&["narrative", "text", "concat"]),
        ruby: value(&["narrative", "text", "ruby"]),
        mono_ruby: value(&["narrative", "text", "mono_ruby"]),
        effect: value(&["narrative", "text", "effect"]),
        nav_start: value(&["system", "nav", "start"]),
        signal: value(&["ui", "signal"]),
        integer_into: integer.and_then(|integer| trait_assoc(db, integer, "into")),
        integer_saturate: integer.and_then(|integer| trait_assoc(db, integer, "saturate")),
    }
}
