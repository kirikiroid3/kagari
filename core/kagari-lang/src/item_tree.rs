//! Item trees: every item of a file, with names, visibility, attributes and nesting, plus
//! the item-level checks of stage 1.

use kagari_syntax::ast::{self, AstNode, HasAttrs, HasName, HasVisibility};
use kagari_syntax::{SyntaxNode, TextRange};
use rowan::ast::SyntaxNodePtr;
use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::diagnostic::{Anchor, Diag, DiagnosticCode};
use crate::hir::lower::lower_path;
use crate::hir::{Name, Path, PathKind};
use crate::input::{FilePath, SourceFile};
use crate::parse_query::parse;

pub type SyntaxPtr = SyntaxNodePtr<kagari_syntax::KagariLanguage>;

macro_rules! index_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }

            pub fn from_usize(index: usize) -> Option<Self> {
                u32::try_from(index).ok().map(Self)
            }
        }
    };
}
pub(crate) use index_newtype;

index_newtype!(
    /// An index into [`ItemTree::items`].
    ItemIndex
);
index_newtype!(
    /// An index into [`ItemTree::blocks`].
    BlockIndex
);

/// An item of a file: `(file, index into ItemTree::items)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemId {
    pub file: SourceFile,
    pub index: ItemIndex,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemTree {
    /// Every item of the file, in pre-order.
    pub items: Vec<Item>,
    /// The file's top-level items.
    pub top: Vec<ItemIndex>,
    /// Blocks that directly contain items.
    pub blocks: Vec<BlockScope>,
}

/// Where the items and item blocks of a file are in its text, indexed like
/// [`ItemTree::items`] and [`ItemTree::blocks`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemSourceMap {
    pub items: Vec<ItemSrc>,
    pub blocks: Vec<SyntaxPtr>,
    pub item_by_ptr: FxHashMap<SyntaxPtr, ItemIndex>,
    pub block_by_ptr: FxHashMap<SyntaxPtr, BlockIndex>,
}

/// The source of one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSrc {
    pub ptr: SyntaxPtr,
    /// The name, or the whole item when it has none.
    pub name: TextRange,
    /// Indexed like [`Item::attrs`].
    pub attrs: Vec<TextRange>,
    /// The names of an enum's variants.
    pub variants: Vec<TextRange>,
    /// The imports of a `use` item: each import's range and its path segments' ranges.
    pub imports: Vec<(TextRange, Vec<TextRange>)>,
}

/// An item tree with its source map and the item-level diagnostics. Only the projections
/// below read it: a semantic query reading it would re-run on every edit of the file.
#[derive(Debug, Default)]
pub struct ItemTreeRaw {
    pub tree: ItemTree,
    pub source_map: ItemSourceMap,
    pub diagnostics: Vec<Diag>,
}

impl ItemTree {
    pub fn item(&self, index: ItemIndex) -> Option<&Item> {
        self.items.get(index.idx())
    }

    pub fn block(&self, index: BlockIndex) -> Option<&BlockScope> {
        self.blocks.get(index.idx())
    }

    /// Every item index of the file, in pre-order.
    pub fn indices(&self) -> impl Iterator<Item = ItemIndex> + use<> {
        (0..self.items.len()).filter_map(ItemIndex::from_usize)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub name: Option<Name>,
    pub vis: Visibility,
    pub attrs: Vec<Name>,
    pub kind: ItemKind,
    pub parent: ItemParent,
}

impl Item {
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|attr| attr == name)
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
    Item(ItemIndex),
    Block(BlockIndex),
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
        variants: Vec<(Name, Shape)>,
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
        items: Vec<ItemIndex>,
    },
    Impl {
        items: Vec<ItemIndex>,
        has_trait: bool,
    },
    Mod {
        /// The items of an inline module; `None` for `mod foo;`.
        inline: Option<Vec<ItemIndex>>,
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
    /// The `fn`, `const` or `static` whose body contains the block.
    pub owner: ItemIndex,
    pub parent_block: Option<BlockIndex>,
    pub items: Vec<ItemIndex>,
}

