//! The algorithm on synthetic types, patterns and inference results.

use std::sync::Arc;

use kagari_syntax::{TextRange, TextSize};
use rustc_hash::FxHashMap;

use super::*;
use crate::db::{MemoryFileSystem, RootDatabase};
use crate::hir::{ExprStore, Path, PathKind};
use crate::signatures::{FieldDef, Generics, VariantDef};
use crate::ty::{CtorRef, IntTy};

struct TestEnv {
    adts: FxHashMap<ItemId, AdtDef>,
    names: FxHashMap<ItemId, Name>,
    std: StdItems,
    consts: FxHashMap<ItemId, (Body, BodyResolution)>,
}

impl PatEnv for TestEnv {
    fn adt(&self, item: ItemId) -> Option<&AdtDef> {
        self.adts.get(&item)
    }

    fn std_items(&self) -> &StdItems {
        &self.std
    }

    fn item_name(&self, item: ItemId) -> Option<Name> {
        self.names.get(&item).cloned()
    }

    fn const_body(&self, item: ItemId) -> Option<ConstBody<'_>> {
        self.consts.get(&item).map(|(body, resolution)| ConstBody {
            item,
            body,
            resolution,
            inference: None,
        })
    }

    /// Integer literals, possibly negated (the real folder is exercised end to end).
    fn fold_int(&self, init: ConstBody<'_>, expr: ExprId) -> Option<i128> {
        match init.body.store.expr(expr) {
            Expr::Literal(Literal::Int { value, .. }) => int_value(*value, false),
            Expr::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } => self.fold_int(init, *inner)?.checked_neg(),
            _ => None,
        }
    }
}

type FieldSpec<'a> = (Option<&'a str>, Ty);

