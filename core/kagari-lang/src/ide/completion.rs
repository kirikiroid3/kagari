//! Completion: the names that can be written at an offset. After `.`, the fields and methods
//! of the receiver's type; after `path::` (or inside `use path::{…}`), what the path contains;
//! in a struct expression or pattern, its fields; elsewhere, the locals, generic parameters and
//! items in scope, `std`, the prelude and the primitive types.

use kagari_syntax::ast::{self, AstNode};
use kagari_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize};
use rustc_hash::FxHashSet;

use super::header::{collapse_whitespace, doc_comment, item_header};
use super::hover::{field_node, item_node, variant_node};
use super::lookup::{FieldId, item_at, node_at, record_field_names};
use crate::db::Db;
use crate::def_map::{
    Def, DefMaps, ModuleId, Namespace, PrimType, Resolution, TypeKey, def_maps,
    resolve_module_path, resolve_rest, type_key,
};
use crate::diagnostic::Anchor;
use crate::hir::lower::lower_path;
use crate::hir::{Path, PathKind, body_source_map_of, signature_of};
use crate::infer::inference;
use crate::input::{Program, SourceFile};
use crate::item_tree::{BlockIndex, ItemId, ItemKind, ItemParent, item_source_map, item_tree};
use crate::parse_query::parse;
use crate::paths::def_path;
use crate::resolve::body_resolution;
use crate::signatures::{GenericParamDefKind, display_ty, program_signatures};
use crate::ty::TyKind;

/// The completions at an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completions {
    /// The text a completion replaces: the identifier at the offset, or an empty range.
    pub range: TextRange,
    pub items: Vec<CompletionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    /// A header or type: `fn helper() -> u32`, `n: u32`, `let x: u32`.
    pub detail: Option<String>,
    /// The doc comment, Markdown.
    pub documentation: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompletionKind {
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
    Field,
    Local,
    TypeParam,
    Primitive,
}

/// What can be written at `offset` in `file`. `None` outside the program, inside comments and
/// strings, and where a new name is being declared.
pub fn completions(
    db: &dyn Db,
    program: Program,
    file: SourceFile,
    offset: TextSize,
) -> Option<Completions> {
    let maps = def_maps(db, program);
    if !maps.file_modules.contains_key(&file) {
        return None;
    }
    let root = parse(db, file).syntax_node();
    let token = root.token_at_offset(offset).left_biased()?;
    let inside = token.text_range().start() < offset;
    let (word, before) = match token.kind() {
        SyntaxKind::LINE_COMMENT => return None,
        SyntaxKind::BLOCK_COMMENT | SyntaxKind::STRING if offset < token.text_range().end() => {
            return None;
        }
        _ if inside && is_word(&token) => {
            let before = prev_token(&token);
            (Some(token), before)
        }
        kind if kind.is_trivia() => (None, prev_token(&token)),
        _ => (None, Some(token)),
    };
    let range = word
        .as_ref()
        .map_or(TextRange::empty(offset), |word| word.text_range());
    let owner = item_at(db, file, offset);
    let cx = Cx {
        db,
        program,
        maps,
        root,
        offset,
        owner,
        module: module_at(db, maps, file, offset, owner),
        out: Vec::new(),
        seen: FxHashSet::default(),
    };
    let items = cx.complete(word.as_ref(), before.as_ref())?;
    Some(Completions { range, items })
}

/// An identifier or a keyword being typed.
fn is_word(token: &SyntaxToken) -> bool {
    matches!(
        token.kind(),
        SyntaxKind::IDENT | SyntaxKind::PUSHDOWN_KW | SyntaxKind::SCENE_KW
    ) || SyntaxKind::from_keyword(token.text()).is_some()
}

/// The previous token that is not trivia.
fn prev_token(token: &SyntaxToken) -> Option<SyntaxToken> {
    let mut current = token.prev_token();
    while let Some(token) = current {
        if !token.kind().is_trivia() {
            return Some(token);
        }
        current = token.prev_token();
    }
    None
}

