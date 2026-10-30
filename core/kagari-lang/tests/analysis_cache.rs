//! The persisted story graph, and reusing the story analysis across runs through the
//! analysis cache.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use kagari_lang::analysis::story_graph;
use kagari_lang::cache::{AnalysisCacheUse, analysis_cache_use, offer_analysis_cache};
use kagari_lang::kagari_debug::{AnalysisCache, NodeKind, Value};
use kagari_lang::{DiagnosticCode, FilePath, MemoryFileSystem, Program, RootDatabase, check};
use salsa::{Database, EventKind};

const MAIN: &str = "/game/main.kix";

const STORY: &str = r#"
static mut SEEN: bool = false;

enum Pick { A, B }
enum Coin { Heads, Tails }
enum Count { One, Two }

#[scene]
#[entry]
pushdown fn Start() {
    s!(t!["hello"]);
    match choice!{
        id = "pick",
        t!["A"] => Pick::A,
        t!["B"] if SEEN => Pick::B,
    } {
        Pick::A => Next(),
        Pick::B => {}
    }
    let count = choice!{ id = "count", t!["one"] => Count::One, t!["two"] => Count::Two };
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t!["heads"]),
        Coin::Tails => s!(t!["tails"]),
    }
    if count == Count::Two {
        s!(t!["two"]);
    }
}

#[scene]
pushdown fn Next() {
    SEEN = true;
    s!(t!["next"]);
}

#[scene]
pushdown fn Never() {
    s!(t!["never"]);
}
"#;

type Executed = Arc<Mutex<Vec<salsa::DatabaseKeyIndex>>>;

fn database(source: &str, executed: Option<Executed>) -> (RootDatabase, Program) {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(PathBuf::from(MAIN), source.to_owned());
    let db = match executed {
        Some(sink) => RootDatabase::with_event_callback(
            Arc::new(fs),
            Box::new(move |event| {
                if let EventKind::WillExecute { database_key } = event.kind {
                    sink.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(database_key);
                }
            }),
        ),
        None => RootDatabase::new(Arc::new(fs)),
    };
    let program = db.program(FilePath::disk(MAIN));
    (db, program)
}

/// Whether the story analysis query ran.
fn analyzed(db: &RootDatabase, executed: &Executed) -> bool {
    executed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|key| {
            db.ingredient_debug_name(key.ingredient_index())
                .starts_with("story_analysis")
        })
}

/// The cache a fresh run writes, round-tripped through its file bytes.
fn fresh_cache(source: &str) -> AnalysisCache {
    let (db, program) = database(source, None);
    check(&db, program);
    let AnalysisCacheUse::Fresh(cache) = analysis_cache_use(&db, program) else {
        panic!("the analysis should have run");
    };
    AnalysisCache::from_bytes(cache.to_bytes().unwrap()).unwrap()
}

#[test]
fn the_graph_names_steps_arms_draws_and_locals() {
    let (db, program) = database(STORY, None);
    let errors: Vec<_> = check(&db, program)
        .iter()
        .filter(|diag| diag.code.as_str().starts_with('E'))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let graph = story_graph(&db, program).unwrap();
    assert!(graph.complete);
    assert_eq!(graph.vars, ["crate::SEEN"]);
    let steps: Vec<String> = graph.reached_steps().into_iter().collect();
    assert_eq!(
        steps,
        [
            "crate::Next#0",
            "crate::Start#0",
            "crate::Start#1",
            "crate::Start#2",
            "crate::Start#3",
            "crate::Start#4",
            "crate::Start#5",
        ],
        "every step but `Never`'s is reached"
    );

    let mut pick_arms = Vec::new();
    let mut count_arms = Vec::new();
    let mut draws = Vec::new();
    let mut counts = Vec::new();
    for (context, node) in graph.reached() {
        let data = graph.node(context, node).unwrap();
        match &data.kind {
            NodeKind::Choice { step: 1, arms, .. } => {
                pick_arms.push(arms.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>());
            }
            NodeKind::Choice { step: 2, arms, .. } => {
                count_arms = arms.iter().map(|(id, _)| id.clone()).collect();
            }
            NodeKind::Draw { draws: drawn } => {
                draws = drawn
                    .iter()
                    .map(|(value, _)| graph.value(*value).cloned().unwrap())
                    .collect();
                for (name, value) in graph.locals(context, node) {
                    if name == "count" {
                        counts.push(value.cloned().unwrap());
                    }
                }
            }
            _ => {}
        }
    }
    let variant = |ty: &str, variant: &str| Value::Adt {
        ty: format!("crate::{ty}"),
        variant: variant.to_owned(),
        fields: Vec::new(),
    };
    // `B` is hidden until `Next` has set `SEEN`, which only `A` leads to; arm IDs are
    // variant paths.
    assert_eq!(pick_arms, [vec!["crate::Pick::A".to_owned()]]);
    assert_eq!(count_arms, ["crate::Count::One", "crate::Count::Two"]);
    assert_eq!(draws, [variant("Coin", "Heads"), variant("Coin", "Tails")]);
    // The draw is reached once per value of the kept `count`.
    counts.sort_by_key(|value| format!("{value:?}"));
    assert_eq!(counts, [variant("Count", "One"), variant("Count", "Two")]);
}

