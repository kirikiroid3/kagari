#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

use kagari_archive::save::{Save, Scalar};

use crate::{
    AnalysisCache, Caller, Context, DebugArtifact, DebugInfo, DiagnosticRecord, Error, Exit,
    FileKind, FunctionDebug, Incomplete, LocalDebug, Node, NodeKind, Point, RegisterRange,
    RelatedRecord, SCHEMA, Scene, ScopeDebug, Severity, SourceRef, StaticDebug, StoryGraph, Target,
    Text, Value,
};

fn every_value() -> Vec<Value> {
    vec![
        Value::unit(),
        Value::Bool(true),
        Value::Int(-(1_i128 << 100)),
        Value::Decimal {
            mantissa: 12345,
            scale: 2,
        },
        Value::Rat { num: -1, den: 3 },
        Value::String("Mio".to_owned()),
        Value::Text(Text::Plain("plain".to_owned())),
        Value::Text(Text::Concat(vec![
            Text::Ruby {
                base: Box::new(Text::Plain("漢".to_owned())),
                reading: Box::new(Text::Plain("かん".to_owned())),
            },
            Text::MonoRuby {
                base: Box::new(Text::Plain("ab".to_owned())),
                readings: vec![Text::Plain("a".to_owned()), Text::Plain("b".to_owned())],
            },
            Text::Effect {
                base: Box::new(Text::Plain("x".to_owned())),
                effect: Box::new(Value::Adt {
                    ty: "std::text::TextEffect".to_owned(),
                    variant: "Shake".to_owned(),
                    fields: vec![Value::Int(2)],
                }),
            },
        ])),
        Value::Array(vec![Value::Int(1), Value::Int(2)]),
        Value::Vec(vec![Value::Tuple(vec![Value::Bool(false)])]),
        Value::Adt {
            ty: "crate::Mood".to_owned(),
            variant: "Warm".to_owned(),
            fields: Vec::new(),
        },
        Value::Asset("bgimage/room.png".to_owned()),
        Value::Handle("crate::VOLUME".to_owned()),
        Value::Fn("crate::render_room".to_owned()),
        Value::Closure("crate::start::{closure#0}".to_owned()),
        Value::Screen,
    ]
}

/// Two entry contexts calling a shared scene; one unreached context and node.
///
/// ```text
/// ctx 0 crate::start:   n0 say#0 → n1 call ctx 2 ─exit 0→ n2 choice#1 ─A→ n3 draw ─1→ exit 0
///                                                          └─#1→ dead      └─2→ n0
/// ctx 1 crate::other:   n0 say#0 → exit 0
/// ctx 2 crate::shared:  n0 say#0 → exit 0;  n1 say#1 (no path reaches it)
/// ctx 3 crate::unused:  never called
/// ```
fn sample_graph() -> StoryGraph {
    let say = |step, next| NodeKind::Say {
        step,
        render: None,
        next,
    };
    let node = |kind| Node {
        block: 1,
        frame: 0,
        globals: 0,
        kind,
    };
    let context = |scene, path: &str, nodes| Context {
        scene,
        path: path.to_owned(),
        args: Vec::new(),
        globals: 0,
        entry: Target::Node(0),
        nodes,
        exits: vec![Exit {
            ret: 0,
            globals: 0,
            outs: vec![1],
        }],
        callers: Vec::new(),
    };
    let mut values = every_value();
    values.push(Value::Int(1));
    values.push(Value::Int(2));
    let one = u32::try_from(values.len() - 2).unwrap();
    let two = one + 1;
    let mut start = context(
        0,
        "crate::start",
        vec![
            node(say(0, Target::Node(1))),
            node(NodeKind::Call {
                callee: 2,
                exits: vec![(0, Target::Node(2))],
            }),
            node(NodeKind::Choice {
                step: 1,
                render: Some(13),
                arms: vec![
                    ("crate::Pick::A".to_owned(), Target::Node(3)),
                    ("#1".to_owned(), Target::Dead),
                ],
            }),
            node(NodeKind::Draw {
                draws: vec![(one, Target::Exit(0)), (two, Target::Node(0))],
            }),
        ],
    );
    start.args = vec![1, 2];
    let mut shared = context(
        1,
        "crate::shared",
        vec![node(say(0, Target::Exit(0))), node(say(1, Target::Exit(0)))],
    );
    shared.callers = vec![Caller {
        context: 0,
        node: 1,
    }];
    StoryGraph {
        complete: false,
        incomplete: vec![Incomplete {
            code: "W5007".to_owned(),
            message: "the story analysis cannot follow the story past this point".to_owned(),
        }],
        vars: vec!["crate::AFFECTION".to_owned()],
        values,
        frames: vec![vec![Some(2), None]],
        globals: vec![vec![2]],
        scenes: vec![
            Scene {
                path: "crate::start".to_owned(),
                points: vec![Point {
                    block: 1,
                    locals: vec!["mood".to_owned(), String::new()],
                }],
            },
            Scene {
                path: "crate::shared".to_owned(),
                points: Vec::new(),
            },
        ],
        contexts: vec![
            start,
            context(0, "crate::other", vec![node(say(0, Target::Exit(0)))]),
            shared,
            context(1, "crate::unused", vec![node(say(0, Target::Exit(0)))]),
        ],
        roots: vec![0, 1],
        // `crate::render_room`; the closure (14) is a delta render.
        keyframes: vec![13],
    }
}

