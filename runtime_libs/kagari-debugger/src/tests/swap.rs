//! Scene hot swap (`kagari/swap`) on compiled fixtures: a rebuilt program replaces the running
//! one, and the story moves over by re-calling the outermost changed scene.

use std::rc::Rc;

use kagari_engine::Engine;
use kagari_engine::debug::StepKind;

use super::{Game, TestPlatform, compile_story};
use crate::frames::Artifact;
use crate::protocol::SwapBody;
use crate::session::Debugger;

/// `Start` #0 Morning, #1 Good night; `Meet` #0 Hello there, #1 Bye; `Greet` #0 Nice to meet
/// you. `Greet` overwrites `TRUST` before its step.
const STORY: &str = r#"
static mut TRUST: Int<0, 9> = 0;

#[entry]
#[scene]
pushdown fn Start() {
    s!(t!["Morning."]);
    TRUST = 1;
    Meet(2);
    s!(t!["Good night."]);
}

#[scene]
pushdown fn Meet(gain: Int<0, 9>) {
    let mood = gain;
    TRUST = gain;
    s!(t!["Hello there."]);
    Greet();
    TRUST = mood;
    s!(t!["Bye."]);
}

#[scene]
pushdown fn Greet() {
    TRUST = 5;
    s!(t!["Nice to meet you."]);
}

fn Title() -> RootPage {
    RootPage(spacer())
}
"#;

/// A query engine recording call snapshots, at `step` of `STORY`.
fn at(step: &str) -> (Game, Debugger) {
    let (bytes, artifact) = compile_story(STORY);
    let mut platform = TestPlatform;
    let mut engine = Engine::boot(&bytes, &mut platform, 0).unwrap();
    engine.debug_record_calls(true);
    let mut game = Game { engine, platform };
    let mut d = Debugger::new(&game.engine, Some(Rc::new(artifact)));
    d.start(&mut game.t(), None, 1).unwrap();
    d.goto(&mut game.t(), step).unwrap();
    (game, d)
}

/// Swaps in `STORY` with `from` replaced by `to`.
fn swap(game: &mut Game, d: &mut Debugger, from: &str, to: &str) -> SwapBody {
    assert!(STORY.contains(from), "{from}");
    let (bytes, artifact) = compile_story(&STORY.replace(from, to));
    d.swap(&mut game.t(), &bytes, artifact).unwrap()
}

fn text(game: &Game) -> String {
    let view = game.engine.debug_view().unwrap();
    match view.step.unwrap().kind {
        StepKind::Say { text, .. } => format!("{:?}", text.unwrap()),
        StepKind::Choice { id, .. } => id.to_owned(),
    }
}

fn trust(game: &mut Game, d: &Debugger) -> String {
    let scopes = d.scopes(&game.t(), 0).unwrap();
    let story = scopes
        .scopes
        .iter()
        .find(|s| s.name == "Story variables")
        .unwrap();
    story.vars[0].value.clone()
}

#[test]
fn a_changed_scene_two_calls_deep_is_called_again() {
    let (mut game, mut d) = at("crate::Greet#0");
    let body = swap(&mut game, &mut d, "Nice to meet you.", "Nice to see you.");
    assert_eq!(body.outcome, "recalled");
    assert_eq!(body.scene.as_deref(), Some("crate::Greet"));
    assert_eq!(body.thrown, ["crate::Greet"]);
    // The scene runs from its entry with the new code, shown at once and held.
    assert_eq!(body.position.step.as_deref(), Some("crate::Greet#0"));
    assert!(text(&game).contains("Nice to see you."), "{}", text(&game));
    assert!(game.engine.debug_held());
    // The trace is the one at the call: two lines advanced.
    assert_eq!((body.position.choices, body.position.steps), (0, 2));
    // The callers keep their positions and locals.
    let stack = d.stack(&game.t()).unwrap();
    let names: Vec<&str> = stack.frames.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(names, ["crate::Greet", "crate::Meet", "crate::Start"]);
    let meet = d.scopes(&game.t(), 1).unwrap();
    let locals: Vec<(String, String)> = meet
        .scopes
        .iter()
        .flat_map(|s| s.vars.iter().map(|v| (v.name.clone(), v.value.clone())))
        .collect();
    assert!(
        locals.contains(&("mood".to_owned(), "2".to_owned())),
        "{locals:?}"
    );
    // Going on returns into the surviving caller.
    d.goto(&mut game.t(), "crate::Meet#1").unwrap();
    assert_eq!(trust(&mut game, &d), "2");
}

