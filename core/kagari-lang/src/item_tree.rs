//! Item trees: every item of a file, with names, visibility, attributes and nesting, plus
//! the item-level checks of stage 1.

use kagari_syntax::ast::{self, AstNode, HasAttrs, HasName, HasVisibility};
use kagari_syntax::{SyntaxNode, TextRange};
use rowan::ast::SyntaxNodePtr;
use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::diagnostic::DiagnosticCode;
use crate::hir::lower::lower_path;
use crate::hir::{Name, Path, PathKind};
use crate::input::{FilePath, SourceFile};
use crate::parse_query::parse;

pub type SyntaxPtr = SyntaxNodePtr<kagari_syntax::KagariLanguage>;

/// An item of a file: `(file, index into ItemTree::items)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemId {
    pub file: SourceFile,
    pub index: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemTree {
    /// Every item of the file, in pre-order.
    pub items: Vec<Item>,
    /// The file's top-level items.
    pub top: Vec<u32>,
    /// Blocks that directly contain items.
    pub blocks: Vec<BlockScope>,
    pub item_by_ptr: FxHashMap<SyntaxPtr, u32>,
    pub block_by_ptr: FxHashMap<SyntaxPtr, u32>,
    pub diagnostics: Vec<(TextRange, DiagnosticCode, String)>,
}

impl ItemTree {
    pub fn item(&self, index: u32) -> Option<&Item> {
        self.items.get(index as usize)
    }

