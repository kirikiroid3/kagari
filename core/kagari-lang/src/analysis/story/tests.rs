#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kagari_debug as kd;
use rustc_hash::FxHashMap;

use crate::db::{Db, MemoryFileSystem, OsFileSystem, RootDatabase};
use crate::diagnostic::{DiagnosticCode, Severity};
use crate::eval::std_items::std_table;
use crate::input::{CompileOptions, FilePath, Program};
use crate::item_tree::ItemId;
use crate::mir::Terminator;
use crate::pipeline::{analysis_runs, check};
use crate::signatures::program_signatures;

use super::explore::{self, StoryCx};
use super::shape::{self, Sharing};
use super::{Scenes, collect, graph, screens, termination};

const MAIN: &str = "/game/main.kix";

/// The `W5011` messages of `source`, compiled with or without the keyframe check.
fn missing_keyframes(source: &str, keyframe_check: bool) -> Vec<String> {
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from(MAIN), source.to_owned());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program_with(
        FilePath::disk(MAIN),
        CompileOptions {
            keyframe_check,
            ..Default::default()
        },
    );
    let diags = check(&db, program);
    let errors: Vec<&str> = diags
        .iter()
        .filter(|diag| diag.severity == Severity::Error)
        .map(|diag| diag.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    diags
        .iter()
        .filter(|diag| diag.code == DiagnosticCode::AnalysisMissingKeyframe)
        .map(|diag| diag.message.clone())
        .collect()
}

const RENDERS: &str = "
enum Answer { Yes, No }
const A: EntityId = EntityId(1);
const fn delta(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(0.5).apply_animation(instant());
    screen
}
const fn key(_screen: Screen) -> Screen {
    Screen::empty()
}
";

const CHOICE_MESSAGE: &str = "this choice has no keyframe render: loading or jumping here \
                              replays renders from an earlier keyframe";