#[test]
fn story_variables_are_back_at_their_values_at_the_call() {
    let (mut game, mut d) = at("crate::Greet#0");
    assert_eq!(trust(&mut game, &d), "5");
    let body = swap(
        &mut game,
        &mut d,
        "    TRUST = 5;\n    s!(t![\"Nice to meet you.\"]);",
        "    s!(t![\"Hi!\"]);\n    TRUST = 5;\n    s!(t![\"Nice to meet you.\"]);",
    );
    assert_eq!(body.outcome, "recalled");
    assert!(text(&game).contains("Hi!"), "{}", text(&game));
    // Greet's write is undone: TRUST is what Meet set before the call.
    assert_eq!(trust(&mut game, &d), "2");
}

#[test]
fn a_save_after_a_swap_replays_on_the_new_program() {
    let (mut game, mut d) = at("crate::Greet#0");
    swap(
        &mut game,
        &mut d,
        "    TRUST = 5;\n    s!(t![\"Nice to meet you.\"]);",
        "    s!(t![\"Hi!\"]);\n    TRUST = 5;\n    s!(t![\"Nice to meet you.\"]);",
    );
    d.goto(&mut game.t(), "crate::Greet#1").unwrap();
    let before = d.position(&game.t());
    // `seed` with the same seed replays the trace from the entry, as a load does.
    d.seed(&mut game.t(), Some(1)).unwrap();
    assert_eq!(d.position(&game.t()), before);
    assert_eq!(before.step.as_deref(), Some("crate::Greet#1"));
    assert_eq!(trust(&mut game, &d), "5");
}

#[test]
fn a_change_off_the_stack_keeps_the_step() {
    let (mut game, mut d) = at("crate::Meet#0");
    let body = swap(&mut game, &mut d, "Nice to meet you.", "Nice to see you.");
    assert_eq!(body.outcome, "kept");
    assert!(body.thrown.is_empty());
    assert_eq!(body.position.step.as_deref(), Some("crate::Meet#0"));
    assert_eq!(trust(&mut game, &d), "2");
    // The callee changed: going on runs its new code.
    d.goto(&mut game.t(), "crate::Greet#0").unwrap();
    assert!(text(&game).contains("Nice to see you."), "{}", text(&game));
}

#[test]
fn a_changed_entry_scene_restarts_the_story() {
    let (mut game, mut d) = at("crate::Greet#0");
    let body = swap(&mut game, &mut d, "Morning.", "Good morning.");
    assert_eq!(body.outcome, "restarted");
    assert!(
        body.reason
            .as_deref()
            .is_some_and(|r| r.contains("entry scene `crate::Start`")),
        "{:?}",
        body.reason
    );
    assert_eq!(body.thrown, ["crate::Start", "crate::Meet", "crate::Greet"]);
    assert_eq!(body.position.step.as_deref(), Some("crate::Start#0"));
    assert_eq!(body.position.seed, 1);
    assert!(text(&game).contains("Good morning."), "{}", text(&game));
}