fn sample_debug() -> DebugInfo {
    DebugInfo {
        functions: vec![
            FunctionDebug {
                path: "crate::start".to_owned(),
                file: Some("src/main.kix".to_owned()),
                body_hash: "ab".repeat(32),
                scopes: vec![
                    ScopeDebug {
                        parent: None,
                        start: 10,
                        end: 200,
                    },
                    ScopeDebug {
                        parent: Some(0),
                        start: 40,
                        end: 90,
                    },
                ],
                locals: vec![LocalDebug {
                    name: "mood".to_owned(),
                    scope: 1,
                    ranges: vec![
                        RegisterRange {
                            start: 0,
                            end: 6,
                            register: 3,
                            cell: false,
                        },
                        RegisterRange {
                            start: 9,
                            end: 12,
                            register: 4,
                            cell: true,
                        },
                    ],
                }],
            },
            FunctionDebug {
                path: "crate::Pick::A".to_owned(),
                ..FunctionDebug::default()
            },
        ],
        statics: vec![StaticDebug {
            path: "crate::TRUST".to_owned(),
            ty: "Int<0, 9>".to_owned(),
        }],
    }
}

fn sample_cache() -> AnalysisCache {
    AnalysisCache {
        toolchain: "0.1.0".to_owned(),
        fingerprint: "00".repeat(32),
        diagnostics: vec![DiagnosticRecord {
            file: SourceRef::Disk("/game/src/main.kix".to_owned()),
            start: 4,
            end: 9,
            severity: Severity::Warning,
            code: "W5009".to_owned(),
            message: "this step is never reached".to_owned(),
            related: vec![RelatedRecord {
                file: SourceRef::Std("narrative.kis".to_owned()),
                start: 0,
                end: 0,
                message: "step `crate::start#3`".to_owned(),
            }],
        }],
        story: Some(sample_graph()),
    }
}

#[test]
fn analysis_cache_round_trips() {
    let cache = sample_cache();
    let bytes = cache.to_bytes().unwrap();
    assert_eq!(AnalysisCache::from_bytes(bytes.clone()).unwrap(), cache);
    // Deterministic: the same content gives the same bytes.
    assert_eq!(cache.to_bytes().unwrap(), bytes);

    let empty = AnalysisCache {
        diagnostics: Vec::new(),
        story: None,
        ..cache
    };
    let bytes = empty.to_bytes().unwrap();
    assert_eq!(AnalysisCache::from_bytes(bytes).unwrap(), empty);
}

