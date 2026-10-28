#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use kagari_num::{Decimal, Rat};
use kagari_syntax::TextRange;
use rustc_hash::FxHashMap;

use super::ctx::Ctx;
use super::domain::DomainCtx;
use super::machine::anim;
use super::machine::builtins::{check_marks, to_text};
use super::machine::screen::{
    ChannelId, EntityProp, LayerTarget, PendingChange, PropTarget, Scheduled, ScheduledKind,
    Screen, Target,
};
use super::machine::value::Value;
use super::machine::{CallSite, EvalError, Machine, Place};
use super::num;
use super::std_items::{Builtin, StdTable};
use super::value::{AssetKind, ConstValue, Text};
use super::*;
use crate::db::{MemoryFileSystem, RootDatabase};
use crate::def_map::{Def, Resolution};
use crate::hir::{Expr, Literal, Pat, body_of, body_source_map_of};
use crate::infer::{InferenceResult, ValueRef};
use crate::input::FilePath;
use crate::item_tree::Shape;
use crate::pipeline::check;
use crate::resolve::body_resolution;
use crate::signatures::{
    AdtDef, FieldDef, GenericParamDef, GenericParamDefKind, Generics, ProgramSignatures, VariantDef,
};
use crate::ty::{Const, CtorRef, FnDefId, GenericArg, GenericArgs, IntTy, Ty, TyKind};

const MAIN: &str = "/game/main.kis";

fn setup(source: &str) -> (RootDatabase, Program) {
    setup_at(MAIN, source)
}

fn setup_at(main: &str, source: &str) -> (RootDatabase, Program) {
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from(main), source.to_owned());
    files.insert(PathBuf::from("/game/a.png"), String::new());
    files.insert(PathBuf::from("/game/sepia.wgsl"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk(main));
    (db, program)
}

fn std_table(db: &RootDatabase, program: Program) -> StdTable {
    super::std_items::std_table(db, program)
        .clone()
        .expect("std loads")
}

/// A top-level item of the game root by name.
fn item(db: &RootDatabase, program: Program, name: &str) -> ItemId {
    let file = *program.root(db);
    let tree = item_tree(db, file);
    let index = tree
        .top
        .iter()
        .copied()
        .find(|index| tree.item(*index).and_then(|item| item.name.as_deref()) == Some(name))
        .unwrap_or_else(|| panic!("no item `{name}`"));
    ItemId { file, index }
}

/// A minimal stand-in for type inference, enough for single bodies: paths from name
/// resolution, integer types from literal suffixes propagated through operators, records
/// and constructor patterns from their resolved paths.
fn mini_infer(db: &RootDatabase, program: Program, item: ItemId) -> InferenceResult {
    let body = body_of(db, item).unwrap();
    let res = body_resolution(db, program, item).body.as_ref().unwrap();
    let mut out = InferenceResult::default();
    let ctor_of = |resolution: &Resolution| -> Option<CtorRef> {
        match resolution {
            Resolution::Def(Def::Item(adt))
                if matches!(
                    item_tree(db, adt.file)
                        .item(adt.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Struct { .. })
                ) =>
            {
                Some(CtorRef {
                    adt: *adt,
                    variant: None,
                    args: GenericArgs::empty(),
                })
            }
            Resolution::Def(Def::Variant { enum_item, index }) => Some(CtorRef {
                adt: *enum_item,
                variant: Some(*index),
                args: GenericArgs::empty(),
            }),
            _ => None,
        }
    };
    for (id, expr) in body.store.exprs.iter() {
        match expr {
            Expr::Path(_) => {
                let value = match res.store.exprs.get(&id) {
                    Some(Resolution::Local(local)) => ValueRef::Local(*local),
                    Some(Resolution::Def(Def::Item(target)) | Resolution::Assoc(target)) => {
                        match &item_tree(db, target.file).item(target.index).unwrap().kind {
                            ItemKind::Fn { .. } => ValueRef::Fn(FnRef {
                                def: FnDefId::Item(*target),
                                self_ty: None,
                                args: GenericArgs::empty(),
                            }),
                            ItemKind::Const { .. } => ValueRef::Const {
                                item: *target,
                                args: Vec::new(),
                            },
                            ItemKind::Static { .. } => ValueRef::Static(*target),
                            ItemKind::Struct {
                                shape: Shape::Tuple(_),
                            } => ValueRef::Fn(FnRef {
                                def: FnDefId::StructCtor(*target),
                                self_ty: None,
                                args: GenericArgs::empty(),
                            }),
                            _ => ValueRef::Ctor(CtorRef {
                                adt: *target,
                                variant: None,
                                args: GenericArgs::empty(),
                            }),
                        }
                    }
                    Some(Resolution::Def(Def::Variant { enum_item, index })) => {
                        let shape = &super::value::adt_variants(db, *enum_item)[*index as usize].1;
                        if matches!(shape, Shape::Tuple(_)) {
                            ValueRef::Fn(FnRef {
                                def: FnDefId::VariantCtor(*enum_item, *index),
                                self_ty: None,
                                args: GenericArgs::empty(),
                            })
                        } else {
                            ValueRef::Ctor(CtorRef {
                                adt: *enum_item,
                                variant: Some(*index),
                                args: GenericArgs::empty(),
                            })
                        }
                    }
                    _ => continue,
                };
                out.paths.insert(id, value);
            }
            Expr::Record { .. } => {
                if let Some(ctor) = res.store.exprs.get(&id).and_then(ctor_of) {
                    out.records.insert(id, ctor);
                }
            }
            _ => {}
        }
    }
    // Integer types: suffixed literals, then operators take their left operand's type.
    for _ in 0..4 {
        for (id, expr) in body.store.exprs.iter() {
            let ty = match expr {
                Expr::Literal(Literal::Int {
                    suffix: Some(suffix),
                    ..
                }) => IntTy::from_suffix(suffix).map(Ty::int),
                Expr::Binary { op, lhs, .. } => match op {
                    crate::hir::BinaryOp::Eq
                    | crate::hir::BinaryOp::Ne
                    | crate::hir::BinaryOp::Lt
                    | crate::hir::BinaryOp::Le
                    | crate::hir::BinaryOp::Gt
                    | crate::hir::BinaryOp::Ge
                    | crate::hir::BinaryOp::And
                    | crate::hir::BinaryOp::Or => Some(Ty::bool()),
                    _ => out.expr_types.get(*lhs).cloned(),
                },
                Expr::Unary { expr, .. } => out.expr_types.get(*expr).cloned(),
                _ => None,
            };
            if let Some(ty) = ty {
                out.expr_types.insert(id, ty);
            }
        }
    }
    for (id, pat) in body.store.pats.iter() {
        if matches!(
            pat,
            Pat::TupleStruct { .. } | Pat::Record { .. } | Pat::Path(_) | Pat::Bind { .. }
        ) && let Some(ctor) = res.store.pats.get(&id).and_then(ctor_of)
        {
            out.pat_ctors.insert(id, ctor);
        }
    }
    out
}

/// Evaluates the `const` item `name` of a game program through the `const_value` query.
fn eval_item(source: &str, name: &str) -> Result<ConstValue, EvalError> {
    let (db, program) = setup(source);
    let id = item(&db, program, name);
    const_value(&db, program, id, GenericArgs::empty()).clone()
}

fn fault_code(result: Result<ConstValue, EvalError>) -> DiagnosticCode {
    match result {
        Err(EvalError {
            kind: ErrorKind::Fault(fault),
            ..
        }) => fault.code,
        other => panic!("expected a fault, got {other:?}"),
    }
}

fn dec(text: &str) -> Decimal {
    Decimal::parse(text).unwrap()
}

#[test]
fn integer_helpers() {
    assert_eq!(num::wrap_to(300, IntTy::U8), Some(44));
    assert_eq!(num::wrap_to(-1, IntTy::U8), Some(255));
    assert_eq!(num::wrap_to(200, IntTy::I8), Some(-56));
    assert_eq!(num::wrap_to(-1, IntTy::U128), None);
    assert_eq!(num::wrap_to(-1, IntTy::I128), Some(-1));
    assert_eq!(num::shl(1, 7, IntTy::U8), Some(128));
    assert_eq!(num::shl(3, 7, IntTy::U8), Some(128));
    assert_eq!(num::shl(1, 8, IntTy::U8), None);
    assert_eq!(num::shr(-8, 1, IntTy::I32), Some(-4));
    assert_eq!(num::shr(0x382000, 16, IntTy::U32), Some(0x38));
    assert_eq!(num::bit_not(0, IntTy::U8), Some(255));
    assert_eq!(num::bit_not(0, IntTy::I32), Some(-1));
}

// ----- Std items ----------------------------------------------------------------------

