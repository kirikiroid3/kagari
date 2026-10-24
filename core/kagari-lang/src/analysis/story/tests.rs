#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::db::{MemoryFileSystem, RootDatabase};
use crate::diagnostic::{DiagnosticCode, Severity};
use crate::input::{CompileOptions, FilePath};
use crate::pipeline::check;

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
