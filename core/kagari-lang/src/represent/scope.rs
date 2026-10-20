//! Stage 3: the item scope of all game-scope kix files.

use kagari_std::PRELUDE;
use kagari_syntax::TextRange;
use kagari_syntax::represent::ast;
use rowan::ast::AstNode;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::collect::collect;
use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::{FilePath, Program, SourceFile};
use crate::parse::parse_represent;
use crate::represent::item_tree::{DefId, Item, ItemKind, item_tree};

/// Names of the primitive types; they cannot be redefined in the type namespace.
pub const PRIMITIVES: [&str; 11] = [
    "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "rational", "str",
];

/// The two namespaces of one scope (the root or a `mod`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ModScope {
    /// Structs, enums, type aliases and mods.
    pub types: FxHashMap<SmolStr, DefId>,
    /// Consts, fns, asset imports, unit structs, and (in the root) the prelude's `Option` variants.
    pub values: FxHashMap<SmolStr, ValueDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueDef {
    Item(DefId),
    Variant { enum_def: DefId, index: u32 },
}

/// The prelude definitions the checker refers to by role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreludeDefs {
    pub screen: DefId,
    pub option: DefId,
    pub image_asset: DefId,
    pub audio_asset: DefId,
    pub voice_asset: DefId,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ItemScope {
    pub root: ModScope,
    pub mods: FxHashMap<DefId, ModScope>,
    /// Methods and associated consts per struct/enum, merged across impl blocks and files.
    pub methods: FxHashMap<DefId, FxHashMap<SmolStr, DefId>>,
    /// The struct/enum each well-formed impl block belongs to.
    pub impl_targets: FxHashMap<DefId, DefId>,
    pub prelude: Option<PreludeDefs>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ItemScope {
    /// The scope of a module, or the root for `None`.
    pub fn module(&self, def: Option<DefId>) -> Option<&ModScope> {
        match def {
            None => Some(&self.root),
            Some(def) => self.mods.get(&def),
        }
    }
}

/// The item behind a definition.
pub fn item(db: &dyn Db, def: DefId) -> Option<&Item> {
    item_tree(db, def.file).items.get(def.index as usize)
}

#[salsa::tracked(returns(ref))]
pub fn item_scope(db: &dyn Db, program: Program) -> ItemScope {
    let collected = collect(db, program);
    let mut builder = Builder {
        db,
        scope: ItemScope::default(),
    };
    let prelude_path = FilePath::Std(PRELUDE.into());
    let mut prelude_file = None;
    for &file in &collected.kix_files {
        let tree = item_tree(db, file);
        for &index in &tree.top {
            builder.define(None, file, index);
        }
        if file.path(db) == &prelude_path && prelude_file.is_none() {
            prelude_file = Some(file);
            builder.prelude(file);
        }
    }
    for &file in &collected.kix_files {
        let tree = item_tree(db, file);
        for (index, item) in (0u32..).zip(&tree.items) {
            if let ItemKind::Impl { generics, children } = &item.kind {
                builder.implement(DefId { file, index }, item, generics, children);
            }
        }
    }
    builder.scope
}

struct Builder<'db> {
    db: &'db dyn Db,
    scope: ItemScope,
}

enum Ns {
    Types,
    Values,
}

