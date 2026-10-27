//! The outline of a file: its items, nested as written, with enum variants.

use kagari_syntax::TextRange;

use super::header::item_header;
use crate::db::Db;
use crate::input::SourceFile;
use crate::item_tree::{
    ItemIndex, ItemKind, ItemParent, ItemSourceMap, ItemTree, item_source_map, item_tree,
};
use crate::parse_query::parse;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Module,
    Function,
    Method,
    Struct,
    Enum,
    Variant,
    TypeAlias,
    Const,
    Static,
    Trait,
    Impl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSymbol {
    pub name: String,
    pub kind: SymbolKind,
    /// The whole definition.
    pub full_range: TextRange,
    /// The name (the whole item for an `impl`).
    pub focus_range: TextRange,
    pub children: Vec<FileSymbol>,
}

/// The items of `file` outside function bodies, top-level ones first, in source order. `use`
/// items are left out.
pub fn file_symbols(db: &dyn Db, file: SourceFile) -> Vec<FileSymbol> {
    let cx = Cx {
        tree: item_tree(db, file),
        map: item_source_map(db, file),
        root: parse(db, file).syntax_node(),
    };
    cx.symbols(&cx.tree.top)
}

struct Cx<'a> {
    tree: &'a ItemTree,
    map: &'a ItemSourceMap,
    root: kagari_syntax::SyntaxNode,
}

impl Cx<'_> {
    fn symbols(&self, items: &[ItemIndex]) -> Vec<FileSymbol> {
        items
            .iter()
            .filter_map(|&index| self.symbol(index))
            .collect()
    }

    fn symbol(&self, index: ItemIndex) -> Option<FileSymbol> {
        let item = self.tree.item(index)?;
        let src = self.map.items.get(index.idx())?;
        let kind = match &item.kind {
            ItemKind::Fn { has_self, .. } => {
                let in_impl_or_trait = match item.parent {
                    ItemParent::Item(parent) => self.tree.item(parent).is_some_and(|parent| {
                        matches!(parent.kind, ItemKind::Impl { .. } | ItemKind::Trait { .. })
                    }),
                    _ => false,
                };
                if in_impl_or_trait && *has_self {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                }
            }
            ItemKind::Struct { .. } => SymbolKind::Struct,
            ItemKind::Enum { .. } => SymbolKind::Enum,
            ItemKind::TypeAlias { .. } => SymbolKind::TypeAlias,
            ItemKind::Const { .. } => SymbolKind::Const,
            ItemKind::Static { .. } => SymbolKind::Static,
            ItemKind::Trait { .. } => SymbolKind::Trait,
            ItemKind::Impl { .. } => SymbolKind::Impl,
            ItemKind::Mod { .. } => SymbolKind::Module,
            ItemKind::Use { .. } => return None,
        };
        let name = match (&item.kind, &item.name) {
            (ItemKind::Impl { .. }, _) => item_header(&src.ptr.to_node(&self.root)),
            (_, Some(name)) => name.to_string(),
            (_, None) => "?".to_owned(),
        };
        let mut children = match &item.kind {
            ItemKind::Mod {
                inline: Some(items),
            }
            | ItemKind::Trait { items }
            | ItemKind::Impl { items, .. } => self.symbols(items),
            _ => Vec::new(),
        };
        if let ItemKind::Enum { variants } = &item.kind {
            children.extend(
                variants
                    .iter()
                    .zip(&src.variants)
                    .map(|((name, _), &range)| FileSymbol {
                        name: name.to_string(),
                        kind: SymbolKind::Variant,
                        full_range: range,
                        focus_range: range,
                        children: Vec::new(),
                    }),
            );
        }
        Some(FileSymbol {
            name,
            kind,
            full_range: src.ptr.text_range(),
            focus_range: src.name,
            children,
        })
    }
}