const KNOWN_ATTRIBUTES: &[&str] = &["scene", "entry", "must_use", "builtin", "shader_arg"];

#[salsa::tracked(returns(ref), no_eq)]
pub fn item_tree_raw(db: &dyn Db, file: SourceFile) -> ItemTreeRaw {
    let root = parse(db, file).tree();
    let mut ctx = Ctx {
        out: ItemTreeRaw::default(),
        file,
        in_std: matches!(file.path(db), FilePath::Std(_)),
    };
    let mut top = Vec::new();
    for item in root.items() {
        if let Some(index) = ctx.lower_item(item, ItemParent::File) {
            top.push(index);
        }
    }
    ctx.out.tree.top = top;
    ctx.out
}

/// The items of a file, without text ranges.
#[salsa::tracked(returns(ref))]
pub fn item_tree(db: &dyn Db, file: SourceFile) -> ItemTree {
    item_tree_raw(db, file).tree.clone()
}

#[salsa::tracked(returns(ref))]
pub fn item_source_map(db: &dyn Db, file: SourceFile) -> ItemSourceMap {
    item_tree_raw(db, file).source_map.clone()
}

/// The item-level diagnostics of a file (unknown attributes, missing bodies, …).
#[salsa::tracked(returns(ref))]
pub fn item_tree_diagnostics(db: &dyn Db, file: SourceFile) -> Vec<Diag> {
    item_tree_raw(db, file).diagnostics.clone()
}

