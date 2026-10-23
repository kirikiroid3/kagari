//! AST → HIR lowering (compilation stage 1).

use kagari_syntax::ast::{self, AstNode, HasGenericParams, HasName};
use kagari_syntax::{SyntaxNode, TextRange};

use super::{
    ArrayExpr, BinaryOp, Body, ClosureKind, Condition, ExprStore, FieldName, FnPtrKind, GenericArg,
    GenericArgs, GenericParam, GenericParamKind, Literal, MatchArm, Name, Pat, PatBound, PatId,
    Path, PathKind, PathSegment, SelfParamKind, Signature, SignatureKind, Stmt, TypeRef, TypeRefId,
    UnaryOp,
};
use super::{Expr, ExprId};
use crate::db::Db;
use crate::input::SourceFile;
use crate::item_tree::{ItemId, ItemTree, item_tree, name_text};
use crate::parse_query::parse;

/// The signatures and bodies of every item of a file, indexed like `ItemTree::items`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileHir {
    pub signatures: Vec<Option<Signature>>,
    pub bodies: Vec<Option<Body>>,
}

#[salsa::tracked(returns(ref))]
pub fn file_hir(db: &dyn Db, file: SourceFile) -> FileHir {
    let root = parse(db, file).syntax_node();
    let tree = item_tree(db, file);
    let mut out = FileHir::default();
    for item in &tree.items {
        let node = item.ptr.to_node(&root);
        let mut lowerer = Lowerer {
            file,
            tree,
            store: ExprStore::default(),
        };
        let signature = lowerer.lower_signature(&node);
        let body = lower_body(file, tree, &node);
        out.signatures.push(signature);
        out.bodies.push(body);
    }
    out
}

fn lower_body(file: SourceFile, tree: &ItemTree, node: &SyntaxNode) -> Option<Body> {
    let mut lowerer = Lowerer {
        file,
        tree,
        store: ExprStore::default(),
    };
    let (params, has_self, value) = if let Some(it) = ast::Fn::cast(node.clone()) {
        let body = it.body()?;
        let param_list = it.param_list();
        let has_self = param_list
            .as_ref()
            .and_then(|list| list.self_param())
            .is_some();
        let params = param_list
            .into_iter()
            .flat_map(|list| list.params())
            .map(|param| {
                let range = param.syntax().text_range();
                lowerer.lower_pat_opt(param.pat(), range)
            })
            .collect();
        (
            params,
            has_self,
            lowerer.lower_expr(ast::Expr::BlockExpr(body)),
        )
    } else if let Some(it) = ast::Const::cast(node.clone()) {
        (Vec::new(), false, lowerer.lower_expr(it.body()?))
    } else {
        let it = ast::Static::cast(node.clone())?;
        (Vec::new(), false, lowerer.lower_expr(it.body()?))
    };
    Some(Body {
        store: lowerer.store,
        params,
        has_self,
        value,
    })
}

pub(super) struct Lowerer<'a> {
    pub(super) file: SourceFile,
    pub(super) tree: &'a ItemTree,
    pub(super) store: ExprStore,
}

fn range_of(node: &impl AstNode) -> TextRange {
    node.syntax().text_range()
}

/// The range of the first non-trivia token of `node`.
fn first_token_range(node: &SyntaxNode) -> TextRange {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
        .map_or_else(|| node.text_range(), |token| token.text_range())
}

fn label_name(token: &kagari_syntax::SyntaxToken) -> Name {
    Name::new(token.text().trim_start_matches('\''))
}