impl Builder<'_> {
    fn define(&mut self, owner: Option<DefId>, file: SourceFile, index: u32) {
        let db = self.db;
        let def = DefId { file, index };
        let Some(item) = item(db, def) else {
            return;
        };
        if item.name.is_empty() && !matches!(item.kind, ItemKind::Impl { .. }) {
            return;
        }
        match &item.kind {
            ItemKind::Struct { fields, .. } => {
                self.insert(owner, Ns::Types, item, ValueDef::Item(def));
                if fields.is_none() {
                    self.insert(owner, Ns::Values, item, ValueDef::Item(def));
                }
            }
            ItemKind::Enum { .. } | ItemKind::TypeAlias { .. } => {
                self.insert(owner, Ns::Types, item, ValueDef::Item(def));
            }
            ItemKind::Const | ItemKind::Fn { .. } | ItemKind::AssetImport { .. } => {
                self.insert(owner, Ns::Values, item, ValueDef::Item(def));
            }
            ItemKind::Mod { children } => {
                self.insert(owner, Ns::Types, item, ValueDef::Item(def));
                self.scope.mods.entry(def).or_default();
                for &child in children {
                    self.define(Some(def), file, child);
                }
            }
            ItemKind::Impl { .. } => {}
        }
    }

    fn insert(&mut self, owner: Option<DefId>, ns: Ns, item: &Item, value: ValueDef) {
        let ValueDef::Item(def) = value else {
            return;
        };
        if matches!(ns, Ns::Types) && PRIMITIVES.contains(&item.name.as_str()) {
            let diag = Diagnostic::new(
                def.file.path(self.db).clone(),
                item.name_range,
                DiagnosticCode::KixReservedName,
                format!("`{}` is a primitive type name", item.name),
            );
            self.scope.diagnostics.push(diag);
            return;
        }
        let scope = match owner {
            None => &mut self.scope.root,
            Some(owner) => self.scope.mods.entry(owner).or_default(),
        };
        let first = match ns {
            Ns::Types => match scope.types.get(&item.name) {
                Some(first) => Some(ValueDef::Item(*first)),
                None => {
                    scope.types.insert(item.name.clone(), def);
                    None
                }
            },
            Ns::Values => match scope.values.get(&item.name) {
                Some(first) => Some(*first),
                None => {
                    scope.values.insert(item.name.clone(), value);
                    None
                }
            },
        };
        if let Some(first) = first {
            let diag = self.duplicate(def.file, item.name_range, &item.name, first);
            self.scope.diagnostics.push(diag);
        }
    }

    fn duplicate(
        &self,
        file: SourceFile,
        range: TextRange,
        name: &str,
        first: ValueDef,
    ) -> Diagnostic {
        let db = self.db;
        let diag = Diagnostic::new(
            file.path(db).clone(),
            range,
            DiagnosticCode::KixDuplicateDefinition,
            format!("`{name}` is defined multiple times"),
        );
        match value_location(db, first) {
            Some((first_file, first_range)) => diag.with_related(
                first_file.path(db).clone(),
                first_range,
                "first defined here",
            ),
            None => diag,
        }
    }

    /// Record the prelude's role definitions and bring `Option`'s variants into scope bare.
    fn prelude(&mut self, file: SourceFile) {
        let find = |name: &str| {
            self.scope
                .root
                .types
                .get(name)
                .copied()
                .filter(|def| def.file == file)
        };
        let defs = (|| {
            Some(PreludeDefs {
                screen: find("Screen")?,
                option: find("Option")?,
                image_asset: find("ImageAsset")?,
                audio_asset: find("AudioAsset")?,
                voice_asset: find("VoiceAsset")?,
            })
        })();
        self.scope.prelude = defs;
        let Some(defs) = defs else {
            return;
        };
        let Some(Item {
            kind: ItemKind::Enum { variants, .. },
            ..
        }) = item(self.db, defs.option)
        else {
            return;
        };
        for ((name, _, _), index) in variants.iter().zip(0u32..) {
            self.scope
                .root
                .values
                .entry(name.clone())
                .or_insert(ValueDef::Variant {
                    enum_def: defs.option,
                    index,
                });
        }
    }

    fn implement(&mut self, def: DefId, item: &Item, generics: &[SmolStr], children: &[u32]) {
        let db = self.db;
        let Some(target) = self.impl_target(def, generics) else {
            let diag = Diagnostic::new(
                def.file.path(db).clone(),
                item.name_range,
                DiagnosticCode::KixBadImplTarget,
                "an impl must name a struct or enum, with exactly the impl's generic parameters as its arguments",
            );
            self.scope.diagnostics.push(diag);
            return;
        };
        self.scope.impl_targets.insert(def, target);
        for &child in children {
            let child_def = DefId {
                file: def.file,
                index: child,
            };
            let Some(child_item) = crate::represent::scope::item(db, child_def) else {
                continue;
            };
            if !matches!(child_item.kind, ItemKind::Fn { .. } | ItemKind::Const)
                || child_item.name.is_empty()
            {
                continue;
            }
            let methods = self.scope.methods.entry(target).or_default();
            match methods.get(&child_item.name) {
                Some(&first) => {
                    let diag = self.duplicate(
                        def.file,
                        child_item.name_range,
                        &child_item.name,
                        ValueDef::Item(first),
                    );
                    self.scope.diagnostics.push(diag);
                }
                None => {
                    methods.insert(child_item.name.clone(), child_def);
                }
            }
        }
    }

    /// The struct/enum named by an impl's self type, if it is well formed.
    fn impl_target(&self, def: DefId, generics: &[SmolStr]) -> Option<DefId> {
        let db = self.db;
        let item = item(db, def)?;
        let root = parse_represent(db, def.file).syntax_node();
        let imp = ast::Impl::cast(item.ptr.try_to_node(&root)?)?;
        let ast::Type::PathType(path_ty) = imp.self_ty()? else {
            return None;
        };
        let path = path_ty.path()?;
        let segments: Vec<ast::PathSegment> = path.segments().collect();
        let (last, init) = segments.split_last()?;
        let mut scope = &self.scope.root;
        for segment in init {
            if segment.generic_arg_list().is_some() {
                return None;
            }
            let ast::PathSegmentKind::Name(name) = segment.kind()? else {
                return None;
            };
            let module = *scope.types.get(name.text().as_str())?;
            scope = self.scope.mods.get(&module)?;
        }
        let ast::PathSegmentKind::Name(name) = last.kind()? else {
            return None;
        };
        let target = *scope.types.get(name.text().as_str())?;
        let target_generics = match &crate::represent::scope::item(db, target)?.kind {
            ItemKind::Struct { generics, .. } | ItemKind::Enum { generics, .. } => generics,
            _ => return None,
        };
        let args: Vec<ast::Type> = last
            .generic_arg_list()
            .map(|list| list.types().collect())
            .unwrap_or_default();
        if args.len() != generics.len() || target_generics.len() != generics.len() {
            return None;
        }
        for (arg, param) in args.iter().zip(generics) {
            let ast::Type::PathType(arg) = arg else {
                return None;
            };
            let segment = arg.path()?.single_segment()?;
            if segment.generic_arg_list().is_some() {
                return None;
            }
            match segment.kind()? {
                ast::PathSegmentKind::Name(name) if name.text() == param.as_str() => {}
                _ => return None,
            }
        }
        Some(target)
    }
}

/// Where a value definition is written.
pub fn value_location(db: &dyn Db, value: ValueDef) -> Option<(SourceFile, TextRange)> {
    match value {
        ValueDef::Item(def) => item(db, def).map(|item| (def.file, item.name_range)),
        ValueDef::Variant { enum_def, index } => match &item(db, enum_def)?.kind {
            ItemKind::Enum { variants, .. } => variants
                .get(index as usize)
                .map(|(_, range, _)| (enum_def.file, *range)),
            _ => None,
        },
    }
}
