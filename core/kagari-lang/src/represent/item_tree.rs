//! Per-file item tree of a `.kix`/`.kirp` file: names, kinds and pointers, no bodies.

use kagari_syntax::TextRange;
use kagari_syntax::represent::RepresentLanguage;
use kagari_syntax::represent::ast::{self, HasAttrs, HasGenericParams, HasName};
use rowan::ast::{AstNode, SyntaxNodePtr};
use smol_str::SmolStr;

use crate::db::Db;
use crate::input::SourceFile;
use crate::parse::parse_represent;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ItemTree {
    /// Every item of the file, nested ones included, in pre-order.
    pub items: Vec<Item>,
    /// Indices of the top-level items.
    pub top: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Empty for an `impl`.
    pub name: SmolStr,
    /// The defining name; for an `impl`, its self type (or the `impl` keyword's node).
    pub name_range: TextRange,
    pub ptr: SyntaxNodePtr<RepresentLanguage>,
    /// `(name, range of the attribute, has args)`.
    pub attrs: Vec<(SmolStr, TextRange, bool)>,
    pub kind: ItemKind,
    /// The enclosing `mod` or `impl`.
    pub parent: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Struct {
        generics: Vec<SmolStr>,
        /// `None` for a unit struct.
        fields: Option<Vec<(SmolStr, TextRange)>>,
    },
    Enum {
        generics: Vec<SmolStr>,
        /// `(name, range, payload arity)`.
        variants: Vec<(SmolStr, TextRange, u32)>,
    },
    TypeAlias {
        generics: Vec<SmolStr>,
    },
    Const,
    AssetImport {
        path: String,
    },
    Fn {
        generics: Vec<SmolStr>,
        has_self: bool,
        has_body: bool,
    },
    Mod {
        children: Vec<u32>,
    },
    Impl {
        generics: Vec<SmolStr>,
        children: Vec<u32>,
    },
}

impl ItemKind {
    /// The generic parameter names, empty for items without them.
    pub fn generics(&self) -> &[SmolStr] {
        match self {
            ItemKind::Struct { generics, .. }
            | ItemKind::Enum { generics, .. }
            | ItemKind::TypeAlias { generics }
            | ItemKind::Fn { generics, .. }
            | ItemKind::Impl { generics, .. } => generics,
            ItemKind::Const | ItemKind::AssetImport { .. } | ItemKind::Mod { .. } => &[],
        }
    }

    /// A short description for messages.
    pub fn describe(&self) -> &'static str {
        match self {
            ItemKind::Struct { .. } => "struct",
            ItemKind::Enum { .. } => "enum",
            ItemKind::TypeAlias { .. } => "type alias",
            ItemKind::Const => "constant",
            ItemKind::AssetImport { .. } => "asset import",
            ItemKind::Fn { .. } => "function",
            ItemKind::Mod { .. } => "module",
            ItemKind::Impl { .. } => "impl",
        }
    }
}

impl Item {
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(attr, _, _)| attr == name)
    }
}

/// A definition: an item of some file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DefId {
    pub file: SourceFile,
    pub index: u32,
}

#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    let mut tree = ItemTree::default();
    let root = parse_represent(db, file).syntax_node();
    if let Some(source) = ast::SourceFile::cast(root) {
        for item in source.items() {
            if let Some(index) = lower_item(&mut tree, &item, None) {
                tree.top.push(index);
            }
        }
    }
    tree
}

fn generics(node: &impl HasGenericParams) -> Vec<SmolStr> {
    node.generic_params()
        .map(|param| SmolStr::new(param.name().map(|name| name.text()).unwrap_or_default()))
        .collect()
}

fn lower_item(tree: &mut ItemTree, item: &ast::Item, parent: Option<u32>) -> Option<u32> {
    let index = u32::try_from(tree.items.len()).ok()?;
    let node = item.syntax();
    let (name, name_range) = match item.name() {
        Some(name) => (SmolStr::new(name.text()), name.syntax().text_range()),
        None => {
            let range = match item {
                ast::Item::Impl(imp) => imp
                    .self_ty()
                    .map(|ty| ty.syntax().text_range())
                    .unwrap_or_else(|| node.text_range()),
                _ => node.text_range(),
            };
            (SmolStr::default(), range)
        }
    };
    let attrs = item
        .attrs()
        .map(|attr| {
            (
                SmolStr::new(attr.name_text().unwrap_or_default()),
                attr.syntax().text_range(),
                attr.has_args(),
            )
        })
        .collect();
    let kind = match item {
        ast::Item::Fn(f) => ItemKind::Fn {
            generics: generics(f),
            has_self: f.param_list().and_then(|list| list.self_param()).is_some(),
            has_body: f.body().is_some(),
        },
        ast::Item::Struct(s) => ItemKind::Struct {
            generics: generics(s),
            fields: s.record_field_list().map(|list| {
                list.fields()
                    .map(|field| match field.name() {
                        Some(name) => (SmolStr::new(name.text()), name.syntax().text_range()),
                        None => (SmolStr::default(), field.syntax().text_range()),
                    })
                    .collect()
            }),
        },
        ast::Item::Enum(e) => ItemKind::Enum {
            generics: generics(e),
            variants: e
                .variants()
                .map(|variant| {
                    let arity = variant
                        .tuple_field_list()
                        .map(|list| list.types().count())
                        .unwrap_or(0);
                    let (name, range) = match variant.name() {
                        Some(name) => (SmolStr::new(name.text()), name.syntax().text_range()),
                        None => (SmolStr::default(), variant.syntax().text_range()),
                    };
                    (name, range, u32::try_from(arity).unwrap_or(u32::MAX))
                })
                .collect(),
        },
        ast::Item::TypeAlias(alias) => ItemKind::TypeAlias {
            generics: generics(alias),
        },
        ast::Item::Const(_) => ItemKind::Const,
        ast::Item::AssetImport(import) => ItemKind::AssetImport {
            path: import.path_value().unwrap_or_default(),
        },
        ast::Item::Mod(_) => ItemKind::Mod {
            children: Vec::new(),
        },
        ast::Item::Impl(imp) => ItemKind::Impl {
            generics: generics(imp),
            children: Vec::new(),
        },
    };
    tree.items.push(Item {
        name,
        name_range,
        ptr: SyntaxNodePtr::new(node),
        attrs,
        kind,
        parent,
    });
    let nested: Vec<ast::Item> = match item {
        ast::Item::Mod(m) => m.items().collect(),
        ast::Item::Impl(imp) => imp.items().collect(),
        _ => Vec::new(),
    };
    if nested.is_empty() {
        return Some(index);
    }
    let children: Vec<u32> = nested
        .iter()
        .filter_map(|child| lower_item(tree, child, Some(index)))
        .collect();
    if let Some(Item {
        kind: ItemKind::Mod { children: slot } | ItemKind::Impl { children: slot, .. },
        ..
    }) = tree.items.get_mut(index as usize)
    {
        *slot = children;
    }
    Some(index)
}