/// Lowers an AST path; `args` lowers each segment's generic argument list. `None` when a
/// segment is missing (a syntax error).
pub(crate) fn lower_path(
    path: &ast::Path,
    args: &mut dyn FnMut(ast::GenericArgList) -> Option<GenericArgs>,
) -> Option<Path> {
    let mut segments = Vec::new();
    let mut current = Some(path.clone());
    while let Some(path) = current {
        segments.push(path.segment()?);
        current = path.qualifier();
    }
    segments.reverse();
    let count = segments.len();
    let mut kind = PathKind::Plain;
    let mut out: Vec<PathSegment> = Vec::new();
    for (index, segment) in segments.into_iter().enumerate() {
        let range = first_token_range(segment.syntax());
        let name = match segment.kind()? {
            ast::PathSegmentKind::Name(name_ref) => Name::new(name_ref.text()),
            ast::PathSegmentKind::CrateKw if index == 0 => {
                kind = PathKind::Crate;
                continue;
            }
            ast::PathSegmentKind::SelfKw if index == 0 && count > 1 => {
                kind = PathKind::SelfModule;
                continue;
            }
            ast::PathSegmentKind::SuperKw
                if out.is_empty() && matches!(kind, PathKind::Plain | PathKind::Super(_)) =>
            {
                kind = match kind {
                    PathKind::Super(count) => PathKind::Super(count.saturating_add(1)),
                    _ => PathKind::Super(1),
                };
                continue;
            }
            ast::PathSegmentKind::CrateKw => Name::new_static("crate"),
            ast::PathSegmentKind::SelfKw => Name::new_static("self"),
            ast::PathSegmentKind::SuperKw => Name::new_static("super"),
            ast::PathSegmentKind::SelfTypeKw => Name::new_static("Self"),
        };
        let args = segment.generic_arg_list().and_then(&mut *args);
        out.push(PathSegment { name, range, args });
    }
    Some(Path {
        kind,
        segments: out,
    })
}