    pub fn block(&self, index: u32) -> Option<&BlockScope> {
        self.blocks.get(index as usize)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub name: Option<Name>,
    pub name_range: TextRange,
    pub ptr: SyntaxPtr,
    pub vis: Visibility,
    pub attrs: Vec<(Name, TextRange)>,
    pub kind: ItemKind,
    pub parent: ItemParent,
}

impl Item {
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(attr, _)| attr == name)
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ItemKind::Fn { .. } => "fn",
            ItemKind::Struct { .. } => "struct",
            ItemKind::Enum { .. } => "enum",
            ItemKind::TypeAlias { .. } => "type",
            ItemKind::Const { .. } => "const",
            ItemKind::Static { .. } => "static",
            ItemKind::Trait { .. } => "trait",
            ItemKind::Impl { .. } => "impl",
            ItemKind::Mod { .. } => "module",
            ItemKind::Use { .. } => "use",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemParent {
    File,
    /// An inline module, impl or trait.
    Item(u32),
    Block(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnKind {
    Plain,
    Const,
    Pushdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Unit,
    Tuple(u32),
    Record(Vec<Name>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Fn {
        kind: FnKind,
        has_body: bool,
        has_self: bool,
    },
    Struct {
        shape: Shape,
    },
    Enum {
        variants: Vec<(Name, TextRange, Shape)>,
    },
    TypeAlias {
        has_value: bool,
    },
    Const {
        has_value: bool,
    },
    Static {
        mutable: bool,
    },
    Trait {
        items: Vec<u32>,
    },
    Impl {
        items: Vec<u32>,
        has_trait: bool,
    },
    Mod {
        /// The items of an inline module; `None` for `mod foo;`.
        inline: Option<Vec<u32>>,
    },
    Use {
        imports: Vec<Import>,
    },
}

/// One flattened import of a `use` item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub path: Path,
    pub alias: Option<Name>,
    pub glob: bool,
    pub range: TextRange,
}

impl Import {
    /// The name the import binds (named imports only).
    pub fn name(&self) -> Option<&Name> {
        self.alias
            .as_ref()
            .or_else(|| self.path.segments.last().map(|segment| &segment.name))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockScope {
    pub ptr: SyntaxPtr,
    /// The `fn`, `const` or `static` whose body contains the block.
    pub owner: u32,
    pub parent_block: Option<u32>,
    pub items: Vec<u32>,
}

const KNOWN_ATTRIBUTES: &[&str] = &["scene", "entry", "must_use", "builtin"];

#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    let root = parse(db, file).tree();
    let mut ctx = Ctx {
        tree: ItemTree::default(),
        in_std: matches!(file.path(db), FilePath::Std(_)),
    };
    let mut top = Vec::new();
    for item in root.items() {
        if let Some(index) = ctx.lower_item(item, ItemParent::File) {
            top.push(index);
        }
    }
    ctx.tree.top = top;
    ctx.tree
}

struct Ctx {
    tree: ItemTree,
    in_std: bool,
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The text of a name node.
pub(crate) fn name_text(name: &ast::Name) -> Name {
    Name::new(name.text())
}

fn name_and_range(node: &impl HasName, fallback: TextRange) -> (Option<Name>, TextRange) {
    match node.name() {
        Some(name) => (Some(name_text(&name)), name.syntax().text_range()),
        None => (None, fallback),
    }
}

fn shape_of(record: Option<ast::RecordFieldList>, tuple: Option<ast::TupleFieldList>) -> Shape {
    if let Some(record) = record {
        Shape::Record(
            record
                .fields()
                .filter_map(|field| field.name().map(|name| name_text(&name)))
                .collect(),
        )
    } else if let Some(tuple) = tuple {
        Shape::Tuple(len_u32(tuple.fields().count()))
    } else {
        Shape::Unit
    }
}

impl Ctx {
    fn diag(&mut self, range: TextRange, code: DiagnosticCode, message: String) {
        self.tree.diagnostics.push((range, code, message));
    }

    fn lower_item(&mut self, item: ast::Item, parent: ItemParent) -> Option<u32> {
        let syntax = item.syntax().clone();
        let range = syntax.text_range();
        let attrs = self.lower_attrs(item.attrs());
        let builtin = attrs.iter().any(|(name, _)| name == "builtin");
        let vis = if item.visibility().is_some() {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let in_trait = self.parent_is_trait(parent);
        let (name, name_range) = match &item {
            ast::Item::Fn(it) => name_and_range(it, range),
            ast::Item::Struct(it) => name_and_range(it, range),
            ast::Item::Enum(it) => name_and_range(it, range),
            ast::Item::TypeAlias(it) => name_and_range(it, range),
            ast::Item::Const(it) => name_and_range(it, range),
            ast::Item::Static(it) => name_and_range(it, range),
            ast::Item::Mod(it) => name_and_range(it, range),
            ast::Item::Trait(it) => name_and_range(it, range),
            ast::Item::Impl(_) | ast::Item::Use(_) => (None, range),
        };
        let shown = name.clone().unwrap_or_default();

        let kind = match &item {
            ast::Item::Fn(it) => {
                let has_body = it.body().is_some();
                if builtin && has_body {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemBuiltinWithBody,
                        "a `#[builtin]` function has no body".to_owned(),
                    );
                }
                if !builtin && !has_body && !in_trait {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemMissingBody,
                        format!("function `{shown}` has no body"),
                    );
                }
                ItemKind::Fn {
                    kind: match it.fn_kind() {
                        ast::FnKind::Plain => FnKind::Plain,
                        ast::FnKind::Const => FnKind::Const,
                        ast::FnKind::Pushdown => FnKind::Pushdown,
                    },
                    has_body,
                    has_self: it
                        .param_list()
                        .and_then(|params| params.self_param())
                        .is_some(),
                }
            }
            ast::Item::Struct(it) => {
                let shape = shape_of(it.record_field_list(), it.tuple_field_list());
                if builtin && shape != Shape::Unit {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemBuiltinWithBody,
                        "a `#[builtin]` struct has no fields".to_owned(),
                    );
                }
                ItemKind::Struct { shape }
            }
            ast::Item::Enum(it) => ItemKind::Enum {
                variants: it
                    .variant_list()
                    .into_iter()
                    .flat_map(|list| list.variants())
                    .filter_map(|variant| {
                        let name = variant.name()?;
                        Some((
                            name_text(&name),
                            name.syntax().text_range(),
                            shape_of(variant.record_field_list(), variant.tuple_field_list()),
                        ))
                    })
                    .collect(),
            },
            ast::Item::TypeAlias(it) => {
                let has_value = it.ty().is_some();
                if !has_value && !in_trait {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemMissingValue,
                        format!("type alias `{shown}` has no type"),
                    );
                }
                ItemKind::TypeAlias { has_value }
            }
            ast::Item::Const(it) => {
                let has_value = it.body().is_some();
                if !has_value && !in_trait {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemMissingValue,
                        format!("constant `{shown}` has no value"),
                    );
                }
                ItemKind::Const { has_value }
            }
            ast::Item::Static(it) => ItemKind::Static {
                mutable: it.mut_token().is_some(),
            },
            ast::Item::Trait(_) => {
                if !self.in_std {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemTraitOutsideStd,
                        "traits are only declared in the standard library".to_owned(),
                    );
                } else if !builtin {
                    self.diag(
                        name_range,
                        DiagnosticCode::ItemTraitNotBuiltin,
                        format!("standard library trait `{shown}` must be `#[builtin]`"),
                    );
                }
                ItemKind::Trait { items: Vec::new() }
            }
            ast::Item::Impl(it) => ItemKind::Impl {
                items: Vec::new(),
                has_trait: it.trait_type().is_some(),
            },
            ast::Item::Mod(it) => {
                if it.item_list().is_none() && matches!(parent, ItemParent::Block(_)) {
                    self.diag(
                        name_range,
                        DiagnosticCode::ModuleInBlock,
                        "file modules cannot be declared inside a block".to_owned(),
                    );
                }
                ItemKind::Mod {
                    inline: it.item_list().map(|_| Vec::new()),
                }
            }
            ast::Item::Use(it) => {
                let mut imports = Vec::new();
                if let Some(tree) = it.use_tree() {
                    flatten_use_tree(&tree, &empty_path(), &mut imports);
                }
                ItemKind::Use { imports }
            }
        };

        let index = len_u32(self.tree.items.len());
        let ptr = SyntaxPtr::new(&syntax);
        self.tree.items.push(Item {
            name,
            name_range,
            ptr,
            vis,
            attrs,
            kind,
            parent,
        });
        self.tree.item_by_ptr.insert(ptr, index);

        // Children: inline module items, associated items, bodies.
        match &item {
            ast::Item::Mod(it) => {
                if let Some(list) = it.item_list() {
                    let children: Vec<u32> = list
                        .items()
                        .filter_map(|child| self.lower_item(child, ItemParent::Item(index)))
                        .collect();
                    if let Some(Item {
                        kind: ItemKind::Mod { inline },
                        ..
                    }) = self.tree.items.get_mut(index as usize)
                    {
                        *inline = Some(children);
                    }
                }
            }
            ast::Item::Trait(it) => {
                let children = self.lower_assoc_items(it.assoc_item_list(), index);
                if let Some(Item {
                    kind: ItemKind::Trait { items },
                    ..
                }) = self.tree.items.get_mut(index as usize)
                {
                    *items = children;
                }
            }
            ast::Item::Impl(it) => {
                let children = self.lower_assoc_items(it.assoc_item_list(), index);
                if let Some(Item {
                    kind: ItemKind::Impl { items, .. },
                    ..
                }) = self.tree.items.get_mut(index as usize)
                {
                    *items = children;
                }
            }
            ast::Item::Fn(it) => {
                if let Some(body) = it.body() {
                    self.walk_body(body.syntax(), index, None);
                }
            }
            ast::Item::Const(it) => {
                if let Some(body) = it.body() {
                    self.walk_body(body.syntax(), index, None);
                }
            }
            ast::Item::Static(it) => {
                if let Some(body) = it.body() {
                    self.walk_body(body.syntax(), index, None);
                }
            }
            ast::Item::Struct(_)
            | ast::Item::Enum(_)
            | ast::Item::TypeAlias(_)
            | ast::Item::Use(_) => {}
        }
        Some(index)
    }