/// The module whose scope applies at `offset`: the innermost block with items, else the module
/// of the item there, else the file's module.
fn module_at(
    db: &dyn Db,
    maps: &DefMaps,
    file: SourceFile,
    offset: TextSize,
    owner: Option<ItemId>,
) -> ModuleId {
    let map = item_source_map(db, file);
    let mut best: Option<(ModuleId, TextRange)> = None;
    for (index, ptr) in map.blocks.iter().enumerate() {
        let range = ptr.text_range();
        let module =
            BlockIndex::from_usize(index).and_then(|block| maps.block_modules.get(&(file, block)));
        if let Some(&module) = module
            && range.contains_inclusive(offset)
            && best.is_none_or(|(_, best)| range.len() < best.len())
        {
            best = Some((module, range));
        }
    }
    if let Some((module, _)) = best {
        return module;
    }
    match owner {
        Some(item) => match maps.inline_modules.get(&(item.file, item.index)) {
            Some(&module) => module,
            None => maps.module_of_item(db, item),
        },
        None => maps
            .file_modules
            .get(&file)
            .copied()
            .unwrap_or(maps.game.root),
    }
}

struct Cx<'db> {
    db: &'db dyn Db,
    program: Program,
    maps: &'db DefMaps,
    root: SyntaxNode,
    offset: TextSize,
    /// The smallest item around the offset.
    owner: Option<ItemId>,
    module: ModuleId,
    out: Vec<CompletionItem>,
    /// Labels already offered: an inner name shadows an outer one.
    seen: FxHashSet<String>,
}

