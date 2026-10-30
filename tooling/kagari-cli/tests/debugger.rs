#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `kagari debug`: the query engine's REPL on a headless game.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use common::{font_home, game, init_project, stderr, stdout};

/// Nested scene calls, a choice guarded by a story variable and a draw.
const STORY: &str = r#"
static mut TRUST: Int<0, 9> = 0;

enum Pick { Bold, Shy }
enum Ride { Wheel, Coaster }
enum Coin { Heads, Tails }

#[scene]
#[entry]
pub pushdown fn Start() {
    s!(who = "Mio", t!["Morning."]);
    s!(t!["Shall we go?"]);
    match choice!{ id = "mood", t!["Be bold"] => Pick::Bold, t!["Be shy"] => Pick::Shy } {
        Pick::Bold => Meet(2),
        Pick::Shy => Meet(0),
    }
    s!(t!["At the gate."]);
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

/// Runs `kagari debug` in `dir` with `script` on its standard input.
fn debug(dir: &Path, args: &[&str], script: &str) -> Output {
    let home = font_home();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kagari"))
        .arg("debug")
        .args(args)
        .current_dir(dir)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &home)
        .env("XDG_DATA_DIRS", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run kagari debug");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn story_project() -> tempfile::TempDir {
    let tmp = init_project();
    std::fs::write(game(&tmp).join("src/scenario/mod.kix"), STORY).unwrap();
    tmp
}

/// The 1-based line of the first line of `STORY` containing `needle`.
fn line_of(needle: &str) -> usize {
    STORY.lines().position(|l| l.contains(needle)).unwrap() + 1
}

/// An interactive `kagari debug`: commands are written as the test goes.
struct Repl {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Everything printed so far.
    out: String,
}

impl Repl {
    fn start(dir: &Path, args: &[&str]) -> Repl {
        let home = font_home();
        let mut child = Command::new(env!("CARGO_BIN_EXE_kagari"))
            .arg("debug")
            .args(args)
            .current_dir(dir)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &home)
            .env("XDG_DATA_DIRS", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("run kagari debug");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        Repl {
            child,
            stdin,
            lines,
            out: String::new(),
        }
    }

    fn send(&mut self, commands: &str) {
        self.stdin.write_all(commands.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Reads the output up to a line containing `needle`.
    fn until(&mut self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self
                .lines
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("no `{needle}` in\n{}", self.out));
            self.out.push_str(&line);
            self.out.push('\n');
            if line.contains(needle) {
                return;
            }
        }
    }

    /// Quits; the whole output.
    fn quit(mut self) -> String {
        self.send("quit\n");
        drop(self.stdin);
        for line in self.lines.iter() {
            self.out.push_str(&line);
            self.out.push('\n');
        }
        let status = self.child.wait().unwrap();
        assert!(status.success(), "{}", self.out);
        self.out
    }
}

#[test]
fn the_repl_moves_through_the_story_and_navigates() {
    let tmp = story_project();
    let dir = game(&tmp);
    let output = debug(
        &dir,
        &[],
        "ls -t 1\n\
         nav crate::scenario::AmusementPark#5\n\
         push crate::scenario::Pick::Bold Coaster\n\
         ls\n\
         goto crate::scenario::AmusementPark#5\n\
         stack\n\
         pop\n\
         detail\n\
         nav crate::scenario::Start#0\n\
         seed 7\n\
         quit\n",
    );
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}\n{}", stderr(&output));
    let expect = [
        // The debug build ran first.
        "wrote target/v0/game.kagari-cache",
        "stopped (entry) at crate::scenario::Start#0",
        "the story graph is complete",
        // ls -t 1: the next choice.
        "crate::scenario::Start#2  choice `mood`",
        // nav: the path, its draw and a seed.
        "reachable:",
        "push crate::scenario::Pick::Bold crate::scenario::Ride::Coaster",
        "draws: crate::scenario::AmusementPark draws",
        // push follows the path; ls shows the target ahead.
        "selected crate::scenario::Start/mood/crate::scenario::Pick::Bold",
        "selected crate::scenario::AmusementPark/ride/crate::scenario::Ride::Coaster",
        "crate::scenario::AmusementPark#5  So fast!",
        // goto, then the stack names the scenes with the source position.
        "#0 crate::scenario::AmusementPark() at crate::scenario::AmusementPark#5  scenario/mod.kix:",
        "#1 crate::scenario::Start() called  scenario/mod.kix:",
        // pop: back at the ride choice, where the guarded arm is shown.
        "at crate::scenario::AmusementPark#1",
        "shown  crate::scenario::Ride::Coaster",
        // Already passed.
        "unreachable",
        "seed 7",
    ];
    for line in expect {
        assert!(out.contains(line), "missing `{line}` in\n{out}");
    }
}

#[test]
fn breakpoints_by_line_and_by_step() {
    let tmp = story_project();
    let dir = game(&tmp);
    let line = STORY
        .lines()
        .position(|l| l.contains("Nice to meet you."))
        .unwrap()
        + 1;
    let output = debug(
        &dir,
        &[],
        &format!(
            "break scenario/mod.kix:{line} crate::scenario::Start#3\n\
             break scenario/mod.kix:1\n\
             push Shy\n\
             continue\n\
             continue\n\
             quit\n"
        ),
    );
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}\n{}", stderr(&output));
    let expect = [
        "breakpoint 0: scenario/mod.kix",
        "(crate::scenario::Greet#0)",
        "breakpoint 1: crate::scenario::Start#3 (a step)",
        "error: scenario/mod.kix:1: no code on this line",
        "stopped (breakpoint) at crate::scenario::Greet#0",
        "stopped (breakpoint) at crate::scenario::Start#3",
    ];
    for line in expect {
        assert!(out.contains(line), "missing `{line}` in\n{out}");
    }
}

#[test]
fn swap_rebuilds_and_moves_the_story_onto_the_new_sources() {
    let tmp = story_project();
    let dir = game(&tmp);
    let greet = line_of("Nice to meet you.");
    let gate = line_of("At the gate.");
    let mut repl = Repl::start(&dir, &[]);
    repl.send(&format!(
        "break scenario/mod.kix:{greet} scenario/mod.kix:{gate}\npush Shy\ncontinue\n"
    ));
    repl.until("stopped (breakpoint) at crate::scenario::Greet#0");

    // The scene the story is in changes: `swap` rebuilds, and its caller calls it again.
    std::fs::write(
        dir.join("src/scenario/mod.kix"),
        STORY.replace("Nice to meet you.", "Nice to see you."),
    )
    .unwrap();
    repl.send("swap\n");
    repl.until("swapped;");
    repl.send("detail\ncontinue\n");
    repl.until("stopped (breakpoint) at crate::scenario::Start#3");
    let out = repl.quit();
    let expect = [
        "swapped; called `crate::scenario::Greet` again",
        "now at crate::scenario::Greet#0",
        "stopped (swap) at crate::scenario::Greet#0",
        "text:     Nice to see you.",
    ];
    for line in expect {
        assert!(out.contains(line), "missing `{line}` in\n{out}");
    }
    // The build ran again for `swap`.
    assert_eq!(
        out.matches("wrote target/v0/game.kagari-cache").count(),
        2,
        "{out}"
    );
    assert!(!out.contains("rejected a breakpoint"), "{out}");
}