#[test]
fn a_new_story_variable_restarts_the_story() {
    let (mut game, mut d) = at("crate::Greet#0");
    let body = swap(
        &mut game,
        &mut d,
        "static mut TRUST: Int<0, 9> = 0;",
        "static mut TRUST: Int<0, 9> = 0;\nstatic mut LUCK: Int<0, 9> = 0;",
    );
    assert_eq!(body.outcome, "restarted");
    assert_eq!(
        body.reason.as_deref(),
        Some("the story variable `crate::LUCK` was added")
    );
    assert_eq!(body.position.step.as_deref(), Some("crate::Start#0"));
}

#[test]
fn a_retyped_story_variable_restarts_the_story() {
    let (mut game, mut d) = at("crate::Meet#0");
    let body = swap(
        &mut game,
        &mut d,
        "static mut TRUST: Int<0, 9> = 0;",
        "static mut TRUST: Int<0, 99> = 0;",
    );
    assert_eq!(body.outcome, "restarted");
    assert!(
        body.reason
            .as_deref()
            .is_some_and(|r| r.contains("`crate::TRUST` changed type")),
        "{:?}",
        body.reason
    );
}

#[test]
fn a_swap_needs_the_running_programs_artifact() {
    let (bytes, _) = compile_story(STORY);
    let mut platform = TestPlatform;
    let engine = Engine::boot(&bytes, &mut platform, 0).unwrap();
    let mut game = Game { engine, platform };
    let mut d = Debugger::new(&game.engine, None);
    let (bytes, artifact): (Vec<u8>, Artifact) = compile_story(STORY);
    let error = d.swap(&mut game.t(), &bytes, artifact).unwrap_err();
    assert!(error.contains("--profile debug"), "{error}");
}

/// `Intro` shows `old.png` and returns; `Start` then calls `Greet`.
const PICTURE_STORY: &str = r#"
const BG: ImageAsset = asset("old.png");
const A: EntityId = EntityId(1);

const fn show(mut screen: Screen) -> Screen {
    screen.entity(A).image().change(BG).apply_animation(cut());
    screen
}

#[entry]
#[scene]
pushdown fn Start() {
    Intro();
    Greet();
    s!(t!["Good night."]);
}

#[scene]
pushdown fn Intro() {
    s!(render = show, t!["A picture."]);
}

#[scene]
pushdown fn Greet() {
    s!(t!["Nice to meet you."]);
}

fn Title() -> RootPage {
    RootPage(spacer())
}
"#;

/// The screen at `Greet`'s call shows `old.png`; the rebuild no longer has that asset, so the
/// screen cannot move and the story restarts from its entry, naming the asset.
#[test]
fn a_loaded_asset_that_is_gone_restarts_the_story() {
    let (bytes, artifact) = compile_story(PICTURE_STORY);
    let mut platform = TestPlatform;
    let mut engine = Engine::boot(&bytes, &mut platform, 0).unwrap();
    engine.debug_record_calls(true);
    let mut game = Game { engine, platform };
    let mut d = Debugger::new(&game.engine, Some(Rc::new(artifact)));
    d.start(&mut game.t(), None, 1).unwrap();
    d.goto(&mut game.t(), "crate::Greet#0").unwrap();
    let edited = PICTURE_STORY
        .replace("asset(\"old.png\")", "asset(\"new.png\")")
        .replace("Nice to meet you.", "Nice to see you.");
    let (bytes, artifact) = compile_story(&edited);
    let body = d.swap(&mut game.t(), &bytes, artifact).unwrap();
    assert_eq!(body.outcome, "restarted", "{body:?}");
    assert_eq!(
        body.reason.as_deref(),
        Some("the asset `old.png` is gone"),
        "{body:?}"
    );
    assert_eq!(body.thrown, ["crate::Greet"]);
    assert_eq!(body.position.step.as_deref(), Some("crate::Intro#0"));
    assert!(game.engine.crash().is_none());
    // The restarted story shows the new picture and plays on.
    d.goto(&mut game.t(), "crate::Greet#0").unwrap();
    assert!(text(&game).contains("Nice to see you."), "{}", text(&game));
}