    fn lower_assoc_items(&mut self, list: Option<ast::AssocItemList>, owner: u32) -> Vec<u32> {
        let Some(list) = list else {
            return Vec::new();
        };
        list.assoc_items()
            .filter_map(|assoc| {
                let item = match assoc {
                    ast::AssocItem::Fn(it) => ast::Item::Fn(it),
                    ast::AssocItem::Const(it) => ast::Item::Const(it),
                    ast::AssocItem::TypeAlias(it) => ast::Item::TypeAlias(it),
                };
                self.lower_item(item, ItemParent::Item(owner))
            })
            .collect()
    }

    fn parent_is_trait(&self, parent: ItemParent) -> bool {
        match parent {
            ItemParent::Item(index) => matches!(
                self.tree.items.get(index as usize).map(|item| &item.kind),
                Some(ItemKind::Trait { .. })
            ),
            ItemParent::File | ItemParent::Block(_) => false,
        }
    }

    /// Finds blocks with items in the body (or initializer) of item `owner`.
    fn walk_body(&mut self, node: &SyntaxNode, owner: u32, current: Option<u32>) {
        if ast::BlockExpr::can_cast(node.kind()) {
            let has_items = node
                .children()
                .any(|child| ast::Item::can_cast(child.kind()));
            if has_items {
                let index = len_u32(self.tree.blocks.len());
                let ptr = SyntaxPtr::new(node);
                self.tree.blocks.push(BlockScope {
                    ptr,
                    owner,
                    parent_block: current,
                    items: Vec::new(),
                });
                self.tree.block_by_ptr.insert(ptr, index);
                let mut items = Vec::new();
                for child in node.children() {
                    if let Some(item) = ast::Item::cast(child.clone()) {
                        if let Some(item) = self.lower_item(item, ItemParent::Block(index)) {
                            items.push(item);
                        }
                    } else {
                        self.walk_body(&child, owner, Some(index));
                    }
                }
                if let Some(block) = self.tree.blocks.get_mut(index as usize) {
                    block.items = items;
                }
                return;
            }
        }
        for child in node.children() {
            // Items are only valid as block statements; anything else nested is not an item.
            self.walk_body(&child, owner, current);
        }
    }