impl Cx<'_> {
    fn complete(
        mut self,
        word: Option<&SyntaxToken>,
        before: Option<&SyntaxToken>,
    ) -> Option<Vec<CompletionItem>> {
        let before_kind = before.map(SyntaxToken::kind);
        if let Some(colons) = before.and_then(path_separator) {
            let resolution = self.qualifier(&colons)?;
            self.members(&resolution);
            return Some(self.out);
        }
        match before_kind {
            Some(SyntaxKind::DOT) => self.dot(before?)?,
            _ => {
                if let Some(list) = before
                    .and_then(SyntaxToken::parent)
                    .filter(|_| {
                        matches!(before_kind, Some(SyntaxKind::L_CURLY | SyntaxKind::COMMA))
                    })
                    .filter(|parent| parent.kind() == SyntaxKind::USE_TREE_LIST)
                {
                    let resolution = self.resolve_qualifier(&use_list_path(&list)?)?;
                    self.members(&resolution);
                    return Some(self.out);
                }
                let in_record = self.record_fields(word, before);
                if in_record == Some(RecordKind::Pat) {
                    return Some(self.out);
                }
                if in_record.is_none() && declares_name(word, before_kind) {
                    return None;
                }
                self.scope(types_only(word, before));
            }
        }
        Some(self.out)
    }

    // ---- items ----

    fn push(
        &mut self,
        label: &str,
        kind: CompletionKind,
        detail: Option<String>,
        documentation: Option<String>,
    ) {
        if self.seen.insert(label.to_owned()) {
            self.out.push(CompletionItem {
                label: label.to_owned(),
                kind,
                detail: detail.filter(|text| !text.is_empty()),
                documentation: documentation.filter(|text| !text.is_empty()),
            });
        }
    }

    fn push_def(&mut self, label: &str, def: Def) {
        match def {
            Def::Item(item) => self.push_item(label, item),
            Def::Variant { enum_item, index } => {
                let node = variant_node(self.db, enum_item, index);
                self.push(
                    label,
                    CompletionKind::Variant,
                    node.as_ref()
                        .map(|node| collapse_whitespace(&node.text().to_string())),
                    node.as_ref().map(doc_comment),
                );
            }
            Def::Module(_) => {
                let detail = format!("mod {}", def_path(self.db, self.maps, def));
                self.push(label, CompletionKind::Module, Some(detail), None);
            }
        }
    }

    fn push_item(&mut self, label: &str, item: ItemId) {
        let Some(data) = item_tree(self.db, item.file).item(item.index) else {
            return;
        };
        let kind = match &data.kind {
            ItemKind::Fn { has_self: true, .. } if matches!(data.parent, ItemParent::Item(_)) => {
                CompletionKind::Method
            }
            ItemKind::Fn { .. } => CompletionKind::Function,
            ItemKind::Struct { .. } => CompletionKind::Struct,
            ItemKind::Enum { .. } => CompletionKind::Enum,
            ItemKind::TypeAlias { .. } => CompletionKind::TypeAlias,
            ItemKind::Const { .. } => CompletionKind::Const,
            ItemKind::Static { .. } => CompletionKind::Static,
            ItemKind::Trait { .. } => CompletionKind::Trait,
            ItemKind::Mod { .. } => CompletionKind::Module,
            ItemKind::Impl { .. } | ItemKind::Use { .. } => return,
        };
        let node = item_node(self.db, item);
        self.push(
            label,
            kind,
            node.as_ref().map(item_header),
            node.as_ref().map(doc_comment),
        );
    }

    fn push_field(&mut self, label: &str, field: FieldId) {
        let node = field_node(self.db, field);
        self.push(
            label,
            CompletionKind::Field,
            node.as_ref()
                .map(|node| collapse_whitespace(&node.text().to_string())),
            node.as_ref().map(doc_comment),
        );
    }

    // ---- `receiver.` ----

    /// The fields and methods of the receiver of the field access or method call of `dot`.
    fn dot(&mut self, dot: &SyntaxToken) -> Option<()> {
        let access = dot.parent()?;
        if !matches!(
            access.kind(),
            SyntaxKind::FIELD_EXPR | SyntaxKind::METHOD_CALL_EXPR
        ) {
            return None;
        }
        let receiver = access.children().next()?.text_range();
        let owner = self.owner?;
        let map = body_source_map_of(self.db, owner)?;
        let result = inference(self.db, self.program, owner)?;
        let mut ty = map
            .exprs
            .iter()
            .filter(|(_, range)| **range == receiver)
            .find_map(|(expr, _)| result.expr_types.get(expr))?
            .clone();
        while let TyKind::Ref { inner, .. } = ty.kind() {
            ty = inner.clone();
        }
        if let TyKind::Adt { item, .. } = ty.kind()
            && let Some(names) = record_field_names(self.db, *item, None)
        {
            for (index, name) in names.iter().enumerate() {
                let Ok(index) = u32::try_from(index) else {
                    continue;
                };
                let field = FieldId {
                    adt: *item,
                    variant: None,
                    index,
                };
                self.push_field(name, field);
            }
        }
        let traits: Vec<ItemId> = match ty.kind() {
            TyKind::Param { owner, index } => program_signatures(self.db, self.program)
                .generics(*owner)
                .and_then(|generics| generics.params.get(usize::try_from(*index).ok()?))
                .map(|param| match &param.kind {
                    GenericParamDefKind::Type { bounds } => bounds.clone(),
                    GenericParamDefKind::Const { .. } => Vec::new(),
                })
                .unwrap_or_default(),
            TyKind::TraitSelf { trait_item } => vec![*trait_item],
            _ => Vec::new(),
        };
        for trait_item in traits {
            self.trait_members(trait_item, true);
        }
        if let Some(key) = crate::signatures::type_key(&ty) {
            self.assoc(key, true);
        }
        Some(())
    }

    // ---- `path::` ----

    /// What the path before `colons` (the first token of a `::`) resolves to.
    fn qualifier(&self, colons: &SyntaxToken) -> Option<Resolution> {
        let last = prev_token(colons)?;
        let end = last.text_range().end();
        let path = last
            .parent_ancestors()
            .take_while(|node| node.text_range().end() == end)
            .filter(|node| node.kind() == SyntaxKind::PATH)
            .last()
            .and_then(ast::Path::cast)?;
        let (path, _) = lower_path(&path, &mut |_| None)?;
        self.resolve_qualifier(&path)
    }

    /// Resolves a qualifier in the type namespace, from the scope at the offset.
    fn resolve_qualifier(&self, path: &Path) -> Option<Resolution> {
        let file = self.maps.module_file(self.module);
        let at = |_| Anchor::Text(file, TextRange::default());
        if path.kind == PathKind::Plain
            && let Some(first) = path.segments.first()
        {
            if first.name == "Self" {
                let base = Resolution::SelfType(self.self_owner()?);
                return resolve_rest(
                    self.maps,
                    self.db,
                    self.module,
                    base,
                    path,
                    1,
                    Namespace::Types,
                    &at,
                )
                .resolved;
            }
            if self
                .generic_params()
                .iter()
                .any(|(name, _)| *name == first.name)
            {
                return None;
            }
        }
        resolve_module_path(self.maps, self.db, self.module, path, Namespace::Types, &at).resolved
    }

    /// The members of a module, enum, type, trait or `Self`.
    fn members(&mut self, resolution: &Resolution) {
        match resolution {
            Resolution::Def(Def::Module(module)) => {
                for ns in [Namespace::Types, Namespace::Values] {
                    let mut entries: Vec<_> = self.maps.module(*module).scope(ns).iter().collect();
                    entries.sort_by(|a, b| a.0.cmp(b.0));
                    for (name, entry) in entries {
                        if self
                            .maps
                            .is_visible(entry.vis, entry.defined_in, self.module)
                        {
                            self.push_def(name, entry.def);
                        }
                    }
                }
            }
            Resolution::Def(Def::Item(item)) => {
                if let Some(ItemKind::Trait { .. }) = item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind)
                {
                    self.trait_members(*item, false);
                } else if let Some(key) = type_key(self.maps, self.db, resolution) {
                    self.type_members(key);
                }
            }
            Resolution::Prim(prim) => self.assoc(TypeKey::Prim(*prim), false),
            Resolution::SelfType(owner) => match self.maps.impl_keys.get(owner) {
                Some(&key) => self.type_members(key),
                None => self.trait_members(*owner, false),
            },
            _ => {}
        }
    }

    /// An enum's variants, then the associated items of a type.
    fn type_members(&mut self, key: TypeKey) {
        if let TypeKey::Item(item) = key
            && let Some(ItemKind::Enum { variants }) = item_tree(self.db, item.file)
                .item(item.index)
                .map(|data| &data.kind)
        {
            for (index, (name, _)) in variants.iter().enumerate() {
                if let Ok(index) = u32::try_from(index) {
                    let def = Def::Variant {
                        enum_item: item,
                        index,
                    };
                    self.push_def(name, def);
                }
            }
        }
        self.assoc(key, false);
    }

    /// The associated items of a type, inherent ones first, then those of the traits it
    /// implements; only methods when `methods`.
    fn assoc(&mut self, key: TypeKey, methods: bool) {
        let mut inherent: Vec<_> = self
            .maps
            .inherent
            .get(&key)
            .into_iter()
            .flatten()
            .map(|(name, item)| (name.clone(), *item))
            .collect();
        inherent.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, item) in inherent {
            let tree = item_tree(self.db, item.file);
            let Some(data) = tree.item(item.index) else {
                continue;
            };
            let visible = self.maps.is_visible(
                data.vis,
                self.maps.module_of_item(self.db, item),
                self.module,
            );
            if visible && (!methods || is_method(&data.kind)) {
                self.push_item(&name, item);
            }
        }
        let impls = self.maps.trait_impls.get(&key).cloned().unwrap_or_default();
        for impl_item in impls {
            match self.maps.impl_traits.get(&impl_item) {
                Some(&trait_item) => self.trait_members(trait_item, methods),
                None => self.container_members(impl_item, methods),
            }
        }
    }

    /// The items of a trait; only methods when `methods`.
    fn trait_members(&mut self, trait_item: ItemId, methods: bool) {
        self.container_members(trait_item, methods);
    }

    fn container_members(&mut self, container: ItemId, methods: bool) {
        let tree = item_tree(self.db, container.file);
        let members = match tree.item(container.index).map(|data| &data.kind) {
            Some(ItemKind::Trait { items } | ItemKind::Impl { items, .. }) => items.clone(),
            _ => return,
        };
        for member in members {
            let Some(data) = tree.item(member) else {
                continue;
            };
            if let Some(name) = &data.name
                && (!methods || is_method(&data.kind))
            {
                let item = ItemId {
                    file: container.file,
                    index: member,
                };
                self.push_item(name, item);
            }
        }
    }

    // ---- struct fields ----

    /// At a field name of a struct expression or pattern: its unwritten fields.
    fn record_fields(
        &mut self,
        word: Option<&SyntaxToken>,
        before: Option<&SyntaxToken>,
    ) -> Option<RecordKind> {
        let (list, current) = match word {
            Some(word) => {
                let field = word.parent_ancestors().nth(1)?;
                let at_name = match field.kind() {
                    SyntaxKind::RECORD_EXPR_FIELD => ast::RecordExprField::cast(field.clone())?
                        .name_ref()
                        .is_some_and(|name| name.syntax().text_range() == word.text_range()),
                    SyntaxKind::RECORD_PAT_FIELD => true,
                    _ => {
                        // A shorthand pattern field: `NAME` in `IDENT_PAT` in the field.
                        let field = word.parent_ancestors().nth(2)?;
                        if field.kind() != SyntaxKind::RECORD_PAT_FIELD {
                            return None;
                        }
                        return self.record_fields_of(field.parent()?, Some(field));
                    }
                };
                if !at_name {
                    return None;
                }
                (field.parent()?, Some(field))
            }
            None => {
                let before = before?;
                if !matches!(before.kind(), SyntaxKind::L_CURLY | SyntaxKind::COMMA) {
                    return None;
                }
                (before.parent()?, None)
            }
        };
        self.record_fields_of(list, current)
    }

    fn record_fields_of(
        &mut self,
        list: SyntaxNode,
        current: Option<SyntaxNode>,
    ) -> Option<RecordKind> {
        let kind = match list.kind() {
            SyntaxKind::RECORD_EXPR_FIELD_LIST => RecordKind::Expr,
            SyntaxKind::RECORD_PAT_FIELD_LIST => RecordKind::Pat,
            _ => return None,
        };
        let record = list.parent()?;
        let owner = self.owner?;
        let map = body_source_map_of(self.db, owner)?;
        let result = inference(self.db, self.program, owner)?;
        let range = record.text_range();
        let ctor = match kind {
            RecordKind::Expr => map
                .exprs
                .iter()
                .filter(|(_, r)| **r == range)
                .find_map(|(expr, _)| result.records.get(&expr)),
            RecordKind::Pat => map
                .pats
                .iter()
                .filter(|(_, r)| **r == range)
                .find_map(|(pat, _)| result.pat_ctors.get(&pat)),
        }?;
        let written: FxHashSet<String> = list
            .children()
            .filter(|field| Some(field) != current.as_ref())
            .filter_map(|field| field_name(&field))
            .collect();
        let names = record_field_names(self.db, ctor.adt, ctor.variant)?;
        for (index, name) in names.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            if !written.contains(name.as_str()) {
                let field = FieldId {
                    adt: ctor.adt,
                    variant: ctor.variant,
                    index,
                };
                self.push_field(name, field);
            }
        }
        Some(kind)
    }

    // ---- names in scope ----

    /// Locals, generic parameters, the items of the enclosing modules, `std`, the prelude and
    /// the primitive types; only type-namespace names when `types_only`.
    fn scope(&mut self, types_only: bool) {
        if !types_only {
            self.locals();
        }
        for (name, kind) in self.generic_params() {
            self.push(&name, kind, None, None);
        }
        let namespaces: &[Namespace] = if types_only {
            &[Namespace::Types]
        } else {
            &[Namespace::Types, Namespace::Values]
        };
        let mut module = self.module;
        loop {
            self.module_entries(module, namespaces);
            let data = self.maps.module(module);
            match (data.is_block(), data.parent) {
                (true, Some(parent)) => module = parent,
                _ => break,
            }
        }
        self.push_def("std", Def::Module(self.maps.std_root()));
        if let Some(prelude) = self.maps.prelude {
            self.module_entries(prelude, namespaces);
        }
        for prim in PRIMITIVES {
            self.push(prim.name(), CompletionKind::Primitive, None, None);
        }
    }

    fn module_entries(&mut self, module: ModuleId, namespaces: &[Namespace]) {
        for &ns in namespaces {
            let mut entries: Vec<_> = self
                .maps
                .module(module)
                .scope(ns)
                .iter()
                .map(|(name, entry)| (name.clone(), entry.def))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, def) in entries {
                self.push_def(&name, def);
            }
        }
    }

    /// The locals in scope at the offset, innermost first.
    fn locals(&mut self) {
        let Some(owner) = self.owner else {
            return;
        };
        let (Some(resolved), Some(map)) = (
            body_resolution(self.db, self.program, owner).body.as_ref(),
            body_source_map_of(self.db, owner),
        ) else {
            return;
        };
        let types = inference(self.db, self.program, owner).map(|result| &result.local_types);
        let mut visible: Vec<(TextSize, crate::resolve::LocalId)> = Vec::new();
        for (local, data) in resolved.locals.iter() {
            match data.pat {
                None => visible.push((TextSize::new(0), local)),
                Some(pat) => {
                    let range = map.pat(pat);
                    if range.end() <= self.offset && in_scope(&self.root, range, self.offset) {
                        visible.push((range.start(), local));
                    }
                }
            }
        }
        visible.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
        for (_, local) in visible {
            let data = &resolved.locals[local];
            let detail = types.and_then(|types| types.get(local)).map(|ty| {
                let mutability = if data.mutable { "mut " } else { "" };
                format!(
                    "let {mutability}{}: {}",
                    data.name,
                    display_ty(self.db, self.program, ty)
                )
            });
            self.push(&data.name, CompletionKind::Local, detail, None);
        }
    }

    /// The generic parameters in scope: the item's own, then its impl's or trait's.
    fn generic_params(&self) -> Vec<(String, CompletionKind)> {
        let Some(owner) = self.owner else {
            return Vec::new();
        };
        let mut owners = vec![owner];
        if let Some(parent) = self.self_owner().filter(|parent| *parent != owner) {
            owners.push(parent);
        }
        owners
            .into_iter()
            .filter_map(|item| signature_of(self.db, item))
            .flat_map(|signature| &signature.generics)
            .map(|param| (param.name.to_string(), CompletionKind::TypeParam))
            .collect()
    }

    /// The impl or trait whose `Self` is in scope.
    fn self_owner(&self) -> Option<ItemId> {
        let owner = self.owner?;
        let tree = item_tree(self.db, owner.file);
        let is_container = |item: ItemId| {
            matches!(
                tree.item(item.index).map(|data| &data.kind),
                Some(ItemKind::Impl { .. } | ItemKind::Trait { .. })
            )
        };
        if is_container(owner) {
            return Some(owner);
        }
        match tree.item(owner.index)?.parent {
            ItemParent::Item(parent) => {
                let parent = ItemId {
                    file: owner.file,
                    index: parent,
                };
                is_container(parent).then_some(parent)
            }
            _ => None,
        }
    }
}

