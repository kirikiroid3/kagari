//! The query engine and the protocol server on a compiled fixture: nested scene calls, a
//! choice guarded by a story variable, a random draw.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod swap;

use std::path::PathBuf;
use std::sync::Arc;

use kagari_engine::{AssetStore, Engine, Frame, Platform, Timestamp};
use kagari_lang::{FilePath, MemoryFileSystem, RootDatabase, Severity, build, render_diagnostics};
use serde_json::{Value, json};

use crate::frames::Artifact;
use crate::protocol::{Message, NavBody, StackBody, command};
use crate::server::Server;
use crate::session::{Debugger, Run, Target};
use crate::transport::{Connection, Link};

const SYSTEM: &str = r#"
use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;
use std::ui::spacer;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    System {
        title: "Story",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
"#;

/// Steps: `Start` #0 Morning, #1 Shall we go, #2 choice `mood`, #3 At the gate, #4 Good night;
/// `Meet` #0; `Greet` #0; `AmusementPark` #0 The park, #1 choice `ride` (Coaster only with
/// `TRUST >= 2`), #2 Lucky (heads), #3 Oh well (tails), #4 Up high, #5 So fast.
const STORY: &str = r#"
static mut TRUST: Int<0, 9> = 0;

enum Pick { Bold, Shy }
enum Ride { Wheel, Coaster }
enum Coin { Heads, Tails }

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
    s!(render = key, who = t!["Mio"], t!["Morning."]);
    TRUST = 1;
    s!(t!["Shall we go?"]);
    match choice!{ id = "mood", render = key, t!["Be bold"] => Pick::Bold, t!["Be shy"] => Pick::Shy } {
        Pick::Bold => Meet(2),
        Pick::Shy => Meet(0),
    }
    s!(render = delta, t!["At the gate."]);
    AmusementPark();
    s!(t!["Good night."]);
}

#[scene]
pushdown fn Meet(gain: Int<0, 9>) {
    TRUST = gain;
    s!(t!["Hello there."]);
    Greet();
}

#[scene]
pushdown fn Greet() {
    s!(t!["Nice to meet you."]);
}

#[scene]
pushdown fn AmusementPark() {
    s!(t!["The park."]);
    let ride = choice!{
        id = "ride",
        t!["Wheel"] => Ride::Wheel,
        t!["Coaster"] if TRUST >= 2 => Ride::Coaster,
    };
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t!["Lucky!"]),
        Coin::Tails => s!(t!["Oh well."]),
    }
    match ride {
        Ride::Wheel => s!(t!["Up high."]),
        Ride::Coaster => s!(t!["So fast!"]),
    }
}

fn Title() -> RootPage {
    RootPage(spacer())
}
"#;

/// The bytecode and the artifact of the fixture.
fn compile() -> (Vec<u8>, Artifact) {
    compile_story(STORY)
}

/// The bytecode and the artifact of the system part with `story`.
fn compile_story(story: &str) -> (Vec<u8>, Artifact) {
    let mut files = rustc_hash::FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), format!("{SYSTEM}{story}"));
    files.insert(PathBuf::from("/game/font.ttf"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk("/game/main.kix"));
    let result = build(&db, program);
    if result
        .diagnostics
        .iter()
        .any(|diag| diag.severity == Severity::Error)
    {
        panic!("{}", render_diagnostics(&db, &result.diagnostics, None));
    }
    let graph = kagari_lang::analysis::story_graph(&db, program);
    assert!(
        graph.as_ref().is_some_and(|g| g.complete),
        "a complete graph"
    );
    (
        result.bytes.expect("bytecode"),
        Artifact {
            graph,
            debug: result.debug.expect("debug info"),
        },
    )
}

const TEST_FONT: &[u8] = include_bytes!("../../kagari-text/tests/fonts/kagari-test-cjk.otf");

struct TestPlatform;

