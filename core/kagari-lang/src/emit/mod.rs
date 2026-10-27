//! Bytecode emission (compilation stage 6): the checked program lowered to a
//! [`kagari_bytecode::Module`]. See `docs/syntax/src/bytecode.md`.
//!
//! Code comes from the MIR of each function (`codegen`); constants, statics and promoted
//! constants come from const evaluation. Only the functions reachable from `main`, the
//! entry scenes and the statics are emitted. Generic functions are monomorphized per
//! instantiation; every table is interned on first use, so the output is deterministic.

mod codegen;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use kagari_bytecode::{
    AdtType, Asset, AssetId, AssetKind as BcAssetKind, ConstId, Constant, Debug, FnDebug, FnFlags,
    FnKind as BcFnKind, FuncId, Function, Import, ImportId, IntType, IntTypeId, Module, Options,
    Static, StaticId, StaticKind, StrId, TypeId, Variant, encode, verify, verify_code,
    word_offsets,
};
use kagari_syntax::{TextRange, TextSize};
use rustc_hash::FxHashMap;

use crate::analysis::system::code::main_fn;
use crate::db::Db;
use crate::def_map::{CrateId, Def, DefMaps, def_maps};
use crate::diagnostic::{Anchor, Diagnostic, DiagnosticCode, Severity, resolve_anchor};
use crate::eval::machine::value::GenericEnv;
use crate::eval::queries::static_value;
use crate::eval::std_items::{StdTable, std_table};
use crate::eval::{AssetKind, ConstValue, HandleKind, Text};
use crate::hir::{Expr, ExprId};
use crate::input::{FilePath, Program};
use crate::item_tree::ItemIndex;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::mir::{BodyKind, item_mir};
use crate::paths;
use crate::pipeline::check;
use crate::signatures::{ProgramSignatures, display_ty, program_signatures};
use crate::thir::PromotedId;
use crate::ty::{Const, FnDefId, FnRef, GenericArg, GenericArgs, IntTy, Ty, TyKind};

/// The result of [`build`].
pub struct Build {
    /// The diagnostics of [`check`], then those of emission.
    pub diagnostics: Vec<Diagnostic>,
    /// The verified module; `None` when there is an error.
    pub module: Option<kagari_bytecode::Module>,
    /// `module` encoded.
    pub bytes: Option<Vec<u8>>,
}

/// Checks the program and, without errors, emits, verifies and encodes its bytecode.
pub fn build(db: &dyn Db, program: Program) -> Build {
    let mut diagnostics = check(db, program).clone();
    let failed = |diagnostics| Build {
        diagnostics,
        module: None,
        bytes: None,
    };
    if diagnostics
        .iter()
        .any(|diag| diag.severity == Severity::Error)
    {
        return failed(diagnostics);
    }
    #[cfg(debug_assertions)]
    validate_mir(db, program);
    let (module, has_main, mut errors) = emit(db, program);
    diagnostics.append(&mut errors);
    let Some(module) = module else {
        return failed(diagnostics);
    };
    // Without `main` the entry scenes and statics are still lowered and verified, so their
    // faults are found.
    let verified = if has_main {
        verify(&module)
    } else {
        verify_code(&module)
    };
    if let Err(error) = verified {
        diagnostics.push(root_diagnostic(
            db,
            program,
            DiagnosticCode::EmitInvalidBytecode,
            format!("internal compiler error: the emitted bytecode fails verification: {error}"),
        ));
        return failed(diagnostics);
    }
    if !has_main {
        diagnostics.push(root_diagnostic(
            db,
            program,
            DiagnosticCode::EmitMissingMain,
            "the root module has no `fn main()`, so the engine has nothing to run at boot"
                .to_owned(),
        ));
        return failed(diagnostics);
    }
    let bytes = encode(&module);
    Build {
        diagnostics,
        module: Some(module),
        bytes: Some(bytes),
    }
}