/// The first token of the `::` that ends at `token`: `token` itself when the parser glued it
/// into a `COLON2`, else the `:` right before it (a `::` with nothing after it is left as two
/// `:` tokens).
fn path_separator(token: &SyntaxToken) -> Option<SyntaxToken> {
    match token.kind() {
        SyntaxKind::COLON2 => Some(token.clone()),
        SyntaxKind::COLON => token.prev_token().filter(|prev| {
            prev.kind() == SyntaxKind::COLON
                && prev.text_range().end() == token.text_range().start()
        }),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordKind {
    Expr,
    Pat,
}

const PRIMITIVES: [PrimType; 14] = [
    PrimType::Bool,
    PrimType::U8,
    PrimType::U16,
    PrimType::U32,
    PrimType::U64,
    PrimType::U128,
    PrimType::I8,
    PrimType::I16,
    PrimType::I32,
    PrimType::I64,
    PrimType::I128,
    PrimType::Usize,
    PrimType::Isize,
    PrimType::Int,
];

fn is_method(kind: &ItemKind) -> bool {
    matches!(kind, ItemKind::Fn { has_self: true, .. })
}

/// The field name of a `RECORD_EXPR_FIELD` or `RECORD_PAT_FIELD`.
fn field_name(field: &SyntaxNode) -> Option<String> {
    if let Some(field) = ast::RecordExprField::cast(field.clone()) {
        return field.name_ref().map(|name| name.text());
    }
    let field = ast::RecordPatField::cast(field.clone())?;
    match (field.name_ref(), field.pat()) {
        (Some(name), _) => Some(name.text()),
        (None, Some(ast::Pat::IdentPat(ident))) => {
            ast::HasName::name(&ident).map(|name| name.text())
        }
        _ => None,
    }
}

/// The path a `use … {…}` list is under: the paths of the enclosing use trees, joined.
fn use_list_path(list: &SyntaxNode) -> Option<Path> {
    let mut parts: Vec<Path> = Vec::new();
    for tree in list.ancestors().filter_map(ast::UseTree::cast) {
        if let Some(path) = tree.path() {
            parts.push(lower_path(&path, &mut |_| None)?.0);
        }
    }
    let mut parts = parts.into_iter().rev();
    let mut joined = parts.next()?;
    for part in parts {
        joined.segments.extend(part.segments);
    }
    Some(joined)
}

/// Whether the word at the offset is a name being declared (`let x`, `fn f`), not a reference.
fn declares_name(word: Option<&SyntaxToken>, before: Option<SyntaxKind>) -> bool {
    match word {
        Some(word) => word
            .parent()
            .is_some_and(|parent| parent.kind() == SyntaxKind::NAME),
        None => matches!(
            before,
            Some(
                SyntaxKind::LET_KW
                    | SyntaxKind::MUT_KW
                    | SyntaxKind::FN_KW
                    | SyntaxKind::STRUCT_KW
                    | SyntaxKind::ENUM_KW
                    | SyntaxKind::TRAIT_KW
                    | SyntaxKind::MOD_KW
                    | SyntaxKind::TYPE_KW
                    | SyntaxKind::CONST_KW
                    | SyntaxKind::STATIC_KW
            )
        ),
    }
}

/// Whether only types can be written at the offset: in a type path, or after the `:` of a
/// type annotation or after `->`.
fn types_only(word: Option<&SyntaxToken>, before: Option<&SyntaxToken>) -> bool {
    if let Some(word) = word {
        return word
            .parent_ancestors()
            .find(|node| {
                node.kind() != SyntaxKind::NAME_REF
                    && node.kind() != SyntaxKind::PATH_SEGMENT
                    && node.kind() != SyntaxKind::PATH
            })
            .is_some_and(|node| node.kind() == SyntaxKind::PATH_TYPE);
    }
    match before {
        Some(token) if token.kind() == SyntaxKind::THIN_ARROW => true,
        Some(token) if token.kind() == SyntaxKind::COLON => token.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                SyntaxKind::LET_STMT
                    | SyntaxKind::PARAM
                    | SyntaxKind::SELF_PARAM
                    | SyntaxKind::RECORD_FIELD
                    | SyntaxKind::CONST
                    | SyntaxKind::STATIC
            )
        }),
        _ => false,
    }
}