impl Platform for TestPlatform {
    fn read_store(&mut self, _name: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn write_store(&mut self, _name: &str, _bytes: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
    fn delete_store(&mut self, _name: &str) -> std::io::Result<()> {
        Ok(())
    }
    fn read_asset(&mut self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(Some(if path.ends_with(".ttf") {
            TEST_FONT.to_vec()
        } else {
            Vec::new()
        }))
    }
    fn audio_duration_ms(&mut self, _path: &str, _bytes: &[u8]) -> Option<u32> {
        None
    }
    fn render_thumbnail(
        &mut self,
        _frame: &Frame,
        _assets: &AssetStore,
        _width: u32,
        _height: u32,
    ) -> Option<Vec<u8>> {
        None
    }
    fn local_time(&self) -> Timestamp {
        Timestamp {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
    fn random_seed(&mut self) -> u64 {
        1
    }
    fn set_title(&mut self, _title: &str) {}
}

struct Game {
    engine: Engine,
    platform: TestPlatform,
}

impl Game {
    fn boot() -> (Game, Artifact) {
        let (bytes, artifact) = compile();
        let mut platform = TestPlatform;
        let engine = Engine::boot(&bytes, &mut platform, 0).unwrap();
        (Game { engine, platform }, artifact)
    }

    fn t(&mut self) -> Target<'_> {
        Target {
            engine: &mut self.engine,
            platform: &mut self.platform,
            now_ms: 0,
        }
    }
}

/// A started query engine.
fn started(seed: u64) -> (Game, Debugger) {
    let (mut game, artifact) = Game::boot();
    let mut debugger = Debugger::new(&game.engine, Some(std::rc::Rc::new(artifact)));
    debugger.start(&mut game.t(), None, seed).unwrap();
    (game, debugger)
}

fn at(game: &mut Game, debugger: &Debugger) -> String {
    debugger.position(&game.t()).step.unwrap()
}

fn never() -> bool {
    false
}

#[test]
fn the_stack_names_frames_args_and_steps() {
    let (mut game, mut d) = started(1);
    assert_eq!(at(&mut game, &d), "crate::Start#0");
    d.push(&mut game.t(), &["Bold".to_owned()]).unwrap();
    assert_eq!(at(&mut game, &d), "crate::Meet#0");
    let stack = d.stack(&game.t()).unwrap();
    let frames: Vec<String> = stack
        .frames
        .iter()
        .map(|f| {
            let args: Vec<String> = f
                .args
                .iter()
                .map(|a| format!("{}={}", a.name, a.value))
                .collect();
            format!("{} {:?} ({})", f.function, f.step, args.join(", "))
        })
        .collect();
    assert_eq!(
        frames,
        [
            r#"crate::Meet Some("crate::Meet#0") (gain=2)"#,
            "crate::Start None ()",
        ]
    );
    // The caller's location is its call site, in the source file (relative to the root).
    let caller = &stack.frames[1];
    assert_eq!(caller.file.as_deref(), Some("main.kix"));
    assert!(caller.offset.is_some());
    // Story variables are a scope of every frame.
    let scopes = d.scopes(&game.t(), 0).unwrap();
    let story = scopes
        .scopes
        .iter()
        .find(|s| s.name == "Story variables")
        .unwrap();
    assert_eq!(story.vars[0].name, "crate::TRUST");
    assert_eq!(story.vars[0].value, "2");
}

#[test]
fn ls_lists_the_lines_up_to_the_next_choice() {
    let (mut game, mut d) = started(1);
    let ls = d.ls(&mut game.t(), None, None).unwrap();
    let steps: Vec<&str> = ls.lines.iter().map(|l| l.step.as_str()).collect();
    assert_eq!(
        steps,
        ["crate::Start#0", "crate::Start#1", "crate::Start#2"]
    );
    assert_eq!(ls.lines[0].who.as_deref(), Some("Mio"));
    assert_eq!(ls.lines[0].text, "Morning.");
    assert_eq!(ls.lines[2].kind, "choice");
    assert_eq!(ls.ends, "choice");
    let head = d.ls(&mut game.t(), Some(1), None).unwrap();
    assert_eq!(head.lines.len(), 1);
    assert_eq!(head.total, 3);
    let tail = d.ls(&mut game.t(), None, Some(2)).unwrap();
    assert_eq!(tail.lines[0].step, "crate::Start#1");
    // ls does not move.
    assert_eq!(at(&mut game, &d), "crate::Start#0");
}

#[test]
fn goto_moves_within_the_segment_only() {
    let (mut game, mut d) = started(1);
    let position = d.goto(&mut game.t(), "crate::Start#2").unwrap();
    assert_eq!(position.step.as_deref(), Some("crate::Start#2"));
    assert_eq!(position.steps, 2);
    assert!(game.engine.debug_held());
    d.goto(&mut game.t(), "crate::Start#0").unwrap();
    assert_eq!(at(&mut game, &d), "crate::Start#0");
    let error = d.goto(&mut game.t(), "crate::Start#3").unwrap_err();
    assert!(
        error.contains("the start of the story") && error.contains("`crate::Start#2`"),
        "{error}"
    );
    assert!(d.goto(&mut game.t(), "crate::Start#9").is_err());
}

#[test]
fn push_selects_arms_and_pop_goes_back() {
    let (mut game, mut d) = started(1);
    let pushed = d
        .push(
            &mut game.t(),
            &["crate::Pick::Shy".to_owned(), "#0".to_owned()],
        )
        .unwrap();
    assert_eq!(
        pushed.selected,
        [
            "crate::Start/mood/crate::Pick::Shy",
            "crate::AmusementPark/ride/crate::Ride::Wheel"
        ]
    );
    let after_ride = at(&mut game, &d);
    assert!(
        after_ride == "crate::AmusementPark#2" || after_ride == "crate::AmusementPark#3",
        "{after_ride}"
    );
    // Shy leaves TRUST at 0: the guarded arm is not offered.
    d.pop(&mut game.t()).unwrap();
    assert_eq!(at(&mut game, &d), "crate::AmusementPark#1");
    let error = d.push(&mut game.t(), &["Coaster".to_owned()]).unwrap_err();
    assert!(error.contains("does not offer `Coaster`"), "{error}");
    assert!(error.contains("`crate::Ride::Wheel`"), "{error}");
    assert_eq!(at(&mut game, &d), "crate::AmusementPark#1");
    let detail = d.detail(&mut game.t(), None).unwrap();
    let arms: Vec<(&str, bool)> = detail
        .arms
        .iter()
        .map(|a| (a.arm.as_str(), a.shown))
        .collect();
    assert_eq!(
        arms,
        [
            ("crate::Ride::Wheel", true),
            ("crate::Ride::Coaster", false)
        ]
    );
    d.pop(&mut game.t()).unwrap();
    assert_eq!(at(&mut game, &d), "crate::Start#2");
    assert!(d.pop(&mut game.t()).is_err());
}

#[test]
fn detail_shows_the_record_and_the_keyframe() {
    let (mut game, mut d) = started(1);
    let detail = d.detail(&mut game.t(), None).unwrap();
    assert_eq!(detail.kind, "say");
    assert_eq!(detail.who.as_deref(), Some("Mio"));
    assert_eq!(detail.text.as_deref(), Some("Morning."));
    assert_eq!(detail.render.as_deref(), Some("crate::key"));
    assert_eq!(detail.keyframe, Some(true));
    let plain = d.detail(&mut game.t(), Some("crate::Start#1")).unwrap();
    assert_eq!(plain.render, None);
    assert_eq!(plain.keyframe, Some(false));
    let choice = d.detail(&mut game.t(), Some("crate::Start#2")).unwrap();
    assert_eq!(choice.choice.as_deref(), Some("mood"));
    assert_eq!(choice.arms.len(), 2);
    // Only steps of the current segment.
    assert!(d.detail(&mut game.t(), Some("crate::Start#3")).is_err());
    d.push(&mut game.t(), &["Bold".to_owned()]).unwrap();
    d.run(&mut game.t(), Run::Next, &mut never).unwrap();
    let gate = d.detail(&mut game.t(), None).unwrap();
    assert_eq!(gate.render.as_deref(), Some("crate::delta"));
    assert_eq!(gate.keyframe, Some(false));
}

#[test]
fn nav_finds_a_path_that_push_follows() {
    let (mut game, mut d) = started(1);
    let nav = d
        .nav(&mut game.t(), "crate::AmusementPark#5", None)
        .unwrap();
    assert_eq!(nav.answer, "reachable", "{nav:?}");
    assert_eq!(nav.push, ["crate::Pick::Bold", "crate::Ride::Coaster"]);
    d.push(&mut game.t(), &nav.push).unwrap();
    let ls = d.ls(&mut game.t(), None, None).unwrap();
    assert!(
        ls.lines.iter().any(|l| l.step == "crate::AmusementPark#5"),
        "{ls:?}"
    );
    d.goto(&mut game.t(), "crate::AmusementPark#5").unwrap();
    assert_eq!(at(&mut game, &d), "crate::AmusementPark#5");
}

#[test]
fn nav_says_unreachable_with_a_complete_graph_and_unknown_without() {
    let (mut game, mut d) = started(1);
    d.push(&mut game.t(), &["Shy".to_owned()]).unwrap();
    let nav = d
        .nav(&mut game.t(), "crate::AmusementPark#5", None)
        .unwrap();
    assert_eq!(nav.answer, "unreachable", "{nav:?}");
    // Already passed: Start's first line cannot be reached again.
    let back = d.nav(&mut game.t(), "crate::Start#0", None).unwrap();
    assert_eq!(back.answer, "unreachable");

    let (mut game, artifact) = Game::boot();
    let mut artifact = artifact;
    let graph = artifact.graph.as_mut().unwrap();
    graph.complete = false;
    graph.incomplete.push(kagari_debug::Incomplete {
        code: "W5006".to_owned(),
        message: "the state cap was hit".to_owned(),
    });
    let mut d = Debugger::new(&game.engine, Some(std::rc::Rc::new(artifact)));
    d.start(&mut game.t(), None, 1).unwrap();
    d.push(&mut game.t(), &["Shy".to_owned()]).unwrap();
    let nav = d
        .nav(&mut game.t(), "crate::AmusementPark#5", None)
        .unwrap();
    assert_eq!(nav.answer, "unknown");
    assert!(nav.reason.unwrap().contains("W5006"));
}

#[test]
fn nav_reports_draws_and_finds_a_seed() {
    let (mut game, mut d) = started(1);
    d.push(&mut game.t(), &["Shy".to_owned()]).unwrap();
    // The segment after a choice spans scenes: Meet, Greet, Start, then the park's choice.
    d.goto(&mut game.t(), "crate::AmusementPark#1").unwrap();
    assert_eq!(at(&mut game, &d), "crate::AmusementPark#1");
    let nav: NavBody = d
        .nav(&mut game.t(), "crate::AmusementPark#2", None)
        .unwrap();
    assert_eq!(nav.answer, "reachable");
    assert_eq!(nav.push, ["crate::Ride::Wheel"]);
    let search = nav.seed.expect("a seed search");
    assert_eq!(search.draws, 1);
    assert_eq!(search.odds, 2);
    let seed = search.found.expect("a seed draws heads");
    d.seed(&mut game.t(), Some(seed)).unwrap();
    d.push(&mut game.t(), &nav.push).unwrap();
    assert_eq!(at(&mut game, &d), "crate::AmusementPark#2");
}

#[test]
fn step_level_execution() {
    let (mut game, mut d) = started(1);
    // next: Start's own lines.
    let body = d.run(&mut game.t(), Run::Next, &mut never).unwrap();
    assert_eq!(body.step.as_deref(), Some("crate::Start#1"));
    let body = d.run(&mut game.t(), Run::Next, &mut never).unwrap();
    assert_eq!(body.step.as_deref(), Some("crate::Start#2"));
    assert!(
        d.run(&mut game.t(), Run::Next, &mut never).is_err(),
        "a choice needs an arm"
    );
    d.push(&mut game.t(), &["Bold".to_owned()]).unwrap();
    // In Meet: stepIn enters Greet.
    let body = d.run(&mut game.t(), Run::StepIn, &mut never).unwrap();
    assert_eq!(body.step.as_deref(), Some("crate::Greet#0"));
    // stepOut: to the step after Greet and Meet return... Greet's caller Meet has no more
    // lines, so the next shallower step is Start's.
    let body = d.run(&mut game.t(), Run::StepOut, &mut never).unwrap();
    assert_eq!(body.step.as_deref(), Some("crate::Start#3"));
    // continue: to a breakpoint.
    d.set_step_breakpoints(&mut game.t(), &["crate::AmusementPark#0".to_owned()]);
    let body = d.run(&mut game.t(), Run::Continue, &mut never).unwrap();
    assert_eq!(body.reason, "breakpoint");
    assert_eq!(body.step.as_deref(), Some("crate::AmusementPark#0"));
    // continue: to the next choice.
    let body = d.run(&mut game.t(), Run::Continue, &mut never).unwrap();
    assert_eq!(body.reason, "choice");
    // next over a scene call: from Meet's line, Greet's line is deeper.
    let (mut game, mut d) = started(1);
    d.push(&mut game.t(), &["Bold".to_owned()]).unwrap();
    let body = d.run(&mut game.t(), Run::Next, &mut never).unwrap();
    assert_eq!(body.step.as_deref(), Some("crate::Start#3"));
}

#[test]
fn the_story_end_keeps_the_last_step() {
    let (mut game, mut d) = started(1);
    d.push(&mut game.t(), &["Bold".to_owned(), "Wheel".to_owned()])
        .unwrap();
    let body = d.run(&mut game.t(), Run::Continue, &mut never).unwrap();
    assert_eq!(body.reason, "end");
    assert_eq!(body.step.as_deref(), Some("crate::Start#4"));
}

// ----- The protocol ---------------------------------------------------------------------

/// A client endpoint and a server over an in-process link.
fn served(live: bool) -> (Game, Server, Connection) {
    let (mut game, artifact) = Game::boot();
    let (client, server) = Link::pair();
    let server = Server::attach(server, Some(artifact), live, &mut game.t());
    (game, server, Connection::new(client))
}

/// The messages that arrived so far.
fn drain(client: &Connection) -> Vec<Message> {
    std::iter::from_fn(|| client.try_recv()).collect()
}

fn response(messages: &[Message], seq: i64) -> (bool, Value) {
    messages
        .iter()
        .find_map(|m| match m {
            Message::Response {
                request_seq,
                success,
                body,
                message,
                ..
            } if *request_seq == seq => Some((
                *success,
                if *success {
                    body.clone()
                } else {
                    json!(message)
                },
            )),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no response to {seq}: {messages:?}"))
}

fn stopped_events(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::Event { event, body, .. } if event == "stopped" => Some(body.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_live_runtime_stops_at_breakpoints_and_moves_on_request() {
    let (mut game, mut server, client) = served(true);
    // The player starts the story (the title has no button: start it as `nav::start` would).
    server.debugger().start(&mut game.t(), None, 1).unwrap();
    game.engine.debug_release();
    let bp = client.request(
        command::SET_STEP_BREAKPOINTS,
        json!({ "steps": ["crate::Start#1", "nope#1"] }),
    );
    server.poll(&mut game.t());
    let messages = drain(&client);
    assert_eq!(
        response(&messages, bp).1,
        json!({ "verified": [true, false] })
    );

    // The player advances: the story holds at the breakpoint.
    for now in [10, 20, 30, 40] {
        game.engine.input(
            kagari_engine::InputEvent::Key {
                key: kagari_engine::Key::Space,
                pressed: true,
            },
            &mut game.platform,
            now,
        );
        game.engine.input(
            kagari_engine::InputEvent::Key {
                key: kagari_engine::Key::Space,
                pressed: false,
            },
            &mut game.platform,
            now,
        );
        game.engine.update(&mut game.platform, now);
    }
    server.poll(&mut game.t());
    let stops = stopped_events(&drain(&client));
    assert_eq!(stops.len(), 1, "{stops:?}");
    assert_eq!(stops[0]["reason"], "breakpoint");
    assert_eq!(stops[0]["step"], "crate::Start#1");

    // push and nav over the protocol.
    let nav = client.request(command::NAV, json!({ "step": "crate::AmusementPark#5" }));
    server.poll(&mut game.t());
    let messages = drain(&client);
    let (ok, body) = response(&messages, nav);
    assert!(ok, "{body}");
    assert_eq!(body["answer"], "reachable");
    let push = client.request(command::PUSH, json!({ "arms": body["push"] }));
    server.poll(&mut game.t());
    let messages = drain(&client);
    assert!(response(&messages, push).0);
    let stops = stopped_events(&messages);
    assert_eq!(stops[0]["reason"], "goto");
    assert!(game.engine.debug_held());

    // continue releases the story to the player.
    let cont = client.request(command::CONTINUE, Value::Null);
    server.poll(&mut game.t());
    let messages = drain(&client);
    assert!(response(&messages, cont).0);
    assert!(!game.engine.debug_held());

    let bye = client.request(command::DISCONNECT, Value::Null);
    server.poll(&mut game.t());
    assert!(response(&drain(&client), bye).0);
    assert!(!server.attached());
}

/// A query engine at the `mood` choice with a breakpoint on `TRUST = gain;`, Meet's first
/// instruction (code word 0).
fn at_choice_with_a_code_breakpoint() -> (Game, Server, Connection) {
    let (mut game, mut server, client) = served(false);
    server.debugger().start(&mut game.t(), None, 1).unwrap();
    server
        .debugger()
        .goto(&mut game.t(), "crate::Start#2")
        .unwrap();
    let points = client.request(
        command::SET_INSTRUCTION_BREAKPOINTS,
        json!({ "points": [{ "function": "crate::Meet", "word": 0 }] }),
    );
    server.poll(&mut game.t());
    assert_eq!(
        response(&drain(&client), points).1["verified"],
        json!([true])
    );
    (game, server, client)
}

#[test]
fn push_runs_into_an_instruction_breakpoint() {
    let (mut game, mut server, client) = at_choice_with_a_code_breakpoint();
    let push = client.request(command::PUSH, json!({ "arms": ["Bold"] }));
    // What the client says at the stop, queued before the story runs into it.
    let stack = client.request(command::STACK_TRACE, Value::Null);
    let scopes = client.request(command::SCOPES, json!({ "frame": 0 }));
    let refused = client.request(command::LS, Value::Null);
    let cont = client.request(command::CONTINUE, Value::Null);
    server.poll(&mut game.t());
    let messages = drain(&client);
    let stops = stopped_events(&messages);
    assert_eq!(stops.len(), 2, "{messages:?}");
    assert_eq!(stops[0]["inCode"], true);
    assert_eq!(stops[0]["reason"], "breakpoint");
    assert_eq!(stops[1]["inCode"], false);
    assert_eq!(stops[1]["step"], "crate::Meet#0");
    let (ok, body) = response(&messages, stack);
    assert!(ok, "{body}");
    let stack: StackBody = serde_json::from_value(body).unwrap();
    let functions: Vec<&str> = stack.frames.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(functions, ["crate::Meet", "crate::Start"]);
    let (ok, scopes) = response(&messages, scopes);
    assert!(ok, "{scopes}");
    assert_eq!(scopes["scopes"][0]["name"], "Arguments");
    assert_eq!(scopes["scopes"][0]["vars"][0]["name"], "gain");
    assert_eq!(scopes["scopes"][0]["vars"][0]["value"], "2");
    let (ok, error) = response(&messages, refused);
    assert!(!ok);
    assert!(error.as_str().unwrap().contains("inside code"), "{error}");
    assert!(response(&messages, cont).0);
    assert!(response(&messages, push).0);
    assert_eq!(at(&mut game, server.debugger()), "crate::Meet#0");
}

#[test]
fn replaying_moves_never_stop_in_code() {
    let (mut game, mut server, client) = at_choice_with_a_code_breakpoint();
    // Past Meet with the hook off, then replay the trace (Meet's code included) with `seed`.
    server.debugger().hook.borrow_mut().armed = false;
    server
        .debugger()
        .push(&mut game.t(), &["Bold".to_owned()])
        .unwrap();
    server.debugger().hook.borrow_mut().armed = true;
    let seed = client.request(command::SEED, json!({ "seed": 3 }));
    server.poll(&mut game.t());
    let messages = drain(&client);
    assert!(response(&messages, seed).0);
    let stops = stopped_events(&messages);
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0]["inCode"], false);
    assert_eq!(stops[0]["reason"], "goto");
}

/// The code word where `TRUST = 1;` starts in `Start`.
fn word_of_trust_one(game: &Game) -> u32 {
    let program = game.engine.program();
    let source = format!("{SYSTEM}{STORY}");
    let offset = u32::try_from(source.find("TRUST = 1;").unwrap()).unwrap();
    let start = program.find_function("crate::Start").unwrap();
    let debug = program.module().debug.as_ref().unwrap();
    let lines = &debug.functions[start.0 as usize].lines;
    lines
        .iter()
        .find(|(_, at)| *at >= offset)
        .map(|(word, _)| *word)
        .unwrap()
}

#[test]
fn a_live_code_step_that_reaches_a_step_ends_there() {
    let (mut game, mut server, client) = served(true);
    server.debugger().start(&mut game.t(), None, 1).unwrap();
    game.engine.debug_release();
    let word = word_of_trust_one(&game);
    let points = client.request(
        command::SET_INSTRUCTION_BREAKPOINTS,
        json!({ "points": [{ "function": "crate::Start", "word": word }] }),
    );
    server.poll(&mut game.t());
    assert_eq!(
        response(&drain(&client), points).1["verified"],
        json!([true])
    );
    // At the stop inside Start, step out: the scene suspends at its next step first.
    let out = client.request(command::STEP_OUT, Value::Null);
    // The player advances: complete the line's reveal, then go on.
    for now in [10, 20] {
        for pressed in [true, false] {
            game.engine.input(
                kagari_engine::InputEvent::Key {
                    key: kagari_engine::Key::Space,
                    pressed,
                },
                &mut game.platform,
                now,
            );
        }
        game.engine.update(&mut game.platform, now);
    }
    server.poll(&mut game.t());
    let messages = drain(&client);
    assert!(response(&messages, out).0);
    let stops = stopped_events(&messages);
    assert_eq!(stops.len(), 2, "{messages:?}");
    assert_eq!(stops[0]["inCode"], true);
    assert_eq!(stops[1]["inCode"], false);
    assert_eq!(stops[1]["reason"], "step");
    assert_eq!(stops[1]["step"], "crate::Start#1");
    assert!(game.engine.debug_held());
    assert!(!server.debugger().hook.borrow().story_step());
}
