//! Rename: every place in the game crate that names a definition, rewritten to a new name.

use kagari_syntax::{SyntaxKind, TextRange, TextSize};

use super::lookup::{Found, Site, Symbol, def_sites, item_sites, record_field_names, reference_at};
use super::{FileEdits, TextEdit};
use crate::db::Db;
use crate::def_map::{CrateId, ModuleOrigin, def_maps};
use crate::hir::{Name, signature_of};
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::resolve::body_resolution;

/// Why a rename cannot be done; the message is shown to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameError(pub String);

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RenameError {}

/// The name at `offset` that [`rename`] would rename, or why it cannot.
pub fn prepare_rename(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Result<TextRange, RenameError> {
    Ok(target(db, program, file, offset)?.range)
}

/// The edits that rename the definition named at `offset`, and every reference to it, to
/// `new_name`: grouped by file in path order, each file's edits in text order.
pub fn rename(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
    new_name: &str,
) -> Result<Vec<FileEdits>, RenameError> {
    check_name(new_name)?;
    let target = target(db, program, file, offset)?;
    let name = target.name.as_str();
    let maps = def_maps(db, program);
    // A local is only named in its own body; a generic parameter only in its file.
    let (files, only_item): (Vec<SourceFile>, Option<ItemId>) = match target.symbols.as_slice() {
        [Symbol::Local { owner, .. }] => (vec![owner.file], Some(*owner)),
        [Symbol::GenericParam { owner, .. }] => (vec![owner.file], None),
        _ => (
            maps.files
                .iter()
                .filter(|(_, krate)| *krate == CrateId::Game)
                .map(|(file, _)| *file)
                .collect(),
            None,
        ),
    };

    let mut out: Vec<FileEdits> = Vec::new();
    for file in files {
        let Some(text) = file.text(db).as_deref() else {
            continue;
        };
        if !text.contains(name) {
            continue;
        }
        let mut sites: Vec<(ItemId, Site)> = Vec::new();
        match only_item {
            Some(item) => sites.extend(
                item_sites(db, program, item)
                    .into_iter()
                    .map(|site| (item, site)),
            ),
            None => {
                sites.extend(def_sites(db, file));
                for index in item_tree(db, file).indices() {
                    let item = ItemId { file, index };
                    sites.extend(
                        item_sites(db, program, item)
                            .into_iter()
                            .map(|site| (item, site)),
                    );
                }
            }
        }
        let mut edits: Vec<TextEdit> = Vec::new();
        // Names that also denote a definition that is not renamed: `use m::x;` imports both a
        // type `x` and a value `x`.
        let mut kept: Vec<TextRange> = Vec::new();
        for (item, site) in sites {
            // Other spellings (an import alias, `Self`) and names synthesized by forms stay.
            if slice(text, site.range) != Some(name) {
                continue;
            }
            let Some(symbol) = site.found.symbol(item) else {
                continue;
            };
            if !target.symbols.contains(&symbol) {
                if site.shorthand.is_none() {
                    kept.push(site.range);
                }
                continue;
            }
            if let Some(edit) = site_edit(text, &site, name, new_name) {
                edits.push(edit);
            }
        }
        if edits.iter().any(|edit| kept.contains(&edit.range)) {
            return Err(RenameError(format!(
                "`{name}` cannot be renamed: a `use` in `{}` imports it together with another \
                 definition named `{name}`",
                file.path(db).display(None)
            )));
        }
        edits.sort_by_key(|edit| (edit.range.start(), edit.range.end()));
        edits.dedup();
        if !edits.is_empty() {
            out.push(FileEdits {
                file: file.path(db).clone(),
                edits,
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
}

/// The edit renaming `site` from `name` to `new_name`.
fn site_edit(text: &str, site: &Site, name: &str, new_name: &str) -> Option<TextEdit> {
    let Some(whole) = site.shorthand else {
        return Some(TextEdit {
            range: site.range,
            new_text: new_name.to_owned(),
        });
    };
    // `S { x }` names the field and the value (or binding) at once: spell out both.
    let whole_text = slice(text, whole)?;
    let new_text = if matches!(site.found, Found::Field(_)) {
        format!("{new_name}: {whole_text}")
    } else {
        let offset = |at: TextSize| at.checked_sub(whole.start()).map(usize::from);
        let before = whole_text.get(..offset(site.range.start())?)?;
        let after = whole_text.get(offset(site.range.end())?..)?;
        format!("{name}: {before}{new_name}{after}")
    };
    Some(TextEdit {
        range: whole,
        new_text,
    })
}

/// What a rename at an offset renames.
struct Target {
    /// The name at the offset.
    range: TextRange,
    name: Name,
    /// The definition plus the ones that must keep its name (see [`related`]).
    symbols: Vec<Symbol>,
}

fn target(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Result<Target, RenameError> {
    let error = |message: String| Err(RenameError(message));
    let Some((owner, range, found)) = reference_at(db, program, file, offset) else {
        return error("there is nothing to rename here".to_owned());
    };
    let text = file
        .text(db)
        .as_deref()
        .and_then(|text| slice(text, range))
        .unwrap_or_default()
        .to_owned();
    let Some((symbol, name)) = found
        .symbol(owner)
        .and_then(|symbol| Some((symbol, symbol_name(db, program, symbol)?)))
    else {
        return error(format!("`{text}` cannot be renamed"));
    };
    if SyntaxKind::from_keyword(&name).is_some() {
        return error(format!("`{name}` cannot be renamed"));
    }
    if text != name {
        return error(format!(
            "`{text}` refers to `{name}`; rename `{name}` where it is declared"
        ));
    }
    let maps = def_maps(db, program);
    let file_module = || {
        error(format!(
            "`{name}` is the module of a file; rename the file instead"
        ))
    };
    // The name of a `mod` item is the module's definition.
    let symbol = match symbol {
        Symbol::Item(item)
            if matches!(
                item_tree(db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Mod { .. })
            ) =>
        {
            match maps.inline_modules.get(&(item.file, item.index)) {
                Some(module) => Symbol::Module(*module),
                None => return file_module(),
            }
        }
        other => other,
    };
    if let Symbol::Module(module) = symbol
        && let ModuleOrigin::File(_) = maps.module(module).origin
    {
        return file_module();
    }
    let in_game = |symbol: Symbol| {
        let file = symbol_file(db, program, symbol);
        maps.files
            .iter()
            .any(|(loaded, krate)| *loaded == file && *krate == CrateId::Game)
    };
    if !in_game(symbol) {
        return error(format!("`{name}` is defined in the standard library"));
    }
    Ok(Target {
        range,
        name,
        symbols: related(db, program, symbol),
    })
}

/// The name `symbol` is declared with.
fn symbol_name(db: &dyn Db, program: Program, symbol: Symbol) -> Option<Name> {
    match symbol {
        Symbol::Item(item) => item_tree(db, item.file).item(item.index)?.name.clone(),
        Symbol::Variant { enum_item, index } => {
            match &item_tree(db, enum_item.file).item(enum_item.index)?.kind {
                ItemKind::Enum { variants } => {
                    Some(variants.get(usize::try_from(index).ok()?)?.0.clone())
                }
                _ => None,
            }
        }
        Symbol::Module(module) => def_maps(db, program).module(module).name.clone(),
        Symbol::Local { owner, local } => {
            let resolved = body_resolution(db, program, owner).body.as_ref()?;
            Some(resolved.locals[local].name.clone())
        }
        Symbol::GenericParam { owner, index } => {
            let generics = &signature_of(db, owner)?.generics;
            Some(generics.get(usize::try_from(index).ok()?)?.name.clone())
        }
        Symbol::Field(field) => {
            let names = record_field_names(db, field.adt, field.variant)?;
            names.get(usize::try_from(field.index).ok()?).cloned()
        }
    }
}

/// The file `symbol` is declared in.
fn symbol_file(db: &dyn Db, program: Program, symbol: Symbol) -> SourceFile {
    match symbol {
        Symbol::Item(item) => item.file,
        Symbol::Variant { enum_item, .. } => enum_item.file,
        Symbol::Module(module) => def_maps(db, program).module(module).origin.file(),
        Symbol::Local { owner, .. } | Symbol::GenericParam { owner, .. } => owner.file,
        Symbol::Field(field) => field.adt.file,
    }
}

/// `symbol` plus the definitions that must keep its name: for an inline module, its `mod`
/// item. (Traits and trait impls are declared only in std, whose items are never renamed.)
fn related(db: &dyn Db, program: Program, symbol: Symbol) -> Vec<Symbol> {
    let mut out = vec![symbol];
    if let Symbol::Module(module) = symbol
        && let ModuleOrigin::Inline { file, item } = def_maps(db, program).module(module).origin
    {
        out.push(Symbol::Item(ItemId { file, index: item }));
    }
    out
}

/// Whether `name` can name a definition: one identifier, not a keyword.
fn check_name(name: &str) -> Result<(), RenameError> {
    let (tokens, errors) = kagari_syntax::lex::lex(name);
    let valid = errors.is_empty()
        && matches!(
            tokens.as_slice(),
            [(SyntaxKind::IDENT, range)] if usize::from(range.len()) == name.len()
        );
    if valid {
        Ok(())
    } else {
        Err(RenameError(format!("`{name}` is not a valid name")))
    }
}

fn slice(text: &str, range: TextRange) -> Option<&str> {
    text.get(usize::from(range.start())..usize::from(range.end()))
}