#[test]
fn every_const_builtin_is_known() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let mut known = 0usize;
    for (item, builtin) in &std.builtins {
        let data = item_tree(&db, item.file).item(item.index).unwrap();
        let ItemKind::Fn { kind, .. } = data.kind else {
            panic!("builtin is not a fn");
        };
        if kind == FnKind::Const {
            assert_ne!(
                *builtin,
                Builtin::Runtime,
                "`{}` in {:?} is a const builtin the evaluator does not implement",
                data.name.as_deref().unwrap_or("?"),
                item.file.path(&db)
            );
            known = known.saturating_add(1);
        } else {
            assert_eq!(*builtin, Builtin::Runtime);
        }
    }
    assert_eq!(known, std_items::BUILTINS_LEN);
}

// ----- Screen model -------------------------------------------------------------------

fn opacity(screen: &Screen, id: u32) -> ConstValue {
    screen.logical.entities[&id].opacity.clone()
}

fn tween(std: &StdTable, easing: ConstValue) -> ConstValue {
    std.tween
        .make(vec![std.duration_value(300), easing, std.duration_value(0)])
}

fn linear(std: &StdTable) -> ConstValue {
    std.easing.unit("Linear")
}

fn transition(std: &StdTable) -> ConstValue {
    let kind = ConstValue::unit();
    std.transition.make(vec![
        kind,
        std.duration_value(0),
        linear(std),
        std.duration_value(0),
    ])
}

fn image(path: &str) -> ConstValue {
    ConstValue::Asset {
        kind: AssetKind::Image,
        path: FilePath::disk(path),
    }
}

#[test]
fn empty_screen_and_spawning() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let mut screen = Screen::empty(&std);
    assert!(screen.contains(0));
    assert_eq!(
        screen.logical.entities[&0].plane,
        std.plane.unit("Background")
    );
    assert_eq!(opacity(&screen, 0), ConstValue::Decimal(Decimal::ONE));
    assert_eq!(screen.logical.hud.text_window, std.visibility.unit("Shown"));
    assert_eq!(
        screen.logical.music.volume,
        ConstValue::Decimal(Decimal::ONE)
    );
    assert!(!screen.contains(7));
    let entity = screen.entity(7, &std);
    assert_eq!(entity.opacity, ConstValue::Decimal(Decimal::ZERO));
    assert_eq!(entity.plane, std.plane.unit("Stage"));
    assert_eq!(entity.gate, std.none());
    assert!(screen.contains(7));
    assert_eq!(screen.entities_on(&std.plane.unit("Stage")), vec![7]);
    assert_eq!(screen.entities_on(&std.plane.unit("Background")), vec![0]);
}

#[test]
fn pending_changes_need_an_animation() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let mut screen = Screen::empty(&std);
    screen.entity(3, &std);
    let change = PendingChange::Prop(
        PropTarget::Entity(3, EntityProp::Opacity),
        ConstValue::Decimal(Decimal::ONE),
    );
    // Making the change does nothing by itself.
    Screen::check_change(&change).unwrap();
    assert_eq!(opacity(&screen, 3), ConstValue::Decimal(Decimal::ZERO));
    screen
        .commit(change, tween(&std, linear(&std)), &std)
        .unwrap();
    assert_eq!(opacity(&screen, 3), ConstValue::Decimal(Decimal::ONE));
    assert_eq!(screen.step.scheduled.len(), 1);

    let swap = PendingChange::Layer(LayerTarget::Part(3, 0), Some(image("/game/a.png")));
    assert_eq!(screen.layer(LayerTarget::Part(3, 0), &std), None);
    screen.commit(swap, transition(&std), &std).unwrap();
    assert_eq!(
        screen.layer(LayerTarget::Part(3, 0), &std),
        Some(image("/game/a.png"))
    );

    // The same track again is a no-op: nothing more is scheduled.
    let track = ConstValue::Tuple(imbl::vector![ConstValue::Int(1)]);
    let fade = ConstValue::unit();
    screen
        .commit(
            PendingChange::Channel(ChannelId::Music, Some(track.clone())),
            fade.clone(),
            &std,
        )
        .unwrap();
    let scheduled = screen.step.scheduled.len();
    screen
        .commit(
            PendingChange::Channel(ChannelId::Music, Some(track.clone())),
            fade,
            &std,
        )
        .unwrap();
    assert_eq!(screen.step.scheduled.len(), scheduled);
    assert_eq!(screen.channel(ChannelId::Music).track, Some(track));
}

#[test]
fn replay_bgm_restarts_the_track() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    // Without a track there is nothing to restart.
    let mut silent = Screen::empty(&std);
    silent.replay_music().unwrap();
    assert!(silent.step.scheduled.is_empty());

    let mut screen = Screen::empty(&std);
    let track = ConstValue::Tuple(imbl::vector![ConstValue::Int(1)]);
    screen
        .commit(
            PendingChange::Channel(ChannelId::Music, Some(track)),
            ConstValue::unit(),
            &std,
        )
        .unwrap();
    let carried = screen.clone();
    screen.replay_music().unwrap();
    let replays: Vec<&Scheduled> = screen
        .step
        .scheduled
        .iter()
        .skip(carried.step.scheduled.len())
        .collect();
    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].kind, ScheduledKind::ReplayMusic);
    assert_eq!(replays[0].target, Some(Target::Channel(ChannelId::Music)));
    // The logical screen is unchanged, so the screens compare equal.
    assert_eq!(screen, carried);
}

fn hash_of(screen: &Screen) -> u64 {
    let mut hasher = DefaultHasher::new();
    screen.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn logical_equality_ignores_the_step() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let empty = Screen::empty(&std);
    let mut timed = Screen::empty(&std);
    timed.wait(500).unwrap();
    timed.barrier();
    timed.at_mark(2);
    timed.stop_sounds(100).unwrap();
    timed
        .step
        .disabled_arms
        .push((0, ConstValue::Text(Text::plain("no"))));
    timed.step.menu_align = std.valign.unit("Top");
    timed
        .apply_motion(Target::Camera, motion(&std, &[(300, 0, 0)], 1), &std)
        .unwrap();
    assert_eq!(empty, timed);
    assert_eq!(hash_of(&empty), hash_of(&timed));
    let mut spawned = Screen::empty(&std);
    spawned.entity(1, &std);
    assert_ne!(empty, spawned);
    // A despawned entity leaves the logical screen at once, and comes back with its values.
    spawned
        .commit(
            PendingChange::Prop(
                PropTarget::Entity(1, EntityProp::Opacity),
                ConstValue::Decimal(dec("0.5")),
            ),
            tween(&std, linear(&std)),
            &std,
        )
        .unwrap();
    spawned.despawn(1);
    assert_eq!(empty, spawned);
    assert!(!spawned.contains(1));
    assert_eq!(
        spawned.entity(1, &std).opacity,
        ConstValue::Decimal(dec("0.5"))
    );
    spawned.despawn(1);
    spawned.step_entry(&std);
    assert_eq!(
        spawned.entity(1, &std).opacity,
        ConstValue::Decimal(Decimal::ZERO)
    );
}

#[test]
fn timeline_rules() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let mut screen = Screen::empty(&std);
    screen.begin_text().unwrap();
    screen.at_mark(1);
    let error = screen.begin_text().unwrap_err();
    assert_eq!(error.code, DiagnosticCode::EvalBeginTextOnMark);
    screen.wait(100).unwrap();
    assert!(screen.begin_text().is_err());
    screen.at_entry();
    screen.begin_text().unwrap();
    screen.wait(u32::MAX).unwrap();
    assert_eq!(
        screen.wait(1).unwrap_err().code,
        DiagnosticCode::EvalDurationOverflow
    );
    let sound = std.sound.make(vec![
        image("/game/a.png"),
        ConstValue::Decimal(dec("1.5")),
        std.duration_value(0),
        std.none(),
        std.duration_value(0),
    ]);
    let mut screen = Screen::empty(&std);
    assert_eq!(
        screen.play(sound, &std).unwrap_err().code,
        DiagnosticCode::EvalFractionOutOfRange
    );
}

// ----- Animation rules ----------------------------------------------------------------

/// A motion with keys `(at, dx, dy)`, scale 1.0, rotation 0°, easing Linear.
fn motion(std: &StdTable, keys: &[(u32, i128, i128)], repeat: i128) -> ConstValue {
    motion_with(std, keys, repeat, linear(std))
}

fn motion_with(
    std: &StdTable,
    keys: &[(u32, i128, i128)],
    repeat: i128,
    easing: ConstValue,
) -> ConstValue {
    let keys = keys
        .iter()
        .map(|(at, dx, dy)| {
            std.motion_key.make(vec![
                std.duration_value(*at),
                std.offset
                    .make(vec![ConstValue::Int(*dx), ConstValue::Int(*dy)]),
                ConstValue::Decimal(Decimal::ONE),
                std.angle.make(vec![ConstValue::Decimal(Decimal::ZERO)]),
                easing.clone(),
            ])
        })
        .collect();
    std.motion.make(vec![
        ConstValue::Vec(keys),
        ConstValue::Int(repeat),
        std.duration_value(0),
    ])
}

