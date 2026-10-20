//! Types of `.kix`/`.kirp` code, type lowering and per-run signature caches.

use std::cell::RefCell;
use std::rc::Rc;

use kagari_syntax::TextRange;
use kagari_syntax::represent::ast::{self, HasName};
use rowan::ast::AstNode;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::SourceFile;
use crate::parse::parse_represent;
use crate::represent::item_tree::{DefId, Item, ItemKind};
use crate::represent::scope::{ItemScope, item};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Bool,
    Int(IntTy),
    Rational,
    Str,
    Unit,
    Adt {
        def: DefId,
        args: Vec<Ty>,
    },
    /// A generic parameter of an item (or of the impl around it).
    Param {
        owner: DefId,
        index: u32,
    },
    /// An inference variable.
    Var(u32),
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntTy {
    /// The inclusive value range.
    pub fn range(self) -> (i128, i128) {
        match self {
            IntTy::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
            IntTy::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
            IntTy::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
            IntTy::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
            IntTy::U8 => (0, i128::from(u8::MAX)),
            IntTy::U16 => (0, i128::from(u16::MAX)),
            IntTy::U32 => (0, i128::from(u32::MAX)),
            IntTy::U64 => (0, i128::from(u64::MAX)),
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64)
    }

    pub fn name(self) -> &'static str {
        match self {
            IntTy::I8 => "i8",
            IntTy::I16 => "i16",
            IntTy::I32 => "i32",
            IntTy::I64 => "i64",
            IntTy::U8 => "u8",
            IntTy::U16 => "u16",
            IntTy::U32 => "u32",
            IntTy::U64 => "u64",
        }
    }
}

/// The primitive type named `name`.
pub fn primitive(name: &str) -> Option<Ty> {
    Some(match name {
        "bool" => Ty::Bool,
        "i8" => Ty::Int(IntTy::I8),
        "i16" => Ty::Int(IntTy::I16),
        "i32" => Ty::Int(IntTy::I32),
        "i64" => Ty::Int(IntTy::I64),
        "u8" => Ty::Int(IntTy::U8),
        "u16" => Ty::Int(IntTy::U16),
        "u32" => Ty::Int(IntTy::U32),
        "u64" => Ty::Int(IntTy::U64),
        "rational" => Ty::Rational,
        "str" => Ty::Str,
        _ => return None,
    })
}

/// A substitution of generic parameters: `(owner, args)` maps `Param { owner, i }` to `args[i]`.
pub type Subst = [(DefId, Vec<Ty>)];

pub fn subst(ty: &Ty, map: &Subst) -> Ty {
    match ty {
        Ty::Param { owner, index } => map
            .iter()
            .find(|(def, _)| def == owner)
            .map(|(_, args)| args.get(*index as usize).cloned().unwrap_or(Ty::Error))
            .unwrap_or_else(|| ty.clone()),
        Ty::Adt { def, args } => Ty::Adt {
            def: *def,
            args: args.iter().map(|arg| subst(arg, map)).collect(),
        },
        _ => ty.clone(),
    }
}

/// Where a type is written: its file, the generic parameters in scope and `Self`.
#[derive(Clone, Debug)]
pub struct TyEnv {
    pub file: SourceFile,
    /// Outer owners first; a later owner's names shadow an earlier one's.
    pub generics: Vec<(DefId, Vec<SmolStr>)>,
    pub self_ty: Option<Ty>,
}

impl TyEnv {
    /// An environment for code outside any item (inline `<r>`).
    pub fn bare(file: SourceFile) -> TyEnv {
        TyEnv {
            file,
            generics: Vec::new(),
            self_ty: None,
        }
    }
}

/// A function signature. `Param`s are owned by the impl (`impl_def`) and the fn itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnSig {
    pub def: DefId,
    pub impl_def: Option<DefId>,
    /// The impl's self type, with the impl's parameters as arguments.
    pub self_ty: Option<Ty>,
    pub has_self: bool,
    /// Parameters after `self`.
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub builtin: bool,
}

type Fields = Option<Rc<Vec<(SmolStr, Ty)>>>;
type Variants = Rc<Vec<(SmolStr, Vec<Ty>)>>;

#[derive(Default)]
struct Cache {
    fields: FxHashMap<DefId, Fields>,
    variants: FxHashMap<DefId, Variants>,
    sigs: FxHashMap<DefId, Rc<FnSig>>,
    consts: FxHashMap<DefId, Ty>,
    aliases: FxHashMap<DefId, Ty>,
    expanding: Vec<DefId>,
}

