#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `kagari dap` end to end: an editor's DAP session on the headless query engine.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use common::{font_home, game, init_project};
use kagari_debugger::protocol::{Message, write_frame};
use serde_json::{Value, json};

/// The story: nested scene calls, a choice guarded by a story variable, a random draw.
const STORY: &str = r#"//! The story.

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
pub pushdown fn Start() {
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
"#;

/// The 1-based line of the first line of `STORY` containing `needle`.
fn line_of(needle: &str) -> u64 {
    let index = STORY
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("`{needle}` is not in the story"));
    index as u64 + 1
}

const TIMEOUT: Duration = Duration::from_secs(300);

/// A DAP client of a `kagari dap` child process.
struct Client {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Message>,
    seq: i64,
    /// Every message received, in order.
    log: Vec<Message>,
    /// Messages received but not consumed yet.
    backlog: Vec<Message>,
}

impl Client {
    fn start(cwd: &Path) -> Client {
        let home = font_home();
        let mut child = Command::new(env!("CARGO_BIN_EXE_kagari"))
            .arg("dap")
            .current_dir(cwd)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &home)
            .env("XDG_DATA_DIRS", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("run kagari dap");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, incoming) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                // Strict framing: stdout holds nothing but DAP frames.
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 {
                    return;
                }
                let length: usize = header
                    .strip_prefix("Content-Length: ")
                    .and_then(|rest| rest.trim_end().parse().ok())
                    .unwrap_or_else(|| panic!("not a DAP header on stdout: {header:?}"));
                let mut blank = String::new();
                reader.read_line(&mut blank).unwrap();
                assert_eq!(blank, "\r\n", "a header ends with a blank line");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let message: Message = serde_json::from_slice(&body).unwrap();
                if tx.send(message).is_err() {
                    return;
                }
            }
        });
        Client {
            child,
            stdin,
            incoming,
            seq: 0,
            log: Vec::new(),
            backlog: Vec::new(),
        }
    }

    fn send(&mut self, command: &str, arguments: Value) -> i64 {
        self.seq += 1;
        let message = Message::Request {
            seq: self.seq,
            command: command.to_owned(),
            arguments,
        };
        write_frame(&mut self.stdin, &serde_json::to_vec(&message).unwrap()).unwrap();
        self.stdin.flush().unwrap();
        self.seq
    }

    /// The first message (backlog first) that `wanted` accepts; the others stay in the
    /// backlog.
    fn wait(&mut self, what: &str, wanted: impl Fn(&Message) -> bool) -> Message {
        if let Some(at) = self.backlog.iter().position(&wanted) {
            return self.backlog.remove(at);
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = self.incoming.recv_timeout(left).unwrap_or_else(|_| {
                panic!("no {what}; received:\n{:#?}", self.log);
            });
            self.log.push(message.clone());
            if wanted(&message) {
                return message;
            }
            self.backlog.push(message);
        }
    }

    /// The response to `seq`: its body, or a panic with its message.
    fn response(&mut self, seq: i64) -> Value {
        match self.response_result(seq) {
            Ok(body) => body,
            Err(message) => panic!("request {seq} failed: {message}"),
        }
    }

    fn response_result(&mut self, seq: i64) -> Result<Value, String> {
        let message = self.wait(
            &format!("response to {seq}"),
            |m| matches!(m, Message::Response { request_seq, .. } if *request_seq == seq),
        );
        let Message::Response {
            success,
            message,
            body,
            ..
        } = message
        else {
            unreachable!()
        };
        if success {
            Ok(body)
        } else {
            Err(message.unwrap_or_default())
        }
    }

    fn request(&mut self, command: &str, arguments: Value) -> Value {
        let seq = self.send(command, arguments);
        self.response(seq)
    }

    fn event(&mut self, name: &str) -> Value {
        let message = self.wait(
            &format!("`{name}` event"),
            |m| matches!(m, Message::Event { event, .. } if event == name),
        );
        let Message::Event { body, .. } = message else {
            unreachable!()
        };
        body
    }

    /// The position in [`Self::log`] of the first message `wanted` accepts.
    fn logged(&self, wanted: impl Fn(&Message) -> bool) -> usize {
        self.log.iter().position(wanted).unwrap()
    }
}