impl Lowerer<'_> {
    pub(super) fn alloc(&mut self, expr: Expr, range: TextRange) -> ExprId {
        self.store.alloc_expr(expr, range)
    }

    pub(super) fn missing(&mut self, range: TextRange) -> ExprId {
        self.alloc(Expr::Missing, range)
    }

    // ---- signatures ----

    fn lower_signature(&mut self, node: &SyntaxNode) -> Option<Signature> {
        let item = ast::Item::cast(node.clone())?;
        let generics = match &item {
            ast::Item::Fn(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::Struct(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::Enum(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::TypeAlias(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::Impl(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::Trait(it) => self.lower_generics(it.generic_param_list()),
            ast::Item::Const(_) | ast::Item::Static(_) | ast::Item::Mod(_) | ast::Item::Use(_) => {
                Vec::new()
            }
        };
        let range = node.text_range();
        let kind = match &item {
            ast::Item::Fn(it) => {
                let param_list = it.param_list();
                let self_param =
                    param_list
                        .as_ref()
                        .and_then(|list| list.self_param())
                        .map(|param| match (param.amp_token(), param.mut_token()) {
                            (Some(_), Some(_)) => SelfParamKind::RefMut,
                            (Some(_), None) => SelfParamKind::Ref,
                            (None, _) => SelfParamKind::Value,
                        });
                let params = param_list
                    .into_iter()
                    .flat_map(|list| list.params())
                    .map(|param| {
                        let range = range_of(&param);
                        self.lower_type_opt(param.ty(), range)
                    })
                    .collect();
                let ret = it
                    .ret_type()
                    .map(|ret| self.lower_type_opt(ret.ty(), range_of(&ret)));
                SignatureKind::Fn {
                    params,
                    self_param,
                    ret,
                }
            }
            ast::Item::Struct(it) => SignatureKind::Struct {
                fields: self.lower_fields(it.record_field_list(), it.tuple_field_list()),
            },
            ast::Item::Enum(it) => SignatureKind::Enum {
                variants: it
                    .variant_list()
                    .into_iter()
                    .flat_map(|list| list.variants())
                    .filter(|variant| variant.name().is_some())
                    .map(|variant| {
                        self.lower_fields(variant.record_field_list(), variant.tuple_field_list())
                    })
                    .collect(),
            },
            ast::Item::Const(it) => SignatureKind::Const {
                ty: self.lower_type_opt(it.ty(), range),
            },
            ast::Item::Static(it) => SignatureKind::Static {
                ty: self.lower_type_opt(it.ty(), range),
            },
            ast::Item::TypeAlias(it) => SignatureKind::TypeAlias {
                bounds: self.lower_bounds(it.type_bound_list()),
                value: it.ty().map(|ty| self.lower_type(ty)),
            },
            ast::Item::Impl(it) => SignatureKind::Impl {
                trait_ref: it.trait_type().map(|ty| self.lower_type(ty)),
                self_ty: self.lower_type_opt(it.self_type(), range),
            },
            ast::Item::Trait(it) => SignatureKind::Trait {
                bounds: self.lower_bounds(it.type_bound_list()),
            },
            ast::Item::Mod(_) | ast::Item::Use(_) => SignatureKind::None,
        };
        Some(Signature {
            store: std::mem::take(&mut self.store),
            generics,
            kind,
        })
    }

    fn lower_generics(&mut self, list: Option<ast::GenericParamList>) -> Vec<GenericParam> {
        let Some(list) = list else {
            return Vec::new();
        };
        list.generic_params()
            .filter_map(|param| match param {
                ast::GenericParam::TypeParam(param) => {
                    let name = param.name()?;
                    Some(GenericParam {
                        name: name_text(&name),
                        range: range_of(&name),
                        kind: GenericParamKind::Type {
                            bounds: self.lower_bounds(param.type_bound_list()),
                        },
                    })
                }
                ast::GenericParam::ConstParam(param) => {
                    let name = param.name()?;
                    let range = range_of(&param);
                    Some(GenericParam {
                        name: name_text(&name),
                        range: range_of(&name),
                        kind: GenericParamKind::Const {
                            ty: self.lower_type_opt(param.ty(), range),
                        },
                    })
                }
            })
            .collect()
    }

    fn lower_bounds(&mut self, list: Option<ast::TypeBoundList>) -> Vec<TypeRefId> {
        let Some(list) = list else {
            return Vec::new();
        };
        list.bounds()
            .map(|bound| {
                let range = range_of(&bound);
                self.lower_type_opt(bound.ty(), range)
            })
            .collect()
    }

    fn lower_fields(
        &mut self,
        record: Option<ast::RecordFieldList>,
        tuple: Option<ast::TupleFieldList>,
    ) -> Vec<(Option<Name>, TypeRefId)> {
        if let Some(record) = record {
            record
                .fields()
                .map(|field| {
                    let range = range_of(&field);
                    (
                        field.name().map(|name| name_text(&name)),
                        self.lower_type_opt(field.ty(), range),
                    )
                })
                .collect()
        } else if let Some(tuple) = tuple {
            tuple
                .fields()
                .map(|field| {
                    let range = range_of(&field);
                    (None, self.lower_type_opt(field.ty(), range))
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    // ---- paths and types ----

    pub(super) fn lower_path(&mut self, path: &ast::Path) -> Option<Path> {
        lower_path(path, &mut |list| Some(self.lower_generic_args(list)))
    }

    fn lower_generic_args(&mut self, list: ast::GenericArgList) -> GenericArgs {
        GenericArgs(
            list.generic_args()
                .map(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => {
                        let range = range_of(&arg);
                        GenericArg::Type(self.lower_type_opt(arg.ty(), range))
                    }
                    ast::GenericArg::ConstArg(arg) => {
                        let range = range_of(&arg);
                        GenericArg::Const(self.lower_expr_opt(arg.expr(), range))
                    }
                })
                .collect(),
        )
    }

    fn lower_type_opt(&mut self, ty: Option<ast::Type>, range: TextRange) -> TypeRefId {
        match ty {
            Some(ty) => self.lower_type(ty),
            None => self.store.alloc_type(TypeRef::Missing, range),
        }
    }

    pub(super) fn lower_type(&mut self, ty: ast::Type) -> TypeRefId {
        let range = range_of(&ty);
        let lowered = match ty {
            ast::Type::PathType(it) => match it.path().and_then(|path| self.lower_path(&path)) {
                Some(path) => TypeRef::Path(path),
                None => TypeRef::Missing,
            },
            ast::Type::TupleType(it) => {
                TypeRef::Tuple(it.fields().map(|field| self.lower_type(field)).collect())
            }
            ast::Type::ParenType(it) => return self.lower_type_opt(it.ty(), range),
            ast::Type::ArrayType(it) => TypeRef::Array {
                elem: self.lower_type_opt(it.ty(), range),
                len: self.lower_expr_opt(it.len(), range),
            },
            ast::Type::RefType(it) => TypeRef::Ref {
                mutable: it.mut_token().is_some(),
                inner: self.lower_type_opt(it.ty(), range),
            },
            ast::Type::InferType(_) => TypeRef::Infer,
            ast::Type::FnPtrType(it) => {
                let (params, ret) = self.lower_fn_type_parts(it.param_list(), it.ret_type());
                TypeRef::FnPtr {
                    kind: match it.kind() {
                        ast::FnPtrKind::Plain => FnPtrKind::Plain,
                        ast::FnPtrKind::Const => FnPtrKind::Const,
                        ast::FnPtrKind::Pushdown => FnPtrKind::Pushdown,
                        ast::FnPtrKind::Scene => FnPtrKind::Scene,
                    },
                    params,
                    ret,
                }
            }
            ast::Type::ClosureType(it) => {
                let (params, ret) = self.lower_fn_type_parts(it.param_list(), it.ret_type());
                TypeRef::Closure {
                    kind: match it.kind() {
                        ast::ClosureKind::Plain => ClosureKind::Plain,
                        ast::ClosureKind::Const => ClosureKind::Const,
                        ast::ClosureKind::Pushdown => ClosureKind::Pushdown,
                    },
                    params,
                    ret,
                }
            }
        };
        self.store.alloc_type(lowered, range)
    }

    fn lower_fn_type_parts(
        &mut self,
        params: Option<ast::ParamList>,
        ret: Option<ast::RetType>,
    ) -> (Vec<TypeRefId>, Option<TypeRefId>) {
        let params = params
            .into_iter()
            .flat_map(|list| list.params())
            .map(|param| {
                let range = range_of(&param);
                self.lower_type_opt(param.ty(), range)
            })
            .collect();
        let ret = ret.map(|ret| {
            let range = range_of(&ret);
            self.lower_type_opt(ret.ty(), range)
        });
        (params, ret)
    }

    // ---- patterns ----

    pub(super) fn lower_pat_opt(&mut self, pat: Option<ast::Pat>, range: TextRange) -> PatId {
        match pat {
            Some(pat) => self.lower_pat(pat),
            None => self.store.alloc_pat(Pat::Missing, range),
        }
    }

    fn lower_pat(&mut self, pat: ast::Pat) -> PatId {
        let range = range_of(&pat);
        let lowered = match pat {
            ast::Pat::WildcardPat(_) => Pat::Wild,
            ast::Pat::IdentPat(it) => match it.name() {
                Some(name) => Pat::Bind {
                    name: name_text(&name),
                    mutable: it.mut_token().is_some(),
                },
                None => Pat::Missing,
            },
            ast::Pat::PathPat(it) => match it.path().and_then(|path| self.lower_path(&path)) {
                Some(path) => Pat::Path(path),
                None => Pat::Missing,
            },
            ast::Pat::LiteralPat(it) => match it.literal().and_then(|lit| lower_literal(&lit)) {
                Some(lit) => Pat::Lit {
                    lit,
                    negated: it.minus_token().is_some(),
                },
                None => Pat::Missing,
            },
            ast::Pat::RangePat(it) => Pat::Range {
                start: it.start().and_then(|pat| self.lower_pat_bound(pat)),
                end: it.end().and_then(|pat| self.lower_pat_bound(pat)),
                inclusive: it.is_inclusive(),
            },
            ast::Pat::TuplePat(it) => {
                Pat::Tuple(it.fields().map(|pat| self.lower_pat(pat)).collect())
            }
            ast::Pat::TupleStructPat(it) => {
                match it.path().and_then(|path| self.lower_path(&path)) {
                    Some(path) => Pat::TupleStruct {
                        path,
                        pats: it.fields().map(|pat| self.lower_pat(pat)).collect(),
                    },
                    None => Pat::Missing,
                }
            }
            ast::Pat::RecordPat(it) => match it.path().and_then(|path| self.lower_path(&path)) {
                Some(path) => {
                    let fields = it
                        .record_pat_field_list()
                        .into_iter()
                        .flat_map(|list| list.fields())
                        .filter_map(|field| {
                            let pat = field.pat();
                            let name = match field.name_ref() {
                                Some(name_ref) => Name::new(name_ref.text()),
                                None => match &pat {
                                    Some(ast::Pat::IdentPat(ident)) => name_text(&ident.name()?),
                                    _ => return None,
                                },
                            };
                            let range = range_of(&field);
                            Some((name, self.lower_pat_opt(pat, range)))
                        })
                        .collect();
                    Pat::Record { path, fields }
                }
                None => Pat::Missing,
            },
            ast::Pat::RestPat(_) => Pat::Rest,
            ast::Pat::OrPat(it) => Pat::Or(it.pats().map(|pat| self.lower_pat(pat)).collect()),
            ast::Pat::ParenPat(it) => return self.lower_pat_opt(it.pat(), range),
        };
        self.store.alloc_pat(lowered, range)
    }

    fn lower_pat_bound(&mut self, pat: ast::Pat) -> Option<PatBound> {
        match pat {
            ast::Pat::LiteralPat(it) => Some(PatBound::Lit {
                lit: lower_literal(&it.literal()?)?,
                negated: it.minus_token().is_some(),
            }),
            ast::Pat::PathPat(it) => Some(PatBound::Path(self.lower_path(&it.path()?)?)),
            ast::Pat::IdentPat(it) => {
                let name = it.name()?;
                Some(PatBound::Path(Path {
                    kind: PathKind::Plain,
                    segments: vec![PathSegment {
                        name: name_text(&name),
                        range: range_of(&name),
                        args: None,
                    }],
                }))
            }
            _ => None,
        }
    }

    // ---- expressions ----

    pub(super) fn lower_expr_opt(&mut self, expr: Option<ast::Expr>, range: TextRange) -> ExprId {
        match expr {
            Some(expr) => self.lower_expr(expr),
            None => self.missing(range),
        }
    }

    pub(super) fn lower_expr(&mut self, expr: ast::Expr) -> ExprId {
        let range = range_of(&expr);
        let lowered = match expr {
            ast::Expr::Literal(it) => match lower_literal(&it) {
                Some(lit) => Expr::Literal(lit),
                None => Expr::Missing,
            },
            ast::Expr::PathExpr(it) => match it.path().and_then(|path| self.lower_path(&path)) {
                Some(path) => Expr::Path(path),
                None => Expr::Missing,
            },
            ast::Expr::ParenExpr(it) => return self.lower_expr_opt(it.expr(), range),
            ast::Expr::BlockExpr(it) => self.lower_block(&it),
            ast::Expr::TupleExpr(it) => {
                Expr::Tuple(it.fields().map(|field| self.lower_expr(field)).collect())
            }
            ast::Expr::ArrayExpr(it) => match it.repeat() {
                Some((value, count)) => Expr::Array(ArrayExpr::Repeat {
                    value: self.lower_expr(value),
                    count: self.lower_expr(count),
                }),
                None if it.semicolon_token().is_some() => {
                    let mut exprs = it.exprs();
                    let value = self.lower_expr_opt(exprs.next(), range);
                    let count = self.lower_expr_opt(exprs.next(), range);
                    Expr::Array(ArrayExpr::Repeat { value, count })
                }
                None => Expr::Array(ArrayExpr::List(
                    it.exprs().map(|expr| self.lower_expr(expr)).collect(),
                )),
            },
            ast::Expr::RecordExpr(it) => match it.path().and_then(|path| self.lower_path(&path)) {
                Some(path) => {
                    let fields = it
                        .record_expr_field_list()
                        .into_iter()
                        .flat_map(|list| list.fields())
                        .filter_map(|field| {
                            let name_ref = field.name_ref()?;
                            let name = Name::new(name_ref.text());
                            let value = match field.expr() {
                                Some(expr) => self.lower_expr(expr),
                                None => {
                                    let range = range_of(&name_ref);
                                    self.alloc(
                                        Expr::Path(Path {
                                            kind: PathKind::Plain,
                                            segments: vec![PathSegment {
                                                name: name.clone(),
                                                range,
                                                args: None,
                                            }],
                                        }),
                                        range,
                                    )
                                }
                            };
                            Some((name, value))
                        })
                        .collect();
                    Expr::Record { path, fields }
                }
                None => Expr::Missing,
            },
            ast::Expr::IfExpr(it) => {
                let cond = self.lower_condition(it.condition(), range);
                let then_branch = match it.then_branch() {
                    Some(block) => self.lower_expr(ast::Expr::BlockExpr(block)),
                    None => self.missing(range),
                };
                let else_branch = it.else_branch().map(|branch| match branch {
                    ast::ElseBranch::Block(block) => self.lower_expr(ast::Expr::BlockExpr(block)),
                    ast::ElseBranch::If(expr) => self.lower_expr(ast::Expr::IfExpr(expr)),
                });
                Expr::If {
                    cond,
                    then_branch,
                    else_branch,
                }
            }
            ast::Expr::MatchExpr(it) => {
                let scrutinee = self.lower_expr_opt(it.expr(), range);
                let arms = it
                    .match_arm_list()
                    .into_iter()
                    .flat_map(|list| list.arms())
                    .map(|arm| {
                        let arm_range = range_of(&arm);
                        MatchArm {
                            pat: self.lower_pat_opt(arm.pat(), arm_range),
                            guard: arm.guard().map(|guard| {
                                let range = range_of(&guard);
                                self.lower_expr_opt(guard.condition(), range)
                            }),
                            body: self.lower_expr_opt(arm.expr(), arm_range),
                        }
                    })
                    .collect();
                Expr::Match { scrutinee, arms }
            }
            ast::Expr::LoopExpr(it) => Expr::Loop {
                label: it
                    .label_decl()
                    .and_then(|label| label.label_token())
                    .map(|token| label_name(&token)),
                body: self.lower_block_opt(it.loop_body(), range),
            },
            ast::Expr::WhileExpr(it) => Expr::While {
                label: it
                    .label_decl()
                    .and_then(|label| label.label_token())
                    .map(|token| label_name(&token)),
                cond: self.lower_condition(it.condition(), range),
                body: self.lower_block_opt(it.loop_body(), range),
            },
            ast::Expr::ForExpr(it) => Expr::For {
                label: it
                    .label_decl()
                    .and_then(|label| label.label_token())
                    .map(|token| label_name(&token)),
                pat: self.lower_pat_opt(it.pat(), range),
                iterable: self.lower_expr_opt(it.iterable(), range),
                body: self.lower_block_opt(it.loop_body(), range),
            },
            ast::Expr::BreakExpr(it) => Expr::Break {
                label: it.label_token().map(|token| label_name(&token)),
                value: it.expr().map(|expr| self.lower_expr(expr)),
            },
            ast::Expr::ContinueExpr(it) => Expr::Continue {
                label: it.label_token().map(|token| label_name(&token)),
            },
            ast::Expr::ReturnExpr(it) => Expr::Return {
                value: it.expr().map(|expr| self.lower_expr(expr)),
            },
            ast::Expr::CallExpr(it) => Expr::Call {
                callee: self.lower_expr_opt(it.expr(), range),
                args: self.lower_args(it.arg_list()),
            },
            ast::Expr::MethodCallExpr(it) => {
                let receiver = self.lower_expr_opt(it.receiver(), range);
                let Some(name_ref) = it.name_ref() else {
                    return self.missing(range);
                };
                Expr::MethodCall {
                    receiver,
                    name: Name::new(name_ref.text()),
                    args_generic: it
                        .generic_arg_list()
                        .map(|list| self.lower_generic_args(list)),
                    args: self.lower_args(it.arg_list()),
                }
            }
            ast::Expr::FieldExpr(it) => {
                let base = self.lower_expr_opt(it.base(), range);
                match it.field() {
                    Some(ast::FieldName::Name(name_ref)) => Expr::Field {
                        base,
                        name: FieldName::Named(Name::new(name_ref.text())),
                    },
                    Some(ast::FieldName::Index(index)) => Expr::Field {
                        base,
                        name: FieldName::Index(index),
                    },
                    None => Expr::Missing,
                }
            }
            ast::Expr::IndexExpr(it) => Expr::Index {
                base: self.lower_expr_opt(it.base(), range),
                index: self.lower_expr_opt(it.index(), range),
            },
            ast::Expr::TryExpr(it) => Expr::Try(self.lower_expr_opt(it.expr(), range)),
            ast::Expr::CastExpr(it) => Expr::Cast {
                expr: self.lower_expr_opt(it.expr(), range),
                ty: self.lower_type_opt(it.ty(), range),
            },
            ast::Expr::PrefixExpr(it) => {
                let expr = self.lower_expr_opt(it.expr(), range);
                match it.op_kind() {
                    Some(op) => Expr::Unary {
                        op: match op {
                            ast::UnaryOp::Neg => UnaryOp::Neg,
                            ast::UnaryOp::Not => UnaryOp::Not,
                            ast::UnaryOp::Deref => UnaryOp::Deref,
                        },
                        expr,
                    },
                    None => Expr::Missing,
                }
            }
            ast::Expr::RefExpr(it) => Expr::Ref {
                mutable: it.mut_token().is_some(),
                expr: self.lower_expr_opt(it.expr(), range),
            },
            ast::Expr::BinExpr(it) => {
                let lhs = self.lower_expr_opt(it.lhs(), range);
                let rhs = self.lower_expr_opt(it.rhs(), range);
                match it.op_kind() {
                    Some(ast::BinOp::Assign) => Expr::Assign {
                        op: None,
                        target: lhs,
                        value: rhs,
                    },
                    Some(ast::BinOp::CompoundAssign(op)) => Expr::Assign {
                        op: Some(arith_op(op)),
                        target: lhs,
                        value: rhs,
                    },
                    Some(ast::BinOp::Arith(op)) => Expr::Binary {
                        op: arith_op(op),
                        lhs,
                        rhs,
                    },
                    Some(ast::BinOp::Cmp(op)) => Expr::Binary {
                        op: match op {
                            ast::CmpOp::Eq => BinaryOp::Eq,
                            ast::CmpOp::Ne => BinaryOp::Ne,
                            ast::CmpOp::Lt => BinaryOp::Lt,
                            ast::CmpOp::Le => BinaryOp::Le,
                            ast::CmpOp::Gt => BinaryOp::Gt,
                            ast::CmpOp::Ge => BinaryOp::Ge,
                        },
                        lhs,
                        rhs,
                    },
                    Some(ast::BinOp::Logic(op)) => Expr::Binary {
                        op: match op {
                            ast::LogicOp::And => BinaryOp::And,
                            ast::LogicOp::Or => BinaryOp::Or,
                        },
                        lhs,
                        rhs,
                    },
                    None => Expr::Missing,
                }
            }
            ast::Expr::RangeExpr(it) => Expr::Range {
                start: it.start().map(|expr| self.lower_expr(expr)),
                end: it.end().map(|expr| self.lower_expr(expr)),
                inclusive: it.is_inclusive(),
            },
            ast::Expr::ClosureExpr(it) => {
                let params = it
                    .param_list()
                    .into_iter()
                    .flat_map(|list| list.params())
                    .map(|param| {
                        let range = range_of(&param);
                        let pat = self.lower_pat_opt(param.pat(), range);
                        let ty = param.ty().map(|ty| self.lower_type(ty));
                        (pat, ty)
                    })
                    .collect();
                let ret = it.ret_type().map(|ret| {
                    let range = range_of(&ret);
                    self.lower_type_opt(ret.ty(), range)
                });
                Expr::Closure {
                    params,
                    ret,
                    body: self.lower_expr_opt(it.body(), range),
                }
            }
            ast::Expr::FormExpr(it) => return self.lower_form(&it),
            ast::Expr::TextForm(it) => return self.lower_text_form(&it),
        };
        self.alloc(lowered, range)
    }

    fn lower_args(&mut self, list: Option<ast::ArgList>) -> Vec<ExprId> {
        list.into_iter()
            .flat_map(|list| list.args())
            .map(|arg| self.lower_expr(arg))
            .collect()
    }

    fn lower_condition(&mut self, cond: Option<ast::Condition>, range: TextRange) -> Condition {
        match cond {
            Some(ast::Condition::LetExpr(it)) => {
                let range = range_of(&it);
                Condition::Let {
                    pat: self.lower_pat_opt(it.pat(), range),
                    expr: self.lower_expr_opt(it.expr(), range),
                }
            }
            Some(ast::Condition::Expr(expr)) => Condition::Expr(self.lower_expr(expr)),
            None => Condition::Expr(self.missing(range)),
        }
    }

    fn lower_block_opt(&mut self, block: Option<ast::BlockExpr>, range: TextRange) -> ExprId {
        match block {
            Some(block) => self.lower_expr(ast::Expr::BlockExpr(block)),
            None => self.missing(range),
        }
    }

    fn lower_block(&mut self, block: &ast::BlockExpr) -> Expr {
        let mut stmts = Vec::new();
        for stmt in block.statements() {
            match stmt {
                ast::Stmt::LetStmt(it) => {
                    let range = range_of(&it);
                    let pat = self.lower_pat_opt(it.pat(), range);
                    let ty = it.ty().map(|ty| self.lower_type(ty));
                    let init = it.initializer().map(|expr| self.lower_expr(expr));
                    stmts.push(Stmt::Let { pat, ty, init });
                }
                ast::Stmt::ExprStmt(it) => {
                    let range = range_of(&it);
                    let expr = self.lower_expr_opt(it.expr(), range);
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semi: it.semicolon_token().is_some(),
                    });
                }
                ast::Stmt::Item(item) => {
                    let ptr = crate::item_tree::SyntaxPtr::new(item.syntax());
                    if let Some(&index) = self.tree.item_by_ptr.get(&ptr) {
                        stmts.push(Stmt::Item(ItemId {
                            file: self.file,
                            index,
                        }));
                    }
                }
            }
        }
        let tail = block.tail_expr().map(|expr| self.lower_expr(expr));
        let scope = self
            .tree
            .block_by_ptr
            .get(&crate::item_tree::SyntaxPtr::new(block.syntax()))
            .copied();
        Expr::Block { stmts, tail, scope }
    }
}

fn arith_op(op: ast::ArithOp) -> BinaryOp {
    match op {
        ast::ArithOp::Add => BinaryOp::Add,
        ast::ArithOp::Sub => BinaryOp::Sub,
        ast::ArithOp::Mul => BinaryOp::Mul,
        ast::ArithOp::Div => BinaryOp::Div,
        ast::ArithOp::Rem => BinaryOp::Rem,
        ast::ArithOp::Shl => BinaryOp::Shl,
        ast::ArithOp::Shr => BinaryOp::Shr,
        ast::ArithOp::BitAnd => BinaryOp::BitAnd,
        ast::ArithOp::BitOr => BinaryOp::BitOr,
        ast::ArithOp::BitXor => BinaryOp::BitXor,
    }
}

pub(super) fn lower_literal(lit: &ast::Literal) -> Option<Literal> {
    Some(match lit.kind()? {
        ast::LiteralKind::Int => {
            let (value, suffix) = lit.int_value()?;
            Literal::Int {
                value,
                suffix: suffix.map(|suffix| suffix.as_str().into()),
            }
        }
        ast::LiteralKind::Decimal => Literal::Decimal(lit.decimal_text()?.as_str().into()),
        ast::LiteralKind::Str => Literal::String(lit.string_value()?),
        ast::LiteralKind::Bool(value) => Literal::Bool(value),
    })
}
