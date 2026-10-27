//! Def maps: the module trees of the game and std crates, their scopes, imports and impls
//! (compilation stage 2, item level).

use kagari_syntax::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::hir::{Name, Path, PathKind, SignatureKind, TypeRef, signature_of};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::{
    BlockIndex, Import, ItemId, ItemIndex, ItemKind, ItemParent, ItemTree, Shape, Visibility,
    item_tree,
};
use crate::module_files::{ModFile, lookup_mod_file};
use crate::resolve::LocalId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CrateId {
    Game,
    Std,
}

crate::item_tree::index_newtype!(
    /// An index into a crate's modules.
    ModuleIndex
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId {
    pub krate: CrateId,
    pub index: ModuleIndex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleOrigin {
    File(SourceFile),
    Inline { file: SourceFile, item: ItemIndex },
    Block { file: SourceFile, block: BlockIndex },
}

impl ModuleOrigin {
    pub fn file(self) -> SourceFile {
        match self {
            ModuleOrigin::File(file)
            | ModuleOrigin::Inline { file, .. }
            | ModuleOrigin::Block { file, .. } => file,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleData {
    pub origin: ModuleOrigin,
    pub parent: Option<ModuleId>,
    pub name: Option<Name>,
    pub types: FxHashMap<Name, ScopeEntry>,
    pub values: FxHashMap<Name, ScopeEntry>,
}

impl ModuleData {
    pub fn scope(&self, ns: Namespace) -> &FxHashMap<Name, ScopeEntry> {
        match ns {
            Namespace::Types => &self.types,
            Namespace::Values => &self.values,
        }
    }

    fn scope_mut(&mut self, ns: Namespace) -> &mut FxHashMap<Name, ScopeEntry> {
        match ns {
            Namespace::Types => &mut self.types,
            Namespace::Values => &mut self.values,
        }
    }

    pub fn is_block(&self) -> bool {
        matches!(self.origin, ModuleOrigin::Block { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeEntry {
    pub def: Def,
    pub vis: Visibility,
    pub defined_in: ModuleId,
    pub source: EntrySource,
    /// Other definitions supplied for the same name by glob imports.
    pub ambiguous: Vec<(Def, Anchor)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySource {
    Definition(ItemId),
    /// A named import: the `use` item and the import's index in it.
    Import(ItemId, u32),
    /// A glob import: the `use` item and the import's index in it.
    Glob(ItemId, u32),
}

impl EntrySource {
    pub fn anchor(self) -> Anchor {
        match self {
            EntrySource::Definition(item) => Anchor::ItemName(item),
            EntrySource::Import(item, index) | EntrySource::Glob(item, index) => {
                Anchor::Import(item, index)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Def {
    Module(ModuleId),
    Item(ItemId),
    Variant { enum_item: ItemId, index: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateDefMap {
    pub modules: Vec<ModuleData>,
    pub root: ModuleId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeKey {
    Item(ItemId),
    Prim(PrimType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrimType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Usize,
    Isize,
    /// `Int<LO, HI>`: an integer in the inclusive range `LO..=HI`, with `i64` bounds.
    Int,
}

impl PrimType {
    pub fn from_name(name: &str) -> Option<PrimType> {
        Some(match name {
            "bool" => PrimType::Bool,
            "u8" => PrimType::U8,
            "u16" => PrimType::U16,
            "u32" => PrimType::U32,
            "u64" => PrimType::U64,
            "u128" => PrimType::U128,
            "i8" => PrimType::I8,
            "i16" => PrimType::I16,
            "i32" => PrimType::I32,
            "i64" => PrimType::I64,
            "i128" => PrimType::I128,
            "usize" => PrimType::Usize,
            "isize" => PrimType::Isize,
            "Int" => PrimType::Int,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            PrimType::Bool => "bool",
            PrimType::U8 => "u8",
            PrimType::U16 => "u16",
            PrimType::U32 => "u32",
            PrimType::U64 => "u64",
            PrimType::U128 => "u128",
            PrimType::I8 => "i8",
            PrimType::I16 => "i16",
            PrimType::I32 => "i32",
            PrimType::I64 => "i64",
            PrimType::I128 => "i128",
            PrimType::Usize => "usize",
            PrimType::Isize => "isize",
            PrimType::Int => "Int",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    Types,
    Values,
}

/// What a path (or a path prefix) refers to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Resolution {
    Local(LocalId),
    Def(Def),
    Prim(PrimType),
    GenericParam {
        owner: ItemId,
        index: u32,
    },
    /// `Self` inside an impl or trait.
    SelfType(ItemId),
    /// An associated item of an impl or trait.
    Assoc(ItemId),
    /// A path continuing past a generic parameter, a trait's `Self` or an alias; resolved by
    /// type checking.
    Deferred {
        base: Box<Resolution>,
        rest: Vec<Name>,
    },
    Error,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathResolution {
    pub resolved: Option<Resolution>,
    pub failed_segment: Option<usize>,
    pub diagnostics: Vec<Diag>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefMaps {
    pub game: CrateDefMap,
    pub std: CrateDefMap,
    /// Every loaded file, in load order.
    pub files: Vec<(SourceFile, CrateId)>,
    pub file_modules: FxHashMap<SourceFile, ModuleId>,
    pub inline_modules: FxHashMap<(SourceFile, ItemIndex), ModuleId>,
    pub block_modules: FxHashMap<(SourceFile, BlockIndex), ModuleId>,
    pub inherent: FxHashMap<TypeKey, FxHashMap<Name, ItemId>>,
    pub trait_impls: FxHashMap<TypeKey, Vec<ItemId>>,
    /// The type each impl is for.
    pub impl_keys: FxHashMap<ItemId, TypeKey>,
    /// The trait each trait impl implements, when it resolves. Items of a
    /// trait impl take the trait's visibility.
    pub impl_traits: FxHashMap<ItemId, ItemId>,
    pub prelude: Option<ModuleId>,
    pub diagnostics: Vec<Diag>,
}

impl DefMaps {
    fn crate_map(&self, krate: CrateId) -> &CrateDefMap {
        match krate {
            CrateId::Game => &self.game,
            CrateId::Std => &self.std,
        }
    }

    fn crate_map_mut(&mut self, krate: CrateId) -> &mut CrateDefMap {
        match krate {
            CrateId::Game => &mut self.game,
            CrateId::Std => &mut self.std,
        }
    }

    /// Module ids are only created by the collector, so indexing never fails.
    pub fn module(&self, id: ModuleId) -> &ModuleData {
        &self.crate_map(id.krate).modules[id.index.idx()]
    }

    fn module_mut(&mut self, id: ModuleId) -> Option<&mut ModuleData> {
        self.crate_map_mut(id.krate).modules.get_mut(id.index.idx())
    }

    pub fn crate_root(&self, krate: CrateId) -> ModuleId {
        self.crate_map(krate).root
    }

    pub fn std_root(&self) -> ModuleId {
        self.std.root
    }

    /// The module an item is defined in (for associated items: the module of their impl/trait).
    pub fn module_of_item(&self, db: &dyn Db, item: ItemId) -> ModuleId {
        let tree = item_tree(db, item.file);
        let mut index = item.index;
        loop {
            let Some(data) = tree.item(index) else {
                return self.file_module(item.file);
            };
            match data.parent {
                ItemParent::File => return self.file_module(item.file),
                ItemParent::Block(block) => {
                    return self
                        .block_modules
                        .get(&(item.file, block))
                        .copied()
                        .unwrap_or_else(|| self.file_module(item.file));
                }
                ItemParent::Item(parent) => {
                    if let Some(module) = self.inline_modules.get(&(item.file, parent)) {
                        return *module;
                    }
                    index = parent;
                }
            }
        }
    }

    fn file_module(&self, file: SourceFile) -> ModuleId {
        self.file_modules
            .get(&file)
            .copied()
            .unwrap_or(self.game.root)
    }

    /// The nearest enclosing module that is not a block.
    pub fn named_module(&self, mut module: ModuleId) -> ModuleId {
        loop {
            let data = self.module(module);
            match (data.is_block(), data.parent) {
                (true, Some(parent)) => module = parent,
                _ => return module,
            }
        }
    }

    /// Whether `module` is `ancestor` or nested in it.
    pub fn is_descendant(&self, mut module: ModuleId, ancestor: ModuleId) -> bool {
        loop {
            if module == ancestor {
                return true;
            }
            match self.module(module).parent {
                Some(parent) => module = parent,
                None => return false,
            }
        }
    }

    pub fn is_visible(&self, entry_vis: Visibility, defined_in: ModuleId, from: ModuleId) -> bool {
        entry_vis == Visibility::Public || self.is_descendant(from, defined_in)
    }

    pub fn module_file(&self, module: ModuleId) -> SourceFile {
        self.module(module).origin.file()
    }

    /// The visibility and defining module of an item or variant, for privacy checks on
    /// associated items and variants.
    fn item_visibility(&self, db: &dyn Db, item: ItemId) -> Visibility {
        item_tree(db, item.file)
            .item(item.index)
            .map_or(Visibility::Public, |data| data.vis)
    }
}

/// The def maps of the program and the standard library.
#[salsa::tracked(returns(ref))]
pub fn def_maps(db: &dyn Db, program: Program) -> DefMaps {
    let mut collector = Collector {
        db,
        maps: DefMaps {
            game: CrateDefMap {
                modules: Vec::new(),
                root: ModuleId {
                    krate: CrateId::Game,
                    index: ModuleIndex(0),
                },
            },
            std: CrateDefMap {
                modules: Vec::new(),
                root: ModuleId {
                    krate: CrateId::Std,
                    index: ModuleIndex(0),
                },
            },
            files: Vec::new(),
            file_modules: FxHashMap::default(),
            inline_modules: FxHashMap::default(),
            block_modules: FxHashMap::default(),
            inherent: FxHashMap::default(),
            trait_impls: FxHashMap::default(),
            impl_keys: FxHashMap::default(),
            impl_traits: FxHashMap::default(),
            prelude: None,
            diagnostics: Vec::new(),
        },
        loaded: FxHashSet::default(),
        imports: Vec::new(),
        globs: Vec::new(),
        impls: Vec::new(),
    };

    let std_root = db.source_file(FilePath::Std(kagari_std::ROOT.into()));
    collector.collect_crate(CrateId::Std, std_root);
    let root = program.root(db);
    let root_path = root.path(db);
    if !crate::input::file_exists(db, *root) {
        collector.maps.diagnostics.push(Diag::new(
            Anchor::Text(*root, TextRange::default()),
            DiagnosticCode::ModuleFileNotFound,
            "cannot read the root file",
        ));
    } else if !matches!(root_path.extension(), Some("kis" | "kix")) {
        collector.maps.diagnostics.push(Diag::new(
            Anchor::Text(*root, TextRange::default()),
            DiagnosticCode::ModuleBadRoot,
            "the root file must be a `.kis` or `.kix` file",
        ));
    }
    collector.collect_crate(CrateId::Game, *root);
    collector.collect_blocks();
    collector.maps.prelude = collector
        .maps
        .module(collector.maps.std.root)
        .types
        .get("prelude")
        .and_then(|entry| match entry.def {
            Def::Module(module) => Some(module),
            Def::Item(_) | Def::Variant { .. } => None,
        });
    collector.resolve_imports();
    collector.collect_impls();
    collector.maps
}

struct PendingImport {
    module: ModuleId,
    /// The `use` item and the import's index in it.
    use_item: ItemId,
    index: u32,
    import: Import,
    vis: Visibility,
    types_done: bool,
    values_done: bool,
    /// Set when the import resolved to an item it may not see.
    private: Option<Diag>,
}

struct Glob {
    importer: ModuleId,
    source: GlobSource,
    vis: Visibility,
    use_item: ItemId,
    index: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GlobSource {
    Module(ModuleId),
    Enum(ItemId),
}

struct Collector<'db> {
    db: &'db dyn Db,
    maps: DefMaps,
    loaded: FxHashSet<SourceFile>,
    imports: Vec<PendingImport>,
    globs: Vec<Glob>,
    impls: Vec<(ItemId, ModuleId)>,
}

impl Collector<'_> {
    fn alloc_module(&mut self, krate: CrateId, data: ModuleData) -> ModuleId {
        let map = self.maps.crate_map_mut(krate);
        let index = ModuleIndex(u32::try_from(map.modules.len()).unwrap_or(u32::MAX));
        map.modules.push(data);
        ModuleId { krate, index }
    }

    fn new_module(
        &mut self,
        krate: CrateId,
        origin: ModuleOrigin,
        parent: Option<ModuleId>,
        name: Option<Name>,
    ) -> ModuleId {
        let id = self.alloc_module(
            krate,
            ModuleData {
                origin,
                parent,
                name,
                types: FxHashMap::default(),
                values: FxHashMap::default(),
            },
        );
        match origin {
            ModuleOrigin::File(file) => {
                self.maps.file_modules.insert(file, id);
            }
            ModuleOrigin::Inline { file, item } => {
                self.maps.inline_modules.insert((file, item), id);
            }
            ModuleOrigin::Block { file, block } => {
                self.maps.block_modules.insert((file, block), id);
            }
        }
        id
    }

    fn collect_crate(&mut self, krate: CrateId, root: SourceFile) {
        let module = self.new_module(krate, ModuleOrigin::File(root), None, None);
        self.maps.crate_map_mut(krate).root = module;
        self.loaded.insert(root);
        self.maps.files.push((root, krate));
        let tree = item_tree(self.db, root);
        let top = tree.top.clone();
        self.collect_items(krate, root, true, &[], module, &top);
    }

    fn collect_items(
        &mut self,
        krate: CrateId,
        file: SourceFile,
        is_crate_root: bool,
        inline_path: &[Name],
        module: ModuleId,
        items: &[ItemIndex],
    ) {
        let tree = item_tree(self.db, file);
        for &index in items {
            self.collect_item(krate, file, tree, is_crate_root, inline_path, module, index);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_item(
        &mut self,
        krate: CrateId,
        file: SourceFile,
        tree: &ItemTree,
        is_crate_root: bool,
        inline_path: &[Name],
        module: ModuleId,
        index: ItemIndex,
    ) {
        let Some(item) = tree.item(index) else {
            return;
        };
        let id = ItemId { file, index };
        let def = Def::Item(id);
        let range = Anchor::ItemName(id);
        match &item.kind {
            ItemKind::Mod { inline } => {
                let Some(name) = item.name.clone() else {
                    return;
                };
                let child = match inline {
                    Some(children) => {
                        let child = self.new_module(
                            krate,
                            ModuleOrigin::Inline { file, item: index },
                            Some(module),
                            Some(name.clone()),
                        );
                        let mut path = inline_path.to_vec();
                        path.push(name.clone());
                        let children = children.clone();
                        self.collect_items(krate, file, is_crate_root, &path, child, &children);
                        child
                    }
                    None => {
                        if matches!(item.parent, ItemParent::Block(_)) {
                            return;
                        }
                        let Some(child_file) =
                            self.load_mod_file(file, is_crate_root, inline_path, &name, range)
                        else {
                            return;
                        };
                        let child = self.new_module(
                            krate,
                            ModuleOrigin::File(child_file),
                            Some(module),
                            Some(name.clone()),
                        );
                        self.maps.files.push((child_file, krate));
                        let child_tree = item_tree(self.db, child_file);
                        let top = child_tree.top.clone();
                        self.collect_items(krate, child_file, false, &[], child, &top);
                        child
                    }
                };
                self.define(
                    module,
                    Namespace::Types,
                    name,
                    Def::Module(child),
                    item.vis,
                    id,
                );
            }
            ItemKind::Struct { shape } => {
                if let Some(name) = item.name.clone() {
                    if !matches!(shape, Shape::Record(_)) {
                        self.define(module, Namespace::Values, name.clone(), def, item.vis, id);
                    }
                    self.define(module, Namespace::Types, name, def, item.vis, id);
                }
            }
            ItemKind::Enum { .. } | ItemKind::Trait { .. } | ItemKind::TypeAlias { .. } => {
                if let Some(name) = item.name.clone() {
                    self.define(module, Namespace::Types, name, def, item.vis, id);
                }
            }
            ItemKind::Fn { .. } | ItemKind::Const { .. } | ItemKind::Static { .. } => {
                if let Some(name) = item.name.clone() {
                    self.define(module, Namespace::Values, name, def, item.vis, id);
                }
            }
            ItemKind::Impl { .. } => self.impls.push((id, module)),
            ItemKind::Use { imports } => {
                for (import_index, import) in imports.iter().enumerate() {
                    self.imports.push(PendingImport {
                        module,
                        use_item: id,
                        index: u32::try_from(import_index).unwrap_or(u32::MAX),
                        import: import.clone(),
                        vis: item.vis,
                        types_done: false,
                        values_done: false,
                        private: None,
                    });
                }
            }
        }
    }

    fn load_mod_file(
        &mut self,
        file: SourceFile,
        is_crate_root: bool,
        inline_path: &[Name],
        name: &Name,
        anchor: Anchor,
    ) -> Option<SourceFile> {
        match lookup_mod_file(self.db, file, is_crate_root, inline_path, name) {
            ModFile::Found(child) => {
                if self.loaded.insert(child) {
                    Some(child)
                } else {
                    self.maps.diagnostics.push(Diag::new(
                        anchor,
                        DiagnosticCode::ModuleFileReused,
                        "file is already loaded as another module",
                    ));
                    None
                }
            }
            ModFile::NotFound(candidates) => {
                let shown: Vec<String> = candidates.iter().map(|c| format!("`{c}`")).collect();
                self.maps.diagnostics.push(Diag::new(
                    anchor,
                    DiagnosticCode::ModuleFileNotFound,
                    format!(
                        "file not found for module `{name}`; expected one of {}",
                        shown.join(", ")
                    ),
                ));
                None
            }
            ModFile::Ambiguous(found) => {
                let shown: Vec<String> = found.iter().map(|c| format!("`{c}`")).collect();
                self.maps.diagnostics.push(Diag::new(
                    anchor,
                    DiagnosticCode::ModuleAmbiguousFile,
                    format!(
                        "module `{name}` has more than one file: {}",
                        shown.join(", ")
                    ),
                ));
                None
            }
        }
    }

    /// Turns every item-holding block of every loaded file into a module.
    fn collect_blocks(&mut self) {
        let files = self.maps.files.clone();
        for (file, krate) in files {
            let tree = item_tree(self.db, file);
            for (index, block) in tree.blocks.iter().enumerate() {
                let Some(index) = BlockIndex::from_usize(index) else {
                    continue;
                };
                let parent = match block.parent_block {
                    Some(parent) => self.maps.block_modules.get(&(file, parent)).copied(),
                    None => Some(self.maps.module_of_item(
                        self.db,
                        ItemId {
                            file,
                            index: block.owner,
                        },
                    )),
                };
                let module = self.new_module(
                    krate,
                    ModuleOrigin::Block { file, block: index },
                    parent,
                    None,
                );
                let items = block.items.clone();
                for item in items {
                    // Block items are never in a crate-root position for `mod` lookup; file
                    // modules in blocks are rejected by the item tree.
                    self.collect_item(krate, file, tree, false, &[], module, item);
                }
            }
        }
    }

    /// Defines `name` in `module`. Definitions and named imports clash; globs never replace them.
    fn define(
        &mut self,
        module: ModuleId,
        ns: Namespace,
        name: Name,
        def: Def,
        vis: Visibility,
        item: ItemId,
    ) {
        self.insert(module, ns, name, def, vis, EntrySource::Definition(item));
    }

    /// Returns whether the scope changed.
    fn insert(
        &mut self,
        module: ModuleId,
        ns: Namespace,
        name: Name,
        def: Def,
        vis: Visibility,
        source: EntrySource,
    ) -> bool {
        let Some(data) = self.maps.module_mut(module) else {
            return false;
        };
        let scope = data.scope_mut(ns);
        let entry = ScopeEntry {
            def,
            vis,
            defined_in: module,
            source,
            ambiguous: Vec::new(),
        };
        match scope.get_mut(&name) {
            None => {
                scope.insert(name, entry);
                true
            }
            Some(existing) => match (existing.source, source) {
                (EntrySource::Glob(..), EntrySource::Glob(..)) => {
                    if existing.def != def && !existing.ambiguous.iter().any(|(d, _)| *d == def) {
                        existing.ambiguous.push((def, source.anchor()));
                        true
                    } else {
                        false
                    }
                }
                (EntrySource::Glob(..), _) => {
                    *existing = entry;
                    true
                }
                (_, EntrySource::Glob(..)) => false,
                (first, _) => {
                    if existing.def == def && matches!(source, EntrySource::Import(..)) {
                        return false;
                    }
                    self.maps.diagnostics.push(
                        Diag::new(
                            source.anchor(),
                            DiagnosticCode::ResolveDuplicate,
                            format!("`{name}` is defined multiple times"),
                        )
                        .with_related(first.anchor(), "first defined here"),
                    );
                    false
                }
            },
        }
    }

    fn resolve_imports(&mut self) {
        loop {
            let mut changed = false;
            for index in 0..self.imports.len() {
                changed |= self.try_import(index);
            }
            for index in 0..self.globs.len() {
                changed |= self.apply_glob(index);
            }
            if !changed {
                break;
            }
        }
        let imports = std::mem::take(&mut self.imports);
        for import in imports {
            if let Some(private) = import.private {
                self.maps.diagnostics.push(private);
            } else if !import.types_done && !import.values_done {
                let (use_item, import_index) = (import.use_item, import.index);
                let failed = resolve_module_path(
                    &self.maps,
                    self.db,
                    import.module,
                    &import.import.path,
                    Namespace::Types,
                    &|segment| Anchor::ImportSegment(use_item, import_index, segment),
                )
                .failed_segment;
                let anchor = failed
                    .filter(|&index| index < import.import.path.segments.len())
                    .and_then(|index| u32::try_from(index).ok())
                    .map_or(Anchor::Import(use_item, import_index), |segment| {
                        Anchor::ImportSegment(use_item, import_index, segment)
                    });
                self.maps.diagnostics.push(Diag::new(
                    anchor,
                    DiagnosticCode::ResolveUnresolvedImport,
                    format!("unresolved import `{}`", import.import.path.display()),
                ));
            }
        }
    }

    /// Tries to resolve pending import `index`; returns whether a scope changed.
    fn try_import(&mut self, index: usize) -> bool {
        let Some(pending) = self.imports.get(index) else {
            return false;
        };
        if pending.private.is_some() || (pending.types_done && pending.values_done) {
            return false;
        }
        let module = pending.module;
        let import = pending.import.clone();
        let vis = pending.vis;
        let (use_item, import_index) = (pending.use_item, pending.index);
        let at = |segment| Anchor::ImportSegment(use_item, import_index, segment);
        if import.glob {
            if pending.types_done {
                return false;
            }
            let result = resolve_module_path(
                &self.maps,
                self.db,
                module,
                &import.path,
                Namespace::Types,
                &at,
            );
            let source = match result.resolved {
                Some(Resolution::Def(Def::Module(source))) => GlobSource::Module(source),
                Some(Resolution::Def(Def::Item(item)))
                    if matches!(
                        item_tree(self.db, item.file)
                            .item(item.index)
                            .map(|i| &i.kind),
                        Some(ItemKind::Enum { .. })
                    ) =>
                {
                    GlobSource::Enum(item)
                }
                _ => return false,
            };
            if let Some(pending) = self.imports.get_mut(index) {
                pending.types_done = true;
                pending.values_done = true;
            }
            self.globs.push(Glob {
                importer: module,
                source,
                vis,
                use_item,
                index: import_index,
            });
            return true;
        }
        let Some(name) = import.name().cloned() else {
            if let Some(pending) = self.imports.get_mut(index) {
                pending.types_done = true;
                pending.values_done = true;
            }
            return false;
        };
        let mut changed = false;
        for ns in [Namespace::Types, Namespace::Values] {
            let done = self.imports.get(index).is_none_or(|pending| match ns {
                Namespace::Types => pending.types_done,
                Namespace::Values => pending.values_done,
            });
            if done {
                continue;
            }
            let result = resolve_module_path(&self.maps, self.db, module, &import.path, ns, &at);
            let Some(Resolution::Def(def)) = result.resolved else {
                continue;
            };
            if let Some(private) = result
                .diagnostics
                .into_iter()
                .find(|diag| diag.code == DiagnosticCode::ResolvePrivate)
            {
                if let Some(pending) = self.imports.get_mut(index) {
                    pending.private = Some(private);
                }
                return changed;
            }
            if let Some(pending) = self.imports.get_mut(index) {
                match ns {
                    Namespace::Types => pending.types_done = true,
                    Namespace::Values => pending.values_done = true,
                }
            }
            self.insert(
                module,
                ns,
                name.clone(),
                def,
                vis,
                EntrySource::Import(use_item, import_index),
            );
            changed = true;
        }
        changed
    }

    fn apply_glob(&mut self, index: usize) -> bool {
        let Some(glob) = self.globs.get(index) else {
            return false;
        };
        let (importer, source, vis) = (glob.importer, glob.source, glob.vis);
        let entry_source = EntrySource::Glob(glob.use_item, glob.index);
        let mut entries: Vec<(Namespace, Name, Def)> = Vec::new();
        match source {
            GlobSource::Module(source) => {
                let data = self.maps.module(source);
                for ns in [Namespace::Types, Namespace::Values] {
                    for (name, entry) in data.scope(ns) {
                        if self.maps.is_visible(entry.vis, entry.defined_in, importer) {
                            entries.push((ns, name.clone(), entry.def));
                        }
                    }
                }
            }
            GlobSource::Enum(item) => {
                if let Some(ItemKind::Enum { variants }) = item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|i| &i.kind)
                {
                    for (index, (name, shape)) in variants.iter().enumerate() {
                        let def = Def::Variant {
                            enum_item: item,
                            index: u32::try_from(index).unwrap_or(u32::MAX),
                        };
                        entries.push((Namespace::Types, name.clone(), def));
                        if !matches!(shape, Shape::Record(_)) {
                            entries.push((Namespace::Values, name.clone(), def));
                        }
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        let mut changed = false;
        for (ns, name, def) in entries {
            changed |= self.insert(importer, ns, name, def, vis, entry_source);
        }
        changed
    }

    /// Whether `item` is the standard library's `Eq` trait.
    fn is_std_eq(&self, item: ItemId) -> bool {
        self.maps.module_of_item(self.db, item) == self.maps.std_root()
            && item_tree(self.db, item.file)
                .item(item.index)
                .and_then(|data| data.name.as_deref())
                == Some("Eq")
    }

    fn collect_impls(&mut self) {
        let impls = std::mem::take(&mut self.impls);
        for (impl_id, module) in impls {
            let Some(signature) = signature_of(self.db, impl_id) else {
                continue;
            };
            let SignatureKind::Impl { trait_ref, self_ty } = &signature.kind else {
                continue;
            };
            let owner = Owner::Signature(impl_id);
            let type_anchor = |ty| Anchor::Node(owner, NodeId::Type(ty));
            // Unresolved traits are reported by name resolution.
            let trait_item = trait_ref.and_then(|trait_ref| {
                let TypeRef::Path(trait_path) = signature.store.type_ref(trait_ref) else {
                    return None;
                };
                let at = |segment| Anchor::Segment(owner, NodeId::Type(trait_ref), segment);
                match resolve_module_path(
                    &self.maps,
                    self.db,
                    module,
                    trait_path,
                    Namespace::Types,
                    &at,
                )
                .resolved
                {
                    Some(Resolution::Def(Def::Item(item))) => Some(item),
                    _ => None,
                }
            });
            if let Some(trait_ref) = trait_ref
                && module.krate == CrateId::Game
            {
                let message = if trait_item.is_some_and(|item| self.is_std_eq(item)) {
                    "only the standard library implements traits; `Eq` is automatic for structs \
                     and enums whose fields are all `Eq`"
                } else {
                    "only the standard library implements traits"
                };
                self.maps.diagnostics.push(Diag::new(
                    type_anchor(*trait_ref),
                    DiagnosticCode::ResolveTraitImplOutsideStd,
                    message,
                ));
            }
            let anchor = type_anchor(*self_ty);
            let key = match signature.store.type_ref(*self_ty) {
                TypeRef::Path(type_path) => {
                    let self_ty = *self_ty;
                    let result = resolve_module_path(
                        &self.maps,
                        self.db,
                        module,
                        type_path,
                        Namespace::Types,
                        &|segment| Anchor::Segment(owner, NodeId::Type(self_ty), segment),
                    );
                    match result.resolved {
                        // Unresolved targets are reported by name resolution.
                        None | Some(Resolution::Error) => continue,
                        Some(resolution) => type_key(&self.maps, self.db, &resolution),
                    }
                }
                _ => None,
            };
            let key = match key {
                Some(TypeKey::Prim(_)) if module.krate != CrateId::Std && trait_ref.is_none() => {
                    self.maps.diagnostics.push(Diag::new(
                        anchor,
                        DiagnosticCode::ResolveBadImplTarget,
                        "inherent impls on primitive types are only allowed in the standard library",
                    ));
                    continue;
                }
                Some(key) => key,
                None => {
                    self.maps.diagnostics.push(Diag::new(
                        anchor,
                        DiagnosticCode::ResolveBadImplTarget,
                        "`impl` target must be a struct, enum or primitive type",
                    ));
                    continue;
                }
            };
            self.maps.impl_keys.insert(impl_id, key);
            if trait_ref.is_some() {
                if let Some(trait_item) = trait_item {
                    self.maps.impl_traits.insert(impl_id, trait_item);
                }
                self.maps.trait_impls.entry(key).or_default().push(impl_id);
                continue;
            }
            let tree = item_tree(self.db, impl_id.file);
            let Some(ItemKind::Impl { items, .. }) = tree.item(impl_id.index).map(|i| &i.kind)
            else {
                continue;
            };
            for &assoc in items {
                let Some(data) = tree.item(assoc) else {
                    continue;
                };
                let Some(name) = data.name.clone() else {
                    continue;
                };
                let assoc_id = ItemId {
                    file: impl_id.file,
                    index: assoc,
                };
                let map = self.maps.inherent.entry(key).or_default();
                if let Some(first) = map.get(&name).copied() {
                    self.maps.diagnostics.push(
                        Diag::new(
                            Anchor::ItemName(assoc_id),
                            DiagnosticCode::ResolveDuplicate,
                            format!("`{name}` is defined multiple times"),
                        )
                        .with_related(Anchor::ItemName(first), "first defined here"),
                    );
                } else {
                    map.insert(name, assoc_id);
                }
            }
        }
    }
}

/// The impl key a resolved type path denotes: structs, enums, primitives and aliases of them.
pub fn type_key(maps: &DefMaps, db: &dyn Db, resolution: &Resolution) -> Option<TypeKey> {
    let mut visited = FxHashSet::default();
    let mut current = resolution.clone();
    loop {
        match current {
            Resolution::Prim(prim) => return Some(TypeKey::Prim(prim)),
            Resolution::Def(Def::Item(item)) => {
                let tree = item_tree(db, item.file);
                match tree.item(item.index).map(|i| &i.kind) {
                    Some(ItemKind::Struct { .. } | ItemKind::Enum { .. }) => {
                        return Some(TypeKey::Item(item));
                    }
                    Some(ItemKind::TypeAlias { .. }) => {
                        if !visited.insert(item) {
                            return None;
                        }
                        let signature = signature_of(db, item)?;
                        let SignatureKind::TypeAlias {
                            value: Some(value), ..
                        } = &signature.kind
                        else {
                            return None;
                        };
                        let TypeRef::Path(path) = signature.store.type_ref(*value) else {
                            return None;
                        };
                        let module = maps.module_of_item(db, item);
                        let (owner, value) = (Owner::Signature(item), *value);
                        current = resolve_module_path(
                            maps,
                            db,
                            module,
                            path,
                            Namespace::Types,
                            &|segment| Anchor::Segment(owner, NodeId::Type(value), segment),
                        )
                        .resolved?;
                    }
                    _ => return None,
                }
            }
            Resolution::SelfType(item) => return maps.impl_keys.get(&item).copied(),
            _ => return None,
        }
    }
}

fn path_prefix(path: &Path, len: usize) -> String {
    Path {
        kind: path.kind,
        segments: path.segments.iter().take(len).cloned().collect(),
    }
    .display()
}

/// Resolves `path` from `module`: the first segment through the lexical chain (block modules,
/// the named module, `std`, the prelude, primitive types), the rest through modules, enums and
/// associated items. The last segment is looked up in `ns`, earlier ones in the type namespace.
pub fn resolve_module_path(
    maps: &DefMaps,
    db: &dyn Db,
    from: ModuleId,
    path: &Path,
    ns: Namespace,
    at: &dyn Fn(u32) -> Anchor,
) -> PathResolution {
    let file = maps.module_file(from);
    let mut result = PathResolution::default();
    let last = path.segments.len().saturating_sub(1);
    let (base, start) = match path.kind {
        PathKind::Crate => (Resolution::Def(Def::Module(maps.crate_root(from.krate))), 0),
        PathKind::SelfModule => (Resolution::Def(Def::Module(maps.named_module(from))), 0),
        PathKind::Super(count) => {
            let mut module = maps.named_module(from);
            for _ in 0..count {
                match maps.module(module).parent {
                    Some(parent) => module = maps.named_module(parent),
                    None => {
                        result.diagnostics.push(Diag::new(
                            at(0),
                            DiagnosticCode::ResolveUnresolvedName,
                            "there are too many leading `super` keywords",
                        ));
                        result.failed_segment = Some(0);
                        return result;
                    }
                }
            }
            (Resolution::Def(Def::Module(module)), 0)
        }
        PathKind::Lang(item) => {
            let mut lang = Path {
                kind: PathKind::Crate,
                segments: item
                    .std_path()
                    .iter()
                    .map(|name| crate::hir::PathSegment {
                        name: Name::new(name),
                        args: None,
                    })
                    .collect(),
            };
            lang.segments.extend(path.segments.iter().cloned());
            let std = maps.std_root();
            let prefix = u32::try_from(item.std_path().len()).unwrap_or(u32::MAX);
            let lang_at = |segment: u32| match segment.checked_sub(prefix) {
                Some(segment) => at(segment),
                None => Anchor::Text(file, TextRange::default()),
            };
            let inner = resolve_rest(
                maps,
                db,
                from,
                Resolution::Def(Def::Module(std)),
                &lang,
                0,
                ns,
                &lang_at,
            );
            if inner.resolved.is_none() {
                result.diagnostics.push(Diag::new(
                    Anchor::Text(file, TextRange::default()),
                    DiagnosticCode::ResolveMissingStdItem,
                    format!(
                        "standard library item `std::{}` is missing",
                        item.std_path().join("::")
                    ),
                ));
                return result;
            }
            return inner;
        }
        PathKind::Plain => {
            let Some(first) = path.segments.first() else {
                return result;
            };
            let first_ns = if last == 0 { ns } else { Namespace::Types };
            match resolve_first_segment(maps, db, from, &first.name, first_ns) {
                Some((resolution, diag)) => {
                    result
                        .diagnostics
                        .extend(diag.map(|note| note.into_diagnostic(at(0))));
                    (resolution, 1)
                }
                None => {
                    let what = match first_ns {
                        Namespace::Types => "type",
                        Namespace::Values => "value",
                    };
                    result.diagnostics.push(Diag::new(
                        at(0),
                        DiagnosticCode::ResolveUnresolvedName,
                        format!("cannot find {what} `{}` in this scope", first.name),
                    ));
                    result.failed_segment = Some(0);
                    return result;
                }
            }
        }
    };
    let rest = resolve_rest(maps, db, from, base, path, start, ns, at);
    result.resolved = rest.resolved;
    result.failed_segment = rest.failed_segment;
    result.diagnostics.extend(rest.diagnostics);
    result
}

/// The resolution of a scope entry, plus an ambiguity note when globs disagree on it.
fn entry_lookup(entry: &ScopeEntry, name: &Name) -> (Resolution, Option<AmbiguityNote>) {
    let note = (!entry.ambiguous.is_empty()).then(|| {
        let mut candidates = vec![entry.source.anchor()];
        candidates.extend(entry.ambiguous.iter().map(|(_, anchor)| *anchor));
        AmbiguityNote {
            name: name.clone(),
            candidates,
        }
    });
    (Resolution::Def(entry.def), note)
}

/// The first segment of a plain path, looked up lexically from `from`.
pub(crate) fn resolve_first_segment(
    maps: &DefMaps,
    _db: &dyn Db,
    from: ModuleId,
    name: &Name,
    ns: Namespace,
) -> Option<(Resolution, Option<AmbiguityNote>)> {
    let mut module = from;
    loop {
        let data = maps.module(module);
        if let Some(entry) = data.scope(ns).get(name) {
            return Some(entry_lookup(entry, name));
        }
        match (data.is_block(), data.parent) {
            (true, Some(parent)) => module = parent,
            _ => break,
        }
    }
    if ns == Namespace::Types && name == "std" {
        return Some((Resolution::Def(Def::Module(maps.std_root())), None));
    }
    if let Some(prelude) = maps.prelude
        && let Some(entry) = maps.module(prelude).scope(ns).get(name)
    {
        return Some((Resolution::Def(entry.def), None));
    }
    if ns == Namespace::Types
        && let Some(prim) = PrimType::from_name(name)
    {
        return Some((Resolution::Prim(prim), None));
    }
    None
}

/// A name supplied by several glob imports; reported where the name is used.
pub(crate) struct AmbiguityNote {
    name: Name,
    candidates: Vec<Anchor>,
}

impl AmbiguityNote {
    pub(crate) fn into_diagnostic(self, anchor: Anchor) -> Diag {
        let mut diag = Diag::new(
            anchor,
            DiagnosticCode::ResolveAmbiguous,
            format!("`{}` is ambiguous", self.name),
        );
        for candidate in self.candidates {
            diag = diag.with_related(
                candidate,
                format!("`{}` could refer to the item imported here", self.name),
            );
        }
        diag
    }
}

/// Resolves `path.segments[start..]` starting from `base`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_rest(
    maps: &DefMaps,
    db: &dyn Db,
    from: ModuleId,
    base: Resolution,
    path: &Path,
    start: usize,
    ns: Namespace,
    at: &dyn Fn(u32) -> Anchor,
) -> PathResolution {
    let mut result = PathResolution::default();
    let last = path.segments.len().saturating_sub(1);
    let mut current = base;
    for (index, segment) in path.segments.iter().enumerate().skip(start) {
        let seg_ns = if index == last { ns } else { Namespace::Types };
        let name = &segment.name;
        let anchor = at(u32::try_from(index).unwrap_or(u32::MAX));
        let prefix = || path_prefix(path, index);
        let next = match &current {
            Resolution::Def(Def::Module(module)) => {
                match maps.module(*module).scope(seg_ns).get(name) {
                    Some(entry) => {
                        if !maps.is_visible(entry.vis, entry.defined_in, from) {
                            result.diagnostics.push(Diag::new(
                                anchor,
                                DiagnosticCode::ResolvePrivate,
                                format!("`{name}` is private"),
                            ));
                        }
                        let (resolution, note) = entry_lookup(entry, name);
                        if let Some(note) = note {
                            result.diagnostics.push(note.into_diagnostic(anchor));
                        }
                        Some(resolution)
                    }
                    None => {
                        result.diagnostics.push(Diag::new(
                            anchor,
                            DiagnosticCode::ResolveUnresolvedName,
                            format!("cannot find `{name}` in `{}`", prefix()),
                        ));
                        None
                    }
                }
            }
            Resolution::Def(Def::Item(item)) => {
                let tree = item_tree(db, item.file);
                match tree.item(item.index).map(|i| &i.kind) {
                    Some(ItemKind::Enum { variants }) => {
                        let variant = variants
                            .iter()
                            .position(|(variant, _)| variant == name)
                            .map(|position| {
                                Resolution::Def(Def::Variant {
                                    enum_item: *item,
                                    index: u32::try_from(position).unwrap_or(u32::MAX),
                                })
                            });
                        variant.or_else(|| {
                            assoc_lookup(
                                maps,
                                db,
                                from,
                                Some(TypeKey::Item(*item)),
                                name,
                                anchor,
                                &mut result,
                                &prefix(),
                            )
                        })
                    }
                    Some(ItemKind::Struct { .. }) => assoc_lookup(
                        maps,
                        db,
                        from,
                        Some(TypeKey::Item(*item)),
                        name,
                        anchor,
                        &mut result,
                        &prefix(),
                    ),
                    Some(ItemKind::TypeAlias { .. }) => match type_key(maps, db, &current) {
                        Some(key) => assoc_lookup(
                            maps,
                            db,
                            from,
                            Some(key),
                            name,
                            anchor,
                            &mut result,
                            &prefix(),
                        ),
                        None => Some(deferred(&current, path, index)),
                    },
                    Some(ItemKind::Trait { items }) => {
                        let found = items.iter().copied().find(|&assoc| {
                            tree.item(assoc).and_then(|i| i.name.as_ref()) == Some(name)
                        });
                        match found {
                            Some(assoc) => Some(Resolution::Assoc(ItemId {
                                file: item.file,
                                index: assoc,
                            })),
                            None => {
                                result.diagnostics.push(Diag::new(
                                    anchor,
                                    DiagnosticCode::ResolveUnresolvedName,
                                    format!("no associated item named `{name}` in `{}`", prefix()),
                                ));
                                None
                            }
                        }
                    }
                    _ => {
                        result.diagnostics.push(Diag::new(
                            anchor,
                            DiagnosticCode::ResolveUnresolvedName,
                            format!("cannot find `{name}` in `{}`", prefix()),
                        ));
                        None
                    }
                }
            }
            Resolution::Prim(prim) => assoc_lookup(
                maps,
                db,
                from,
                Some(TypeKey::Prim(*prim)),
                name,
                anchor,
                &mut result,
                &prefix(),
            ),
            Resolution::SelfType(owner) => {
                let is_trait = matches!(
                    item_tree(db, owner.file).item(owner.index).map(|i| &i.kind),
                    Some(ItemKind::Trait { .. })
                );
                match maps.impl_keys.get(owner) {
                    Some(key) if !is_trait => assoc_lookup(
                        maps,
                        db,
                        from,
                        Some(*key),
                        name,
                        anchor,
                        &mut result,
                        &prefix(),
                    ),
                    _ => Some(deferred(&current, path, index)),
                }
            }
            Resolution::GenericParam { .. } | Resolution::Deferred { .. } => {
                Some(deferred(&current, path, index))
            }
            Resolution::Def(Def::Variant { .. })
            | Resolution::Assoc(_)
            | Resolution::Local(_)
            | Resolution::Error => {
                result.diagnostics.push(Diag::new(
                    anchor,
                    DiagnosticCode::ResolveUnresolvedName,
                    format!("cannot find `{name}` in `{}`", prefix()),
                ));
                None
            }
        };
        match next {
            Some(Resolution::Deferred { base, rest }) => {
                result.resolved = Some(Resolution::Deferred { base, rest });
                return result;
            }
            Some(next) => current = next,
            None => {
                result.failed_segment = Some(index);
                return result;
            }
        }
    }
    result.resolved = Some(current);
    result
}

/// `base` followed by the unresolved segments from `index` on.
fn deferred(base: &Resolution, path: &Path, index: usize) -> Resolution {
    let (base, mut rest) = match base {
        Resolution::Deferred { base, rest } => (base.clone(), rest.clone()),
        other => (Box::new(other.clone()), Vec::new()),
    };
    rest.extend(
        path.segments
            .iter()
            .skip(index)
            .map(|segment| segment.name.clone()),
    );
    Resolution::Deferred { base, rest }
}

#[allow(clippy::too_many_arguments)]
fn assoc_lookup(
    maps: &DefMaps,
    db: &dyn Db,
    from: ModuleId,
    key: Option<TypeKey>,
    name: &Name,
    anchor: Anchor,
    result: &mut PathResolution,
    prefix: &str,
) -> Option<Resolution> {
    // Inherent items carry their own visibility; items of a trait impl are
    // visible wherever the trait is. An impl whose trait failed to resolve
    // has already been reported, so its items are not checked again.
    let found = key.and_then(|key| {
        maps.inherent
            .get(&key)
            .and_then(|items| items.get(name).copied())
            .map(|assoc| (assoc, Some(assoc)))
            .or_else(|| {
                maps.trait_impls.get(&key).and_then(|impls| {
                    impls.iter().find_map(|impl_id| {
                        let tree = item_tree(db, impl_id.file);
                        let Some(ItemKind::Impl { items, .. }) =
                            tree.item(impl_id.index).map(|i| &i.kind)
                        else {
                            return None;
                        };
                        let index = items.iter().copied().find(|&assoc| {
                            tree.item(assoc).and_then(|i| i.name.as_ref()) == Some(name)
                        })?;
                        let assoc = ItemId {
                            file: impl_id.file,
                            index,
                        };
                        Some((assoc, maps.impl_traits.get(impl_id).copied()))
                    })
                })
            })
    });
    match found {
        Some((assoc, vis_item)) => {
            if let Some(vis_item) = vis_item
                && !maps.is_visible(
                    maps.item_visibility(db, vis_item),
                    maps.module_of_item(db, vis_item),
                    from,
                )
            {
                result.diagnostics.push(Diag::new(
                    anchor,
                    DiagnosticCode::ResolvePrivate,
                    format!("`{name}` is private"),
                ));
            }
            Some(Resolution::Assoc(assoc))
        }
        None => {
            result.diagnostics.push(Diag::new(
                anchor,
                DiagnosticCode::ResolveUnresolvedName,
                format!("no associated item named `{name}` in `{prefix}`"),
            ));
            None
        }
    }
}