/// Validates the MIR of every item with a body; a violation is a compiler bug.
#[cfg(debug_assertions)]
#[allow(clippy::panic)]
fn validate_mir(db: &dyn Db, program: Program) {
    let maps = def_maps(db, program);
    for (file, _) in maps.files.iter() {
        let tree = item_tree(db, *file);
        for position in 0..tree.items.len() {
            let Some(index) = ItemIndex::from_usize(position) else {
                continue;
            };
            let item = ItemId { file: *file, index };
            let Some(mir) = crate::mir::item_mir(db, program, item).as_ref() else {
                continue;
            };
            for body in mir.bodies() {
                let errors = crate::mir::validate::validate(db, program, body);
                if !errors.is_empty() {
                    panic!(
                        "invalid MIR for {} ({:?}):\n{}",
                        paths::assoc_path(db, maps, item),
                        body.source,
                        errors.join("\n")
                    );
                }
            }
        }
    }
}

fn root_diagnostic(
    db: &dyn Db,
    program: Program,
    code: DiagnosticCode,
    message: String,
) -> Diagnostic {
    Diagnostic::new(
        program.root(db).path(db).clone(),
        TextRange::empty(TextSize::from(0)),
        code,
        message,
    )
}

/// Emits the module of a checked program: the module (`None` after an emission error),
/// whether it has a `main`, and the emission errors.
fn emit(db: &dyn Db, program: Program) -> (Option<Module>, bool, Vec<Diagnostic>) {
    // Std failed to load: `check` already reported it.
    let Some(std) = std_table(db, program).as_ref() else {
        return (None, false, Vec::new());
    };
    let sigs = program_signatures(db, program);
    let main = main_fn(db, program)
        .filter(|main| sigs.fn_sig(*main).is_some_and(|sig| sig.params.is_empty()));
    let mut emitter = Emitter::new(db, program, std, sigs, main);
    emitter.statics();
    emitter.roots();
    emitter.run();
    match emitter.finish() {
        Ok(module) => (Some(module), main.is_some(), Vec::new()),
        Err(errors) => (None, main.is_some(), errors),
    }
}

/// A function of the module.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Instance {
    /// An item with a body, fully substituted.
    Fn(FnRef),
    /// A closure: the instance of the item whose body contains it, and the closure.
    Closure { owner: Box<Instance>, expr: ExprId },
    /// A tuple struct or tuple variant constructor used as a value (without generic
    /// arguments: the constructed value is the same for every instantiation).
    Ctor(FnRef),
    /// A builtin used as a value: a wrapper that lowers like a direct call.
    Builtin(FnRef),
    /// The initializer of a `const` or plain `static` whose value holds a `Screen`, which
    /// cannot be a constant.
    ConstInit(ItemId, Vec<GenericArg>),
    /// A promoted constant of a body whose value holds a `Screen` (or faults): the instance
    /// owning the body, and the constant.
    Promoted(Box<Instance>, PromotedId),
}

/// A function's source file (index into the Debug files) and its
/// `(instruction index, byte offset)` lines.
type FnLines = (Option<u32>, Vec<(u32, u32)>);

struct Emitter<'db> {
    db: &'db dyn Db,
    program: Program,
    std: &'db StdTable,
    sigs: &'db ProgramSignatures,
    maps: &'db DefMaps,
    /// `fn main` of the root module, when it has one.
    main: Option<ItemId>,
    root_dir: Option<PathBuf>,
    /// The program's asset directory; bytecode asset paths are relative to it.
    asset_root: Option<PathBuf>,
    module: Module,
    strings: FxHashMap<Arc<str>, StrId>,
    types: FxHashMap<ItemId, TypeId>,
    int_types: FxHashMap<IntType, IntTypeId>,
    imports: FxHashMap<ItemId, ImportId>,
    assets: FxHashMap<(AssetKind, FilePath), AssetId>,
    constants: FxHashMap<Constant, ConstId>,
    statics: FxHashMap<ItemId, StaticId>,
    instances: Vec<Instance>,
    instance_index: FxHashMap<Instance, FuncId>,
    paths_seen: FxHashMap<String, u32>,
    queue: VecDeque<FuncId>,
    /// Per function, its source file and `(instruction index, byte offset)` lines.
    lines: Vec<FnLines>,
    files: FxHashMap<String, u32>,
    diagnostics: Vec<Diagnostic>,
}