#[test]
fn the_graph_flags_keyframe_renders() {
    let source = r#"
enum Answer { Yes, No }
const A: EntityId = EntityId(1);
const fn delta(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(0.5).apply_animation(instant());
    screen
}
const fn key(_screen: Screen) -> Screen {
    Screen::empty()
}

#[entry]
#[scene]
pushdown fn Start() {
    s!(render = key, t!["a"]);
    s!(render = delta, t!["b"]);
    let _ = choice!{ id = "c", render = key, t!["Yes"] => Answer::Yes, t!["No"] => Answer::No };
    s!(t!["c"]);
}
"#;
    // Without the opt-in keyframe check: the graph flags keyframes regardless.
    let (db, program) = database(source, None);
    let errors: Vec<_> = check(&db, program)
        .iter()
        .filter(|diag| diag.code.as_str().starts_with('E'))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let graph = story_graph(&db, program).unwrap();
    let mut renders = Vec::new();
    for (context, node) in graph.reached() {
        let (step, render) = match &graph.node(context, node).unwrap().kind {
            NodeKind::Say { step, render, .. } | NodeKind::Choice { step, render, .. } => {
                (*step, *render)
            }
            _ => continue,
        };
        let flagged = render.map(|id| (graph.value(id).cloned().unwrap(), graph.is_keyframe(id)));
        renders.push((step, flagged));
    }
    let render = |name: &str, keyframe| Some((Value::Fn(format!("crate::{name}")), keyframe));
    assert_eq!(
        renders,
        [
            (0, render("key", true)),
            (1, render("delta", false)),
            (2, render("key", true)),
            (3, None),
        ]
    );
    assert_eq!(graph.keyframes.len(), 1, "one keyframe render value");
}

#[test]
fn an_unchanged_program_reuses_the_cached_analysis() {
    let cache = fresh_cache(STORY);
    let unreachable = |diags: &[kagari_lang::Diagnostic]| {
        diags
            .iter()
            .filter(|diag| diag.code == DiagnosticCode::AnalysisUnreachableScene)
            .count()
    };

    let executed = Executed::default();
    let (mut db, program) = database(STORY, Some(executed.clone()));
    assert!(offer_analysis_cache(&mut db, program, &cache));
    let reused = check(&db, program).clone();
    assert_eq!(
        unreachable(&reused),
        1,
        "the cached `Never` warning is kept"
    );
    assert!(!analyzed(&db, &executed), "the story analysis must not run");
    assert!(matches!(
        analysis_cache_use(&db, program),
        AnalysisCacheUse::Reused
    ));

    // Without the offer, the same diagnostics come from running the analysis.
    let (db, program) = database(STORY, None);
    assert_eq!(check(&db, program), &reused);
}

#[test]
fn a_changed_source_runs_the_analysis_again() {
    let cache = fresh_cache(STORY);
    let changed = STORY.replace("pushdown fn Never()", "#[entry]\npushdown fn Never()");

    let executed = Executed::default();
    let (mut db, program) = database(&changed, Some(executed.clone()));
    assert!(offer_analysis_cache(&mut db, program, &cache));
    let diags = check(&db, program);
    assert!(
        !diags
            .iter()
            .any(|diag| diag.code == DiagnosticCode::AnalysisUnreachableScene),
        "`Never` is an entry scene now: {diags:#?}"
    );
    assert!(
        analyzed(&db, &executed),
        "the story analysis must run again"
    );
    let AnalysisCacheUse::Fresh(fresh) = analysis_cache_use(&db, program) else {
        panic!("the stale cache must not be reused");
    };
    assert_ne!(fresh.fingerprint, cache.fingerprint);
    assert!(
        fresh
            .story
            .unwrap()
            .reached_steps()
            .contains("crate::Never#0")
    );
}

#[test]
fn errors_in_earlier_stages_skip_the_analysis_and_its_cache() {
    let broken = STORY.replace("SEEN = true;", "SEEN = 1;");
    let (db, program) = database(&broken, None);
    check(&db, program);
    assert!(matches!(
        analysis_cache_use(&db, program),
        AnalysisCacheUse::NotRun
    ));
}