fn adt_def(
    is_enum: bool,
    builtin: bool,
    variants: Vec<(&str, Shape, Vec<FieldSpec<'_>>)>,
) -> AdtDef {
    AdtDef {
        generics: Generics::default(),
        is_enum,
        builtin,
        must_use: false,
        variants: variants
            .into_iter()
            .map(|(name, shape, fields)| VariantDef {
                name: Name::new(name),
                shape,
                fields: fields
                    .into_iter()
                    .map(|(name, ty)| FieldDef {
                        name: name.map(Name::new),
                        ty,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn record(names: &[&str]) -> Shape {
    Shape::Record(names.iter().map(Name::new).collect())
}

fn dummy_path() -> Path {
    Path {
        kind: PathKind::Plain,
        segments: Vec::new(),
    }
}

/// A synthetic program: ADTs and constants.
struct World {
    _db: RootDatabase,
    env: TestEnv,
    route: ItemId,
    option: ItemId,
    result: ItemId,
    point: ItemId,
    empty: ItemId,
    big: ItemId,
    shape: ItemId,
    text: ItemId,
    unit: ItemId,
    const_x: ItemId,
    low: ItemId,
    neg: ItemId,
    opaque: ItemId,
    wrapped: ItemId,
}

impl World {
    fn new() -> World {
        let db = RootDatabase::new(Arc::new(MemoryFileSystem::default()));
        let file = db.source_file(FilePath::Std("synthetic.kis".to_owned()));
        let id = |index: u32| ItemId { file, index };
        let (route, option, result, point, empty, big, shape, text, unit) = (
            id(0),
            id(1),
            id(2),
            id(3),
            id(4),
            id(5),
            id(6),
            id(7),
            id(8),
        );
        let (const_x, low, neg, opaque, wrapped, some_fn) =
            (id(9), id(10), id(11), id(12), id(13), id(14));
        let param = |owner: ItemId, index: u32| Ty::Param { owner, index };
        let mut adts = FxHashMap::default();
        adts.insert(
            route,
            adt_def(
                true,
                false,
                vec![
                    ("A", Shape::Unit, vec![]),
                    ("B", Shape::Unit, vec![]),
                    ("C", Shape::Unit, vec![]),
                ],
            ),
        );
        adts.insert(
            option,
            adt_def(
                true,
                false,
                vec![
                    ("None", Shape::Unit, vec![]),
                    ("Some", Shape::Tuple(1), vec![(None, param(option, 0))]),
                ],
            ),
        );
        adts.insert(
            result,
            adt_def(
                true,
                false,
                vec![
                    ("Ok", Shape::Tuple(1), vec![(None, param(result, 0))]),
                    ("Err", Shape::Tuple(1), vec![(None, param(result, 1))]),
                ],
            ),
        );
        adts.insert(
            point,
            adt_def(
                false,
                false,
                vec![(
                    "Point",
                    record(&["x", "y"]),
                    vec![
                        (Some("x"), Ty::Int(IntTy::U64)),
                        (Some("y"), Ty::Int(IntTy::U64)),
                    ],
                )],
            ),
        );
        adts.insert(empty, adt_def(true, false, vec![]));
        adts.insert(
            big,
            adt_def(
                true,
                false,
                ["A", "B", "C", "D", "E"]
                    .into_iter()
                    .map(|name| (name, Shape::Unit, vec![]))
                    .collect(),
            ),
        );
        adts.insert(
            shape,
            adt_def(
                true,
                false,
                vec![
                    (
                        "Circle",
                        record(&["r"]),
                        vec![(Some("r"), Ty::Int(IntTy::U8))],
                    ),
                    (
                        "Rect",
                        record(&["w", "h"]),
                        vec![(Some("w"), Ty::Bool), (Some("h"), Ty::Bool)],
                    ),
                    ("Dot", Shape::Unit, vec![]),
                ],
            ),
        );
        adts.insert(text, adt_def(false, true, vec![]));
        adts.insert(
            unit,
            adt_def(false, false, vec![("Unit", Shape::Unit, vec![])]),
        );

        let mut names = FxHashMap::default();
        for (item, name) in [
            (route, "Route"),
            (option, "Option"),
            (result, "Result"),
            (point, "Point"),
            (empty, "Empty"),
            (big, "Big"),
            (shape, "Shape"),
            (text, "Text"),
            (unit, "Unit"),
            (const_x, "CONST_X"),
            (low, "LOW"),
            (neg, "NEG"),
            (opaque, "OPAQUE"),
            (wrapped, "WRAPPED"),
        ] {
            names.insert(item, Name::new(name));
        }

        let mut consts = FxHashMap::default();
        // const CONST_X: Route = Route::C;
        consts.insert(
            const_x,
            const_body(|store, res| {
                path_expr(
                    store,
                    res,
                    Resolution::Def(Def::Variant {
                        enum_item: route,
                        index: 2,
                    }),
                )
            }),
        );
        // const LOW: u64 = 1;
        consts.insert(low, const_body(|store, _| int_expr(store, 1)));
        // const NEG: i32 = -3;
        consts.insert(
            neg,
            const_body(|store, _| {
                let three = int_expr(store, 3);
                store.alloc_expr(
                    Expr::Unary {
                        op: UnaryOp::Neg,
                        expr: three,
                    },
                    TextRange::default(),
                )
            }),
        );
        // const OPAQUE: Route = some_fn();
        consts.insert(
            opaque,
            const_body(|store, res| {
                let callee = path_expr(store, res, Resolution::Def(Def::Item(some_fn)));
                store.alloc_expr(
                    Expr::Call {
                        callee,
                        args: vec![],
                    },
                    TextRange::default(),
                )
            }),
        );
        // const WRAPPED: Option<Route> = Some(Route::A);
        consts.insert(
            wrapped,
            const_body(|store, res| {
                let callee = path_expr(
                    store,
                    res,
                    Resolution::Def(Def::Variant {
                        enum_item: option,
                        index: 1,
                    }),
                );
                let arg = path_expr(
                    store,
                    res,
                    Resolution::Def(Def::Variant {
                        enum_item: route,
                        index: 0,
                    }),
                );
                store.alloc_expr(
                    Expr::Call {
                        callee,
                        args: vec![arg],
                    },
                    TextRange::default(),
                )
            }),
        );

        World {
            env: TestEnv {
                adts,
                names,
                std: StdItems {
                    option: Some(option),
                    result: Some(result),
                    ..StdItems::default()
                },
                consts,
            },
            _db: db,
            route,
            option,
            result,
            point,
            empty,
            big,
            shape,
            text,
            unit,
            const_x,
            low,
            neg,
            opaque,
            wrapped,
        }
    }

    fn adt(&self, item: ItemId) -> Ty {
        Ty::Adt {
            item,
            args: Vec::new(),
        }
    }

    fn option_of(&self, ty: Ty) -> Ty {
        Ty::Adt {
            item: self.option,
            args: vec![GenericArg::Type(ty)],
        }
    }

    fn result_of(&self, ok: Ty, err: Ty) -> Ty {
        Ty::Adt {
            item: self.result,
            args: vec![GenericArg::Type(ok), GenericArg::Type(err)],
        }
    }
}

fn const_body(
    build: impl FnOnce(&mut ExprStore, &mut BodyResolution) -> ExprId,
) -> (Body, BodyResolution) {
    let mut store = ExprStore::default();
    let mut resolution = BodyResolution::default();
    let value = build(&mut store, &mut resolution);
    (
        Body {
            store,
            params: Vec::new(),
            has_self: false,
            self_mutable: false,
            value,
        },
        resolution,
    )
}

fn path_expr(store: &mut ExprStore, res: &mut BodyResolution, resolution: Resolution) -> ExprId {
    let expr = store.alloc_expr(Expr::Path(dummy_path()), TextRange::default());
    res.store.exprs.insert(expr, resolution);
    expr
}

fn int_expr(store: &mut ExprStore, value: u128) -> ExprId {
    store.alloc_expr(
        Expr::Literal(Literal::Int {
            value,
            suffix: None,
        }),
        TextRange::default(),
    )
}

/// A body under construction, with its resolution and inference result.
#[derive(Default)]
struct B {
    store: ExprStore,
    res: BodyResolution,
    inf: InferenceResult,
    params: Vec<PatId>,
    next: u32,
}

impl B {
    fn range(&mut self) -> TextRange {
        let start = TextSize::from(self.next);
        self.next = self.next.saturating_add(1);
        TextRange::new(start, start)
    }

    fn pat(&mut self, pat: Pat) -> PatId {
        let range = self.range();
        self.store.alloc_pat(pat, range)
    }

    fn expr(&mut self, expr: Expr) -> ExprId {
        let range = self.range();
        self.store.alloc_expr(expr, range)
    }

    fn wild(&mut self) -> PatId {
        self.pat(Pat::Wild)
    }

    fn bind(&mut self, name: &str) -> PatId {
        self.pat(Pat::Bind {
            name: Name::new(name),
            mutable: false,
        })
    }

    fn boolean(&mut self, value: bool) -> PatId {
        self.pat(Pat::Lit {
            lit: Literal::Bool(value),
            negated: false,
        })
    }

    fn int(&mut self, value: i128) -> PatId {
        self.pat(Pat::Lit {
            lit: Literal::Int {
                value: value.unsigned_abs(),
                suffix: None,
            },
            negated: value < 0,
        })
    }

    fn text(&mut self, value: &str) -> PatId {
        self.pat(Pat::Lit {
            lit: Literal::String(value.to_owned()),
            negated: false,
        })
    }

    fn range_pat(&mut self, lo: Option<i128>, hi: Option<i128>, inclusive: bool) -> PatId {
        let bound = |value: i128| PatBound::Lit {
            lit: Literal::Int {
                value: value.unsigned_abs(),
                suffix: None,
            },
            negated: value < 0,
        };
        self.pat(Pat::Range {
            start: lo.map(bound),
            end: hi.map(bound),
            inclusive,
        })
    }

    /// `CONST..=hi`.
    fn const_range(&mut self, item: ItemId, hi: i128) -> PatId {
        let pat = self.pat(Pat::Range {
            start: Some(PatBound::Path(dummy_path())),
            end: Some(PatBound::Lit {
                lit: Literal::Int {
                    value: hi.unsigned_abs(),
                    suffix: None,
                },
                negated: hi < 0,
            }),
            inclusive: true,
        });
        self.res
            .store
            .range_bounds
            .insert((pat, 0), Resolution::Def(Def::Item(item)));
        pat
    }

    fn tuple(&mut self, pats: Vec<PatId>) -> PatId {
        self.pat(Pat::Tuple(pats))
    }

    fn rest(&mut self) -> PatId {
        self.pat(Pat::Rest)
    }

    fn or(&mut self, pats: Vec<PatId>) -> PatId {
        self.pat(Pat::Or(pats))
    }

    /// `Variant(pats)` (or `Variant` when `pats` is `None`) as typed by inference.
    fn ctor(
        &mut self,
        adt: ItemId,
        variant: Option<u32>,
        args: Vec<GenericArg>,
        pats: Option<Vec<PatId>>,
    ) -> PatId {
        let pat = match pats {
            Some(pats) => self.pat(Pat::TupleStruct {
                path: dummy_path(),
                pats,
            }),
            None => self.pat(Pat::Path(dummy_path())),
        };
        self.inf
            .pat_ctors
            .insert(pat, CtorRef { adt, variant, args });
        pat
    }

    fn unit_variant(&mut self, adt: ItemId, index: u32) -> PatId {
        self.ctor(adt, Some(index), Vec::new(), None)
    }

    fn some(&mut self, w: &World, inner_ty: Ty, inner: PatId) -> PatId {
        self.ctor(
            w.option,
            Some(1),
            vec![GenericArg::Type(inner_ty)],
            Some(vec![inner]),
        )
    }

    fn none(&mut self, w: &World, inner_ty: Ty) -> PatId {
        self.ctor(w.option, Some(0), vec![GenericArg::Type(inner_ty)], None)
    }

    fn record_pat(
        &mut self,
        adt: ItemId,
        variant: Option<u32>,
        fields: Vec<(&str, PatId)>,
    ) -> PatId {
        let pat = self.pat(Pat::Record {
            path: dummy_path(),
            fields: fields
                .into_iter()
                .map(|(name, pat)| (Name::new(name), pat))
                .collect(),
        });
        self.inf.pat_ctors.insert(
            pat,
            CtorRef {
                adt,
                variant,
                args: Vec::new(),
            },
        );
        pat
    }

    /// An identifier pattern resolved to a constant.
    fn const_pat(&mut self, item: ItemId) -> PatId {
        let pat = self.bind("CONST");
        self.res
            .store
            .pats
            .insert(pat, Resolution::Def(Def::Item(item)));
        pat
    }

    fn match_on(&mut self, ty: Ty, arms: Vec<(PatId, bool)>) -> ExprId {
        let scrutinee = self.expr(Expr::Path(dummy_path()));
        self.inf.expr_types.insert(scrutinee, ty);
        let arms = arms
            .into_iter()
            .map(|(pat, guarded)| {
                let guard = guarded.then(|| self.expr(Expr::Literal(Literal::Bool(true))));
                let body = self.expr(Expr::Tuple(Vec::new()));
                MatchArm { pat, guard, body }
            })
            .collect();
        self.expr(Expr::Match { scrutinee, arms })
    }

    fn let_stmt(&mut self, pat: PatId, ty: Ty) -> ExprId {
        self.inf.pat_types.insert(pat, ty);
        self.expr(Expr::Block {
            stmts: vec![Stmt::Let {
                pat,
                ty: None,
                init: None,
            }],
            tail: None,
            scope: None,
        })
    }

    fn check(&self, w: &World, value: ExprId) -> Vec<Diagnostic> {
        let body = Body {
            store: self.store.clone(),
            params: self.params.clone(),
            has_self: false,
            self_mutable: false,
            value,
        };
        check_body_in(
            &w.env,
            FilePath::Std("test.kis".to_owned()),
            &body,
            &self.res,
            &self.inf,
        )
    }

    /// `code: message` lines; unreachable patterns name the arm by index in `arms`.
    fn render(&self, diags: &[Diagnostic], arms: &[PatId]) -> Vec<String> {
        diags
            .iter()
            .map(|diag| {
                let at = arms
                    .iter()
                    .position(|&pat| self.store.pat_range(pat) == diag.range)
                    .map(|index| format!(" (arm {index})"))
                    .unwrap_or_default();
                format!("{}: {}{at}", diag.code.as_str(), diag.message)
            })
            .collect()
    }
}

/// Checks `match` on `ty` with `arms` (pattern, guarded).
fn check_match(w: &World, b: &mut B, ty: Ty, arms: Vec<(PatId, bool)>) -> Vec<String> {
    let pats: Vec<PatId> = arms.iter().map(|(pat, _)| *pat).collect();
    let value = b.match_on(ty, arms);
    let diags = b.check(w, value);
    b.render(&diags, &pats)
}

fn unguarded(pats: Vec<PatId>) -> Vec<(PatId, bool)> {
    pats.into_iter().map(|pat| (pat, false)).collect()
}

const NON_EXHAUSTIVE: &str = "type/non-exhaustive-match: non-exhaustive patterns:";
const UNREACHABLE: &str = "type/unreachable-pattern: unreachable pattern";

fn missing(list: &str) -> String {
    format!("{NON_EXHAUSTIVE} {list} not covered")
}

fn unreachable(arm: usize) -> String {
    format!("{UNREACHABLE} (arm {arm})")
}

#[test]
fn bool_matches() {
    let w = World::new();
    let mut b = B::default();
    let arms = vec![b.boolean(true)];
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, unguarded(arms)),
        vec![missing("`false`")]
    );

    let mut b = B::default();
    let arms = vec![b.boolean(true), b.boolean(false)];
    assert!(check_match(&w, &mut b, Ty::Bool, unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.boolean(true), b.boolean(false), b.wild()];
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, unguarded(arms)),
        vec![unreachable(2)]
    );

    let mut b = B::default();
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, Vec::new()),
        vec![missing("`false` and `true`")]
    );
}

#[test]
fn enum_with_payloads() {
    let w = World::new();
    let route = w.adt(w.route);
    let opt = w.option_of(route.clone());

    let mut b = B::default();
    let a = b.unit_variant(w.route, 0);
    let arms = vec![b.some(&w, route.clone(), a), b.none(&w, route.clone())];
    assert_eq!(
        check_match(&w, &mut b, opt.clone(), unguarded(arms)),
        vec![missing("`Some(Route::B)` and `Some(Route::C)`")]
    );

    let mut b = B::default();
    assert_eq!(
        check_match(&w, &mut b, opt.clone(), Vec::new()),
        vec![missing("`None` and `Some(_)`")]
    );

    let mut b = B::default();
    let x = b.bind("x");
    let arms = vec![b.some(&w, route.clone(), x), b.none(&w, route.clone())];
    assert!(check_match(&w, &mut b, opt, unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.unit_variant(w.route, 0)];
    assert_eq!(
        check_match(&w, &mut b, route, unguarded(arms)),
        vec![missing("`Route::B` and `Route::C`")]
    );
}

#[test]
fn nested_tuples() {
    let w = World::new();
    let ty = Ty::Tuple(vec![Ty::Tuple(vec![Ty::Bool, Ty::Bool]), Ty::Bool]);
    let mut b = B::default();
    let (t1, w1) = (b.boolean(true), b.wild());
    let inner1 = b.tuple(vec![t1, w1]);
    let w2 = b.wild();
    let arm1 = b.tuple(vec![inner1, w2]);
    let (w3, t3) = (b.wild(), b.boolean(true));
    let inner2 = b.tuple(vec![w3, t3]);
    let t4 = b.boolean(true);
    let arm2 = b.tuple(vec![inner2, t4]);
    assert_eq!(
        check_match(&w, &mut b, ty, unguarded(vec![arm1, arm2])),
        vec![missing(
            "`((false, false), _)` and `((false, true), false)`"
        )]
    );

    // `..` in tuples.
    let ty = Ty::Tuple(vec![Ty::Bool, Ty::Bool, Ty::Bool]);
    let mut b = B::default();
    let (t, r) = (b.boolean(true), b.rest());
    let arm1 = b.tuple(vec![t, r]);
    let (f, r, t) = (b.boolean(false), b.rest(), b.boolean(true));
    let arm2 = b.tuple(vec![f, r, t]);
    assert_eq!(
        check_match(&w, &mut b, ty, unguarded(vec![arm1, arm2])),
        vec![missing("`(false, _, false)`")]
    );

    // A one-element tuple.
    let mut b = B::default();
    let t = b.boolean(true);
    let arm = b.tuple(vec![t]);
    assert_eq!(
        check_match(&w, &mut b, Ty::Tuple(vec![Ty::Bool]), unguarded(vec![arm])),
        vec![missing("`(false,)`")]
    );
}

#[test]
fn bounded_int_ranges() {
    let w = World::new();
    let mut b = B::default();
    let arms = vec![
        b.range_pat(Some(0), Some(3), true),
        b.range_pat(Some(4), Some(5), true),
    ];
    assert!(check_match(&w, &mut b, Ty::bounded(0, 5), unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.range_pat(Some(0), Some(3), true)];
    assert_eq!(
        check_match(&w, &mut b, Ty::bounded(0, 5), unguarded(arms)),
        vec![missing("`4..=5`")]
    );

    let mut b = B::default();
    let arms = vec![b.range_pat(Some(0), Some(5), true)];
    assert_eq!(
        check_match(&w, &mut b, Ty::bounded(0, 10), unguarded(arms)),
        vec![missing("`6..=10`")]
    );

    let mut b = B::default();
    let arms = vec![
        b.range_pat(Some(4), Some(5), true),
        b.int(4),
        b.range_pat(Some(0), Some(4), false),
    ];
    assert_eq!(
        check_match(&w, &mut b, Ty::bounded(0, 5), unguarded(arms)),
        vec![unreachable(1)]
    );
}

#[test]
fn rust_int_ranges_with_gaps() {
    let w = World::new();
    let i32_ty = Ty::Int(IntTy::I32);
    let mut b = B::default();
    let arms = vec![
        b.range_pat(None, Some(-1), true),
        b.range_pat(Some(1), Some(5), true),
        b.range_pat(Some(10), None, true),
    ];
    assert_eq!(
        check_match(&w, &mut b, i32_ty.clone(), unguarded(arms)),
        vec![missing("`0` and `6..=9`")]
    );

    let mut b = B::default();
    let arms = vec![b.range_pat(None, Some(5), true)];
    assert_eq!(
        check_match(&w, &mut b, i32_ty.clone(), unguarded(arms)),
        vec![missing("`6..=i32::MAX`")]
    );

    let mut b = B::default();
    assert_eq!(
        check_match(&w, &mut b, i32_ty, Vec::new()),
        vec![missing("`i32::MIN..=i32::MAX`")]
    );

    let mut b = B::default();
    let arms = vec![
        b.range_pat(Some(0), Some(100), true),
        b.range_pat(Some(200), Some(255), true),
    ];
    assert_eq!(
        check_match(&w, &mut b, Ty::Int(IntTy::U8), unguarded(arms)),
        vec![missing("`101..=199`")]
    );

    let mut b = B::default();
    let arms = vec![
        b.range_pat(Some(0), Some(10), false),
        b.range_pat(Some(10), None, true),
    ];
    assert!(check_match(&w, &mut b, Ty::Int(IntTy::U8), unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![
        b.range_pat(Some(-128), Some(-1), true),
        b.int(0),
        b.range_pat(Some(1), None, true),
    ];
    assert!(check_match(&w, &mut b, Ty::Int(IntTy::I8), unguarded(arms)).is_empty());

    // Through a reference.
    let mut b = B::default();
    let arms = vec![b.boolean(true)];
    let ty = Ty::Ref {
        mutable: false,
        inner: Box::new(Ty::Bool),
    };
    assert_eq!(
        check_match(&w, &mut b, ty, unguarded(arms)),
        vec![missing("`false`")]
    );
}

#[test]
fn guards() {
    let w = World::new();
    let mut b = B::default();
    let arms = vec![(b.boolean(true), true), (b.boolean(false), false)];
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, arms),
        vec![missing("`true`")]
    );

    let mut b = B::default();
    let arms = vec![(b.wild(), false), (b.boolean(true), true)];
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, arms),
        vec![unreachable(1)]
    );

    let mut b = B::default();
    let arms = vec![
        (b.boolean(true), true),
        (b.boolean(true), false),
        (b.boolean(false), false),
    ];
    assert!(check_match(&w, &mut b, Ty::Bool, arms).is_empty());
}

#[test]
fn or_patterns() {
    let w = World::new();
    let route = w.adt(w.route);
    let mut b = B::default();
    let (a, bb) = (b.unit_variant(w.route, 0), b.unit_variant(w.route, 1));
    let arms = vec![b.or(vec![a, bb]), b.unit_variant(w.route, 2)];
    assert!(check_match(&w, &mut b, route.clone(), unguarded(arms)).is_empty());

    let mut b = B::default();
    let first = b.unit_variant(w.route, 0);
    let (a, bb) = (b.unit_variant(w.route, 0), b.unit_variant(w.route, 1));
    let second = b.or(vec![a, bb]);
    let arms = vec![first, second, b.unit_variant(w.route, 2)];
    assert!(check_match(&w, &mut b, route.clone(), unguarded(arms)).is_empty());

    let mut b = B::default();
    let (a, bb) = (b.unit_variant(w.route, 0), b.unit_variant(w.route, 1));
    let first = b.or(vec![a, bb]);
    let (bb, a) = (b.unit_variant(w.route, 1), b.unit_variant(w.route, 0));
    let second = b.or(vec![bb, a]);
    let arms = vec![first, second, b.unit_variant(w.route, 2)];
    assert_eq!(
        check_match(&w, &mut b, route, unguarded(arms)),
        vec![unreachable(1)]
    );

    // Nested alternatives.
    let mut b = B::default();
    let (t, f) = (b.boolean(true), b.boolean(false));
    let either = b.or(vec![t, f]);
    let t = b.boolean(true);
    let arm = b.tuple(vec![either, t]);
    assert_eq!(
        check_match(
            &w,
            &mut b,
            Ty::Tuple(vec![Ty::Bool, Ty::Bool]),
            unguarded(vec![arm])
        ),
        vec![missing("`(false, false)` and `(true, false)`")]
    );
}

#[test]
fn unreachable_arms() {
    let w = World::new();
    let mut b = B::default();
    let arms = vec![b.wild(), b.unit_variant(w.route, 0)];
    assert_eq!(
        check_match(&w, &mut b, w.adt(w.route), unguarded(arms)),
        vec![unreachable(1)]
    );

    let mut b = B::default();
    let arms = vec![b.bind("x"), b.bind("y")];
    assert_eq!(
        check_match(&w, &mut b, Ty::Bool, unguarded(arms)),
        vec![unreachable(1)]
    );
}

#[test]
fn refutable_bindings() {
    let w = World::new();
    let route = w.adt(w.route);
    let mut b = B::default();
    let x = b.bind("x");
    let pat = b.some(&w, route.clone(), x);
    let value = b.let_stmt(pat, w.option_of(route.clone()));
    assert_eq!(
        b.render(&b.check(&w, value), &[]),
        vec!["type/refutable-pattern: refutable pattern in local binding: `None` not covered"]
    );

    let mut b = B::default();
    let (x, y) = (b.bind("x"), b.bind("y"));
    let pat = b.tuple(vec![x, y]);
    let value = b.let_stmt(pat, Ty::Tuple(vec![Ty::Bool, Ty::Bool]));
    assert!(b.check(&w, value).is_empty());

    let mut b = B::default();
    let x = b.bind("x");
    let pat = b.record_pat(w.point, None, vec![("x", x)]);
    let value = b.let_stmt(pat, w.adt(w.point));
    assert!(b.check(&w, value).is_empty());

    let mut b = B::default();
    let param = b.unit_variant(w.route, 0);
    b.inf.pat_types.insert(param, route.clone());
    b.params.push(param);
    let value = b.expr(Expr::Tuple(Vec::new()));
    assert_eq!(
        b.render(&b.check(&w, value), &[]),
        vec![
            "type/refutable-pattern: refutable pattern in function argument: `Route::B` and \
             `Route::C` not covered"
        ]
    );

    let mut b = B::default();
    let pat = b.int(3);
    b.inf.pat_types.insert(pat, Ty::Int(IntTy::U8));
    let iterable = b.expr(Expr::Missing);
    let body = b.expr(Expr::Tuple(Vec::new()));
    let value = b.expr(Expr::For {
        label: None,
        pat,
        iterable,
        body,
    });
    assert_eq!(
        b.render(&b.check(&w, value), &[]),
        vec![
            "type/refutable-pattern: refutable pattern in `for` loop binding: `0..=2` and \
             `4..=u8::MAX` not covered"
        ]
    );
}

#[test]
fn empty_types() {
    let w = World::new();
    let mut b = B::default();
    assert!(check_match(&w, &mut b, w.adt(w.empty), Vec::new()).is_empty());

    let mut b = B::default();
    assert!(check_match(&w, &mut b, Ty::Never, Vec::new()).is_empty());

    let route = w.adt(w.route);
    let mut b = B::default();
    let x = b.wild();
    let ok = b.ctor(
        w.result,
        Some(0),
        vec![
            GenericArg::Type(route.clone()),
            GenericArg::Type(w.adt(w.empty)),
        ],
        Some(vec![x]),
    );
    assert!(
        check_match(
            &w,
            &mut b,
            w.result_of(route.clone(), w.adt(w.empty)),
            unguarded(vec![ok])
        )
        .is_empty()
    );

    let mut b = B::default();
    let x = b.wild();
    let ok = b.ctor(w.result, Some(0), Vec::new(), Some(vec![x]));
    assert_eq!(
        check_match(
            &w,
            &mut b,
            w.result_of(route.clone(), route),
            unguarded(vec![ok])
        ),
        vec![missing("`Err(_)`")]
    );
}

#[test]
fn opaque_text_literals() {
    let w = World::new();
    let text = w.adt(w.text);
    let mut b = B::default();
    let arms = vec![b.text("a"), b.text("b")];
    assert_eq!(
        check_match(&w, &mut b, text.clone(), unguarded(arms)),
        vec![missing("`_`")]
    );

    let mut b = B::default();
    let arms = vec![b.text("a"), b.text("a"), b.wild()];
    assert_eq!(
        check_match(&w, &mut b, text.clone(), unguarded(arms)),
        vec![unreachable(1)]
    );

    let mut b = B::default();
    let arms = vec![b.text("a"), b.bind("other")];
    assert!(check_match(&w, &mut b, text.clone(), unguarded(arms)).is_empty());

    let mut b = B::default();
    let pat = b.text("a");
    let value = b.let_stmt(pat, text);
    assert_eq!(
        b.render(&b.check(&w, value), &[]),
        vec!["type/refutable-pattern: refutable pattern in local binding: `_` not covered"]
    );
}

#[test]
fn witness_formatting() {
    let w = World::new();
    let mut b = B::default();
    let arms = vec![b.unit_variant(w.big, 0)];
    assert_eq!(
        check_match(&w, &mut b, w.adt(w.big), unguarded(arms)),
        vec![missing("`Big::B`, `Big::C`, `Big::D` and 1 more")]
    );

    let shape = w.adt(w.shape);
    let mut b = B::default();
    let arms = vec![b.unit_variant(w.shape, 2)];
    assert_eq!(
        check_match(&w, &mut b, shape.clone(), unguarded(arms)),
        vec![missing("`Shape::Circle { .. }` and `Shape::Rect { .. }`")]
    );

    let mut b = B::default();
    let t = b.boolean(true);
    let rect = b.record_pat(w.shape, Some(1), vec![("w", t)]);
    let circle = b.record_pat(w.shape, Some(0), vec![]);
    let arms = vec![rect, circle, b.unit_variant(w.shape, 2)];
    assert_eq!(
        check_match(&w, &mut b, shape, unguarded(arms)),
        vec![missing("`Shape::Rect { w: false, .. }`")]
    );

    let mut b = B::default();
    let zero = b.int(0);
    let arms = vec![b.record_pat(w.point, None, vec![("x", zero)])];
    assert_eq!(
        check_match(&w, &mut b, w.adt(w.point), unguarded(arms)),
        vec![missing("`Point { x: 1..=u64::MAX, .. }`")]
    );

    let mut b = B::default();
    assert_eq!(
        check_match(&w, &mut b, w.adt(w.unit), Vec::new()),
        vec![missing("`Unit`")]
    );

    let mut b = B::default();
    let arms = vec![b.ctor(w.unit, None, Vec::new(), None)];
    assert!(check_match(&w, &mut b, w.adt(w.unit), unguarded(arms)).is_empty());

    assert_eq!(witness_list(&["a".to_owned()]), "`a`");
    assert_eq!(
        witness_list(&["a".to_owned(), "b".to_owned()]),
        "`a` and `b`"
    );
    assert_eq!(
        witness_list(&["a".to_owned(), "b".to_owned(), "c".to_owned()]),
        "`a`, `b` and `c`"
    );
}

#[test]
fn constant_patterns() {
    let w = World::new();
    let route = w.adt(w.route);
    let mut b = B::default();
    let (a, bb) = (b.unit_variant(w.route, 0), b.unit_variant(w.route, 1));
    let arms = vec![b.or(vec![a, bb]), b.const_pat(w.const_x)];
    assert!(check_match(&w, &mut b, route.clone(), unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.const_pat(w.const_x), b.unit_variant(w.route, 2), b.wild()];
    assert_eq!(
        check_match(&w, &mut b, route.clone(), unguarded(arms)),
        vec![unreachable(1)]
    );

    let mut b = B::default();
    let arms = vec![b.const_range(w.low, 9)];
    assert_eq!(
        check_match(&w, &mut b, Ty::Int(IntTy::U64), unguarded(arms)),
        vec![missing("`0` and `10..=u64::MAX`")]
    );

    let mut b = B::default();
    let arms = vec![b.const_pat(w.neg)];
    assert_eq!(
        check_match(&w, &mut b, Ty::Int(IntTy::I32), unguarded(arms)),
        vec![missing("`i32::MIN..=-4` and `-2..=i32::MAX`")]
    );

    // An opaque constant covers only itself.
    let mut b = B::default();
    let arms = vec![
        b.const_pat(w.opaque),
        b.unit_variant(w.route, 0),
        b.unit_variant(w.route, 1),
        b.unit_variant(w.route, 2),
    ];
    assert!(check_match(&w, &mut b, route.clone(), unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.const_pat(w.opaque), b.const_pat(w.opaque)];
    assert_eq!(
        check_match(&w, &mut b, route.clone(), unguarded(arms)),
        vec![
            unreachable(1),
            missing("`Route::A`, `Route::B` and `Route::C`"),
        ]
    );

    // `const WRAPPED: Option<Route> = Some(Route::A);`
    let mut b = B::default();
    let (bb, c) = (b.unit_variant(w.route, 1), b.unit_variant(w.route, 2));
    let either = b.or(vec![bb, c]);
    let arms = vec![
        b.const_pat(w.wrapped),
        b.some(&w, route.clone(), either),
        b.none(&w, route.clone()),
    ];
    assert!(check_match(&w, &mut b, w.option_of(route), unguarded(arms)).is_empty());
}

#[test]
fn errors_skip_silently() {
    let w = World::new();
    let route = w.adt(w.route);
    let mut b = B::default();
    let arm = b.unit_variant(w.route, 0);
    b.inf.pat_types.insert(arm, Ty::Error);
    assert!(check_match(&w, &mut b, route, unguarded(vec![arm])).is_empty());

    let mut b = B::default();
    let arms = vec![b.boolean(true)];
    assert!(check_match(&w, &mut b, Ty::Error, unguarded(arms)).is_empty());

    let mut b = B::default();
    let arms = vec![b.int(7)];
    assert!(check_match(&w, &mut b, Ty::bounded(0, 5), unguarded(arms)).is_empty());

    let mut b = B::default();
    let missing_pat = b.pat(Pat::Missing);
    let arms = vec![b.boolean(true), missing_pat];
    assert!(check_match(&w, &mut b, Ty::Bool, unguarded(arms)).is_empty());
}

/// The checks on real inference, plus a constant folded by the shared folder.
#[test]
fn end_to_end() {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(
        "/game/main.kix".into(),
        "enum Route { A, B, C }\n\
         const LOW: u64 = 1 + 0;\n\
         fn pick(r: Route, n: u64) -> u64 {\n\
             let x = match n {\n\
                 LOW..=9 => 0,\n\
             };\n\
             match r {\n\
                 Route::A => x,\n\
             }\n\
         }\n"
        .to_owned(),
    );
    let db = RootDatabase::new(Arc::new(fs));
    let program = db.program(FilePath::disk("/game/main.kix"));
    let messages: Vec<&str> = file_pattern_checks(&db, program, *program.root(&db))
        .iter()
        .map(|diag| diag.message.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "non-exhaustive patterns: `0` and `10..=u64::MAX` not covered",
            "non-exhaustive patterns: `Route::B` and `Route::C` not covered",
        ]
    );
    let all = crate::check(&db, program);
    assert_eq!(all.len(), 2, "{all:?}");
}