#[test]
fn motion_identity_rule() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let check = |value: ConstValue| anim::check_motion(&value, &std).map_err(|fault| fault.code);
    assert_eq!(
        check(motion(&std, &[(150, 0, -20), (300, 0, 0)], 1)),
        Ok(())
    );
    assert_eq!(
        check(motion(&std, &[], 1)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    assert_eq!(
        check(motion(&std, &[(0, 0, 0)], 1)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    assert_eq!(
        check(motion(&std, &[(150, 1, 0), (150, 0, 0)], 1)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    assert_eq!(
        check(motion(&std, &[(150, 1, 0), (100, 0, 0)], 1)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    assert_eq!(
        check(motion(&std, &[(150, 0, 5)], 1)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    assert_eq!(
        check(motion(&std, &[(150, 0, 0)], 0)),
        Err(DiagnosticCode::EvalInvalidMotion)
    );
    let steps0 = ConstValue::Adt {
        item: std.easing.item,
        variant: std.easing.variant("Steps").unwrap(),
        fields: imbl::vector![ConstValue::Int(0)],
    };
    assert_eq!(
        check(motion_with(&std, &[(150, 0, 0)], 1, steps0)),
        Err(DiagnosticCode::EvalInvalidEasing)
    );
    // A scale that does not return to 1.0 breaks identity too.
    let mut bad_scale = motion(&std, &[(100, 0, 0)], 1);
    if let ConstValue::Adt { fields, .. } = &mut bad_scale
        && let ConstValue::Vec(keys) = &mut fields[0]
        && let ConstValue::Adt { fields, .. } = &mut keys[0]
    {
        fields[2] = ConstValue::Decimal(dec("1.5"));
    }
    assert_eq!(check(bad_scale), Err(DiagnosticCode::EvalInvalidMotion));
}

#[test]
fn easing_rules() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let bezier = |x1: &str, y1: &str, x2: &str| ConstValue::Adt {
        item: std.easing.item,
        variant: std.easing.variant("CubicBezier").unwrap(),
        fields: imbl::vector![
            ConstValue::Decimal(dec(x1)),
            ConstValue::Decimal(dec(y1)),
            ConstValue::Decimal(dec(x2)),
            ConstValue::Decimal(dec("1.0")),
        ],
    };
    let check = |easing: ConstValue| anim::check_easing(&easing, &std).map_err(|fault| fault.code);
    assert_eq!(check(bezier("0.25", "2.5", "0.75")), Ok(()));
    assert_eq!(
        check(bezier("1.5", "0.1", "0.75")),
        Err(DiagnosticCode::EvalInvalidEasing)
    );
    assert_eq!(
        check(bezier("0.5", "0.1", "1.01")),
        Err(DiagnosticCode::EvalInvalidEasing)
    );
    let steps = |n: i128| ConstValue::Adt {
        item: std.easing.item,
        variant: std.easing.variant("Steps").unwrap(),
        fields: imbl::vector![ConstValue::Int(n)],
    };
    assert_eq!(check(steps(3)), Ok(()));
    assert_eq!(check(steps(0)), Err(DiagnosticCode::EvalInvalidEasing));
    assert_eq!(check(linear(&std)), Ok(()));
    // An applied tween checks its easing.
    let mut screen = Screen::empty(&std);
    let change = PendingChange::Prop(PropTarget::CameraZoom, ConstValue::Decimal(dec("2.0")));
    assert_eq!(
        screen
            .commit(change, tween(&std, steps(0)), &std)
            .unwrap_err()
            .code,
        DiagnosticCode::EvalInvalidEasing
    );
}

#[test]
fn fraction_rules() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let decimal = |text: &str| ConstValue::Decimal(dec(text));
    let check = |change: PendingChange| Screen::check_change(&change).map_err(|f| f.code);
    let opacity = |value| PendingChange::Prop(PropTarget::Entity(1, EntityProp::Opacity), value);
    assert_eq!(check(opacity(decimal("1.0"))), Ok(()));
    assert_eq!(
        check(opacity(decimal("1.5"))),
        Err(DiagnosticCode::EvalFractionOutOfRange)
    );
    let volume =
        |value| PendingChange::Prop(PropTarget::ChannelVolume(ChannelId::Ambience(2)), value);
    assert_eq!(
        check(volume(ConstValue::Decimal(
            dec("0.1").checked_neg().unwrap()
        ))),
        Err(DiagnosticCode::EvalFractionOutOfRange)
    );
    let anchor = |x: &str, y: &str| std.anchor.make(vec![decimal(x), decimal(y)]);
    assert_eq!(anim::check_anchor(&anchor("0.5", "1.0"), &std), Ok(()));
    assert_eq!(
        anim::check_anchor(&anchor("0.5", "2.0"), &std).map_err(|f| f.code),
        Err(DiagnosticCode::EvalFractionOutOfRange)
    );
}

// ----- Text ---------------------------------------------------------------------------

fn mark_effect(std: &StdTable, mark: Option<i128>) -> ConstValue {
    let fields = std
        .text_effect
        .fields
        .iter()
        .map(|field| match (field.as_str(), mark) {
            ("mark", Some(n)) => std.some(std.mark.make(vec![ConstValue::Int(n)])),
            _ => std.none(),
        })
        .collect();
    std.text_effect.make(fields)
}

#[test]
fn text_building_and_marks() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let marked = |n| Text::Effect {
        base: Box::new(Text::plain("")),
        effect: Box::new(mark_effect(&std, Some(n))),
    };
    let ok = Text::Concat(imbl::vector![
        Text::plain("Wait"),
        marked(1),
        Text::plain("—what"),
        marked(2),
        Text::Effect {
            base: Box::new(Text::plain("!")),
            effect: Box::new(mark_effect(&std, None)),
        },
    ]);
    check_marks(&ok, &std).unwrap();
    assert_eq!(ok.base_string(), "Wait—what!");
    let duplicate = Text::Concat(imbl::vector![
        marked(1),
        Text::Concat(imbl::vector![Text::plain("a"), marked(1)]),
    ]);
    assert_eq!(
        check_marks(&duplicate, &std).unwrap_err().code,
        DiagnosticCode::EvalDuplicateMark
    );
    let ruby = Text::Ruby {
        base: Box::new(Text::plain("数多")),
        reading: Box::new(Text::plain("あまた")),
    };
    assert_eq!(ruby.base_string(), "数多");
    assert_eq!(to_text(&ConstValue::Int(-42)), Some(Text::plain("-42")));
    assert_eq!(
        to_text(&ConstValue::Decimal(dec("0.50"))),
        Some(Text::plain("0.5"))
    );
    assert_eq!(
        to_text(&ConstValue::Rat(Rat::new(3, 4).unwrap())),
        Some(Text::plain("3/4"))
    );
    assert_eq!(to_text(&ConstValue::Text(ruby.clone())), Some(ruby));
    assert_eq!(to_text(&ConstValue::Bool(true)), None);
}

// ----- Builtins through places --------------------------------------------------------

struct Harness {
    db: RootDatabase,
    program: Program,
    std: StdTable,
    item: ItemId,
    /// The result type of the calls.
    ty: Option<Ty>,
}

impl Harness {
    fn new() -> Harness {
        let (db, program) = setup("const X: u32 = 1u32;");
        let std = std_table(&db, program);
        let item = item(&db, program, "X");
        Harness {
            db,
            program,
            std,
            item,
            ty: None,
        }
    }
}

fn call(
    machine: &mut Machine<'_>,
    harness: &Harness,
    builtin: Builtin,
    args: Vec<Value>,
) -> Result<Value, ErrorKind> {
    let fn_ref = FnRef {
        def: FnDefId::Item(harness.item),
        self_ty: None,
        args: GenericArgs::empty(),
    };
    let value = body_of(&harness.db, harness.item).unwrap().value;
    let site = CallSite {
        at: Some(crate::diagnostic::Anchor::Node(
            crate::diagnostic::Owner::Body(harness.item),
            crate::diagnostic::NodeId::Expr(value),
        )),
        ty: harness.ty.clone(),
    };
    machine.builtin(site, builtin, &fn_ref, args)
}

fn reference(cell: usize) -> Value {
    Value::Ref(Box::new(Place {
        cell,
        path: Vec::new(),
        obj: None,
    }))
}

#[test]
fn screen_builtins_through_references() {
    let harness = Harness::new();
    let std = &harness.std;
    let mut m = Machine::new(&harness.db, harness.program, std);
    let screen = call(&mut m, &harness, Builtin::ScreenEmpty, vec![]).unwrap();
    let cell = m.alloc(screen);
    let id = m.thaw(&std.entity_id_value(4));
    let entity = call(
        &mut m,
        &harness,
        Builtin::ScreenEntity,
        vec![reference(cell), id.clone()],
    )
    .unwrap();
    let prop = call(
        &mut m,
        &harness,
        Builtin::EntityOpacity,
        vec![entity.clone()],
    )
    .unwrap();
    let change = call(
        &mut m,
        &harness,
        Builtin::PropChange,
        vec![prop.clone(), Value::Decimal(Decimal::ONE)],
    )
    .unwrap();
    // Not applied yet.
    let current = call(&mut m, &harness, Builtin::PropGet, vec![prop.clone()]).unwrap();
    assert!(matches!(current, Value::Decimal(value) if value == Decimal::ZERO));
    let instant = m.thaw(&tween(std, linear(std)));
    call(
        &mut m,
        &harness,
        Builtin::ChangeApplyAnimation,
        vec![change, instant],
    )
    .unwrap();
    let current = call(&mut m, &harness, Builtin::PropGet, vec![prop.clone()]).unwrap();
    assert!(matches!(current, Value::Decimal(value) if value == Decimal::ONE));
    // An out-of-range opacity is rejected where the change is made.
    let bad = call(
        &mut m,
        &harness,
        Builtin::PropChange,
        vec![prop, Value::Decimal(dec("1.5"))],
    );
    assert!(
        matches!(bad, Err(ErrorKind::Fault(fault)) if fault.code == DiagnosticCode::EvalFractionOutOfRange)
    );
    let contains = call(
        &mut m,
        &harness,
        Builtin::ScreenContains,
        vec![reference(cell), id.clone()],
    )
    .unwrap();
    assert!(matches!(contains, Value::Bool(true)));
    call(
        &mut m,
        &harness,
        Builtin::ScreenDespawn,
        vec![reference(cell), id.clone()],
    )
    .unwrap();
    let contains = call(
        &mut m,
        &harness,
        Builtin::ScreenContains,
        vec![reference(cell), id],
    )
    .unwrap();
    assert!(matches!(contains, Value::Bool(false)));
    // `begin_text` on a mark.
    let mark = m.thaw(&std.mark.make(vec![ConstValue::Int(1)]));
    call(
        &mut m,
        &harness,
        Builtin::ScreenAtMark,
        vec![reference(cell), mark],
    )
    .unwrap();
    let begin = call(
        &mut m,
        &harness,
        Builtin::ScreenBeginText,
        vec![reference(cell)],
    );
    assert!(
        matches!(begin, Err(ErrorKind::Fault(fault)) if fault.code == DiagnosticCode::EvalBeginTextOnMark)
    );
    // The probe screen is inspected by any builtin.
    let probe = m.alloc(Value::Probe);
    let one = m.thaw(&std.entity_id_value(1));
    let entity = call(
        &mut m,
        &harness,
        Builtin::ScreenEntity,
        vec![reference(probe), one],
    );
    assert_eq!(entity.unwrap_err(), ErrorKind::Inspected);
    assert_eq!(
        call(&mut m, &harness, Builtin::ScreenEmpty, vec![]).map(|_| ()),
        Ok(())
    );
}

#[test]
fn vec_and_text_builtins() {
    let harness = Harness::new();
    let std = &harness.std;
    let mut m = Machine::new(&harness.db, harness.program, std);
    let vec = call(&mut m, &harness, Builtin::VecNew, vec![]).unwrap();
    let cell = m.alloc(vec);
    call(
        &mut m,
        &harness,
        Builtin::VecPush,
        vec![reference(cell), Value::Int(7)],
    )
    .unwrap();
    let len = call(&mut m, &harness, Builtin::VecLen, vec![reference(cell)]).unwrap();
    assert!(matches!(len, Value::Int(1)));
    call(
        &mut m,
        &harness,
        Builtin::VecSet,
        vec![reference(cell), Value::Int(0), Value::Int(9)],
    )
    .unwrap();
    let got = call(
        &mut m,
        &harness,
        Builtin::VecGet,
        vec![reference(cell), Value::Int(0)],
    )
    .unwrap();
    assert_eq!(m.freeze(&got).unwrap(), std.some(ConstValue::Int(9)));
    let missing = call(
        &mut m,
        &harness,
        Builtin::VecGet,
        vec![reference(cell), Value::Int(5)],
    )
    .unwrap();
    assert_eq!(m.freeze(&missing).unwrap(), std.none());
    let set = call(
        &mut m,
        &harness,
        Builtin::VecSet,
        vec![reference(cell), Value::Int(1), Value::Int(9)],
    );
    assert!(
        matches!(set, Err(ErrorKind::Fault(fault)) if fault.code == DiagnosticCode::EvalIndexOutOfRange)
    );

    let effect = m.thaw(&mark_effect(std, Some(3)));
    let marked = call(
        &mut m,
        &harness,
        Builtin::Effect,
        vec![Value::Text(Text::plain("x")), effect],
    )
    .unwrap();
    let concat = call(
        &mut m,
        &harness,
        Builtin::Concat,
        vec![Value::Array(imbl::vector![
            marked.clone(),
            Value::Text(Text::plain("y")),
            marked,
        ])],
    );
    assert!(
        matches!(concat, Err(ErrorKind::Fault(fault)) if fault.code == DiagnosticCode::EvalDuplicateMark)
    );
    let text = call(&mut m, &harness, Builtin::ToText, vec![Value::Int(12)]).unwrap();
    assert!(matches!(text, Value::Text(Text::Plain(text)) if &*text == "12"));
    let handle = call(&mut m, &harness, Builtin::Setting, vec![Value::Bool(true)]).unwrap();
    assert!(matches!(
        handle,
        Value::NewHandle {
            kind: HandleKind::Setting,
            ..
        }
    ));
    assert!(matches!(
        m.freeze(&handle),
        Err(ErrorKind::Fault(fault)) if fault.code == DiagnosticCode::EvalHandleOutsideStatic
    ));
    let runtime = call(&mut m, &harness, Builtin::Runtime, vec![]);
    assert!(matches!(runtime, Err(ErrorKind::Abort)));
}

#[test]
fn integer_conversion_builtins() {
    let mut harness = Harness::new();
    harness.ty = Some(Ty::int(IntTy::U8));
    let std = &harness.std;
    let mut m = Machine::new(&harness.db, harness.program, std);
    let saturated = call(
        &mut m,
        &harness,
        Builtin::IntegerSaturate,
        vec![Value::Int(300)],
    )
    .unwrap();
    assert!(matches!(saturated, Value::Int(255)));
    let saturated = call(
        &mut m,
        &harness,
        Builtin::IntegerSaturate,
        vec![Value::Int(-3)],
    )
    .unwrap();
    assert!(matches!(saturated, Value::Int(0)));
    let into = call(&mut m, &harness, Builtin::IntegerInto, vec![Value::Int(42)]).unwrap();
    assert!(matches!(into, Value::Int(42)));
}

// ----- The interpreter over bodies ----------------------------------------------------

#[test]
fn integer_overflow_and_division() {
    assert_eq!(
        fault_code(eval_item("const A: u8 = 200u8 + 100u8;", "A")),
        DiagnosticCode::EvalOverflow
    );
    assert_eq!(
        fault_code(eval_item("const A: i8 = -(-128i8);", "A")),
        DiagnosticCode::EvalOverflow
    );
    assert_eq!(
        fault_code(eval_item("const B: i32 = 1i32 / (2i32 - 2i32);", "B")),
        DiagnosticCode::EvalDivisionByZero
    );
    assert_eq!(
        fault_code(eval_item("const B: i32 = 1i32 % 0i32;", "B")),
        DiagnosticCode::EvalDivisionByZero
    );
    assert_eq!(
        eval_item("const C: u8 = 200u8 + 55u8;", "C"),
        Ok(ConstValue::Int(255))
    );
    assert_eq!(
        eval_item("const D: u32 = (0x382000u32 >> 16u32) & 255u32;", "D"),
        Ok(ConstValue::Int(0x38))
    );
    assert_eq!(
        eval_item("const E: Decimal = 1.0 / 3.0;", "E"),
        Ok(ConstValue::Decimal(dec("0.3333333")))
    );
    assert_eq!(
        eval_item("const E: Decimal = 0.5 * 0.5 + 0.25;", "E"),
        Ok(ConstValue::Decimal(dec("0.5")))
    );
}

#[test]
fn budget_is_bounded() {
    assert_eq!(
        fault_code(eval_item("const A: u32 = { loop {} };", "A")),
        DiagnosticCode::EvalBudgetExceeded
    );
}

#[test]
fn control_flow() {
    let source = "const S: i32 = {
        let mut i = 0i32;
        let mut s = 0i32;
        while i < 10i32 {
            i += 1i32;
            if i % 2i32 == 0i32 { continue; }
            s += i;
        }
        let t = 'outer: loop {
            loop { break 'outer s * 2i32; }
        };
        for k in 0i32..3i32 { s += k; }
        for k in [1i32, 2i32] { s += k; }
        s + t
    };";
    // s = 25 after the while loop, t = 50; then s += 0 + 1 + 2 and 1 + 2.
    assert_eq!(eval_item(source, "S"), Ok(ConstValue::Int(81)));
}

#[test]
fn patterns() {
    let source = "const E: i32 = match 7i32 { 0..=5 => 1i32, 6 | 7 => 2i32, _ => 3i32 };";
    assert_eq!(eval_item(source, "E"), Ok(ConstValue::Int(2)));
    let source = "const E: i32 = match -3i32 { -5..=-1 => 1i32, _ => 2i32 };";
    assert_eq!(eval_item(source, "E"), Ok(ConstValue::Int(1)));
    let source = "const K: i32 = match Some(5i32) { Some(x) => x, None => 0i32 };";
    assert_eq!(eval_item(source, "K"), Ok(ConstValue::Int(5)));
    let source = "const J: i32 = { let (a, _, b) = (1i32, true, 2i32); a * 10i32 + b };";
    assert_eq!(eval_item(source, "J"), Ok(ConstValue::Int(12)));
    let source = "enum Mood { Warm, Cold }
        const M: bool = match (Mood::Cold, 3i32) { (Mood::Warm, _) => false, (Mood::Cold, ..) => true };";
    assert_eq!(eval_item(source, "M"), Ok(ConstValue::Bool(true)));
    let source = "const LIMIT: i32 = 4i32;
        const L: i32 = match 4i32 { LIMIT => 1i32, _ => 0i32 };";
    assert_eq!(eval_item(source, "L"), Ok(ConstValue::Int(1)));
    // Stage 3 reports the non-exhaustive match; evaluation stops silently.
    assert_eq!(
        eval_item("const F: i32 = match 9i32 { 0..=5 => 1i32 };", "F"),
        Err(EvalError::abort())
    );
}

#[test]
fn records_and_references() {
    let source = "struct P { x: i32, y: i32 }
        const P1: P = { let mut a = 1i32; let r = &mut a; *r = 5i32; P { y: a, x: 2i32 } };";
    let (db, program) = setup(source);
    let p = item(&db, program, "P");
    assert_eq!(
        eval_item(source, "P1"),
        Ok(ConstValue::Adt {
            item: p,
            variant: 0,
            fields: imbl::vector![ConstValue::Int(2), ConstValue::Int(5)],
        })
    );
}

#[test]
fn vec_set_out_of_range_and_try() {
    let source = "const G: Vec<u32> = { let mut v = Vec::new(); Vec::push(&mut v, 1u32); Vec::set(&mut v, 3usize, 2u32); v };";
    assert_eq!(
        fault_code(eval_item(source, "G")),
        DiagnosticCode::EvalIndexOutOfRange
    );
    let source = "const G: Vec<u32> = { let mut v = Vec::new(); Vec::push(&mut v, 1u32); Vec::set(&mut v, 0usize, 2u32); v };";
    assert_eq!(
        eval_item(source, "G"),
        Ok(ConstValue::Vec(imbl::vector![ConstValue::Int(2)]))
    );
    // Stage 3 rejects `?` in a constant; evaluation stops without a second diagnostic.
    let source = "const I: u8 = { let x: Option<u8> = None; x? };";
    assert_eq!(eval_item(source, "I"), Err(EvalError::abort()));
    let source = "const fn get(x: Option<u8>) -> Option<u8> { Some(x?) }
        const I: u8 = match get(Some(3u8)) { Some(v) => v, None => 0u8 };";
    assert_eq!(eval_item(source, "I"), Ok(ConstValue::Int(3)));
    let source = "const fn get(x: Option<u8>) -> Option<u8> { Some(x?) }
        const I: u8 = match get(None) { Some(v) => v, None => 0u8 };";
    assert_eq!(eval_item(source, "I"), Ok(ConstValue::Int(0)));
    let source =
        "const A: [i32; 2] = [1i32, 2i32]; const B: i32 = { let a = [1i32, 2i32]; a[2usize] };";
    assert_eq!(
        fault_code(eval_item(source, "B")),
        DiagnosticCode::EvalIndexOutOfRange
    );
}

#[test]
fn text_and_asset_values() {
    let source = "use std::narrative::text::concat;
        const H: Text = concat([\"a\", \"b\"]);";
    assert_eq!(
        eval_item(source, "H"),
        Ok(ConstValue::Text(Text::Concat(imbl::vector![
            Text::plain("a"),
            Text::plain("b")
        ])))
    );
    let source = "const IMG: ImageAsset = asset(\"a.png\");";
    assert_eq!(eval_item(source, "IMG"), Ok(image("/game/a.png")));
    let source = "const S: Setting<bool> = setting(true);";
    assert_eq!(
        fault_code(eval_item(source, "S")),
        DiagnosticCode::EvalHandleOutsideStatic
    );
}

// ----- Constant expressions -----------------------------------------------------------

#[test]
fn constant_expressions() {
    let source = "static mut FLAG: bool = false;
        static HANDLE: Setting<bool> = setting(true);
        const fn f(a: i32, b: bool) -> i32 { a }
        pushdown fn p(param: bool) {
            let k = 3i32;
            let mut m = 1i32;
            let both = f(k + 1i32, param);
            let pure = f(k, true);
            let flag = FLAG;
            let handle = HANDLE;
            let closure = |x: i32| x;
            m += 1i32;
        }";
    let (db, program) = setup(source);
    let p = item(&db, program, "p");
    let infer = mini_infer(&db, program, p);
    let body = body_of(&db, p).unwrap();
    let res = body_resolution(&db, program, p).body.as_ref().unwrap();
    // Every expression needs a type to be constant; give the untyped ones `bool`.
    let mut infer = infer;
    for (id, _) in body.store.exprs.iter() {
        if infer.expr_types.get(id).is_none() {
            infer.expr_types.insert(id, Ty::bool());
        }
    }
    for (id, expr) in body.store.exprs.iter() {
        if matches!(expr, Expr::Closure { .. }) {
            infer.closures.insert(
                id,
                crate::infer::ClosureInfo {
                    kind: crate::hir::ClosureKind::Pushdown,
                    captures: Vec::new(),
                },
            );
        }
    }
    let consts = const_exprs::analyze(&db, body, res, &infer);
    let text = |id: ExprId| {
        let range = body_source_map_of(&db, p).unwrap().expr(id);
        source[usize::from(range.start())..usize::from(range.end())].to_owned()
    };
    let mut constant: Vec<String> = consts.exprs.iter().map(|id| text(*id)).collect();
    constant.sort();
    for expected in [
        "k + 1i32",
        "f(k, true)",
        "HANDLE",
        "|x: i32| x",
        "3i32",
        "k",
    ] {
        assert!(
            constant.iter().any(|text| text == expected),
            "`{expected}` should be constant: {constant:?}"
        );
    }
    for unexpected in ["f(k + 1i32, param)", "param", "FLAG", "m", "m += 1i32", "x"] {
        assert!(
            !constant.iter().any(|text| text == unexpected),
            "`{unexpected}` should not be constant"
        );
    }
    assert!(consts.locals.len() >= 2);
}

// ----- Finite domains -----------------------------------------------------------------

fn bounded(lo: i128, hi: i128) -> Ty {
    Ty::bounded(lo, hi)
}

#[test]
fn finite_domain_sizes_and_values() {
    let source = "enum Mood { Warm, Polite, Stranger }
        struct Pair { a: bool, m: Mood }
        enum E { A(bool), B }
        enum Opt<T> { None, Some(T) }
        const fn r1(s: Screen) -> Screen { s }
        const fn r2(s: Screen) -> Screen { s }";
    let (db, program) = setup(source);
    let std = std_table(&db, program);
    let mood = item(&db, program, "Mood");
    let pair = item(&db, program, "Pair");
    let e = item(&db, program, "E");
    let opt = item(&db, program, "Opt");
    let adt = |item: ItemId| Ty::adt(item, GenericArgs::empty());
    let variant = |name: &str, fields: Vec<Ty>| VariantDef {
        name: name.into(),
        shape: Shape::Tuple(u32::try_from(fields.len()).unwrap()),
        fields: fields
            .into_iter()
            .map(|ty| FieldDef { name: None, ty })
            .collect(),
    };
    let mut sigs = ProgramSignatures::default();
    let def = |is_enum, variants, generics| {
        ItemSig::Adt(AdtDef {
            generics,
            is_enum,
            builtin: false,
            must_use: false,
            shader_arg: false,
            variants,
        })
    };
    sigs.items.insert(
        mood,
        def(
            true,
            vec![
                variant("Warm", vec![]),
                variant("Polite", vec![]),
                variant("Stranger", vec![]),
            ],
            Generics::default(),
        ),
    );
    sigs.items.insert(
        pair,
        def(
            false,
            vec![variant("Pair", vec![Ty::bool(), adt(mood)])],
            Generics::default(),
        ),
    );
    sigs.items.insert(
        e,
        def(
            true,
            vec![variant("A", vec![Ty::bool()]), variant("B", vec![])],
            Generics::default(),
        ),
    );
    sigs.items.insert(
        opt,
        def(
            true,
            vec![
                variant("None", vec![]),
                variant(
                    "Some",
                    vec![Ty::new(TyKind::Param {
                        owner: opt,
                        index: 0,
                    })],
                ),
            ],
            Generics {
                parent: None,
                parent_count: 0,
                params: vec![GenericParamDef {
                    name: "T".into(),
                    kind: GenericParamDefKind::Type { bounds: Vec::new() },
                }],
            },
        ),
    );
    let screen = Ty::adt(std.screen, GenericArgs::empty());
    let render_sig = FnSig {
        generics: Generics::default(),
        kind: FnKind::Const,
        scene: false,
        entry: false,
        builtin: false,
        self_param: None,
        self_mutable: false,
        params: vec![screen.clone()],
        ret: screen.clone(),
    };
    let r1 = item(&db, program, "r1");
    let r2 = item(&db, program, "r2");
    let functions = vec![(r1, &render_sig), (r2, &render_sig)];
    let texts = vec![Text::plain("a"), Text::plain("b")];
    let mut assets = FxHashMap::default();
    assets.insert(AssetKind::Image, vec![image("/game/a.png")]);
    let domain = DomainCtx {
        sigs: &sigs,
        std: &std,
        texts: &texts,
        assets: &assets,
        functions: &functions,
    };
    let opt_of = |ty: Ty| Ty::adt(opt, GenericArgs::new(vec![GenericArg::Type(ty)]));
    assert_eq!(domain.size(&Ty::bool()), Some(2));
    assert_eq!(domain.size(&Ty::unit()), Some(1));
    assert_eq!(domain.size(&bounded(0, 5)), Some(6));
    assert_eq!(domain.size(&bounded(-3, 3)), Some(7));
    assert_eq!(
        domain.size(&Ty::tuple(vec![Ty::bool(), bounded(0, 5)])),
        Some(12)
    );
    assert_eq!(
        domain.size(&Ty::new(TyKind::Array {
            elem: Ty::bool(),
            len: Const::Value(3)
        })),
        Some(8)
    );
    assert_eq!(domain.size(&adt(mood)), Some(3));
    assert_eq!(domain.size(&adt(pair)), Some(6));
    assert_eq!(domain.size(&adt(e)), Some(3));
    assert_eq!(domain.size(&opt_of(adt(mood))), Some(4));
    assert_eq!(domain.size(&opt_of(opt_of(Ty::bool()))), Some(4));
    assert_eq!(domain.size(&Ty::reference(true, adt(mood))), Some(3));
    assert_eq!(
        domain.size(&Ty::adt(std.text, GenericArgs::empty())),
        Some(2)
    );
    assert_eq!(
        domain.size(&Ty::adt(std.assets[0].0, GenericArgs::empty())),
        Some(1)
    );
    assert_eq!(
        domain.size(&Ty::adt(std.assets[1].0, GenericArgs::empty())),
        Some(0)
    );
    let render_ptr = Ty::new(TyKind::FnPtr {
        kind: crate::hir::FnPtrKind::Const,
        params: vec![screen.clone()].into(),
        ret: screen.clone(),
    });
    assert_eq!(domain.size(&render_ptr), Some(2));
    let scene_ptr = Ty::new(TyKind::FnPtr {
        kind: crate::hir::FnPtrKind::Scene,
        params: vec![screen.clone()].into(),
        ret: screen,
    });
    assert_eq!(domain.size(&scene_ptr), Some(0));
    // Not finite.
    assert_eq!(domain.size(&Ty::int(IntTy::U8)), None);
    assert_eq!(
        domain.size(&Ty::adt(std.screen, GenericArgs::empty())),
        None
    );
    assert_eq!(
        domain.size(&Ty::new(TyKind::FnPtr {
            kind: crate::hir::FnPtrKind::Plain,
            params: Box::default(),
            ret: Ty::unit()
        })),
        None
    );

    let values = domain
        .values(
            &Ty::tuple(vec![Ty::bool(), bounded(0, 1)]),
            TABLE_DOMAIN_CAP,
        )
        .unwrap();
    let pairs: Vec<ConstValue> = [(false, 0), (false, 1), (true, 0), (true, 1)]
        .iter()
        .map(|(b, n)| ConstValue::Tuple(imbl::vector![ConstValue::Bool(*b), ConstValue::Int(*n)]))
        .collect();
    assert_eq!(values, pairs);
    let values = domain.values(&adt(e), TABLE_DOMAIN_CAP).unwrap();
    assert_eq!(
        values,
        vec![
            ConstValue::Adt {
                item: e,
                variant: 0,
                fields: imbl::vector![ConstValue::Bool(false)]
            },
            ConstValue::Adt {
                item: e,
                variant: 0,
                fields: imbl::vector![ConstValue::Bool(true)]
            },
            ConstValue::Adt {
                item: e,
                variant: 1,
                fields: imbl::vector![]
            },
        ]
    );
    assert_eq!(
        domain.values(&render_ptr, TABLE_DOMAIN_CAP).unwrap().len(),
        2
    );
}

#[test]
fn tabulation_cap() {
    let (db, program) = setup("");
    let std = std_table(&db, program);
    let sigs = ProgramSignatures::default();
    let assets = FxHashMap::default();
    let domain = DomainCtx {
        sigs: &sigs,
        std: &std,
        texts: &[],
        assets: &assets,
        functions: &[],
    };
    let byte_pair = Ty::tuple(vec![bounded(0, 255), bounded(0, 255)]);
    assert_eq!(domain.size(&byte_pair), Some(TABLE_DOMAIN_CAP));
    assert_eq!(
        domain
            .values(&byte_pair, TABLE_DOMAIN_CAP)
            .map(|values| values.len()),
        Some(65_536)
    );
    let too_many = Ty::new(TyKind::Array {
        elem: Ty::bool(),
        len: Const::Value(17),
    });
    assert_eq!(domain.size(&too_many), Some(131_072));
    assert_eq!(domain.values(&too_many, TABLE_DOMAIN_CAP), None);
    let huge = Ty::new(TyKind::Array {
        elem: bounded(0, 1000),
        len: Const::Value(100),
    });
    assert_eq!(domain.size(&huge), Some(u64::MAX));
    assert_eq!(domain.size(&bounded(i128::MIN, i128::MAX)), None);
}

// ----- Folding and tabulation of story bodies -----------------------------------------

#[test]
fn folds_and_tabulates_story_bodies() {
    let source = "use std::narrative::text::{concat, to_text};
        static mut DAY: Int<1, 3> = 1;
        static mut BIG: Int<0, 70000> = 0;
        pushdown fn p() {
            let line = concat([\"Day \", to_text(DAY)]);
            let big = to_text(BIG);
            let folded = concat([\"a\", \"b\"]);
        }";
    let (db, program) = setup(source);
    let std = std_table(&db, program);
    let sigs = program_signatures(&db, program);
    let p = item(&db, program, "p");
    let mut driver = Driver {
        db: &db,
        program,
        ctx: Ctx::new(&db, program, &std, sigs),
        out: ConstEval::default(),
        texts: Vec::new(),
        text_set: Default::default(),
    };
    driver.run();
    let ab = ConstValue::Text(Text::Concat(imbl::vector![
        Text::plain("a"),
        Text::plain("b")
    ]));
    let mir = crate::mir::item_mir(&db, program, p).as_ref().unwrap();
    let promoted: Vec<ConstValue> = (0..mir.promoted.len())
        .map(|index| {
            let index = crate::thir::PromotedId::from_usize(index).unwrap();
            promoted_value(&db, program, p, index).clone().unwrap()
        })
        .collect();
    assert_eq!(promoted, vec![ab]);

    let by_len = |len: usize| {
        driver
            .out
            .tables
            .values()
            .filter(|table| table.rows.len() == len)
            .count()
    };
    // `to_text(DAY)` has 3 rows, and so has the `concat` over its results.
    assert_eq!(driver.out.tables.len(), 2);
    assert_eq!(by_len(3), 2);
    let concat = driver
        .out
        .tables
        .values()
        .find(|table| matches!(table.rows[0].1, ConstValue::Text(Text::Concat(_))))
        .unwrap();
    assert_eq!(
        concat.rows[1],
        (
            vec![ConstValue::Array(imbl::vector![
                ConstValue::Text(Text::plain("Day ")),
                ConstValue::Text(Text::plain("2")),
            ])],
            ConstValue::Text(Text::Concat(imbl::vector![
                Text::plain("Day "),
                Text::plain("2")
            ])),
        )
    );
    // `to_text(BIG)` has more inputs than the cap.
    let codes: Vec<DiagnosticCode> = driver
        .out
        .diagnostics
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert_eq!(codes, vec![DiagnosticCode::EvalDomainTooLarge]);
    // Texts computed by tables are texts of the program.
    assert!(driver.texts.contains(&Text::plain("3")));
}

#[test]
fn overflow_in_duration_code_is_a_duration_overflow() {
    let (db, program) = setup("const D: u32 = 1u32;");
    let std = std_table(&db, program);
    let sigs = ProgramSignatures::default();
    let mut driver = Driver {
        db: &db,
        program,
        ctx: Ctx::new(&db, program, &std, &sigs),
        out: ConstEval::default(),
        texts: Vec::new(),
        text_set: Default::default(),
    };
    let error = |file| EvalError {
        kind: ErrorKind::Fault(Fault::new(
            DiagnosticCode::EvalOverflow,
            "arithmetic overflow",
        )),
        at: Some(crate::diagnostic::Anchor::Text(file, TextRange::default())),
    };
    let root = *program.root(&db);
    let anchor = crate::diagnostic::Anchor::Text(root, TextRange::new(1.into(), 2.into()));
    driver.report(true, anchor, &error(std.time_file), None);
    driver.report(true, anchor, &error(root), Some(" (with arguments `(1)`)"));
    driver.report(false, anchor, &error(root), None);
    let diags = &driver.out.diagnostics;
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].code, DiagnosticCode::EvalDurationOverflow);
    assert_eq!(diags[0].related.len(), 1);
    assert_eq!(diags[1].code, DiagnosticCode::EvalOverflow);
    assert_eq!(
        diags[1].message,
        "arithmetic overflow (with arguments `(1)`)"
    );
}

// ----- End to end, with inference ------------------------------------------------------

const GAME: &str = "
use std::stage::{Character, character};
use std::screen::shader::{ColorShader, filter};

const A: EntityId = EntityId(3);
const IMG: ImageAsset = asset(\"a.png\");
const D: Duration = secs(2).plus(ms(250)).times(2).div(3);
const J: Motion = jump(20, ms(300));
const Q: Motion = quake(8, 4, ms(200));
const SEPIA: ColorShader<()> = asset(\"sepia.wgsl\");
const COLOR: Color = hex(0x382010);
static PETALS: Setting<bool> = setting(true);
static mut DAY: Int<1, 3> = 2;
static mut MET: bool = false;
const CH: Character = character(A).offset(offset(10, -5));

struct Board<const N: u64> { x: u64 }
impl<const N: u64> Board<N> {
    const CELLS: u64 = N * N;
    const fn new() -> Board<N> { Board { x: 1 } }
    const fn cells(self) -> u64 { Self::CELLS + self.x }
}
const CELLS: u64 = Board::<3>::new().cells();
const fn closures() -> u32 { let add = |x: u32| x + 1; let k = 5u32; let times = |y: u32| y * k; add(2) + times(3) }
const CLOSURES: u32 = closures();
const fn fields() -> i32 { let mut p = point(1, 2); p.x = p.x + p.y * 10; let r = &mut p; r.y = 7; p.x + p.y }
const FIELDS: i32 = fields();

enum Mood { Warm, Polite, Stranger }
const fn mood(day: Int<1, 3>, met: bool) -> Mood {
    match (day, met) { (3, true) => Mood::Warm, (_, true) => Mood::Polite, _ => Mood::Stranger }
}

const fn petals(mut screen: Screen) -> Screen {
    screen.entity(A).gate(Some(PETALS));
    screen.entity(A).image().change(IMG).apply_animation(fade(ms(500)));
    screen.entity(A).apply_animation(J);
    screen.camera().apply_animation(Q);
    screen.filter().change(Some(filter(SEPIA, ()))).apply_animation(tween(D));
    CH.show(&mut screen, IMG, point(100, 200), tween(ms(300)));
    screen
}
const PETALS_SCREEN: Screen = petals(Screen::empty());
const fn fresh(_screen: Screen) -> Screen {
    let mut screen = Screen::empty();
    screen.background().change(IMG).apply_animation(cut());
    screen
}

#[entry]
#[scene]
pushdown fn Start(met: bool) {
    MET = met;
    s!(render = petals, t![\"Day \" {DAY} \"!\"]);
    s!(render = if MET { petals } else { fresh }, t![\"x\"]);
    s!(render = |screen| screen, t![\"identity\"]);
    s!(render = |_screen| Screen::empty(), t![\"empty\"]);
    DAY += 1;
    match mood(DAY, MET) {
        Mood::Warm => s!(t![\"w\"]),
        _ => s!(t![\"o\"]),
    }
}
";

/// Runs on the default test thread (2 MB of stack): evaluation grows its stack on demand.
#[test]
fn deep_recursion_grows_the_stack() {
    let source = "
        const fn deep(n: u64) -> u64 { if n == 0 { 0 } else { deep(n - 1) + 1 } }
        const DEEP: u64 = deep(5000);
        const fn forever(n: u64) -> u64 { forever(n + 1) }
        const FOREVER: u64 = forever(0);
    ";
    let (db, program) = setup(source);
    let eval = const_eval(&db, program);
    assert_eq!(
        eval.consts[&item(&db, program, "DEEP")],
        ConstValue::Int(5000)
    );
    let messages: Vec<(DiagnosticCode, &str)> = eval
        .diagnostics
        .iter()
        .map(|diag| (diag.code, diag.message.as_str()))
        .collect();
    assert_eq!(
        messages,
        vec![(
            DiagnosticCode::EvalBudgetExceeded,
            "recursion too deep: more than 10000 nested calls during constant evaluation"
        )]
    );
}

#[test]
fn rat_builtins() {
    let source = "
        const R: Rat = Rat::new(2, -4);
        const D: Decimal = Rat::new(1, 3).to_decimal();
        const Z: Rat = Rat::new(1, 0);
    ";
    let (db, program) = setup(source);
    let eval = const_eval(&db, program);
    let value = |name: &str| eval.consts.get(&item(&db, program, name)).cloned();
    assert_eq!(
        value("R").map(|value| display_value(&db, &value)),
        Some("-1/2".to_owned())
    );
    assert_eq!(value("D"), Some(ConstValue::Decimal(dec("0.3333333"))));
    let codes: Vec<DiagnosticCode> = eval.diagnostics.iter().map(|diag| diag.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::EvalDivisionByZero]);
}

#[test]
fn constant_cycles_are_errors() {
    let source = "
        const A: u32 = A;
        const B: u32 = C + 1;
        const C: u32 = B;
        const D: u32 = A + 1;
        const E: u32 = 3;
    ";
    let (db, program) = setup(source);
    let eval = const_eval(&db, program);
    let reported: Vec<(DiagnosticCode, &str)> = eval
        .diagnostics
        .iter()
        .map(|diag| (diag.code, diag.message.as_str()))
        .collect();
    // One error per constant in a cycle, named after it; a constant that only uses a
    // failed one (`D`) adds nothing.
    assert_eq!(
        reported,
        vec![
            (
                DiagnosticCode::EvalConstCycle,
                "the value of `A` depends on itself, so it cannot be computed at compile time"
            ),
            (
                DiagnosticCode::EvalConstCycle,
                "the value of `B` depends on itself, so it cannot be computed at compile time"
            ),
            (
                DiagnosticCode::EvalConstCycle,
                "the value of `C` depends on itself, so it cannot be computed at compile time"
            ),
        ]
    );
    assert_eq!(
        eval.consts.get(&item(&db, program, "E")),
        Some(&ConstValue::Int(3))
    );
}

#[test]
fn evaluates_a_game_end_to_end() {
    let (db, program) = setup_at("/game/main.kix", GAME);
    let diags: Vec<String> = check(&db, program)
        .iter()
        .map(|diag| format!("{}: {}", diag.code.as_str(), diag.message))
        .collect();
    assert!(diags.is_empty(), "{diags:#?}");
    let std = std_table(&db, program);
    let eval = const_eval(&db, program);
    let value = |name: &str| eval.consts[&item(&db, program, name)].clone();
    assert_eq!(value("D"), std.duration_value(1500));
    assert_eq!(value("CELLS"), ConstValue::Int(10));
    assert_eq!(value("CLOSURES"), ConstValue::Int(18));
    assert_eq!(value("FIELDS"), ConstValue::Int(28));
    let color = value("COLOR");
    assert_eq!(
        color,
        std.color
            .make([0x38, 0x20, 0x10, 255].map(ConstValue::Int).to_vec())
    );
    let ConstValue::Adt { fields: keys, .. } = value("Q") else {
        panic!("Motion");
    };
    assert!(matches!(&keys[0], ConstValue::Vec(keys) if keys.len() == 4));
    // Statics: the initial value of a story variable, the default of a handle.
    let petals = item(&db, program, "PETALS");
    assert_eq!(eval.statics[&item(&db, program, "DAY")], ConstValue::Int(2));
    assert_eq!(eval.statics[&petals], ConstValue::Bool(true));

    // The screen `petals` builds.
    let ConstValue::Screen(screen) = value("PETALS_SCREEN") else {
        panic!("not a screen");
    };
    let entity = &screen.logical.entities[&3];
    assert_eq!(
        entity.gate,
        std.some(ConstValue::Handle {
            kind: HandleKind::Setting,
            item: petals,
        })
    );
    assert_eq!(entity.parts.get(&0), Some(&image("/game/a.png")));
    assert_eq!(
        entity.position,
        std.point
            .make(vec![ConstValue::Int(110), ConstValue::Int(195)])
    );
    assert_eq!(entity.opacity, ConstValue::Decimal(Decimal::ONE));
    let Some(Some(ConstValue::Adt { fields, .. })) = std.as_option(&screen.logical.filter) else {
        panic!("no screen filter");
    };
    assert_eq!(
        fields.iter().cloned().collect::<Vec<_>>(),
        vec![
            ConstValue::Asset {
                kind: AssetKind::Shader(kagari_shader::ShaderRole::Color),
                path: FilePath::disk("/game/sepia.wgsl"),
            },
            ConstValue::Tuple(imbl::Vector::new()),
        ]
    );
    let motions = screen
        .step
        .scheduled
        .iter()
        .filter(|scheduled| matches!(scheduled.kind, ScheduledKind::Motion(_)))
        .count();
    assert_eq!(motions, 2);

    // Renders: `fresh` and the closure building an empty screen are keyframes.
    let mut renders: Vec<(String, bool)> = eval
        .renders
        .iter()
        .map(|(id, info)| {
            let name = match id {
                RenderId::Fn(item) => super::value::item_name(&db, *item),
                RenderId::Closure(owner, expr) => {
                    let range = body_source_map_of(&db, *owner).unwrap().expr(*expr);
                    GAME[usize::from(range.start())..usize::from(range.end())].to_owned()
                }
            };
            (name, info.keyframe)
        })
        .collect();
    renders.sort();
    assert_eq!(
        renders,
        vec![
            ("fresh".to_owned(), true),
            ("petals".to_owned(), false),
            ("|_screen| Screen::empty()".to_owned(), true),
            ("|screen| screen".to_owned(), false),
        ]
    );

    // Tables: `to_text(DAY)`, the `concat` over it, and `mood(DAY, MET)`.
    let mut sizes: Vec<usize> = eval.tables.values().map(|table| table.rows.len()).collect();
    sizes.sort();
    assert_eq!(sizes, vec![3, 3, 6]);
    let mood = eval
        .tables
        .values()
        .find(|table| table.rows.len() == 6)
        .unwrap();
    let mood_item = item(&db, program, "Mood");
    let warm = |day: i128, met: bool| {
        mood.rows
            .iter()
            .find(|(inputs, _)| inputs == &vec![ConstValue::Int(day), ConstValue::Bool(met)])
            .map(|(_, output)| output.clone())
    };
    assert_eq!(
        warm(3, true),
        Some(ConstValue::Adt {
            item: mood_item,
            variant: 0,
            fields: imbl::vector![]
        })
    );
    assert_eq!(
        warm(1, false),
        Some(ConstValue::Adt {
            item: mood_item,
            variant: 2,
            fields: imbl::vector![]
        })
    );
}

const SHADERS: &str = "
use std::screen::{Pose, degrees, pose, size};
use std::screen::shader::{
    ColorShader, TextureShader, TransitionalMotionShader, filter, paint, transitional_motion,
};

#[shader_arg]
struct Amount { amount: Decimal }

const SEPIA: ColorShader<Amount> = asset(\"sepia.wgsl\");
const SPOTS: TextureShader<()> = asset(\"sepia.wgsl\");
const PATH: TransitionalMotionShader<()> = asset(\"sepia.wgsl\");
const A: EntityId = EntityId(3);

const fn tinted(amount: Decimal) -> Screen {
    let mut screen = Screen::empty();
    screen.filter().change(Some(filter(SEPIA, Amount { amount }))).apply_animation(instant());
    screen
}
const HALF: Screen = tinted(0.5);
const FULL: Screen = tinted(1.0);
const HALF_AGAIN: Screen = tinted(0.5);

const fn travelled() -> Screen {
    let mut screen = Screen::empty();
    screen.entity(A).position().change(point(10, 20)).apply_animation(instant());
    screen.entity(A).travel(
        pose(point(900, 400), 2.0, degrees(90.0)),
        transitional_motion(PATH, (), ms(1200)).delay(ms(100)),
    );
    screen
}
const TRAVELLED: Screen = travelled();

const fn painted() -> Screen {
    let mut screen = Screen::empty();
    screen.entity(A).image().paint(paint(SPOTS, (), size(400, 300))).apply_animation(cut());
    screen
}
const PAINTED: Screen = painted();
const fn painted_get() -> bool {
    let mut screen = painted();
    match screen.entity(A).image().get() {
        None => true,
        Some(_) => false,
    }
}
const PAINTED_GET: bool = painted_get();
";

fn shader_screen(eval: &ConstEval, db: &RootDatabase, program: Program, name: &str) -> Screen {
    match &eval.consts[&item(db, program, name)] {
        ConstValue::Screen(screen) => (**screen).clone(),
        other => panic!("{name} is not a screen: {other:?}"),
    }
}

#[test]
fn shader_effects_on_the_screen() {
    let (db, program) = setup_at("/game/main.kix", SHADERS);
    let diags: Vec<String> = check(&db, program)
        .iter()
        .map(|diag| format!("{}: {}", diag.code.as_str(), diag.message))
        .collect();
    assert!(diags.is_empty(), "{diags:#?}");
    let std = std_table(&db, program);
    let eval = const_eval(&db, program);

    // Screens differing only in filter arguments differ.
    let half = shader_screen(eval, &db, program, "HALF");
    // `erase_shader` and `erase_args` are the identity.
    let Some(Some(ConstValue::Adt { fields, .. })) = std.as_option(&half.logical.filter) else {
        panic!("no screen filter");
    };
    assert_eq!(
        fields[0],
        ConstValue::Asset {
            kind: AssetKind::Shader(kagari_shader::ShaderRole::Color),
            path: FilePath::disk("/game/sepia.wgsl"),
        }
    );
    assert!(
        matches!(&fields[1], ConstValue::Adt { fields, .. } if fields[0] == ConstValue::Decimal(Decimal::new(5, 1)))
    );
    assert_ne!(half, shader_screen(eval, &db, program, "FULL"));
    assert_eq!(half, shader_screen(eval, &db, program, "HALF_AGAIN"));

    // `travel` sets the logical pose and schedules three tweens and the path.
    let travelled = shader_screen(eval, &db, program, "TRAVELLED");
    let entity = &travelled.logical.entities[&3];
    assert_eq!(
        entity.position,
        std.point
            .make(vec![ConstValue::Int(900), ConstValue::Int(400)])
    );
    assert_eq!(entity.scale, ConstValue::Decimal(Decimal::from_int(2)));
    let pose_tweens = travelled
        .step
        .scheduled
        .iter()
        .filter(|scheduled| {
            matches!(
                (&scheduled.kind, scheduled.target),
                (
                    ScheduledKind::Tween(_),
                    Some(Target::Prop(PropTarget::Entity(
                        3,
                        EntityProp::Position | EntityProp::Scale | EntityProp::Rotation
                    )))
                )
            ) && scheduled.start.offset_ms == 100
        })
        .count();
    assert_eq!(pose_tweens, 3);
    let travels: Vec<&Scheduled> = travelled
        .step
        .scheduled
        .iter()
        .filter(|scheduled| matches!(scheduled.kind, ScheduledKind::Travel { .. }))
        .collect();
    assert_eq!(travels.len(), 1);
    assert_eq!(travels[0].target, Some(Target::Travel(3)));
    assert_eq!(travels[0].start.offset_ms, 100);
    let ScheduledKind::Travel { from, .. } = &travels[0].kind else {
        unreachable!()
    };
    assert_eq!(
        from.field(0),
        Some(
            &std.point
                .make(vec![ConstValue::Int(10), ConstValue::Int(20)])
        )
    );

    // `paint` swaps a part; `ImageLayer::get` is `None` on it.
    let painted = shader_screen(eval, &db, program, "PAINTED");
    assert!(matches!(
        painted.logical.entities[&3].parts.get(&0),
        Some(ConstValue::Adt { .. })
    ));
    assert_eq!(
        eval.consts[&item(&db, program, "PAINTED_GET")],
        ConstValue::Bool(true)
    );
}
