//! Go to definition.

use kagari_syntax::{TextRange, TextSize};

use super::NavTarget;
use super::lookup::{Symbol, def_sites, reference_at};
use crate::db::Db;
use crate::def_map::{ModuleOrigin, def_maps};
use crate::hir::{body_source_map_of, signature_source_map_of};
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemIndex, item_source_map};
use crate::resolve::body_resolution;

/// The definition of what is at `offset` in `file`.
pub fn definition(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Option<NavTarget> {
    let (owner, _, found) = reference_at(db, program, file, offset)?;
    target(db, program, found.symbol(owner)?)
}

/// Where `symbol` is defined: its file and the range of its name.
pub(crate) fn target(db: &dyn Db, program: Program, symbol: Symbol) -> Option<NavTarget> {
    match symbol {
        Symbol::Local { owner, local } => {
            let resolved = body_resolution(db, program, owner).body.as_ref()?;
            let pat = resolved.locals[local].pat?;
            let range = body_source_map_of(db, owner)?.pat(pat);
            Some(NavTarget {
                file: owner.file.path(db).clone(),
                range,
            })
        }
        Symbol::Item(item) => item_name(db, item.file, item.index),
        Symbol::Variant { enum_item, index } => {
            let src = item_source_map(db, enum_item.file)
                .items
                .get(enum_item.index.idx())?;
            let range = *src.variants.get(usize::try_from(index).ok()?)?;
            Some(NavTarget {
                file: enum_item.file.path(db).clone(),
                range,
            })
        }
        Symbol::Module(module) => match def_maps(db, program).module(module).origin {
            ModuleOrigin::File(file) => Some(NavTarget {
                file: file.path(db).clone(),
                range: TextRange::default(),
            }),
            ModuleOrigin::Inline { file, item } => item_name(db, file, item),
            ModuleOrigin::Block { .. } => None,
        },
        Symbol::GenericParam { owner, index } => {
            let map = signature_source_map_of(db, owner)?;
            let range = *map.generic_params.get(usize::try_from(index).ok()?)?;
            Some(NavTarget {
                file: owner.file.path(db).clone(),
                range,
            })
        }
        Symbol::Field(field) => {
            let (_, site) = def_sites(db, field.adt.file)
                .into_iter()
                .find(|(_, site)| site.found.symbol(field.adt) == Some(symbol))?;
            Some(NavTarget {
                file: field.adt.file.path(db).clone(),
                range: site.range,
            })
        }
    }
}

fn item_name(db: &dyn Db, file: SourceFile, index: ItemIndex) -> Option<NavTarget> {
    let src = item_source_map(db, file).items.get(index.idx())?;
    Some(NavTarget {
        file: file.path(db).clone(),
        range: src.name,
    })
}