/// Whether `value` holds a `Screen`, which no constant can.
fn contains_screen(value: &ConstValue) -> bool {
    let mut found = false;
    value.walk(&mut |value| found |= matches!(value, ConstValue::Screen(_)));
    found
}

fn index(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The `(lo, hi, bits, signed)` of an integer type; `i128` when unknown.
fn int_type_of(ty: &Ty) -> IntType {
    match ty.kind() {
        TyKind::Int(int) => IntType {
            lo: IntTy::min(*int),
            hi: IntTy::max(*int),
            bits: crate::eval::int_bits(*int),
            signed: crate::eval::is_signed(*int),
        },
        TyKind::Bounded {
            lo: Const::Value(lo),
            hi: Const::Value(hi),
        } => IntType {
            lo: *lo,
            hi: *hi,
            bits: 128,
            signed: true,
        },
        _ => IntType::I128,
    }
}

fn bc_asset_kind(kind: AssetKind) -> BcAssetKind {
    match kind {
        AssetKind::Image => BcAssetKind::Image,
        AssetKind::Audio => BcAssetKind::Audio,
        AssetKind::Voice => BcAssetKind::Voice,
        AssetKind::Video => BcAssetKind::Video,
        AssetKind::Font => BcAssetKind::Font,
        AssetKind::Shader => BcAssetKind::Shader,
    }
}

impl<'db> Emitter<'db> {
    fn new(
        db: &'db dyn Db,
        program: Program,
        std: &'db StdTable,
        sigs: &'db ProgramSignatures,
        main: Option<ItemId>,
    ) -> Self {
        let root_dir = match program.root(db).path(db) {
            FilePath::Disk(path) => path.parent().map(PathBuf::from),
            FilePath::Std(_) => None,
        };
        let asset_root = match program.asset_root(db) {
            FilePath::Disk(path) => Some(path.to_path_buf()),
            FilePath::Std(_) => None,
        };
        let mut emitter = Emitter {
            db,
            program,
            std,
            sigs,
            maps: def_maps(db, program),
            main,
            root_dir,
            asset_root,
            module: Module {
                options: Options {
                    max_call_depth: program.options(db).recursion_depth,
                },
                ..Module::default()
            },
            strings: FxHashMap::default(),
            types: FxHashMap::default(),
            int_types: FxHashMap::default(),
            imports: FxHashMap::default(),
            assets: FxHashMap::default(),
            constants: FxHashMap::default(),
            statics: FxHashMap::default(),
            instances: Vec::new(),
            instance_index: FxHashMap::default(),
            paths_seen: FxHashMap::default(),
            queue: VecDeque::new(),
            lines: Vec::new(),
            files: FxHashMap::default(),
            diagnostics: Vec::new(),
        };
        emitter.int_type(IntType::I128);
        emitter
    }

    // ----- Tables -------------------------------------------------------------------

    fn string(&mut self, text: &str) -> StrId {
        if let Some(id) = self.strings.get(text) {
            return *id;
        }
        let id = StrId(index(self.module.strings.len()));
        self.module.strings.push(text.to_owned());
        self.strings.insert(Arc::from(text), id);
        id
    }

    fn type_id(&mut self, item: ItemId) -> TypeId {
        if let Some(id) = self.types.get(&item) {
            return *id;
        }
        let path = paths::item_path(self.db, self.maps, item);
        let path = self.string(&path);
        let (is_enum, variants) = match self.sigs.adt(item) {
            Some(adt) => (
                adt.is_enum,
                adt.variants
                    .iter()
                    .map(|variant| {
                        (
                            variant.name.to_string(),
                            variant
                                .fields
                                .iter()
                                .map(|field| field.name.as_ref().map(ToString::to_string))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            None => (false, Vec::new()),
        };
        let variants = variants
            .into_iter()
            .map(|(name, fields)| Variant {
                name: self.string(&name),
                fields: fields
                    .into_iter()
                    .map(|field| field.map(|field| self.string(&field)))
                    .collect(),
            })
            .collect();
        let id = TypeId(index(self.module.types.len()));
        self.module.types.push(AdtType {
            path,
            is_enum,
            variants,
        });
        self.types.insert(item, id);
        id
    }

    fn int_type(&mut self, ty: IntType) -> IntTypeId {
        if let Some(id) = self.int_types.get(&ty) {
            return *id;
        }
        let id = IntTypeId(index(self.module.int_types.len()));
        self.module.int_types.push(ty);
        self.int_types.insert(ty, id);
        id
    }

    fn import(&mut self, item: ItemId) -> ImportId {
        if let Some(id) = self.imports.get(&item) {
            return *id;
        }
        let path = paths::assoc_path(self.db, self.maps, item);
        let path = self.string(&path);
        let (kind, params) = match self.sigs.fn_sig(item) {
            Some(sig) => (
                match sig.kind {
                    FnKind::Plain => BcFnKind::Fn,
                    FnKind::Const => BcFnKind::Const,
                    FnKind::Pushdown => BcFnKind::Instant,
                },
                u16::try_from(sig.params.len()).unwrap_or(u16::MAX),
            ),
            None => (BcFnKind::Fn, 0),
        };
        let id = ImportId(index(self.module.imports.len()));
        self.module.imports.push(Import { path, kind, params });
        self.imports.insert(item, id);
        id
    }

    fn asset(&mut self, kind: AssetKind, path: &FilePath) -> AssetId {
        let key = (kind, path.clone());
        if let Some(id) = self.assets.get(&key) {
            return *id;
        }
        let shown = path.display(self.asset_root.as_deref());
        let shown = self.string(&shown);
        let id = AssetId(index(self.module.assets.len()));
        self.module.assets.push(Asset {
            kind: bc_asset_kind(kind),
            path: shown,
        });
        self.assets.insert(key, id);
        id
    }

    fn intern(&mut self, constant: Constant) -> ConstId {
        if let Some(id) = self.constants.get(&constant) {
            return *id;
        }
        let id = ConstId(index(self.module.constants.len()));
        self.module.constants.push(constant.clone());
        self.constants.insert(constant, id);
        id
    }

    /// The constant of `value`; `None` when it holds a `Screen`. Children are interned
    /// first, so a constant only refers to lower indices.
    fn constant(&mut self, value: &ConstValue) -> Option<ConstId> {
        let constant = match value {
            ConstValue::Bool(value) => Constant::Bool(*value),
            ConstValue::Int(value) => Constant::Int(*value),
            ConstValue::Decimal(value) => Constant::Decimal {
                mantissa: value.mantissa(),
                scale: value.scale(),
            },
            ConstValue::Rat(value) => Constant::Rat {
                num: value.numer(),
                den: value.denom(),
            },
            ConstValue::Text(text) => return self.text(text),
            ConstValue::String(text) => Constant::String(self.string(text)),
            ConstValue::Tuple(items) => Constant::Tuple(self.constants(items)?),
            ConstValue::Array(items) => Constant::Array(self.constants(items)?),
            ConstValue::Vec(items) => Constant::Vec(self.constants(items)?),
            ConstValue::Adt {
                item,
                variant,
                fields,
            } => {
                let fields = self.constants(fields)?;
                Constant::Adt {
                    ty: self.type_id(*item),
                    variant: *variant,
                    fields,
                }
            }
            ConstValue::Asset { kind, path } => Constant::Asset(self.asset(*kind, path)),
            ConstValue::Fn(fn_ref) => Constant::Function(self.func_value(fn_ref)),
            ConstValue::Closure { owner, expr } => {
                let owner = self.closure_owner(*owner);
                Constant::Function(self.request(Instance::Closure {
                    owner: Box::new(owner),
                    expr: *expr,
                }))
            }
            ConstValue::Handle { item, .. } => Constant::Handle(*self.statics.get(item)?),
            ConstValue::Screen(_) => return None,
        };
        Some(self.intern(constant))
    }

    fn constants(&mut self, values: &imbl::Vector<ConstValue>) -> Option<Vec<ConstId>> {
        values.iter().map(|value| self.constant(value)).collect()
    }

    fn text(&mut self, text: &Text) -> Option<ConstId> {
        let constant = match text {
            Text::Plain(text) => Constant::Text(self.string(text)),
            Text::Ruby { base, reading } => Constant::Ruby {
                base: self.text(base)?,
                reading: self.text(reading)?,
            },
            Text::MonoRuby { base, readings } => Constant::MonoRuby {
                base: self.text(base)?,
                readings: readings
                    .iter()
                    .map(|reading| self.text(reading))
                    .collect::<Option<_>>()?,
            },
            Text::Effect { base, effect } => Constant::Effect {
                base: self.text(base)?,
                effect: self.constant(effect)?,
            },
            Text::Concat(items) => Constant::Concat(
                items
                    .iter()
                    .map(|item| self.text(item))
                    .collect::<Option<_>>()?,
            ),
        };
        Some(self.intern(constant))
    }

    /// The instance that owns a capture-less closure constant: the function, or the
    /// initializer of the `const`/`static` it appears in.
    fn closure_owner(&self, owner: ItemId) -> Instance {
        match item_tree(self.db, owner.file)
            .item(owner.index)
            .map(|data| &data.kind)
        {
            Some(ItemKind::Const { .. } | ItemKind::Static { .. }) => {
                Instance::ConstInit(owner, Vec::new())
            }
            _ => Instance::Fn(FnRef {
                def: FnDefId::Item(owner),
                self_ty: None,
                args: GenericArgs::empty(),
            }),
        }
    }

    /// The function a `FnRef` value names: its body, its constructor or its builtin
    /// wrapper.
    fn func_value(&mut self, fn_ref: &FnRef) -> FuncId {
        match fn_ref.def {
            FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => {
                self.request(Instance::Ctor(FnRef {
                    def: fn_ref.def,
                    self_ty: None,
                    args: GenericArgs::empty(),
                }))
            }
            FnDefId::Item(item) if self.std.builtin(item).is_some() => {
                self.request(Instance::Builtin(fn_ref.clone()))
            }
            FnDefId::Item(_) => self.request(Instance::Fn(fn_ref.clone())),
        }
    }

    /// Every game `static mut` (story variables), then every `Setting`/`Meta` static of
    /// the game and std, in file and item order.
    fn statics(&mut self) {
        let mut handles = Vec::new();
        let mut story = Vec::new();
        let mut files: Vec<_> = self
            .maps
            .files
            .iter()
            .filter(|(_, krate)| *krate == CrateId::Game)
            .copied()
            .collect();
        files.extend(
            self.maps
                .files
                .iter()
                .filter(|(_, krate)| *krate == CrateId::Std)
                .copied(),
        );
        for (file, krate) in files {
            let tree = item_tree(self.db, file);
            for (position, data) in tree.items.iter().enumerate() {
                let ItemKind::Static { mutable } = data.kind else {
                    continue;
                };
                let Some(item_index) = ItemIndex::from_usize(position) else {
                    continue;
                };
                let item = ItemId {
                    file,
                    index: item_index,
                };
                if mutable {
                    if krate == CrateId::Game {
                        story.push((item, StaticKind::StoryVariable));
                    }
                    continue;
                }
                if let Ok(value) = static_value(self.db, self.program, item)
                    && let Some(kind) = value.handle
                {
                    let kind = match kind {
                        HandleKind::Setting => StaticKind::Setting,
                        HandleKind::Meta => StaticKind::Meta,
                    };
                    handles.push((item, kind));
                }
            }
        }
        story.extend(handles);
        // Every static gets its index before any initial value is interned: a handle
        // constant may name another static.
        for (position, (item, _)) in story.iter().enumerate() {
            self.statics.insert(*item, StaticId(index(position)));
        }
        for (item, kind) in story {
            let value = static_value(self.db, self.program, item)
                .as_ref()
                .ok()
                .map(|value| value.value.clone());
            let path = paths::item_path(self.db, self.maps, item);
            let path = self.string(&path);
            let init = value.as_ref().and_then(|value| self.constant(value));
            let Some(init) = init else {
                self.internal_error(item, "a static has no constant initial value");
                continue;
            };
            self.module.statics.push(Static { path, kind, init });
        }
    }

    /// The roots of the module: `fn main` and every non-generic entry scene of the game, in
    /// item order. Every other function is reached from them, or from a static.
    fn roots(&mut self) {
        let files: Vec<_> = self
            .maps
            .files
            .iter()
            .filter(|(_, krate)| *krate == CrateId::Game)
            .map(|(file, _)| *file)
            .collect();
        for file in files {
            let tree = item_tree(self.db, file);
            for (position, data) in tree.items.iter().enumerate() {
                if !matches!(data.kind, ItemKind::Fn { has_body: true, .. }) {
                    continue;
                }
                let Some(item_index) = ItemIndex::from_usize(position) else {
                    continue;
                };
                let item = ItemId {
                    file,
                    index: item_index,
                };
                let root = Some(item) == self.main
                    || self.sigs.fn_sig(item).is_some_and(|sig| {
                        sig.entry
                            && sig.generics.params.is_empty()
                            && sig.generics.parent_count == 0
                    });
                if !root {
                    continue;
                }
                self.request(Instance::Fn(FnRef {
                    def: FnDefId::Item(item),
                    self_ty: None,
                    args: GenericArgs::empty(),
                }));
            }
        }
    }

    // ----- Instances ----------------------------------------------------------------

    /// The function of `instance`, created (and queued for lowering) on first request.
    fn request(&mut self, instance: Instance) -> FuncId {
        if let Some(id) = self.instance_index.get(&instance) {
            return *id;
        }
        let id = FuncId(index(self.instances.len()));
        let path = self.instance_path(&instance);
        let count = self.paths_seen.entry(path.clone()).or_insert(0);
        *count = count.saturating_add(1);
        let path = if *count > 1 {
            format!("{path}#{count}")
        } else {
            path
        };
        let path = self.string(&path);
        let (kind, params) = self.instance_shape(&instance);
        let item = self.instance_item(&instance);
        let flags = FnFlags {
            entry: matches!(instance, Instance::Fn(_))
                && item
                    .and_then(|item| self.sigs.fn_sig(item))
                    .is_some_and(|sig| sig.entry),
            main: matches!(&instance, Instance::Fn(fn_ref)
                if Some(fn_ref.def) == self.main.map(FnDefId::Item)),
            closure: matches!(instance, Instance::Closure { .. }),
            synthetic: matches!(
                instance,
                Instance::Ctor(_)
                    | Instance::Builtin(_)
                    | Instance::ConstInit(..)
                    | Instance::Promoted(..)
            ),
        };
        self.module.functions.push(Function {
            path,
            kind,
            flags,
            params,
            regs: params,
            code: Vec::new(),
        });
        self.lines.push((None, Vec::new()));
        self.instances.push(instance.clone());
        self.instance_index.insert(instance, id);
        self.queue.push_back(id);
        id
    }

    /// The item whose body (or signature) the instance comes from.
    fn instance_item(&self, instance: &Instance) -> Option<ItemId> {
        match instance {
            Instance::Fn(fn_ref) | Instance::Builtin(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => Some(item),
                _ => None,
            },
            Instance::Closure { owner, .. } => self.instance_item(owner),
            Instance::Ctor(_) => None,
            Instance::ConstInit(item, _) => Some(*item),
            Instance::Promoted(owner, _) => self.instance_item(owner),
        }
    }

    fn fn_kind(&self, item: ItemId) -> BcFnKind {
        match self.sigs.fn_sig(item) {
            Some(sig) => match sig.kind {
                FnKind::Plain => BcFnKind::Fn,
                FnKind::Const => BcFnKind::Const,
                FnKind::Pushdown if sig.scene => BcFnKind::Scene,
                FnKind::Pushdown => BcFnKind::Instant,
            },
            None => BcFnKind::Const,
        }
    }

    /// The kind and parameter count of an instance.
    fn instance_shape(&self, instance: &Instance) -> (BcFnKind, u16) {
        let count = |len: usize| u16::try_from(len).unwrap_or(u16::MAX);
        match instance {
            Instance::Fn(fn_ref) | Instance::Builtin(fn_ref) => {
                let FnDefId::Item(item) = fn_ref.def else {
                    return (BcFnKind::Const, 0);
                };
                let params = self.sigs.fn_sig(item).map_or(0, |sig| sig.params.len());
                (self.fn_kind(item), count(params))
            }
            Instance::Closure { owner, expr } => {
                let body = self
                    .instance_item(owner)
                    .and_then(|item| item_mir(self.db, self.program, item).as_ref())
                    .and_then(|mir| mir.closure(*expr));
                let Some(body) = body else {
                    return (BcFnKind::Const, 0);
                };
                let kind = match body.kind {
                    BodyKind::Fn => BcFnKind::Fn,
                    BodyKind::Instant => BcFnKind::Instant,
                    BodyKind::Scene => BcFnKind::Scene,
                    BodyKind::ConstFn | BodyKind::Const => BcFnKind::Const,
                };
                (kind, count(body.arg_count as usize))
            }
            Instance::Ctor(fn_ref) => {
                let fields = match fn_ref.def {
                    FnDefId::StructCtor(item) => self
                        .sigs
                        .adt(item)
                        .and_then(|adt| adt.variants.first())
                        .map_or(0, |variant| variant.fields.len()),
                    FnDefId::VariantCtor(item, variant) => self
                        .sigs
                        .adt(item)
                        .and_then(|adt| adt.variants.get(variant as usize))
                        .map_or(0, |variant| variant.fields.len()),
                    FnDefId::Item(_) => 0,
                };
                (BcFnKind::Const, count(fields))
            }
            Instance::ConstInit(..) | Instance::Promoted(..) => (BcFnKind::Const, 0),
        }
    }

    fn args_suffix(&self, args: &[GenericArg]) -> String {
        if args.is_empty() {
            return String::new();
        }
        let shown: Vec<String> = args
            .iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => display_ty(self.db, self.program, ty),
                GenericArg::Const(Const::Value(value)) => value.to_string(),
                GenericArg::Const(_) => "_".to_owned(),
            })
            .collect();
        format!("::<{}>", shown.join(", "))
    }

    fn instance_path(&self, instance: &Instance) -> String {
        match instance {
            Instance::Fn(fn_ref) | Instance::Builtin(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => format!(
                    "{}{}",
                    paths::assoc_path(self.db, self.maps, item),
                    self.args_suffix(&fn_ref.args)
                ),
                _ => "?".to_owned(),
            },
            Instance::Ctor(fn_ref) => match fn_ref.def {
                FnDefId::StructCtor(item) => paths::def_path(self.db, self.maps, Def::Item(item)),
                FnDefId::VariantCtor(enum_item, index) => {
                    paths::def_path(self.db, self.maps, Def::Variant { enum_item, index })
                }
                FnDefId::Item(item) => paths::item_path(self.db, self.maps, item),
            },
            Instance::ConstInit(item, args) => format!(
                "{}{}",
                paths::assoc_path(self.db, self.maps, *item),
                self.args_suffix(args)
            ),
            Instance::Closure { owner, expr } => {
                let number = self
                    .instance_item(owner)
                    .and_then(|item| crate::hir::body_of(self.db, item))
                    .and_then(|body| {
                        body.store
                            .exprs
                            .iter()
                            .filter(|(_, data)| matches!(data, Expr::Closure { .. }))
                            .position(|(id, _)| id == *expr)
                    })
                    .unwrap_or(0);
                format!("{}::{{closure#{number}}}", self.instance_path(owner))
            }
            Instance::Promoted(owner, index) => {
                format!(
                    "{}::{{promoted#{}}}",
                    self.instance_path(owner),
                    index.idx()
                )
            }
        }
    }

    /// The generic environment of an instance's body.
    fn instance_env(&self, instance: &Instance) -> GenericEnv {
        match instance {
            Instance::Fn(fn_ref) | Instance::Builtin(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => GenericEnv::for_item(self.sigs, item, fn_ref.args.clone()),
                _ => GenericEnv::default(),
            },
            Instance::Closure { owner, .. } => self.instance_env(owner),
            Instance::ConstInit(item, args) => {
                GenericEnv::for_item(self.sigs, *item, GenericArgs::new(args.clone()))
            }
            Instance::Ctor(_) => GenericEnv::default(),
            Instance::Promoted(owner, _) => self.instance_env(owner),
        }
    }

    fn int_of(&mut self, ty: &Ty) -> IntTypeId {
        self.int_type(int_type_of(ty))
    }

    // ----- Driver -------------------------------------------------------------------

    fn run(&mut self) {
        while let Some(id) = self.queue.pop_front() {
            let Some(instance) = self.instances.get(id.0 as usize).cloned() else {
                continue;
            };
            let lowered = codegen::lower_instance(self, &instance);
            let file = self
                .instance_item(&instance)
                .filter(|_| !matches!(instance, Instance::Ctor(_) | Instance::Builtin(_)))
                .map(|item| {
                    let shown = item.file.path(self.db).display(self.root_dir.as_deref());
                    let next = index(self.files.len());
                    *self.files.entry(shown).or_insert(next)
                });
            let path = self
                .module
                .functions
                .get(id.0 as usize)
                .and_then(|function| self.module.strings.get(function.path.0 as usize))
                .cloned()
                .unwrap_or_default();
            let too_large = lowered.too_large
                || u32::try_from(
                    lowered
                        .code
                        .iter()
                        .map(|instr| u64::from(instr.words()))
                        .sum::<u64>(),
                )
                .is_err();
            if too_large {
                let item = self.instance_item(&instance);
                let diagnostic = match item {
                    Some(item) => {
                        let (_, range) = resolve_anchor(self.db, &Anchor::ItemName(item));
                        Diagnostic::new(
                            item.file.path(self.db).clone(),
                            range,
                            DiagnosticCode::EmitFunctionTooLarge,
                            format!(
                                "`{path}` is too large for the bytecode: it needs more than 65535 \
                                 registers"
                            ),
                        )
                    }
                    None => root_diagnostic(
                        self.db,
                        self.program,
                        DiagnosticCode::EmitFunctionTooLarge,
                        format!(
                            "`{path}` is too large for the bytecode: it needs more than 65535 \
                             registers"
                        ),
                    ),
                };
                self.diagnostics.push(diagnostic);
            }
            if let Some(function) = self.module.functions.get_mut(id.0 as usize) {
                function.regs = lowered.regs.max(function.params);
                function.code = lowered.code;
            }
            if let Some(slot) = self.lines.get_mut(id.0 as usize) {
                *slot = (file, lowered.lines);
            }
        }
    }

    fn internal_error(&mut self, item: ItemId, message: &str) {
        self.diagnostics.push(Diagnostic::new(
            item.file.path(self.db).clone(),
            resolve_anchor(self.db, &Anchor::ItemName(item)).1,
            DiagnosticCode::EmitInvalidBytecode,
            format!("internal compiler error: {message}"),
        ));
    }

    fn finish(mut self) -> Result<Module, Vec<Diagnostic>> {
        if !self.diagnostics.is_empty() {
            return Err(self.diagnostics);
        }
        let mut files: Vec<(u32, String)> = self
            .files
            .iter()
            .map(|(path, id)| (*id, path.clone()))
            .collect();
        files.sort();
        let files = files
            .into_iter()
            .map(|(_, path)| self.string(&path))
            .collect();
        let functions = self
            .module
            .functions
            .iter()
            .zip(&self.lines)
            .map(|(function, (file, lines))| {
                let offsets = word_offsets(&function.code);
                FnDebug {
                    file: *file,
                    lines: lines
                        .iter()
                        .filter_map(|(instr, offset)| {
                            Some((*offsets.get(*instr as usize)?, *offset))
                        })
                        .collect(),
                }
            })
            .collect();
        self.module.debug = Some(Debug { files, functions });
        Ok(self.module)
    }
}