#[test]
fn debug_artifact_round_trips() {
    let artifact = DebugArtifact {
        toolchain: "0.1.0".to_owned(),
        fingerprint: "11".repeat(32),
        script_hash: "22".repeat(32),
        story: Some(sample_graph()),
        debug: sample_debug(),
    };
    let bytes = artifact.to_bytes().unwrap();
    assert_eq!(DebugArtifact::from_bytes(bytes).unwrap(), artifact);
}

#[test]
fn walks_from_the_roots() {
    let graph = sample_graph();
    let reached = graph.reached();
    assert_eq!(
        reached,
        vec![(0, 0), (0, 1), (2, 0), (0, 2), (0, 3), (1, 0)],
        "the shared scene's second step and the unused context are not reached"
    );
    let steps: Vec<String> = graph.reached_steps().into_iter().collect();
    assert_eq!(
        steps,
        [
            "crate::other#0",
            "crate::shared#0",
            "crate::start#0",
            "crate::start#1"
        ]
    );
    assert_eq!(graph.step_id(0, 1), None, "a call node is not a step");
    assert_eq!(
        graph.locals(0, 0),
        vec![("mood", Some(&Value::Int(-(1_i128 << 100)))), ("", None)]
    );
    assert_eq!(
        graph.vars(0, 0),
        vec![("crate::AFFECTION", Some(&Value::Int(-(1_i128 << 100))))]
    );
    assert!(graph.is_keyframe(13));
    assert!(!graph.is_keyframe(14));
}

#[test]
fn rejects_another_kind_or_schema() {
    let bytes = sample_cache().to_bytes().unwrap();
    assert_eq!(
        DebugArtifact::from_bytes(bytes),
        Err(Error::Kind {
            expected: FileKind::Debug,
            found: "analysis".to_owned()
        })
    );

    let mut save = Save::from_bytes(sample_cache().to_bytes().unwrap()).unwrap();
    save.root_mut()
        .set("schema", Scalar::I64(SCHEMA + 1))
        .unwrap();
    assert_eq!(
        AnalysisCache::from_bytes(save.into_bytes()),
        Err(Error::Schema(SCHEMA + 1))
    );

    let mut save = Save::from_bytes(sample_cache().to_bytes().unwrap()).unwrap();
    save.root_mut().set("schema", Scalar::I64(1)).unwrap();
    assert_eq!(
        AnalysisCache::from_bytes(save.into_bytes()),
        Err(Error::Schema(1)),
        "schema 1 has no keyframes"
    );

    assert!(matches!(
        AnalysisCache::from_bytes(b"not a cache".to_vec()),
        Err(Error::Save(_))
    ));
}

#[test]
fn rejects_malformed_nodes() {
    let mut save = Save::from_bytes(sample_cache().to_bytes().unwrap()).unwrap();
    {
        let mut root = save.root_mut();
        let mut story = root.table("story").unwrap();
        let mut contexts = story.array("contexts").unwrap();
        let mut context = contexts.table_at(0).unwrap();
        let mut nodes = context.array("nodes").unwrap();
        let mut say = nodes.table_at(0).unwrap();
        // A `say` resumes exactly once.
        say.array("next").unwrap().push(Scalar::Null).unwrap();
    }
    assert_eq!(
        AnalysisCache::from_bytes(save.into_bytes()),
        Err(Error::Malformed("next"))
    );

    let mut save = Save::from_bytes(sample_cache().to_bytes().unwrap()).unwrap();
    save.root_mut()
        .table("story")
        .unwrap()
        .set("roots", Scalar::I64(0))
        .unwrap();
    assert_eq!(
        AnalysisCache::from_bytes(save.into_bytes()),
        Err(Error::Malformed("roots"))
    );

    let mut save = Save::from_bytes(sample_cache().to_bytes().unwrap()).unwrap();
    {
        let mut root = save.root_mut();
        let mut story = root.table("story").unwrap();
        // Keyframes are ascending and unique.
        story
            .array("keyframes")
            .unwrap()
            .push(Scalar::I64(13))
            .unwrap();
    }
    assert_eq!(
        AnalysisCache::from_bytes(save.into_bytes()),
        Err(Error::Malformed("keyframes"))
    );
}