#[test]
fn an_editor_debugs_the_story_over_dap() {
    let tmp = init_project();
    let dir = game(&tmp);
    let source = dir.join("src/scenario/mod.kix");
    std::fs::write(&source, STORY).unwrap();
    let mut client = Client::start(&dir);

    let capabilities = client.request("initialize", json!({ "adapterID": "kagari" }));
    assert_eq!(capabilities["supportsConfigurationDoneRequest"], true);
    assert_eq!(capabilities["supportsTerminateRequest"], true);
    assert_eq!(capabilities["supportsEvaluateForHovers"], false);

    // Launch: the debug build's output goes to the console; then the launch is answered,
    // `initialized` follows, and the held story's entry stop.
    let launch = client.send(
        "launch",
        json!({ "mode": "headless", "manifestPath": "kagari.toml", "cwd": dir }),
    );
    client.response(launch);
    client.event("initialized");
    let stopped = client.event("stopped");
    assert_eq!(stopped["reason"], "entry", "{stopped}");
    assert_eq!(stopped["threadId"], 1);
    let answered = client
        .logged(|m| matches!(m, Message::Response { request_seq, .. } if *request_seq == launch));
    let initialized =
        client.logged(|m| matches!(m, Message::Event { event, .. } if event == "initialized"));
    let entry = client.logged(|m| matches!(m, Message::Event { event, .. } if event == "stopped"));
    assert!(answered < initialized && initialized < entry);
    let consoled = client.log.iter().any(|m| {
        matches!(m, Message::Event { event, body, .. }
            if event == "output" && body["output"].as_str().unwrap_or("").contains("game.kagari"))
    });
    assert!(
        consoled,
        "the build output is in the console: {:#?}",
        client.log
    );

    // A line with a step in a called scene breaks; a blank line says why it cannot.
    let greet = line_of(r#"s!(t!["Nice to meet you."])"#);
    let blank = line_of("static mut TRUST") + 1;
    let set = client.request(
        "setBreakpoints",
        json!({
            "source": { "path": source },
            "breakpoints": [{ "line": greet }, { "line": blank }],
        }),
    );
    let breakpoints = set["breakpoints"].as_array().unwrap();
    assert_eq!(breakpoints.len(), 2);
    assert_eq!(breakpoints[0]["verified"], true, "{set}");
    assert_eq!(breakpoints[0]["line"], greet);
    assert_eq!(breakpoints[1]["verified"], false, "{set}");
    assert!(breakpoints[1]["message"].as_str().is_some(), "{set}");
    client.request("configurationDone", Value::Null);

    let threads = client.request("threads", Value::Null);
    assert_eq!(threads["threads"][0]["id"], 1);

    // The entry scene's first `s!`.
    let trace = client.request("stackTrace", json!({ "threadId": 1 }));
    let top = &trace["stackFrames"][0];
    assert_eq!(top["line"], line_of(r#"t!["Morning."]"#), "{trace}");
    assert!(
        top["source"]["path"].as_str().unwrap().ends_with("mod.kix"),
        "{trace}"
    );
    assert!(top["name"].as_str().unwrap().contains("Start#0"), "{trace}");

    // Variables: the story variables are a scope of the frame.
    let scopes = client.request("scopes", json!({ "frameId": 0 }));
    let story = scopes["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Story variables")
        .unwrap_or_else(|| panic!("{scopes}"))
        .clone();
    let reference = story["variablesReference"].as_u64().unwrap();
    assert!(reference > 0);
    let vars = client.request("variables", json!({ "variablesReference": reference }));
    assert!(
        vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"].as_str().unwrap().ends_with("TRUST")),
        "{vars}"
    );

    // `kagari/nav` finds the way to the called scene; `kagari/push` follows it.
    let meet = "crate::scenario::Meet#0";
    let nav = client.request("kagari/nav", json!({ "step": meet }));
    assert_eq!(nav["answer"], "reachable", "{nav}");
    let arms = nav["push"].clone();
    assert!(!arms.as_array().unwrap().is_empty(), "{nav}");
    let pushed = client.request("kagari/push", json!({ "arms": arms }));
    assert_eq!(pushed["position"]["step"], meet, "{pushed}");
    let moved = client.event("stopped");
    assert_eq!(moved["reason"], "goto", "{moved}");

    // `evaluate` runs the query engine's command lines.
    let ls = client.request("evaluate", json!({ "expression": "ls", "context": "repl" }));
    assert!(
        ls["result"]
            .as_str()
            .unwrap()
            .contains("crate::scenario::Greet#0"),
        "{ls}"
    );
    let refused = client.send(
        "evaluate",
        json!({ "expression": "break crate::scenario::Greet#0", "context": "repl" }),
    );
    assert!(client.response_result(refused).is_err());

    // `continue` runs to the breakpoint in `Greet`.
    let continued = client.request("continue", json!({ "threadId": 1 }));
    assert_eq!(continued["allThreadsContinued"], true);
    let hit = client.event("stopped");
    assert_eq!(hit["reason"], "breakpoint", "{hit}");
    let trace = client.request("stackTrace", json!({ "threadId": 1 }));
    assert_eq!(trace["stackFrames"][0]["line"], greet, "{trace}");
    assert_eq!(trace["totalFrames"], 3, "Greet in Meet in Start: {trace}");

    finish(client);
}

/// Disconnects; the adapter exits successfully.
fn finish(mut client: Client) {
    client.request("disconnect", json!({}));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = client.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "kagari dap did not exit");
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
}

#[test]
fn a_line_without_a_step_breaks_in_code_and_next_leaves_the_line() {
    let tmp = init_project();
    let dir = game(&tmp);
    let source = dir.join("src/scenario/mod.kix");
    // Two statements on one line: two source positions inside code on the same line, which
    // one `next` passes.
    std::fs::write(
        &source,
        STORY.replace("TRUST = 1;", "TRUST = 0; TRUST = 1;"),
    )
    .unwrap();
    let mut client = Client::start(&dir);
    client.request("initialize", json!({ "adapterID": "kagari" }));
    let manifest = dir.join("kagari.toml");
    client.request(
        "launch",
        json!({ "mode": "headless", "manifestPath": manifest }),
    );
    client.event("initialized");
    assert_eq!(client.event("stopped")["reason"], "entry");

    let assignment = line_of("TRUST = 1;");
    let set = client.request(
        "setBreakpoints",
        json!({ "source": { "path": source }, "breakpoints": [{ "line": assignment }] }),
    );
    assert_eq!(set["breakpoints"][0]["verified"], true, "{set}");
    client.request("configurationDone", Value::Null);

    client.request("continue", json!({ "threadId": 1 }));
    let hit = client.event("stopped");
    assert_eq!(hit["reason"], "breakpoint", "{hit}");
    let trace = client.request("stackTrace", json!({ "threadId": 1 }));
    let top = &trace["stackFrames"][0];
    assert_eq!(top["line"], assignment, "{trace}");
    assert!(
        !top["name"].as_str().unwrap().contains('#'),
        "inside code, not at a step: {trace}"
    );

    // `next` steps by source line: the stop it shows is on another line.
    client.request("next", json!({ "threadId": 1 }));
    let stepped = client.event("stopped");
    assert_eq!(stepped["reason"], "step", "{stepped}");
    let trace = client.request("stackTrace", json!({ "threadId": 1 }));
    assert_ne!(trace["stackFrames"][0]["line"], assignment, "{trace}");

    finish(client);
}

#[test]
fn kagari_swap_rebuilds_and_sets_the_breakpoints_again() {
    let tmp = init_project();
    let dir = game(&tmp);
    let source = dir.join("src/scenario/mod.kix");
    std::fs::write(&source, STORY).unwrap();
    let mut client = Client::start(&dir);
    client.request("initialize", json!({ "adapterID": "kagari" }));
    client.request(
        "launch",
        json!({ "mode": "headless", "manifestPath": "kagari.toml", "cwd": dir }),
    );
    client.event("initialized");
    assert_eq!(client.event("stopped")["reason"], "entry");

    let greet = line_of(r#"s!(t!["Nice to meet you."])"#);
    let set = client.request(
        "setBreakpoints",
        json!({ "source": { "path": source }, "breakpoints": [{ "line": greet }] }),
    );
    let id = set["breakpoints"][0]["id"].clone();
    client.request("configurationDone", Value::Null);
    client.request("kagari/push", json!({ "arms": ["Shy"] }));
    assert_eq!(client.event("stopped")["reason"], "goto");
    client.request("continue", json!({ "threadId": 1 }));
    assert_eq!(client.event("stopped")["reason"], "breakpoint");

    // The scene the story is in changes: its caller calls it again.
    std::fs::write(
        &source,
        STORY.replace("Nice to meet you.", "Nice to see you."),
    )
    .unwrap();
    let swapped = client.request("kagari/swap", Value::Null);
    assert_eq!(swapped["outcome"], "recalled", "{swapped}");
    assert_eq!(swapped["scene"], "crate::scenario::Greet", "{swapped}");
    assert_eq!(
        swapped["position"]["step"], "crate::scenario::Greet#0",
        "{swapped}"
    );
    let stopped = client.event("stopped");
    assert_eq!(stopped["reason"], "swap", "{stopped}");
    assert!(
        stopped["description"]
            .as_str()
            .unwrap()
            .contains("called `crate::scenario::Greet` again"),
        "{stopped}"
    );
    // The line breakpoint, resolved again on the new sources.
    let changed = client.event("breakpoint");
    assert_eq!(changed["reason"], "changed", "{changed}");
    assert_eq!(changed["breakpoint"]["id"], id, "{changed}");
    assert_eq!(changed["breakpoint"]["verified"], true, "{changed}");
    client.wait("the summary in the console", |m| {
        matches!(m, Message::Event { event, body, .. }
            if event == "output" && body["output"].as_str().unwrap_or("").starts_with("swapped;"))
    });

    // The story shows the new text.
    let detail = client.request(
        "evaluate",
        json!({ "expression": "detail", "context": "repl" }),
    );
    assert!(
        detail["result"]
            .as_str()
            .unwrap()
            .contains("Nice to see you."),
        "{detail}"
    );
    let trace = client.request("stackTrace", json!({ "threadId": 1 }));
    assert_eq!(trace["stackFrames"][0]["line"], greet, "{trace}");

    finish(client);
}