/// Whether the binding whose pattern is at `pat` is in scope at `offset`, which is after it.
fn in_scope(root: &SyntaxNode, pat: TextRange, offset: TextSize) -> bool {
    let Some(start) = node_at(root, pat, SyntaxKind::IDENT_PAT) else {
        return true;
    };
    for node in start.ancestors() {
        let range = node.text_range();
        match node.kind() {
            // `let` bindings are visible after the statement, to the end of the block.
            SyntaxKind::LET_STMT => {
                return offset >= range.end()
                    && node
                        .parent()
                        .is_some_and(|block| block.text_range().contains_inclusive(offset));
            }
            // `if let` and `while let` bindings are visible in the block they guard.
            SyntaxKind::LET_EXPR => {
                return node
                    .parent()
                    .and_then(|parent| {
                        parent
                            .children()
                            .find(|child| child.kind() == SyntaxKind::BLOCK_EXPR)
                    })
                    .is_some_and(|block| block.text_range().contains_inclusive(offset));
            }
            SyntaxKind::FOR_EXPR => {
                return node
                    .children()
                    .find(|child| child.kind() == SyntaxKind::BLOCK_EXPR)
                    .is_some_and(|block| block.text_range().contains_inclusive(offset));
            }
            SyntaxKind::MATCH_ARM | SyntaxKind::CLOSURE_EXPR | SyntaxKind::FN => {
                return range.contains_inclusive(offset);
            }
            _ => {}
        }
    }
    true
}