/// Type information for one checking run. Not a salsa structure: caches live per run.
pub struct TyCtx<'db> {
    pub db: &'db dyn Db,
    pub scope: &'db ItemScope,
    cache: RefCell<Cache>,
}

/// What a type path resolves to.
pub enum TypeRes {
    Ty(Ty),
    Def(DefId),
}

impl<'db> TyCtx<'db> {
    pub fn new(db: &'db dyn Db, scope: &'db ItemScope) -> TyCtx<'db> {
        TyCtx {
            db,
            scope,
            cache: RefCell::new(Cache::default()),
        }
    }

    pub fn item(&self, def: DefId) -> Option<&'db Item> {
        item(self.db, def)
    }

    pub fn name(&self, def: DefId) -> SmolStr {
        self.item(def)
            .map(|item| item.name.clone())
            .unwrap_or_default()
    }

    pub fn has_attr(&self, def: DefId, name: &str) -> bool {
        self.item(def).is_some_and(|item| item.has_attr(name))
    }

    /// The syntax node of a definition.
    pub fn node<N: AstNode<Language = kagari_syntax::represent::RepresentLanguage>>(
        &self,
        def: DefId,
    ) -> Option<N> {
        let root = parse_represent(self.db, def.file).syntax_node();
        N::cast(self.item(def)?.ptr.try_to_node(&root)?)
    }

    /// The impl directly around `def`, if any.
    pub fn parent_impl(&self, def: DefId) -> Option<DefId> {
        let parent = self.item(def)?.parent?;
        let parent = DefId {
            file: def.file,
            index: parent,
        };
        matches!(self.item(parent)?.kind, ItemKind::Impl { .. }).then_some(parent)
    }

    /// The self type of an impl block, with its own parameters as arguments.
    pub fn impl_self_ty(&self, impl_def: DefId) -> Option<Ty> {
        let target = *self.scope.impl_targets.get(&impl_def)?;
        let count = self.item(impl_def)?.kind.generics().len();
        Some(Ty::Adt {
            def: target,
            args: params(impl_def, count),
        })
    }

    /// The environment in which the types of `def` are written.
    pub fn env_for(&self, def: DefId) -> TyEnv {
        let mut env = TyEnv::bare(def.file);
        let Some(item) = self.item(def) else {
            return env;
        };
        if let Some(imp) = self.parent_impl(def) {
            if let Some(imp_item) = self.item(imp) {
                env.generics.push((imp, imp_item.kind.generics().to_vec()));
            }
            env.self_ty = Some(self.impl_self_ty(imp).unwrap_or(Ty::Error));
        }
        match &item.kind {
            ItemKind::Struct { generics, .. } | ItemKind::Enum { generics, .. } => {
                env.self_ty = Some(Ty::Adt {
                    def,
                    args: params(def, generics.len()),
                });
                env.generics.push((def, generics.clone()));
            }
            ItemKind::Impl { generics, .. } => {
                env.generics.push((def, generics.clone()));
                env.self_ty = Some(self.impl_self_ty(def).unwrap_or(Ty::Error));
            }
            ItemKind::TypeAlias { generics } | ItemKind::Fn { generics, .. } => {
                env.generics.push((def, generics.clone()));
            }
            ItemKind::Const | ItemKind::AssetImport { .. } | ItemKind::Mod { .. } => {}
        }
        env
    }

    /// Lower a written type, reporting resolution errors into `out`.
    pub fn lower_type(&self, env: &TyEnv, ty: &ast::Type, out: &mut Vec<Diagnostic>) -> Ty {
        match ty {
            ast::Type::UnitType(_) => Ty::Unit,
            ast::Type::PathType(path_ty) => match path_ty.path() {
                Some(path) => self.lower_path_type(env, &path, out),
                None => Ty::Error,
            },
        }
    }

    /// Lower an optional type; a missing type (a syntax error) is `Error`.
    pub fn lower_opt(&self, env: &TyEnv, ty: Option<ast::Type>, out: &mut Vec<Diagnostic>) -> Ty {
        ty.map(|ty| self.lower_type(env, &ty, out))
            .unwrap_or(Ty::Error)
    }

    fn error(
        &self,
        env: &TyEnv,
        range: TextRange,
        code: DiagnosticCode,
        message: String,
        out: &mut Vec<Diagnostic>,
    ) -> Ty {
        out.push(Diagnostic::new(
            env.file.path(self.db).clone(),
            range,
            code,
            message,
        ));
        Ty::Error
    }

    fn lower_path_type(&self, env: &TyEnv, path: &ast::Path, out: &mut Vec<Diagnostic>) -> Ty {
        let segments: Vec<ast::PathSegment> = path.segments().collect();
        let Some(last) = segments.last() else {
            return Ty::Error;
        };
        let args: Vec<Ty> = last
            .generic_arg_list()
            .map(|list| {
                list.types()
                    .map(|arg| self.lower_type(env, &arg, out))
                    .collect()
            })
            .unwrap_or_default();
        let def = match self.resolve_type_path(env, &segments, out) {
            None => return Ty::Error,
            Some(TypeRes::Ty(ty)) => {
                if last.generic_arg_list().is_some() {
                    return self.wrong_generic_count(env, path, 0, args.len(), out);
                }
                return ty;
            }
            Some(TypeRes::Def(def)) => def,
        };
        self.instantiate(env, path.syntax().text_range(), def, args, out)
    }

    /// The type `def<args>`: an ADT, or an expanded alias.
    pub fn instantiate(
        &self,
        env: &TyEnv,
        range: TextRange,
        def: DefId,
        args: Vec<Ty>,
        out: &mut Vec<Diagnostic>,
    ) -> Ty {
        let Some(item) = self.item(def) else {
            return Ty::Error;
        };
        let expected = item.kind.generics().len();
        match &item.kind {
            ItemKind::Struct { .. } | ItemKind::Enum { .. } | ItemKind::TypeAlias { .. }
                if args.len() != expected =>
            {
                let message = format!(
                    "`{}` takes {expected} generic argument(s), but {} were given",
                    item.name,
                    args.len()
                );
                self.error(
                    env,
                    range,
                    DiagnosticCode::TypeWrongGenericCount,
                    message,
                    out,
                )
            }
            ItemKind::Struct { .. } | ItemKind::Enum { .. } => Ty::Adt { def, args },
            ItemKind::TypeAlias { .. } => subst(&self.alias_target(def), &[(def, args)]),
            kind => {
                let message = format!("`{}` is a {}, not a type", item.name, kind.describe());
                self.error(env, range, DiagnosticCode::TypeUnresolvedType, message, out)
            }
        }
    }

    fn wrong_generic_count(
        &self,
        env: &TyEnv,
        path: &ast::Path,
        expected: usize,
        given: usize,
        out: &mut Vec<Diagnostic>,
    ) -> Ty {
        let message = format!(
            "`{}` takes {expected} generic argument(s), but {given} were given",
            path.syntax().text()
        );
        self.error(
            env,
            path.syntax().text_range(),
            DiagnosticCode::TypeWrongGenericCount,
            message,
            out,
        )
    }

    /// Resolve the segments of a type path (generic args of the last segment are not lowered).
    pub fn resolve_type_path(
        &self,
        env: &TyEnv,
        segments: &[ast::PathSegment],
        out: &mut Vec<Diagnostic>,
    ) -> Option<TypeRes> {
        let (first, rest) = segments.split_first()?;
        let first_kind = first.kind()?;
        if rest.is_empty() {
            match &first_kind {
                ast::PathSegmentKind::SelfType => {
                    return Some(TypeRes::Ty(match &env.self_ty {
                        Some(ty) => ty.clone(),
                        None => self.error(
                            env,
                            first.syntax().text_range(),
                            DiagnosticCode::TypeSelfOutsideImpl,
                            "`Self` is only available inside an impl".to_owned(),
                            out,
                        ),
                    }));
                }
                ast::PathSegmentKind::SelfValue => return Some(TypeRes::Ty(Ty::Error)),
                ast::PathSegmentKind::Name(name) => {
                    let text = name.text();
                    if let Some(ty) = primitive(&text) {
                        return Some(TypeRes::Ty(ty));
                    }
                    if let Some(ty) = generic_param(env, &text) {
                        return Some(TypeRes::Ty(ty));
                    }
                }
            }
        }
        let mut module: Option<DefId> = None;
        for (position, segment) in segments.iter().enumerate() {
            let is_last = position.saturating_add(1) == segments.len();
            let ast::PathSegmentKind::Name(name) = segment.kind()? else {
                let range = segment.syntax().text_range();
                let message = "`Self` and `self` cannot appear inside a type path".to_owned();
                self.error(env, range, DiagnosticCode::TypeUnresolvedType, message, out);
                return None;
            };
            let text = name.text();
            let found = self
                .scope
                .module(module)
                .and_then(|scope| scope.types.get(text.as_str()))
                .copied();
            let Some(def) = found else {
                let message = format!("cannot find type `{text}`");
                let range = segment.syntax().text_range();
                self.error(env, range, DiagnosticCode::TypeUnresolvedType, message, out);
                return None;
            };
            if is_last {
                return Some(TypeRes::Def(def));
            }
            let is_mod = matches!(
                self.item(def).map(|item| &item.kind),
                Some(ItemKind::Mod { .. })
            );
            if !is_mod || segment.generic_arg_list().is_some() {
                let message = format!("`{text}` is not a module");
                let range = segment.syntax().text_range();
                self.error(env, range, DiagnosticCode::TypeUnresolvedType, message, out);
                return None;
            }
            module = Some(def);
        }
        None
    }

    /// The body of an alias with its own parameters free; `Error` on a cycle.
    pub fn alias_target(&self, def: DefId) -> Ty {
        if let Some(ty) = self.cache.borrow().aliases.get(&def) {
            return ty.clone();
        }
        {
            let mut cache = self.cache.borrow_mut();
            if cache.expanding.contains(&def) {
                return Ty::Error;
            }
            cache.expanding.push(def);
        }
        let env = self.env_for(def);
        let alias: Option<ast::TypeAlias> = self.node(def);
        let ty = self.lower_opt(&env, alias.and_then(|alias| alias.ty()), &mut Vec::new());
        let mut cache = self.cache.borrow_mut();
        cache.expanding.retain(|other| *other != def);
        cache.aliases.insert(def, ty.clone());
        ty
    }

    /// Whether the alias `def` refers back to itself through aliases.
    pub fn alias_is_cyclic(&self, def: DefId) -> bool {
        let mut stack = vec![def];
        let mut seen: Vec<DefId> = Vec::new();
        while let Some(current) = stack.pop() {
            for next in self.alias_refs(current) {
                if next == def {
                    return true;
                }
                if !seen.contains(&next) {
                    seen.push(next);
                    stack.push(next);
                }
            }
        }
        false
    }

    /// Aliases named anywhere in the body of the alias `def`.
    fn alias_refs(&self, def: DefId) -> Vec<DefId> {
        let env = self.env_for(def);
        let Some(alias) = self.node::<ast::TypeAlias>(def) else {
            return Vec::new();
        };
        let Some(ty) = alias.ty() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for path in ty.syntax().descendants().filter_map(ast::Path::cast) {
            let segments: Vec<ast::PathSegment> = path.segments().collect();
            if let Some(TypeRes::Def(target)) =
                self.resolve_type_path(&env, &segments, &mut Vec::new())
                && matches!(
                    self.item(target).map(|item| &item.kind),
                    Some(ItemKind::TypeAlias { .. })
                )
            {
                found.push(target);
            }
        }
        found
    }

    /// The fields of a struct (with its own parameters free); `None` for unit structs and
    /// non-structs.
    pub fn struct_fields(&self, def: DefId) -> Option<Rc<Vec<(SmolStr, Ty)>>> {
        if let Some(fields) = self.cache.borrow().fields.get(&def) {
            return fields.clone();
        }
        let fields = (|| {
            let strukt: ast::Struct = self.node(def)?;
            let list = strukt.record_field_list()?;
            let env = self.env_for(def);
            let fields: Vec<(SmolStr, Ty)> = list
                .fields()
                .map(|field| {
                    let name =
                        SmolStr::new(field.name().map(|name| name.text()).unwrap_or_default());
                    (name, self.lower_opt(&env, field.ty(), &mut Vec::new()))
                })
                .collect();
            Some(Rc::new(fields))
        })();
        self.cache.borrow_mut().fields.insert(def, fields.clone());
        fields
    }

    /// The variants of an enum and their payload types (with its own parameters free).
    pub fn enum_variants(&self, def: DefId) -> Rc<Vec<(SmolStr, Vec<Ty>)>> {
        if let Some(variants) = self.cache.borrow().variants.get(&def) {
            return variants.clone();
        }
        let env = self.env_for(def);
        let variants: Vec<(SmolStr, Vec<Ty>)> = self
            .node::<ast::Enum>(def)
            .map(|enm| {
                enm.variants()
                    .map(|variant| {
                        let name = SmolStr::new(
                            variant.name().map(|name| name.text()).unwrap_or_default(),
                        );
                        let payload = variant
                            .tuple_field_list()
                            .map(|list| {
                                list.types()
                                    .map(|ty| self.lower_type(&env, &ty, &mut Vec::new()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (name, payload)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let variants = Rc::new(variants);
        self.cache
            .borrow_mut()
            .variants
            .insert(def, variants.clone());
        variants
    }

    /// The signature of a fn.
    pub fn fn_sig(&self, def: DefId) -> Rc<FnSig> {
        if let Some(sig) = self.cache.borrow().sigs.get(&def) {
            return sig.clone();
        }
        let env = self.env_for(def);
        let impl_def = self.parent_impl(def);
        let function: Option<ast::Fn> = self.node(def);
        let list = function.as_ref().and_then(|function| function.param_list());
        let mut sink = Vec::new();
        let params = list
            .as_ref()
            .map(|list| {
                list.params()
                    .map(|param| self.lower_opt(&env, param.ty(), &mut sink))
                    .collect()
            })
            .unwrap_or_default();
        let ret = match function.as_ref().and_then(|function| function.ret_type()) {
            Some(ret) => self.lower_opt(&env, ret.ty(), &mut sink),
            None => Ty::Unit,
        };
        let sig = Rc::new(FnSig {
            def,
            impl_def,
            self_ty: env.self_ty.clone(),
            has_self: list.as_ref().and_then(|list| list.self_param()).is_some(),
            params,
            ret,
            builtin: self.has_attr(def, "builtin"),
        });
        self.cache.borrow_mut().sigs.insert(def, sig.clone());
        sig
    }

    /// The declared type of a const or asset import.
    pub fn value_ty(&self, def: DefId) -> Ty {
        if let Some(ty) = self.cache.borrow().consts.get(&def) {
            return ty.clone();
        }
        let env = self.env_for(def);
        let written = match self.item(def).map(|item| &item.kind) {
            Some(ItemKind::Const) => self.node::<ast::Const>(def).and_then(|c| c.ty()),
            Some(ItemKind::AssetImport { .. }) => {
                self.node::<ast::AssetImport>(def).and_then(|i| i.ty())
            }
            _ => None,
        };
        let ty = self.lower_opt(&env, written, &mut Vec::new());
        self.cache.borrow_mut().consts.insert(def, ty.clone());
        ty
    }

    /// The prelude struct `def` as a type, if the prelude is complete.
    pub fn prelude_ty(&self, pick: impl Fn(&crate::represent::scope::PreludeDefs) -> DefId) -> Ty {
        match &self.scope.prelude {
            Some(defs) => Ty::Adt {
                def: pick(defs),
                args: Vec::new(),
            },
            None => Ty::Error,
        }
    }

    /// The name of a generic parameter.
    pub fn param_name(&self, owner: DefId, index: u32) -> SmolStr {
        self.item(owner)
            .and_then(|item| item.kind.generics().get(index as usize).cloned())
            .unwrap_or_else(|| SmolStr::new("?"))
    }

    /// Render a type; `var` renders inference variables.
    pub fn display(&self, ty: &Ty, var: &dyn Fn(u32) -> String) -> String {
        match ty {
            Ty::Bool => "bool".to_owned(),
            Ty::Int(int) => int.name().to_owned(),
            Ty::Rational => "rational".to_owned(),
            Ty::Str => "str".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Adt { def, args } => {
                let name = self.name(*def);
                if args.is_empty() {
                    name.to_string()
                } else {
                    let args: Vec<String> = args.iter().map(|arg| self.display(arg, var)).collect();
                    format!("{name}<{}>", args.join(", "))
                }
            }
            Ty::Param { owner, index } => self.param_name(*owner, *index).to_string(),
            Ty::Var(var_index) => var(*var_index),
            Ty::Error => "{unknown}".to_owned(),
        }
    }
}

/// `[Param { owner, 0 }, …]`.
pub fn params(owner: DefId, count: usize) -> Vec<Ty> {
    (0u32..)
        .take(count)
        .map(|index| Ty::Param { owner, index })
        .collect()
}

fn generic_param(env: &TyEnv, name: &str) -> Option<Ty> {
    env.generics.iter().rev().find_map(|(owner, names)| {
        let index = names.iter().position(|param| param == name)?;
        Some(Ty::Param {
            owner: *owner,
            index: u32::try_from(index).ok()?,
        })
    })
}