    fn lower_attrs(&mut self, attrs: impl Iterator<Item = ast::Attr>) -> Vec<(Name, TextRange)> {
        let mut out = Vec::new();
        for attr in attrs {
            let range = attr.syntax().text_range();
            let Some(path) = attr.path() else {
                continue;
            };
            let name = Name::new(path.syntax().text().to_string().trim());
            if !KNOWN_ATTRIBUTES.contains(&name.as_str()) {
                self.diag(
                    range,
                    DiagnosticCode::ItemUnknownAttribute,
                    format!("unknown attribute `#[{name}]`"),
                );
                continue;
            }
            if attr.token_tree().is_some() {
                self.diag(
                    range,
                    DiagnosticCode::ItemAttributeArguments,
                    format!("`#[{name}]` takes no arguments"),
                );
            }
            if name == "builtin" && !self.in_std {
                self.diag(
                    range,
                    DiagnosticCode::ItemBuiltinOutsideStd,
                    "`#[builtin]` is only allowed in the standard library".to_owned(),
                );
            }
            out.push((name, range));
        }
        out
    }
}

fn empty_path() -> Path {
    Path {
        kind: PathKind::Plain,
        segments: Vec::new(),
    }
}

/// `prefix` followed by `rest` (whose own kind is used only when `prefix` is empty).
fn join_paths(prefix: &Path, rest: Path) -> Path {
    if prefix.kind == PathKind::Plain && prefix.segments.is_empty() {
        return rest;
    }
    let mut joined = prefix.clone();
    joined.segments.extend(rest.segments);
    joined
}

fn is_self_path(path: &Path) -> bool {
    path.kind == PathKind::Plain
        && path.segments.len() == 1
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.name == "self")
}

fn flatten_use_tree(tree: &ast::UseTree, prefix: &Path, out: &mut Vec<Import>) {
    let range = tree.syntax().text_range();
    let own = tree
        .path()
        .and_then(|path| lower_path(&path, &mut |_| None))
        .unwrap_or_else(empty_path);
    let alias = tree
        .rename()
        .and_then(|rename| rename.name())
        .map(|name| name_text(&name));
    if tree.star_token().is_some() {
        out.push(Import {
            path: join_paths(prefix, own),
            alias: None,
            glob: true,
            range,
        });
    } else if let Some(list) = tree.use_tree_list() {
        let base = join_paths(prefix, own);
        for sub in list.use_trees() {
            flatten_use_tree(&sub, &base, out);
        }
    } else if is_self_path(&own) && !(prefix.kind == PathKind::Plain && prefix.segments.is_empty())
    {
        // `a::{self}` imports `a` itself.
        out.push(Import {
            path: prefix.clone(),
            alias,
            glob: false,
            range,
        });
    } else if !own.segments.is_empty() || own.kind != PathKind::Plain {
        out.push(Import {
            path: join_paths(prefix, own),
            alias,
            glob: false,
            range,
        });
    }
}