struct Ctx {
    out: ItemTreeRaw,
    file: SourceFile,
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
        self.out
            .diagnostics
            .push(Diag::new(Anchor::Text(self.file, range), code, message));
    }

    fn lower_item(&mut self, item: ast::Item, parent: ItemParent) -> Option<ItemIndex> {
        let syntax = item.syntax().clone();
        let range = syntax.text_range();
        let attrs = self.lower_attrs(item.attrs());
        let builtin = attrs.iter().any(|(name, _)| name == "builtin");
        let (attrs, attr_ranges): (Vec<Name>, Vec<TextRange>) = attrs.into_iter().unzip();
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
        let mut variant_ranges = Vec::new();
        let mut import_ranges = Vec::new();

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
            ast::Item::Enum(it) => {
                let mut variants = Vec::new();
                for variant in it
                    .variant_list()
                    .into_iter()
                    .flat_map(|list| list.variants())
                {
                    let Some(name) = variant.name() else {
                        continue;
                    };
                    variant_ranges.push(name.syntax().text_range());
                    variants.push((
                        name_text(&name),
                        shape_of(variant.record_field_list(), variant.tuple_field_list()),
                    ));
                }
                ItemKind::Enum { variants }
            }
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
                let (imports, ranges): (Vec<Import>, Vec<_>) = imports.into_iter().unzip();
                import_ranges = ranges;
                ItemKind::Use { imports }
            }
        };

        let index = ItemIndex(len_u32(self.out.tree.items.len()));
        let ptr = SyntaxPtr::new(&syntax);
        self.out.tree.items.push(Item {
            name,
            vis,
            attrs,
            kind,
            parent,
        });
        self.out.source_map.items.push(ItemSrc {
            ptr,
            name: name_range,
            attrs: attr_ranges,
            variants: variant_ranges,
            imports: import_ranges,
        });
        self.out.source_map.item_by_ptr.insert(ptr, index);

        // Children: inline module items, associated items, bodies.
        match &item {
            ast::Item::Mod(it) => {
                if let Some(list) = it.item_list() {
                    let children: Vec<ItemIndex> = list
                        .items()
                        .filter_map(|child| self.lower_item(child, ItemParent::Item(index)))
                        .collect();
                    if let Some(Item {
                        kind: ItemKind::Mod { inline },
                        ..
                    }) = self.out.tree.items.get_mut(index.idx())
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
                }) = self.out.tree.items.get_mut(index.idx())
                {
                    *items = children;
                }
            }
            ast::Item::Impl(it) => {
                let children = self.lower_assoc_items(it.assoc_item_list(), index);
                if let Some(Item {
                    kind: ItemKind::Impl { items, .. },
                    ..
                }) = self.out.tree.items.get_mut(index.idx())
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

    fn lower_assoc_items(
        &mut self,
        list: Option<ast::AssocItemList>,
        owner: ItemIndex,
    ) -> Vec<ItemIndex> {
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
                self.out.tree.items.get(index.idx()).map(|item| &item.kind),
                Some(ItemKind::Trait { .. })
            ),
            ItemParent::File | ItemParent::Block(_) => false,
        }
    }

    /// Finds blocks with items in the body (or initializer) of item `owner`.
    fn walk_body(&mut self, node: &SyntaxNode, owner: ItemIndex, current: Option<BlockIndex>) {
        if ast::BlockExpr::can_cast(node.kind()) {
            let has_items = node
                .children()
                .any(|child| ast::Item::can_cast(child.kind()));
            if has_items {
                let index = BlockIndex(len_u32(self.out.tree.blocks.len()));
                let ptr = SyntaxPtr::new(node);
                self.out.tree.blocks.push(BlockScope {
                    owner,
                    parent_block: current,
                    items: Vec::new(),
                });
                self.out.source_map.blocks.push(ptr);
                self.out.source_map.block_by_ptr.insert(ptr, index);
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
                if let Some(block) = self.out.tree.blocks.get_mut(index.idx()) {
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

/// A path with the ranges of its segments.
type RangedPath = (Path, Vec<TextRange>);

fn empty_path() -> RangedPath {
    (
        Path {
            kind: PathKind::Plain,
            segments: Vec::new(),
        },
        Vec::new(),
    )
}

fn is_empty_path(path: &Path) -> bool {
    path.kind == PathKind::Plain && path.segments.is_empty()
}

/// `prefix` followed by `rest` (whose own kind is used only when `prefix` is empty).
fn join_paths(prefix: &RangedPath, rest: RangedPath) -> RangedPath {
    if is_empty_path(&prefix.0) {
        return rest;
    }
    let (mut joined, mut ranges) = prefix.clone();
    joined.segments.extend(rest.0.segments);
    ranges.extend(rest.1);
    (joined, ranges)
}

fn is_self_path(path: &Path) -> bool {
    path.kind == PathKind::Plain
        && path.segments.len() == 1
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.name == "self")
}

/// Flattens a use tree into imports, each with its range and its path segments' ranges.
fn flatten_use_tree(
    tree: &ast::UseTree,
    prefix: &RangedPath,
    out: &mut Vec<(Import, (TextRange, Vec<TextRange>))>,
) {
    let range = tree.syntax().text_range();
    let own = tree
        .path()
        .and_then(|path| lower_path(&path, &mut |_| None))
        .map_or_else(empty_path, |(path, ranges)| (path, ranges.to_vec()));
    let alias = tree
        .rename()
        .and_then(|rename| rename.name())
        .map(|name| name_text(&name));
    let (path, segments) = if tree.star_token().is_some() {
        let (path, segments) = join_paths(prefix, own);
        let import = Import {
            path,
            alias: None,
            glob: true,
        };
        out.push((import, (range, segments)));
        return;
    } else if let Some(list) = tree.use_tree_list() {
        let base = join_paths(prefix, own);
        for sub in list.use_trees() {
            flatten_use_tree(&sub, &base, out);
        }
        return;
    } else if is_self_path(&own.0) && !is_empty_path(&prefix.0) {
        // `a::{self}` imports `a` itself.
        prefix.clone()
    } else if !is_empty_path(&own.0) {
        join_paths(prefix, own)
    } else {
        return;
    };
    let import = Import {
        path,
        alias,
        glob: false,
    };
    out.push((import, (range, segments)));
}