#[test]
fn a_delta_choice_needs_a_keyframe() {
    let source = format!(
        "{RENDERS}
#[entry]
#[scene]
pushdown fn Start() {{
    s!(render = key, t![\"a\"]);
    let _ = choice!{{ id = \"c\", render = delta, t![\"Yes\"] => Answer::Yes, t![\"No\"] => Answer::No }};
}}
"
    );
    assert_eq!(missing_keyframes(&source, true), vec![CHOICE_MESSAGE]);
    assert_eq!(missing_keyframes(&source, false), Vec::<String>::new());
}

#[test]
fn a_keyframe_choice_passes() {
    let source = format!(
        "{RENDERS}
#[entry]
#[scene]
pushdown fn Start() {{
    s!(render = key, t![\"a\"]);
    let _ = choice!{{ id = \"c\", render = key, t![\"Yes\"] => Answer::Yes, t![\"No\"] => Answer::No }};
}}
"
    );
    assert_eq!(missing_keyframes(&source, true), Vec::<String>::new());
}

#[test]
fn the_first_step_of_an_entry_scene_needs_a_keyframe() {
    let source = format!(
        "{RENDERS}
#[entry]
#[scene]
pushdown fn Start() {{
    Opening();
    s!(render = delta, t![\"b\"]);
}}

#[scene]
pushdown fn Opening() {{
    s!(t![\"a\"]);
}}
"
    );
    assert_eq!(
        missing_keyframes(&source, true),
        vec!["the first step of entry scene `Start` has no keyframe render"]
    );
    assert_eq!(missing_keyframes(&source, false), Vec::<String>::new());
}

fn database(source: &str) -> RootDatabase {
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from(MAIN), source.to_owned());
    RootDatabase::new(Arc::new(MemoryFileSystem(files)))
}

/// The messages of the errors of `program`.
fn errors(db: &dyn Db, program: Program) -> Vec<String> {
    check(db, program)
        .iter()
        .filter(|diag| diag.severity == Severity::Error)
        .map(|diag| diag.message.clone())
        .collect()
}

/// The scenes and the story variables of `program`, which must pass stages 1–4.
fn scenes_of<'db>(db: &'db dyn Db, program: Program) -> (Scenes<'db>, Vec<ItemId>) {
    assert!(analysis_runs(db, program), "{:#?}", errors(db, program));
    collect(db, program, program_signatures(db, program))
}

/// The shape of scene `name` of `source`: whether its footprint holds each story variable
/// (`None`: it holds all of them), and per region its parent, the ordinals of the steps of
/// its blocks and whether story variables are part of its key.
#[allow(clippy::type_complexity)]
fn shape_of(source: &str, name: &str) -> (Option<Vec<bool>>, Vec<(Option<u32>, Vec<u32>, bool)>) {
    let db = database(source);
    let program = db.program(FilePath::disk(MAIN));
    let (scenes, vars) = scenes_of(&db, program);
    let cx = StoryCx {
        db: &db,
        program,
        std: std_table(&db, program).as_ref().unwrap(),
        sigs: program_signatures(&db, program),
    };
    let shapes = shape::shapes(cx, &scenes, &vars, Sharing::On);
    let (scene, shape) = scenes
        .list
        .iter()
        .zip(&shapes)
        .find(|(scene, _)| scene.name(&db) == name)
        .unwrap();
    let footprint = shape
        .footprint
        .as_ref()
        .map(|slots| (0..vars.len()).map(|slot| slots.contains(slot)).collect());
    let regions = shape
        .regions
        .iter()
        .map(|region| {
            let mut steps: Vec<u32> = region
                .blocks
                .iter()
                .filter_map(|block| match scene.mir.main.block(*block)?.term {
                    Terminator::Step { id, .. } => Some(id.0),
                    _ => None,
                })
                .collect();
            steps.sort_unstable();
            (region.parent, steps, !region.key_vars.is_empty())
        })
        .collect();
    (footprint, regions)
}

#[test]
fn steps_that_write_nothing_form_one_region() {
    // The choice heads the region; the last step returns, so it is outside.
    let source = "
enum Pick { A, B }

#[scene]
#[entry]
pushdown fn Start() {
    match choice!{ id = \"c\", t![\"A\"] => Pick::A, t![\"B\"] => Pick::B } {
        Pick::A => s!(t![\"a\"]),
        Pick::B => s!(t![\"b\"]),
    }
    s!(t![\"m\"]);
    s!(t![\"n\"]);
}
";
    assert_eq!(
        shape_of(source, "Start"),
        (Some(vec![]), vec![(None, vec![0, 1, 2, 3], false)])
    );
}

#[test]
fn lines_between_writes_and_reads_form_nested_regions() {
    // The choices write and the endings return. The lines are one region keyed by the flags
    // its last segment reads (the `if`); its lines but the last read nothing.
    let source = "
static mut MET_1: bool = false;
static mut MET_2: bool = false;
static mut OTHER: bool = false;

enum Pick { Yes, No }

#[scene]
#[entry]
pushdown fn Start() {
    match choice!{ id = \"c1\", t![\"Talk\"] => Pick::Yes, t![\"Leave\"] => Pick::No } {
        Pick::Yes => MET_1 = true,
        Pick::No => {}
    }
    match choice!{ id = \"c2\", t![\"Talk\"] => Pick::Yes, t![\"Leave\"] => Pick::No } {
        Pick::Yes => mark(),
        Pick::No => {}
    }
    Interlude();
    s!(t![\"Line 1.\"]);
    s!(t![\"Line 2.\"]);
    s!(t![\"Line 3.\"]);
    if MET_1 && MET_2 {
        s!(t![\"Good end.\"]);
    } else {
        s!(t![\"Normal end.\"]);
    }
}

pushdown fn mark() {
    MET_2 = true;
}

#[scene]
pushdown fn Interlude() {
    s!(t![\"Meanwhile.\"]);
    if OTHER {
        s!(t![\"Elsewhere.\"]);
    }
}

#[scene]
pushdown fn Unknown(next: scene fn() -> ()) {
    next();
}
";
    assert_eq!(
        shape_of(source, "Start"),
        (
            Some(vec![true, true, true]),
            vec![(None, vec![2, 3, 4], true), (Some(0), vec![2, 3], false)]
        )
    );
    // A scene holds what it and the scenes and procedures it calls access.
    assert_eq!(
        shape_of(source, "Interlude").0,
        Some(vec![false, false, true])
    );
    // A call through a pointer may access anything.
    assert_eq!(shape_of(source, "Unknown").0, None);
}

/// The story of `gen-probe.sh` of the research experiment E0002: `k` yes/no choices, each of
/// which may set one flag; then `l` lines that read no story variable; then an ending that
/// reads all `k` flags. Layout `scene`: the lines are a scene, called once; `inline`: they
/// are written in the entry scene.
fn flags(layout: &str, k: usize, l: usize) -> String {
    let mut out = String::new();
    for i in 1..=k {
        writeln!(out, "static mut MET_{i}: bool = false;").unwrap();
    }
    out.push_str("\nenum Pick { Yes, No }\n\n#[scene]\n#[entry]\npub pushdown fn Start() {\n");
    for i in 1..=k {
        writeln!(
            out,
            "    match choice!{{ id = \"c{i}\", t![\"Talk\"] => Pick::Yes, t![\"Leave\"] => Pick::No }} {{\n        \
             Pick::Yes => MET_{i} = true,\n        Pick::No => {{}}\n    }}"
        )
        .unwrap();
    }
    let lines = |out: &mut String| {
        for j in 1..=l {
            writeln!(out, "    s!(t![\"Line {j}.\"]);").unwrap();
        }
    };
    match layout {
        "scene" => out.push_str("    Interlude();\n"),
        "inline" => lines(&mut out),
        _ => panic!("unknown layout: {layout}"),
    }
    let all: Vec<String> = (1..=k).map(|i| format!("MET_{i}")).collect();
    writeln!(
        out,
        "    if {} {{\n        s!(t![\"Good end.\"]);\n    }} else {{\n        s!(t![\"Normal end.\"]);\n    }}\n}}",
        all.join(" && ")
    )
    .unwrap();
    if layout == "scene" {
        out.push_str("\n#[scene]\npushdown fn Interlude() {\n");
        lines(&mut out);
        out.push_str("}\n");
    }
    out
}

/// The story of `gen-counters.sh` of the research experiment E0003: `c` rounds, each one
/// choice with `m` arms (arm `j` adds 1 to counter `A_j`) and then `l` lines that read no
/// story variable; the ending compares the counters. Layout `scenes`: the lines of round `i`
/// are a scene `Lines_i`, called once; `inline`: all rounds are written in the entry scene.
fn counters(layout: &str, m: usize, c: usize, l: usize) -> String {
    assert!(
        matches!(layout, "inline" | "scenes"),
        "unknown layout: {layout}"
    );
    let mut out = String::new();
    for j in 1..=m {
        writeln!(out, "static mut A_{j}: Int<0, {c}> = 0;").unwrap();
    }
    out.push_str("\nenum Pick {");
    for j in 1..=m {
        write!(out, " P{j},").unwrap();
    }
    out.push_str(" }\n\n#[scene]\n#[entry]\npub pushdown fn Start() {\n");
    let lines = |out: &mut String, i: usize| {
        for line in 1..=l {
            writeln!(out, "    s!(t![\"Line {i}.{line}.\"]);").unwrap();
        }
    };
    for i in 1..=c {
        write!(out, "    match choice!{{ id = \"c{i}\",").unwrap();
        for j in 1..=m {
            write!(out, " t![\"Arm {j}\"] => Pick::P{j},").unwrap();
        }
        out.push_str(" } {\n");
        for j in 1..=m {
            writeln!(out, "        Pick::P{j} => A_{j} += 1,").unwrap();
        }
        out.push_str("    }\n");
        if layout == "scenes" {
            writeln!(out, "    Lines_{i}();").unwrap();
        } else {
            lines(&mut out, i);
        }
    }
    // The ending: the first counter that is at least as large as all later ones.
    for j in 1..m {
        let cond: Vec<String> = (j + 1..=m).map(|h| format!("A_{j} >= A_{h}")).collect();
        let head = if j == 1 { "    if" } else { "    } else if" };
        writeln!(
            out,
            "{head} {} {{\n        s!(t![\"Ending {j}.\"]);",
            cond.join(" && ")
        )
        .unwrap();
    }
    writeln!(
        out,
        "    }} else {{\n        s!(t![\"Ending {m}.\"]);\n    }}\n}}"
    )
    .unwrap();
    if layout == "scenes" {
        for i in 1..=c {
            writeln!(out, "\n#[scene]\npushdown fn Lines_{i}() {{").unwrap();
            lines(&mut out, i);
            out.push_str("}\n");
        }
    }
    out
}

/// What the story analysis finds with or without sharing.
struct Run {
    /// Code, anchor and message of every diagnostic but `W5004` (which names producers on
    /// the walked path; `inconsistent` has its steps), sorted. The witness of an `E5003`
    /// is the first stuck state found, so it is cut off.
    diagnostics: Vec<(&'static str, String, String)>,
    /// The scene and `push_step` call of every step whose screen depends on the path.
    inconsistent: Vec<(usize, u32)>,
    graph: kd::StoryGraph,
}

/// Runs the story analysis of `program`, which must pass stages 1–4; `None` when the story
/// does not terminate or has no entry scene.
fn explored(db: &dyn Db, program: Program, sharing: Sharing) -> Option<Run> {
    let sigs = program_signatures(db, program);
    let cx = StoryCx {
        db,
        program,
        std: std_table(db, program).as_ref().unwrap(),
        sigs,
    };
    let (scenes, vars) = collect(db, program, sigs);
    let mut diags = Vec::new();
    if !termination::check(db, program, &scenes, sigs, &mut diags) {
        return None;
    }
    let shapes = shape::shapes(cx, &scenes, &vars, sharing);
    let explored = explore::explore(cx, &scenes, &shapes, &vars, &mut diags)?;
    let inconsistent = screens::check(cx, &scenes, &explored, &mut diags);
    explored.report_reachability(db, program, &scenes, &mut diags);
    let items: Vec<ItemId> = scenes.list.iter().map(|scene| scene.item).collect();
    let graph = graph::convert(db, program, &items, &explored, &diags);
    let mut diagnostics: Vec<(&'static str, String, String)> = diags
        .iter()
        .filter(|diag| diag.code != DiagnosticCode::AnalysisInconsistentScreen)
        .map(|diag| {
            let message = diag.resolve(db).message;
            let message = match diag.code {
                DiagnosticCode::AnalysisStuckChoice => message
                    .split(" (for example when ")
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                _ => message,
            };
            (diag.code.as_str(), format!("{:?}", diag.anchor), message)
        })
        .collect();
    diagnostics.sort();
    Some(Run {
        diagnostics,
        inconsistent: inconsistent
            .into_iter()
            .map(|(scene, expr)| (scene, expr.into_raw().into_u32()))
            .collect(),
        graph,
    })
}

/// The states of the graph, as the state cap counts them.
fn nodes(graph: &kd::StoryGraph) -> usize {
    graph
        .contexts
        .iter()
        .map(|context| context.nodes.len())
        .sum()
}

/// Per choice step, every list of arm IDs some state of the graph shows.
fn arms(graph: &kd::StoryGraph) -> BTreeMap<String, BTreeSet<Vec<String>>> {
    let mut out: BTreeMap<String, BTreeSet<Vec<String>>> = BTreeMap::new();
    for (context, data) in graph.contexts.iter().enumerate() {
        for (node, data) in data.nodes.iter().enumerate() {
            if let kd::NodeKind::Choice { arms, .. } = &data.kind {
                let step = graph.step_id(context as u32, node as u32).unwrap();
                out.entry(step)
                    .or_default()
                    .insert(arms.iter().map(|(arm, _)| arm.clone()).collect());
            }
        }
    }
    out
}

/// What the player meets on a walk through a graph.
#[derive(Debug, PartialEq)]
enum Met {
    /// A line: its step ID.
    Say(String),
    /// A choice: its step ID and the arm IDs it shows.
    Choice(String, Vec<String>),
    /// A draw: the values it can give.
    Draw(Vec<kd::Value>),
    /// The graph has no state past this point.
    Dead,
    /// The entry scene returned.
    Returned,
}

/// A walk through a graph, as the debugger's tracker follows a game: the call nodes of the
/// callers, bottom first, then the node the walk is at.
struct Walker<'g> {
    graph: &'g kd::StoryGraph,
    stack: Vec<(u32, u32)>,
    target: kd::Target,
}

impl<'g> Walker<'g> {
    fn new(graph: &'g kd::StoryGraph, root: u32) -> Walker<'g> {
        Walker {
            graph,
            stack: vec![(root, u32::MAX)],
            target: graph.context(root).unwrap().entry,
        }
    }

    /// Walks to the next step, draw or ending: into the callee at a call, back to the
    /// caller at an exit.
    fn next(&mut self) -> Met {
        let graph = self.graph;
        for _ in 0..100_000 {
            match self.target {
                kd::Target::Dead => return Met::Dead,
                kd::Target::Exit(exit) => {
                    self.stack.pop();
                    let Some(&(caller, node)) = self.stack.last() else {
                        return Met::Returned;
                    };
                    let kd::NodeKind::Call { exits, .. } = &graph.node(caller, node).unwrap().kind
                    else {
                        panic!("an exit returns to a node that is not a call");
                    };
                    self.target = exits
                        .iter()
                        .find(|(id, _)| *id == exit)
                        .map_or(kd::Target::Dead, |(_, target)| *target);
                }
                kd::Target::Node(node) => {
                    let top = self.stack.last_mut().unwrap();
                    top.1 = node;
                    let context = top.0;
                    let step = || graph.step_id(context, node).unwrap();
                    match &graph.node(context, node).unwrap().kind {
                        kd::NodeKind::Say { .. } => return Met::Say(step()),
                        kd::NodeKind::Choice { arms, .. } => {
                            let arms = arms.iter().map(|(arm, _)| arm.clone()).collect();
                            return Met::Choice(step(), arms);
                        }
                        kd::NodeKind::Draw { draws } => {
                            return Met::Draw(
                                draws
                                    .iter()
                                    .map(|(value, _)| graph.value(*value).unwrap().clone())
                                    .collect(),
                            );
                        }
                        kd::NodeKind::Call { callee, .. } => {
                            self.target = graph.context(*callee).unwrap().entry;
                            self.stack.push((*callee, u32::MAX));
                        }
                    }
                }
            }
        }
        panic!("the graph loops without a step");
    }

    /// Takes outcome `index` of the step or draw the walk is at.
    fn take(&mut self, index: usize) {
        let &(context, node) = self.stack.last().unwrap();
        self.target = match &self.graph.node(context, node).unwrap().kind {
            kd::NodeKind::Say { next, .. } => *next,
            kd::NodeKind::Choice { arms, .. } => arms[index].1,
            kd::NodeKind::Draw { draws } => draws[index].1,
            kd::NodeKind::Call { .. } => panic!("a walk rests at a step or a draw"),
        };
    }
}

/// Walks both graphs 64 times from every root with the same pseudo-random inputs: they
/// must show the same steps, arms, draws and endings.
fn walk_in_lockstep(name: &str, off: &kd::StoryGraph, on: &kd::StoryGraph) {
    assert_eq!(off.roots.len(), on.roots.len(), "{name}: roots");
    for (root, (&left, &right)) in off.roots.iter().zip(&on.roots).enumerate() {
        for walk in 0..64_u64 {
            let mut seed = ((root as u64) << 32) | walk;
            let mut left = Walker::new(off, left);
            let mut right = Walker::new(on, right);
            for step in 0..400 {
                let met = left.next();
                assert_eq!(
                    met,
                    right.next(),
                    "{name}: root {root}, walk {walk}, step {step} (without sharing, with sharing)"
                );
                let outcomes = match &met {
                    Met::Say(_) => 1,
                    Met::Choice(_, arms) => arms.len(),
                    Met::Draw(values) => values.len(),
                    Met::Dead | Met::Returned => 0,
                };
                if outcomes == 0 {
                    break;
                }
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let pick = (seed >> 33) as usize % outcomes;
                left.take(pick);
                right.take(pick);
            }
        }
    }
}

/// Asserts that the analysis of `program` finds the same with and without sharing; returns
/// whether it explored the story.
fn same_with_and_without_sharing(name: &str, db: &dyn Db, program: Program) -> bool {
    let (Some(off), Some(on)) = (
        explored(db, program, Sharing::Off),
        explored(db, program, Sharing::On),
    ) else {
        return false;
    };
    assert_eq!(off.diagnostics, on.diagnostics, "{name}: diagnostics");
    assert_eq!(
        off.inconsistent, on.inconsistent,
        "{name}: steps whose screen depends on the path"
    );
    assert_eq!(off.graph.complete, on.graph.complete, "{name}: complete");
    assert_eq!(
        off.graph.reached_steps(),
        on.graph.reached_steps(),
        "{name}: reached steps"
    );
    assert_eq!(arms(&off.graph), arms(&on.graph), "{name}: arms");
    walk_in_lockstep(name, &off.graph, &on.graph);
    true
}

const RENDERED: &str = "
const MIO: EntityId = EntityId(30);

enum Place { Roof, Classroom }

const fn roof(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}

const fn classroom(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(0.5).apply_animation(tween(ms(300)));
    screen
}
";

/// The probe stories of the research experiment E0002 about the consistency check: a merge
/// of two screens in a called scene, before a step after the call, inlined, and separated
/// by a story variable that nothing reads afterwards; then the inlined merge without the
/// story variable, and with one more line after the merge.
fn consistency_stories() -> Vec<(&'static str, String)> {
    const WHERE: &str = "
#[scene]
#[entry]
pub pushdown fn Start() {
    match choice!{ id = \"where\", t![\"Roof\"] => Place::Roof, t![\"Classroom\"] => Place::Classroom } {
        Place::Roof => {
            SET_ROOF
            s!(render = roof, t![\"The wind is cold.\"]);
        }
        Place::Classroom => {
            SET_CLASSROOM
            s!(render = classroom, t![\"It's quiet.\"]);
        }
    }
    AFTER
}
";
    const SUB: &str = "
#[scene]
pushdown fn Sub() {
    X = 0;
    s!(t![\"Same story state, two screens.\"]);
}
";
    const X: &str = "static mut X: Int<0, 2> = 0;";
    const SAME: &str = "s!(t![\"Same story state, two screens.\"]);";
    let story = |statics: &str, roof: &str, classroom: &str, after: &str, rest: &str| {
        let start = WHERE
            .replace("SET_ROOF", roof)
            .replace("SET_CLASSROOM", classroom)
            .replace("AFTER", after);
        format!("{RENDERED}\n{statics}\n{start}{rest}")
    };
    vec![
        ("a-sub-scene", story(X, "X = 1;", "X = 2;", "Sub();", SUB)),
        (
            "b-sub-scene-then-step",
            story(
                X,
                "X = 1;",
                "X = 2;",
                "Sub();\n    s!(t![\"After the call.\"]);",
                SUB,
            ),
        ),
        (
            "c-inlined",
            story(X, "X = 1;", "X = 2;", &format!("X = 0;\n    {SAME}"), ""),
        ),
        (
            "d-dead-variable",
            story(
                "static mut PLACE: Place = Place::Roof;",
                "PLACE = Place::Roof;",
                "PLACE = Place::Classroom;",
                "s!(render = if PLACE == Place::Roof { roof } else { classroom }, t![\"Still here.\"]);\n    \
                 s!(t![\"Next line.\"]);",
                "",
            ),
        ),
        ("c-inlined without X", story("", "", "", SAME, "")),
        (
            "c-inlined without X, one more line",
            story(
                "",
                "",
                "",
                &format!("{SAME}\n    s!(t![\"One more line.\"]);"),
                "",
            ),
        ),
    ]
}

/// A hub: a loop around a choice whose arms call scenes that set flags and hide themselves.
const HUB: &str = "
static mut SEEN_A: bool = false;
static mut SEEN_B: bool = false;
enum Go { A, B, Leave }
#[scene]
#[entry]
pub pushdown fn Start() {
    loop {
        match choice!{
            id = \"hub\",
            t![\"A\"] if !SEEN_A => Go::A,
            t![\"B\"] if !SEEN_B => Go::B,
            t![\"Leave\"] => Go::Leave,
        } {
            Go::A => VisitA(),
            Go::B => VisitB(),
            Go::Leave => break,
        }
    }
    if SEEN_A && SEEN_B { s!(t![\"Both.\"]); } else { s!(t![\"Not both.\"]); }
}
#[scene]
pushdown fn VisitA() { SEEN_A = true; s!(t![\"A1\"]); s!(t![\"A2\"]); s!(t![\"A3\"]); }
#[scene]
pushdown fn VisitB() { SEEN_B = true; s!(t![\"B1\"]); s!(t![\"B2\"]); s!(t![\"B3\"]); }
";

/// Locals, an entry argument, a draw and a story variable, kept and read across regions.
const LOCALS: &str = "
static mut N: Int<0, 3> = 0;
enum Pick { One, Two }
enum Coin { Heads, Tails }
#[scene]
#[entry]
pub pushdown fn Start(flag: bool) {
    match choice!{ id = \"n\", t![\"+1\"] => Pick::One, t![\"+2\"] => Pick::Two } {
        Pick::One => N += 1,
        Pick::Two => N += 2,
    }
    s!(t![\"a\"]);
    let pick = choice!{ id = \"p\", t![\"one\"] => Pick::One, t![\"two\"] => Pick::Two };
    s!(t![\"b\"]);
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t![\"heads\"]),
        Coin::Tails => s!(t![\"tails\"]),
    }
    s!(t![\"c\"]);
    if pick == Pick::Two { s!(t![\"two\"]); }
    s!(t![\"N is \" {N}]);
    if flag { s!(t![\"flag\"]); }
    s!(t![\"end\"]);
}
";

/// Scene calls that borrow a local, right after lines that form a region: the caller goes
/// on at the call where the region left, and the callee still gets the local's value.
const BORROWS: &str = "
static mut TOTAL: Int<0, 9> = 0;
enum Pick { One, Two }
#[scene]
#[entry]
pub pushdown fn Start() {
    let mut score: Int<0, 9> = 0;
    match choice!{ id = \"p\", t![\"one\"] => Pick::One, t![\"two\"] => Pick::Two } {
        Pick::One => score = 1,
        Pick::Two => score = 2,
    }
    s!(t![\"a\"]);
    s!(t![\"b\"]);
    Raise(&mut score);
    s!(t![\"c\"]);
    s!(t![\"d\"]);
    Show(&score);
    TOTAL = score;
    s!(t![\"Total \" {TOTAL}]);
}
#[scene]
pushdown fn Raise(score: &mut Int<0, 9>) {
    s!(t![\"raise\"]);
    *score += 1;
}
#[scene]
pushdown fn Show(score: &Int<0, 9>) {
    if *score > 2 { s!(t![\"high\"]); } else { s!(t![\"low\"]); }
}
";

/// The analysis fixtures and the story fixtures of the bytecode tests.
fn fixtures() -> Vec<PathBuf> {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(tests.join("game_source/analysis"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    dirs.sort();
    dirs.push(tests.join("bytecode/005__story"));
    dirs.push(tests.join("bytecode/007__step_ids"));
    let fixtures: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|dir| [dir.join("main.kix"), dir.join("main.kis")])
        .filter(|path| path.is_file())
        .collect();
    assert!(fixtures.len() >= 11, "{fixtures:#?}");
    fixtures
}

#[test]
fn sharing_keeps_every_result() {
    let mut sources = consistency_stories();
    sources.extend([
        ("HUB", HUB.to_owned()),
        ("LOCALS", LOCALS.to_owned()),
        ("BORROWS", BORROWS.to_owned()),
        ("flags inline", flags("inline", 4, 6)),
        ("flags scene", flags("scene", 4, 6)),
        ("counters inline", counters("inline", 3, 4, 5)),
        ("counters scenes", counters("scenes", 3, 4, 5)),
    ]);
    for (name, source) in sources {
        let db = database(&source);
        let program = db.program(FilePath::disk(MAIN));
        assert!(
            analysis_runs(&db, program),
            "{name}: {:#?}",
            errors(&db, program)
        );
        assert!(
            same_with_and_without_sharing(name, &db, program),
            "{name}: the story was not explored"
        );
    }
    let mut explored = Vec::new();
    for path in fixtures() {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let program = db.program(FilePath::disk(&path));
        let name = path.display().to_string();
        if analysis_runs(&db, program) && same_with_and_without_sharing(&name, &db, program) {
            explored.push(name);
        }
    }
    assert!(explored.len() >= 6, "{explored:#?}");
}

#[test]
fn shared_lines_do_not_multiply_states() {
    let count = |source: &str| {
        let db = database(source);
        let program = db.program(FilePath::disk(MAIN));
        let states = |sharing| nodes(&explored(&db, program, sharing).unwrap().graph);
        (states(Sharing::Off), states(Sharing::On))
    };
    // Without sharing: every choice, line and ending once per valuation of the flags that
    // reaches it (the count the released explorer had). With sharing: 63 choices, 64 calls
    // of the region of the lines, 64 contexts of that region with 2 nodes each (keyed by the
    // flags the last line's segment reads), its 39 pure lines once, 64 endings.
    let (off, on) = count(&flags("inline", 6, 40));
    assert_eq!(off, 64 * 42 - 1);
    assert!(on <= 6 * 64 + 40, "{on}");
    // The scene of the lines reads no flag: it is explored once.
    let (off, on) = count(&flags("scene", 6, 40));
    assert_eq!(off, 64 * 43 - 1);
    assert!(on <= 4 * 64 + 48, "{on}");
    for layout in ["inline", "scenes"] {
        let (off, on) = count(&counters(layout, 3, 6, 20));
        assert!(
            on * 3 < off,
            "{layout}: {on} states with sharing, {off} without"
        );
    }
}

#[test]
fn stories_past_the_old_cap_are_checked_completely() {
    // 208 895 and 550 956 states without sharing; the default cap is 200 000.
    for source in [flags("inline", 11, 100), counters("inline", 3, 30, 100)] {
        let db = database(&source);
        let program = db.program(FilePath::disk(MAIN));
        let codes: Vec<DiagnosticCode> = check(&db, program).iter().map(|diag| diag.code).collect();
        assert!(
            !codes.contains(&DiagnosticCode::AnalysisTooManyStates)
                && !codes.contains(&DiagnosticCode::AnalysisIncomplete),
            "{codes:?}"
        );
    }
}
